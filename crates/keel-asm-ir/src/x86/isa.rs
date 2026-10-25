//! Shared generated x86 instruction forms. Compiler semantics are separate.
mod features;
mod generated;
mod register_names;
pub(super) use features::{feature_policy, opcode_features};
pub use register_names::register_name;
pub(super) mod fixed;
pub(super) mod v2;
mod v3;
pub use generated::FORMS;
pub use v3::V3_FORMS;

#[derive(Clone, Copy, Debug)]
pub struct IsaOperand {
    pub kind: &'static str,
    pub encoding: &'static str,
    pub register: &'static str,
    /// Encoded memory/immediate widths in bytes for operand sizes 16/32/64.
    pub widths: [u16; 3],
}
impl IsaOperand {
    pub fn bytes(self, size: u16) -> u16 {
        self.widths[match size {
            16 => 0,
            64 => 2,
            _ => 1,
        }]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IsaForm {
    pub mnemonic: &'static str,
    pub vex: bool,
    pub map: u8,
    pub byte: u8,
    /// -1 means ignored, -2 means any except F3, otherwise the required prefix.
    pub prefix: i16,
    pub sizes: u8,
    pub addresses: u8,
    pub osz: &'static str,
    pub mod_mask: u8,
    pub reg_mask: u8,
    pub rm_mask: u8,
    pub w: i8,
    pub b: i8,
    pub vector: u16,
    pub lock: bool,
    pub rep: bool,
    pub repne: bool,
    pub gather: bool,
    pub ignore_mod: bool,
    pub far: bool,
    pub decode: bool,
    pub group1: i8,
    pub operands: &'static [IsaOperand],
    pub isa: &'static str,
    pub extension: &'static str,
    /// Upstream's explicit privilege metadata, when present. Absence is unknown.
    pub cpl: Option<u8>,
}
impl IsaForm {
    pub fn allows_size(self, bits: u16) -> bool {
        self.sizes
            & match bits {
                16 => 1,
                8 | 32 => 2,
                64 => 4,
                _ => 0,
            }
            != 0
    }
    /// Effective operand size after interpreting legacy 66 and REX.W.
    pub fn operand_size(self, p66: bool, w: bool) -> u16 {
        match self.osz {
            "byteop" => 8,
            "force64" => 64,
            "default64" => {
                if p66 {
                    16
                } else {
                    64
                }
            }
            "force32_or_64" => 64,
            "ignore66" | "force32_or_rexw" => {
                if w {
                    64
                } else {
                    32
                }
            }
            _ => {
                if w {
                    64
                } else if p66 && self.prefix != 0x66 {
                    16
                } else {
                    32
                }
            }
        }
    }
    pub fn opcode(self) -> ([u8; 3], usize) {
        match self.map {
            0 => ([self.byte, 0, 0], 1),
            1 => ([0x0f, self.byte, 0], 2),
            2 => ([0x0f, 0x38, self.byte], 3),
            3 => ([0x0f, 0x3a, self.byte], 3),
            _ => unreachable!(),
        }
    }
}

pub fn forms(mnemonic: &str) -> &'static [IsaForm] {
    let start = FORMS.partition_point(|f| f.mnemonic < mnemonic);
    let end = start + FORMS[start..].partition_point(|f| f.mnemonic == mnemonic);
    &FORMS[start..end]
}

pub fn decode_forms(vex: bool, map: u8, byte: u8) -> impl Iterator<Item = &'static IsaForm> {
    let indices = if map < 4 {
        let (start, end) = generated::DECODE_RANGES
            [usize::from(vex) * 1024 + usize::from(map) * 256 + usize::from(byte)];
        &generated::DECODE_ORDER[usize::from(start)..usize::from(end)]
    } else {
        &[]
    };
    indices.iter().map(|index| &FORMS[usize::from(*index)])
}
