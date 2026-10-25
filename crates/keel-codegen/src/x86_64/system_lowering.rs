//! Shared execution of reviewed, generated system-intrinsic recipes.
//! Privilege, architectural state and continuation remain caller-owned.
use super::*;

include!("generated/system_lowering.rs");

impl FunctionCompiler<'_> {
    pub(super) fn native_generated_system(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        match recipe(id).expect("generated system dispatch") {
            SystemLowering::Fixed(opcode) => self.emit(opcode, []),
            SystemLowering::SystemRegister { register, write } => {
                if write {
                    let input = self.use_reg(args[0], S0);
                    self.emit(Opcode::Mov, [asm_arg(register), asm_arg(r64(input))]);
                } else {
                    let output = self.def_reg(result);
                    self.emit(Opcode::Mov, [asm_arg(r64(output)), asm_arg(register)]);
                    self.def_done(result, output);
                }
            }
            SystemLowering::Unary {
                opcode,
                bits,
                operand,
            } => {
                let input = self.use_reg(args[0], S0);
                let operand = match operand {
                    SystemOperand::Register => asm_arg(Register {
                        width: bits,
                        ..r64(input)
                    }),
                    // AddressOnly encodes a memory-shaped address without claiming a data read.
                    // The IR compiler memory effect is declared separately from this syntax.
                    SystemOperand::MemoryRead | SystemOperand::AddressOnly => {
                        asm_arg(sized(memory(r64(input), 0), bits))
                    }
                };
                self.emit(opcode, [operand]);
            }
            SystemLowering::AdjacentFixed([first, second]) => {
                // No allocation, result publication, spills or labels between these emissions.
                self.emit(first, []);
                self.emit(second, []);
            }
            SystemLowering::HandwrittenMsr(handler) => {
                self.native_cpu_control(handler, args, result)
            }
        }
    }
}
