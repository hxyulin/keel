//! Layout-bearing SysV va_arg. No signature or pointer-definition inference.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn aggregate_va_arg(
        &mut self,
        inst: Inst,
        layout: &keel_ir::Layout,
        list: Value,
        home: keel_ir::StackSlot,
        producer: Option<keel_ir::X86VectorAbi>,
        result: Option<Value>,
    ) {
        let home = self.frame(self.stack_slot_offsets[home.index()]);
        // R10 is reserved from allocation. Keeping list there also permits
        // exact short chunk copies through the ordinary RAX/RDX scratch pair.
        let list_reg = self.use_reg(list, S2);
        self.emit(Opcode::Mov, [asm_arg(r64(R10)), asm_arg(r64(list_reg))]);
        let overflow = format!(".L{}_{inst}_aggregate_overflow", self.func.name);
        let done = format!(".L{}_{inst}_aggregate_done", self.func.name);
        let policy = sysv::aggregate_policy(layout, producer);
        let wrapper = match &policy {
            sysv::ProducerAggregate::Vector(ty) => Some(*ty),
            _ => None,
        };
        let classes = match policy {
            // Unnamed wide vectors are overflow-only, including wrappers
            // whose ordinary named placement uses YMM or ZMM.
            sysv::ProducerAggregate::Vector(ty) if ty.bytes().is_some_and(|n| n < 32) => {
                Some(vec![sysv::Class::Sse])
            }
            sysv::ProducerAggregate::Eightbytes(classes)
                if !classes
                    .iter()
                    .any(|c| matches!(c, sysv::Class::X87 | sysv::Class::X87Up)) =>
            {
                Some(classes)
            }
            _ => None,
        };
        if let Some(classes) = classes {
            let gp = classes
                .iter()
                .filter(|c| **c == sysv::Class::Integer)
                .count() as u32;
            let fp = classes.iter().filter(|c| **c == sysv::Class::Sse).count() as u32;
            for (field, demand, limit, stride) in [(0, gp, 48, 8), (4, fp, 176, 16)] {
                if demand == 0 {
                    continue;
                }
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(r32(S0)),
                        asm_arg(sized(memory(r64(R10), field), 32)),
                    ],
                );
                self.emit(
                    Opcode::Cmp,
                    [asm_arg(r32(S0)), asm_arg(limit - demand * stride)],
                );
                self.emit(Opcode::Jcc(7), [asm_arg(&overflow)]);
            }
            // Neither cursor advances unless both banks can supply all pieces.
            let (mut gi, mut fi) = (0, 0);
            for (i, class) in classes.iter().enumerate() {
                // Retain the storage index, but padding-only eightbytes have no source.
                if *class == sysv::Class::NoClass {
                    continue;
                }
                let (field, displacement) = if *class == sysv::Class::Integer {
                    let at = gi * 8;
                    gi += 1;
                    (0, at)
                } else {
                    let at = fi * 16;
                    fi += 1;
                    (4, at)
                };
                self.emit(
                    Opcode::Mov,
                    [
                        asm_arg(r32(S0)),
                        asm_arg(sized(memory(r64(R10), field), 32)),
                    ],
                );
                self.emit(
                    Opcode::Mov,
                    [asm_arg(r64(S1)), asm_arg(sized(memory(r64(R10), 16), 64))],
                );
                self.emit(Opcode::Add, [asm_arg(r64(S1)), asm_arg(r64(S0))]);
                if displacement != 0 {
                    self.emit(Opcode::Add, [asm_arg(r64(S1)), asm_arg(displacement)]);
                }
                let offset = i as u64 * 8;
                let bytes = if wrapper.is_some() {
                    layout.size()
                } else {
                    (layout.size() - offset).min(8)
                };
                self.copy_bytes(&offset_memory(&home, offset as i128), S1, bytes);
            }
            // Commit only after the complete object has been read into its home.
            for (field, demand, stride) in [(0, gp, 8), (4, fp, 16)] {
                if demand == 0 {
                    continue;
                }
                self.emit(
                    Opcode::Add,
                    [
                        asm_arg(sized(memory(r64(R10), field), 32)),
                        asm_arg(demand * stride),
                    ],
                );
            }
            self.emit(Opcode::Jmp, [asm_arg(&done)]);
        }
        self.out.label(overflow);
        self.emit(
            Opcode::Mov,
            [asm_arg(r64(S1)), asm_arg(sized(memory(r64(R10), 8), 64))],
        );
        let alignment = layout.align().max(8);
        if alignment > 8 {
            self.emit(Opcode::Add, [asm_arg(r64(S1)), asm_arg(alignment - 1)]);
            self.emit(
                Opcode::And,
                [asm_arg(r64(S1)), asm_arg(-(alignment as i64))],
            );
        }
        self.copy_bytes(&home, S1, layout.size());
        self.emit(
            Opcode::Add,
            [asm_arg(r64(S1)), asm_arg(layout.size().next_multiple_of(8))],
        );
        self.emit(
            Opcode::Mov,
            [asm_arg(sized(memory(r64(R10), 8), 64)), asm_arg(r64(S1))],
        );
        self.out.label(done);
        let d = self.def_reg(result);
        self.emit(Opcode::Lea, [asm_arg(r64(d)), asm_arg(home)]);
        self.def_done(result, d);
    }
}
