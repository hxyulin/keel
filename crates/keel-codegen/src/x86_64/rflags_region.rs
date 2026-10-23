//! Exact terminal-monitor FLAGS region expansion, checked after frame/allocation.
//! Native acceptance is pending independent root-owned guest proof.
use super::*;
use keel_asm_ir::x86::Instruction;
use keel_asm_ir::{Item, Program};
use keel_ir::rflags_region::{Exit, Operation as Op, Plan};

struct Storage {
    saved: Memory,
    captures: Vec<Memory>,
    observations: Vec<Memory>,
}
fn instruction<const N: usize>(p: &mut Program, opcode: Opcode, operands: [AsmOperand; N]) {
    p.x86_instruction(Instruction {
        opcode,
        operands: Vec::from(operands),
        lock: false,
    });
}
fn fixed(p: &mut Program, name: &str) {
    instruction(p, Opcode::parse(name).unwrap(), []);
}
fn snapshot(p: &mut Program, home: &Memory) {
    fixed(p, "pushfq");
    instruction(p, Opcode::Pop, [asm_arg(r64(S0))]);
    instruction(p, Opcode::Mov, [asm_arg(home.clone()), asm_arg(r64(S0))]);
}
fn apply(p: &mut Program, home: &Memory) {
    instruction(p, Opcode::Mov, [asm_arg(r64(S0)), asm_arg(home.clone())]);
    instruction(p, Opcode::Push, [asm_arg(r64(S0))]);
    fixed(p, "popfq");
}
fn lower(plan: &Plan, storage: &Storage, prefix: &str) -> Program {
    let mut p = Program::new();
    for (b, block) in plan.blocks.iter().enumerate() {
        p.label(format!("{prefix}_{b}"));
        for (position, op) in block.operations.iter().enumerate() {
            p.label(format!("{prefix}_{b}_op{position}"));
            match *op {
                Op::SaveEntry => snapshot(&mut p, &storage.saved),
                Op::Read { observation } => snapshot(&mut p, &storage.observations[observation]),
                Op::Swap {
                    capture,
                    observation,
                } => {
                    snapshot(&mut p, &storage.observations[observation]);
                    apply(&mut p, &storage.captures[capture]);
                }
                Op::ReapplyEntryImage => apply(&mut p, &storage.saved),
            }
        }
        match block.exit {
            Exit::Jump(target) => {
                instruction(&mut p, Opcode::Jmp, [asm_arg(format!("{prefix}_{target}"))])
            }
            Exit::FlagsBranch {
                condition,
                taken,
                not_taken,
            } => {
                instruction(
                    &mut p,
                    Opcode::Jcc(condition as u8),
                    [asm_arg(format!("{prefix}_{taken}"))],
                );
                instruction(
                    &mut p,
                    Opcode::Jmp,
                    [asm_arg(format!("{prefix}_{not_taken}"))],
                );
            }
            Exit::Normal => instruction(&mut p, Opcode::Jmp, [asm_arg(format!("{prefix}_done"))]),
        }
    }
    p.label(format!("{prefix}_done"));
    p
}

