//! GNU inline assembly. Stage inputs/results and preserve GPR/SIMD homes so
//! the operand assignment is independent of allocator choices and scratch regs.
use super::*;
use keel_ir::{AsmArch, AsmClobber, AsmOperand as Constraint, InlineAsm, PhysReg};

#[derive(Clone, Copy)]
struct Home {
    offset: usize,
    bits: u16,
}

pub(super) struct Plan {
    regs: Vec<Option<PhysReg>>,
    flags: Vec<(usize, u8)>,
    initializers: Vec<(PhysReg, usize)>,
    text: String,
    /// Registers overwritten by operand staging or the template, in slot order.
    pub saved_regs: Vec<u8>,
    pub bytes: usize,
    saved_simd: Vec<u8>,
    simd_home: usize,
    simd_bits: u16,
    inputs: Vec<Home>,
    outputs: Vec<Home>,
}

fn available(r: u8, frame: Option<u8>) -> bool {
    r < 16 && r != 4 && r != 5 && frame != Some(r)
}

pub(super) fn plan(
    module: &Module,
    func: &Function,
    asm: &InlineAsm,
    args: &[Value],
    frame: Option<u8>,
    unique: u64,
    wide_vectors: bool,
) -> Result<Plan, String> {
    for (&constraint, &arg) in asm.inputs.iter().zip(args) {
        if constraint == Constraint::Imm
            && let Some((symbol, _)) = keel_ir::symbolic_immediate(func, arg)
            && module
                .data_object(func.symbol_name(symbol))
                .is_some_and(|d| d.thread_local)
        {
            return Err(
                "TLS symbols require a relocation contract not supported by asm immediates".into(),
            );
        }
    }
    let n = asm.outputs.len();
    let mut early = vec![false; n];
    for &out in &asm.early_clobbers {
        if out as usize >= n || early[out as usize] {
            return Err("invalid or duplicate asm early-clobber output".into());
        }
        early[out as usize] = true;
    }
    if args.len() != asm.inputs.len() {
        return Err("asm input arity mismatch".into());
    }
    let stack_input = asm.stack_pointer_input()?;
    if let Some(i) = stack_input
        && !keel_ir::is_stack_pointer_snapshot(func, args[i])
    {
        return Err("reserved-stack asm requires an actual stack-pointer snapshot".into());
    }
    let mut ops: Vec<_> = asm
        .outputs
        .iter()
        .map(|(o, _)| *o)
        .chain(asm.inputs.iter().copied())
        .collect();
    for (op, &arg) in ops.iter_mut().skip(n).zip(args) {
        if matches!(
            *op,
            Constraint::RegImm
                | Constraint::RegSignedImm32
                | Constraint::RegShiftImm5
                | Constraint::PortImm8
        ) {
            let symbolic = keel_ir::symbolic_immediate(func, arg).is_some_and(|(symbol, _)| {
                !module
                    .data_object(func.symbol_name(symbol))
                    .is_some_and(|d| d.thread_local)
            });
            let numeric = iconst(func, arg);
            let immediate = match *op {
                Constraint::RegImm => numeric.is_some() || symbolic,
                Constraint::RegSignedImm32 => numeric.is_some_and(|n| i32::try_from(n).is_ok()),
                Constraint::RegShiftImm5 => numeric.is_some_and(|n| (0..=31).contains(&n)),
                Constraint::PortImm8 => numeric.is_some_and(|n| (0..=255).contains(&n)),
                _ => unreachable!(),
            };
            // No code-model/range guarantee exists for symbolic e operands.
            // Preserve their value through the register alternative instead.
            *op = if immediate {
                Constraint::Imm
            } else if *op == Constraint::PortImm8 {
                Constraint::Fixed(keel_ir::PhysReg {
                    arch: AsmArch::X86,
                    float: false,
                    num: 2,
                })
            } else {
                Constraint::Reg
            };
        }
    }
    let mut taken = [[false; 16]; 2];
    for c in &asm.clobbers {
        if let AsmClobber::Reg(r) = c {
            if r.arch != AsmArch::X86
                || if r.float {
                    r.num >= 16
                } else {
                    !available(r.num, frame)
                }
            {
                return Err(format!("unsupported or reserved x86 asm clobber {r}"));
            }
            taken[usize::from(r.float)][r.num as usize] = true;
        }
    }
    let mut regs = vec![None; ops.len()];
    for (i, op) in ops.iter().enumerate() {
        let ty = if i < n {
            asm.outputs[i].1
        } else {
            func.value_type(func.resolve(args[i - n]))
        };
        let simd = matches!(ty, Type::F32 | Type::F64) || ty.is_vector();
        if !((ty.is_int() && ty != Type::I128) || ty == Type::Ptr || simd) {
            return Err(format!("unsupported x86 asm operand type {ty}"));
        }
        if ty.is_vector() && !matches!(ty.bytes(), Some(8 | 16 | 32)) {
            return Err("x86 asm supports only 64/128/256-bit vectors".into());
        }
        match *op {
            Constraint::Reg if !simd => {}
            Constraint::FloatReg if simd => {}
            Constraint::StackPointer if i >= n && ty == Type::I64 => {}
            Constraint::Flag(cc) if i < n && cc < 16 && ty.is_int() && !early[i] => {}
            Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
                if i >= n && ty == Type::Ptr => {}
            Constraint::Fixed(r) => {
                if r.arch != AsmArch::X86
                    || r.float != simd
                    || if r.float {
                        r.num >= 16
                    } else {
                        !available(r.num, frame)
                    }
                {
                    return Err(format!("unsupported or reserved x86 asm operand {r}"));
                }
                // Outputs have overlapping write lifetimes. Fixed inputs may
                // share a non-early output, but never a clobber or early output.
                if taken[usize::from(r.float)][r.num as usize]
                    && (i < n
                        || asm
                            .clobbers
                            .iter()
                            .any(|c| matches!(c, AsmClobber::Reg(clobber) if *clobber == r))
                        || (0..n).any(|out| regs[out] == Some(r) && early[out]))
                {
                    return Err(format!(
                        "asm operand {i}: register {r} overlaps another output, early-clobber or clobber"
                    ));
                }
                taken[usize::from(r.float)][r.num as usize] = true;
                regs[i] = Some(r);
            }
            Constraint::Imm | Constraint::NumericImm
                if i >= n
                    && (iconst(func, args[i - n]).is_some()
                        || (*op == Constraint::Imm
                            && keel_ir::symbolic_immediate(func, args[i - n]).is_some())) => {}
            Constraint::Tied(out)
                if i >= n
                    && (out as usize) < n
                    && !matches!(asm.outputs[out as usize].0, Constraint::Flag(_)) => {}
            _ => {
                return Err(format!(
                    "unsupported x86 asm constraint `{op}` for operand {i}"
                ));
            }
        }
    }
    for (i, op) in ops.iter().enumerate() {
        let float = *op == Constraint::FloatReg;
        if float
            || matches!(
                op,
                Constraint::Reg | Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
            )
        {
            let r = (0..16)
                .find(|&r| (float || available(r, frame)) && !taken[usize::from(float)][r as usize])
                .ok_or("too many x86 asm operands for the free registers")?;
            taken[usize::from(float)][r as usize] = true;
            regs[i] = Some(PhysReg {
                arch: AsmArch::X86,
                float,
                num: r,
            });
        }
    }
    for (i, op) in ops.iter().enumerate().skip(n) {
        if let Constraint::Tied(out) = *op {
            let out = out as usize;
            regs[i] = Some(regs[out].ok_or("asm tie requires a register output")?);
        }
    }
    // Each physical register gets exactly one initialization. Compare with
    // the widest input so no narrower reload can destroy required upper bits.
    let mut initializers: Vec<(PhysReg, usize)> = Vec::new();
    for (i, &arg) in args.iter().enumerate() {
        let Some(r) = regs[n + i] else { continue };
        if let Some((_, previous)) = initializers.iter_mut().find(|(reg, _)| *reg == r) {
            if !keel_ir::asm::same_register_input(func, args[*previous], arg) {
                return Err(format!(
                    "asm inputs {previous} and {i} require conflicting or unproven values in {r}"
                ));
            }
            if func.value_type(func.resolve(arg)).bytes().unwrap_or(8)
                > func
                    .value_type(func.resolve(args[*previous]))
                    .bytes()
                    .unwrap_or(8)
            {
                *previous = i;
            }
        } else {
            initializers.push((r, i));
        }
    }
    let text = substitute(func, asm, args, &regs, unique)?;
    // Staging uses RAX, while operand initialization and template outputs can
    // overwrite their assigned homes. A memory/flags clobber changes no GPR.
    let mut overwritten = [[false; 16]; 2];
    for c in &asm.clobbers {
        if let AsmClobber::Reg(r) = c {
            overwritten[usize::from(r.float)][r.num as usize] = true;
        }
    }
    for &r in regs.iter().flatten() {
        overwritten[usize::from(r.float)][r.num as usize] = true;
    }
    if regs.iter().any(Option::is_some) || n != 0 {
        overwritten[0][S0 as usize] = true;
    }
    if regs.iter().flatten().any(|r| r.float) {
        overwritten[1][FS0 as usize] = true;
    }
    let saved_regs: Vec<_> = (0..16).filter(|&r| overwritten[0][r as usize]).collect();
    let saved_simd: Vec<_> = (0..16).filter(|&r| overwritten[1][r as usize]).collect();
    let simd_bits = if wide_vectors { 256 } else { 128 };
    let simd_home = saved_regs.len() * 8;
    let mut bytes = simd_home + saved_simd.len() * usize::from(simd_bits / 8);
    let mut home = |ty: Type| {
        let bits = ty.bytes().unwrap_or(8).max(8) as u16 * 8;
        let offset = bytes;
        bytes += usize::from(bits / 8);
        Home { offset, bits }
    };
    let inputs = args
        .iter()
        .map(|&arg| home(func.value_type(func.resolve(arg))))
        .collect();
    let outputs = asm.outputs.iter().map(|&(_, ty)| home(ty)).collect();
    if saved_regs.is_empty() && saved_simd.is_empty() && n == 0 {
        bytes = 0;
    }
    // The shared object assembler owns instruction/directive validation and
    // feature checks. Parsing fragments here would bypass GNU preprocessing
    // (numeric local labels and macros in particular).
    Ok(Plan {
        regs,
        initializers,
        flags: asm
            .outputs
            .iter()
            .enumerate()
            .filter_map(|(i, (op, _))| match op {
                Constraint::Flag(cc) => Some((i, *cc)),
                _ => None,
            })
            .collect(),
        text,
        saved_regs,
        saved_simd,
        simd_bits,
        simd_home,
        inputs,
        outputs,
        bytes,
    })
}

