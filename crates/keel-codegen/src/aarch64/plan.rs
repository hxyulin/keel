//! The phases of [`FunctionCompiler::new_for_target`], in the order it
//! runs them: preflight checks, the call area at the bottom of the frame,
//! inline assembly plans, the allocation pools, selection and its covers,
//! allocation, and the spill area.
use super::*;
use crate::cover::Cover;

/// Rejects signatures the target cannot call or return through.
pub(super) fn preflight(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    linux: bool,
) -> Result<(), String> {
    if !linux
        && func
            .layout_insts()
            .any(|(_, i)| matches!(func.inst_data(i), InstData::VaArg { ty: Type::I128, .. }))
    {
        return Err("va_arg.i128 requires AArch64 Linux".into());
    }
    if linux && func.personality.is_some() {
        return Err("exception handling on AArch64 Linux is not supported yet".into());
    }
    // Every Apple arm64 CPU has FEAT_SHA256. Linux AArch64 does not assume it.
    if linux
        && let Some(id) = func
            .layout_insts()
            .find_map(|(_, i)| match func.inst_data(i) {
                InstData::Intrinsic { id, .. } if id.is_aarch64_target() => Some(*id),
                _ => None,
            })
    {
        return Err(format!(
            "{id} requires FEAT_SHA256, which AArch64 Linux targets do not assume"
        ));
    }
    let (CallConv::C | CallConv::Fast) = func.sig.cc;
    match func.sig.returns.as_slice() {
        [] => {}
        // The C ABI has the callee extend these to 32 bits. Private
        // cc(fast) edges only require the declared narrow value.
        [
            AbiParam {
                ty: Type::I8 | Type::I16,
                ext: ArgExt::None,
                ..
            },
        ] if func.sig.cc == CallConv::C && !linux => {
            return Err(
                "an i8/i16 return value needs `sext` or `uext`: C callers expect 32 bits".into(),
            );
        }
        [_] => {}
        _ => {
            abi::multi_result_locs(&func.sig)?;
        }
    }
    for (_, callee) in func.callees() {
        let what = || format!("call to @{}", callee.name);
        check_call_signature(
            what,
            &callee.sig,
            !linux && symbols.is_external(&callee.name),
        )?;
    }
    // The target of an indirect call may be an external function.
    for (_, sig) in func.signatures() {
        check_call_signature(|| format!("indirect call {sig}"), sig, !linux)?;
    }
    Ok(())
}

/// Rejects instructions the backend cannot lower: x86 intrinsics and
/// FLAGS regions, and high multiplies of i128.
pub(super) fn check_instructions(
    func: &Function,
    present: crate::kinds::InstKinds,
) -> Result<(), String> {
    for (_, inst) in func.layout_insts().filter(|_| present.intrinsic) {
        if matches!(func.inst_data(inst), InstData::X86RflagsRegion { .. })
            || matches!(func.inst_data(inst), InstData::Intrinsic { id,.. } if id.is_x86_target())
        {
            return Err(
                "x86 architectural intrinsic (including F16C) is not supported on AArch64".into(),
            );
        }
    }

    for (_, inst) in func.layout_insts().filter(|_| present.high_multiply) {
        if let InstData::Binary {
            op: op @ (BinaryOp::Umulhi | BinaryOp::Smulhi),
            args,
        } = func.inst_data(inst)
            && func.value_type(func.resolve(args[0])) == Type::I128
        {
            return Err(format!("{op} on i128 is not supported yet"));
        }
        if let InstData::Overflow {
            op: op @ (OverflowOp::Umul | OverflowOp::Smul),
            args,
        } = func.inst_data(inst)
            && func.value_type(func.resolve(args[0])) == Type::I128
        {
            return Err(format!("{op} on i128 is not supported yet"));
        }
    }
    Ok(())
}

/// A tail call's arguments must all go in registers: the caller's
/// frame, where stack arguments would go, is gone by the jump.
pub(super) fn check_tail_calls(
    func: &Function,
    present: crate::kinds::InstKinds,
    linux: bool,
) -> Result<(), String> {
    for (_, inst) in func.layout_insts().filter(|_| present.return_call) {
        let (args, sig) = match func.inst_data(inst) {
            InstData::ReturnCall { func: callee, args } => (args, &func.callee(*callee).sig),
            InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
            _ => continue,
        };
        let kinds = arg_kinds(func, &sig.params, args);
        let (locs, stack) = abi::arg_locs_for(&kinds, sig.params.len(), linux);
        let in_regs = locs
            .iter()
            .all(|loc| matches!(loc, ArgLoc::Value(Slot::Reg(_))));
        if stack > 0 || !in_regs {
            return Err(format!(
                "`{}` needs stack or struct arguments, which tail calls do not support",
                func.display_inst(inst)
            ));
        }
        // An sret result may have to be loaded into registers on return,
        // which the callee would not do.
        if func
            .sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, Purpose::Sret(_)))
        {
            return Err("a function returning a struct cannot make tail calls".into());
        }
    }
    Ok(())
}

