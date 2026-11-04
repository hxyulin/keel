//! Packed VL VSIB forms retain explicit encoded width and short-result semantics.
use super::{Intrinsic, Type, VectorType};
use keel_asm_ir::x86::EvexMnemonic;
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlIndexed {
    pub name: &'static str,
    pub opcode: EvexMnemonic,
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
            ($name:literal,$opcode:ident,$bits:literal,$index:ident,$data:ident,$lanes:literal,$scatter:literal) => {{
                const D: Type = Type::Vector(VectorType::$data);
                X86Avx512VlIndexed {
                    name: $name,
                    opcode: EvexMnemonic::$opcode,
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
                Vpgatherdd,
                128,
                I32x4,
                I32x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterdd128 => descriptor!(
                "x86_avx512vl_vpscatterdd128",
                Vpscatterdd,
                128,
                I32x4,
                I32x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherdps128 => descriptor!(
                "x86_avx512vl_vgatherdps128",
                Vgatherdps,
                128,
                I32x4,
                F32x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterdps128 => descriptor!(
                "x86_avx512vl_vscatterdps128",
                Vscatterdps,
                128,
                I32x4,
                F32x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherdq128 => descriptor!(
                "x86_avx512vl_vpgatherdq128",
                Vpgatherdq,
                128,
                I32x4,
                I64x2,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterdq128 => descriptor!(
                "x86_avx512vl_vpscatterdq128",
                Vpscatterdq,
                128,
                I32x4,
                I64x2,
                2,
                true
            ),
            Self::X86Avx512VlVgatherdpd128 => descriptor!(
                "x86_avx512vl_vgatherdpd128",
                Vgatherdpd,
                128,
                I32x4,
                F64x2,
                2,
                false
            ),
            Self::X86Avx512VlVscatterdpd128 => descriptor!(
                "x86_avx512vl_vscatterdpd128",
                Vscatterdpd,
                128,
                I32x4,
                F64x2,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherqd128 => descriptor!(
                "x86_avx512vl_vpgatherqd128",
                Vpgatherqd,
                128,
                I64x2,
                I32x4,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterqd128 => descriptor!(
                "x86_avx512vl_vpscatterqd128",
                Vpscatterqd,
                128,
                I64x2,
                I32x4,
                2,
                true
            ),
            Self::X86Avx512VlVgatherqps128 => descriptor!(
                "x86_avx512vl_vgatherqps128",
                Vgatherqps,
                128,
                I64x2,
                F32x4,
                2,
                false
            ),
            Self::X86Avx512VlVscatterqps128 => descriptor!(
                "x86_avx512vl_vscatterqps128",
                Vscatterqps,
                128,
                I64x2,
                F32x4,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherqq128 => descriptor!(
                "x86_avx512vl_vpgatherqq128",
                Vpgatherqq,
                128,
                I64x2,
                I64x2,
                2,
                false
            ),
            Self::X86Avx512VlVpscatterqq128 => descriptor!(
                "x86_avx512vl_vpscatterqq128",
                Vpscatterqq,
                128,
                I64x2,
                I64x2,
                2,
                true
            ),
            Self::X86Avx512VlVgatherqpd128 => descriptor!(
                "x86_avx512vl_vgatherqpd128",
                Vgatherqpd,
                128,
                I64x2,
                F64x2,
                2,
                false
            ),
            Self::X86Avx512VlVscatterqpd128 => descriptor!(
                "x86_avx512vl_vscatterqpd128",
                Vscatterqpd,
                128,
                I64x2,
                F64x2,
                2,
                true
            ),
            Self::X86Avx512VlVpgatherdd256 => descriptor!(
                "x86_avx512vl_vpgatherdd256",
                Vpgatherdd,
                256,
                I32x8,
                I32x8,
                8,
                false
            ),
            Self::X86Avx512VlVpscatterdd256 => descriptor!(
                "x86_avx512vl_vpscatterdd256",
                Vpscatterdd,
                256,
                I32x8,
                I32x8,
                8,
                true
            ),
            Self::X86Avx512VlVgatherdps256 => descriptor!(
                "x86_avx512vl_vgatherdps256",
                Vgatherdps,
                256,
                I32x8,
                F32x8,
                8,
                false
            ),
            Self::X86Avx512VlVscatterdps256 => descriptor!(
                "x86_avx512vl_vscatterdps256",
                Vscatterdps,
                256,
                I32x8,
                F32x8,
                8,
                true
            ),
            Self::X86Avx512VlVpgatherdq256 => descriptor!(
                "x86_avx512vl_vpgatherdq256",
                Vpgatherdq,
                256,
                I32x4,
                I64x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterdq256 => descriptor!(
                "x86_avx512vl_vpscatterdq256",
                Vpscatterdq,
                256,
                I32x4,
                I64x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherdpd256 => descriptor!(
                "x86_avx512vl_vgatherdpd256",
                Vgatherdpd,
                256,
                I32x4,
                F64x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterdpd256 => descriptor!(
                "x86_avx512vl_vscatterdpd256",
                Vscatterdpd,
                256,
                I32x4,
                F64x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherqd256 => descriptor!(
                "x86_avx512vl_vpgatherqd256",
                Vpgatherqd,
                256,
                I64x4,
                I32x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterqd256 => descriptor!(
                "x86_avx512vl_vpscatterqd256",
                Vpscatterqd,
                256,
                I64x4,
                I32x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherqps256 => descriptor!(
                "x86_avx512vl_vgatherqps256",
                Vgatherqps,
                256,
                I64x4,
                F32x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterqps256 => descriptor!(
                "x86_avx512vl_vscatterqps256",
                Vscatterqps,
                256,
                I64x4,
                F32x4,
                4,
                true
            ),
            Self::X86Avx512VlVpgatherqq256 => descriptor!(
                "x86_avx512vl_vpgatherqq256",
                Vpgatherqq,
                256,
                I64x4,
                I64x4,
                4,
                false
            ),
            Self::X86Avx512VlVpscatterqq256 => descriptor!(
                "x86_avx512vl_vpscatterqq256",
                Vpscatterqq,
                256,
                I64x4,
                I64x4,
                4,
                true
            ),
            Self::X86Avx512VlVgatherqpd256 => descriptor!(
                "x86_avx512vl_vgatherqpd256",
                Vgatherqpd,
                256,
                I64x4,
                F64x4,
                4,
                false
            ),
            Self::X86Avx512VlVscatterqpd256 => descriptor!(
                "x86_avx512vl_vscatterqpd256",
                Vscatterqpd,
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