fn substitute(
    func: &Function,
    asm: &InlineAsm,
    args: &[Value],
    regs: &[Option<PhysReg>],
    unique: u64,
) -> Result<String, String> {
    let n = asm.outputs.len();
    let mut text = String::new();
    let mut chars = asm.template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            text.push(c);
            continue;
        }
        if chars.next_if_eq(&'%').is_some() {
            text.push('%');
            continue;
        }
        if chars.next_if_eq(&'=').is_some() {
            write!(text, "{unique}").unwrap();
            continue;
        }
        let modifier = chars.next_if(|c| c.is_ascii_alphabetic());
        let delimited = chars.next_if_eq(&'[').is_some();
        let digits: String = std::iter::from_fn(|| chars.next_if(char::is_ascii_digit)).collect();
        if delimited && chars.next() != Some(']') {
            return Err("unclosed asm operand reference".into());
        }
        let i: usize = digits
            .parse()
            .map_err(|_| "asm % requires an operand number, %% or %=")?;
        if i >= regs.len() {
            return Err("asm operand reference out of range".into());
        }
        if i < n && matches!(asm.outputs[i].0, Constraint::Flag(_)) {
            return Err("flag outputs cannot be referenced in an asm template".into());
        }
        if i >= n && asm.inputs[i - n] == Constraint::StackPointer {
            if !matches!(modifier, None | Some('q')) {
                return Err("reserved-stack asm supports only the 64-bit RSP view".into());
            }
            // The input value establishes a dependency, not a value to load
            // into RSP. Printing the physical register is safe only while the
            // template preserves the enclosing frame bases and restores RSP.
            text.push_str("%rsp");
            continue;
        }
        if let Some(r) = regs[i] {
            if i >= n
                && matches!(
                    asm.inputs[i - n],
                    Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
                )
            {
                if modifier.is_some() {
                    return Err("unsupported x86 asm memory modifier".into());
                }
                write!(text, "(%{})", r64(r.num)).unwrap();
                continue;
            }
            let ty = if i < n {
                asm.outputs[i].1
            } else {
                func.value_type(func.resolve(args[i - n]))
            };
            if r.float {
                let bits = match modifier {
                    None => {
                        if ty.bytes() == Some(32) {
                            256
                        } else {
                            128
                        }
                    }
                    Some('x') => 128,
                    Some('t') => 256,
                    _ => return Err("unsupported x86 asm SIMD register modifier".into()),
                };
                if bits == 256 && ty.bytes() != Some(32) {
                    return Err("asm YMM modifier requires a 256-bit operand".into());
                }
                write!(text, "%{}", simd_width(r.num, bits)).unwrap();
                continue;
            }
            let width = match modifier {
                None => {
                    if ty == Type::Ptr {
                        64
                    } else {
                        ty.bytes().ok_or("unsized asm register")? * 8
                    }
                }
                Some('b') => 8,
                Some('w') => 16,
                Some('k') => 32,
                Some('q') => 64,
                Some('a') => match ty {
                    Type::Ptr | Type::I64 => 64,
                    Type::I32 => 32,
                    _ => {
                        return Err(
                            "x86 asm address modifier requires a 32- or 64-bit register".into()
                        );
                    }
                },
                _ => return Err("unsupported x86 asm register modifier".into()),
            };
            if modifier == Some('a') {
                text.push('(');
            }
            write!(
                text,
                "%{}",
                Register {
                    class: RegisterClass::Gpr,
                    number: r.num,
                    width: width as u16,
                    high_byte: false
                }
            )
            .unwrap();
            if modifier == Some('a') {
                text.push(')');
            }
        } else {
            if i < n
                || !matches!(
                    modifier,
                    None | Some('c' | 'a' | 'n' | 'b' | 'w' | 'k' | 'q')
                )
            {
                return Err("unsupported x86 asm immediate modifier".into());
            }
            let arg = args[i - n];
            if matches!(modifier, None | Some('b' | 'w' | 'k' | 'q')) {
                text.push('$');
            }
            if let Some(value) = iconst(func, arg) {
                // GNU negates in the host-wide integer domain, including
                // wrapping INT64_MIN back to itself.
                let value = if modifier == Some('n') {
                    value.wrapping_neg()
                } else {
                    value
                };
                write!(text, "{value}").unwrap();
            } else if let Some((symbol, addend)) = keel_ir::symbolic_immediate(func, arg) {
                if modifier == Some('n') {
                    return Err("x86 asm negation modifier requires a numeric constant".into());
                }
                text.push_str(func.symbol_name(symbol));
                if addend > 0 {
                    write!(text, "+{addend}").unwrap();
                }
                if addend < 0 {
                    write!(text, "{addend}").unwrap();
                }
                if modifier == Some('a') {
                    text.push_str("(%rip)");
                }
            } else {
                return Err("asm immediate requires a constant".into());
            }
        }
    }
    Ok(text)
}

