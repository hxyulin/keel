//! Stack-resident extended values. Only an ABI result is live on the x87 stack
//! between IR instructions. See docs/targets/x86_64/f80/README.md for the FP environment contract.
use super::*;

fn op(name: &str) -> Opcode {
    Opcode::parse(name).expect("inventoried x87 operation")
}
fn st(n: u8) -> Register {
    Register {
        class: RegisterClass::X87,
        number: n,
        width: 80,
        high_byte: false,
    }
}

pub(super) fn validate(module: &Module) -> Result<(), CodegenError> {
    for f in &module.functions {
        for (_, i) in f.layout_insts() {
            let d = f.inst_data(i);
            let mut extended = f
                .inst_results(i)
                .iter()
                .any(|&v| f.value_type(v) == Type::F80);
            d.for_each_operand(|v| extended |= f.value_type(f.resolve(v)) == Type::F80);
            if !extended {
                continue;
            }
            let supported = match d {
                InstData::Intrinsic { id, .. } if id.is_x86_x87_numerical() => true,
                InstData::Fma { .. }
                | InstData::VaArg { .. }
                | InstData::F80const { .. }
                | InstData::Load { .. }
                | InstData::Store { .. }
                | InstData::Select { .. }
                | InstData::Fcmp { .. }
                | InstData::Call { .. }
                | InstData::CallIndirect { .. }
                | InstData::ReturnCall { .. }
                | InstData::ReturnCallIndirect { .. }
                | InstData::Return { .. }
                | InstData::Jump { .. }
                | InstData::Brif { .. }
                | InstData::Switch { .. } => true,
                InstData::Binary { op, .. } => matches!(
                    op,
                    BinaryOp::Fadd
                        | BinaryOp::Fsub
                        | BinaryOp::Fmul
                        | BinaryOp::Fdiv
                        | BinaryOp::Fmin
                        | BinaryOp::Fmax
                        | BinaryOp::Fcopysign
                ),
                InstData::Unary { op, .. } => matches!(
                    op,
                    UnaryOp::Fneg
                        | UnaryOp::Fabs
                        | UnaryOp::Fsqrt
                        | UnaryOp::Fnearest
                        | UnaryOp::Ffloor
                        | UnaryOp::Fceil
                        | UnaryOp::Ftrunc
                ),
                InstData::Convert { op, .. } => matches!(
                    op,
                    ConvertOp::Fpext
                        | ConvertOp::Fptrunc
                        | ConvertOp::Sitofp
                        | ConvertOp::Uitofp
                        | ConvertOp::Fptosi
                        | ConvertOp::Fptoui
                ),
                _ => false,
            };
            if !supported {
                return Err(CodegenError {
                    func: f.name.clone(),
                    message: format!("f80 operation not implemented yet: {}", f.display_inst(i)),
                });
            }
        }
    }
    Ok(())
}

