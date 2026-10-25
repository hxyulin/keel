//! Legacy x86-64-v2 instruction identities and encoding inventory.
//! Intel SDM Vol. 2 is authoritative; legacy XMM/GPR and SSSE3 MMX forms, no VEX/YMM.
use super::Features;

include!("isa/v2_opcodes.rs");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V2Form {
    Binary,
    Control,
    Control64,
    Blend,
    Memory,
    Extract,
    Insert,
    Crc32,
    Fisttp,
    Cx16,
    Nullary,
}
#[derive(Clone, Copy, Debug)]
pub struct V2Spec {
    pub mnemonic: &'static str,
    pub family: &'static str,
    pub prefix: u8,
    /// 0x0f = two-byte map; 0x38/0x3a = three-byte maps.
    pub map: u8,
    pub byte: u8,
    /// Zero for variable-width and nullary forms; see V2Form.
    pub memory_bits: u16,
    pub form: V2Form,
}
impl V2Opcode {
    /// All sixteen legacy SSSE3 identities have both MMX/m64 and XMM/m128
    /// forms. The opcode map/byte and feature requirement are shared; MMX
    /// omits the XMM form's mandatory 66 prefix.
    pub fn supports_mmx(self) -> bool {
        self.spec().family == "ssse3"
    }

    pub fn spec(self) -> V2Spec {
        super::isa::v2::spec(self)
    }
    /// Only PCMP string forms accept an explicit REX.W prefix here.
    pub fn with_rex_w(self) -> Option<Self> {
        Some(match self {
            Self::Pcmpestri | Self::Pcmpestri64 => Self::Pcmpestri64,
            Self::Pcmpestrm | Self::Pcmpestrm64 => Self::Pcmpestrm64,
            Self::Pcmpistri | Self::Pcmpistri64 => Self::Pcmpistri64,
            Self::Pcmpistrm | Self::Pcmpistrm64 => Self::Pcmpistrm64,
            _ => return None,
        })
    }
    pub fn required_features(self) -> Features {
        super::isa::opcode_features(super::Opcode::V2(self))
    }
}
