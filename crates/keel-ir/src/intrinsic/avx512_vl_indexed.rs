//! Packed VL VSIB forms retain explicit encoded width and short-result semantics.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlIndexed {
    pub name: &'static str,
    pub opcode: &'static str,
    pub encoded_bits: u16,
    pub index: VectorType,
    pub data: VectorType,
    pub active_lanes: u8,
    pub scatter: bool,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// AVX512F plus AVX512VL signed-index memory operation with a scalar mask.
    pub fn x86_avx512_vl_indexed(self) -> Option<X86Avx512VlIndexed> {
        macro_rules! descriptor {
            ($name:literal,$opcode:literal,$bits:literal,$index:ident,$data:ident,$lanes:literal,$scatter:literal) => {{
                const D: Type = Type::Vector(VectorType::$data);
                X86Avx512VlIndexed {
                    name: $name,
                    opcode: $opcode,
                    encoded_bits: $bits,
                    index: VectorType::$index,
                    data: VectorType::$data,
                    active_lanes: $lanes,
                    scatter: $scatter,
                    params: &[
                        Type::Ptr,
                        Type::Vector(VectorType::$index),
                        D,
                        Type::I8,
                        Type::I32,
                    ],
                    returns: if $scatter { &[] } else { &[D] },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlVpgatherdd128 => descriptor!(
                "x86_avx512vl_vpgatherdd128",
                "vpgatherdd",
                128,
                I32x4,
                I32x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterdd128 => descriptor!(
                "x86_avx512vl_vpscatterdd128",
                "vpscatterdd",
                128,
                I32x4,
                I32x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherdps128 => descriptor!(
                "x86_avx512vl_vgatherdps128",
                "vgatherdps",
                128,
                I32x4,
                F32x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterdps128 => descriptor!(
                "x86_avx512vl_vscatterdps128",
                "vscatterdps",
                128,
                I32x4,
                F32x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherdq128 => descriptor!(
                "x86_avx512vl_vpgatherdq128",
                "vpgatherdq",
                128,
                I32x4,
                I64x2,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterdq128 => descriptor!(
                "x86_avx512vl_vpscatterdq128",
                "vpscatterdq",
                128,
                I32x4,
                I64x2,
                2,
                true
            ),
            Self::X86Avx512VlVgatherdpd128 => descriptor!(
                "x86_avx512vl_vgatherdpd128",
                "vgatherdpd",
                128,
                I32x4,
                F64x2,
                2,
                false
            ),
            Self::X86Avx512VlVscatterdpd128 => descriptor!(
                "x86_avx512vl_vscatterdpd128",
                "vscatterdpd",
                128,
                I32x4,
                F64x2,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherqd128 => descriptor!(
                "x86_avx512vl_vpgatherqd128",
                "vpgatherqd",
                128,
                I64x2,
                I32x4,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterqd128 => descriptor!(
                "x86_avx512vl_vpscatterqd128",
                "vpscatterqd",
                128,
                I64x2,
                I32x4,
                2,
                true
            ),
            Self::X86Avx512VlVgatherqps128 => descriptor!(
                "x86_avx512vl_vgatherqps128",
                "vgatherqps",
                128,
                I64x2,
                F32x4,
                2,
                false
            ),
            Self::X86Avx512VlVscatterqps128 => descriptor!(
                "x86_avx512vl_vscatterqps128",
                "vscatterqps",
                128,
                I64x2,
                F32x4,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherqq128 => descriptor!(
                "x86_avx512vl_vpgatherqq128",
                "vpgatherqq",
                128,
                I64x2,
                I64x2,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterqq128 => descriptor!(
                "x86_avx512vl_vpscatterqq128",
                "vpscatterqq",
                128,
                I64x2,
                I64x2,
                2,
                true
            ),
            Self::X86Avx512VlVgatherqpd128 => descriptor!(
                "x86_avx512vl_vgatherqpd128",
                "vgatherqpd",
                128,
                I64x2,
                F64x2,
                2,
                false
            ),
            Self::X86Avx512VlVscatterqpd128 => descriptor!(
                "x86_avx512vl_vscatterqpd128",
                "vscatterqpd",
                128,
                I64x2,
                F64x2,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherdd256 => descriptor!(
                "x86_avx512vl_vpgatherdd256",
                "vpgatherdd",
                256,
                I32x8,
                I32x8,
                8,
                false
            ),
            Self::X86Avx512VlVpscatterdd256 => descriptor!(
                "x86_avx512vl_vpscatterdd256",
                "vpscatterdd",
                256,
                I32x8,
                I32x8,
                8,
                true
            ),
            Self::X86Avx512VlVgatherdps256 => descriptor!(
                "x86_avx512vl_vgatherdps256",
                "vgatherdps",
                256,
                I32x8,
                F32x8,
                8,
                false
            ),
            Self::X86Avx512VlVscatterdps256 => descriptor!(
                "x86_avx512vl_vscatterdps256",
                "vscatterdps",
                256,
                I32x8,
                F32x8,
                8,
                true
            ),
            Self::X86Avx512VlVpgatherdq256 => descriptor!(
                "x86_avx512vl_vpgatherdq256",
                "vpgatherdq",
                256,
                I32x4,
                I64x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterdq256 => descriptor!(
                "x86_avx512vl_vpscatterdq256",
                "vpscatterdq",
                256,
                I32x4,
                I64x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherdpd256 => descriptor!(
                "x86_avx512vl_vgatherdpd256",
                "vgatherdpd",
                256,
                I32x4,
                F64x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterdpd256 => descriptor!(
                "x86_avx512vl_vscatterdpd256",
                "vscatterdpd",
                256,
                I32x4,
                F64x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherqd256 => descriptor!(
                "x86_avx512vl_vpgatherqd256",
                "vpgatherqd",
                256,
                I64x4,
                I32x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterqd256 => descriptor!(
                "x86_avx512vl_vpscatterqd256",
                "vpscatterqd",
                256,
                I64x4,
                I32x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherqps256 => descriptor!(
                "x86_avx512vl_vgatherqps256",
                "vgatherqps",
                256,
                I64x4,
                F32x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterqps256 => descriptor!(
                "x86_avx512vl_vscatterqps256",
                "vscatterqps",
                256,
                I64x4,
                F32x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherqq256 => descriptor!(
                "x86_avx512vl_vpgatherqq256",
                "vpgatherqq",
                256,
                I64x4,
                I64x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterqq256 => descriptor!(
                "x86_avx512vl_vpscatterqq256",
                "vpscatterqq",
                256,
                I64x4,
                I64x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherqpd256 => descriptor!(
                "x86_avx512vl_vgatherqpd256",
                "vgatherqpd",
                256,
                I64x4,
                F64x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterqpd256 => descriptor!(
                "x86_avx512vl_vscatterqpd256",
                "vscatterqpd",
                256,
                I64x4,
                F64x4,
                4,
                true
            ),
            _ => return None,
        })
    }
}
