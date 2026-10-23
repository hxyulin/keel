//! Exactly one raw principal opcode. No numerical F80 homes or SW snapshot.
//! Caller owns the coherent physical stack until an explicit region terminator.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_owned_x87_stack(&mut self, id: Intrinsic) {
        let op = id.x86_owned_x87_stack().expect("owned stack opcode");
        let opcode = Opcode::parse(op.mnemonic).expect("baseline x87 opcode");
        if let Some(index) = op.stack_index {
            let register = Register {
                class: RegisterClass::X87,
                number: index,
                width: 80,
                high_byte: false,
            };
            self.emit(opcode, [asm_arg(register)]);
        } else {
            self.emit(opcode, []);
        }
    }
}
