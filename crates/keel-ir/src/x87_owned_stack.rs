//! Principal-opcode leaves inside the caller-owned legacy x87 region.
//! Registry appends are owned by root; descriptors do not allocate IDs.
use crate::Intrinsic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedX87StackOp {
    pub name: &'static str,
    pub mnemonic: &'static str,
    pub stack_index: Option<u8>,
    pub bytes: [u8; 2],
    pub top_delta: i8,
    pub defined_sw_mask: u16,
}

const fn free(name: &'static str, i: u8) -> OwnedX87StackOp {
    OwnedX87StackOp {
        name,
        mnemonic: "ffree",
        stack_index: Some(i),
        bytes: [0xdd, 0xc0 + i],
        top_delta: 0,
        defined_sw_mask: 0xb8ff,
    }
}

/// Fixed logical ST(i) selectors, never an unchecked runtime stack index.
pub const OWNED_X87_STACK_OPS: [OwnedX87StackOp; 11] = [
    free("x86_x87_ffree0", 0),
    free("x86_x87_ffree1", 1),
    free("x86_x87_ffree2", 2),
    free("x86_x87_ffree3", 3),
    free("x86_x87_ffree4", 4),
    free("x86_x87_ffree5", 5),
    free("x86_x87_ffree6", 6),
    free("x86_x87_ffree7", 7),
    OwnedX87StackOp {
        name: "x86_x87_fdecstp",
        mnemonic: "fdecstp",
        stack_index: None,
        bytes: [0xd9, 0xf6],
        top_delta: -1,
        defined_sw_mask: 0xbaff,
    },
    OwnedX87StackOp {
        name: "x86_x87_fincstp",
        mnemonic: "fincstp",
        stack_index: None,
        bytes: [0xd9, 0xf7],
        top_delta: 1,
        defined_sw_mask: 0xbaff,
    },
    OwnedX87StackOp {
        name: "x86_x87_fnop",
        mnemonic: "fnop",
        stack_index: None,
        bytes: [0xd9, 0xd0],
        top_delta: 0,
        defined_sw_mask: 0xb8ff,
    },
];

impl Intrinsic {
    /// Zero arguments/results, impure Barrier; no F80 SSA or hidden stack work.
    /// Physical tag/TOP/CW/SW and pointer behavior is that of the actual opcode.
    /// No universal current-FOP/FDP or selector retention is synthesized.
    pub fn x86_owned_x87_stack(self) -> Option<&'static OwnedX87StackOp> {
        let index = match self {
            Self::X86X87Ffree0 => 0,
            Self::X86X87Ffree1 => 1,
            Self::X86X87Ffree2 => 2,
            Self::X86X87Ffree3 => 3,
            Self::X86X87Ffree4 => 4,
            Self::X86X87Ffree5 => 5,
            Self::X86X87Ffree6 => 6,
            Self::X86X87Ffree7 => 7,
            Self::X86X87Fdecstp => 8,
            Self::X86X87Fincstp => 9,
            Self::X86X87Fnop => 10,
            _ => return None,
        };
        Some(&OWNED_X87_STACK_OPS[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_roster_has_unique_names_and_two_byte_principals() {
        for (i, op) in OWNED_X87_STACK_OPS.iter().enumerate() {
            assert!(!OWNED_X87_STACK_OPS[..i].iter().any(|p| p.name == op.name));
            if let Some(index) = op.stack_index {
                assert!(index < 8);
                assert_eq!(op.bytes, [0xdd, 0xc0 + index]);
            }
        }
    }
}