impl FunctionCompiler<'_> {
    pub(super) fn native_inline_asm(&mut self, inst: Inst, args: &[Value]) {
        let plan = self
            .inline_asm_plans
            .remove(&inst)
            .expect("asm checked before emission");
        if plan.bytes == 0 {
            self.out.inline_assembly(&plan.text);
            return;
        }
        let home = self.inline_asm_home.expect("asm frame prepared");
        let mem = |this: &Self, slot: Home| sized(this.frame(home + slot.offset), slot.bits);
        for (slot, &r) in plan.saved_regs.iter().enumerate() {
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(self.frame(home + 8 * slot), 64)),
                    asm_arg(r64(r)),
                ],
            );
        }
        for (slot, &r) in plan.saved_simd.iter().enumerate() {
            let at = home + plan.simd_home + slot * usize::from(plan.simd_bits / 8);
            self.emit(
                move_bits(plan.simd_bits),
                [
                    asm_arg(sized(self.frame(at), plan.simd_bits)),
                    asm_arg(simd_width(r, plan.simd_bits)),
                ],
            );
        }
        let n = plan.outputs.len();
        for (i, &arg) in args.iter().enumerate() {
            let Some(reg) = plan.regs[n + i] else {
                continue;
            };
            let dst = mem(self, plan.inputs[i]);
            if reg.float {
                // Sources are snapshotted before any asm register is initialized.
                let bits = plan.inputs[i].bits;
                match self.loc(arg) {
                    Loc::FReg(r) => self.emit(
                        move_bits(bits),
                        [asm_arg(dst), asm_arg(simd_width(r, bits))],
                    ),
                    Loc::Stack(slot) => {
                        self.emit(
                            move_bits(bits),
                            [
                                asm_arg(simd_width(FS0, bits)),
                                asm_arg(sized(self.slot(slot), bits)),
                            ],
                        );
                        self.emit(
                            move_bits(bits),
                            [asm_arg(dst), asm_arg(simd_width(FS0, bits))],
                        );
                    }
                    _ => unreachable!("SIMD asm input has a SIMD location"),
                }
            } else {
                match self.loc(arg) {
                    Loc::Reg(r) => {
                        if let Ok(slot) = plan.saved_regs.binary_search(&r) {
                            self.emit(
                                Opcode::Mov,
                                [
                                    asm_arg(r64(S0)),
                                    asm_arg(sized(self.frame(home + 8 * slot), 64)),
                                ],
                            );
                        } else {
                            self.emit_move(Loc::Reg(S0), Loc::Reg(r));
                        }
                    }
                    src => self.emit_move(Loc::Reg(S0), src),
                }
                self.emit(Opcode::Mov, [asm_arg(dst), asm_arg(r64(S0))]);
            }
        }
        for &(r, i) in &plan.initializers {
            let src = mem(self, plan.inputs[i]);
            if r.float {
                self.emit(
                    move_bits(plan.inputs[i].bits),
                    [
                        asm_arg(simd_width(r.num, plan.inputs[i].bits)),
                        asm_arg(src),
                    ],
                );
            } else {
                self.emit(Opcode::Mov, [asm_arg(r64(r.num)), asm_arg(src)]);
            }
        }
        for &(i, _) in &plan.flags {
            self.emit(
                Opcode::Mov,
                [asm_arg(mem(self, plan.outputs[i])), asm_arg(0i64)],
            );
        }
        self.out.inline_assembly(&plan.text);
        for &(i, cc) in &plan.flags {
            self.emit(
                Opcode::Setcc(cc),
                [asm_arg(sized(self.frame(home + plan.outputs[i].offset), 8))],
            );
        }
        for (i, slot) in plan.outputs.iter().enumerate() {
            let Some(r) = plan.regs[i] else { continue };
            if r.float {
                self.emit(
                    move_bits(slot.bits),
                    [
                        asm_arg(mem(self, *slot)),
                        asm_arg(simd_width(r.num, slot.bits)),
                    ],
                );
            } else {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(mem(self, *slot)), asm_arg(r64(r.num))],
                );
            }
        }
        for (slot, &r) in plan.saved_regs.iter().enumerate() {
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(r64(r)),
                    asm_arg(sized(self.frame(home + 8 * slot), 64)),
                ],
            );
        }
        for (slot, &r) in plan.saved_simd.iter().enumerate() {
            let at = home + plan.simd_home + slot * usize::from(plan.simd_bits / 8);
            self.emit(
                move_bits(plan.simd_bits),
                [
                    asm_arg(simd_width(r, plan.simd_bits)),
                    asm_arg(sized(self.frame(at), plan.simd_bits)),
                ],
            );
        }
        for (i, &value) in self.func.inst_results(inst).iter().enumerate() {
            if let Some(dst) = self.defined(value) {
                if self.ty(value).is_float() || self.ty(value).is_vector() {
                    self.emit(
                        move_bits(plan.outputs[i].bits),
                        [
                            asm_arg(simd_width(FS0, plan.outputs[i].bits)),
                            asm_arg(mem(self, plan.outputs[i])),
                        ],
                    );
                    match dst {
                        Loc::FReg(r) => self.emit(
                            move_bits(plan.outputs[i].bits),
                            [
                                asm_arg(simd_width(r, plan.outputs[i].bits)),
                                asm_arg(simd_width(FS0, plan.outputs[i].bits)),
                            ],
                        ),
                        Loc::Stack(slot) => self.emit(
                            move_bits(plan.outputs[i].bits),
                            [
                                asm_arg(sized(self.slot(slot), plan.outputs[i].bits)),
                                asm_arg(simd_width(FS0, plan.outputs[i].bits)),
                            ],
                        ),
                        _ => unreachable!("SIMD asm result has a SIMD location"),
                    }
                } else {
                    self.emit(
                        Opcode::Mov,
                        [asm_arg(r64(S0)), asm_arg(mem(self, plan.outputs[i]))],
                    );
                    self.emit_move(dst, Loc::Reg(S0));
                }
            }
        }
    }
}
