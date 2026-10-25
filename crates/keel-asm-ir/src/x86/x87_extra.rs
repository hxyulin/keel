//! Additional architectural x87 stack, integer-memory, and state operations.
include!("isa/x87_opcodes.rs");

impl X87Opcode {
    pub(super) fn environment(self) -> bool {
        matches!(
            self,
            Self::Fldenv | Self::Fstenv | Self::Fnstenv | Self::Fsave | Self::Fnsave | Self::Frstor
        )
    }
    pub(super) fn from_att(self, args: &[super::Operand]) -> Self {
        use super::{Operand, RegisterClass};
        // GAS/LLVM retain the historical reversed subtraction/division spelling
        // for ST(i)-destination forms. The shared model uses Intel semantics.
        let other_destination = matches!(args, [Operand::Register(a), Operand::Register(b)]
            if a.class == RegisterClass::X87 && a.number != 0
                && b.class == RegisterClass::X87 && b.number == 0);
        match self {
            Self::Fdivp => Self::Fdivrp,
            Self::Fdivrp => Self::Fdivp,
            Self::Fsub if other_destination => Self::Fsubr,
            Self::Fsubr if other_destination => Self::Fsub,
            Self::Fdiv if other_destination => Self::Fdivr,
            Self::Fdivr if other_destination => Self::Fdiv,
            other => other,
        }
    }
}

impl super::Instruction {
    pub(super) fn x87_data16(mut self) -> Result<Self, String> {
        if !self.lock && matches!(self.opcode, super::Opcode::X87(op) if op.environment()) {
            if let [super::Operand::Memory(m)] = self.operands.as_mut_slice() {
                if m.width == 0 {
                    m.width = 16;
                    return Ok(self);
                }
            }
        }
        Err("data16 requires an unsized x87 environment/state memory operand".into())
    }
}
