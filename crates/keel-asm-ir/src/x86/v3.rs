//! Stable Keel v3 identities and matching API. Encoding forms are generated
//! from the shared pinned ISA catalogue by tools/x86-isa/generate.py.
use super::{Features, Opcode, Operand, RegisterClass};
include!("isa/v3_opcodes.rs");
impl V3Opcode {
    pub fn required_features(self) -> Features {
        super::isa::opcode_features(Opcode::V3(self))
    }
    pub fn with_rex_w(self) -> Option<Self> {
        Some(match self {
            Self::Vpcmpestri => Self::Vpcmpestri64,
            Self::Vpcmpestrm => Self::Vpcmpestrm64,
            Self::Vpcmpistri => Self::Vpcmpistri64,
            _ => return None,
        })
    }
}
pub fn requirement(isa: &str) -> Features {
    super::isa::feature_policy(isa)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V3Operand {
    X,
    Y,
    D,
    Q,
    M(u16),
    I,
}
impl V3Operand {
    pub fn matches(self, o: &Operand) -> bool {
        use V3Operand::*;
        match (self, o) {
            (X, Operand::Register(r)) => r.class == RegisterClass::Xmm,
            (Y, Operand::Register(r)) => r.class == RegisterClass::Ymm,
            (D, Operand::Register(r)) => r.class == RegisterClass::Gpr && r.width == 32,
            (Q, Operand::Register(r)) => r.class == RegisterClass::Gpr && r.width == 64,
            (M(w), Operand::Memory(m)) => m.width == 0 || m.width == w,
            (I, Operand::Immediate(_)) => true,
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct V3Form {
    pub opcode: Opcode,
    pub isa: &'static str,
    pub map: u8,
    pub pp: u8,
    pub byte: u8,
    pub l: bool,
    pub w: bool,
    pub operands: &'static [V3Operand],
    pub field: i8,
    pub digit: u8,
    pub source: i8,
    pub rm: i8,
    pub imm: i8,
    pub imm_reg: bool,
    pub vsib: u16,
}
// Capture one borrowed view in the form scan, as the owned matcher did.
struct InstructionRef<'a> {
    opcode: Opcode,
    operands: &'a [Operand],
}
impl V3Form {
    #[inline]
    pub fn matches(&self, i: &super::Instruction) -> bool {
        self.matches_operands(&InstructionRef {
            opcode: i.opcode,
            operands: &i.operands,
        })
    }

    fn matches_operands(&self, i: &InstructionRef<'_>) -> bool {
        self.opcode == i.opcode
            && self.operands.len() == i.operands.len()
            && self.operands.iter().zip(i.operands).all(|(s, o)| s.matches(o))
            && (self.vsib == 0
                || i.operands.iter().any(|o| matches!(o, Operand::Memory(m) if m.index.is_some_and(|r|r.width==self.vsib && matches!(r.class,RegisterClass::Xmm | RegisterClass::Ymm)))))
    }
}
pub(super) fn form(opcode: Opcode, operands: &[Operand]) -> Option<&'static V3Form> {
    let i = InstructionRef { opcode, operands };
    V3_FORMS
        .iter()
        .filter(|f| f.matches_operands(&i))
        .min_by_key(|f| (u8::from(f.w), f.map))
}
pub use super::isa::V3_FORMS;
