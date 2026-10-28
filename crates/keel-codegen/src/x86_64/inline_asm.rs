//! Scalar GNU inline assembly. Stage inputs/results and preserve GPR homes so
//! the operand assignment is independent of allocator choices and scratch regs.
use super::*;
use keel_ir::{AsmArch, AsmClobber, AsmOperand as Constraint, InlineAsm};

pub(super) struct Plan {
    regs: Vec<Option<u8>>,
    flags: Vec<(usize, u8)>,
    initializers: [Option<usize>; 16],
    text: String,
    /// Registers overwritten by operand staging or the template, in slot order.
    pub saved_regs: Vec<u8>,
    pub bytes: usize,
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
    let mut taken = [false; 16];
    for c in &asm.clobbers {
        if let AsmClobber::Reg(r) = c {
            if r.arch != AsmArch::X86 || r.float || !available(r.num, frame) {
                return Err(format!("unsupported or reserved x86 asm clobber {r}"));
            }
            taken[r.num as usize] = true;
        }
    }
    let mut regs = vec![None; ops.len()];
    for (i, op) in ops.iter().enumerate() {
        let ty = if i < n {
            asm.outputs[i].1
        } else {
            func.value_type(func.resolve(args[i - n]))
        };
        if !((ty.is_int() && ty != Type::I128) || ty == Type::Ptr) {
            return Err(format!(
                "x86 asm operand {i} requires a scalar integer or pointer"
            ));
        }
        match *op {
            Constraint::Reg => {}
            Constraint::StackPointer if i >= n && ty == Type::I64 => {}
            Constraint::Flag(cc) if i < n && cc < 16 && ty.is_int() && !early[i] => {}
            Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
                if i >= n && ty == Type::Ptr => {}
            Constraint::Fixed(r) => {
                if r.arch != AsmArch::X86 || r.float || !available(r.num, frame) {
                    return Err(format!("unsupported or reserved x86 asm operand {r}"));
                }
                // Outputs have overlapping write lifetimes. Fixed inputs may
                // share a non-early output, but never a clobber or early output.
                if taken[r.num as usize]
                    && (i < n
                        || asm
                            .clobbers
                            .iter()
                            .any(|c| matches!(c, AsmClobber::Reg(clobber) if *clobber == r))
                        || (0..n).any(|out| regs[out] == Some(r.num) && early[out]))
                {
                    return Err(format!(
                        "asm operand {i}: register {r} overlaps another output, early-clobber or clobber"
                    ));
                }
                taken[r.num as usize] = true;
                regs[i] = Some(r.num);
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
        if matches!(
            op,
            Constraint::Reg | Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
        ) {
            let r = (0..16)
                .find(|&r| available(r, frame) && !taken[r as usize])
                .ok_or("too many x86 asm operands for the free registers")?;
            taken[r as usize] = true;
            regs[i] = Some(r);
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
    let mut initializers: [Option<usize>; 16] = [None; 16];
    for (i, &arg) in args.iter().enumerate() {
        let Some(r) = regs[n + i] else { continue };
        let slot = &mut initializers[r as usize];
        if let Some(previous) = *slot {
            if !keel_ir::asm::same_register_input(func, args[previous], arg) {
                return Err(format!(
                    "asm inputs {previous} and {i} require conflicting or unproven values in {}",
                    r64(r)
                ));
            }
            if func.value_type(func.resolve(arg)).bytes().unwrap_or(8)
                > func
                    .value_type(func.resolve(args[previous]))
                    .bytes()
                    .unwrap_or(8)
            {
                *slot = Some(i);
            }
        } else {
            *slot = Some(i);
        }
    }
    let text = substitute(func, asm, args, &regs, unique)?;
    // Staging uses RAX, while operand initialization and template outputs can
    // overwrite their assigned homes. A memory/flags clobber changes no GPR.
    let mut overwritten = [false; 16];
    for c in &asm.clobbers {
        if let AsmClobber::Reg(r) = c {
            overwritten[r.num as usize] = true;
        }
    }
    for &r in regs.iter().flatten() {
        overwritten[r as usize] = true;
    }
    if regs.iter().any(Option::is_some) || n != 0 {
        overwritten[S0 as usize] = true;
    }
    let saved_regs: Vec<_> = (0..16).filter(|&r| overwritten[r as usize]).collect();
    let bytes = if saved_regs.is_empty() && n == 0 {
        0
    } else {
        8 * (saved_regs.len() + args.len() + n)
    };
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
        bytes,
    })
}

fn substitute(
    func: &Function,
    asm: &InlineAsm,
    args: &[Value],
    regs: &[Option<u8>],
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
                write!(text, "(%{})", r64(r)).unwrap();
                continue;
            }
            let ty = if i < n {
                asm.outputs[i].1
            } else {
                func.value_type(func.resolve(args[i - n]))
            };
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
                    number: r,
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
        let n = plan.regs.len() - args.len();
        let inputs_at = plan.saved_regs.len();
        let outputs_at = inputs_at + args.len();
        let memory_at = |this: &Self, slot| sized(this.frame(home + 8 * slot), 64);
        // Preserve overwritten GPR homes, including scratch registers, before
        // any input rematerialization or memory-to-memory copy uses RAX.
        for (slot, &r) in plan.saved_regs.iter().enumerate() {
            self.emit(
                Opcode::Mov,
                [asm_arg(memory_at(self, slot)), asm_arg(r64(r))],
            );
        }
        for (i, &arg) in args.iter().enumerate() {
            if plan.regs[n + i].is_none() {
                continue;
            }
            let dst = memory_at(self, inputs_at + i);
            match self.loc(arg) {
                Loc::Reg(r) => {
                    if let Ok(slot) = plan.saved_regs.binary_search(&r) {
                        self.emit(
                            Opcode::Mov,
                            [asm_arg(r64(S0)), asm_arg(memory_at(self, slot))],
                        );
                    } else {
                        self.emit_move(Loc::Reg(S0), Loc::Reg(r));
                    }
                }
                src => self.emit_move(Loc::Reg(S0), src),
            }
            self.emit(Opcode::Mov, [asm_arg(dst), asm_arg(r64(S0))]);
        }
        // Every input now has a stable frame home. Loading RAX last is not
        // needed: these MOV loads use only the stable frame base.
        for (r, initializer) in plan.initializers.iter().enumerate() {
            if let Some(i) = *initializer {
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(r64(r as u8)),
                        asm_arg(memory_at(self, inputs_at + i)),
                    ],
                );
            }
        }
        // Initialize full-width boolean homes without touching flags after
        // the template. SETcc then writes only each slot's low byte.
        for &(i, _) in &plan.flags {
            self.emit(
                Opcode::Mov,
                [asm_arg(memory_at(self, outputs_at + i)), asm_arg(0i64)],
            );
        }
        self.out.inline_assembly(&plan.text);
        for &(i, cc) in &plan.flags {
            self.emit(
                Opcode::Setcc(cc),
                [asm_arg(sized(self.frame(home + 8 * (outputs_at + i)), 8))],
            );
        }
        for i in 0..n {
            let Some(r) = plan.regs[i] else { continue };
            self.emit(
                Opcode::Mov,
                [asm_arg(memory_at(self, outputs_at + i)), asm_arg(r64(r))],
            );
        }
        for (slot, &r) in plan.saved_regs.iter().enumerate() {
            self.emit(
                Opcode::Mov,
                [asm_arg(r64(r)), asm_arg(memory_at(self, slot))],
            );
        }
        for (i, &value) in self.func.inst_results(inst).iter().enumerate() {
            if let Some(dst) = self.defined(value) {
                // Restore all old homes first, then install SSA result homes.
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(S0)), asm_arg(memory_at(self, outputs_at + i))],
                );
                self.emit_move(dst, Loc::Reg(S0));
            }
        }
    }
}