/// The bottom of the frame: what calls need (their stack arguments,
/// copies of big structs passed by value, and a slot for an `sret`
/// address), then this function's parameter memory and Linux variadic
/// save area.
pub(super) struct CallArea {
    pub call_layouts: HashMap<Inst, abi::CallLayout>,
    pub copy_base: usize,
    pub sret_save: Option<usize>,
    pub param_areas: Vec<Option<usize>>,
    pub variadic_area: Option<usize>,
    pub spill_base: usize,
}

impl CallArea {
    pub fn plan(
        func: &Function,
        present: crate::kinds::InstKinds,
        param_locs: &[ArgLoc],
        linux: bool,
    ) -> Result<Self, String> {
        let (mut outgoing, mut copies, mut needs_sret_save) = (0, 0u64, false);
        let mut call_layouts = HashMap::default();
        for (_, inst) in func.layout_insts().filter(|_| present.call) {
            let (args, params) = match func.inst_data(inst) {
                InstData::Call { func: callee, args }
                | InstData::Invoke {
                    func: callee, args, ..
                } => (args, &func.callee(*callee).sig.params),
                InstData::CallIndirect { sig, args, .. } => (args, &func.signature(*sig).params),
                _ => continue,
            };
            if !linux
                && args
                    .iter()
                    .skip(params.len())
                    .any(|&v| func.value_type(func.resolve(v)) == Type::I128)
            {
                return Err("variadic i128 arguments are unsupported on AArch64 Darwin".into());
            }
            // Up to four named scalars take at most eight registers of a
            // class: they need neither stack nor copies.
            if args.len() == params.len()
                && args.len() <= 4
                && params.iter().all(|p| p.purpose == Purpose::Normal)
            {
                continue;
            }
            let kinds = arg_kinds(func, params, args);
            let layout = abi::CallLayout::new(&kinds, params.len(), linux);
            outgoing = outgoing.max(layout.stack_bytes);
            copies = copies.max(layout.copy_bytes);
            needs_sret_save |= layout
                .locations
                .iter()
                .any(|loc| matches!(loc, ArgLoc::SretRegs(_)));
            call_layouts.insert(inst, layout);
        }
        let copy_base = (outgoing as usize).next_multiple_of(16);
        let mut offset = copy_base + copies as usize;
        let sret_save = needs_sret_save.then(|| {
            offset = offset.next_multiple_of(8) + 8;
            offset - 8
        });
        let mut param_areas = Vec::new();
        for (loc, param) in param_locs.iter().zip(&func.sig.params) {
            param_areas.push(match loc {
                ArgLoc::InRegs(_) | ArgLoc::SretRegs(_) => {
                    let layout = param.layout().expect("a struct");
                    offset = offset.next_multiple_of(layout.align().max(8) as usize);
                    let area = offset;
                    offset += layout.size().next_multiple_of(8) as usize;
                    Some(area)
                }
                _ => None,
            });
        }
        let variadic_area = if linux && func.sig.variadic {
            offset = offset.next_multiple_of(16);
            let area = offset;
            offset += 192;
            Some(area)
        } else {
            None
        };
        let spill_base = offset.next_multiple_of(8);
        Ok(Self {
            call_layouts,
            copy_base,
            sret_save,
            param_areas,
            variadic_area,
            spill_base,
        })
    }
}

/// Inline assembly plans, and the callee-saved registers the assembly
/// clobbers: saved by the prologue, and never allocated, so no value
/// lives in one across the assembly.
pub(super) struct InlineAsmPlans {
    pub asm_saved: Vec<u8>,
    pub asm_float_saved: Vec<u8>,
    pub has_inline_asm: bool,
    pub asm_plans: HashMap<Inst, AsmPlan>,
}

