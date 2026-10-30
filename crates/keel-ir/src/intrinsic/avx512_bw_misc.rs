//! BW absolute value, scalar broadcasts, blends and byte SAD operations.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512BwMiscKind {
    Abs,
    Broadcast,
    Blend,
    Sad,
    Dbsad,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512BwMisc {
    pub name: &'static str,
    pub kind: X86Avx512BwMiscKind,
    pub result: VectorType,
    pub scalar: Type,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    pub fn x86_avx512_bw_misc(self) -> Option<X86Avx512BwMisc> {
        macro_rules! descriptor {
            ($name:literal,$kind:ident,$result:ident,$byte:ident,$mask:ident,$scalar:ident,$opcode:literal) => {{
                const R: Type = Type::Vector(VectorType::$result);
                const B: Type = Type::Vector(VectorType::$byte);
                const M: Type = Type::$mask;
                const S: Type = Type::$scalar;
                use X86Avx512BwMiscKind::*;
                X86Avx512BwMisc {
                    name: $name,
                    kind: $kind,
                    result: VectorType::$result,
                    scalar: S,
                    opcode: $opcode,
                    params: match $kind {
                        Abs => &[R, M, R],
                        Broadcast => &[R, M, S],
                        Blend => &[M, R, R],
                        Sad => &[B, B],
                        Dbsad => &[R, M, B, B, Type::I32],
                    },
                    returns: &[R],
                }
            }};
        }
        Some(match self {
            Self::X86Avx512BwPabsb128 => descriptor!(
                "x86_avx512bw_pabsb128",
                Abs,
                I8x16,
                I8x16,
                I16,
                I8,
                "vpabsb"
            ),
            Self::X86Avx512BwPabsw128 => descriptor!(
                "x86_avx512bw_pabsw128",
                Abs,
                I16x8,
                I8x16,
                I8,
                I16,
                "vpabsw"
            ),
            Self::X86Avx512BwBroadcastb128 => descriptor!(
                "x86_avx512bw_broadcastb128",
                Broadcast,
                I8x16,
                I8x16,
                I16,
                I8,
                "vpbroadcastb"
            ),
            Self::X86Avx512BwBroadcastw128 => descriptor!(
                "x86_avx512bw_broadcastw128",
                Broadcast,
                I16x8,
                I8x16,
                I8,
                I16,
                "vpbroadcastw"
            ),
            Self::X86Avx512BwBlendb128 => descriptor!(
                "x86_avx512bw_blendb128",
                Blend,
                I8x16,
                I8x16,
                I16,
                I8,
                "vpblendmb"
            ),
            Self::X86Avx512BwBlendw128 => descriptor!(
                "x86_avx512bw_blendw128",
                Blend,
                I16x8,
                I8x16,
                I8,
                I16,
                "vpblendmw"
            ),
            Self::X86Avx512BwPsadbw128 => descriptor!(
                "x86_avx512bw_psadbw128",
                Sad,
                I64x2,
                I8x16,
                I8,
                I8,
                "vpsadbw"
            ),
            Self::X86Avx512BwDbsad128 => descriptor!(
                "x86_avx512bw_dbsad128",
                Dbsad,
                I16x8,
                I8x16,
                I8,
                I8,
                "vdbpsadbw"
            ),
            Self::X86Avx512BwPabsb256 => descriptor!(
                "x86_avx512bw_pabsb256",
                Abs,
                I8x32,
                I8x32,
                I32,
                I8,
                "vpabsb"
            ),
            Self::X86Avx512BwPabsw256 => descriptor!(
                "x86_avx512bw_pabsw256",
                Abs,
                I16x16,
                I8x32,
                I16,
                I16,
                "vpabsw"
            ),
            Self::X86Avx512BwBroadcastb256 => descriptor!(
                "x86_avx512bw_broadcastb256",
                Broadcast,
                I8x32,
                I8x32,
                I32,
                I8,
                "vpbroadcastb"
            ),
            Self::X86Avx512BwBroadcastw256 => descriptor!(
                "x86_avx512bw_broadcastw256",
                Broadcast,
                I16x16,
                I8x32,
                I16,
                I16,
                "vpbroadcastw"
            ),
            Self::X86Avx512BwBlendb256 => descriptor!(
                "x86_avx512bw_blendb256",
                Blend,
                I8x32,
                I8x32,
                I32,
                I8,
                "vpblendmb"
            ),
            Self::X86Avx512BwBlendw256 => descriptor!(
                "x86_avx512bw_blendw256",
                Blend,
                I16x16,
                I8x32,
                I16,
                I16,
                "vpblendmw"
            ),
            Self::X86Avx512BwPsadbw256 => descriptor!(
                "x86_avx512bw_psadbw256",
                Sad,
                I64x4,
                I8x32,
                I8,
                I8,
                "vpsadbw"
            ),
            Self::X86Avx512BwDbsad256 => descriptor!(
                "x86_avx512bw_dbsad256",
                Dbsad,
                I16x16,
                I8x32,
                I16,
                I8,
                "vdbpsadbw"
            ),
            Self::X86Avx512BwPabsb512 => descriptor!(
                "x86_avx512bw_pabsb512",
                Abs,
                I8x64,
                I8x64,
                I64,
                I8,
                "vpabsb"
            ),
            Self::X86Avx512BwPabsw512 => descriptor!(
                "x86_avx512bw_pabsw512",
                Abs,
                I16x32,
                I8x64,
                I32,
                I16,
                "vpabsw"
            ),
            Self::X86Avx512BwBroadcastb512 => descriptor!(
                "x86_avx512bw_broadcastb512",
                Broadcast,
                I8x64,
                I8x64,
                I64,
                I8,
                "vpbroadcastb"
            ),
            Self::X86Avx512BwBroadcastw512 => descriptor!(
                "x86_avx512bw_broadcastw512",
                Broadcast,
                I16x32,
                I8x64,
                I32,
                I16,
                "vpbroadcastw"
            ),
            Self::X86Avx512BwBlendb512 => descriptor!(
                "x86_avx512bw_blendb512",
                Blend,
                I8x64,
                I8x64,
                I64,
                I8,
                "vpblendmb"
            ),
            Self::X86Avx512BwBlendw512 => descriptor!(
                "x86_avx512bw_blendw512",
                Blend,
                I16x32,
                I8x64,
                I32,
                I16,
                "vpblendmw"
            ),
            Self::X86Avx512BwPsadbw512 => descriptor!(
                "x86_avx512bw_psadbw512",
                Sad,
                I64x8,
                I8x64,
                I8,
                I8,
                "vpsadbw"
            ),
            Self::X86Avx512BwDbsad512 => descriptor!(
                "x86_avx512bw_dbsad512",
                Dbsad,
                I16x32,
                I8x64,
                I32,
                I8,
                "vdbpsadbw"
            ),
            _ => return None,
        })
    }
}
