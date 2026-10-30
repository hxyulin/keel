//! Packed-prefix VL transfers distinguish source-selection and destination masks.
use super::{Intrinsic, Type, VectorType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512VlPackKind {
    Compress,
    Expand,
    CompressStore,
    ExpandLoad,
}
#[derive(Clone, Copy, Debug)]
pub struct X86Avx512VlPack {
    pub name: &'static str,
    pub vector: VectorType,
    pub kind: X86Avx512VlPackKind,
    pub opcode: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
}
impl Intrinsic {
    /// Exact packed-prefix transfers requiring AVX512F and AVX512VL.
    pub fn x86_avx512_vl_pack(self) -> Option<X86Avx512VlPack> {
        macro_rules! descriptor {
            ($kind:ident, $name:literal, $vector:ident, $opcode:literal) => {{
                const V: Type = Type::Vector(VectorType::$vector);
                use X86Avx512VlPackKind::*;
                X86Avx512VlPack {
                    name: $name,
                    vector: VectorType::$vector,
                    kind: $kind,
                    opcode: $opcode,
                    params: match $kind {
                        Compress | Expand => &[V, Type::I8, V],
                        CompressStore | ExpandLoad => &[Type::Ptr, V, Type::I8],
                    },
                    returns: match $kind {
                        CompressStore => &[],
                        _ => &[V],
                    },
                }
            }};
        }
        Some(match self {
            Self::X86Avx512VlCompressd128 => {
                descriptor!(Compress, "x86_avx512vl_compressd128", I32x4, "vpcompressd")
            }
            Self::X86Avx512VlExpandd128 => {
                descriptor!(Expand, "x86_avx512vl_expandd128", I32x4, "vpexpandd")
            }
            Self::X86Avx512VlCompressStored128 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstored128",
                I32x4,
                "vpcompressd"
            ),
            Self::X86Avx512VlExpandLoadd128 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadd128",
                I32x4,
                "vpexpandd"
            ),
            Self::X86Avx512VlCompressq128 => {
                descriptor!(Compress, "x86_avx512vl_compressq128", I64x2, "vpcompressq")
            }
            Self::X86Avx512VlExpandq128 => {
                descriptor!(Expand, "x86_avx512vl_expandq128", I64x2, "vpexpandq")
            }
            Self::X86Avx512VlCompressStoreq128 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstoreq128",
                I64x2,
                "vpcompressq"
            ),
            Self::X86Avx512VlExpandLoadq128 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadq128",
                I64x2,
                "vpexpandq"
            ),
            Self::X86Avx512VlCompressps128 => {
                descriptor!(Compress, "x86_avx512vl_compressps128", F32x4, "vcompressps")
            }
            Self::X86Avx512VlExpandps128 => {
                descriptor!(Expand, "x86_avx512vl_expandps128", F32x4, "vexpandps")
            }
            Self::X86Avx512VlCompressStoreps128 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstoreps128",
                F32x4,
                "vcompressps"
            ),
            Self::X86Avx512VlExpandLoadps128 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadps128",
                F32x4,
                "vexpandps"
            ),
            Self::X86Avx512VlCompresspd128 => {
                descriptor!(Compress, "x86_avx512vl_compresspd128", F64x2, "vcompresspd")
            }
            Self::X86Avx512VlExpandpd128 => {
                descriptor!(Expand, "x86_avx512vl_expandpd128", F64x2, "vexpandpd")
            }
            Self::X86Avx512VlCompressStorepd128 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstorepd128",
                F64x2,
                "vcompresspd"
            ),
            Self::X86Avx512VlExpandLoadpd128 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadpd128",
                F64x2,
                "vexpandpd"
            ),
            Self::X86Avx512VlCompressd256 => {
                descriptor!(Compress, "x86_avx512vl_compressd256", I32x8, "vpcompressd")
            }
            Self::X86Avx512VlExpandd256 => {
                descriptor!(Expand, "x86_avx512vl_expandd256", I32x8, "vpexpandd")
            }
            Self::X86Avx512VlCompressStored256 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstored256",
                I32x8,
                "vpcompressd"
            ),
            Self::X86Avx512VlExpandLoadd256 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadd256",
                I32x8,
                "vpexpandd"
            ),
            Self::X86Avx512VlCompressq256 => {
                descriptor!(Compress, "x86_avx512vl_compressq256", I64x4, "vpcompressq")
            }
            Self::X86Avx512VlExpandq256 => {
                descriptor!(Expand, "x86_avx512vl_expandq256", I64x4, "vpexpandq")
            }
            Self::X86Avx512VlCompressStoreq256 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstoreq256",
                I64x4,
                "vpcompressq"
            ),
            Self::X86Avx512VlExpandLoadq256 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadq256",
                I64x4,
                "vpexpandq"
            ),
            Self::X86Avx512VlCompressps256 => {
                descriptor!(Compress, "x86_avx512vl_compressps256", F32x8, "vcompressps")
            }
            Self::X86Avx512VlExpandps256 => {
                descriptor!(Expand, "x86_avx512vl_expandps256", F32x8, "vexpandps")
            }
            Self::X86Avx512VlCompressStoreps256 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstoreps256",
                F32x8,
                "vcompressps"
            ),
            Self::X86Avx512VlExpandLoadps256 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadps256",
                F32x8,
                "vexpandps"
            ),
            Self::X86Avx512VlCompresspd256 => {
                descriptor!(Compress, "x86_avx512vl_compresspd256", F64x4, "vcompresspd")
            }
            Self::X86Avx512VlExpandpd256 => {
                descriptor!(Expand, "x86_avx512vl_expandpd256", F64x4, "vexpandpd")
            }
            Self::X86Avx512VlCompressStorepd256 => descriptor!(
                CompressStore,
                "x86_avx512vl_compressstorepd256",
                F64x4,
                "vcompresspd"
            ),
            Self::X86Avx512VlExpandLoadpd256 => descriptor!(
                ExpandLoad,
                "x86_avx512vl_expandloadpd256",
                F64x4,
                "vexpandpd"
            ),
            _ => return None,
        })
    }
}
