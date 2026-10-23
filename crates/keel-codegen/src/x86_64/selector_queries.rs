//! Descriptor queries observe implicit descriptor state and are pinned barriers.
//! Sources are I16 or an exact m16 access (including its architectural faults).
//! LAR/LSL take an old destination and return (same-width value, I32 ZF=0/1).
//! Successful LAR bits 19:16 are undefined at widths 32/64; no FLAGS-region
//! preservation is promised. Intel SDM 093 Vol.2A 3-533/573, Vol.2C 5-162.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_selector_query(&mut self, inst: Inst, id: Intrinsic, args: &[Value]) {
        let (op, width, mem) = id.x86_selector_query().expect("selector query dispatch");
        let query = width != 0;
        // S0/S1/S2 are allocator-reserved. Stage every input before publishing
        // either result, even if homes alias dying inputs or both spill.
        if query {
            let old = self.use_reg(args[0], S0);
            if old != S0 {
                self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(old))]);
            }
        }
        let selector = self.use_reg(args[usize::from(query)], S1);
        if selector != S1 {
            self.emit(Opcode::Mov, [asm_arg(r64(S1)), asm_arg(r64(selector))]);
        }
        let source = if mem {
            asm_arg(sized(memory(r64(S1), 0), 16))
        } else {
            asm_arg(r16(S1))
        };
        let opcode = Opcode::parse(op).expect("baseline descriptor query");
        if query {
            self.emit(opcode, [asm_arg(Register { width, ..r64(S0) }), source]);
        } else {
            self.emit(opcode, [source]);
        }
        // Must be the very next instruction: later normalization/publication
        // and allocator work may modify FLAGS. SETZ writes just DL.
        self.emit(Opcode::Setcc(4), [asm_arg(r8(S2))]);
        self.emit(Opcode::Movzx, [asm_arg(r32(S2)), asm_arg(r8(S2))]);
        if width == 16 {
            self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r16(S0))]);
        }
        let results = self.func.inst_results(inst);
        for (i, &value) in results.iter().enumerate() {
            let source = if query && i == 0 { S0 } else { S2 };
            let d = self.def_reg(Some(value));
            if d != source {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(source))]);
            }
            self.def_done(Some(value), d);
        }
    }
}