impl InlineAsmPlans {
    pub fn plan(
        func: &Function,
        present: crate::kinds::InstKinds,
        moves_sp: bool,
    ) -> Result<Self, String> {
        let (mut asm_saved, mut asm_float_saved) = (Vec::new(), Vec::new());
        let mut has_inline_asm = false;
        let mut asm_plans = HashMap::default();
        for (_, inst) in func.layout_insts().filter(|_| present.inline_asm) {
            let InstData::InlineAsm { asm, args } = func.inst_data(inst) else {
                continue;
            };
            has_inline_asm = true;
            let at = |message: String| format!("`{}`: {message}", func.display_inst(inst));
            for (&constraint, &arg) in asm.inputs.iter().zip(args) {
                if let AsmOperand::Tied(out) = constraint
                    && func.value_type(func.resolve(arg)) != asm.outputs[out as usize].1
                {
                    return Err(at("different-width asm ties require the x86 backend".into()));
                }
            }
            asm_plans.insert(inst, AsmPlan::new(func, asm, args).map_err(at)?);
            for clobber in &asm.clobbers {
                let AsmClobber::Reg(r) = *clobber else {
                    continue;
                };
                match (r.float, r.num) {
                    (false, 18) => return Err(at("x18 is reserved by the platform".into())),
                    (false, 29) => return Err(at("x29 is the frame pointer".into())),
                    (false, BASE) if moves_sp => {
                        return Err(at(
                            "x19 is the base pointer of a function that allocates stack".into(),
                        ));
                    }
                    (false, n) if REGISTERS.callee_saved.contains(&n) => asm_saved.push(n),
                    (true, n) if REGISTERS.float_callee_saved.contains(&n) => {
                        asm_float_saved.push(n)
                    }
                    _ => {}
                }
            }
        }
        Ok(Self {
            asm_saved,
            asm_float_saved,
            has_inline_asm,
            asm_plans,
        })
    }
}

/// Whether the function reads a thread-local through the TLS getter.
pub(super) fn has_tls_getter(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    present: crate::kinds::InstKinds,
) -> bool {
    present.symbol_addr
        && symbols.has_thread_locals()
        && func.layout_insts().any(|(_, i)| match func.inst_data(i) {
            InstData::SymbolAddr { symbol } => symbols
                .data_object(func.symbol_name(*symbol))
                .is_some_and(|data| data.thread_local),
            _ => false,
        })
}

/// Whether a scalar integer private leaf may allocate x0-x7. It never
/// runs the wide-ABI staging or call helpers that reserve them, so it can
/// compute in its argument/result bank directly. Custom pools, including
/// forced-spill tests, keep their registers. `incoming` is the bytes of
/// stack parameters.
pub(super) fn argument_bank(
    func: &Function,
    module: &Module,
    pools: &Pools,
    incoming: u32,
    has_tls_getter: bool,
    linux: bool,
) -> bool {
    !linux
        && func.sig.cc == CallConv::Fast
        && !has_tls_getter
        && module.files.is_empty()
        && func.personality.is_none()
        && incoming == 0
        && func.num_stack_slots() == 0
        && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        && pools.caller_saved == REGISTERS.caller_saved
        && pools.callee_saved == REGISTERS.callee_saved
        && pools.float_caller_saved == REGISTERS.float_caller_saved
        && pools.float_callee_saved == REGISTERS.float_callee_saved
        && (0..func.num_values()).all(|i| {
            matches!(
                func.value_type(Value::new(i)),
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
            )
        })
        && !func.layout_insts().any(|(_, i)| {
            let d = func.inst_data(i);
            is_call(func, d)
                || matches!(
                    d,
                    InstData::Intrinsic { .. }
                        | InstData::InlineAsm { .. }
                        | InstData::ReturnCall { .. }
                        | InstData::ReturnCallIndirect { .. }
                        | InstData::LandingPad { .. }
                )
        })
}

/// The registers the allocator may use: the target's pools, widened by
/// the argument bank and narrowed by the base register and inline
/// assembly clobbers.
pub(super) struct AllocationPools {
    private_callers: Vec<u8>,
    callee_saved: Vec<u8>,
    float_callee_saved: Vec<u8>,
}

impl AllocationPools {
    pub fn plan(pools: &Pools, argument_bank: bool, asm: &InlineAsmPlans, moves_sp: bool) -> Self {
        let mut private_callers = Vec::new();
        if argument_bank {
            private_callers.extend(0..8);
        }
        private_callers.extend_from_slice(pools.caller_saved);
        let callee_saved: Vec<u8> = (pools.callee_saved.iter().copied())
            .filter(|&r| !(moves_sp && r == BASE) && !asm.asm_saved.contains(&r))
            .collect();
        let float_callee_saved: Vec<u8> = (pools.float_callee_saved.iter().copied())
            .filter(|r| !asm.asm_float_saved.contains(r))
            .collect();
        Self {
            private_callers,
            callee_saved,
            float_callee_saved,
        }
    }