/// Stack-only sparse plans retain their complete dataflow. Expanding every old
/// spill unit gives each original slot enough capacity for an extended payload,
/// without overlapping its old neighboring slot. All sparse locations follow
/// the same mapping; dedicated register fallbacks are only safe without sparse
/// F80 transitions whose register ownership cannot be recovered from transfers.
pub(super) fn stack_homes(
    input: &crate::regalloc::AllocationInput<'_>,
    a: &mut Allocation,
) -> Result<HashSet<u32>, String> {
    let f = input.func;
    let extended = |part: crate::regalloc::ValuePart| f.value_type(part.value) == Type::F80;
    let values: Vec<_> = (0..f.num_values())
        .map(Value::new)
        .filter(|&v| f.value_type(v) == Type::F80)
        .collect();
    let mut homes = HashSet::default();
    if values.is_empty() {
        return Ok(homes);
    }
    a.validate(input)?;
    if !a.edits.is_empty() {
        let mut unsupported = false;
        let mut register_fallback = false;
        for &v in &values {
            match a.loc(v) {
                Some(Loc::Stack(slot)) => {
                    homes.insert(slot);
                }
                Some(_) => register_fallback = true,
                None => {}
            }
        }
        a.edits.for_each_binding(|part, loc| {
            if extended(part) {
                match (part.high, a.loc(part.value), loc) {
                    (false, Some(Loc::Stack(_)), Loc::Stack(slot)) => {
                        homes.insert(slot);
                    }
                    _ => unsupported = true,
                }
            }
        });
        if unsupported {
            return Err("f80 sparse bindings require stack fallback homes and stack-only low-part locations".into());
        }
        let mut transfers = Vec::new();
        a.edits.for_each_transfer(|pair| transfers.push(pair));
        if register_fallback && !transfers.is_empty() {
            return Err("f80 register fallbacks with sparse transfers are unsupported; provide stack fallback homes".into());
        }
        // Transfers have no value/type tag. Propagate the complete-payload
        // width through every connected stack location, including temporaries.
        loop {
            let mut changed = false;
            for &(dst, src) in &transfers {
                let contains = |loc| matches!(loc, Loc::Stack(s) if homes.contains(&s));
                if contains(dst) || contains(src) {
                    let (Loc::Stack(dst), Loc::Stack(src)) = (dst, src) else {
                        return Err("f80 sparse transfers require stack-only source and destination locations".into());
                    };
                    changed |= homes.insert(dst);
                    changed |= homes.insert(src);
                }
            }
            if !changed {
                break;
            }
        }
        let wide_conflict = |part: crate::regalloc::ValuePart, loc| {
            f.value_type(part.value).is_vector()
                && f.value_type(part.value).bytes().unwrap() > 16
                && matches!(loc, Loc::Stack(slot) if homes.contains(&slot))
        };
        let mut conflict = false;
        for i in 0..f.num_values() {
            if let Some(loc) = a.loc(Value::new(i)) {
                conflict |= wide_conflict(crate::regalloc::ValuePart::low(Value::new(i)), loc);
            }
        }
        a.edits
            .for_each_binding(|part, loc| conflict |= wide_conflict(part, loc));
        if conflict {
            return Err(
                "f80 sparse transfer homes shared with wide vectors are unsupported".into(),
            );
        }
        a.spill_slots = a
            .spill_slots
            .checked_mul(2)
            .ok_or("f80 sparse frame capacity overflow")?;
        let expand = |loc| match loc {
            Loc::Stack(slot) => Loc::Stack(slot * 2),
            other => other,
        };
        for loc in a.locs.iter_mut().flatten().flatten() {
            *loc = expand(*loc);
        }
        a.edits.map_locations(expand);
        homes = homes.into_iter().map(|slot| slot * 2).collect();
    }
    for v in values {
        if let Some(loc) = a.loc(v) {
            if !a.edits.is_empty()
                && let Loc::Stack(slot) = loc
            {
                homes.insert(slot);
                continue;
            }
            let slot = a
                .spill_slots
                .checked_add(1)
                .ok_or("f80 frame capacity overflow")?
                & !1;
            a.spill_slots = slot.checked_add(2).ok_or("f80 frame capacity overflow")?;
            a.locs[0][v.index()] = Some(Loc::Stack(slot));
            homes.insert(slot);
        }
    }
    Ok(homes)
}