/// Closed instruction-by-instruction parser over the ACTUAL typed expansion.
/// No permissive opcode whitelist: operands, order, widths, destinations, labels,
/// saved/captured-home provenance and every transient stack pair must match.
fn validate(plan: &Plan, storage: &Storage, prefix: &str, actual: &Program) -> Result<(), String> {
    keel_ir::rflags_region::analyze(plan).map_err(|e| e.to_string())?;
    if storage.captures.len() != plan.captures.len()
        || storage.observations.len() != plan.observations
    {
        return Err("FLAGS region storage arity mismatch".into());
    }
    let homes = std::iter::once(&storage.saved)
        .chain(&storage.captures)
        .chain(&storage.observations)
        .collect::<Vec<_>>();
    for (i, h) in homes.iter().enumerate() {
        let mut expected = storage.saved.clone();
        expected.displacement.expression.constant += 8 * i as i128;
        if h.width != 64
            || **h != expected
            || !matches!(h.base, Some(r) if r == r64(5) || r == r64(15))
            || h.index.is_some()
            || h.segment.is_some()
            || h.scale != 1
            || h.displacement.modifier != keel_asm_ir::x86::Modifier::None
            || h.displacement.expression.add.is_some()
            || h.displacement.expression.subtract.is_some()
            || h.displacement.expression.constant % 8 != 0
        {
            return Err(
                "FLAGS region private homes must be contiguous aligned frame-relative exact m64"
                    .into(),
            );
        }
    }
    let mut items = actual.items.iter();
    let mut take = |expected: Item| -> Result<(), String> {
        match items.next() {
            Some(item) if *item == expected => Ok(()),
            item => Err(format!(
                "FLAGS region post-expansion resource mismatch: expected {expected:?}, got {item:?}"
            )),
        }
    };
    fn ins(opcode: Opcode, operands: Vec<AsmOperand>) -> Item {
        Item::X86Instruction(Instruction {
            opcode,
            operands,
            lock: false,
        })
    }
    let pop = Opcode::Pop;
    let push = Opcode::Push;
    let mov = Opcode::Mov;
    let pushfq = Opcode::parse("pushfq").unwrap();
    let popfq = Opcode::parse("popfq").unwrap();
    for (b, block) in plan.blocks.iter().enumerate() {
        take(Item::Label(format!("{prefix}_{b}")))?;
        for (position, op) in block.operations.iter().enumerate() {
            take(Item::Label(format!("{prefix}_{b}_op{position}")))?;
            match *op {
                Op::SaveEntry | Op::Read { .. } | Op::Swap { .. } => {
                    // This complete pair has RSP -8 then 0, no arithmetic;
                    // CFA base is independently preflighted as stable rbp+16.
                    take(ins(pushfq, vec![]))?;
                    take(ins(pop, vec![asm_arg(r64(S0))]))?;
                    let home = match *op {
                        Op::SaveEntry => &storage.saved,
                        Op::Read { observation } | Op::Swap { observation, .. } => {
                            &storage.observations[observation]
                        }
                        _ => unreachable!(),
                    };
                    take(ins(mov, vec![asm_arg(home.clone()), asm_arg(r64(S0))]))?;
                }
                Op::ReapplyEntryImage => {}
            }
            let input = match *op {
                Op::Swap { capture, .. } => Some(&storage.captures[capture]),
                Op::ReapplyEntryImage => Some(&storage.saved),
                _ => None,
            };
            if let Some(input) = input {
                take(ins(mov, vec![asm_arg(r64(S0)), asm_arg(input.clone())]))?;
                take(ins(push, vec![asm_arg(r64(S0))]))?;
                take(ins(popfq, vec![]))?;
            }
        }
        match block.exit {
            Exit::Jump(target) => take(ins(
                Opcode::Jmp,
                vec![asm_arg(format!("{prefix}_{target}"))],
            ))?,
            Exit::FlagsBranch {
                condition,
                taken,
                not_taken,
            } => {
                take(ins(
                    Opcode::Jcc(condition as u8),
                    vec![asm_arg(format!("{prefix}_{taken}"))],
                ))?;
                take(ins(
                    Opcode::Jmp,
                    vec![asm_arg(format!("{prefix}_{not_taken}"))],
                ))?;
            }
            Exit::Normal => take(ins(Opcode::Jmp, vec![asm_arg(format!("{prefix}_done"))]))?,
        }
    }
    take(Item::Label(format!("{prefix}_done")))?;
    drop(take);
    if items.next().is_some() {
        return Err("FLAGS region has trailing unchecked instructions".into());
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn prepare_rflags_regions(&mut self) -> Result<(), String> {
        for (_, inst) in self.func.layout_insts() {
            let InstData::X86RflagsRegion { plan, .. } = self.func.inst_data(inst) else {
                continue;
            };
            if self.frameless {
                return Err("FLAGS region requires stable rbp CFA".into());
            }
            let base = self
                .rflags_home
                .ok_or("FLAGS region private storage was not preallocated")?;
            let slots = 1 + plan.captures.len() + plan.observations;
            if base % 8 != 0 || base + slots * 8 > self.frame_size {
                return Err("FLAGS region private frame bounds/alignment preflight failed".into());
            }
            let home = |n| sized(self.frame(base + 8 * n), 64);
            let storage = Storage {
                saved: home(0),
                captures: (0..plan.captures.len()).map(|i| home(1 + i)).collect(),
                observations: (0..plan.observations)
                    .map(|i| home(1 + plan.captures.len() + i))
                    .collect(),
            };
            let prefix = format!(".L{}_{}_rflags", self.func.name, inst);
            let p = lower(plan, &storage, &prefix);
            validate(plan, &storage, &prefix, &p)?;
            self.rflags_programs.insert(inst, p);
        }
        Ok(())
    }
    pub(super) fn native_rflags_region(&mut self, inst: Inst, args: &[Value]) {
        let base = self.rflags_home.expect("preflighted private frame");
        // Every allocated/rematerialized input is copied before SaveEntry and
        // before any result publication. Captures and observations cannot alias.
        for (i, &arg) in args.iter().enumerate() {
            let r = self.use_reg(arg, S0);
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(self.frame(base + 8 * (1 + i)), 64)),
                    asm_arg(r64(r)),
                ],
            );
        }
        let p = self
            .rflags_programs
            .remove(&inst)
            .expect("post-allocation checked raw program");
        self.out.checked_region(p);
        for (i, &v) in self.func.inst_results(inst).iter().enumerate() {
            let d = self.def_reg(Some(v));
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(r64(d)),
                    asm_arg(sized(self.frame(base + 8 * (1 + args.len() + i)), 64)),
                ],
            );
            self.def_done(Some(v), d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn case() -> (Plan, Storage, Program) {
        let p = Plan::parse(
            "save; swap 0 0; jcc 4 1 2 | swap 1 1; jump 3 | read 1; jump 3 | reapply; normal",
            2,
            2,
        )
        .unwrap();
        let home = |i: i128| sized(memory(r64(5), -64 + 8 * i), 64);
        let s = Storage {
            saved: home(0),
            captures: vec![home(1), home(2)],
            observations: vec![home(3), home(4)],
        };
        let actual = lower(&p, &s, ".Ltest");
        (p, s, actual)
    }
    #[test]
    fn every_instruction_rejects_deletion_reordering_hidden_arithmetic_and_lock() {
        let (p, s, actual) = case();
        validate(&p, &s, ".Ltest", &actual).unwrap();
        for index in 0..actual.items.len() {
            let mut deleted = actual.clone();
            deleted.items.remove(index);
            assert!(validate(&p, &s, ".Ltest", &deleted).is_err());
            if let Item::X86Instruction(_) = &actual.items[index] {
                let mut locked = actual.clone();
                let Item::X86Instruction(i) = &mut locked.items[index] else {
                    unreachable!()
                };
                i.lock = true;
                assert!(validate(&p, &s, ".Ltest", &locked).is_err());
                let mut changed = actual.clone();
                let Item::X86Instruction(i) = &mut changed.items[index] else {
                    unreachable!()
                };
                i.opcode = Opcode::Add;
                assert!(validate(&p, &s, ".Ltest", &changed).is_err());
            }
            if index + 1 < actual.items.len() && actual.items[index] != actual.items[index + 1] {
                let mut reordered = actual.clone();
                reordered.items.swap(index, index + 1);
                assert!(validate(&p, &s, ".Ltest", &reordered).is_err());
            }
        }
        for item in [
            Item::Text("popfq".into()),
            Item::Directive(Directive::Cfi(Cfi::DefCfaOffset(8))),
            Item::X86Instruction(Instruction {
                opcode: Opcode::Ret,
                operands: vec![],
                lock: false,
            }),
        ] {
            let mut changed = actual.clone();
            changed.items.push(item);
            assert!(validate(&p, &s, ".Ltest", &changed).is_err());
        }
    }
    #[test]
    fn actual_operand_width_register_home_and_branch_provenance_are_checked() {
        let (p, s, actual) = case();
        let mut count = 0;
        for index in 0..actual.items.len() {
            let Item::X86Instruction(original) = &actual.items[index] else {
                continue;
            };
            for operand in 0..original.operands.len() {
                let mut changed = actual.clone();
                let Item::X86Instruction(i) = &mut changed.items[index] else {
                    unreachable!()
                };
                match &mut i.operands[operand] {
                    AsmOperand::Register(r) => r.width = 32,
                    AsmOperand::Memory(m) => m.displacement.expression.constant += 8,
                    AsmOperand::Immediate(a) => {
                        a.expression = keel_asm_ir::Expression::symbol(".Lescape")
                    }
                }
                assert!(validate(&p, &s, ".Ltest", &changed).is_err());
                count += 1;
            }
        }
        assert!(count > 25);
    }
    #[test]
    fn storage_rejects_overlap_unaligned_and_non_frame_resources() {
        let (p, mut s, _) = case();
        s.observations[0] = s.saved.clone();
        let actual = lower(&p, &s, ".Ltest");
        assert!(validate(&p, &s, ".Ltest", &actual).is_err());
        let (p, mut s, _) = case();
        s.saved.displacement.expression.constant += 1;
        let actual = lower(&p, &s, ".Ltest");
        assert!(validate(&p, &s, ".Ltest", &actual).is_err());
        let (p, mut s, _) = case();
        s.captures[0].base = Some(r64(4));
        let actual = lower(&p, &s, ".Ltest");
        assert!(validate(&p, &s, ".Ltest", &actual).is_err());
    }
}