    pub fn pools<'p>(&'p self, pools: &Pools<'p>) -> Pools<'p> {
        Pools {
            caller_saved: &self.private_callers,
            callee_saved: &self.callee_saved,
            float_callee_saved: &self.float_callee_saved,
            ..*pools
        }
    }
}

/// The selection and the AArch64 covers layered on it, each keyed by the
/// instruction it replaces.
pub(super) struct Covers {
    pub selection: crate::select::Selection,
    pub absolute_values: HashMap<Inst, Value>,
    pub absolute_differences: HashMap<Inst, absolute_diff::ExactSabal>,
    pub packed_absolute_values: HashMap<Inst, Value>,
    pub packed_minmax: HashMap<Inst, packed_minmax::MinMax>,
    pub extended_adds: HashMap<Inst, extended_add::ExtendedAdd>,
    pub packed_low_loads: HashMap<Inst, Value>,
    pub replicate_loads: HashMap<Inst, Value>,
    pub constant_shifts: HashMap<Inst, u32>,
    pub widening_adds: HashMap<Inst, vector::WideningAdd>,
    pub fused_compares: HashSet<Value>,
}

impl Covers {
    pub fn select(
        func: &Function,
        pipeline: &crate::CodegenPipeline<'_>,
        facts: &crate::kinds::FunctionFacts<'_>,
        leaf_summaries: &leaf_calls::Summaries,
    ) -> Self {
        let (present, uses) = (facts.kinds, &facts.machine_uses);
        let mut selection = {
            backend_scope!(Selection);
            pipeline.selector.select_with_uses(
                func,
                crate::select::Target::Aarch64,
                &|data| {
                    folded_imm(func, data)
                        .map(|(v, _)| v)
                        .into_iter()
                        .chain(folded_divisor(func, data))
                        .collect()
                },
                uses,
            )
        };
        selection.omitted_defs = crate::kinds::unused_cas_flags(facts);
        let on = |cover| selection.enabled(cover);
        let [
            abs,
            abs_diff,
            packed_abs,
            minmax,
            extended_add,
            select_compare,
        ] = [
            Cover::Aarch64Abs,
            Cover::Aarch64AbsDiff,
            Cover::PackedAbs,
            Cover::Aarch64PackedMinmax,
            Cover::Aarch64ExtendedAdd,
            Cover::Aarch64SelectCompare,
        ]
        .map(on);
        let [low, replicate, immediate, widening_add, compare_branch] = [
            Cover::Aarch64PackedLowLoad,
            Cover::Aarch64ReplicateLoad,
            Cover::Aarch64VectorImmediate,
            Cover::Aarch64WideningAdd,
            Cover::CompareBranch,
        ]
        .map(on);
        // Each cover is anchored at an instruction kind; without one it
        // selects nothing and leaves the selection untouched.
        let absolute_values = if present.select && abs {
            absolute::select(func, &mut selection, uses)
        } else {
            HashMap::default()
        };
        let absolute_differences = if present.widen_low && abs_diff {
            absolute_diff::select(func, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_absolute_values = if present.vector_select && packed_abs {
            crate::packed_absolute::select(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_minmax = if present.vector_icmp && minmax {
            packed_minmax::select(func, &mut selection)
        } else {
            HashMap::default()
        };
        // Inline assembly may observe the leaves; such functions keep them.
        let extended_adds = if present.ptradd && !present.inline_asm && extended_add {
            extended_add::select(func, &mut selection, uses)
        } else {
            HashMap::default()
        };
        if present.select && select_compare {
            selection.fuse_aarch64_selects(
                func,
                |data| folded_imm(func, data).map(|(v, _)| v).into_iter().collect(),
                uses,
            );
        }
        let packed_low_loads = if present.vector_splat && low {
            packed_load::select(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let replicate_loads = if present.vector_splat && replicate {
            vector::select_replicate_loads(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let constant_shifts = if present.vector_splat && immediate {
            vector::select_constant_operands(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let widening_adds = if present.widen_low && widening_add {
            vector::select_widening_adds(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let mut fused_compares = if present.compare && present.brif && compare_branch {
            backend_scope!(CompareFusion);
            selection.fuse_aarch64_compares(
                func,
                |data| folded_imm(func, data).map(|(v, _)| v).into_iter().collect(),
                uses,
            )
        } else {
            HashSet::default()
        };
        if present.atomic_cas && present.brif && compare_branch {
            fused_compares.extend(selection.fuse_cas_branches(func, uses));
        }
        let calls = present.call && !leaf_summaries.is_empty();
        for (_, inst) in func.layout_insts().filter(|_| calls) {
            if let InstData::Call { func: callee, .. } = func.inst_data(inst) {
                let decl = func.callee(*callee);
                if let Some((sig, preserved)) = leaf_summaries.get(&decl.name)
                    && &decl.sig == sig
                {
                    selection.preserved_call_gprs.insert(inst, *preserved);
                }
            }
        }
        Self {
            selection,
            absolute_values,
            absolute_differences,
            packed_absolute_values,
            packed_minmax,
            extended_adds,
            packed_low_loads,
            replicate_loads,
            constant_shifts,
            widening_adds,
            fused_compares,
        }
    }
}

/// Runs the allocator and validates the result.
pub(super) fn allocate(
    func: &Function,
    pools: &Pools,
    selection: &crate::select::Selection,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<Allocation, String> {
    let is_call = |data: &InstData| is_call(func, data);
    let returns_twice = |data: &InstData| lowering::returns_twice(func, data);
    let input = crate::regalloc::AllocationInput {
        func,
        pools,
        selection,
        is_call: &is_call,
        returns_twice: &returns_twice,
    };
    let mut alloc = pipeline.allocator.allocate(&input);
    alloc.finish(&input);
    alloc.validate(&input)?;
    Ok(alloc)
}

/// Adds the registers inline assembly clobbers, and the base register of
/// a function that moves `sp`, to those the prologue saves.
pub(super) fn add_saved_registers(
    alloc: &mut Allocation,
    mut asm_saved: Vec<u8>,
    asm_float_saved: Vec<u8>,
    moves_sp: bool,
) {
    if moves_sp {
        asm_saved.push(BASE);
    }
    for (used, extra) in [
        (&mut alloc.callee_saved_used, asm_saved),
        (&mut alloc.float_callee_saved_used, asm_float_saved),
    ] {
        used.extend(extra);
        used.sort_unstable();
        used.dedup();
    }
}

/// The spill slots and the copy temporary, above the call area.
pub(super) struct SpillArea {
    pub vector_spills: HashMap<u32, u32>,
    pub copy_temp: Option<u32>,
    /// The first offset past the area.
    pub end: usize,
}

impl SpillArea {
    pub fn plan(func: &Function, alloc: &Allocation, spill_base: usize) -> Result<Self, String> {
        let mut vector_spills = HashMap::default();
        let has_vectors = (0..func.num_values()).any(|i| {
            func.value_type(Value::new(i)).is_vector() && alloc.loc(Value::new(i)).is_some()
        });
        if has_vectors {
            for i in 0..func.num_values() {
                let v = Value::new(i);
                if func.value_type(v).is_vector()
                    && let Some(Loc::Stack(slot)) = alloc.loc(v)
                {
                    vector_spills.insert(slot, func.value_type(v).bytes().unwrap());
                }
            }
        }
        let copy_temp = has_vectors.then_some(alloc.spill_slots);
        if let Some(slot) = copy_temp {
            vector_spills.insert(slot, 16);
        }
        let offset =
            spill_base + 8 * (alloc.spill_slots as usize + if has_vectors { 2 } else { 0 });
        if offset > MAX_SLOT_OFFSET {
            return Err(format!(
                "{offset} bytes of spill slots is too many for this allocator"
            ));
        }
        Ok(Self {
            vector_spills,
            copy_temp,
            end: offset,
        })
    }
}

/// A private leaf that neither needs nor observes a stack frame.
/// `incoming` is the bytes of stack parameters.
pub(super) fn frameless(
    func: &Function,
    module: &Module,
    has_tls_getter: bool,
    frame_size: usize,
    incoming: u32,
    alloc: &Allocation,
) -> bool {
    func.sig.cc == CallConv::Fast
        && !has_tls_getter
        && module.files.is_empty()
        && func.personality.is_none()
        && frame_size == 0
        && incoming == 0
        && alloc.callee_saved_used.is_empty()
        && alloc.float_callee_saved_used.is_empty()
        && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        && !func.layout_insts().any(|(_, i)| {
            let d = func.inst_data(i);
            is_call(func, d)
                || matches!(
                    d,
                    InstData::Intrinsic { .. }
                        | InstData::InlineAsm { .. }
                        | InstData::ReturnCall { .. }
                        | InstData::ReturnCallIndirect { .. }
                        | InstData::LandingPad { .. }
                )
        })
}
