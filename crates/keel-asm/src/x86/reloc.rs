//! x86-64 ELF relocation kinds the assembler emits.
use object::elf;

/// An x86-64 ELF relocation kind. The discriminant is the ELF `r_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum RelocKind {
    /// `R_X86_64_64`: S + A.
    Abs64 = elf::R_X86_64_64 as u8,
    /// `R_X86_64_PC32`: S + A - P.
    Pc32 = elf::R_X86_64_PC32 as u8,
    /// `R_X86_64_PLT32`: L + A - P.
    Plt32 = elf::R_X86_64_PLT32 as u8,
    /// `R_X86_64_GOTPCREL`: G + GOT + A - P.
    GotPcRel = elf::R_X86_64_GOTPCREL as u8,
    /// `R_X86_64_32`: S + A, zero-extended.
    Abs32 = elf::R_X86_64_32 as u8,
    /// `R_X86_64_32S`: S + A, sign-extended.
    Abs32S = elf::R_X86_64_32S as u8,
    /// `R_X86_64_16`: S + A.
    Abs16 = elf::R_X86_64_16 as u8,
    /// `R_X86_64_8`: S + A.
    Abs8 = elf::R_X86_64_8 as u8,
    /// `R_X86_64_PC8`: S + A - P.
    Pc8 = elf::R_X86_64_PC8 as u8,
    /// `R_X86_64_DTPOFF64`: offset in the module's TLS block.
    DtpOff64 = elf::R_X86_64_DTPOFF64 as u8,
    /// `R_X86_64_GOTTPOFF`: PC-relative GOT entry holding the TP offset.
    GotTpOff = elf::R_X86_64_GOTTPOFF as u8,
    /// `R_X86_64_TPOFF32`: offset from the thread pointer.
    TpOff32 = elf::R_X86_64_TPOFF32 as u8,
}

const ALL: [RelocKind; 12] = [
    RelocKind::Abs64,
    RelocKind::Pc32,
    RelocKind::Plt32,
    RelocKind::GotPcRel,
    RelocKind::Abs32,
    RelocKind::Abs32S,
    RelocKind::Abs16,
    RelocKind::Abs8,
    RelocKind::Pc8,
    RelocKind::DtpOff64,
    RelocKind::GotTpOff,
    RelocKind::TpOff32,
];

impl RelocKind {
    /// The ELF `r_type` value.
    pub const fn r_type(self) -> u32 {
        self as u32
    }
    pub fn from_r_type(r_type: u32) -> Option<Self> {
        ALL.into_iter().find(|k| k.r_type() == r_type)
    }
    /// The ELF specification name, such as `R_X86_64_PLT32`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Abs64 => "R_X86_64_64",
            Self::Pc32 => "R_X86_64_PC32",
            Self::Plt32 => "R_X86_64_PLT32",
            Self::GotPcRel => "R_X86_64_GOTPCREL",
            Self::Abs32 => "R_X86_64_32",
            Self::Abs32S => "R_X86_64_32S",
            Self::Abs16 => "R_X86_64_16",
            Self::Abs8 => "R_X86_64_8",
            Self::Pc8 => "R_X86_64_PC8",
            Self::DtpOff64 => "R_X86_64_DTPOFF64",
            Self::GotTpOff => "R_X86_64_GOTTPOFF",
            Self::TpOff32 => "R_X86_64_TPOFF32",
        }
    }
    /// Whether the field counts from the end of the instruction (P).
    pub(super) const fn pc_relative(self) -> bool {
        matches!(
            self,
            Self::Pc32 | Self::Plt32 | Self::GotPcRel | Self::Pc8 | Self::GotTpOff
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_type_round_trips() {
        for kind in ALL {
            assert_eq!(RelocKind::from_r_type(kind.r_type()), Some(kind));
        }
        assert_eq!(RelocKind::from_r_type(0), None);
        assert_eq!(RelocKind::from_r_type(elf::R_X86_64_GOTPCRELX), None);
    }
}