/// Sparse allocation overrides must preserve dedicated F80 memory homes too.
pub(super) fn validate_homes(
    input: &crate::regalloc::AllocationInput<'_>,
    a: &Allocation,
) -> Result<(), String> {
    for (_, inst) in input.func.layout_insts() {
        let mut bad = false;
        input.for_each_operand(inst, |operand| {
            if input.func.value_type(operand.part.value) == Type::F80 {
                let loc =
                    a.operand_loc(a.edits.instruction_index(inst), operand.part, operand.kind);
                bad |= loc.is_some_and(|loc| !matches!(loc, Loc::Stack(_)));
            }
        });
        if bad {
            return Err("f80 requires stack homes at every instruction allocation boundary".into());
        }
    }
    for block in input.func.blocks() {
        for &v in input.func.block_params(block) {
            if input.func.value_type(v) == Type::F80
                && a.param_loc(block, crate::regalloc::ValuePart::low(v))
                    .is_some_and(|l| !matches!(l, Loc::Stack(_)))
            {
                return Err("f80 requires stack homes at block boundaries".into());
            }
        }
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn f80_home(&self, value: Value) -> Memory {
        let Loc::Stack(slot) = self.loc(value) else {
            unreachable!("F80 always has a stack home")
        };
        sized(self.slot(slot), 80)
    }
    pub(super) fn f80_copy(&mut self, to: &Memory, from: &Memory) {
        // Read the complete payload before writing; padding is never accessed.
        self.emit(
            Opcode::Mov,
            [asm_arg(r64(S0)), asm_arg(sized(from.clone(), 64))],
        );
        self.emit(
            Opcode::Movzx,
            [asm_arg(r32(S2)), asm_arg(sized(offset_memory(from, 8), 16))],
        );
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(to.clone(), 64)), asm_arg(r64(S0))],
        );
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(offset_memory(to, 8), 16)), asm_arg(r16(S2))],
        );
    }
    pub(super) fn f80_constant(&mut self, result: Option<Value>, significand: u64, sign_exp: u16) {
        let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) else {
            return;
        };
        let to = self.slot(slot);
        self.emit(Opcode::Movabs, [asm_arg(r64(S0)), asm_arg(significand)]);
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(to.clone(), 64)), asm_arg(r64(S0))],
        );
        self.emit(
            Opcode::Mov,
            [
                asm_arg(sized(offset_memory(&to, 8), 16)),
                asm_arg(u32::from(sign_exp)),
            ],
        );
    }
    pub(super) fn f80_push(&mut self, v: Value) {
        self.emit(Opcode::Fld, [asm_arg(self.f80_home(v))]);
    }
    pub(super) fn f80_pop_result(&mut self, result: Option<Value>) {
        if let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) {
            self.emit(Opcode::Fstp, [asm_arg(sized(self.slot(slot), 80))]);
        } else {
            self.emit(Opcode::Fstp, [asm_arg(st(0))]);
        }
    }
    pub(super) fn f80_inst(&mut self, inst: Inst, result: Option<Value>) -> bool {
        match *self.func.inst_data(inst) {
            InstData::Fma { args } if self.ty(args[0]) == Type::F80 => {
                for (i, value) in args.into_iter().enumerate() {
                    self.f80_copy(&memory(r64(4), (i * 16) as i128), &self.f80_home(value));
                }
                self.call_function(Opcode::Call, "fmal");
                self.f80_pop_result(result);
            }
            InstData::Binary {
                op: operation @ (BinaryOp::Fmin | BinaryOp::Fmax | BinaryOp::Fcopysign),
                args,
            } if self.ty(args[0]) == Type::F80 => {
                self.f80_special_binary(inst, operation, args, result);
            }
            InstData::Binary {
                op: operation,
                args,
            } if self.ty(args[0]) == Type::F80 => {
                self.f80_push(args[0]);
                self.f80_push(args[1]);
                let name = match operation {
                    BinaryOp::Fadd => "faddp",
                    BinaryOp::Fsub => "fsubp",
                    BinaryOp::Fmul => "fmulp",
                    BinaryOp::Fdiv => "fdivp",
                    _ => unreachable!("validated F80 binary"),
                };
                self.emit(op(name), [asm_arg(st(1)), asm_arg(st(0))]);
                self.f80_pop_result(result);
            }
            InstData::Unary {
                op: operation @ (UnaryOp::Fneg | UnaryOp::Fabs),
                arg,
            } if self.ty(arg) == Type::F80 => {
                if let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) {
                    let to = self.slot(slot);
                    self.f80_copy(&to, &self.f80_home(arg));
                    self.emit(
                        if operation == UnaryOp::Fneg {
                            Opcode::Xor
                        } else {
                            Opcode::And
                        },
                        [
                            asm_arg(sized(offset_memory(&to, 8), 16)),
                            asm_arg(if operation == UnaryOp::Fneg {
                                0x8000
                            } else {
                                0x7fff
                            }),
                        ],
                    );
                }
            }
            InstData::Unary {
                op:
                    operation @ (UnaryOp::Fnearest | UnaryOp::Ffloor | UnaryOp::Fceil | UnaryOp::Ftrunc),
                arg,
            } if self.ty(arg) == Type::F80 => {
                self.f80_push(arg);
                let mode = match operation {
                    UnaryOp::Fnearest => 0,
                    UnaryOp::Ffloor => 1,
                    UnaryOp::Fceil => 2,
                    UnaryOp::Ftrunc => 3,
                    _ => unreachable!(),
                };
                let saved = self.f80_rounding(mode);
                self.emit(op("frndint"), []);
                self.emit(Opcode::Fldcw, [asm_arg(saved)]);
                self.f80_pop_result(result);
            }
            InstData::Unary { op: operation, arg } if self.ty(arg) == Type::F80 => {
                self.f80_push(arg);
                self.emit(
                    op(match operation {
                        UnaryOp::Fneg => "fchs",
                        UnaryOp::Fabs => "fabs",
                        UnaryOp::Fsqrt => "fsqrt",
                        _ => unreachable!("validated F80 unary"),
                    }),
                    [],
                );
                self.f80_pop_result(result);
            }
            InstData::Load {
                ty: Type::F80,
                addr,
                ..
            } => {
                let to = result
                    .and_then(|v| self.defined(v))
                    .unwrap_or(Loc::Stack(self.copy_temp.expect("F80 temporary")));
                let Loc::Stack(slot) = to else { unreachable!() };
                let from = self.mem(addr, S1, Type::F80);
                self.f80_copy(&self.slot(slot), &from);
            }
            InstData::VaArg {
                ty: Type::F80,
                list,
                ..
            } => {
                let p = self.use_reg(list, S2);
                // overflow_arg_area is the only source, with a 16-byte stride.
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(S1)), asm_arg(sized(memory(r64(p), 8), 64))],
                );
                self.emit(Opcode::Add, [asm_arg(r64(S1)), asm_arg(15)]);
                self.emit(Opcode::And, [asm_arg(r64(S1)), asm_arg(-16)]);
                self.emit(
                    Opcode::Lea,
                    [asm_arg(r64(S0)), asm_arg(memory(r64(S1), 16))],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(sized(memory(r64(p), 8), 64)), asm_arg(r64(S0))],
                );
                let to = result
                    .and_then(|v| self.defined(v))
                    .unwrap_or(Loc::Stack(self.copy_temp.expect("F80 temporary")));
                let Loc::Stack(slot) = to else { unreachable!() };
                self.f80_copy(&self.slot(slot), &memory(r64(S1), 0));
            }
            InstData::Store { value, addr, .. } if self.ty(value) == Type::F80 => {
                let to = self.mem(addr, S1, Type::F80);
                self.f80_copy(&to, &self.f80_home(value));
            }
            InstData::Select { args } if self.ty(args[1]) == Type::F80 => {
                let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) else {
                    return true;
                };
                let alternate = format!(".L{}_{}_f80_false", self.func.name, inst);
                let done = format!(".L{}_{}_f80_select", self.func.name, inst);
                self.test_value(args[0]);
                self.emit(Opcode::Jcc(4), [asm_arg(&alternate)]);
                self.f80_copy(&self.slot(slot), &self.f80_home(args[1]));
                self.emit(Opcode::Jmp, [asm_arg(&done)]);
                self.out.label(alternate);
                self.f80_copy(&self.slot(slot), &self.f80_home(args[2]));
                self.out.label(done);
            }
            InstData::Convert {
                op: operation @ (ConvertOp::Sitofp | ConvertOp::Uitofp),
                ty: Type::F80,
                arg,
            } => {
                self.f80_from_int(inst, operation == ConvertOp::Sitofp, arg, result);
            }
            InstData::Convert {
                op: operation @ (ConvertOp::Fptosi | ConvertOp::Fptoui),
                ty,
                arg,
            } if self.ty(arg) == Type::F80 => {
                self.f80_to_int(inst, operation == ConvertOp::Fptosi, ty, arg, result);
            }
            InstData::Convert { ty, arg, .. } if ty == Type::F80 || self.ty(arg) == Type::F80 => {
                let temporary = self.slot(self.copy_temp.expect("F80 staging"));
                if ty == Type::F80 {
                    let from = self.ty(arg);
                    let r = self.use_freg(arg, FS0);
                    self.emit(
                        movs(from),
                        [
                            asm_arg(sized(temporary.clone(), ptr(from))),
                            asm_arg(simd(r, from)),
                        ],
                    );
                    self.emit(Opcode::Fld, [asm_arg(sized(temporary, ptr(from)))]);
                    self.f80_pop_result(result);
                } else {
                    self.f80_push(arg);
                    self.emit(Opcode::Fstp, [asm_arg(sized(temporary.clone(), ptr(ty)))]);
                    let d = self.def_freg(result);
                    self.emit(
                        movs(ty),
                        [asm_arg(simd(d, ty)), asm_arg(sized(temporary, ptr(ty)))],
                    );
                    self.def_done(result, d);
                }
            }
            _ => return false,
        }
        true
    }
    pub(super) fn f80_compare(&mut self, cond: FloatCC, args: [Value; 2]) -> &'static str {
        let (a, b, cc) = match cond {
            FloatCC::Lt => (args[1], args[0], "a"),
            FloatCC::Le => (args[1], args[0], "ae"),
            FloatCC::Gt => (args[0], args[1], "a"),
            FloatCC::Ge => (args[0], args[1], "ae"),
            FloatCC::Ord => (args[0], args[1], "np"),
            FloatCC::Uno => (args[0], args[1], "p"),
            FloatCC::Eq | FloatCC::Ne => (args[0], args[1], "ne"),
        };
        self.f80_push(b);
        self.f80_push(a);
        self.emit(Opcode::Fucomip, [asm_arg(st(0)), asm_arg(st(1))]);
        self.emit(Opcode::Fstp, [asm_arg(st(0))]);
        if matches!(cond, FloatCC::Eq | FloatCC::Ne) {
            if self.func.resolve(args[0]) == self.func.resolve(args[1]) {
                return if cond == FloatCC::Eq { "np" } else { "p" };
            }
            self.emit(
                Opcode::Setcc(if cond == FloatCC::Eq { 4 } else { 5 }),
                [asm_arg(r8(S0))],
            );
            self.emit(
                Opcode::Setcc(if cond == FloatCC::Eq { 11 } else { 10 }),
                [asm_arg(r8(S1))],
            );
            self.emit(
                if cond == FloatCC::Eq {
                    Opcode::And
                } else {
                    Opcode::Or
                },
                [asm_arg(r8(S0)), asm_arg(r8(S1))],
            );
        }
        cc
    }
}

