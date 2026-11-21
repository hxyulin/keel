//! The phases of [`FunctionCompiler::new`], in the order it runs them:
//! preflight checks, vector shape, the call area at the bottom of the
//! frame, selection and its covers, allocation, spill homes, the fixed
//! system slots, inline assembly plans, and the frame size.
use super::*;
use crate::cover::Cover;

/// The widest vector the function handles, and what it asks of the frame.
pub(super) struct VectorShape {
    /// The widest vector an ABI wrapper passes in registers, or 0.
    pub wrapper_bytes: u32,
    pub simd_bytes: u32,
    pub simd_bits: u16,
    pub wide_vectors: bool,
    pub frame_align: usize,
    pub frame_register: Option<u8>,
}

impl VectorShape {
    pub fn of(
        func: &Function,
        facts: &crate::kinds::FunctionFacts<'_>,
        call_alignment: usize,
    ) -> Self {
        let abi_align = |sig: &keel_ir::Signature| {
            sig.params
                .iter()
                .chain(&sig.returns)
                .filter_map(|p| {
                    p.layout()
                        .map(|l| l.align() as usize)
                        .or_else(|| matches!(p.ty, Type::I128 | Type::F80).then_some(16))
                })
                .max()
                .unwrap_or(call_alignment)
        };
        let wrappers = |sig: &keel_ir::Signature| {
            sig.params
                .iter()
                .chain(&sig.returns)
                .filter_map(|p| p.layout())
                .filter_map(
                    |layout| match sysv::aggregate_policy(layout, sig.x86_vector_abi) {
                        sysv::ProducerAggregate::Vector(ty) => Some(ty),
                        _ => None,
                    },
                )
                .map(|t| t.bytes().unwrap())
                .max()
                .unwrap_or(0)
        };
        let wrapper_bytes = std::iter::once(wrappers(&func.sig))
            .chain(func.callees().map(|(_, d)| wrappers(&d.sig)))
            .chain(func.signatures().map(|(_, s)| wrappers(s)))
            .max()
            .unwrap_or(0);
        let mut abi_align = std::iter::once(abi_align(&func.sig))
            .chain(func.callees().map(|(_, d)| abi_align(&d.sig)))
            .chain(func.signatures().map(|(_, s)| abi_align(s)))
            .max()
            .unwrap_or(call_alignment);
        // The strictest alignment of an argument or `va_arg` aggregate.
        for &inst in &facts.calls {
            let args = func.inst_data(inst).call_args().expect("a call");
            for (i, &arg) in args.iter().enumerate() {
                if let Some(layout) = args.abi(i).and_then(|p| p.layout()) {
                    abi_align = abi_align.max(layout.align() as usize);
                } else if call_alignment == 8
                    && matches!(func.value_type(func.resolve(arg)), Type::I128 | Type::F80)
                {
                    abi_align = abi_align.max(16);
                }
            }
        }
        if facts.kinds.aggregate_va_arg {
            for (_, inst) in func.layout_insts() {
                if let InstData::AggregateVaArg { layout, .. } = func.inst_data(inst) {
                    abi_align = abi_align.max(layout.align() as usize);
                }
            }
        }
        // The widest vector parameter or result. Each is in the value table:
        // when nothing there is wider than the floor, the walk cannot raise it.
        let vector_bytes = |value: Value| {
            func.value_type(func.resolve(value))
                .vector()
                .map_or(0, |v| v.bytes())
        };
        let mut vector_align = wrapper_bytes.max(call_alignment as u32);
        if (0..func.num_values()).any(|i| vector_bytes(Value::new(i)) > vector_align) {
            for block in func.blocks() {
                for &param in func.block_params(block) {
                    vector_align = vector_align.max(vector_bytes(param));
                }
                for &inst in func.block_insts(block) {
                    for &result in func.inst_results(inst) {
                        vector_align = vector_align.max(vector_bytes(result));
                    }
                }
            }
        }
        let simd_bytes = vector_align.max(16);
        let simd_bits = (simd_bytes * 8) as u16;
        let wide_vectors = simd_bytes >= 32;
        // `stack_alloc` promises 16-byte storage, and the variadic SIMD
        // save area is written with MOVAPS.
        let local_minimum =
            if (func.sig.variadic && !func.attrs.x86_no_sse) || facts.kinds.stack_alloc {
                16
            } else {
                call_alignment
            };
        let frame_align = func
            .stack_slots()
            .map(|(_, s)| s.align as usize)
            .max()
            .unwrap_or(local_minimum)
            .max(local_minimum)
            .max(vector_align as usize)
            .max(abi_align);
        let frame_register = (frame_align > call_alignment).then_some(15);
        Self {
            wrapper_bytes,
            simd_bytes,
            simd_bits,
            wide_vectors,
            frame_align,
            frame_register,
        }
    }
}

