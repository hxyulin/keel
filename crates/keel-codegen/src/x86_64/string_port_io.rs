//! Exact string port I/O, including an opaque owned-terminal FLAGS boundary.
//! No ordinary IR is lowered inside ownership. See docs/targets/x86_64/system/string-port-io.md.
use super::*;
use keel_asm_ir::x86::{CoreOpcode, Instruction, Repeat, StringKind};
use keel_asm_ir::{Item, Program};

pub(super) const FRAME_BYTES: usize = 80;
const INDEX_SAVE: usize = 0;
const PORT: usize = 1;
const ADDRESS: usize = 2;
const COUNT: usize = 3;
const REQUESTED: usize = 4;
const ENTRY: usize = 5;
const NEXT: usize = 6;
const REMAINING: usize = 7;
const BEFORE: usize = 8;
const AFTER: usize = 9;

struct Storage([Memory; 10]);
impl Storage {
    fn at(&self, n: usize) -> AsmOperand {
        asm_arg(self.0[n].clone())
    }
    fn port(&self) -> AsmOperand {
        asm_arg(sized(self.0[PORT].clone(), 16))
    }
}
fn principal(id: Intrinsic) -> Opcode {
    let (output, width, rep, _) = id.x86_string_port_io().expect("string I/O descriptor");
    Opcode::Core(CoreOpcode::String {
        kind: if output {
            StringKind::Outs
        } else {
            StringKind::Ins
        },
        width,
        repeat: if rep { Repeat::Rep } else { Repeat::None },
        address32: false,
        segment: None,
    })
}
struct Recipe<'a> {
    p: Program,
    prefix: &'a str,
    pc: usize,
}
impl Recipe<'_> {
    fn ins<const N: usize>(&mut self, opcode: Opcode, operands: [AsmOperand; N]) {
        // Every instruction, including either side of each stack pair, has a
        // distinct local boundary. No automatic exception cleanup/resume exists.
        self.p.label(format!("{}_pc{}", self.prefix, self.pc));
        self.pc += 1;
        self.p.x86_instruction(Instruction {
            opcode,
            operands: Vec::from(operands),
            lock: false,
        });
    }
    fn snapshot(&mut self, home: AsmOperand) {
        self.ins(Opcode::parse("pushfq").unwrap(), []);
        self.ins(Opcode::Pop, [asm_arg(r64(S0))]);
        self.ins(Opcode::Mov, [home, asm_arg(r64(S0))]);
    }
    fn apply(&mut self, home: AsmOperand) {
        self.ins(Opcode::Mov, [asm_arg(r64(S0)), home]);
        self.ins(Opcode::Push, [asm_arg(r64(S0))]);
        self.ins(Opcode::parse("popfq").unwrap(), []);
    }
}
fn lower(id: Intrinsic, s: &Storage, prefix: &str) -> Program {
    let (out, _, rep, owned) = id.x86_string_port_io().unwrap();
    let index = if out { 6 } else { 7 }; // RSI / RDI: ordinary allocated registers.
    let mut r = Recipe {
        p: Program::new(),
        prefix,
        pc: 0,
    };
    r.ins(Opcode::Mov, [s.at(INDEX_SAVE), asm_arg(r64(index))]);
    if owned {
        r.snapshot(s.at(ENTRY));
    }
    r.ins(Opcode::Mov, [asm_arg(r16(S2)), s.port()]);
    r.ins(Opcode::Mov, [asm_arg(r64(index)), s.at(ADDRESS)]);
    if rep {
        r.ins(Opcode::Mov, [asm_arg(r64(S1)), s.at(COUNT)]);
    }
    if owned {
        r.apply(s.at(REQUESTED));
        r.snapshot(s.at(BEFORE));
    }
    r.ins(principal(id), []);
    // MOV only; the hardware index/count, rather than a recomputed value.
    r.ins(Opcode::Mov, [s.at(NEXT), asm_arg(r64(index))]);
    if rep {
        r.ins(Opcode::Mov, [s.at(REMAINING), asm_arg(r64(S1))]);
    }
    if owned {
        r.snapshot(s.at(AFTER));
        r.apply(s.at(ENTRY));
    }
    r.ins(Opcode::Mov, [asm_arg(r64(index)), s.at(INDEX_SAVE)]);
    r.p
}

