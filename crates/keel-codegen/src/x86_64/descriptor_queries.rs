//! Long-mode descriptor/status observations and privileged single-operand writes.
//! Permission, state ownership and fault handling remain caller preconditions.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_system_write(&mut self, id: Intrinsic, args: &[Value]) {
        let (op, width, pointer) = id.x86_system_write().expect("system write dispatch");
        let input = self.use_reg(args[0], S0);
        let operand = if pointer {
            // The INVLPG operand computes an address; the hardware performs
            // no data read. The descriptor/status memory forms read directly.
            asm_arg(sized(memory(r64(input), 0), width))
        } else {
            asm_arg(Register {
                width,
                ..r64(input)
            })
        };
        self.emit(Opcode::parse(op).unwrap(), [operand]);
    }

    pub(super) fn native_descriptor_query(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (op, width, bytes) = id.x86_descriptor_query().expect("descriptor dispatch");
        // Capture the address/old destination before publishing any result.
        // S0 is allocator-reserved; use_reg may rematerialize or load a spill.
        let input = self.use_reg(args[0], S0);
        if bytes != 0 {
            // One hardware write: m80 for SGDT/SIDT, m16 for the others.
            // No aggregate store, padding, helper, permission probe or fence.
            self.emit(
                Opcode::parse(op).unwrap(),
                [asm_arg(sized(memory(r64(input), 0), width))],
            );
        } else {
            if input != S0 {
                self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(input))]);
            }
            // Retain the actual operand size. r16 preserves bits63:16 of the
            // old container; r32 clears bits63:32; r64 is the hardware result.
            // In particular SMSW r32/r64 is not a zero-extended m16 query.
            self.emit(
                Opcode::parse(op).unwrap(),
                [asm_arg(Register { width, ..r64(S0) })],
            );
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
            }
            self.def_done(result, d);
        }
    }
}