/// Rejects what the backend cannot lower: unsupported signatures and
/// instructions, malformed FLAGS regions, and illegal intrinsics.
pub(super) fn preflight(
    func: &Function,
    features: keel_asm_ir::x86::Features,
) -> Result<(), String> {
    let not_yet = |what: &str| format!("{what} not supported on x86-64 yet");
    if func.attrs.x86_no_x87
        && (func
            .sig
            .params
            .iter()
            .chain(&func.sig.returns)
            .any(|p| p.ty == Type::F80)
            || func.layout_insts().any(|(_, i)| {
                let mut uses_x87 = func
                    .inst_results(i)
                    .iter()
                    .any(|&v| func.value_type(v) == Type::F80);
                func.inst_data(i)
                    .for_each_operand(|v| uses_x87 |= func.value_type(v) == Type::F80);
                uses_x87
            }))
    {
        return Err("x86 x87 register use is disabled".into());
    }
    if let Some(error) =
        func.layout_insts().find_map(|(_, i)| {
            match func.inst_data(i) {
            InstData::VaArg { ty: Type::I128, .. } => {
                Some("va_arg.i128 requires AArch64 Linux".into())
            }
            data if !features.cx16 && crate::lowering::wide_atomic(func, data) => Some(
            "i128 atomics require CX16: compile with `--cpu-level v2` or `--target-features cx16`"
                .into(),
        ),
            _ => None,
        }
        })
    {
        return Err(error);
    }
    let (CallConv::C | CallConv::Fast) = func.sig.cc;
    match func.sig.returns.as_slice() {
        [] => {}
        // clang assumes the callee extends these to 32 bits.
        [
            AbiParam {
                ty: Type::I8 | Type::I16,
                ext: ArgExt::None,
                ..
            },
        ] if func.sig.cc == CallConv::C => {
            return Err(
                "an i8/i16 return value needs `sext` or `uext`: C callers expect 32 bits".into(),
            );
        }
        [_] => {}
        _ => {
            sysv::multi_result_locs(&func.sig)?;
        }
    }
    if func.personality.is_some() {
        return Err(not_yet("exceptions are"));
    }
    for (_, inst) in func.layout_insts() {
        if let InstData::X86RflagsRegion { plan, args } = func.inst_data(inst) {
            keel_ir::rflags_region::analyze(plan).map_err(|e| e.to_string())?;
            if args.len() != plan.captures.len()
                || args
                    .iter()
                    .any(|&v| func.value_type(func.resolve(v)) != Type::I64)
                || func.inst_results(inst).len() != plan.observations
                || func
                    .inst_results(inst)
                    .iter()
                    .any(|&v| func.value_type(v) != Type::I64)
            {
                return Err("FLAGS region exact I64 capture/result signature mismatch".into());
            }
        }
        if let InstData::Intrinsic { id, args } = func.inst_data(inst) {
            if id.is_aarch64_target() {
                return Err(format!(
                    "{id} is an AArch64 intrinsic, not supported on x86-64"
                ));
            }
            if let Some(family) = id.x86_family() {
                intrinsic_family::validate(family, func, *id, args, features)?;
            }
        }
        if let Some(what) = unsupported(func, inst) {
            return Err(format!(
                "`{}`: {what} not supported on x86-64 yet",
                func.display_inst(inst)
            ));
        }
    }
    Ok(())
}

/// A tail call jumps to the callee with our caller's return address on
/// the stack, so its stack arguments go where ours came in: they must fit
/// there. Register-only single-vector wrappers also fit.
pub(super) fn check_guaranteed_tails(
    func: &Function,
    named: &sysv::Placement,
    present: crate::kinds::InstKinds,
    call_alignment: usize,
) -> Result<(), String> {
    if !present.return_call {
        return Ok(());
    }
    for (_, inst) in func.layout_insts() {
        let (args, sig) = match func.inst_data(inst) {
            InstData::ReturnCall { func: callee, args } => (args, &func.callee(*callee).sig),
            InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
            _ => continue,
        };
        let placement = sysv::signature_arg_locs(sig, &arg_kinds(func, sig, args));
        if placement.stack > named.stack {
            return Err(format!(
                "`{}`: guaranteed tail outgoing stack area grows from {} to {} bytes",
                func.display_inst(inst),
                named.stack,
                placement.stack
            ));
        }
        let incoming_alignment = named.stack_align(call_alignment as u32);
        let outgoing_alignment = placement.stack_align(call_alignment as u32);
        if outgoing_alignment > incoming_alignment {
            return Err(format!(
                "`{}`: guaranteed tail requires unsupported incoming stack alignment {} (received {})",
                func.display_inst(inst),
                outgoing_alignment,
                incoming_alignment,
            ));
        }
        // Logical void is insufficient: aggregate results may use ST0,
        // GP/SIMD registers, or a hidden RDI pointer. Require the same
        // result layout and placement, and forward the original IR result
        // parameter (also needed when the physical result is in registers).
        let result_param = |s: &keel_ir::Signature| {
            s.params
                .iter()
                .position(|p| matches!(p.purpose, Purpose::Sret(_)))
        };
        let incoming_result = result_param(&func.sig);
        let outgoing_result = result_param(sig);
        let matching_result = match (incoming_result, outgoing_result) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                func.sig.params[a] == sig.params[b]
                    && named.locs[a] == placement.locs[b]
                    && func.entry_block().is_some_and(|entry| {
                        func.resolve(args[b]) == func.resolve(func.block_params(entry)[a])
                    })
            }
            _ => false,
        };
        if !matching_result
            || func.sig.returns != sig.returns
            || sysv::scalar_memory_result(&func.sig) != sysv::scalar_memory_result(sig)
        {
            return Err(format!(
                "`{}`: guaranteed tail return protocol mismatch or non-forwarded sret destination",
                func.display_inst(inst)
            ));
        }
    }
    Ok(())
}

/// The bottom of the frame: what calls need (their stack arguments,
/// staging for struct arguments in registers, and a slot for an `sret`
/// address), then this function's own result and parameter memory.
pub(super) struct CallArea {
    pub outgoing: usize,
    /// Eight-byte staging slots placed after the spill slots.
    pub staging: usize,
    pub scalar_result: Option<usize>,
    pub sret_save: Option<usize>,
    pub sret_ptr: Option<usize>,
    pub param_areas: Vec<Option<usize>>,
    pub reg_save: Option<usize>,
    /// The first offset past the area.
    pub end: usize,
}