impl FunctionCompiler<'_> {
    fn f80_temporary(&self) -> Memory {
        self.slot(self.copy_temp.expect("F80 temporary"))
    }
    /// An exact power of two, optionally negative, as an 80-bit payload.
    fn f80_power(&mut self, exponent: u16, negative: bool) {
        let tmp = self.f80_temporary();
        self.emit(Opcode::Movabs, [asm_arg(r64(S0)), asm_arg(1u64 << 63)]);
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(tmp.clone(), 64)), asm_arg(r64(S0))],
        );
        self.emit(
            Opcode::Mov,
            [
                asm_arg(sized(offset_memory(&tmp, 8), 16)),
                asm_arg(u32::from(16383 + exponent) | if negative { 0x8000 } else { 0 }),
            ],
        );
        self.emit(Opcode::Fld, [asm_arg(sized(tmp, 80))]);
    }
    fn f80_cmp_power(&mut self, value: Value, exponent: u16, negative: bool) {
        self.f80_power(exponent, negative);
        self.f80_push(value);
        self.emit(Opcode::Fucomip, [asm_arg(st(0)), asm_arg(st(1))]);
        self.emit(Opcode::Fstp, [asm_arg(st(0))]);
    }
    fn f80_from_int(&mut self, inst: Inst, signed: bool, value: Value, result: Option<Value>) {
        let from = self.ty(value);
        let tmp = self.f80_temporary();
        let source = self.use_reg(value, S0);
        if !signed && from == Type::I64 {
            // Every u64 is exactly representable. Normalize the integer payload
            // directly, including values above i64::MAX, without FP rounding.
            let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) else {
                return;
            };
            let to = self.slot(slot);
            let zero = format!(".L{}_{}_u64_f80_zero", self.func.name, inst);
            let done = format!(".L{}_{}_u64_f80_done", self.func.name, inst);
            self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(source))]);
            self.emit(Opcode::Test, [asm_arg(r64(S0)), asm_arg(r64(S0))]);
            self.emit(Opcode::Jcc(4), [asm_arg(&zero)]);
            self.emit(Opcode::Bsr, [asm_arg(r64(S2)), asm_arg(r64(S0))]);
            self.emit(Opcode::Mov, [asm_arg(r32(S1)), asm_arg(63)]);
            self.emit(Opcode::Sub, [asm_arg(r32(S1)), asm_arg(r32(S2))]);
            self.emit(Opcode::Shl, [asm_arg(r64(S0)), asm_arg(r8(S1))]);
            self.emit(Opcode::Add, [asm_arg(r32(S2)), asm_arg(16383)]);
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(to.clone(), 64)), asm_arg(r64(S0))],
            );
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(offset_memory(&to, 8), 16)), asm_arg(r16(S2))],
            );
            self.emit(Opcode::Jmp, [asm_arg(&done)]);
            self.out.label(zero);
            self.f80_constant(result, 0, 0);
            self.out.label(done);
            return;
        }
        let source = self.extend(S0, source, from, signed);
        let width = if signed && from == Type::I32 { 32 } else { 64 };
        if from != Type::I64 && !(signed && from == Type::I32) {
            self.emit(
                if signed { Opcode::Movsxd } else { Opcode::Mov },
                [
                    asm_arg(if signed { r64(S0) } else { r32(S0) }),
                    asm_arg(r32(source)),
                ],
            );
            self.emit(
                Opcode::Mov,
                [asm_arg(sized(tmp.clone(), 64)), asm_arg(r64(S0))],
            );
        } else {
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(tmp.clone(), width)),
                    asm_arg(if width == 32 {
                        r32(source)
                    } else {
                        r64(source)
                    }),
                ],
            );
        }
        self.emit(Opcode::Fild, [asm_arg(sized(tmp, width))]);
        self.f80_pop_result(result);
    }
    /// Round toward zero independently of inherited RC; restore the exact CW.
    /// This baseline-v1 sequence deliberately does not require SSE3 FISTTP.
    fn f80_truncate_st0(&mut self, width: u16) {
        let tmp = self.f80_temporary();
        let saved = self.f80_rounding(3);
        self.emit(Opcode::Fistp, [asm_arg(sized(tmp.clone(), width))]);
        self.emit(Opcode::Fldcw, [asm_arg(saved)]);
        self.emit(
            Opcode::Mov,
            [
                asm_arg(if width == 32 { r32(S0) } else { r64(S0) }),
                asm_arg(sized(tmp, width)),
            ],
        );
    }
    /// Match existing IR conversions: saturation, truncation, NaN -> zero.
    fn f80_to_int(
        &mut self,
        inst: Inst,
        signed: bool,
        to: Type,
        value: Value,
        result: Option<Value>,
    ) {
        let label = |part: &str| format!(".L{}_{}_f80_int_{part}", self.func.name, inst);
        let (zero, lower, upper, large, done) = (
            label("zero"),
            label("lower"),
            label("upper"),
            label("large"),
            label("done"),
        );
        let bits = if to == Type::I64 { 64 } else { 32 };
        self.f80_compare(FloatCC::Uno, [value, value]);
        self.emit(Opcode::Jcc(10), [asm_arg(&zero)]);
        self.f80_cmp_power(value, bits - u16::from(signed), false);
        self.emit(Opcode::Jcc(3), [asm_arg(&upper)]);
        if signed {
            self.f80_cmp_power(value, bits - 1, true);
            self.emit(Opcode::Jcc(6), [asm_arg(&lower)]);
        } else {
            self.emit(Opcode::Fldz, []);
            self.f80_push(value);
            self.emit(Opcode::Fucomip, [asm_arg(st(0)), asm_arg(st(1))]);
            self.emit(Opcode::Fstp, [asm_arg(st(0))]);
            self.emit(Opcode::Jcc(6), [asm_arg(&zero)]);
        }
        if !signed && bits == 64 {
            self.f80_cmp_power(value, 63, false);
            self.emit(Opcode::Jcc(3), [asm_arg(&large)]);
        }
        self.f80_push(value);
        self.f80_truncate_st0(if signed { bits } else { 64 });
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        if !signed && bits == 64 {
            self.out.label(large);
            // In [2^63,2^64) every F80 value is integral: its explicit
            // significand already is the u64 result, independent of PC/RC.
            self.emit(
                Opcode::Mov,
                [asm_arg(r64(S0)), asm_arg(sized(self.f80_home(value), 64))],
            );
            self.emit(Opcode::Jmp, [asm_arg(&done)]);
        }
        self.out.label(upper);
        self.emit(
            Opcode::Movabs,
            [
                asm_arg(r64(S0)),
                asm_arg(if signed {
                    (1u64 << (bits - 1)) - 1
                } else if bits == 64 {
                    u64::MAX
                } else {
                    u64::from(u32::MAX)
                }),
            ],
        );
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        if signed {
            self.out.label(lower);
            self.emit(
                Opcode::Movabs,
                [asm_arg(r64(S0)), asm_arg(1u64 << (bits - 1))],
            );
            self.emit(Opcode::Jmp, [asm_arg(&done)]);
        }
        self.out.label(zero);
        self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(0)]);
        self.out.label(done);
        let d = self.def_reg(result);
        self.emit(
            Opcode::Mov,
            [
                asm_arg(if bits == 32 { r32(d) } else { r64(d) }),
                asm_arg(if bits == 32 { r32(S0) } else { r64(S0) }),
            ],
        );
        self.def_done(result, d);
    }
}