/// Independent exact parser of the post-frame typed program. Verifies resource
/// provenance, immutable inputs, implicit registers, actual element/address/count
/// widths, FLAGS/stack pairs and register restoration before output publication.
fn validate(id: Intrinsic, s: &Storage, prefix: &str, p: &Program) -> Result<(), String> {
    let (output, bits, rep, owned) = id.x86_string_port_io().ok_or("not string port I/O")?;
    if !matches!(bits, 8 | 16 | 32) {
        return Err("invalid string I/O width".into());
    }
    for (i, home) in s.0.iter().enumerate() {
        let mut expected = s.0[0].clone();
        expected.displacement.expression.constant += 8 * i as i128;
        if *home != expected
            || home.width != 64
            || !matches!(home.base, Some(r) if r == r64(5) || r == r64(15))
            || home.index.is_some()
            || home.segment.is_some()
            || home.scale != 1
            || home.displacement.modifier != keel_asm_ir::x86::Modifier::None
            || home.displacement.expression.add.is_some()
            || home.displacement.expression.subtract.is_some()
            || home.displacement.expression.constant % 8 != 0
        {
            return Err(
                "string I/O requires disjoint aligned contiguous private frame homes".into(),
            );
        }
    }
    let mut items = p.items.iter();
    let mut pc = 0;
    let mut expect = |opcode, operands: Vec<AsmOperand>| -> Result<(), String> {
        let label = Item::Label(format!("{prefix}_pc{pc}"));
        pc += 1;
        let ins = Item::X86Instruction(Instruction {
            opcode,
            operands,
            lock: false,
        });
        if items.next() != Some(&label) || items.next() != Some(&ins) {
            return Err(format!(
                "string I/O exact resource sequence mismatch at boundary {}",
                pc - 1
            ));
        }
        Ok(())
    };
    let index = asm_arg(r64(if output { 6 } else { 7 }));
    let a = asm_arg(r64(S0));
    let pushfq = Opcode::parse("pushfq").unwrap();
    let popfq = Opcode::parse("popfq").unwrap();
    expect(Opcode::Mov, vec![s.at(INDEX_SAVE), index.clone()])?;
    if owned {
        expect(pushfq, vec![])?;
        expect(Opcode::Pop, vec![a.clone()])?;
        expect(Opcode::Mov, vec![s.at(ENTRY), a.clone()])?;
    }
    expect(Opcode::Mov, vec![asm_arg(r16(2)), s.port()])?;
    expect(Opcode::Mov, vec![index.clone(), s.at(ADDRESS)])?;
    if rep {
        expect(Opcode::Mov, vec![asm_arg(r64(1)), s.at(COUNT)])?;
    }
    if owned {
        expect(Opcode::Mov, vec![a.clone(), s.at(REQUESTED)])?;
        expect(Opcode::Push, vec![a.clone()])?;
        expect(popfq, vec![])?;
        expect(pushfq, vec![])?;
        expect(Opcode::Pop, vec![a.clone()])?;
        expect(Opcode::Mov, vec![s.at(BEFORE), a.clone()])?;
    }
    expect(
        Opcode::Core(CoreOpcode::String {
            kind: if output {
                StringKind::Outs
            } else {
                StringKind::Ins
            },
            width: bits,
            repeat: if rep { Repeat::Rep } else { Repeat::None },
            address32: false,
            segment: None,
        }),
        vec![],
    )?;
    expect(Opcode::Mov, vec![s.at(NEXT), index.clone()])?;
    if rep {
        expect(Opcode::Mov, vec![s.at(REMAINING), asm_arg(r64(1))])?;
    }
    if owned {
        expect(pushfq, vec![])?;
        expect(Opcode::Pop, vec![a.clone()])?;
        expect(Opcode::Mov, vec![s.at(AFTER), a.clone()])?;
        expect(Opcode::Mov, vec![a.clone(), s.at(ENTRY)])?;
        expect(Opcode::Push, vec![a])?;
        expect(popfq, vec![])?;
    }
    expect(Opcode::Mov, vec![index, s.at(INDEX_SAVE)])?;
    drop(expect);
    if items.next().is_some() {
        return Err("unchecked trailing string I/O resource".into());
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn prepare_string_port_io(&mut self) -> Result<(), String> {
        for (_, inst) in self.func.layout_insts() {
            let InstData::Intrinsic { id, args } = self.func.inst_data(inst) else {
                continue;
            };
            if id.x86_string_port_io().is_none() {
                continue;
            }
            let info = id.info();
            let results = self.func.inst_results(inst);
            if args.len() != info.params.len()
                || results.len() != info.returns.len()
                || args
                    .iter()
                    .zip(info.params)
                    .any(|(&v, &t)| self.func.value_type(self.func.resolve(v)) != t)
                || results
                    .iter()
                    .zip(info.returns)
                    .any(|(&v, &t)| self.func.value_type(v) != t)
            {
                return Err("string port I/O exact operand/result types or arity mismatch".into());
            }
            if self.frameless {
                return Err("string port I/O requires stable rbp CFA".into());
            }
            let base = self
                .string_io_home
                .ok_or("string I/O private frame not reserved")?;
            if base % 8 != 0
                || base
                    .checked_add(FRAME_BYTES)
                    .is_none_or(|n| n > self.frame_size)
            {
                return Err("string I/O private frame bounds/alignment failed".into());
            }
            let s = Storage(std::array::from_fn(|i| sized(self.frame(base + 8 * i), 64)));
            let prefix = format!(".L{}_{}_stringio", self.func.name, inst);
            let p = lower(*id, &s, &prefix);
            validate(*id, &s, &prefix, &p)?;
            self.string_io_programs.insert(inst, p);
        }
        Ok(())
    }
    pub(super) fn native_string_port_io(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        let (_, _, rep, owned) = id.x86_string_port_io().unwrap();
        let base = self.string_io_home.expect("preflighted string I/O frame");
        // CAPTURE ALL inputs before the fixed-index save or any implicit GPR
        // write. use_reg may rematerialize, but only into allocator-reserved S0.
        // Result homes are not touched until every fixed allocated GPR is back.
        let slots = if owned {
            &[REQUESTED, PORT, ADDRESS, COUNT][..]
        } else {
            &[PORT, ADDRESS, COUNT][..]
        };
        for (&v, &slot) in args.iter().zip(slots) {
            let r = self.use_reg(v, S0);
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(self.frame(base + 8 * slot), 64)),
                    asm_arg(r64(r)),
                ],
            );
        }
        self.out.checked_region(
            self.string_io_programs
                .remove(&inst)
                .expect("checked string I/O program"),
        );
        let mut homes = vec![NEXT];
        if rep {
            homes.push(REMAINING);
        }
        if owned {
            homes.extend([BEFORE, AFTER]);
        }
        for (&v, slot) in self.func.inst_results(inst).iter().zip(homes) {
            let d = self.def_reg(Some(v));
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(r64(d)),
                    asm_arg(sized(self.frame(base + 8 * slot), 64)),
                ],
            );
            self.def_done(Some(v), d);
        }
    }
}