impl CallArea {
    pub fn plan(
        func: &Function,
        facts: &crate::kinds::FunctionFacts<'_>,
        named: &sysv::Placement,
        frame_align: usize,
    ) -> Self {
        // An f80 FMA calls `fmal` with its three operands on the stack.
        let f80_fma = facts.kinds.fma
            && func
                .layout_insts()
                .any(|(_, inst)| match func.inst_data(inst) {
                    InstData::Fma { args } => func.value_type(func.resolve(args[0])) == Type::F80,
                    _ => false,
                });
        let (mut outgoing, mut staging, mut needs_sret_save) =
            (if f80_fma { 48 } else { 0 }, 0, false);
        let mut scalar_result = false;
        for &inst in &facts.calls {
            let data = func.inst_data(inst);
            let (args, sig) = match data {
                InstData::Call { func: callee, args } => (args, &func.callee(*callee).sig),
                InstData::CallIndirect { sig, args, .. }
                | InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
                InstData::ReturnCall { func: callee, args } => (args, &func.callee(*callee).sig),
                _ => continue,
            };
            scalar_result |= matches!(data, InstData::Call { .. } | InstData::CallIndirect { .. })
                && sysv::scalar_memory_result(sig);
            let params = &sig.params;
            // Up to three scalars take at most six integer registers: they
            // need neither stack nor staging.
            if !sysv::scalar_memory_result(sig)
                && args.len() <= 3
                && params.iter().all(|p| p.purpose == Purpose::Normal)
                && !(sig.params.len()..args.len())
                    .any(|i| args.abi(i).is_some_and(|p| p.purpose != Purpose::Normal))
                && !args.iter().any(|&v| {
                    matches!(
                        func.value_type(func.resolve(v)),
                        Type::F80 | Type::Vector(keel_ir::VectorType::F64x1)
                    ) || func
                        .value_type(func.resolve(v))
                        .bytes()
                        .is_some_and(|bytes| bytes >= 32)
                })
            {
                continue;
            }
            let placement = sysv::signature_arg_locs(sig, &arg_kinds(func, sig, args));
            outgoing = outgoing.max(placement.stack);
            let mut halves = 0;
            for (i, loc) in placement.locs.iter().enumerate() {
                match loc {
                    ArgLoc::InRegs(regs) => {
                        halves += args
                            .abi(i)
                            .or_else(|| params.get(i))
                            .and_then(|p| p.layout())
                            .and_then(sysv::register_vector)
                            .map_or(regs.len(), |t| (t.bytes().unwrap() / 8) as usize)
                    }
                    ArgLoc::SretRegs(_) | ArgLoc::SretX87 => needs_sret_save = true,
                    _ => {}
                }
            }
            staging = staging.max(halves);
        }
        let outgoing = (outgoing as usize).next_multiple_of(frame_align);
        let mut offset = outgoing;
        let slot8 = |offset: &mut usize| {
            *offset += 8;
            *offset - 8
        };
        let scalar_result = scalar_result.then(|| slot8(&mut offset));
        let sret_save = needs_sret_save.then(|| slot8(&mut offset));
        let sret_ptr = (named.locs.contains(&ArgLoc::SretMemory)
            || sysv::scalar_memory_result(&func.sig))
        .then(|| slot8(&mut offset));
        let mut param_areas = Vec::new();
        for (loc, param) in named.locs.iter().zip(&func.sig.params) {
            param_areas.push(match loc {
                ArgLoc::InRegs(_) | ArgLoc::SretRegs(_) | ArgLoc::SretX87 => {
                    let layout = param.layout().expect("a struct");
                    offset = offset.next_multiple_of(layout.align().max(8) as usize);
                    let area = offset;
                    offset += layout.size().next_multiple_of(8) as usize;
                    Some(area)
                }
                _ => None,
            });
        }
        // `movaps` stores need 16-byte alignment. GPR-only varargs
        // use just the integer portion of the System V save area.
        let reg_save = func.sig.variadic.then(|| {
            let (alignment, bytes) = if func.attrs.x86_no_sse {
                (8, 48)
            } else {
                (16, 176)
            };
            let base = offset.next_multiple_of(alignment);
            offset = base + bytes;
            base
        });
        Self {
            outgoing,
            staging,
            scalar_result,
            sret_save,
            sret_ptr,
            param_areas,
            reg_save,
            end: offset,
        }
    }
}

fn prefers_dispatch_allocation(func: &Function) -> bool {
    use keel_ir::dominators::{ControlFlowGraph, DominatorTree};

    if func.num_blocks() < 64 {
        return false;
    }
    if func
        .blocks()
        .filter_map(|block| func.block_insts(block).last())
        .any(|&switch| crate::dispatch_tail::is_large_dense_switch(func, switch))
    {
        return true;
    }
    if !func.local || func.num_blocks() < 128 {
        return false;
    }
    // A smaller dispatch benefits when state remains live across a loop
    // backedge. Keep the original policy for smaller one-off switches.
    let mut switches = func.blocks().filter(|&block| {
        let body = func.block_insts(block);
        body.len() > 1
            && body
                .last()
                .is_some_and(|&switch| crate::dispatch_tail::is_dense_switch(func, switch, 24))
    });
    let Some(block) = switches.next() else {
        return false;
    };
    if switches.next().is_some() {
        return false;
    }
    let cfg = ControlFlowGraph::compute(func);
    let dom = DominatorTree::compute(func, &cfg);
    cfg.preds(block)
        .iter()
        .any(|&pred| dom.dominates(block, pred))
}

