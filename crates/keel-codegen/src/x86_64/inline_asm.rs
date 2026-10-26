//! Scalar GNU inline assembly. Stage inputs/results and preserve GPR homes so
//! the operand assignment is independent of allocator choices and scratch regs.
use super::*;
use keel_ir::{AsmArch, AsmClobber, AsmOperand as Constraint, InlineAsm};

pub(super) struct Plan {
    regs: Vec<Option<u8>>,
    text: String,
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
    let ops: Vec<_> = asm
        .outputs
        .iter()
        .map(|(o, _)| *o)
        .chain(asm.inputs.iter().copied())
        .collect();
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
            Constraint::Mem | Constraint::MemWrite | Constraint::MemReadWrite
                if i >= n && ty == Type::Ptr => {}
            Constraint::Fixed(r) => {
                if r.arch != AsmArch::X86 || r.float || !available(r.num, frame) {
                    return Err(format!("unsupported or reserved x86 asm operand {r}"));
                }
                if taken[r.num as usize] {
                    return Err(format!(
                        "asm operand {i}: register {r} overlaps another operand or clobber; use a matching constraint"
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
                if i >= n && (out as usize) < n && asm.outputs[out as usize].1 == ty => {}
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
    let mut tied = vec![false; n];
    for (i, op) in ops.iter().enumerate().skip(n) {
        if let Constraint::Tied(out) = *op {
            let out = out as usize;
            if tied[out] {
                return Err("asm output is tied to more than one input".into());
            }
            tied[out] = true;
            regs[i] = Some(regs[out].ok_or("asm tie requires a register output")?);
        }
    }
    let text = substitute(func, asm, args, &regs, unique)?;
    // The shared object assembler owns instruction/directive validation and
    // feature checks. Parsing fragments here would bypass GNU preprocessing
    // (numeric local labels and macros in particular).
    Ok(Plan {
        regs,
        text,
        bytes: 8 * (16 + args.len() + n),
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
            if i < n || !matches!(modifier, None | Some('c' | 'a' | 'n')) {
                return Err("unsupported x86 asm immediate modifier".into());
            }
            let arg = args[i - n];
            if modifier.is_none() {
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
        let home = self.inline_asm_home.expect("asm frame prepared");
        let plan = self
            .inline_asm_plans
            .remove(&inst)
            .expect("asm checked before emission");
        let memory_at = |this: &Self, slot| sized(this.frame(home + 8 * slot), 64);
        // Preserve every GPR home, including scratch registers. This runs before
        // any input rematerialization or memory-to-memory copy uses RAX.
        for r in 0..16 {
            if available(r, self.frame_register) {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(memory_at(self, r as usize)), asm_arg(r64(r))],
                );
            }
        }
        for (i, &arg) in args.iter().enumerate() {
            let op = self.func.inst_data(inst);
            if matches!(op, InstData::InlineAsm { asm, .. } if matches!(asm.inputs[i], Constraint::Imm | Constraint::NumericImm))
            {
                continue;
            }
            let dst = memory_at(self, 16 + i);
            match self.loc(arg) {
                Loc::Reg(r) => self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(S0)), asm_arg(memory_at(self, r as usize))],
                ),
                src => self.emit_move(Loc::Reg(S0), src),
            }
            self.emit(Opcode::Mov, [asm_arg(dst), asm_arg(r64(S0))]);
        }
        let n = plan.regs.len() - args.len();
        // Every input now has a stable frame home. Loading RAX last is not
        // needed: these MOV loads use only the stable frame base.
        for (i, _) in args.iter().enumerate() {
            if let Some(r) = plan.regs[n + i] {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(r)), asm_arg(memory_at(self, 16 + i))],
                );
            }
        }
        self.out.inline_assembly(&plan.text);
        for i in 0..n {
            let r = plan.regs[i].expect("register output");
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(memory_at(self, 16 + args.len() + i)),
                    asm_arg(r64(r)),
                ],
            );
        }
        for r in 0..16 {
            if available(r, self.frame_register) {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(r)), asm_arg(memory_at(self, r as usize))],
                );
            }
        }
        for (i, &value) in self.func.inst_results(inst).iter().enumerate() {
            if let Some(dst) = self.defined(value) {
                // Restore all old homes first, then install SSA result homes.
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(r64(S0)),
                        asm_arg(memory_at(self, 16 + args.len() + i)),
                    ],
                );
                self.emit_move(dst, Loc::Reg(S0));
            }
        }
    }
}
