use crate::{FieldId, Instruction, Opcode};
use std::fmt;

/// Symbol-bearing instructions emitted by the compiler. Displacements and
/// linker relocations are resolved by the assembler after layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolicInstruction {
    Conditional {
        condition: u8,
        target: String,
    },
    Compare {
        register: u8,
        is64: bool,
        nonzero: bool,
        target: String,
    },
    Address {
        register: u8,
        target: String,
    },
    PageAddress {
        register: u8,
        kind: PageKind,
        target: String,
    },
    PageOffset {
        register: u8,
        base: u8,
        load: bool,
        kind: PageKind,
        target: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Direct,
    Got,
    ThreadLocal,
}
impl SymbolicInstruction {
    pub fn target(&self) -> &str {
        match self {
            Self::Conditional { target, .. }
            | Self::Compare { target, .. }
            | Self::Address { target, .. }
            | Self::PageAddress { target, .. }
            | Self::PageOffset { target, .. } => target,
        }
    }
    pub fn zero_instruction(&self) -> Instruction {
        use FieldId as F;
        use Opcode as O;
        match *self {
            Self::Conditional { condition, .. } => Instruction::new(
                O::B_only_condbranch,
                [(F::cond, condition as u32), (F::imm19, 0)],
            ),
            Self::Compare {
                register,
                is64,
                nonzero,
                ..
            } => Instruction::new(
                match (is64, nonzero) {
                    (true, true) => O::CBNZ_64_compbranch,
                    (true, false) => O::CBZ_64_compbranch,
                    (false, true) => O::CBNZ_32_compbranch,
                    _ => O::CBZ_32_compbranch,
                },
                [(F::Rt, register as u32), (F::imm19, 0)],
            ),
            Self::Address { register, .. } => Instruction::new(
                O::ADR_only_pcreladdr,
                [(F::Rd, register as u32), (F::immlo, 0), (F::immhi, 0)],
            ),
            Self::PageAddress { register, .. } => Instruction::new(
                O::ADRP_only_pcreladdr,
                [(F::Rd, register as u32), (F::immlo, 0), (F::immhi, 0)],
            ),
            Self::PageOffset {
                register,
                base,
                load,
                ..
            } => {
                if load {
                    Instruction::new(
                        O::LDR_64_ldst_pos,
                        [
                            (F::Rt, register as u32),
                            (F::Rn, base as u32),
                            (F::imm12, 0),
                        ],
                    )
                } else {
                    Instruction::new(
                        O::ADD_64_addsub_imm,
                        [
                            (F::Rd, register as u32),
                            (F::Rn, base as u32),
                            (F::imm12, 0),
                            (F::sh, 0),
                        ],
                    )
                }
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Conditional { condition, .. } = self
            && *condition > 15
        {
            return Err("invalid branch condition".into());
        }
        if let Self::PageOffset {
            load: false, kind, ..
        } = self
            && *kind != PageKind::Direct
        {
            return Err("GOT/TLVP page offsets require a load".into());
        }
        Ok(())
    }
    pub fn inverted_to(&self, target: String) -> Option<Self> {
        match *self {
            Self::Conditional { condition, .. } => Some(Self::Conditional {
                condition: condition ^ 1,
                target,
            }),
            Self::Compare {
                register,
                is64,
                nonzero,
                ..
            } => Some(Self::Compare {
                register,
                is64,
                nonzero: !nonzero,
                target,
            }),
            _ => None,
        }
    }
}
impl fmt::Display for SymbolicInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = |kind: PageKind, offset| match (kind, offset) {
            (PageKind::Direct, false) => "PAGE",
            (PageKind::Direct, true) => "PAGEOFF",
            (PageKind::Got, false) => "GOTPAGE",
            (PageKind::Got, true) => "GOTPAGEOFF",
            (PageKind::ThreadLocal, false) => "TLVPPAGE",
            (PageKind::ThreadLocal, true) => "TLVPPAGEOFF",
        };
        match self {
            Self::Conditional { condition, target } => write!(
                f,
                "b.{}\t{target}",
                [
                    "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt",
                    "le", "al", "nv"
                ][*condition as usize]
            ),
            Self::Compare {
                register,
                is64,
                nonzero,
                target,
            } => write!(
                f,
                "{}\t{}{register}, {target}",
                if *nonzero { "cbnz" } else { "cbz" },
                if *is64 { 'x' } else { 'w' }
            ),
            Self::Address { register, target } => write!(f, "adr\tx{register}, {target}"),
            Self::PageAddress {
                register,
                kind,
                target,
            } => write!(f, "adrp\tx{register}, {target}@{}", suffix(*kind, false)),
            Self::PageOffset {
                register,
                base,
                load,
                kind,
                target,
            } => {
                if *load {
                    write!(
                        f,
                        "ldr\tx{register}, [x{base}, {target}@{}]",
                        suffix(*kind, true)
                    )
                } else {
                    write!(
                        f,
                        "add\tx{register}, x{base}, {target}@{}",
                        suffix(*kind, true)
                    )
                }
            }
        }
    }
}