impl FunctionCompiler<'_> {
    fn f80_rounding(&mut self, mode: u32) -> Memory {
        let tmp = self.f80_temporary();
        let saved = sized(offset_memory(&tmp, 8), 16);
        let changed = sized(offset_memory(&tmp, 10), 16);
        self.emit(Opcode::Fnstcw, [asm_arg(saved.clone())]);
        self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(saved.clone())]);
        self.emit(Opcode::And, [asm_arg(r32(S0)), asm_arg(0xf3ff)]);
        self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(mode << 10)]);
        self.emit(Opcode::Mov, [asm_arg(changed.clone()), asm_arg(r16(S0))]);
        self.emit(Opcode::Fldcw, [asm_arg(changed)]);
        saved
    }
    fn f80_special_binary(
        &mut self,
        inst: Inst,
        operation: BinaryOp,
        args: [Value; 2],
        result: Option<Value>,
    ) {
        let Some(Loc::Stack(slot)) = result.and_then(|v| self.defined(v)) else {
            return;
        };
        let to = self.slot(slot);
        let second_overlaps = self.loc(args[1]) == Loc::Stack(slot);
        if operation == BinaryOp::Fcopysign {
            // A dying input may share the result home. RCX survives the
            // payload copy's RAX/RDX scratch writes.
            let sign = if second_overlaps { S1 } else { S0 };
            if second_overlaps {
                self.emit(
                    Opcode::Movzx,
                    [
                        asm_arg(r32(sign)),
                        asm_arg(sized(offset_memory(&self.f80_home(args[1]), 8), 16)),
                    ],
                );
            }
            self.f80_copy(&to, &self.f80_home(args[0]));
            if !second_overlaps {
                self.emit(
                    Opcode::Movzx,
                    [
                        asm_arg(r32(sign)),
                        asm_arg(sized(offset_memory(&self.f80_home(args[1]), 8), 16)),
                    ],
                );
            }
            self.emit(Opcode::And, [asm_arg(r32(sign)), asm_arg(0x8000)]);
            self.emit(
                Opcode::And,
                [asm_arg(sized(offset_memory(&to, 8), 16)), asm_arg(0x7fff)],
            );
            self.emit(
                Opcode::Or,
                [
                    asm_arg(sized(offset_memory(&to, 8), 16)),
                    asm_arg(r16(sign)),
                ],
            );
            return;
        }
        let label = |part: &str| format!(".L{}_{}_f80_minmax_{part}", self.func.name, inst);
        let (nan, equal, first, done) =
            (label("nan"), label("equal"), label("first"), label("done"));
        self.f80_push(args[1]);
        self.f80_push(args[0]);
        self.emit(Opcode::Fucomip, [asm_arg(st(0)), asm_arg(st(1))]);
        self.emit(Opcode::Fstp, [asm_arg(st(0))]);
        self.emit(Opcode::Jcc(10), [asm_arg(&nan)]);
        self.emit(Opcode::Jcc(4), [asm_arg(&equal)]);
        self.emit(
            Opcode::Jcc(if operation == BinaryOp::Fmin { 2 } else { 7 }),
            [asm_arg(&first)],
        );
        self.f80_copy(&to, &self.f80_home(args[1]));
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(equal);
        let sign = if second_overlaps { S1 } else { S0 };
        if second_overlaps {
            self.emit(
                Opcode::Movzx,
                [
                    asm_arg(r32(sign)),
                    asm_arg(sized(offset_memory(&self.f80_home(args[1]), 8), 16)),
                ],
            );
        }
        self.f80_copy(&to, &self.f80_home(args[0]));
        if !second_overlaps {
            self.emit(
                Opcode::Movzx,
                [
                    asm_arg(r32(sign)),
                    asm_arg(sized(offset_memory(&self.f80_home(args[1]), 8), 16)),
                ],
            );
        }
        self.emit(
            if operation == BinaryOp::Fmin {
                Opcode::Or
            } else {
                Opcode::And
            },
            [
                asm_arg(sized(offset_memory(&to, 8), 16)),
                asm_arg(r16(sign)),
            ],
        );
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(first);
        self.f80_copy(&to, &self.f80_home(args[0]));
        self.emit(Opcode::Jmp, [asm_arg(&done)]);
        self.out.label(nan);
        self.f80_push(args[0]);
        self.f80_push(args[1]);
        self.emit(op("faddp"), [asm_arg(st(1)), asm_arg(st(0))]);
        self.f80_pop_result(result);
        self.out.label(done);
    }
}