/// The selection and the x86 covers layered on it, each keyed by the
/// instruction it replaces.
pub(super) struct Covers {
    pub selection: crate::select::Selection,
    pub fused_compares: HashSet<Value>,
    pub packed_absolute_values: HashMap<Inst, Value>,
    pub packed_positive_values: HashMap<Inst, Value>,
    pub packed_sign_values: HashMap<Inst, packed_sign::Plan>,
    pub packed_low_loads: HashMap<Inst, Value>,
}

impl Covers {
    pub fn select(
        func: &Function,
        pipeline: &crate::CodegenPipeline<'_>,
        facts: &crate::kinds::FunctionFacts<'_>,
        across_stack_addr: bool,
    ) -> Self {
        let (present, uses) = (facts.kinds, &facts.machine_uses);
        let mut selection = pipeline.selector.select_with_uses(
            func,
            crate::select::Target::X86,
            &|data| folds(func, data),
            uses,
        );
        selection.omitted_defs = crate::kinds::unused_cas_flags(facts);
        selection.large_dispatch_allocation = prefers_dispatch_allocation(func);
        let on = |cover| selection.enabled(cover);
        let [
            mask_tests,
            compare_branch,
            select_compare,
            packed_abs,
            positive,
            sign,
            low,
            wide,
        ] = [
            Cover::X86MaskTest,
            Cover::CompareBranch,
            Cover::X86SelectCompare,
            Cover::PackedAbs,
            Cover::X86PackedPositive,
            Cover::X86PackedSign,
            Cover::X86PackedLowLoad,
            Cover::X86PackedWideLoad,
        ]
        .map(on);
        let signed_byte = on(Cover::X86SignedByteLoad);
        if present.compare && mask_tests {
            selection.fold_mask_tests(func, uses, Cover::X86MaskTest, |ty, _| ty == Type::I32);
        }
        // Each cover is anchored at an instruction kind; without one it
        // selects nothing and leaves the selection untouched.
        let mut fused_compares = if present.compare && present.brif && compare_branch {
            backend_scope!(CompareFusion);
            selection.fuse_compares(func, |data| folds(func, data), uses, across_stack_addr)
        } else {
            HashSet::default()
        };
        if present.atomic_cas && present.brif && compare_branch {
            fused_compares.extend(selection.fuse_cas_branches(func, uses));
        }
        if present.select && select_compare {
            selection.fuse_x86_selects(func, |data| folds(func, data), uses, present.inline_asm);
        }
        let packed_absolute_values = if present.vector_select && packed_abs {
            crate::packed_absolute::select(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_positive_values = if present.vector_splat && positive {
            packed_positive::select(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_sign_values = if present.vector_icmp && sign {
            packed_sign::select(facts, &mut selection)
        } else {
            HashMap::default()
        };
        let mut packed_low_loads = if present.vector_splat && low {
            packed_load::select(
                facts,
                &mut selection,
                pipeline.selector.x86_features().sse41,
            )
        } else {
            HashMap::default()
        };
        if present.widen_low && wide {
            packed_low_loads.extend(packed_load::select_wide(
                facts,
                &mut selection,
                pipeline.selector.x86_features(),
            ));
        }
        if signed_byte {
            selection.fold_signed_byte_loads(func, uses);
        }
        Self {
            selection,
            fused_compares,
            packed_absolute_values,
            packed_positive_values,
            packed_sign_values,
            packed_low_loads,
        }
    }
}

/// Keep scalar values in their first ABI register when interference permits.
pub(super) fn prefer_abi_gprs(
    func: &Function,
    named: &sysv::Placement,
    selection: &mut crate::select::Selection,
) {
    let hint = |selection: &mut crate::select::Selection, value: Value, loc: &ArgLoc| {
        if let ArgLoc::Value(Slot::Regs(register, None)) = loc {
            selection.prefer_abi_gpr(func, value, *register);
        }
    };
    if let Some(entry) = func.entry_block() {
        for (&value, loc) in func.block_params(entry).iter().zip(&named.locs) {
            hint(selection, value, loc);
        }
    }
    let mut placement = sysv::Placement::default();
    for (_, inst) in func.layout_insts() {
        let (args, sig) = match func.inst_data(inst) {
            InstData::Call { func: callee, args } | InstData::ReturnCall { func: callee, args } => {
                (args, &func.callee(*callee).sig)
            }
            InstData::CallIndirect { sig, args, .. }
            | InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
            InstData::Return { args } if func.sig.returns.len() == 1 => {
                for &value in args {
                    selection.prefer_abi_gpr(func, value, Loc::Reg(RAX));
                }
                continue;
            }
            _ => continue,
        };
        placement.fill_signature(sig, arg_kinds(func, sig, args));
        for (&value, loc) in args.iter().zip(&placement.locs) {
            hint(selection, value, loc);
        }
        if sig.returns.len() == 1 {
            for &value in func.inst_results(inst) {
                selection.prefer_abi_gpr(func, value, Loc::Reg(RAX));
            }
        }
    }
}

#[derive(Default)]
pub(super) struct KernelImmediates {
    pub stores: HashMap<Inst, keel_ir::Symbol>,
    pub memory_args: HashMap<Inst, [Option<Value>; 3]>,
}

impl KernelImmediates {
    pub fn select(
        func: &Function,
        symbols: &crate::symbols::Symbols<'_>,
        facts: &crate::kinds::FunctionFacts<'_>,
        shape: &VectorShape,
        selection: &mut crate::select::Selection,
    ) -> Self {
        let mut immediates = Self::default();
        if !facts.kinds.symbol_addr && (!facts.kinds.intrinsic || shape.wide_vectors) {
            return immediates;
        }
        let static_symbol = |symbol| {
            !symbols
                .data_object(func.symbol_name(symbol))
                .is_some_and(|data| data.thread_local)
        };
        for (block, inst) in func.layout_insts() {
            if selection.covered(inst) || selection.has_constraint(inst) {
                continue;
            }
            let source = |value: Value, selection: &crate::select::Selection| {
                let ValueDef::Result(source, _) = func.value_def(value) else {
                    return None;
                };
                (facts.machine_uses[value.index()] == 1
                    && func.inst_block(source) == block
                    && selection.is_free(source))
                .then_some(source)
            };
            match func.inst_data(inst) {
                &InstData::Store { value, .. } if facts.kinds.symbol_addr => {
                    let value = func.resolve(value);
                    let Some(source) = source(value, selection) else {
                        continue;
                    };
                    let InstData::SymbolAddr { symbol } = *func.inst_data(source) else {
                        continue;
                    };
                    if !static_symbol(symbol) {
                        continue;
                    }
                    let mut operands = selection.operands(func, inst);
                    let Some(index) = operands.iter().position(|&v| v == value) else {
                        continue;
                    };
                    operands.remove(index);
                    selection.set_operands(inst, operands);
                    selection.cover(source);
                    immediates.stores.insert(inst, symbol);
                }
                InstData::Intrinsic {
                    id:
                        Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp,
                    args,
                } if !shape.wide_vectors && args.len() == 3 => {
                    let mut operands = selection.operands(func, inst);
                    let mut values = [None; 3];
                    for (entry, &arg) in values.iter_mut().zip(args) {
                        let value = func.resolve(arg);
                        let Some(source) = source(value, selection) else {
                            continue;
                        };
                        match *func.inst_data(source) {
                            InstData::Iconst { .. } => {}
                            InstData::SymbolAddr { symbol } if static_symbol(symbol) => {}
                            _ => continue,
                        }
                        let Some(index) = operands.iter().position(|&v| v == value) else {
                            continue;
                        };
                        operands.remove(index);
                        selection.cover(source);
                        *entry = Some(value);
                    }
                    if values.iter().any(Option::is_some) {
                        selection.set_operands(inst, operands);
                        immediates.memory_args.insert(inst, values);
                    }
                }
                _ => {}
            }
        }
        immediates
    }
}

pub(super) struct ForwardedReturn {
    pub call: Inst,
    pub ret: Inst,
    pub value: Value,
}

/// Leave a single-use call result in rax across an immediately adjacent return.
pub(super) fn forwarded_returns(
    func: &Function,
    facts: &crate::kinds::FunctionFacts<'_>,
    selection: &mut crate::select::Selection,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Vec<ForwardedReturn> {
    let mut sites = Vec::new();
    let [ret] = func.sig.returns.as_slice() else {
        return sites;
    };
    let scalar_result = |sig: &keel_ir::Signature| {
        sig.cc == CallConv::C
            && !sig.returns_twice
            && !sig
                .params
                .iter()
                .any(|p| matches!(p.purpose, Purpose::Sret(_)))
    };
    if !facts.kinds.call
        || !scalar_result(&func.sig)
        || !matches!(ret.ty, Type::I32 | Type::I64 | Type::Ptr)
        || ret.ext != ArgExt::None
        || ret.purpose != Purpose::Normal
        || (pipeline.allocator.name() != "compact"
            && pipeline.allocator.call_site_base() != Some(regalloc::AllocationTier::Compact))
    {
        return sites;
    }
    for block in func.blocks() {
        let Some((_, &[call, ret])) = func.block_insts(block).split_last_chunk::<2>() else {
            continue;
        };
        let InstData::Call { func: callee, .. } = *func.inst_data(call) else {
            continue;
        };
        let InstData::Return { args } = func.inst_data(ret) else {
            continue;
        };
        let ([value], [returned]) = (func.inst_results(call), args.as_slice()) else {
            continue;
        };
        let value = *value;
        let sig = &func.callee(callee).sig;
        if func.resolve(*returned) == value
            && func.resolve(value) == value
            && facts.machine_uses[value.index()] == 1
            && scalar_result(sig)
            && sig.returns == func.sig.returns
            && selection.is_free(call)
            && selection.is_free(ret)
            && !selection.has_operand_override(ret)
            && selection.omitted_defs.insert(value)
        {
            selection.set_operands(ret, Vec::new());
            sites.push(ForwardedReturn { call, ret, value });
        }
    }
    sites
}

pub(super) fn returns_preserve_rax(alloc: &Allocation, sites: &[ForwardedReturn]) -> bool {
    sites.iter().all(|site| {
        alloc
            .edits
            .instruction_index(site.call)
            .is_none_or(|i| alloc.edits.instruction(i).after.is_empty())
            && alloc
                .edits
                .instruction_index(site.ret)
                .is_none_or(|i| alloc.edits.instruction(i).before.is_empty())
    })
}

/// Every ordinary frame reference sees the same rsp after the prologue.
/// Inline asm operands still use rbp because the template may push registers.
pub(super) fn has_fixed_kernel_frame(func: &Function, present: crate::kinds::InstKinds) -> bool {
    func.attrs.x86_no_sse
        && !func.sig.returns_twice
        && func.personality.is_none()
        && !present.return_call
        && !present.landing_pad
        && !present.stack_alloc
        && (!present.call && !present.intrinsic
            || func
                .layout_insts()
                .all(|(_, inst)| match func.inst_data(inst) {
                    InstData::Intrinsic { id, .. } => matches!(
                        id,
                        Intrinsic::Memcpy
                            | Intrinsic::Memmove
                            | Intrinsic::Memset
                            | Intrinsic::Memcmp
                            | Intrinsic::LifetimeStart
                            | Intrinsic::LifetimeEnd
                    ),
                    InstData::Invoke { .. } | InstData::X86RflagsRegion { .. } => false,
                    _ => true,
                }))
}

/// Index the relative offsets by destination label, including distinct copy
/// stubs for edges that reach the same block with different arguments.
pub(super) fn compact_switch_table(
    labels: impl Iterator<Item = String>,
) -> Option<(Vec<String>, Vec<u8>)> {
    let mut targets = Vec::new();
    let mut indices = Vec::new();
    let mut seen = HashMap::default();
    for label in labels {
        let index = *seen.entry(label.clone()).or_insert_with(|| {
            let index = targets.len();
            targets.push(label);
            index
        });
        indices.push(u8::try_from(index).ok()?);
    }
    // Compared with four bytes per case, leave room for the indexed byte load.
    (targets.len() * 4 + 8 < indices.len() * 3).then_some((targets, indices))
}

/// Runs the allocator, places the extended-precision stack homes and
/// validates the result. Gives the allocation and those homes' slots.
pub(super) fn allocate(
    func: &Function,
    pools: &Pools,
    selection: &crate::select::Selection,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<(Allocation, HashSet<u32>), String> {
    // Read the features once: the allocator tests every instruction.
    let features = pipeline.selector.x86_features();
    let is_call = |data: &InstData| is_call(func, data, features);
    let returns_twice = |data: &InstData| lowering::returns_twice(func, data);
    let input = crate::regalloc::AllocationInput {
        func,
        pools,
        selection,
        is_call: &is_call,
        returns_twice: &returns_twice,
    };
    let mut alloc = pipeline.allocator.allocate(&input);
    let f80_homes = f80::stack_homes(&input, &mut alloc)?;
    alloc.finish(&input);
    alloc.validate(&input)?;
    f80::validate_homes(&input, &alloc)?;
    Ok((alloc, f80_homes))
}

/// The spill slots, the staging slots after them, and the copy
/// temporary, which starts the area above the call area.
pub(super) struct SpillArea {
    pub vector_spills: HashMap<u32, u16>,
    pub has_vectors: bool,
    pub copy_temp: Option<u32>,
    pub spill_base: usize,
    /// The first offset past the area.
    pub end: usize,
}

impl SpillArea {
    pub fn plan(
        func: &Function,
        alloc: &Allocation,
        f80_homes: HashSet<u32>,
        shape: &VectorShape,
        calls: &CallArea,
    ) -> Self {
        let mut vector_spills = HashMap::default();
        let has_vectors = shape.wrapper_bytes != 0
            || (0..func.num_values()).any(|i| func.value_type(Value::new(i)) == Type::F80)
            || (0..func.num_values()).any(|i| {
                func.value_type(Value::new(i)).is_vector() && alloc.loc(Value::new(i)).is_some()
            });
        if has_vectors {
            for i in 0..func.num_values() {
                let value = Value::new(i);
                if (func.value_type(value).is_vector() || func.value_type(value) == Type::F80)
                    && let Some(Loc::Stack(slot)) = alloc.loc(value)
                {
                    vector_spills.insert(
                        slot,
                        if func.value_type(value) == Type::F80 {
                            128
                        } else {
                            ptr(func.value_type(value))
                        },
                    );
                }
            }
        }
        for slot in f80_homes {
            vector_spills.insert(slot, 128);
        }
        // Some instruction recipes also use this slot for staging. Keep their
        // storage conservatively; ordinary SIMD copies use reserved registers.
        let needs_copy_temp = has_vectors
            && (shape.wide_vectors
                || (0..func.num_values()).any(|i| func.value_type(Value::new(i)) == Type::F80)
                || func
                    .layout_insts()
                    .any(|(_, i)| matches!(func.inst_data(i), InstData::Intrinsic { .. })));
        let copy_temp = needs_copy_temp.then_some(alloc.spill_slots + calls.staging as u32);
        if let Some(slot) = copy_temp {
            vector_spills.insert(slot, shape.simd_bits);
        }
        let spill_base = calls.end.next_multiple_of(if shape.wide_vectors {
            shape.simd_bytes as usize
        } else {
            8
        });
        let end = spill_base
            + 8 * (alloc.spill_slots as usize
                + calls.staging
                + if needs_copy_temp {
                    shape.simd_bytes as usize / 8
                } else {
                    0
                });
        Self {
            vector_spills,
            has_vectors,
            copy_temp,
            spill_base,
            end,
        }
    }
}

/// Private slots for MXCSR, the x87 control and status words, a saved
/// RBX, string port I/O and FLAGS regions, each present only when an
/// instruction needs it.
pub(super) struct SystemSlots {
    pub mxcsr_save: Option<usize>,
    pub mxcsr_access: Option<usize>,
    pub x87_state: Option<usize>,
    pub cx16_save: Option<usize>,
    pub string_io_home: Option<usize>,
    pub rflags_home: Option<usize>,
    /// The first offset past the slots.
    pub end: usize,
}

impl SystemSlots {
    pub fn plan(
        func: &Function,
        present: crate::kinds::InstKinds,
        features: keel_asm_ir::x86::Features,
        offset: usize,
    ) -> Self {
        // `present.intrinsic` also covers FLAGS regions.
        let intrinsic = |want: fn(&InstData) -> bool| {
            present.intrinsic && func.layout_insts().any(|(_, i)| want(func.inst_data(i)))
        };
        // Without SSE4.1, rounding reads MXCSR through this slot.
        let mxcsr_save = (!features.sse41 && present.rounding).then_some(offset);
        let offset = offset + if mxcsr_save.is_some() { 4 } else { 0 };
        let mxcsr_access = intrinsic(|data| {
            matches!(
                data,
                InstData::Intrinsic {
                    id: Intrinsic::X86SseReadMxcsr | Intrinsic::X86SseSwapMxcsr,
                    ..
                }
            )
        })
        .then_some(offset.next_multiple_of(4));
        let offset = mxcsr_access.map_or(offset, |slot| slot + 8);
        let x87_state = intrinsic(|data| {
            matches!(
                data,
                InstData::Intrinsic {
                    id: Intrinsic::X86X87ReadCwNowait
                        | Intrinsic::X86X87ReadSwNowait
                        | Intrinsic::X86X87ReadCwWait
                        | Intrinsic::X86X87ReadSwWait
                        | Intrinsic::X86X87ExchangeCw,
                    ..
                }
            )
        })
        .then_some(offset.next_multiple_of(2));
        let offset = x87_state.map_or(offset, |slot| slot + 4);
        let cx16_save = ((present.intrinsic || present.atomic)
            && func.layout_insts().any(|(_, i)| {
                matches!(
                    func.inst_data(i),
                    InstData::Intrinsic {
                        id: Intrinsic::X86Cx16Cmpxchg16b
                            | Intrinsic::X86Cpuid
                            | Intrinsic::X86CpuXcr0,
                        ..
                    }
                ) || crate::lowering::wide_atomic(func, func.inst_data(i))
            }))
        .then_some(offset.next_multiple_of(8));
        let offset = cx16_save.map_or(offset, |slot| slot + 8);
        let string_io_home = intrinsic(|data| {
            matches!(data, InstData::Intrinsic { id, .. } if id.x86_string_port_io().is_some())
        })
        .then_some(offset.next_multiple_of(8));
        let offset = string_io_home.map_or(offset, |home| home + string_port_io::FRAME_BYTES);
        let rflags_bytes = (present.intrinsic.then(|| func.layout_insts()))
            .into_iter()
            .flatten()
            .filter_map(|(_, i)| match func.inst_data(i) {
                InstData::X86RflagsRegion { plan, .. } => {
                    Some(8 * (1 + plan.captures.len() + plan.observations))
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let rflags_home = (rflags_bytes != 0).then_some(offset.next_multiple_of(8));
        let end = rflags_home.map_or(offset, |home| home + rflags_bytes);
        Self {
            mxcsr_save,
            mxcsr_access,
            x87_state,
            cx16_save,
            string_io_home,
            rflags_home,
            end,
        }
    }
}

/// Plans each inline assembly statement. Only inline assembly needs the
/// module index; most functions have none.
pub(super) fn plan_inline_asm(
    module: &keel_ir::Module,
    func: &Function,
    present: crate::kinds::InstKinds,
    shape: &VectorShape,
    features: keel_asm_ir::x86::Features,
    compact_inputs: bool,
) -> Result<HashMap<Inst, inline_asm::Plan>, String> {
    let mut function_index = None;
    let mut inline_asm_plans = HashMap::default();
    if !present.inline_asm {
        return Ok(inline_asm_plans);
    }
    for (_, inst) in func.layout_insts() {
        if matches!(func.inst_data(inst), InstData::InlineAsm { .. }) {
            let function_index = *function_index.get_or_insert_with(|| {
                module
                    .functions
                    .iter()
                    .position(|f| f.name == func.name)
                    .expect("lowered function keeps its module identity") as u64
            });
            let plan = inline_asm::plan(
                module,
                func,
                inst,
                shape,
                (function_index << 32) | inst.index() as u64,
                features.avx512f,
                compact_inputs,
            )?;
            inline_asm_plans.insert(inst, plan);
        }
    }
    Ok(inline_asm_plans)
}

/// The callee-saved registers the prologue pushes: those the allocator
/// used, RBX for CX16 and CPUID, those inline assembly clobbers, and the
/// frame register.
pub(super) fn saved_registers(
    alloc: &Allocation,
    cx16_save: bool,
    inline_asm_plans: &HashMap<Inst, inline_asm::Plan>,
    frame_register: Option<u8>,
) -> Vec<u8> {
    // RSP is 16-byte aligned after PUSH RBP. Reserve call-site
    // alignment after accounting for the saved-register pushes.
    let mut saved = alloc.callee_saved_used.clone();
    // A fault can interrupt CX16 while RBX holds desired.low. The normal
    // prologue save/CFI must describe the original caller value even when
    // the allocator never used RBX (including an empty custom pool).
    if cx16_save && !saved.contains(&3) {
        saved.push(3);
    }
    for plan in inline_asm_plans.values() {
        for r in [3, 12, 13, 14, 15] {
            if plan.saved_regs.contains(&r) && !saved.contains(&r) {
                saved.push(r);
            }
        }
    }
    if let Some(r) = frame_register
        && !saved.contains(&r)
    {
        saved.push(r);
    }
    saved
}

/// No call boundary, implicit helper, stack observer or local storage
/// needs RSP alignment in this leaf. Saved-register pushes need only
/// eight-byte slots; keep the incoming/outgoing SysV call contract.
pub(super) fn simple_leaf(
    func: &Function,
    symbols: &crate::symbols::Symbols<'_>,
    present: crate::kinds::InstKinds,
    features: keel_asm_ir::x86::Features,
) -> bool {
    let has_tls_address = present.symbol_addr
        && symbols.has_thread_locals()
        && func.layout_insts().any(|(_, i)| match func.inst_data(i) {
            InstData::SymbolAddr { symbol } => symbols
                .data_object(func.symbol_name(*symbol))
                .is_some_and(|data| data.thread_local),
            _ => false,
        });
    !has_tls_address
        && symbols.module.files.is_empty()
        && func.personality.is_none()
        && !func.layout_insts().any(|(_, i)| {
            let d = func.inst_data(i);
            is_call(func, d, features)
                || matches!(
                    d,
                    InstData::Intrinsic { .. }
                        | InstData::X86RflagsRegion { .. }
                        | InstData::InlineAsm { .. }
                        | InstData::ReturnCall { .. }
                        | InstData::ReturnCallIndirect { .. }
                        | InstData::Invoke { .. }
                        | InstData::LandingPad { .. }
                )
        })
}

/// What `sub rsp` allocates below the saved registers, given `offset`
/// bytes of frame contents.
pub(super) fn frame_size(
    goal: crate::CodegenGoal,
    shape: &VectorShape,
    simple_leaf: bool,
    saved: &[u8],
    offset: usize,
    call_alignment: usize,
) -> Result<usize, String> {
    let frame_register = shape.frame_register;
    let frame_size = if goal == crate::CodegenGoal::Size
        && simple_leaf
        && offset == 0
        && frame_register.is_none()
    {
        0
    } else {
        (offset + 8 * saved.len()).next_multiple_of(call_alignment) - 8 * saved.len()
            + if frame_register.is_some() {
                shape.frame_align - 1
            } else {
                0
            }
    };
    if frame_size > i32::MAX as usize / 2 {
        return Err(format!("frame of {frame_size} bytes is too large"));
    }
    Ok(frame_size)
}

/// An odd number of saved GPRs aligns RSP for calls without a frame
/// pointer or an additional stack adjustment. Restrict the speed policy
/// to small local pointer wrappers with no stack storage or observers.
pub(super) fn register_only_frame(
    func: &Function,
    facts: &crate::kinds::FunctionFacts<'_>,
    named: &sysv::Placement,
    saved: &[u8],
    offset: usize,
) -> bool {
    let present = facts.kinds;
    offset == 0
        && !func.attrs.frame_pointer
        && func.local
        && (2..=3).contains(&func.sig.params.len())
        && func.sig.params[0].ty == Type::Ptr
        && (func.sig.returns.is_empty()
            || func.sig.returns.len() == 1 && func.sig.returns[0].ty == Type::Ptr)
        && func.blocks().take(19).count() <= 18
        && func.layout_insts().take(161).count() <= 160
        && !func.sig.variadic
        && !func.sig.returns_twice
        && !func.attrs.optnone
        && func.personality.is_none()
        && named.stack == 0
        && func.num_stack_slots() == 0
        && saved.len() >= 3
        && saved.len() % 2 == 1
        && present.call
        && !present.return_call
        && !present.intrinsic
        && !present.inline_asm
        && !present.landing_pad
        && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        && facts.calls.iter().all(|&i| {
            let InstData::Call { func: callee, args } = func.inst_data(i) else {
                return false;
            };
            let sig = &func.callee(*callee).sig;
            !sig.variadic
                && !sig.returns_twice
                && !args.has_explicit_abi()
                && args.len() <= 6
                && sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        })
}

/// Size mode also omits eligible scalar C leaf frames. Speed mode
/// keeps its existing layout, which was measured with scalar frames.
/// Keep frames for stack observers and functions needing frame storage.
pub(super) fn frameless(
    func: &Function,
    goal: crate::CodegenGoal,
    simple_leaf: bool,
    has_vectors: bool,
    frame_size: usize,
    named: &sysv::Placement,
    saved: &[u8],
) -> bool {
    !func.attrs.frame_pointer
        && (func.sig.cc == CallConv::Fast
            || (func.sig.cc == CallConv::C && (has_vectors || goal == crate::CodegenGoal::Size)))
        && simple_leaf
        && frame_size == 0
        && named.stack == 0
        && saved.is_empty()
        && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
}

/// The return site that owns the shared epilogue, in size mode.
///
/// With saved registers, the epilogue is at least a four-byte LEA,
/// one byte per POP, POP RBP and RET. A near JMP costs at most five
/// bytes. Keeping one existing return site avoids any extra copy or
/// fall-through branch, so each redirected return strictly saves bytes.
pub(super) fn shared_return(
    func: &Function,
    goal: crate::CodegenGoal,
    alloc: &Allocation,
    saved: &[u8],
) -> Option<Inst> {
    if goal == crate::CodegenGoal::Size && !saved.is_empty() {
        let mut returns = alloc
            .order
            .iter()
            .flat_map(|&block| func.block_insts(block))
            .copied()
            .filter(|&inst| matches!(func.inst_data(inst), InstData::Return { .. }));
        returns.next().and_then(|_| returns.next_back())
    } else {
        None
    }
}