#[cfg(test)]
mod proof_tests {
    use super::*;
    #[test]
    fn exact_resource_parser_rejects_every_sequence_and_home_mutation() {
        let mut variants = 0;
        let mut mutants = 0;
        for id in Intrinsic::ALL
            .into_iter()
            .filter(|i| i.x86_string_port_io().is_some())
        {
            let homes = || {
                Storage(std::array::from_fn(|i| {
                    sized(memory(r64(5), -80 + 8 * i as i128), 64)
                }))
            };
            let s = homes();
            let p = lower(id, &s, ".Lproof");
            validate(id, &s, ".Lproof", &p).unwrap();
            variants += 1;
            for n in 0..p.items.len() {
                let mut m = p.clone();
                m.items.remove(n);
                assert!(validate(id, &s, ".Lproof", &m).is_err());
                mutants += 1;
                let mut m = p.clone();
                m.items.insert(
                    n,
                    Item::X86Instruction(Instruction {
                        opcode: Opcode::parse("cld").unwrap(),
                        operands: vec![],
                        lock: false,
                    }),
                );
                assert!(validate(id, &s, ".Lproof", &m).is_err());
                mutants += 1;
                let mut m = p.clone();
                match &mut m.items[n] {
                    Item::Label(label) => label.push_str("_wrong_boundary"),
                    Item::X86Instruction(ins) => ins.lock = true,
                    _ => unreachable!(),
                }
                assert!(validate(id, &s, ".Lproof", &m).is_err());
                mutants += 1;
                if let Item::X86Instruction(ins) = &p.items[n] {
                    for operand in 0..ins.operands.len() {
                        let mut m = p.clone();
                        let Item::X86Instruction(i) = &mut m.items[n] else {
                            unreachable!()
                        };
                        i.operands[operand] = asm_arg(r32(11));
                        assert!(validate(id, &s, ".Lproof", &m).is_err());
                        mutants += 1;
                    }
                    if matches!(ins.opcode, Opcode::Core(CoreOpcode::String { .. })) {
                        for wrong in [
                            Opcode::Core(CoreOpcode::String {
                                kind: StringKind::Ins,
                                width: 64,
                                repeat: Repeat::Rep,
                                address32: false,
                                segment: None,
                            }),
                            Opcode::Core(CoreOpcode::String {
                                kind: StringKind::Outs,
                                width: 8,
                                repeat: Repeat::Repne,
                                address32: true,
                                segment: Some(4),
                            }),
                        ] {
                            let mut m = p.clone();
                            let Item::X86Instruction(i) = &mut m.items[n] else {
                                unreachable!()
                            };
                            i.opcode = wrong;
                            assert!(validate(id, &s, ".Lproof", &m).is_err());
                            mutants += 1;
                        }
                    }
                }
            }
            for slot in 0..10 {
                let mut bad = homes();
                bad.0[slot].base = Some(r64(7)); // caller index is not a stable frame capability.
                assert!(validate(id, &bad, ".Lproof", &p).is_err());
                let mut bad = homes();
                bad.0[slot].displacement.expression.constant += 1;
                assert!(validate(id, &bad, ".Lproof", &p).is_err());
                mutants += 2;
            }
            let mut extra = p.clone();
            extra.items.push(Item::Label(".Lescape".into()));
            assert!(validate(id, &s, ".Lproof", &extra).is_err());
            mutants += 1;
        }
        assert_eq!(variants, 24);
        assert!(mutants > 2000);
    }
}
