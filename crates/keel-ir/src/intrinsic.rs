//! Intrinsics: operations with known meaning that behave like calls, written
//! `intrinsic memcpy(v0, v1, v2)`.
//!
//! Pure arithmetic belongs in the instruction set, where the optimizer sees
//! it directly. Intrinsics are for call-like operations (memory functions)
//! and target-specific ones. Everything a pass needs to know about one lives
//! in its [`IntrinsicInfo`]. Reviewed x86 system declarations come from
//! `tools/x86-isa/system-intrinsics.json`; their lowering stays in the backends.

use crate::{Type, VectorType};
use std::fmt;

#[macro_use]
#[path = "generated/x86_system_intrinsics.rs"]
mod generated_system;

#[macro_use]
#[path = "generated/x86_query_intrinsics.rs"]
mod generated_query;

#[path = "generated/intrinsic_registry.rs"]
mod generated_registry;
pub use generated_registry::Intrinsic;
#[path = "intrinsic/avx512_dq_special.rs"]
mod avx512_dq_special;
pub use avx512_dq_special::{X86Avx512DqSpecial, X86Avx512DqSpecialKind};
#[path = "intrinsic/avx512_dq_lane.rs"]
mod avx512_dq_lane;
pub use avx512_dq_lane::{X86Avx512DqLane, X86Avx512DqLaneKind};
#[path = "intrinsic/avx512_bw.rs"]
mod avx512_bw;
#[path = "intrinsic/avx512_vl_fma.rs"]
mod avx512_vl_fma;
#[path = "intrinsic/avx512_vl_special.rs"]
mod avx512_vl_special;
pub use avx512_bw::{X86Avx512BwCore, X86Avx512BwKind};
#[path = "intrinsic/avx512_bw_mask.rs"]
mod avx512_bw_mask;
pub use avx512_bw_mask::{X86Avx512BwMask, X86Avx512BwMaskKind};
#[path = "intrinsic/avx512_bw_shift.rs"]
mod avx512_bw_shift;
#[path = "intrinsic/avx512_vl_permute.rs"]
mod avx512_vl_permute;
#[path = "intrinsic/avx512_vl_shift.rs"]
mod avx512_vl_shift;
pub use avx512_bw_shift::{X86Avx512BwShift, X86Avx512BwShiftKind};
pub use avx512_vl_permute::X86Avx512VlPermute;
#[path = "intrinsic/avx512_bw_pack.rs"]
mod avx512_bw_pack;
pub use avx512_bw_pack::X86Avx512BwPack;
pub use avx512_vl_shift::{X86Avx512VlShift, X86Avx512VlShiftKind};
#[path = "intrinsic/avx512_bw_mul.rs"]
mod avx512_bw_mul;
pub use avx512_bw_mul::X86Avx512BwMul;

#[derive(Debug)]
pub struct IntrinsicInfo {
    pub name: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
    /// Pure intrinsics float like pure instructions; the rest are pinned.
    pub pure: bool,
}

/// Register input arrangement for the AVX512F permutation family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86Avx512PermuteKind {
    Full,
    Local,
    TwoIndex,
    TwoTable,
    Immediate,
    PairImmediate,
    Pair,
    Unary,
}
impl X86Avx512PermuteKind {
    pub const fn control_index(self) -> Option<usize> {
        match self {
            Self::Immediate => Some(3),
            Self::PairImmediate => Some(4),
            _ => None,
        }
    }
}

impl Intrinsic {
    /// DQ sign-bit extraction and zero/all-one lane expansion.
    pub fn x86_avx512_dq_bridge(self) -> Option<(&'static str, VectorType)> {
        Some(match self {
            Self::X86Avx512DqMovd2m128 => ("vpmovd2m", VectorType::I32x4),
            Self::X86Avx512DqMovq2m128 => ("vpmovq2m", VectorType::I64x2),
            Self::X86Avx512DqMovm2d128 => ("vpmovm2d", VectorType::I32x4),
            Self::X86Avx512DqMovm2q128 => ("vpmovm2q", VectorType::I64x2),
            Self::X86Avx512DqMovd2m256 => ("vpmovd2m", VectorType::I32x8),
            Self::X86Avx512DqMovq2m256 => ("vpmovq2m", VectorType::I64x4),
            Self::X86Avx512DqMovm2d256 => ("vpmovm2d", VectorType::I32x8),
            Self::X86Avx512DqMovm2q256 => ("vpmovm2q", VectorType::I64x4),
            Self::X86Avx512DqMovd2m512 => ("vpmovd2m", VectorType::I32x16),
            Self::X86Avx512DqMovq2m512 => ("vpmovq2m", VectorType::I64x8),
            Self::X86Avx512DqMovm2d512 => ("vpmovm2d", VectorType::I32x16),
            Self::X86Avx512DqMovm2q512 => ("vpmovm2q", VectorType::I64x8),
            _ => return None,
        })
    }

    /// Conflict detection, packed leading-zero count and mask broadcasts, requiring AVX512CD.
    pub fn x86_avx512_cd(self) -> Option<(&'static str, VectorType)> {
        Some(match self {
            Self::X86Avx512CdMaskConflictd128 => ("vpconflictd", VectorType::I32x4),
            Self::X86Avx512CdMaskConflictq128 => ("vpconflictq", VectorType::I64x2),
            Self::X86Avx512CdMaskLzcntd128 => ("vplzcntd", VectorType::I32x4),
            Self::X86Avx512CdMaskLzcntq128 => ("vplzcntq", VectorType::I64x2),
            Self::X86Avx512CdMaskConflictd256 => ("vpconflictd", VectorType::I32x8),
            Self::X86Avx512CdMaskConflictq256 => ("vpconflictq", VectorType::I64x4),
            Self::X86Avx512CdMaskLzcntd256 => ("vplzcntd", VectorType::I32x8),
            Self::X86Avx512CdMaskLzcntq256 => ("vplzcntq", VectorType::I64x4),
            Self::X86Avx512CdMaskConflictd512 => ("vpconflictd", VectorType::I32x16),
            Self::X86Avx512CdMaskConflictq512 => ("vpconflictq", VectorType::I64x8),
            Self::X86Avx512CdMaskLzcntd512 => ("vplzcntd", VectorType::I32x16),
            Self::X86Avx512CdMaskLzcntq512 => ("vplzcntq", VectorType::I64x8),
            Self::X86Avx512CdBroadcastmb128 => ("vpbroadcastmb2q", VectorType::I64x2),
            Self::X86Avx512CdBroadcastmw128 => ("vpbroadcastmw2d", VectorType::I32x4),
            Self::X86Avx512CdBroadcastmb256 => ("vpbroadcastmb2q", VectorType::I64x4),
            Self::X86Avx512CdBroadcastmw256 => ("vpbroadcastmw2d", VectorType::I32x8),
            Self::X86Avx512CdBroadcastmb512 => ("vpbroadcastmb2q", VectorType::I64x8),
            Self::X86Avx512CdBroadcastmw512 => ("vpbroadcastmw2d", VectorType::I32x16),
            _ => return None,
        })
    }

    /// Pure DQ multiplication and floating-container bitwise operations.
    pub fn x86_avx512_dq_opcode(self) -> Option<(&'static str, VectorType)> {
        Some(match self {
            Self::X86Avx512DqMulloq128 => ("vpmullq", VectorType::I64x2),
            Self::X86Avx512DqAndps128 => ("vandps", VectorType::F32x4),
            Self::X86Avx512DqAndpd128 => ("vandpd", VectorType::F64x2),
            Self::X86Avx512DqAndnps128 => ("vandnps", VectorType::F32x4),
            Self::X86Avx512DqAndnpd128 => ("vandnpd", VectorType::F64x2),
            Self::X86Avx512DqOrps128 => ("vorps", VectorType::F32x4),
            Self::X86Avx512DqOrpd128 => ("vorpd", VectorType::F64x2),
            Self::X86Avx512DqXorps128 => ("vxorps", VectorType::F32x4),
            Self::X86Avx512DqXorpd128 => ("vxorpd", VectorType::F64x2),
            Self::X86Avx512DqMulloq256 => ("vpmullq", VectorType::I64x4),
            Self::X86Avx512DqAndps256 => ("vandps", VectorType::F32x8),
            Self::X86Avx512DqAndpd256 => ("vandpd", VectorType::F64x4),
            Self::X86Avx512DqAndnps256 => ("vandnps", VectorType::F32x8),
            Self::X86Avx512DqAndnpd256 => ("vandnpd", VectorType::F64x4),
            Self::X86Avx512DqOrps256 => ("vorps", VectorType::F32x8),
            Self::X86Avx512DqOrpd256 => ("vorpd", VectorType::F64x4),
            Self::X86Avx512DqXorps256 => ("vxorps", VectorType::F32x8),
            Self::X86Avx512DqXorpd256 => ("vxorpd", VectorType::F64x4),
            Self::X86Avx512DqMulloq512 => ("vpmullq", VectorType::I64x8),
            Self::X86Avx512DqAndps512 => ("vandps", VectorType::F32x16),
            Self::X86Avx512DqAndpd512 => ("vandpd", VectorType::F64x8),
            Self::X86Avx512DqAndnps512 => ("vandnps", VectorType::F32x16),
            Self::X86Avx512DqAndnpd512 => ("vandnpd", VectorType::F64x8),
            Self::X86Avx512DqOrps512 => ("vorps", VectorType::F32x16),
            Self::X86Avx512DqOrpd512 => ("vorpd", VectorType::F64x8),
            Self::X86Avx512DqXorps512 => ("vxorps", VectorType::F32x16),
            Self::X86Avx512DqXorpd512 => ("vxorpd", VectorType::F64x8),
            _ => return None,
        })
    }

    /// Whether this is a typed SHA-256 operation.
    pub fn is_x86_sha256(self) -> bool {
        matches!(
            self,
            Self::X86Sha256msg1 | Self::X86Sha256msg2 | Self::X86Sha256rnds2
        )
    }

    pub fn is_x86_pclmul(self) -> bool {
        matches!(
            self,
            Self::X86Pclmulqdq128 | Self::X86Vpclmulqdq256 | Self::X86Vpclmulqdq512
        )
    }

    /// Foundation and VL operations with scalar i8/i16 lane masks.
    pub fn is_x86_avx512_mask(self) -> bool {
        self.x86_avx512_cd().is_some()
            || self.x86_avx512_dq_opcode().is_some()
            || self.x86_avx512_vl_integer_move().is_some()
            || self.x86_avx512_vl_float_arithmetic().is_some()
            || self.is_x86_avx512_vl_core()
            || self.is_x86_avx512_float_mask()
            || matches!(
                self,
                Self::X86Avx512Cmpd
                    | Self::X86Avx512Cmpq
                    | Self::X86Avx512Cmpud
                    | Self::X86Avx512Cmpuq
                    | Self::X86Avx512MaskAddd
                    | Self::X86Avx512MaskAddq
                    | Self::X86Avx512MaskSubd
                    | Self::X86Avx512MaskSubq
                    | Self::X86Avx512MaskLoadd
                    | Self::X86Avx512MaskLoadq
                    | Self::X86Avx512MaskStored
                    | Self::X86Avx512MaskStoreq
            )
    }

    pub fn is_x86_avx512_compare(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Cmpd | Self::X86Avx512Cmpq | Self::X86Avx512Cmpud | Self::X86Avx512Cmpuq
        )
    }

    /// Raw AVX512F floating operations with scalar lane masks.
    pub fn is_x86_avx512_float_mask(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Cmpps
                | Self::X86Avx512Cmppd
                | Self::X86Avx512CmpRoundps
                | Self::X86Avx512CmpRoundpd
                | Self::X86Avx512MaskAddps
                | Self::X86Avx512MaskAddpd
                | Self::X86Avx512MaskSubps
                | Self::X86Avx512MaskSubpd
                | Self::X86Avx512MaskMulps
                | Self::X86Avx512MaskMulpd
                | Self::X86Avx512MaskDivps
                | Self::X86Avx512MaskDivpd
                | Self::X86Avx512MaskSqrtps
                | Self::X86Avx512MaskSqrtpd
                | Self::X86Avx512MaskLoadps
                | Self::X86Avx512MaskLoadpd
                | Self::X86Avx512MaskStoreps
                | Self::X86Avx512MaskStorepd
        )
    }

    pub fn is_x86_avx512_float_compare(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Cmpps
                | Self::X86Avx512Cmppd
                | Self::X86Avx512CmpRoundps
                | Self::X86Avx512CmpRoundpd
        )
    }

    /// AVX512F gathers/scatters preserve immutable SSA indices, data and masks.
    pub fn is_x86_avx512_indexed(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Vpgatherdd
                | Self::X86Avx512Vpgatherdq
                | Self::X86Avx512Vpgatherqd
                | Self::X86Avx512Vpgatherqq
                | Self::X86Avx512Vgatherdps
                | Self::X86Avx512Vgatherdpd
                | Self::X86Avx512Vgatherqps
                | Self::X86Avx512Vgatherqpd
                | Self::X86Avx512Vpscatterdd
                | Self::X86Avx512Vpscatterdq
                | Self::X86Avx512Vpscatterqd
                | Self::X86Avx512Vpscatterqq
                | Self::X86Avx512Vscatterdps
                | Self::X86Avx512Vscatterdpd
                | Self::X86Avx512Vscatterqps
                | Self::X86Avx512Vscatterqpd
        )
    }

    pub fn is_x86_avx512_scatter(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Vpscatterdd
                | Self::X86Avx512Vpscatterdq
                | Self::X86Avx512Vpscatterqd
                | Self::X86Avx512Vpscatterqq
                | Self::X86Avx512Vscatterdps
                | Self::X86Avx512Vscatterdpd
                | Self::X86Avx512Vscatterqps
                | Self::X86Avx512Vscatterqpd
        )
    }

    /// AVX512F compression/expansion uses scalar masks and immutable SSA inputs.
    pub fn is_x86_avx512_pack(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Compressd
                | Self::X86Avx512Compressq
                | Self::X86Avx512Compressps
                | Self::X86Avx512Compresspd
                | Self::X86Avx512Expandd
                | Self::X86Avx512Expandq
                | Self::X86Avx512Expandps
                | Self::X86Avx512Expandpd
                | Self::X86Avx512CompressStored
                | Self::X86Avx512CompressStoreq
                | Self::X86Avx512CompressStoreps
                | Self::X86Avx512CompressStorepd
                | Self::X86Avx512ExpandLoadd
                | Self::X86Avx512ExpandLoadq
                | Self::X86Avx512ExpandLoadps
                | Self::X86Avx512ExpandLoadpd
        )
    }

    pub fn is_x86_avx512_compress_store(self) -> bool {
        matches!(
            self,
            Self::X86Avx512CompressStored
                | Self::X86Avx512CompressStoreq
                | Self::X86Avx512CompressStoreps
                | Self::X86Avx512CompressStorepd
        )
    }

    pub fn is_x86_avx512_expand_load(self) -> bool {
        matches!(
            self,
            Self::X86Avx512ExpandLoadd
                | Self::X86Avx512ExpandLoadq
                | Self::X86Avx512ExpandLoadps
                | Self::X86Avx512ExpandLoadpd
        )
    }

    /// Raw qword/FP conversion opcode, encoded length and exact containers.
    pub fn x86_avx512_dq_convert(self) -> Option<(&'static str, u16, VectorType, VectorType)> {
        Some(match self {
            Self::X86Avx512DqVcvtqq2pd128 => {
                ("vcvtqq2pd", 128, VectorType::I64x2, VectorType::F64x2)
            }
            Self::X86Avx512DqVcvtuqq2pd128 => {
                ("vcvtuqq2pd", 128, VectorType::I64x2, VectorType::F64x2)
            }
            Self::X86Avx512DqVcvtqq2ps128 => {
                ("vcvtqq2ps", 128, VectorType::I64x2, VectorType::F32x4)
            }
            Self::X86Avx512DqVcvtuqq2ps128 => {
                ("vcvtuqq2ps", 128, VectorType::I64x2, VectorType::F32x4)
            }
            Self::X86Avx512DqVcvtpd2qq128 => {
                ("vcvtpd2qq", 128, VectorType::F64x2, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvtpd2uqq128 => {
                ("vcvtpd2uqq", 128, VectorType::F64x2, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvtps2qq128 => {
                ("vcvtps2qq", 128, VectorType::F32x4, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvtps2uqq128 => {
                ("vcvtps2uqq", 128, VectorType::F32x4, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvttpd2qq128 => {
                ("vcvttpd2qq", 128, VectorType::F64x2, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvttpd2uqq128 => {
                ("vcvttpd2uqq", 128, VectorType::F64x2, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvttps2qq128 => {
                ("vcvttps2qq", 128, VectorType::F32x4, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvttps2uqq128 => {
                ("vcvttps2uqq", 128, VectorType::F32x4, VectorType::I64x2)
            }
            Self::X86Avx512DqVcvtqq2pd256 => {
                ("vcvtqq2pd", 256, VectorType::I64x4, VectorType::F64x4)
            }
            Self::X86Avx512DqVcvtuqq2pd256 => {
                ("vcvtuqq2pd", 256, VectorType::I64x4, VectorType::F64x4)
            }
            Self::X86Avx512DqVcvtqq2ps256 => {
                ("vcvtqq2ps", 256, VectorType::I64x4, VectorType::F32x4)
            }
            Self::X86Avx512DqVcvtuqq2ps256 => {
                ("vcvtuqq2ps", 256, VectorType::I64x4, VectorType::F32x4)
            }
            Self::X86Avx512DqVcvtpd2qq256 => {
                ("vcvtpd2qq", 256, VectorType::F64x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvtpd2uqq256 => {
                ("vcvtpd2uqq", 256, VectorType::F64x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvtps2qq256 => {
                ("vcvtps2qq", 256, VectorType::F32x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvtps2uqq256 => {
                ("vcvtps2uqq", 256, VectorType::F32x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvttpd2qq256 => {
                ("vcvttpd2qq", 256, VectorType::F64x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvttpd2uqq256 => {
                ("vcvttpd2uqq", 256, VectorType::F64x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvttps2qq256 => {
                ("vcvttps2qq", 256, VectorType::F32x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvttps2uqq256 => {
                ("vcvttps2uqq", 256, VectorType::F32x4, VectorType::I64x4)
            }
            Self::X86Avx512DqVcvtqq2pd512 => {
                ("vcvtqq2pd", 512, VectorType::I64x8, VectorType::F64x8)
            }
            Self::X86Avx512DqVcvtuqq2pd512 => {
                ("vcvtuqq2pd", 512, VectorType::I64x8, VectorType::F64x8)
            }
            Self::X86Avx512DqVcvtqq2ps512 => {
                ("vcvtqq2ps", 512, VectorType::I64x8, VectorType::F32x8)
            }
            Self::X86Avx512DqVcvtuqq2ps512 => {
                ("vcvtuqq2ps", 512, VectorType::I64x8, VectorType::F32x8)
            }
            Self::X86Avx512DqVcvtpd2qq512 => {
                ("vcvtpd2qq", 512, VectorType::F64x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvtpd2uqq512 => {
                ("vcvtpd2uqq", 512, VectorType::F64x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvtps2qq512 => {
                ("vcvtps2qq", 512, VectorType::F32x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvtps2uqq512 => {
                ("vcvtps2uqq", 512, VectorType::F32x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvttpd2qq512 => {
                ("vcvttpd2qq", 512, VectorType::F64x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvttpd2uqq512 => {
                ("vcvttpd2uqq", 512, VectorType::F64x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvttps2qq512 => {
                ("vcvttps2qq", 512, VectorType::F32x8, VectorType::I64x8)
            }
            Self::X86Avx512DqVcvttps2uqq512 => {
                ("vcvttps2uqq", 512, VectorType::F32x8, VectorType::I64x8)
            }
            _ => return None,
        })
    }

    /// Narrow forms have no control. Wide rounded forms accept ER+SAE;
    /// wide truncating forms accept SAE only. Four uses normal hardware
    /// execution; truncating forms always round toward zero.
    pub fn valid_x86_avx512_dq_convert_control(self, control: i64) -> bool {
        self.x86_avx512_dq_convert()
            .is_some_and(|(op, width, _, _)| {
                width == 512
                    && if op.starts_with("vcvtt") {
                        matches!(control, 4 | 8)
                    } else {
                        matches!(control, 4 | 8..=11)
                    }
            })
    }

    /// Native opcode, encoded vector length and exact input/output containers.
    /// Narrow conversions have no ER or SAE. Only VCVTPS2PH has an imm8.
    pub fn x86_avx512_vl_convert(self) -> Option<(&'static str, u16, VectorType, VectorType)> {
        Some(match self {
            Self::X86Avx512VlVcvtdq2ps128 => {
                ("vcvtdq2ps", 128, VectorType::I32x4, VectorType::F32x4)
            }
            Self::X86Avx512VlVcvtdq2ps256 => {
                ("vcvtdq2ps", 256, VectorType::I32x8, VectorType::F32x8)
            }
            Self::X86Avx512VlVcvtudq2ps128 => {
                ("vcvtudq2ps", 128, VectorType::I32x4, VectorType::F32x4)
            }
            Self::X86Avx512VlVcvtudq2ps256 => {
                ("vcvtudq2ps", 256, VectorType::I32x8, VectorType::F32x8)
            }
            Self::X86Avx512VlVcvtdq2pd128 => {
                ("vcvtdq2pd", 128, VectorType::I32x4, VectorType::F64x2)
            }
            Self::X86Avx512VlVcvtdq2pd256 => {
                ("vcvtdq2pd", 256, VectorType::I32x4, VectorType::F64x4)
            }
            Self::X86Avx512VlVcvtudq2pd128 => {
                ("vcvtudq2pd", 128, VectorType::I32x4, VectorType::F64x2)
            }
            Self::X86Avx512VlVcvtudq2pd256 => {
                ("vcvtudq2pd", 256, VectorType::I32x4, VectorType::F64x4)
            }
            Self::X86Avx512VlVcvtps2dq128 => {
                ("vcvtps2dq", 128, VectorType::F32x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtps2dq256 => {
                ("vcvtps2dq", 256, VectorType::F32x8, VectorType::I32x8)
            }
            Self::X86Avx512VlVcvtps2udq128 => {
                ("vcvtps2udq", 128, VectorType::F32x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtps2udq256 => {
                ("vcvtps2udq", 256, VectorType::F32x8, VectorType::I32x8)
            }
            Self::X86Avx512VlVcvttps2dq128 => {
                ("vcvttps2dq", 128, VectorType::F32x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttps2dq256 => {
                ("vcvttps2dq", 256, VectorType::F32x8, VectorType::I32x8)
            }
            Self::X86Avx512VlVcvttps2udq128 => {
                ("vcvttps2udq", 128, VectorType::F32x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttps2udq256 => {
                ("vcvttps2udq", 256, VectorType::F32x8, VectorType::I32x8)
            }
            Self::X86Avx512VlVcvtpd2dq128 => {
                ("vcvtpd2dq", 128, VectorType::F64x2, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtpd2dq256 => {
                ("vcvtpd2dq", 256, VectorType::F64x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtpd2udq128 => {
                ("vcvtpd2udq", 128, VectorType::F64x2, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtpd2udq256 => {
                ("vcvtpd2udq", 256, VectorType::F64x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttpd2dq128 => {
                ("vcvttpd2dq", 128, VectorType::F64x2, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttpd2dq256 => {
                ("vcvttpd2dq", 256, VectorType::F64x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttpd2udq128 => {
                ("vcvttpd2udq", 128, VectorType::F64x2, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvttpd2udq256 => {
                ("vcvttpd2udq", 256, VectorType::F64x4, VectorType::I32x4)
            }
            Self::X86Avx512VlVcvtps2pd128 => {
                ("vcvtps2pd", 128, VectorType::F32x4, VectorType::F64x2)
            }
            Self::X86Avx512VlVcvtps2pd256 => {
                ("vcvtps2pd", 256, VectorType::F32x4, VectorType::F64x4)
            }
            Self::X86Avx512VlVcvtpd2ps128 => {
                ("vcvtpd2ps", 128, VectorType::F64x2, VectorType::F32x4)
            }
            Self::X86Avx512VlVcvtpd2ps256 => {
                ("vcvtpd2ps", 256, VectorType::F64x4, VectorType::F32x4)
            }
            Self::X86Avx512VlVcvtph2ps128 => {
                ("vcvtph2ps", 128, VectorType::I16x8, VectorType::F32x4)
            }
            Self::X86Avx512VlVcvtph2ps256 => {
                ("vcvtph2ps", 256, VectorType::I16x8, VectorType::F32x8)
            }
            Self::X86Avx512VlVcvtps2ph128 => {
                ("vcvtps2ph", 128, VectorType::F32x4, VectorType::I16x8)
            }
            Self::X86Avx512VlVcvtps2ph256 => {
                ("vcvtps2ph", 256, VectorType::F32x8, VectorType::I16x8)
            }
            _ => return None,
        })
    }

    pub fn is_x86_avx512_convert(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Vcvtdq2ps
                | Self::X86Avx512Vcvtudq2ps
                | Self::X86Avx512Vcvtdq2pd
                | Self::X86Avx512Vcvtudq2pd
                | Self::X86Avx512Vcvtps2dq
                | Self::X86Avx512Vcvtps2udq
                | Self::X86Avx512Vcvttps2dq
                | Self::X86Avx512Vcvttps2udq
                | Self::X86Avx512Vcvtpd2dq
                | Self::X86Avx512Vcvtpd2udq
                | Self::X86Avx512Vcvttpd2dq
                | Self::X86Avx512Vcvttpd2udq
                | Self::X86Avx512Vcvtps2pd
                | Self::X86Avx512Vcvtpd2ps
                | Self::X86Avx512Vcvtph2ps
                | Self::X86Avx512Vcvtps2ph
        )
    }

    /// 4 uses MXCSR. RC forms accept 8..11 for static rounding with SAE;
    /// SAE-only forms accept 8. Half narrowing accepts rounding 0..4 with
    /// optional bit 3 for SAE. Exact integer widening accepts only 4.
    pub fn valid_x86_avx512_convert_control(self, control: i64) -> bool {
        match self {
            Self::X86Avx512Vcvtdq2ps
            | Self::X86Avx512Vcvtudq2ps
            | Self::X86Avx512Vcvtps2dq
            | Self::X86Avx512Vcvtps2udq
            | Self::X86Avx512Vcvtpd2dq
            | Self::X86Avx512Vcvtpd2udq
            | Self::X86Avx512Vcvtpd2ps => matches!(control, 4 | 8..=11),
            Self::X86Avx512Vcvttps2dq
            | Self::X86Avx512Vcvttps2udq
            | Self::X86Avx512Vcvttpd2dq
            | Self::X86Avx512Vcvttpd2udq
            | Self::X86Avx512Vcvtps2pd
            | Self::X86Avx512Vcvtph2ps => matches!(control, 4 | 8),
            Self::X86Avx512Vcvtdq2pd | Self::X86Avx512Vcvtudq2pd => control == 4,
            Self::X86Avx512Vcvtps2ph => matches!(control, 0..=4 | 8..=12),
            _ => false,
        }
    }

    /// Native opcode, optional immediate index and optional rounding/SAE index.
    pub fn x86_avx512_special(self) -> Option<(&'static str, Option<usize>, Option<usize>)> {
        if let Some((opcode, _, immediate)) = self.x86_avx512_vl_special() {
            return Some((opcode, immediate, None));
        }
        Some(match self {
            Self::X86Avx512Getexpps => ("vgetexpps", None, Some(3)),
            Self::X86Avx512Getexppd => ("vgetexppd", None, Some(3)),
            Self::X86Avx512Getmantps => ("vgetmantps", Some(3), Some(4)),
            Self::X86Avx512Getmantpd => ("vgetmantpd", Some(3), Some(4)),
            Self::X86Avx512Scalefps => ("vscalefps", None, Some(4)),
            Self::X86Avx512Scalefpd => ("vscalefpd", None, Some(4)),
            Self::X86Avx512Rcp14ps => ("vrcp14ps", None, None),
            Self::X86Avx512Rcp14pd => ("vrcp14pd", None, None),
            Self::X86Avx512Rsqrt14ps => ("vrsqrt14ps", None, None),
            Self::X86Avx512Rsqrt14pd => ("vrsqrt14pd", None, None),
            Self::X86Avx512Roundscaleps => ("vrndscaleps", Some(3), Some(4)),
            Self::X86Avx512Roundscalepd => ("vrndscalepd", Some(3), Some(4)),
            Self::X86Avx512Fixupimmps => ("vfixupimmps", Some(4), Some(5)),
            Self::X86Avx512Fixupimmpd => ("vfixupimmpd", Some(4), Some(5)),
            _ => return None,
        })
    }

    pub fn valid_x86_avx512_special_control(self, index: usize, value: i64) -> bool {
        let Some((name, imm, control)) = self.x86_avx512_special() else {
            return false;
        };
        if Some(index) == imm {
            return (0..=if name.starts_with("vgetmant") {
                15
            } else {
                255
            })
                .contains(&value);
        }
        Some(index) == control
            && (value == 4
                || if name.starts_with("vscalef") {
                    (8..=11).contains(&value)
                } else {
                    value == 8
                })
    }

    /// Pure AVX512F integer controls, distinct from portable modulo shifts.
    pub fn is_x86_avx512_integer(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskTernlogd
                | Self::X86Avx512MaskTernlogq
                | Self::X86Avx512MaskRold
                | Self::X86Avx512MaskRolq
                | Self::X86Avx512MaskRord
                | Self::X86Avx512MaskRorq
                | Self::X86Avx512MaskRolvd
                | Self::X86Avx512MaskRolvq
                | Self::X86Avx512MaskRorvd
                | Self::X86Avx512MaskRorvq
                | Self::X86Avx512MaskSllvd
                | Self::X86Avx512MaskSllvq
                | Self::X86Avx512MaskSrlvd
                | Self::X86Avx512MaskSrlvq
                | Self::X86Avx512MaskSravd
                | Self::X86Avx512MaskSravq
                | Self::X86Avx512Testd
                | Self::X86Avx512Testq
                | Self::X86Avx512Testnd
                | Self::X86Avx512Testnq
                | Self::X86Avx512MaskAbsd
                | Self::X86Avx512MaskAbsq
                | Self::X86Avx512MaskMuldq
                | Self::X86Avx512MaskMuludq
        )
    }

    /// The byte immediate operand, when the operation has one.
    pub fn x86_avx512_integer_control(self) -> Option<usize> {
        match self {
            Self::X86Avx512MaskTernlogd | Self::X86Avx512MaskTernlogq => Some(5),
            Self::X86Avx512MaskRold
            | Self::X86Avx512MaskRolq
            | Self::X86Avx512MaskRord
            | Self::X86Avx512MaskRorq => Some(3),
            _ => None,
        }
    }

    pub fn is_x86_avx512_integer_test(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Testd
                | Self::X86Avx512Testq
                | Self::X86Avx512Testnd
                | Self::X86Avx512Testnq
        )
    }

    /// Raw FMA operands retain their encoded positions, including NaN priority.
    /// 132 computes a*c+b, 213 computes b*a+c, and 231 computes b*c+a.
    /// Alternating add/sub families select their architectural lane signs.
    pub fn x86_avx512_fma_opcode(self) -> Option<&'static str> {
        if let Some((opcode, _)) = self.x86_avx512_vl_fma() {
            return Some(opcode);
        }
        Some(match self {
            Self::X86Avx512Vfmadd132ps => "vfmadd132ps",
            Self::X86Avx512Vfmadd132pd => "vfmadd132pd",
            Self::X86Avx512Vfmadd213ps => "vfmadd213ps",
            Self::X86Avx512Vfmadd213pd => "vfmadd213pd",
            Self::X86Avx512Vfmadd231ps => "vfmadd231ps",
            Self::X86Avx512Vfmadd231pd => "vfmadd231pd",
            Self::X86Avx512Vfmsub132ps => "vfmsub132ps",
            Self::X86Avx512Vfmsub132pd => "vfmsub132pd",
            Self::X86Avx512Vfmsub213ps => "vfmsub213ps",
            Self::X86Avx512Vfmsub213pd => "vfmsub213pd",
            Self::X86Avx512Vfmsub231ps => "vfmsub231ps",
            Self::X86Avx512Vfmsub231pd => "vfmsub231pd",
            Self::X86Avx512Vfnmadd132ps => "vfnmadd132ps",
            Self::X86Avx512Vfnmadd132pd => "vfnmadd132pd",
            Self::X86Avx512Vfnmadd213ps => "vfnmadd213ps",
            Self::X86Avx512Vfnmadd213pd => "vfnmadd213pd",
            Self::X86Avx512Vfnmadd231ps => "vfnmadd231ps",
            Self::X86Avx512Vfnmadd231pd => "vfnmadd231pd",
            Self::X86Avx512Vfnmsub132ps => "vfnmsub132ps",
            Self::X86Avx512Vfnmsub132pd => "vfnmsub132pd",
            Self::X86Avx512Vfnmsub213ps => "vfnmsub213ps",
            Self::X86Avx512Vfnmsub213pd => "vfnmsub213pd",
            Self::X86Avx512Vfnmsub231ps => "vfnmsub231ps",
            Self::X86Avx512Vfnmsub231pd => "vfnmsub231pd",
            Self::X86Avx512Vfmaddsub132ps => "vfmaddsub132ps",
            Self::X86Avx512Vfmaddsub132pd => "vfmaddsub132pd",
            Self::X86Avx512Vfmaddsub213ps => "vfmaddsub213ps",
            Self::X86Avx512Vfmaddsub213pd => "vfmaddsub213pd",
            Self::X86Avx512Vfmaddsub231ps => "vfmaddsub231ps",
            Self::X86Avx512Vfmaddsub231pd => "vfmaddsub231pd",
            Self::X86Avx512Vfmsubadd132ps => "vfmsubadd132ps",
            Self::X86Avx512Vfmsubadd132pd => "vfmsubadd132pd",
            Self::X86Avx512Vfmsubadd213ps => "vfmsubadd213ps",
            Self::X86Avx512Vfmsubadd213pd => "vfmsubadd213pd",
            Self::X86Avx512Vfmsubadd231ps => "vfmsubadd231ps",
            Self::X86Avx512Vfmsubadd231pd => "vfmsubadd231pd",
            _ => return None,
        })
    }

    /// Scalar EVEX opcode, optional immediate index and optional SAE/rounding index.
    pub fn x86_avx512_scalar(self) -> Option<(&'static str, Option<usize>, Option<usize>)> {
        Some(match self {
            Self::X86Avx512Addss => ("vaddss", None, Some(4)),
            Self::X86Avx512Addsd => ("vaddsd", None, Some(4)),
            Self::X86Avx512Subss => ("vsubss", None, Some(4)),
            Self::X86Avx512Subsd => ("vsubsd", None, Some(4)),
            Self::X86Avx512Mulss => ("vmulss", None, Some(4)),
            Self::X86Avx512Mulsd => ("vmulsd", None, Some(4)),
            Self::X86Avx512Divss => ("vdivss", None, Some(4)),
            Self::X86Avx512Divsd => ("vdivsd", None, Some(4)),
            Self::X86Avx512Sqrtss => ("vsqrtss", None, Some(4)),
            Self::X86Avx512Sqrtsd => ("vsqrtsd", None, Some(4)),
            Self::X86Avx512Minss => ("vminss", None, Some(4)),
            Self::X86Avx512Minsd => ("vminsd", None, Some(4)),
            Self::X86Avx512Maxss => ("vmaxss", None, Some(4)),
            Self::X86Avx512Maxsd => ("vmaxsd", None, Some(4)),
            Self::X86Avx512Cmpss => ("vcmpss", Some(3), Some(4)),
            Self::X86Avx512Cmpsd => ("vcmpsd", Some(3), Some(4)),
            Self::X86Avx512Getexpss => ("vgetexpss", None, Some(4)),
            Self::X86Avx512Getexpsd => ("vgetexpsd", None, Some(4)),
            Self::X86Avx512Getmantss => ("vgetmantss", Some(4), Some(5)),
            Self::X86Avx512Getmantsd => ("vgetmantsd", Some(4), Some(5)),
            Self::X86Avx512Scalefss => ("vscalefss", None, Some(4)),
            Self::X86Avx512Scalefsd => ("vscalefsd", None, Some(4)),
            Self::X86Avx512Rcp14ss => ("vrcp14ss", None, None),
            Self::X86Avx512Rcp14sd => ("vrcp14sd", None, None),
            Self::X86Avx512Rsqrt14ss => ("vrsqrt14ss", None, None),
            Self::X86Avx512Rsqrt14sd => ("vrsqrt14sd", None, None),
            Self::X86Avx512Roundscaless => ("vrndscaless", Some(4), Some(5)),
            Self::X86Avx512Roundscalesd => ("vrndscalesd", Some(4), Some(5)),
            Self::X86Avx512Fixupimmss => ("vfixupimmss", Some(4), Some(5)),
            Self::X86Avx512Fixupimmsd => ("vfixupimmsd", Some(4), Some(5)),
            _ => return None,
        })
    }

    pub fn x86_avx512_scalar_embedded_rounding(self) -> bool {
        self.x86_avx512_scalar().is_some_and(|(name, _, _)| {
            ["vadd", "vsub", "vmul", "vdiv", "vsqrt", "vscalef"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
    }

    pub fn valid_x86_avx512_scalar_control(self, index: usize, value: i64) -> bool {
        let Some((name, immediate, control)) = self.x86_avx512_scalar() else {
            return false;
        };
        if Some(index) == immediate {
            return (0..=if name.starts_with("vcmp") {
                31
            } else if name.starts_with("vgetmant") {
                15
            } else {
                255
            })
                .contains(&value);
        }
        Some(index) == control
            && (value == 4
                || if self.x86_avx512_scalar_embedded_rounding() {
                    (8..=11).contains(&value)
                } else {
                    value == 8
                })
    }

    /// Foundation word mask algebra with scalar SSA masks and transient K registers.
    pub fn is_x86_avx512_mask_alu(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Kandw
                | Self::X86Avx512Kandnw
                | Self::X86Avx512Korw
                | Self::X86Avx512Kxorw
                | Self::X86Avx512Kxnorw
                | Self::X86Avx512Knotw
                | Self::X86Avx512Kshiftlw
                | Self::X86Avx512Kshiftrw
                | Self::X86Avx512Kunpckbw
                | Self::X86Avx512Ktestw
                | Self::X86Avx512Kortestw
        )
    }

    pub fn has_x86_avx512_mask_shift_control(self) -> bool {
        matches!(self, Self::X86Avx512Kshiftlw | Self::X86Avx512Kshiftrw)
    }

    /// Scalar FMA keeps encoded operand positions and destination-a upper lanes.
    pub fn x86_avx512_scalar_fma_opcode(self) -> Option<&'static str> {
        Some(match self {
            Self::X86Avx512Vfmadd132ss => "vfmadd132ss",
            Self::X86Avx512Vfmadd132sd => "vfmadd132sd",
            Self::X86Avx512Vfmadd213ss => "vfmadd213ss",
            Self::X86Avx512Vfmadd213sd => "vfmadd213sd",
            Self::X86Avx512Vfmadd231ss => "vfmadd231ss",
            Self::X86Avx512Vfmadd231sd => "vfmadd231sd",
            Self::X86Avx512Vfmsub132ss => "vfmsub132ss",
            Self::X86Avx512Vfmsub132sd => "vfmsub132sd",
            Self::X86Avx512Vfmsub213ss => "vfmsub213ss",
            Self::X86Avx512Vfmsub213sd => "vfmsub213sd",
            Self::X86Avx512Vfmsub231ss => "vfmsub231ss",
            Self::X86Avx512Vfmsub231sd => "vfmsub231sd",
            Self::X86Avx512Vfnmadd132ss => "vfnmadd132ss",
            Self::X86Avx512Vfnmadd132sd => "vfnmadd132sd",
            Self::X86Avx512Vfnmadd213ss => "vfnmadd213ss",
            Self::X86Avx512Vfnmadd213sd => "vfnmadd213sd",
            Self::X86Avx512Vfnmadd231ss => "vfnmadd231ss",
            Self::X86Avx512Vfnmadd231sd => "vfnmadd231sd",
            Self::X86Avx512Vfnmsub132ss => "vfnmsub132ss",
            Self::X86Avx512Vfnmsub132sd => "vfnmsub132sd",
            Self::X86Avx512Vfnmsub213ss => "vfnmsub213ss",
            Self::X86Avx512Vfnmsub213sd => "vfnmsub213sd",
            Self::X86Avx512Vfnmsub231ss => "vfnmsub231ss",
            Self::X86Avx512Vfnmsub231sd => "vfnmsub231sd",
            _ => return None,
        })
    }

    /// Scalar conversion/flag opcode and rounding/SAE control index.
    pub fn x86_avx512_scalar_convert(self) -> Option<(&'static str, usize)> {
        Some(match self {
            Self::X86Avx512Vcvtsi2ss32 => ("vcvtsi2ss", 2),
            Self::X86Avx512Vcvtsi2sd32 => ("vcvtsi2sd", 2),
            Self::X86Avx512Vcvtsi2ss64 => ("vcvtsi2ss", 2),
            Self::X86Avx512Vcvtsi2sd64 => ("vcvtsi2sd", 2),
            Self::X86Avx512Vcvtusi2ss32 => ("vcvtusi2ss", 2),
            Self::X86Avx512Vcvtusi2sd32 => ("vcvtusi2sd", 2),
            Self::X86Avx512Vcvtusi2ss64 => ("vcvtusi2ss", 2),
            Self::X86Avx512Vcvtusi2sd64 => ("vcvtusi2sd", 2),
            Self::X86Avx512Vcvtss2si32 => ("vcvtss2si", 1),
            Self::X86Avx512Vcvtss2si64 => ("vcvtss2si", 1),
            Self::X86Avx512Vcvttss2si32 => ("vcvttss2si", 1),
            Self::X86Avx512Vcvttss2si64 => ("vcvttss2si", 1),
            Self::X86Avx512Vcvtss2usi32 => ("vcvtss2usi", 1),
            Self::X86Avx512Vcvtss2usi64 => ("vcvtss2usi", 1),
            Self::X86Avx512Vcvttss2usi32 => ("vcvttss2usi", 1),
            Self::X86Avx512Vcvttss2usi64 => ("vcvttss2usi", 1),
            Self::X86Avx512Vcvtsd2si32 => ("vcvtsd2si", 1),
            Self::X86Avx512Vcvtsd2si64 => ("vcvtsd2si", 1),
            Self::X86Avx512Vcvttsd2si32 => ("vcvttsd2si", 1),
            Self::X86Avx512Vcvttsd2si64 => ("vcvttsd2si", 1),
            Self::X86Avx512Vcvtsd2usi32 => ("vcvtsd2usi", 1),
            Self::X86Avx512Vcvtsd2usi64 => ("vcvtsd2usi", 1),
            Self::X86Avx512Vcvttsd2usi32 => ("vcvttsd2usi", 1),
            Self::X86Avx512Vcvttsd2usi64 => ("vcvttsd2usi", 1),
            Self::X86Avx512Vcvtss2sd => ("vcvtss2sd", 4),
            Self::X86Avx512Vcvtsd2ss => ("vcvtsd2ss", 4),
            Self::X86Avx512Vcomiss => ("vcomiss", 2),
            Self::X86Avx512Vcomisd => ("vcomisd", 2),
            Self::X86Avx512Vucomiss => ("vucomiss", 2),
            Self::X86Avx512Vucomisd => ("vucomisd", 2),
            _ => return None,
        })
    }
    pub fn x86_avx512_scalar_convert_embedded_rounding(self) -> bool {
        matches!(
            self,
            Self::X86Avx512Vcvtsi2ss32
                | Self::X86Avx512Vcvtsi2ss64
                | Self::X86Avx512Vcvtsi2sd64
                | Self::X86Avx512Vcvtusi2ss32
                | Self::X86Avx512Vcvtusi2ss64
                | Self::X86Avx512Vcvtusi2sd64
                | Self::X86Avx512Vcvtss2si32
                | Self::X86Avx512Vcvtss2si64
                | Self::X86Avx512Vcvtss2usi32
                | Self::X86Avx512Vcvtss2usi64
                | Self::X86Avx512Vcvtsd2si32
                | Self::X86Avx512Vcvtsd2si64
                | Self::X86Avx512Vcvtsd2usi32
                | Self::X86Avx512Vcvtsd2usi64
                | Self::X86Avx512Vcvtsd2ss
        )
    }
    pub fn valid_x86_avx512_scalar_convert_control(self, value: i64) -> bool {
        if self.x86_avx512_scalar_convert().is_none() {
            return false;
        }
        if matches!(
            self,
            Self::X86Avx512Vcvtsi2sd32 | Self::X86Avx512Vcvtusi2sd32
        ) {
            return value == 4;
        }
        value == 4
            || if self.x86_avx512_scalar_convert_embedded_rounding() {
                (8..=11).contains(&value)
            } else {
                value == 8
            }
    }

    /// Masked AVX512F integer conversions, excluding AVX512BW word/byte forms.
    pub fn is_x86_avx512_integer_convert(self) -> bool {
        self.x86_avx512_integer_convert_opcode().is_some()
    }

    pub fn is_x86_avx512_integer_convert_load(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskLoadPmovsxbd
                | Self::X86Avx512MaskLoadPmovsxbq
                | Self::X86Avx512MaskLoadPmovsxwd
                | Self::X86Avx512MaskLoadPmovsxwq
                | Self::X86Avx512MaskLoadPmovsxdq
                | Self::X86Avx512MaskLoadPmovzxbd
                | Self::X86Avx512MaskLoadPmovzxbq
                | Self::X86Avx512MaskLoadPmovzxwd
                | Self::X86Avx512MaskLoadPmovzxwq
                | Self::X86Avx512MaskLoadPmovzxdq
        )
    }

    pub fn is_x86_avx512_integer_convert_store(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskStorePmovdb
                | Self::X86Avx512MaskStorePmovdw
                | Self::X86Avx512MaskStorePmovqb
                | Self::X86Avx512MaskStorePmovqd
                | Self::X86Avx512MaskStorePmovqw
                | Self::X86Avx512MaskStorePmovsdb
                | Self::X86Avx512MaskStorePmovsdw
                | Self::X86Avx512MaskStorePmovsqb
                | Self::X86Avx512MaskStorePmovsqd
                | Self::X86Avx512MaskStorePmovsqw
                | Self::X86Avx512MaskStorePmovusdb
                | Self::X86Avx512MaskStorePmovusdw
                | Self::X86Avx512MaskStorePmovusqb
                | Self::X86Avx512MaskStorePmovusqd
                | Self::X86Avx512MaskStorePmovusqw
        )
    }

    pub fn x86_avx512_integer_convert_opcode(self) -> Option<&'static str> {
        Some(match self {
            Self::X86Avx512MaskPmovsxbd | Self::X86Avx512MaskLoadPmovsxbd => "vpmovsxbd",
            Self::X86Avx512MaskPmovsxbq | Self::X86Avx512MaskLoadPmovsxbq => "vpmovsxbq",
            Self::X86Avx512MaskPmovsxwd | Self::X86Avx512MaskLoadPmovsxwd => "vpmovsxwd",
            Self::X86Avx512MaskPmovsxwq | Self::X86Avx512MaskLoadPmovsxwq => "vpmovsxwq",
            Self::X86Avx512MaskPmovsxdq | Self::X86Avx512MaskLoadPmovsxdq => "vpmovsxdq",
            Self::X86Avx512MaskPmovzxbd | Self::X86Avx512MaskLoadPmovzxbd => "vpmovzxbd",
            Self::X86Avx512MaskPmovzxbq | Self::X86Avx512MaskLoadPmovzxbq => "vpmovzxbq",
            Self::X86Avx512MaskPmovzxwd | Self::X86Avx512MaskLoadPmovzxwd => "vpmovzxwd",
            Self::X86Avx512MaskPmovzxwq | Self::X86Avx512MaskLoadPmovzxwq => "vpmovzxwq",
            Self::X86Avx512MaskPmovzxdq | Self::X86Avx512MaskLoadPmovzxdq => "vpmovzxdq",
            Self::X86Avx512MaskPmovdb | Self::X86Avx512MaskStorePmovdb => "vpmovdb",
            Self::X86Avx512MaskPmovdw | Self::X86Avx512MaskStorePmovdw => "vpmovdw",
            Self::X86Avx512MaskPmovqb | Self::X86Avx512MaskStorePmovqb => "vpmovqb",
            Self::X86Avx512MaskPmovqd | Self::X86Avx512MaskStorePmovqd => "vpmovqd",
            Self::X86Avx512MaskPmovqw | Self::X86Avx512MaskStorePmovqw => "vpmovqw",
            Self::X86Avx512MaskPmovsdb | Self::X86Avx512MaskStorePmovsdb => "vpmovsdb",
            Self::X86Avx512MaskPmovsdw | Self::X86Avx512MaskStorePmovsdw => "vpmovsdw",
            Self::X86Avx512MaskPmovsqb | Self::X86Avx512MaskStorePmovsqb => "vpmovsqb",
            Self::X86Avx512MaskPmovsqd | Self::X86Avx512MaskStorePmovsqd => "vpmovsqd",
            Self::X86Avx512MaskPmovsqw | Self::X86Avx512MaskStorePmovsqw => "vpmovsqw",
            Self::X86Avx512MaskPmovusdb | Self::X86Avx512MaskStorePmovusdb => "vpmovusdb",
            Self::X86Avx512MaskPmovusdw | Self::X86Avx512MaskStorePmovusdw => "vpmovusdw",
            Self::X86Avx512MaskPmovusqb | Self::X86Avx512MaskStorePmovusqb => "vpmovusqb",
            Self::X86Avx512MaskPmovusqd | Self::X86Avx512MaskStorePmovusqd => "vpmovusqd",
            Self::X86Avx512MaskPmovusqw | Self::X86Avx512MaskStorePmovusqw => "vpmovusqw",
            _ => return None,
        })
    }

    /// Full encoded memory capacity in bytes; inactive lanes suppress accesses.
    pub fn x86_avx512_integer_convert_memory_bytes(self) -> Option<u16> {
        Some(match self {
            Self::X86Avx512MaskLoadPmovsxbq
            | Self::X86Avx512MaskLoadPmovzxbq
            | Self::X86Avx512MaskStorePmovqb
            | Self::X86Avx512MaskStorePmovsqb
            | Self::X86Avx512MaskStorePmovusqb => 8,
            Self::X86Avx512MaskLoadPmovsxbd
            | Self::X86Avx512MaskLoadPmovsxwq
            | Self::X86Avx512MaskLoadPmovzxbd
            | Self::X86Avx512MaskLoadPmovzxwq
            | Self::X86Avx512MaskStorePmovdb
            | Self::X86Avx512MaskStorePmovqw
            | Self::X86Avx512MaskStorePmovsdb
            | Self::X86Avx512MaskStorePmovsqw
            | Self::X86Avx512MaskStorePmovusdb
            | Self::X86Avx512MaskStorePmovusqw => 16,
            Self::X86Avx512MaskLoadPmovsxwd
            | Self::X86Avx512MaskLoadPmovsxdq
            | Self::X86Avx512MaskLoadPmovzxwd
            | Self::X86Avx512MaskLoadPmovzxdq
            | Self::X86Avx512MaskStorePmovdw
            | Self::X86Avx512MaskStorePmovqd
            | Self::X86Avx512MaskStorePmovsdw
            | Self::X86Avx512MaskStorePmovsqd
            | Self::X86Avx512MaskStorePmovusdw
            | Self::X86Avx512MaskStorePmovusqd => 32,
            _ => return None,
        })
    }

    pub fn x86_avx512_permute(self) -> Option<(&'static str, X86Avx512PermuteKind)> {
        if let Some(descriptor) = self.x86_avx512_vl_permute() {
            return Some((descriptor.opcode, descriptor.kind));
        }
        use X86Avx512PermuteKind::*;
        Some(match self {
            Self::X86Avx512Vpermd => ("vpermd", Full),
            Self::X86Avx512Vpermq => ("vpermq", Full),
            Self::X86Avx512Vpermps => ("vpermps", Full),
            Self::X86Avx512Vpermpd => ("vpermpd", Full),
            Self::X86Avx512Vpermi2d => ("vpermi2d", TwoIndex),
            Self::X86Avx512Vpermi2q => ("vpermi2q", TwoIndex),
            Self::X86Avx512Vpermi2ps => ("vpermi2ps", TwoIndex),
            Self::X86Avx512Vpermi2pd => ("vpermi2pd", TwoIndex),
            Self::X86Avx512Vpermt2d => ("vpermt2d", TwoTable),
            Self::X86Avx512Vpermt2q => ("vpermt2q", TwoTable),
            Self::X86Avx512Vpermt2ps => ("vpermt2ps", TwoTable),
            Self::X86Avx512Vpermt2pd => ("vpermt2pd", TwoTable),
            Self::X86Avx512Vpermilps => ("vpermilps", Local),
            Self::X86Avx512Vpermilpd => ("vpermilpd", Local),
            Self::X86Avx512VpermilpsImm => ("vpermilps", Immediate),
            Self::X86Avx512VpermilpdImm => ("vpermilpd", Immediate),
            Self::X86Avx512VpermqImm => ("vpermq", Immediate),
            Self::X86Avx512VpermpdImm => ("vpermpd", Immediate),
            Self::X86Avx512Vshufps => ("vshufps", PairImmediate),
            Self::X86Avx512Vshufpd => ("vshufpd", PairImmediate),
            Self::X86Avx512Vpshufd => ("vpshufd", Immediate),
            Self::X86Avx512Vshufi32x4 => ("vshufi32x4", PairImmediate),
            Self::X86Avx512Vshufi64x2 => ("vshufi64x2", PairImmediate),
            Self::X86Avx512Vshuff32x4 => ("vshuff32x4", PairImmediate),
            Self::X86Avx512Vshuff64x2 => ("vshuff64x2", PairImmediate),
            Self::X86Avx512Valignd => ("valignd", PairImmediate),
            Self::X86Avx512Valignq => ("valignq", PairImmediate),
            Self::X86Avx512Vpunpckldq => ("vpunpckldq", Pair),
            Self::X86Avx512Vpunpcklqdq => ("vpunpcklqdq", Pair),
            Self::X86Avx512Vunpcklps => ("vunpcklps", Pair),
            Self::X86Avx512Vunpcklpd => ("vunpcklpd", Pair),
            Self::X86Avx512Vpunpckhdq => ("vpunpckhdq", Pair),
            Self::X86Avx512Vpunpckhqdq => ("vpunpckhqdq", Pair),
            Self::X86Avx512Vunpckhps => ("vunpckhps", Pair),
            Self::X86Avx512Vunpckhpd => ("vunpckhpd", Pair),
            Self::X86Avx512Vmovshdup => ("vmovshdup", Unary),
            Self::X86Avx512Vmovsldup => ("vmovsldup", Unary),
            Self::X86Avx512Vmovddup => ("vmovddup", Unary),
            _ => return None,
        })
    }

    pub fn x86_avx512_packed_opcode(self) -> Option<&'static str> {
        Some(match self {
            Self::X86Avx512MaskMulld => "vpmulld",
            Self::X86Avx512MaskMinsd => "vpminsd",
            Self::X86Avx512MaskMinud => "vpminud",
            Self::X86Avx512MaskMaxsd => "vpmaxsd",
            Self::X86Avx512MaskMaxud => "vpmaxud",
            Self::X86Avx512MaskAndd => "vpandd",
            Self::X86Avx512MaskAndnd => "vpandnd",
            Self::X86Avx512MaskOrd => "vpord",
            Self::X86Avx512MaskXord => "vpxord",
            Self::X86Avx512MaskMinsq => "vpminsq",
            Self::X86Avx512MaskMinuq => "vpminuq",
            Self::X86Avx512MaskMaxsq => "vpmaxsq",
            Self::X86Avx512MaskMaxuq => "vpmaxuq",
            Self::X86Avx512MaskAndq => "vpandq",
            Self::X86Avx512MaskAndnq => "vpandnq",
            Self::X86Avx512MaskOrq => "vporq",
            Self::X86Avx512MaskXorq => "vpxorq",
            Self::X86Avx512MaskAddRoundps => "vaddps",
            Self::X86Avx512MaskAddRoundpd => "vaddpd",
            Self::X86Avx512MaskSubRoundps => "vsubps",
            Self::X86Avx512MaskSubRoundpd => "vsubpd",
            Self::X86Avx512MaskMulRoundps => "vmulps",
            Self::X86Avx512MaskMulRoundpd => "vmulpd",
            Self::X86Avx512MaskDivRoundps => "vdivps",
            Self::X86Avx512MaskDivRoundpd => "vdivpd",
            Self::X86Avx512MaskSqrtRoundps => "vsqrtps",
            Self::X86Avx512MaskSqrtRoundpd => "vsqrtpd",
            Self::X86Avx512MaskMinps => "vminps",
            Self::X86Avx512MaskMinpd => "vminpd",
            Self::X86Avx512MaskMaxps => "vmaxps",
            Self::X86Avx512MaskMaxpd => "vmaxpd",
            Self::X86Avx512MaskMoved => "vmovdqu32",
            Self::X86Avx512Blendd => "vpblendmd",
            Self::X86Avx512MaskMoveq => "vmovdqu64",
            Self::X86Avx512Blendq => "vpblendmq",
            Self::X86Avx512MaskMoveps => "vmovdqu32",
            Self::X86Avx512Blendps => "vblendmps",
            Self::X86Avx512MaskMovepd => "vmovdqu64",
            Self::X86Avx512Blendpd => "vblendmpd",
            Self::X86Avx512MaskAlignedLoadd => "vmovdqa32",
            Self::X86Avx512MaskAlignedStored => "vmovdqa32",
            Self::X86Avx512MaskAlignedLoadq => "vmovdqa64",
            Self::X86Avx512MaskAlignedStoreq => "vmovdqa64",
            Self::X86Avx512MaskAlignedLoadps => "vmovaps",
            Self::X86Avx512MaskAlignedStoreps => "vmovaps",
            Self::X86Avx512MaskAlignedLoadpd => "vmovapd",
            Self::X86Avx512MaskAlignedStorepd => "vmovapd",
            Self::X86Avx512StreamLoaddq => "vmovntdqa",
            Self::X86Avx512StreamStoredq => "vmovntdq",
            Self::X86Avx512StreamStoreps => "vmovntps",
            Self::X86Avx512StreamStorepd => "vmovntpd",
            _ => return None,
        })
    }
    pub fn x86_avx512_packed_control(self) -> Option<usize> {
        self.x86_avx512_packed_opcode()?;
        let info = self.info();
        (info
            .returns
            .first()
            .is_some_and(|t| t.vector().is_some_and(|s| s.lane_type().is_float()))
            && info.params.last() == Some(&Type::I32))
        .then_some(info.params.len() - 1)
    }
    pub fn valid_x86_avx512_packed_control(self, value: i64) -> bool {
        let Some(name) = self.x86_avx512_packed_opcode() else {
            return false;
        };
        self.x86_avx512_packed_control().is_some()
            && (value == 4
                || if name.starts_with("vmin") || name.starts_with("vmax") {
                    value == 8
                } else {
                    (8..=11).contains(&value)
                })
    }

    /// Foundation immediate/shared shifts, broadcasts and quarter/half transfers.
    pub fn is_x86_avx512_lane(self) -> bool {
        self.x86_avx512_lane_opcode().is_some()
    }

    pub fn is_x86_avx512_lane_load(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskLoadPbroadcastd
                | Self::X86Avx512MaskLoadPbroadcastq
                | Self::X86Avx512MaskLoadBroadcastss
                | Self::X86Avx512MaskLoadBroadcastsd
                | Self::X86Avx512MaskLoadBroadcasti32x4
                | Self::X86Avx512MaskLoadBroadcasti64x4
                | Self::X86Avx512MaskLoadBroadcastf32x4
                | Self::X86Avx512MaskLoadBroadcastf64x4
                | Self::X86Avx512MaskLoadInserti32x4
                | Self::X86Avx512MaskLoadInserti64x4
                | Self::X86Avx512MaskLoadInsertf32x4
                | Self::X86Avx512MaskLoadInsertf64x4
        )
    }

    pub fn is_x86_avx512_lane_store(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskStoreExtracti32x4
                | Self::X86Avx512MaskStoreExtracti64x4
                | Self::X86Avx512MaskStoreExtractf32x4
                | Self::X86Avx512MaskStoreExtractf64x4
        )
    }

    pub fn x86_avx512_lane_opcode(self) -> Option<&'static str> {
        Some(match self {
            Self::X86Avx512MaskSllid | Self::X86Avx512MaskSlld => "vpslld",
            Self::X86Avx512MaskSlliq | Self::X86Avx512MaskSllq => "vpsllq",
            Self::X86Avx512MaskSrlid | Self::X86Avx512MaskSrld => "vpsrld",
            Self::X86Avx512MaskSrliq | Self::X86Avx512MaskSrlq => "vpsrlq",
            Self::X86Avx512MaskSraid | Self::X86Avx512MaskSrad => "vpsrad",
            Self::X86Avx512MaskSraiq | Self::X86Avx512MaskSraq => "vpsraq",
            Self::X86Avx512MaskPbroadcastd | Self::X86Avx512MaskLoadPbroadcastd => "vpbroadcastd",
            Self::X86Avx512MaskPbroadcastq | Self::X86Avx512MaskLoadPbroadcastq => "vpbroadcastq",
            Self::X86Avx512MaskBroadcastss | Self::X86Avx512MaskLoadBroadcastss => "vbroadcastss",
            Self::X86Avx512MaskBroadcastsd | Self::X86Avx512MaskLoadBroadcastsd => "vbroadcastsd",
            Self::X86Avx512MaskBroadcasti32x4 | Self::X86Avx512MaskLoadBroadcasti32x4 => {
                "vbroadcasti32x4"
            }
            Self::X86Avx512MaskBroadcasti64x4 | Self::X86Avx512MaskLoadBroadcasti64x4 => {
                "vbroadcasti64x4"
            }
            Self::X86Avx512MaskBroadcastf32x4 | Self::X86Avx512MaskLoadBroadcastf32x4 => {
                "vbroadcastf32x4"
            }
            Self::X86Avx512MaskBroadcastf64x4 | Self::X86Avx512MaskLoadBroadcastf64x4 => {
                "vbroadcastf64x4"
            }
            Self::X86Avx512MaskExtracti32x4 | Self::X86Avx512MaskStoreExtracti32x4 => {
                "vextracti32x4"
            }
            Self::X86Avx512MaskExtracti64x4 | Self::X86Avx512MaskStoreExtracti64x4 => {
                "vextracti64x4"
            }
            Self::X86Avx512MaskExtractf32x4 | Self::X86Avx512MaskStoreExtractf32x4 => {
                "vextractf32x4"
            }
            Self::X86Avx512MaskExtractf64x4 | Self::X86Avx512MaskStoreExtractf64x4 => {
                "vextractf64x4"
            }
            Self::X86Avx512MaskInserti32x4 | Self::X86Avx512MaskLoadInserti32x4 => "vinserti32x4",
            Self::X86Avx512MaskInserti64x4 | Self::X86Avx512MaskLoadInserti64x4 => "vinserti64x4",
            Self::X86Avx512MaskInsertf32x4 | Self::X86Avx512MaskLoadInsertf32x4 => "vinsertf32x4",
            Self::X86Avx512MaskInsertf64x4 | Self::X86Avx512MaskLoadInsertf64x4 => "vinsertf64x4",
            _ => return None,
        })
    }

    pub fn x86_avx512_lane_control(self) -> Option<(usize, i64)> {
        Some(match self {
            Self::X86Avx512MaskSllid
            | Self::X86Avx512MaskSlliq
            | Self::X86Avx512MaskSrlid
            | Self::X86Avx512MaskSrliq
            | Self::X86Avx512MaskSraid
            | Self::X86Avx512MaskSraiq => (3, 255),
            Self::X86Avx512MaskExtracti32x4
            | Self::X86Avx512MaskExtractf32x4
            | Self::X86Avx512MaskStoreExtracti32x4
            | Self::X86Avx512MaskStoreExtractf32x4 => (3, 3),
            Self::X86Avx512MaskExtracti64x4
            | Self::X86Avx512MaskExtractf64x4
            | Self::X86Avx512MaskStoreExtracti64x4
            | Self::X86Avx512MaskStoreExtractf64x4 => (3, 1),
            Self::X86Avx512MaskInserti32x4
            | Self::X86Avx512MaskInsertf32x4
            | Self::X86Avx512MaskLoadInserti32x4
            | Self::X86Avx512MaskLoadInsertf32x4 => (4, 3),
            Self::X86Avx512MaskInserti64x4
            | Self::X86Avx512MaskInsertf64x4
            | Self::X86Avx512MaskLoadInserti64x4
            | Self::X86Avx512MaskLoadInsertf64x4 => (4, 1),
            _ => return None,
        })
    }

    pub fn x86_avx512_lane_memory_bytes(self) -> Option<u16> {
        Some(match self {
            Self::X86Avx512MaskLoadPbroadcastd | Self::X86Avx512MaskLoadBroadcastss => 4,
            Self::X86Avx512MaskLoadPbroadcastq | Self::X86Avx512MaskLoadBroadcastsd => 8,
            Self::X86Avx512MaskLoadBroadcasti32x4
            | Self::X86Avx512MaskLoadBroadcastf32x4
            | Self::X86Avx512MaskStoreExtracti32x4
            | Self::X86Avx512MaskStoreExtractf32x4
            | Self::X86Avx512MaskLoadInserti32x4
            | Self::X86Avx512MaskLoadInsertf32x4 => 16,
            Self::X86Avx512MaskLoadBroadcasti64x4
            | Self::X86Avx512MaskLoadBroadcastf64x4
            | Self::X86Avx512MaskStoreExtracti64x4
            | Self::X86Avx512MaskStoreExtractf64x4
            | Self::X86Avx512MaskLoadInserti64x4
            | Self::X86Avx512MaskLoadInsertf64x4 => 32,
            _ => return None,
        })
    }

    pub fn is_x86_avx512_scalar_move(self) -> bool {
        matches!(
            self,
            Self::X86Avx512MaskMovess
                | Self::X86Avx512MaskMovesd
                | Self::X86Avx512MaskLoadss
                | Self::X86Avx512MaskLoadsd
                | Self::X86Avx512MaskStoress
                | Self::X86Avx512MaskStoresd
        )
    }

    /// The 128- and 256-bit masked core requires both AVX512F and AVX512VL.
    pub fn is_x86_avx512_vl_core(self) -> bool {
        matches!(
            self,
            Self::X86Avx512VlMaskAddd128
                | Self::X86Avx512VlMaskAddq128
                | Self::X86Avx512VlMaskSubd128
                | Self::X86Avx512VlMaskSubq128
                | Self::X86Avx512VlMaskLoadd128
                | Self::X86Avx512VlMaskLoadq128
                | Self::X86Avx512VlMaskStored128
                | Self::X86Avx512VlMaskStoreq128
                | Self::X86Avx512VlCmpd128
                | Self::X86Avx512VlCmpq128
                | Self::X86Avx512VlCmpud128
                | Self::X86Avx512VlCmpuq128
                | Self::X86Avx512VlMaskAddps128
                | Self::X86Avx512VlMaskAddpd128
                | Self::X86Avx512VlMaskSubps128
                | Self::X86Avx512VlMaskSubpd128
                | Self::X86Avx512VlMaskLoadps128
                | Self::X86Avx512VlMaskLoadpd128
                | Self::X86Avx512VlMaskStoreps128
                | Self::X86Avx512VlMaskStorepd128
                | Self::X86Avx512VlCmpps128
                | Self::X86Avx512VlCmppd128
                | Self::X86Avx512VlMaskAddd256
                | Self::X86Avx512VlMaskAddq256
                | Self::X86Avx512VlMaskSubd256
                | Self::X86Avx512VlMaskSubq256
                | Self::X86Avx512VlMaskLoadd256
                | Self::X86Avx512VlMaskLoadq256
                | Self::X86Avx512VlMaskStored256
                | Self::X86Avx512VlMaskStoreq256
                | Self::X86Avx512VlCmpd256
                | Self::X86Avx512VlCmpq256
                | Self::X86Avx512VlCmpud256
                | Self::X86Avx512VlCmpuq256
                | Self::X86Avx512VlMaskAddps256
                | Self::X86Avx512VlMaskAddpd256
                | Self::X86Avx512VlMaskSubps256
                | Self::X86Avx512VlMaskSubpd256
                | Self::X86Avx512VlMaskLoadps256
                | Self::X86Avx512VlMaskLoadpd256
                | Self::X86Avx512VlMaskStoreps256
                | Self::X86Avx512VlMaskStorepd256
                | Self::X86Avx512VlCmpps256
                | Self::X86Avx512VlCmppd256
        )
    }

    /// Shared masked opcode. Operand types determine the physical vector width.
    pub fn x86_avx512_mask_opcode(self) -> Option<&'static str> {
        if let Some((opcode, _)) = self.x86_avx512_cd() {
            return Some(opcode);
        }
        if let Some((opcode, _)) = self.x86_avx512_dq_opcode() {
            return Some(opcode);
        }
        if let Some((opcode, _)) = self.x86_avx512_vl_integer_move() {
            return Some(opcode);
        }
        if let Some((opcode, _)) = self.x86_avx512_vl_float_arithmetic() {
            return Some(opcode);
        }
        Some(match self {
            Self::X86Avx512Cmpd => "vpcmpd",
            Self::X86Avx512Cmpq => "vpcmpq",
            Self::X86Avx512Cmpud => "vpcmpud",
            Self::X86Avx512Cmpuq => "vpcmpuq",
            Self::X86Avx512MaskAddd => "vpaddd",
            Self::X86Avx512MaskSubd => "vpsubd",
            Self::X86Avx512MaskLoadd => "vmovdqu32",
            Self::X86Avx512MaskStored => "vmovdqu32",
            Self::X86Avx512MaskAddq => "vpaddq",
            Self::X86Avx512MaskSubq => "vpsubq",
            Self::X86Avx512MaskLoadq => "vmovdqu64",
            Self::X86Avx512MaskStoreq => "vmovdqu64",
            Self::X86Avx512MaskAddps => "vaddps",
            Self::X86Avx512MaskSubps => "vsubps",
            Self::X86Avx512MaskLoadps => "vmovups",
            Self::X86Avx512MaskStoreps => "vmovups",
            Self::X86Avx512MaskAddpd => "vaddpd",
            Self::X86Avx512MaskSubpd => "vsubpd",
            Self::X86Avx512MaskLoadpd => "vmovupd",
            Self::X86Avx512MaskStorepd => "vmovupd",
            Self::X86Avx512MaskMulps => "vmulps",
            Self::X86Avx512MaskDivps => "vdivps",
            Self::X86Avx512MaskSqrtps => "vsqrtps",
            Self::X86Avx512Cmpps => "vcmpps",
            Self::X86Avx512CmpRoundps => "vcmpps",
            Self::X86Avx512MaskMulpd => "vmulpd",
            Self::X86Avx512MaskDivpd => "vdivpd",
            Self::X86Avx512MaskSqrtpd => "vsqrtpd",
            Self::X86Avx512Cmppd => "vcmppd",
            Self::X86Avx512CmpRoundpd => "vcmppd",
            Self::X86Avx512VlMaskAddd128 => "vpaddd",
            Self::X86Avx512VlMaskAddq128 => "vpaddq",
            Self::X86Avx512VlMaskSubd128 => "vpsubd",
            Self::X86Avx512VlMaskSubq128 => "vpsubq",
            Self::X86Avx512VlMaskLoadd128 => "vmovdqu32",
            Self::X86Avx512VlMaskLoadq128 => "vmovdqu64",
            Self::X86Avx512VlMaskStored128 => "vmovdqu32",
            Self::X86Avx512VlMaskStoreq128 => "vmovdqu64",
            Self::X86Avx512VlCmpd128 => "vpcmpd",
            Self::X86Avx512VlCmpq128 => "vpcmpq",
            Self::X86Avx512VlCmpud128 => "vpcmpud",
            Self::X86Avx512VlCmpuq128 => "vpcmpuq",
            Self::X86Avx512VlMaskAddps128 => "vaddps",
            Self::X86Avx512VlMaskAddpd128 => "vaddpd",
            Self::X86Avx512VlMaskSubps128 => "vsubps",
            Self::X86Avx512VlMaskSubpd128 => "vsubpd",
            Self::X86Avx512VlMaskLoadps128 => "vmovups",
            Self::X86Avx512VlMaskLoadpd128 => "vmovupd",
            Self::X86Avx512VlMaskStoreps128 => "vmovups",
            Self::X86Avx512VlMaskStorepd128 => "vmovupd",
            Self::X86Avx512VlCmpps128 => "vcmpps",
            Self::X86Avx512VlCmppd128 => "vcmppd",
            Self::X86Avx512VlMaskAddd256 => "vpaddd",
            Self::X86Avx512VlMaskAddq256 => "vpaddq",
            Self::X86Avx512VlMaskSubd256 => "vpsubd",
            Self::X86Avx512VlMaskSubq256 => "vpsubq",
            Self::X86Avx512VlMaskLoadd256 => "vmovdqu32",
            Self::X86Avx512VlMaskLoadq256 => "vmovdqu64",
            Self::X86Avx512VlMaskStored256 => "vmovdqu32",
            Self::X86Avx512VlMaskStoreq256 => "vmovdqu64",
            Self::X86Avx512VlCmpd256 => "vpcmpd",
            Self::X86Avx512VlCmpq256 => "vpcmpq",
            Self::X86Avx512VlCmpud256 => "vpcmpud",
            Self::X86Avx512VlCmpuq256 => "vpcmpuq",
            Self::X86Avx512VlMaskAddps256 => "vaddps",
            Self::X86Avx512VlMaskAddpd256 => "vaddpd",
            Self::X86Avx512VlMaskSubps256 => "vsubps",
            Self::X86Avx512VlMaskSubpd256 => "vsubpd",
            Self::X86Avx512VlMaskLoadps256 => "vmovups",
            Self::X86Avx512VlMaskLoadpd256 => "vmovupd",
            Self::X86Avx512VlMaskStoreps256 => "vmovups",
            Self::X86Avx512VlMaskStorepd256 => "vmovupd",
            Self::X86Avx512VlCmpps256 => "vcmpps",
            Self::X86Avx512VlCmppd256 => "vcmppd",
            _ => return None,
        })
    }

    /// Raw VL floating operation with its explicit container shape.
    pub fn x86_avx512_vl_float_arithmetic(self) -> Option<(&'static str, VectorType)> {
        Some(match self {
            Self::X86Avx512VlMaskMulps128 => ("vmulps", VectorType::F32x4),
            Self::X86Avx512VlMaskMulpd128 => ("vmulpd", VectorType::F64x2),
            Self::X86Avx512VlMaskDivps128 => ("vdivps", VectorType::F32x4),
            Self::X86Avx512VlMaskDivpd128 => ("vdivpd", VectorType::F64x2),
            Self::X86Avx512VlMaskSqrtps128 => ("vsqrtps", VectorType::F32x4),
            Self::X86Avx512VlMaskSqrtpd128 => ("vsqrtpd", VectorType::F64x2),
            Self::X86Avx512VlMaskMinps128 => ("vminps", VectorType::F32x4),
            Self::X86Avx512VlMaskMinpd128 => ("vminpd", VectorType::F64x2),
            Self::X86Avx512VlMaskMaxps128 => ("vmaxps", VectorType::F32x4),
            Self::X86Avx512VlMaskMaxpd128 => ("vmaxpd", VectorType::F64x2),
            Self::X86Avx512VlMaskMulps256 => ("vmulps", VectorType::F32x8),
            Self::X86Avx512VlMaskMulpd256 => ("vmulpd", VectorType::F64x4),
            Self::X86Avx512VlMaskDivps256 => ("vdivps", VectorType::F32x8),
            Self::X86Avx512VlMaskDivpd256 => ("vdivpd", VectorType::F64x4),
            Self::X86Avx512VlMaskSqrtps256 => ("vsqrtps", VectorType::F32x8),
            Self::X86Avx512VlMaskSqrtpd256 => ("vsqrtpd", VectorType::F64x4),
            Self::X86Avx512VlMaskMinps256 => ("vminps", VectorType::F32x8),
            Self::X86Avx512VlMaskMinpd256 => ("vminpd", VectorType::F64x4),
            Self::X86Avx512VlMaskMaxps256 => ("vmaxps", VectorType::F32x8),
            Self::X86Avx512VlMaskMaxpd256 => ("vmaxpd", VectorType::F64x4),
            _ => return None,
        })
    }

    /// Explicit operation and container for pure VL integer arithmetic and moves.
    pub fn x86_avx512_vl_integer_move(self) -> Option<(&'static str, VectorType)> {
        Some(match self {
            Self::X86Avx512VlMaskMulld128 => ("vpmulld", VectorType::I32x4),
            Self::X86Avx512VlMaskMinsd128 => ("vpminsd", VectorType::I32x4),
            Self::X86Avx512VlMaskMinud128 => ("vpminud", VectorType::I32x4),
            Self::X86Avx512VlMaskMaxsd128 => ("vpmaxsd", VectorType::I32x4),
            Self::X86Avx512VlMaskMaxud128 => ("vpmaxud", VectorType::I32x4),
            Self::X86Avx512VlMaskAndd128 => ("vpandd", VectorType::I32x4),
            Self::X86Avx512VlMaskAndnd128 => ("vpandnd", VectorType::I32x4),
            Self::X86Avx512VlMaskOrd128 => ("vpord", VectorType::I32x4),
            Self::X86Avx512VlMaskXord128 => ("vpxord", VectorType::I32x4),
            Self::X86Avx512VlMaskMinsq128 => ("vpminsq", VectorType::I64x2),
            Self::X86Avx512VlMaskMinuq128 => ("vpminuq", VectorType::I64x2),
            Self::X86Avx512VlMaskMaxsq128 => ("vpmaxsq", VectorType::I64x2),
            Self::X86Avx512VlMaskMaxuq128 => ("vpmaxuq", VectorType::I64x2),
            Self::X86Avx512VlMaskAndq128 => ("vpandq", VectorType::I64x2),
            Self::X86Avx512VlMaskAndnq128 => ("vpandnq", VectorType::I64x2),
            Self::X86Avx512VlMaskOrq128 => ("vporq", VectorType::I64x2),
            Self::X86Avx512VlMaskXorq128 => ("vpxorq", VectorType::I64x2),
            Self::X86Avx512VlMaskMoved128 => ("vmovdqu32", VectorType::I32x4),
            Self::X86Avx512VlMaskMoveq128 => ("vmovdqu64", VectorType::I64x2),
            Self::X86Avx512VlMaskMoveps128 => ("vmovdqu32", VectorType::F32x4),
            Self::X86Avx512VlMaskMovepd128 => ("vmovdqu64", VectorType::F64x2),
            Self::X86Avx512VlMaskMulld256 => ("vpmulld", VectorType::I32x8),
            Self::X86Avx512VlMaskMinsd256 => ("vpminsd", VectorType::I32x8),
            Self::X86Avx512VlMaskMinud256 => ("vpminud", VectorType::I32x8),
            Self::X86Avx512VlMaskMaxsd256 => ("vpmaxsd", VectorType::I32x8),
            Self::X86Avx512VlMaskMaxud256 => ("vpmaxud", VectorType::I32x8),
            Self::X86Avx512VlMaskAndd256 => ("vpandd", VectorType::I32x8),
            Self::X86Avx512VlMaskAndnd256 => ("vpandnd", VectorType::I32x8),
            Self::X86Avx512VlMaskOrd256 => ("vpord", VectorType::I32x8),
            Self::X86Avx512VlMaskXord256 => ("vpxord", VectorType::I32x8),
            Self::X86Avx512VlMaskMinsq256 => ("vpminsq", VectorType::I64x4),
            Self::X86Avx512VlMaskMinuq256 => ("vpminuq", VectorType::I64x4),
            Self::X86Avx512VlMaskMaxsq256 => ("vpmaxsq", VectorType::I64x4),
            Self::X86Avx512VlMaskMaxuq256 => ("vpmaxuq", VectorType::I64x4),
            Self::X86Avx512VlMaskAndq256 => ("vpandq", VectorType::I64x4),
            Self::X86Avx512VlMaskAndnq256 => ("vpandnq", VectorType::I64x4),
            Self::X86Avx512VlMaskOrq256 => ("vporq", VectorType::I64x4),
            Self::X86Avx512VlMaskXorq256 => ("vpxorq", VectorType::I64x4),
            Self::X86Avx512VlMaskMoved256 => ("vmovdqu32", VectorType::I32x8),
            Self::X86Avx512VlMaskMoveq256 => ("vmovdqu64", VectorType::I64x4),
            Self::X86Avx512VlMaskMoveps256 => ("vmovdqu32", VectorType::F32x8),
            Self::X86Avx512VlMaskMovepd256 => ("vmovdqu64", VectorType::F64x4),
            _ => return None,
        })
    }

    pub fn info(self) -> IntrinsicInfo {
        const MEM: &[Type] = &[Type::Ptr, Type::Ptr, Type::I64];
        let (name, params, returns, pure): (_, &'static [Type], &'static [Type], _) = match self {
            Intrinsic::X86Avx512BwAddb128
            | Intrinsic::X86Avx512BwSubb128
            | Intrinsic::X86Avx512BwAddsb128
            | Intrinsic::X86Avx512BwSubsb128
            | Intrinsic::X86Avx512BwAddusb128
            | Intrinsic::X86Avx512BwSubusb128
            | Intrinsic::X86Avx512BwMinsb128
            | Intrinsic::X86Avx512BwMinub128
            | Intrinsic::X86Avx512BwMaxsb128
            | Intrinsic::X86Avx512BwMaxub128
            | Intrinsic::X86Avx512BwMoveb128
            | Intrinsic::X86Avx512BwLoadb128
            | Intrinsic::X86Avx512BwStoreb128
            | Intrinsic::X86Avx512BwAddw128
            | Intrinsic::X86Avx512BwSubw128
            | Intrinsic::X86Avx512BwAddsw128
            | Intrinsic::X86Avx512BwSubsw128
            | Intrinsic::X86Avx512BwAddusw128
            | Intrinsic::X86Avx512BwSubusw128
            | Intrinsic::X86Avx512BwMinsw128
            | Intrinsic::X86Avx512BwMinuw128
            | Intrinsic::X86Avx512BwMaxsw128
            | Intrinsic::X86Avx512BwMaxuw128
            | Intrinsic::X86Avx512BwMovew128
            | Intrinsic::X86Avx512BwLoadw128
            | Intrinsic::X86Avx512BwStorew128
            | Intrinsic::X86Avx512BwAddb256
            | Intrinsic::X86Avx512BwSubb256
            | Intrinsic::X86Avx512BwAddsb256
            | Intrinsic::X86Avx512BwSubsb256
            | Intrinsic::X86Avx512BwAddusb256
            | Intrinsic::X86Avx512BwSubusb256
            | Intrinsic::X86Avx512BwMinsb256
            | Intrinsic::X86Avx512BwMinub256
            | Intrinsic::X86Avx512BwMaxsb256
            | Intrinsic::X86Avx512BwMaxub256
            | Intrinsic::X86Avx512BwMoveb256
            | Intrinsic::X86Avx512BwLoadb256
            | Intrinsic::X86Avx512BwStoreb256
            | Intrinsic::X86Avx512BwAddw256
            | Intrinsic::X86Avx512BwSubw256
            | Intrinsic::X86Avx512BwAddsw256
            | Intrinsic::X86Avx512BwSubsw256
            | Intrinsic::X86Avx512BwAddusw256
            | Intrinsic::X86Avx512BwSubusw256
            | Intrinsic::X86Avx512BwMinsw256
            | Intrinsic::X86Avx512BwMinuw256
            | Intrinsic::X86Avx512BwMaxsw256
            | Intrinsic::X86Avx512BwMaxuw256
            | Intrinsic::X86Avx512BwMovew256
            | Intrinsic::X86Avx512BwLoadw256
            | Intrinsic::X86Avx512BwStorew256
            | Intrinsic::X86Avx512BwAddb512
            | Intrinsic::X86Avx512BwSubb512
            | Intrinsic::X86Avx512BwAddsb512
            | Intrinsic::X86Avx512BwSubsb512
            | Intrinsic::X86Avx512BwAddusb512
            | Intrinsic::X86Avx512BwSubusb512
            | Intrinsic::X86Avx512BwMinsb512
            | Intrinsic::X86Avx512BwMinub512
            | Intrinsic::X86Avx512BwMaxsb512
            | Intrinsic::X86Avx512BwMaxub512
            | Intrinsic::X86Avx512BwMoveb512
            | Intrinsic::X86Avx512BwLoadb512
            | Intrinsic::X86Avx512BwStoreb512
            | Intrinsic::X86Avx512BwAddw512
            | Intrinsic::X86Avx512BwSubw512
            | Intrinsic::X86Avx512BwAddsw512
            | Intrinsic::X86Avx512BwSubsw512
            | Intrinsic::X86Avx512BwAddusw512
            | Intrinsic::X86Avx512BwSubusw512
            | Intrinsic::X86Avx512BwMinsw512
            | Intrinsic::X86Avx512BwMinuw512
            | Intrinsic::X86Avx512BwMaxsw512
            | Intrinsic::X86Avx512BwMaxuw512
            | Intrinsic::X86Avx512BwMovew512
            | Intrinsic::X86Avx512BwLoadw512
            | Intrinsic::X86Avx512BwStorew512 => {
                let descriptor = self.x86_avx512_bw_core().expect("BW core descriptor");
                (
                    descriptor.name,
                    descriptor.params,
                    descriptor.returns,
                    !matches!(
                        descriptor.kind,
                        X86Avx512BwKind::Load | X86Avx512BwKind::Store
                    ),
                )
            }
            Intrinsic::X86Avx512BwCmpb128
            | Intrinsic::X86Avx512BwCmpub128
            | Intrinsic::X86Avx512BwTestb128
            | Intrinsic::X86Avx512BwTestnb128
            | Intrinsic::X86Avx512BwMovb2m128
            | Intrinsic::X86Avx512BwMovm2b128
            | Intrinsic::X86Avx512BwCmpw128
            | Intrinsic::X86Avx512BwCmpuw128
            | Intrinsic::X86Avx512BwTestw128
            | Intrinsic::X86Avx512BwTestnw128
            | Intrinsic::X86Avx512BwMovw2m128
            | Intrinsic::X86Avx512BwMovm2w128
            | Intrinsic::X86Avx512BwCmpb256
            | Intrinsic::X86Avx512BwCmpub256
            | Intrinsic::X86Avx512BwTestb256
            | Intrinsic::X86Avx512BwTestnb256
            | Intrinsic::X86Avx512BwMovb2m256
            | Intrinsic::X86Avx512BwMovm2b256
            | Intrinsic::X86Avx512BwCmpw256
            | Intrinsic::X86Avx512BwCmpuw256
            | Intrinsic::X86Avx512BwTestw256
            | Intrinsic::X86Avx512BwTestnw256
            | Intrinsic::X86Avx512BwMovw2m256
            | Intrinsic::X86Avx512BwMovm2w256
            | Intrinsic::X86Avx512BwCmpb512
            | Intrinsic::X86Avx512BwCmpub512
            | Intrinsic::X86Avx512BwTestb512
            | Intrinsic::X86Avx512BwTestnb512
            | Intrinsic::X86Avx512BwMovb2m512
            | Intrinsic::X86Avx512BwMovm2b512
            | Intrinsic::X86Avx512BwCmpw512
            | Intrinsic::X86Avx512BwCmpuw512
            | Intrinsic::X86Avx512BwTestw512
            | Intrinsic::X86Avx512BwTestnw512
            | Intrinsic::X86Avx512BwMovw2m512
            | Intrinsic::X86Avx512BwMovm2w512 => {
                let descriptor = self.x86_avx512_bw_mask().expect("BW mask descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512VlSllid128
            | Intrinsic::X86Avx512VlSlliq128
            | Intrinsic::X86Avx512VlSrlid128
            | Intrinsic::X86Avx512VlSrliq128
            | Intrinsic::X86Avx512VlSraid128
            | Intrinsic::X86Avx512VlSraiq128
            | Intrinsic::X86Avx512VlSlld128
            | Intrinsic::X86Avx512VlSllq128
            | Intrinsic::X86Avx512VlSrld128
            | Intrinsic::X86Avx512VlSrlq128
            | Intrinsic::X86Avx512VlSrad128
            | Intrinsic::X86Avx512VlSraq128
            | Intrinsic::X86Avx512VlSllvd128
            | Intrinsic::X86Avx512VlSllvq128
            | Intrinsic::X86Avx512VlSrlvd128
            | Intrinsic::X86Avx512VlSrlvq128
            | Intrinsic::X86Avx512VlSravd128
            | Intrinsic::X86Avx512VlSravq128
            | Intrinsic::X86Avx512VlRold128
            | Intrinsic::X86Avx512VlRolq128
            | Intrinsic::X86Avx512VlRord128
            | Intrinsic::X86Avx512VlRorq128
            | Intrinsic::X86Avx512VlRolvd128
            | Intrinsic::X86Avx512VlRolvq128
            | Intrinsic::X86Avx512VlRorvd128
            | Intrinsic::X86Avx512VlRorvq128
            | Intrinsic::X86Avx512VlSllid256
            | Intrinsic::X86Avx512VlSlliq256
            | Intrinsic::X86Avx512VlSrlid256
            | Intrinsic::X86Avx512VlSrliq256
            | Intrinsic::X86Avx512VlSraid256
            | Intrinsic::X86Avx512VlSraiq256
            | Intrinsic::X86Avx512VlSlld256
            | Intrinsic::X86Avx512VlSllq256
            | Intrinsic::X86Avx512VlSrld256
            | Intrinsic::X86Avx512VlSrlq256
            | Intrinsic::X86Avx512VlSrad256
            | Intrinsic::X86Avx512VlSraq256
            | Intrinsic::X86Avx512VlSllvd256
            | Intrinsic::X86Avx512VlSllvq256
            | Intrinsic::X86Avx512VlSrlvd256
            | Intrinsic::X86Avx512VlSrlvq256
            | Intrinsic::X86Avx512VlSravd256
            | Intrinsic::X86Avx512VlSravq256
            | Intrinsic::X86Avx512VlRold256
            | Intrinsic::X86Avx512VlRolq256
            | Intrinsic::X86Avx512VlRord256
            | Intrinsic::X86Avx512VlRorq256
            | Intrinsic::X86Avx512VlRolvd256
            | Intrinsic::X86Avx512VlRolvq256
            | Intrinsic::X86Avx512VlRorvd256
            | Intrinsic::X86Avx512VlRorvq256 => {
                let descriptor = self.x86_avx512_vl_shift().expect("VL shift descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512BwSlliw128
            | Intrinsic::X86Avx512BwSllw128
            | Intrinsic::X86Avx512BwSllvw128
            | Intrinsic::X86Avx512BwSrliw128
            | Intrinsic::X86Avx512BwSrlw128
            | Intrinsic::X86Avx512BwSrlvw128
            | Intrinsic::X86Avx512BwSraiw128
            | Intrinsic::X86Avx512BwSraw128
            | Intrinsic::X86Avx512BwSravw128
            | Intrinsic::X86Avx512BwSlliw256
            | Intrinsic::X86Avx512BwSllw256
            | Intrinsic::X86Avx512BwSllvw256
            | Intrinsic::X86Avx512BwSrliw256
            | Intrinsic::X86Avx512BwSrlw256
            | Intrinsic::X86Avx512BwSrlvw256
            | Intrinsic::X86Avx512BwSraiw256
            | Intrinsic::X86Avx512BwSraw256
            | Intrinsic::X86Avx512BwSravw256
            | Intrinsic::X86Avx512BwSlliw512
            | Intrinsic::X86Avx512BwSllw512
            | Intrinsic::X86Avx512BwSllvw512
            | Intrinsic::X86Avx512BwSrliw512
            | Intrinsic::X86Avx512BwSrlw512
            | Intrinsic::X86Avx512BwSrlvw512
            | Intrinsic::X86Avx512BwSraiw512
            | Intrinsic::X86Avx512BwSraw512
            | Intrinsic::X86Avx512BwSravw512 => {
                let descriptor = self.x86_avx512_bw_shift().expect("BW shift descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512BwMullw128
            | Intrinsic::X86Avx512BwMulhw128
            | Intrinsic::X86Avx512BwMulhuw128
            | Intrinsic::X86Avx512BwMulhrsw128
            | Intrinsic::X86Avx512BwMaddwd128
            | Intrinsic::X86Avx512BwMaddubsw128
            | Intrinsic::X86Avx512BwAvgb128
            | Intrinsic::X86Avx512BwAvgw128
            | Intrinsic::X86Avx512BwMullw256
            | Intrinsic::X86Avx512BwMulhw256
            | Intrinsic::X86Avx512BwMulhuw256
            | Intrinsic::X86Avx512BwMulhrsw256
            | Intrinsic::X86Avx512BwMaddwd256
            | Intrinsic::X86Avx512BwMaddubsw256
            | Intrinsic::X86Avx512BwAvgb256
            | Intrinsic::X86Avx512BwAvgw256
            | Intrinsic::X86Avx512BwMullw512
            | Intrinsic::X86Avx512BwMulhw512
            | Intrinsic::X86Avx512BwMulhuw512
            | Intrinsic::X86Avx512BwMulhrsw512
            | Intrinsic::X86Avx512BwMaddwd512
            | Intrinsic::X86Avx512BwMaddubsw512
            | Intrinsic::X86Avx512BwAvgb512
            | Intrinsic::X86Avx512BwAvgw512 => {
                let descriptor = self.x86_avx512_bw_mul().expect("BW multiply descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512BwPacksswb128
            | Intrinsic::X86Avx512BwPackuswb128
            | Intrinsic::X86Avx512BwPackssdw128
            | Intrinsic::X86Avx512BwPackusdw128
            | Intrinsic::X86Avx512BwUnpcklbw128
            | Intrinsic::X86Avx512BwUnpckhbw128
            | Intrinsic::X86Avx512BwUnpcklwd128
            | Intrinsic::X86Avx512BwUnpckhwd128
            | Intrinsic::X86Avx512BwPacksswb256
            | Intrinsic::X86Avx512BwPackuswb256
            | Intrinsic::X86Avx512BwPackssdw256
            | Intrinsic::X86Avx512BwPackusdw256
            | Intrinsic::X86Avx512BwUnpcklbw256
            | Intrinsic::X86Avx512BwUnpckhbw256
            | Intrinsic::X86Avx512BwUnpcklwd256
            | Intrinsic::X86Avx512BwUnpckhwd256
            | Intrinsic::X86Avx512BwPacksswb512
            | Intrinsic::X86Avx512BwPackuswb512
            | Intrinsic::X86Avx512BwPackssdw512
            | Intrinsic::X86Avx512BwPackusdw512
            | Intrinsic::X86Avx512BwUnpcklbw512
            | Intrinsic::X86Avx512BwUnpckhbw512
            | Intrinsic::X86Avx512BwUnpcklwd512
            | Intrinsic::X86Avx512BwUnpckhwd512 => {
                let descriptor = self
                    .x86_avx512_bw_pack()
                    .expect("BW pack/unpack descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512MaskTernlogd => (
                "x86_avx512_maskternlogd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskTernlogq => (
                "x86_avx512_maskternlogq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskRold => (
                "x86_avx512_maskrold",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskRolq => (
                "x86_avx512_maskrolq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskRord => (
                "x86_avx512_maskrord",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskRorq => (
                "x86_avx512_maskrorq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskRolvd => (
                "x86_avx512_maskrolvd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskRolvq => (
                "x86_avx512_maskrolvq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskRorvd => (
                "x86_avx512_maskrorvd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskRorvq => (
                "x86_avx512_maskrorvq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSllvd => (
                "x86_avx512_masksllvd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSllvq => (
                "x86_avx512_masksllvq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrlvd => (
                "x86_avx512_masksrlvd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrlvq => (
                "x86_avx512_masksrlvq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSravd => (
                "x86_avx512_masksravd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSravq => (
                "x86_avx512_masksravq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Testd => (
                "x86_avx512_testd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                ],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Testq => (
                "x86_avx512_testq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512Testnd => (
                "x86_avx512_testnd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                ],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Testnq => (
                "x86_avx512_testnq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512MaskAbsd => (
                "x86_avx512_maskabsd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskAbsq => (
                "x86_avx512_maskabsq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMuldq => (
                "x86_avx512_maskmuldq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMuludq => (
                "x86_avx512_maskmuludq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Kandw => (
                "x86_avx512_kandw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Kandnw => (
                "x86_avx512_kandnw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Korw => (
                "x86_avx512_korw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Kxorw => (
                "x86_avx512_kxorw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Kxnorw => (
                "x86_avx512_kxnorw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Knotw => ("x86_avx512_knotw", &[Type::I16], &[Type::I16], true),
            Intrinsic::X86Avx512Kshiftlw => (
                "x86_avx512_kshiftlw",
                &[Type::I16, Type::I32],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Kshiftrw => (
                "x86_avx512_kshiftrw",
                &[Type::I16, Type::I32],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Kunpckbw => (
                "x86_avx512_kunpckbw",
                &[Type::I16, Type::I16],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Ktestw => (
                "x86_avx512_ktestw",
                &[Type::I16, Type::I16],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512Kortestw => (
                "x86_avx512_kortestw",
                &[Type::I16, Type::I16],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsxbd => (
                "x86_avx512_maskpmovsxbd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I8x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsxbq => (
                "x86_avx512_maskpmovsxbq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I8x16),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsxwd => (
                "x86_avx512_maskpmovsxwd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I16x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsxwq => (
                "x86_avx512_maskpmovsxwq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I16x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsxdq => (
                "x86_avx512_maskpmovsxdq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovzxbd => (
                "x86_avx512_maskpmovzxbd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I8x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovzxbq => (
                "x86_avx512_maskpmovzxbq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I8x16),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovzxwd => (
                "x86_avx512_maskpmovzxwd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I16x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovzxwq => (
                "x86_avx512_maskpmovzxwq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I16x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovzxdq => (
                "x86_avx512_maskpmovzxdq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadPmovsxbd => (
                "x86_avx512_maskloadpmovsxbd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovsxbq => (
                "x86_avx512_maskloadpmovsxbq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovsxwd => (
                "x86_avx512_maskloadpmovsxwd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovsxwq => (
                "x86_avx512_maskloadpmovsxwq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovsxdq => (
                "x86_avx512_maskloadpmovsxdq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovzxbd => (
                "x86_avx512_maskloadpmovzxbd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovzxbq => (
                "x86_avx512_maskloadpmovzxbq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovzxwd => (
                "x86_avx512_maskloadpmovzxwd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovzxwq => (
                "x86_avx512_maskloadpmovzxwq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPmovzxdq => (
                "x86_avx512_maskloadpmovzxdq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskPmovdb => (
                "x86_avx512_maskpmovdb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovdw => (
                "x86_avx512_maskpmovdw",
                &[
                    Type::Vector(VectorType::I16x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovqb => (
                "x86_avx512_maskpmovqb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovqd => (
                "x86_avx512_maskpmovqd",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovqw => (
                "x86_avx512_maskpmovqw",
                &[
                    Type::Vector(VectorType::I16x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsdb => (
                "x86_avx512_maskpmovsdb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsdw => (
                "x86_avx512_maskpmovsdw",
                &[
                    Type::Vector(VectorType::I16x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsqb => (
                "x86_avx512_maskpmovsqb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsqd => (
                "x86_avx512_maskpmovsqd",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovsqw => (
                "x86_avx512_maskpmovsqw",
                &[
                    Type::Vector(VectorType::I16x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovusdb => (
                "x86_avx512_maskpmovusdb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovusdw => (
                "x86_avx512_maskpmovusdw",
                &[
                    Type::Vector(VectorType::I16x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovusqb => (
                "x86_avx512_maskpmovusqb",
                &[
                    Type::Vector(VectorType::I8x16),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovusqd => (
                "x86_avx512_maskpmovusqd",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPmovusqw => (
                "x86_avx512_maskpmovusqw",
                &[
                    Type::Vector(VectorType::I16x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskStorePmovdb => (
                "x86_avx512_maskstorepmovdb",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovdw => (
                "x86_avx512_maskstorepmovdw",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovqb => (
                "x86_avx512_maskstorepmovqb",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovqd => (
                "x86_avx512_maskstorepmovqd",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovqw => (
                "x86_avx512_maskstorepmovqw",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovsdb => (
                "x86_avx512_maskstorepmovsdb",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovsdw => (
                "x86_avx512_maskstorepmovsdw",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovsqb => (
                "x86_avx512_maskstorepmovsqb",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovsqd => (
                "x86_avx512_maskstorepmovsqd",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovsqw => (
                "x86_avx512_maskstorepmovsqw",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovusdb => (
                "x86_avx512_maskstorepmovusdb",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovusdw => (
                "x86_avx512_maskstorepmovusdw",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovusqb => (
                "x86_avx512_maskstorepmovusqb",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovusqd => (
                "x86_avx512_maskstorepmovusqd",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorePmovusqw => (
                "x86_avx512_maskstorepmovusqw",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskSllid => (
                "x86_avx512_masksllid",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSlliq => (
                "x86_avx512_maskslliq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrlid => (
                "x86_avx512_masksrlid",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrliq => (
                "x86_avx512_masksrliq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSraid => (
                "x86_avx512_masksraid",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSraiq => (
                "x86_avx512_masksraiq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSlld => (
                "x86_avx512_maskslld",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSllq => (
                "x86_avx512_masksllq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrld => (
                "x86_avx512_masksrld",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrlq => (
                "x86_avx512_masksrlq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSrad => (
                "x86_avx512_masksrad",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSraq => (
                "x86_avx512_masksraq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskPbroadcastd => (
                "x86_avx512_maskpbroadcastd",
                &[Type::Vector(VectorType::I32x16), Type::I16, Type::I32],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskPbroadcastq => (
                "x86_avx512_maskpbroadcastq",
                &[Type::Vector(VectorType::I64x8), Type::I8, Type::I64],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskBroadcastss => (
                "x86_avx512_maskbroadcastss",
                &[Type::Vector(VectorType::F32x16), Type::I16, Type::F32],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskBroadcastsd => (
                "x86_avx512_maskbroadcastsd",
                &[Type::Vector(VectorType::F64x8), Type::I8, Type::F64],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadPbroadcastd => (
                "x86_avx512_maskloadpbroadcastd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadPbroadcastq => (
                "x86_avx512_maskloadpbroadcastq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcastss => (
                "x86_avx512_maskloadbroadcastss",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcastsd => (
                "x86_avx512_maskloadbroadcastsd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskBroadcasti32x4 => (
                "x86_avx512_maskbroadcasti32x4",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskBroadcasti64x4 => (
                "x86_avx512_maskbroadcasti64x4",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskBroadcastf32x4 => (
                "x86_avx512_maskbroadcastf32x4",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskBroadcastf64x4 => (
                "x86_avx512_maskbroadcastf64x4",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcasti32x4 => (
                "x86_avx512_maskloadbroadcasti32x4",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcasti64x4 => (
                "x86_avx512_maskloadbroadcasti64x4",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcastf32x4 => (
                "x86_avx512_maskloadbroadcastf32x4",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadBroadcastf64x4 => (
                "x86_avx512_maskloadbroadcastf64x4",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskExtracti32x4 => (
                "x86_avx512_maskextracti32x4",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512MaskExtracti64x4 => (
                "x86_avx512_maskextracti64x4",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512MaskExtractf32x4 => (
                "x86_avx512_maskextractf32x4",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512MaskExtractf64x4 => (
                "x86_avx512_maskextractf64x4",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512MaskStoreExtracti32x4 => (
                "x86_avx512_maskstoreextracti32x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStoreExtracti64x4 => (
                "x86_avx512_maskstoreextracti64x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStoreExtractf32x4 => (
                "x86_avx512_maskstoreextractf32x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::F32x16),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStoreExtractf64x4 => (
                "x86_avx512_maskstoreextractf64x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskInserti32x4 => (
                "x86_avx512_maskinserti32x4",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskInserti64x4 => (
                "x86_avx512_maskinserti64x4",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskInsertf32x4 => (
                "x86_avx512_maskinsertf32x4",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskInsertf64x4 => (
                "x86_avx512_maskinsertf64x4",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadInserti32x4 => (
                "x86_avx512_maskloadinserti32x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadInserti64x4 => (
                "x86_avx512_maskloadinserti64x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadInsertf32x4 => (
                "x86_avx512_maskloadinsertf32x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadInsertf64x4 => (
                "x86_avx512_maskloadinsertf64x4",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            x86_system_intrinsics!(all) => return self.generated_system_info().unwrap(),
            x86_query_intrinsics!(all) => return self.generated_query_info().unwrap(),
            Intrinsic::X86Avx512Compressd => (
                "x86_avx512_compressd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Compressq => (
                "x86_avx512_compressq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Compressps => (
                "x86_avx512_compressps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Compresspd => (
                "x86_avx512_compresspd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Expandd => (
                "x86_avx512_expandd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Expandq => (
                "x86_avx512_expandq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Expandps => (
                "x86_avx512_expandps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Expandpd => (
                "x86_avx512_expandpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512CompressStored => (
                "x86_avx512_compressstored",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512CompressStoreq => (
                "x86_avx512_compressstoreq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512CompressStoreps => (
                "x86_avx512_compressstoreps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512CompressStorepd => (
                "x86_avx512_compressstorepd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512ExpandLoadd => (
                "x86_avx512_expandloadd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512ExpandLoadq => (
                "x86_avx512_expandloadq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512ExpandLoadps => (
                "x86_avx512_expandloadps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512ExpandLoadpd => (
                "x86_avx512_expandloadpd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512VlGetexpps128
            | Intrinsic::X86Avx512VlGetexppd128
            | Intrinsic::X86Avx512VlGetmantps128
            | Intrinsic::X86Avx512VlGetmantpd128
            | Intrinsic::X86Avx512VlScalefps128
            | Intrinsic::X86Avx512VlScalefpd128
            | Intrinsic::X86Avx512VlRcp14ps128
            | Intrinsic::X86Avx512VlRcp14pd128
            | Intrinsic::X86Avx512VlRsqrt14ps128
            | Intrinsic::X86Avx512VlRsqrt14pd128
            | Intrinsic::X86Avx512VlRoundscaleps128
            | Intrinsic::X86Avx512VlRoundscalepd128
            | Intrinsic::X86Avx512VlFixupimmps128
            | Intrinsic::X86Avx512VlFixupimmpd128
            | Intrinsic::X86Avx512VlGetexpps256
            | Intrinsic::X86Avx512VlGetexppd256
            | Intrinsic::X86Avx512VlGetmantps256
            | Intrinsic::X86Avx512VlGetmantpd256
            | Intrinsic::X86Avx512VlScalefps256
            | Intrinsic::X86Avx512VlScalefpd256
            | Intrinsic::X86Avx512VlRcp14ps256
            | Intrinsic::X86Avx512VlRcp14pd256
            | Intrinsic::X86Avx512VlRsqrt14ps256
            | Intrinsic::X86Avx512VlRsqrt14pd256
            | Intrinsic::X86Avx512VlRoundscaleps256
            | Intrinsic::X86Avx512VlRoundscalepd256
            | Intrinsic::X86Avx512VlFixupimmps256
            | Intrinsic::X86Avx512VlFixupimmpd256 => {
                let info = avx512_vl_special::info(self);
                (info.name, info.params, info.returns, info.pure)
            }
            Intrinsic::X86Avx512Getexpps => (
                "x86_avx512_getexpps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Getexppd => (
                "x86_avx512_getexppd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Getmantps => (
                "x86_avx512_getmantps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Getmantpd => (
                "x86_avx512_getmantpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Scalefps => (
                "x86_avx512_scalefps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Scalefpd => (
                "x86_avx512_scalefpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Rcp14ps => (
                "x86_avx512_rcp14ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Rcp14pd => (
                "x86_avx512_rcp14pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Rsqrt14ps => (
                "x86_avx512_rsqrt14ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Rsqrt14pd => (
                "x86_avx512_rsqrt14pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Roundscaleps => (
                "x86_avx512_roundscaleps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Roundscalepd => (
                "x86_avx512_roundscalepd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Fixupimmps => (
                "x86_avx512_fixupimmps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Fixupimmpd => (
                "x86_avx512_fixupimmpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMovess => (
                "x86_avx512_maskmovess",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512MaskMovesd => (
                "x86_avx512_maskmovesd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadss => (
                "x86_avx512_maskloadss",
                &[Type::Ptr, Type::Vector(VectorType::F32x4), Type::I8],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadsd => (
                "x86_avx512_maskloadsd",
                &[Type::Ptr, Type::Vector(VectorType::F64x2), Type::I8],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512MaskStoress => (
                "x86_avx512_maskstoress",
                &[Type::Ptr, Type::Vector(VectorType::F32x4), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStoresd => (
                "x86_avx512_maskstoresd",
                &[Type::Ptr, Type::Vector(VectorType::F64x2), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMulld128 => (
                "x86_avx512vl_maskmulld128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinsd128 => (
                "x86_avx512vl_maskminsd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinud128 => (
                "x86_avx512vl_maskminud128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxsd128 => (
                "x86_avx512vl_maskmaxsd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxud128 => (
                "x86_avx512vl_maskmaxud128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndd128 => (
                "x86_avx512vl_maskandd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndnd128 => (
                "x86_avx512vl_maskandnd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskOrd128 => (
                "x86_avx512vl_maskord128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskXord128 => (
                "x86_avx512vl_maskxord128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinsq128 => (
                "x86_avx512vl_maskminsq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinuq128 => (
                "x86_avx512vl_maskminuq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxsq128 => (
                "x86_avx512vl_maskmaxsq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxuq128 => (
                "x86_avx512vl_maskmaxuq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndq128 => (
                "x86_avx512vl_maskandq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndnq128 => (
                "x86_avx512vl_maskandnq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskOrq128 => (
                "x86_avx512vl_maskorq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskXorq128 => (
                "x86_avx512vl_maskxorq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoved128 => (
                "x86_avx512vl_maskmoved128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoveq128 => (
                "x86_avx512vl_maskmoveq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoveps128 => (
                "x86_avx512vl_maskmoveps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMovepd128 => (
                "x86_avx512vl_maskmovepd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMulld256 => (
                "x86_avx512vl_maskmulld256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinsd256 => (
                "x86_avx512vl_maskminsd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinud256 => (
                "x86_avx512vl_maskminud256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxsd256 => (
                "x86_avx512vl_maskmaxsd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxud256 => (
                "x86_avx512vl_maskmaxud256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndd256 => (
                "x86_avx512vl_maskandd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndnd256 => (
                "x86_avx512vl_maskandnd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskOrd256 => (
                "x86_avx512vl_maskord256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskXord256 => (
                "x86_avx512vl_maskxord256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinsq256 => (
                "x86_avx512vl_maskminsq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMinuq256 => (
                "x86_avx512vl_maskminuq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxsq256 => (
                "x86_avx512vl_maskmaxsq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMaxuq256 => (
                "x86_avx512vl_maskmaxuq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndq256 => (
                "x86_avx512vl_maskandq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAndnq256 => (
                "x86_avx512vl_maskandnq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskOrq256 => (
                "x86_avx512vl_maskorq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskXorq256 => (
                "x86_avx512vl_maskxorq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoved256 => (
                "x86_avx512vl_maskmoved256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoveq256 => (
                "x86_avx512vl_maskmoveq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMoveps256 => (
                "x86_avx512vl_maskmoveps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMovepd256 => (
                "x86_avx512vl_maskmovepd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskMulps128 => (
                "x86_avx512vl_maskmulps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMulpd128 => (
                "x86_avx512vl_maskmulpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskDivps128 => (
                "x86_avx512vl_maskdivps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskDivpd128 => (
                "x86_avx512vl_maskdivpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSqrtps128 => (
                "x86_avx512vl_masksqrtps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSqrtpd128 => (
                "x86_avx512vl_masksqrtpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMinps128 => (
                "x86_avx512vl_maskminps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMinpd128 => (
                "x86_avx512vl_maskminpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMaxps128 => (
                "x86_avx512vl_maskmaxps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMaxpd128 => (
                "x86_avx512vl_maskmaxpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMulps256 => (
                "x86_avx512vl_maskmulps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMulpd256 => (
                "x86_avx512vl_maskmulpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskDivps256 => (
                "x86_avx512vl_maskdivps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskDivpd256 => (
                "x86_avx512vl_maskdivpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSqrtps256 => (
                "x86_avx512vl_masksqrtps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSqrtpd256 => (
                "x86_avx512vl_masksqrtpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMinps256 => (
                "x86_avx512vl_maskminps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMinpd256 => (
                "x86_avx512vl_maskminpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMaxps256 => (
                "x86_avx512vl_maskmaxps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskMaxpd256 => (
                "x86_avx512vl_maskmaxpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskAddd128 => (
                "x86_avx512vl_maskaddd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAddq128 => (
                "x86_avx512vl_maskaddq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskSubd128 => (
                "x86_avx512vl_masksubd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskSubq128 => (
                "x86_avx512vl_masksubq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskLoadd128 => (
                "x86_avx512vl_maskloadd128",
                &[Type::Ptr, Type::Vector(VectorType::I32x4), Type::I8],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadq128 => (
                "x86_avx512vl_maskloadq128",
                &[Type::Ptr, Type::Vector(VectorType::I64x2), Type::I8],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStored128 => (
                "x86_avx512vl_maskstored128",
                &[Type::Ptr, Type::Vector(VectorType::I32x4), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStoreq128 => (
                "x86_avx512vl_maskstoreq128",
                &[Type::Ptr, Type::Vector(VectorType::I64x2), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlCmpd128 => (
                "x86_avx512vl_cmpd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpq128 => (
                "x86_avx512vl_cmpq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpud128 => (
                "x86_avx512vl_cmpud128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpuq128 => (
                "x86_avx512vl_cmpuq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAddps128 => (
                "x86_avx512vl_maskaddps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskAddpd128 => (
                "x86_avx512vl_maskaddpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSubps128 => (
                "x86_avx512vl_masksubps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSubpd128 => (
                "x86_avx512vl_masksubpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadps128 => (
                "x86_avx512vl_maskloadps128",
                &[Type::Ptr, Type::Vector(VectorType::F32x4), Type::I8],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadpd128 => (
                "x86_avx512vl_maskloadpd128",
                &[Type::Ptr, Type::Vector(VectorType::F64x2), Type::I8],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStoreps128 => (
                "x86_avx512vl_maskstoreps128",
                &[Type::Ptr, Type::Vector(VectorType::F32x4), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStorepd128 => (
                "x86_avx512vl_maskstorepd128",
                &[Type::Ptr, Type::Vector(VectorType::F64x2), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlCmpps128 => (
                "x86_avx512vl_cmpps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512VlCmppd128 => (
                "x86_avx512vl_cmppd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512VlMaskAddd256 => (
                "x86_avx512vl_maskaddd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAddq256 => (
                "x86_avx512vl_maskaddq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskSubd256 => (
                "x86_avx512vl_masksubd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskSubq256 => (
                "x86_avx512vl_masksubq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512VlMaskLoadd256 => (
                "x86_avx512vl_maskloadd256",
                &[Type::Ptr, Type::Vector(VectorType::I32x8), Type::I8],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadq256 => (
                "x86_avx512vl_maskloadq256",
                &[Type::Ptr, Type::Vector(VectorType::I64x4), Type::I8],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStored256 => (
                "x86_avx512vl_maskstored256",
                &[Type::Ptr, Type::Vector(VectorType::I32x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStoreq256 => (
                "x86_avx512vl_maskstoreq256",
                &[Type::Ptr, Type::Vector(VectorType::I64x4), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlCmpd256 => (
                "x86_avx512vl_cmpd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpq256 => (
                "x86_avx512vl_cmpq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpud256 => (
                "x86_avx512vl_cmpud256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlCmpuq256 => (
                "x86_avx512vl_cmpuq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512VlMaskAddps256 => (
                "x86_avx512vl_maskaddps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskAddpd256 => (
                "x86_avx512vl_maskaddpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSubps256 => (
                "x86_avx512vl_masksubps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskSubpd256 => (
                "x86_avx512vl_masksubpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadps256 => (
                "x86_avx512vl_maskloadps256",
                &[Type::Ptr, Type::Vector(VectorType::F32x8), Type::I8],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskLoadpd256 => (
                "x86_avx512vl_maskloadpd256",
                &[Type::Ptr, Type::Vector(VectorType::F64x4), Type::I8],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStoreps256 => (
                "x86_avx512vl_maskstoreps256",
                &[Type::Ptr, Type::Vector(VectorType::F32x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlMaskStorepd256 => (
                "x86_avx512vl_maskstorepd256",
                &[Type::Ptr, Type::Vector(VectorType::F64x4), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512VlCmpps256 => (
                "x86_avx512vl_cmpps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512VlCmppd256 => (
                "x86_avx512vl_cmppd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512MaskMulld => (
                "x86_avx512_maskmulld",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMinsd => (
                "x86_avx512_maskminsd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMinud => (
                "x86_avx512_maskminud",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMaxsd => (
                "x86_avx512_maskmaxsd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMaxud => (
                "x86_avx512_maskmaxud",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskAndd => (
                "x86_avx512_maskandd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskAndnd => (
                "x86_avx512_maskandnd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskOrd => (
                "x86_avx512_maskord",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskXord => (
                "x86_avx512_maskxord",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMinsq => (
                "x86_avx512_maskminsq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMinuq => (
                "x86_avx512_maskminuq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMaxsq => (
                "x86_avx512_maskmaxsq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMaxuq => (
                "x86_avx512_maskmaxuq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskAndq => (
                "x86_avx512_maskandq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskAndnq => (
                "x86_avx512_maskandnq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskOrq => (
                "x86_avx512_maskorq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskXorq => (
                "x86_avx512_maskxorq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskAddRoundps => (
                "x86_avx512_maskaddroundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskAddRoundpd => (
                "x86_avx512_maskaddroundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskSubRoundps => (
                "x86_avx512_masksubroundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskSubRoundpd => (
                "x86_avx512_masksubroundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMulRoundps => (
                "x86_avx512_maskmulroundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskMulRoundpd => (
                "x86_avx512_maskmulroundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskDivRoundps => (
                "x86_avx512_maskdivroundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskDivRoundpd => (
                "x86_avx512_maskdivroundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskSqrtRoundps => (
                "x86_avx512_masksqrtroundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskSqrtRoundpd => (
                "x86_avx512_masksqrtroundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMinps => (
                "x86_avx512_maskminps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskMinpd => (
                "x86_avx512_maskminpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMaxps => (
                "x86_avx512_maskmaxps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskMaxpd => (
                "x86_avx512_maskmaxpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMoved => (
                "x86_avx512_maskmoved",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Blendd => (
                "x86_avx512_blendd",
                &[
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMoveq => (
                "x86_avx512_maskmoveq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Blendq => (
                "x86_avx512_blendq",
                &[
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskMoveps => (
                "x86_avx512_maskmoveps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Blendps => (
                "x86_avx512_blendps",
                &[
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskMovepd => (
                "x86_avx512_maskmovepd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Blendpd => (
                "x86_avx512_blendpd",
                &[
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskAlignedLoadd => (
                "x86_avx512_maskalignedloadd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedStored => (
                "x86_avx512_maskalignedstored",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedLoadq => (
                "x86_avx512_maskalignedloadq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedStoreq => (
                "x86_avx512_maskalignedstoreq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedLoadps => (
                "x86_avx512_maskalignedloadps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedStoreps => (
                "x86_avx512_maskalignedstoreps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedLoadpd => (
                "x86_avx512_maskalignedloadpd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskAlignedStorepd => (
                "x86_avx512_maskalignedstorepd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512StreamLoaddq => (
                "x86_avx512_streamloaddq",
                &[Type::Ptr],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512StreamStoredq => (
                "x86_avx512_streamstoredq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8)],
                &[],
                false,
            ),
            Intrinsic::X86Avx512StreamStoreps => (
                "x86_avx512_streamstoreps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16)],
                &[],
                false,
            ),
            Intrinsic::X86Avx512StreamStorepd => (
                "x86_avx512_streamstorepd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8)],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsi2ss32 => (
                "x86_avx512_vcvtsi2ss32",
                &[Type::Vector(VectorType::F32x4), Type::I32, Type::I32],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsi2sd32 => (
                "x86_avx512_vcvtsi2sd32",
                &[Type::Vector(VectorType::F64x2), Type::I32, Type::I32],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsi2ss64 => (
                "x86_avx512_vcvtsi2ss64",
                &[Type::Vector(VectorType::F32x4), Type::I64, Type::I32],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsi2sd64 => (
                "x86_avx512_vcvtsi2sd64",
                &[Type::Vector(VectorType::F64x2), Type::I64, Type::I32],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtusi2ss32 => (
                "x86_avx512_vcvtusi2ss32",
                &[Type::Vector(VectorType::F32x4), Type::I32, Type::I32],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtusi2sd32 => (
                "x86_avx512_vcvtusi2sd32",
                &[Type::Vector(VectorType::F64x2), Type::I32, Type::I32],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtusi2ss64 => (
                "x86_avx512_vcvtusi2ss64",
                &[Type::Vector(VectorType::F32x4), Type::I64, Type::I32],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtusi2sd64 => (
                "x86_avx512_vcvtusi2sd64",
                &[Type::Vector(VectorType::F64x2), Type::I64, Type::I32],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtss2si32 => (
                "x86_avx512_vcvtss2si32",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvtss2si64 => (
                "x86_avx512_vcvtss2si64",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvttss2si32 => (
                "x86_avx512_vcvttss2si32",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvttss2si64 => (
                "x86_avx512_vcvttss2si64",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvtss2usi32 => (
                "x86_avx512_vcvtss2usi32",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvtss2usi64 => (
                "x86_avx512_vcvtss2usi64",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvttss2usi32 => (
                "x86_avx512_vcvttss2usi32",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvttss2usi64 => (
                "x86_avx512_vcvttss2usi64",
                &[Type::Vector(VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsd2si32 => (
                "x86_avx512_vcvtsd2si32",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsd2si64 => (
                "x86_avx512_vcvtsd2si64",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvttsd2si32 => (
                "x86_avx512_vcvttsd2si32",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvttsd2si64 => (
                "x86_avx512_vcvttsd2si64",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsd2usi32 => (
                "x86_avx512_vcvtsd2usi32",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsd2usi64 => (
                "x86_avx512_vcvtsd2usi64",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvttsd2usi32 => (
                "x86_avx512_vcvttsd2usi32",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Avx512Vcvttsd2usi64 => (
                "x86_avx512_vcvttsd2usi64",
                &[Type::Vector(VectorType::F64x2), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Avx512Vcvtss2sd => (
                "x86_avx512_vcvtss2sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtsd2ss => (
                "x86_avx512_vcvtsd2ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vcomiss => (
                "x86_avx512_vcomiss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Vcomisd => (
                "x86_avx512_vcomisd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Vucomiss => (
                "x86_avx512_vucomiss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Vucomisd => (
                "x86_avx512_vucomisd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Addss => (
                "x86_avx512_addss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Addsd => (
                "x86_avx512_addsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Subss => (
                "x86_avx512_subss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Subsd => (
                "x86_avx512_subsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Mulss => (
                "x86_avx512_mulss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Mulsd => (
                "x86_avx512_mulsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Divss => (
                "x86_avx512_divss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Divsd => (
                "x86_avx512_divsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Sqrtss => (
                "x86_avx512_sqrtss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Sqrtsd => (
                "x86_avx512_sqrtsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Minss => (
                "x86_avx512_minss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Minsd => (
                "x86_avx512_minsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Maxss => (
                "x86_avx512_maxss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Maxsd => (
                "x86_avx512_maxsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Cmpss => (
                "x86_avx512_cmpss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Cmpsd => (
                "x86_avx512_cmpsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Getexpss => (
                "x86_avx512_getexpss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Getexpsd => (
                "x86_avx512_getexpsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Getmantss => (
                "x86_avx512_getmantss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Getmantsd => (
                "x86_avx512_getmantsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Scalefss => (
                "x86_avx512_scalefss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Scalefsd => (
                "x86_avx512_scalefsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Rcp14ss => (
                "x86_avx512_rcp14ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Rcp14sd => (
                "x86_avx512_rcp14sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Rsqrt14ss => (
                "x86_avx512_rsqrt14ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Rsqrt14sd => (
                "x86_avx512_rsqrt14sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Roundscaless => (
                "x86_avx512_roundscaless",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Roundscalesd => (
                "x86_avx512_roundscalesd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Fixupimmss => (
                "x86_avx512_fixupimmss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::I32x4),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Fixupimmsd => (
                "x86_avx512_fixupimmsd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlVpermi2d128
            | Intrinsic::X86Avx512VlVpermi2q128
            | Intrinsic::X86Avx512VlVpermi2ps128
            | Intrinsic::X86Avx512VlVpermi2pd128
            | Intrinsic::X86Avx512VlVpermt2d128
            | Intrinsic::X86Avx512VlVpermt2q128
            | Intrinsic::X86Avx512VlVpermt2ps128
            | Intrinsic::X86Avx512VlVpermt2pd128
            | Intrinsic::X86Avx512VlVpermilps128
            | Intrinsic::X86Avx512VlVpermilpd128
            | Intrinsic::X86Avx512VlVpermilpsImm128
            | Intrinsic::X86Avx512VlVpermilpdImm128
            | Intrinsic::X86Avx512VlVshufps128
            | Intrinsic::X86Avx512VlVshufpd128
            | Intrinsic::X86Avx512VlVpshufd128
            | Intrinsic::X86Avx512VlValignd128
            | Intrinsic::X86Avx512VlValignq128
            | Intrinsic::X86Avx512VlVpunpckldq128
            | Intrinsic::X86Avx512VlVpunpcklqdq128
            | Intrinsic::X86Avx512VlVunpcklps128
            | Intrinsic::X86Avx512VlVunpcklpd128
            | Intrinsic::X86Avx512VlVpunpckhdq128
            | Intrinsic::X86Avx512VlVpunpckhqdq128
            | Intrinsic::X86Avx512VlVunpckhps128
            | Intrinsic::X86Avx512VlVunpckhpd128
            | Intrinsic::X86Avx512VlVmovshdup128
            | Intrinsic::X86Avx512VlVmovsldup128
            | Intrinsic::X86Avx512VlVmovddup128
            | Intrinsic::X86Avx512VlVpermd256
            | Intrinsic::X86Avx512VlVpermq256
            | Intrinsic::X86Avx512VlVpermps256
            | Intrinsic::X86Avx512VlVpermpd256
            | Intrinsic::X86Avx512VlVpermi2d256
            | Intrinsic::X86Avx512VlVpermi2q256
            | Intrinsic::X86Avx512VlVpermi2ps256
            | Intrinsic::X86Avx512VlVpermi2pd256
            | Intrinsic::X86Avx512VlVpermt2d256
            | Intrinsic::X86Avx512VlVpermt2q256
            | Intrinsic::X86Avx512VlVpermt2ps256
            | Intrinsic::X86Avx512VlVpermt2pd256
            | Intrinsic::X86Avx512VlVpermilps256
            | Intrinsic::X86Avx512VlVpermilpd256
            | Intrinsic::X86Avx512VlVpermilpsImm256
            | Intrinsic::X86Avx512VlVpermilpdImm256
            | Intrinsic::X86Avx512VlVpermqImm256
            | Intrinsic::X86Avx512VlVpermpdImm256
            | Intrinsic::X86Avx512VlVshufps256
            | Intrinsic::X86Avx512VlVshufpd256
            | Intrinsic::X86Avx512VlVpshufd256
            | Intrinsic::X86Avx512VlVshufi32x4256
            | Intrinsic::X86Avx512VlVshufi64x2256
            | Intrinsic::X86Avx512VlVshuff32x4256
            | Intrinsic::X86Avx512VlVshuff64x2256
            | Intrinsic::X86Avx512VlValignd256
            | Intrinsic::X86Avx512VlValignq256
            | Intrinsic::X86Avx512VlVpunpckldq256
            | Intrinsic::X86Avx512VlVpunpcklqdq256
            | Intrinsic::X86Avx512VlVunpcklps256
            | Intrinsic::X86Avx512VlVunpcklpd256
            | Intrinsic::X86Avx512VlVpunpckhdq256
            | Intrinsic::X86Avx512VlVpunpckhqdq256
            | Intrinsic::X86Avx512VlVunpckhps256
            | Intrinsic::X86Avx512VlVunpckhpd256
            | Intrinsic::X86Avx512VlVmovshdup256
            | Intrinsic::X86Avx512VlVmovsldup256
            | Intrinsic::X86Avx512VlVmovddup256 => {
                let descriptor = self
                    .x86_avx512_vl_permute()
                    .expect("VL permutation descriptor");
                (descriptor.name, descriptor.params, descriptor.returns, true)
            }
            Intrinsic::X86Avx512Vpermd => (
                "x86_avx512_vpermd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermq => (
                "x86_avx512_vpermq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermps => (
                "x86_avx512_vpermps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermpd => (
                "x86_avx512_vpermpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermi2d => (
                "x86_avx512_vpermi2d",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermi2q => (
                "x86_avx512_vpermi2q",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermi2ps => (
                "x86_avx512_vpermi2ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermi2pd => (
                "x86_avx512_vpermi2pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermt2d => (
                "x86_avx512_vpermt2d",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermt2q => (
                "x86_avx512_vpermt2q",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermt2ps => (
                "x86_avx512_vpermt2ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermt2pd => (
                "x86_avx512_vpermt2pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpermilps => (
                "x86_avx512_vpermilps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpermilpd => (
                "x86_avx512_vpermilpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512VpermilpsImm => (
                "x86_avx512_vpermilpsimm",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512VpermilpdImm => (
                "x86_avx512_vpermilpdimm",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512VpermqImm => (
                "x86_avx512_vpermqimm",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512VpermpdImm => (
                "x86_avx512_vpermpdimm",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vshufps => (
                "x86_avx512_vshufps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vshufpd => (
                "x86_avx512_vshufpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpshufd => (
                "x86_avx512_vpshufd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vshufi32x4 => (
                "x86_avx512_vshufi32x4",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vshufi64x2 => (
                "x86_avx512_vshufi64x2",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vshuff32x4 => (
                "x86_avx512_vshuff32x4",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vshuff64x2 => (
                "x86_avx512_vshuff64x2",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Valignd => (
                "x86_avx512_valignd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Valignq => (
                "x86_avx512_valignq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpunpckldq => (
                "x86_avx512_vpunpckldq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpunpcklqdq => (
                "x86_avx512_vpunpcklqdq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vunpcklps => (
                "x86_avx512_vunpcklps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vunpcklpd => (
                "x86_avx512_vunpcklpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vpunpckhdq => (
                "x86_avx512_vpunpckhdq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vpunpckhqdq => (
                "x86_avx512_vpunpckhqdq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vunpckhps => (
                "x86_avx512_vunpckhps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vunpckhpd => (
                "x86_avx512_vunpckhpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vmovshdup => (
                "x86_avx512_vmovshdup",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vmovsldup => (
                "x86_avx512_vmovsldup",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512Vmovddup => (
                "x86_avx512_vmovddup",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512Vfmadd132ss => (
                "x86_avx512_vfmadd132ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd132sd => (
                "x86_avx512_vfmadd132sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd213ss => (
                "x86_avx512_vfmadd213ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd213sd => (
                "x86_avx512_vfmadd213sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd231ss => (
                "x86_avx512_vfmadd231ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd231sd => (
                "x86_avx512_vfmadd231sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub132ss => (
                "x86_avx512_vfmsub132ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub132sd => (
                "x86_avx512_vfmsub132sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub213ss => (
                "x86_avx512_vfmsub213ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub213sd => (
                "x86_avx512_vfmsub213sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub231ss => (
                "x86_avx512_vfmsub231ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub231sd => (
                "x86_avx512_vfmsub231sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd132ss => (
                "x86_avx512_vfnmadd132ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd132sd => (
                "x86_avx512_vfnmadd132sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd213ss => (
                "x86_avx512_vfnmadd213ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd213sd => (
                "x86_avx512_vfnmadd213sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd231ss => (
                "x86_avx512_vfnmadd231ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd231sd => (
                "x86_avx512_vfnmadd231sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub132ss => (
                "x86_avx512_vfnmsub132ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub132sd => (
                "x86_avx512_vfnmsub132sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub213ss => (
                "x86_avx512_vfnmsub213ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub213sd => (
                "x86_avx512_vfnmsub213sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub231ss => (
                "x86_avx512_vfnmsub231ss",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub231sd => (
                "x86_avx512_vfnmsub231sd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlVfmadd132ps128
            | Intrinsic::X86Avx512VlVfmadd132pd128
            | Intrinsic::X86Avx512VlVfmadd213ps128
            | Intrinsic::X86Avx512VlVfmadd213pd128
            | Intrinsic::X86Avx512VlVfmadd231ps128
            | Intrinsic::X86Avx512VlVfmadd231pd128
            | Intrinsic::X86Avx512VlVfmsub132ps128
            | Intrinsic::X86Avx512VlVfmsub132pd128
            | Intrinsic::X86Avx512VlVfmsub213ps128
            | Intrinsic::X86Avx512VlVfmsub213pd128
            | Intrinsic::X86Avx512VlVfmsub231ps128
            | Intrinsic::X86Avx512VlVfmsub231pd128
            | Intrinsic::X86Avx512VlVfnmadd132ps128
            | Intrinsic::X86Avx512VlVfnmadd132pd128
            | Intrinsic::X86Avx512VlVfnmadd213ps128
            | Intrinsic::X86Avx512VlVfnmadd213pd128
            | Intrinsic::X86Avx512VlVfnmadd231ps128
            | Intrinsic::X86Avx512VlVfnmadd231pd128
            | Intrinsic::X86Avx512VlVfnmsub132ps128
            | Intrinsic::X86Avx512VlVfnmsub132pd128
            | Intrinsic::X86Avx512VlVfnmsub213ps128
            | Intrinsic::X86Avx512VlVfnmsub213pd128
            | Intrinsic::X86Avx512VlVfnmsub231ps128
            | Intrinsic::X86Avx512VlVfnmsub231pd128
            | Intrinsic::X86Avx512VlVfmaddsub132ps128
            | Intrinsic::X86Avx512VlVfmaddsub132pd128
            | Intrinsic::X86Avx512VlVfmaddsub213ps128
            | Intrinsic::X86Avx512VlVfmaddsub213pd128
            | Intrinsic::X86Avx512VlVfmaddsub231ps128
            | Intrinsic::X86Avx512VlVfmaddsub231pd128
            | Intrinsic::X86Avx512VlVfmsubadd132ps128
            | Intrinsic::X86Avx512VlVfmsubadd132pd128
            | Intrinsic::X86Avx512VlVfmsubadd213ps128
            | Intrinsic::X86Avx512VlVfmsubadd213pd128
            | Intrinsic::X86Avx512VlVfmsubadd231ps128
            | Intrinsic::X86Avx512VlVfmsubadd231pd128
            | Intrinsic::X86Avx512VlVfmadd132ps256
            | Intrinsic::X86Avx512VlVfmadd132pd256
            | Intrinsic::X86Avx512VlVfmadd213ps256
            | Intrinsic::X86Avx512VlVfmadd213pd256
            | Intrinsic::X86Avx512VlVfmadd231ps256
            | Intrinsic::X86Avx512VlVfmadd231pd256
            | Intrinsic::X86Avx512VlVfmsub132ps256
            | Intrinsic::X86Avx512VlVfmsub132pd256
            | Intrinsic::X86Avx512VlVfmsub213ps256
            | Intrinsic::X86Avx512VlVfmsub213pd256
            | Intrinsic::X86Avx512VlVfmsub231ps256
            | Intrinsic::X86Avx512VlVfmsub231pd256
            | Intrinsic::X86Avx512VlVfnmadd132ps256
            | Intrinsic::X86Avx512VlVfnmadd132pd256
            | Intrinsic::X86Avx512VlVfnmadd213ps256
            | Intrinsic::X86Avx512VlVfnmadd213pd256
            | Intrinsic::X86Avx512VlVfnmadd231ps256
            | Intrinsic::X86Avx512VlVfnmadd231pd256
            | Intrinsic::X86Avx512VlVfnmsub132ps256
            | Intrinsic::X86Avx512VlVfnmsub132pd256
            | Intrinsic::X86Avx512VlVfnmsub213ps256
            | Intrinsic::X86Avx512VlVfnmsub213pd256
            | Intrinsic::X86Avx512VlVfnmsub231ps256
            | Intrinsic::X86Avx512VlVfnmsub231pd256
            | Intrinsic::X86Avx512VlVfmaddsub132ps256
            | Intrinsic::X86Avx512VlVfmaddsub132pd256
            | Intrinsic::X86Avx512VlVfmaddsub213ps256
            | Intrinsic::X86Avx512VlVfmaddsub213pd256
            | Intrinsic::X86Avx512VlVfmaddsub231ps256
            | Intrinsic::X86Avx512VlVfmaddsub231pd256
            | Intrinsic::X86Avx512VlVfmsubadd132ps256
            | Intrinsic::X86Avx512VlVfmsubadd132pd256
            | Intrinsic::X86Avx512VlVfmsubadd213ps256
            | Intrinsic::X86Avx512VlVfmsubadd213pd256
            | Intrinsic::X86Avx512VlVfmsubadd231ps256
            | Intrinsic::X86Avx512VlVfmsubadd231pd256 => {
                let info = avx512_vl_fma::info(self);
                (info.name, info.params, info.returns, info.pure)
            }
            Intrinsic::X86Avx512Vfmadd132ps => (
                "x86_avx512_vfmadd132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd132pd => (
                "x86_avx512_vfmadd132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd213ps => (
                "x86_avx512_vfmadd213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd213pd => (
                "x86_avx512_vfmadd213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd231ps => (
                "x86_avx512_vfmadd231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmadd231pd => (
                "x86_avx512_vfmadd231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub132ps => (
                "x86_avx512_vfmsub132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub132pd => (
                "x86_avx512_vfmsub132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub213ps => (
                "x86_avx512_vfmsub213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub213pd => (
                "x86_avx512_vfmsub213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub231ps => (
                "x86_avx512_vfmsub231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsub231pd => (
                "x86_avx512_vfmsub231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd132ps => (
                "x86_avx512_vfnmadd132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd132pd => (
                "x86_avx512_vfnmadd132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd213ps => (
                "x86_avx512_vfnmadd213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd213pd => (
                "x86_avx512_vfnmadd213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd231ps => (
                "x86_avx512_vfnmadd231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmadd231pd => (
                "x86_avx512_vfnmadd231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub132ps => (
                "x86_avx512_vfnmsub132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub132pd => (
                "x86_avx512_vfnmsub132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub213ps => (
                "x86_avx512_vfnmsub213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub213pd => (
                "x86_avx512_vfnmsub213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub231ps => (
                "x86_avx512_vfnmsub231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfnmsub231pd => (
                "x86_avx512_vfnmsub231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub132ps => (
                "x86_avx512_vfmaddsub132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub132pd => (
                "x86_avx512_vfmaddsub132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub213ps => (
                "x86_avx512_vfmaddsub213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub213pd => (
                "x86_avx512_vfmaddsub213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub231ps => (
                "x86_avx512_vfmaddsub231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmaddsub231pd => (
                "x86_avx512_vfmaddsub231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd132ps => (
                "x86_avx512_vfmsubadd132ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd132pd => (
                "x86_avx512_vfmsubadd132pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd213ps => (
                "x86_avx512_vfmsubadd213ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd213pd => (
                "x86_avx512_vfmsubadd213pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd231ps => (
                "x86_avx512_vfmsubadd231ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vfmsubadd231pd => (
                "x86_avx512_vfmsubadd231pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtdq2ps128 => (
                "x86_avx512vl_vcvtdq2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtdq2ps256 => (
                "x86_avx512vl_vcvtdq2ps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtudq2ps128 => (
                "x86_avx512vl_vcvtudq2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtudq2ps256 => (
                "x86_avx512vl_vcvtudq2ps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtdq2pd128 => (
                "x86_avx512vl_vcvtdq2pd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtdq2pd256 => (
                "x86_avx512vl_vcvtdq2pd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtudq2pd128 => (
                "x86_avx512vl_vcvtudq2pd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtudq2pd256 => (
                "x86_avx512vl_vcvtudq2pd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2dq128 => (
                "x86_avx512vl_vcvtps2dq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2dq256 => (
                "x86_avx512vl_vcvtps2dq256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2udq128 => (
                "x86_avx512vl_vcvtps2udq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2udq256 => (
                "x86_avx512vl_vcvtps2udq256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttps2dq128 => (
                "x86_avx512vl_vcvttps2dq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttps2dq256 => (
                "x86_avx512vl_vcvttps2dq256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttps2udq128 => (
                "x86_avx512vl_vcvttps2udq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttps2udq256 => (
                "x86_avx512vl_vcvttps2udq256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2dq128 => (
                "x86_avx512vl_vcvtpd2dq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2dq256 => (
                "x86_avx512vl_vcvtpd2dq256",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2udq128 => (
                "x86_avx512vl_vcvtpd2udq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2udq256 => (
                "x86_avx512vl_vcvtpd2udq256",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttpd2dq128 => (
                "x86_avx512vl_vcvttpd2dq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttpd2dq256 => (
                "x86_avx512vl_vcvttpd2dq256",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttpd2udq128 => (
                "x86_avx512vl_vcvttpd2udq128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvttpd2udq256 => (
                "x86_avx512vl_vcvttpd2udq256",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2pd128 => (
                "x86_avx512vl_vcvtps2pd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2pd256 => (
                "x86_avx512vl_vcvtps2pd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2ps128 => (
                "x86_avx512vl_vcvtpd2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtpd2ps256 => (
                "x86_avx512vl_vcvtpd2ps256",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtph2ps128 => (
                "x86_avx512vl_vcvtph2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I16x8),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtph2ps256 => (
                "x86_avx512vl_vcvtph2ps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::I16x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2ph128 => (
                "x86_avx512vl_vcvtps2ph128",
                &[
                    Type::Vector(VectorType::I16x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I16x8)],
                false,
            ),
            Intrinsic::X86Avx512VlVcvtps2ph256 => (
                "x86_avx512vl_vcvtps2ph256",
                &[
                    Type::Vector(VectorType::I16x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I16x8)],
                false,
            ),
            Intrinsic::X86Avx512CdBroadcastmb128 => (
                "x86_avx512cd_broadcastmb128",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512CdBroadcastmw128 => (
                "x86_avx512cd_broadcastmw128",
                &[Type::I16],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512CdBroadcastmb256 => (
                "x86_avx512cd_broadcastmb256",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512CdBroadcastmw256 => (
                "x86_avx512cd_broadcastmw256",
                &[Type::I16],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512CdBroadcastmb512 => (
                "x86_avx512cd_broadcastmb512",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512CdBroadcastmw512 => (
                "x86_avx512cd_broadcastmw512",
                &[Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictd128 => (
                "x86_avx512cd_maskconflictd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictq128 => (
                "x86_avx512cd_maskconflictq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntd128 => (
                "x86_avx512cd_masklzcntd128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntq128 => (
                "x86_avx512cd_masklzcntq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictd256 => (
                "x86_avx512cd_maskconflictd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictq256 => (
                "x86_avx512cd_maskconflictq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntd256 => (
                "x86_avx512cd_masklzcntd256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntq256 => (
                "x86_avx512cd_masklzcntq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictd512 => (
                "x86_avx512cd_maskconflictd512",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskConflictq512 => (
                "x86_avx512cd_maskconflictq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntd512 => (
                "x86_avx512cd_masklzcntd512",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512CdMaskLzcntq512 => (
                "x86_avx512cd_masklzcntq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqMulloq128 => (
                "x86_avx512dq_mulloq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqAndps128 => (
                "x86_avx512dq_andps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqAndpd128 => (
                "x86_avx512dq_andpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnps128 => (
                "x86_avx512dq_andnps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnpd128 => (
                "x86_avx512dq_andnpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqOrps128 => (
                "x86_avx512dq_orps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqOrpd128 => (
                "x86_avx512dq_orpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqXorps128 => (
                "x86_avx512dq_xorps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqXorpd128 => (
                "x86_avx512dq_xorpd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqMulloq256 => (
                "x86_avx512dq_mulloq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqAndps256 => (
                "x86_avx512dq_andps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqAndpd256 => (
                "x86_avx512dq_andpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnps256 => (
                "x86_avx512dq_andnps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnpd256 => (
                "x86_avx512dq_andnpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqOrps256 => (
                "x86_avx512dq_orps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqOrpd256 => (
                "x86_avx512dq_orpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqXorps256 => (
                "x86_avx512dq_xorps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqXorpd256 => (
                "x86_avx512dq_xorpd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqMulloq512 => (
                "x86_avx512dq_mulloq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqAndps512 => (
                "x86_avx512dq_andps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqAndpd512 => (
                "x86_avx512dq_andpd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnps512 => (
                "x86_avx512dq_andnps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqAndnpd512 => (
                "x86_avx512dq_andnpd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqOrps512 => (
                "x86_avx512dq_orps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqOrpd512 => (
                "x86_avx512dq_orpd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqXorps512 => (
                "x86_avx512dq_xorps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqXorpd512 => (
                "x86_avx512dq_xorpd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqVcvtqq2pd128 => (
                "x86_avx512dq_vcvtqq2pd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2pd128 => (
                "x86_avx512dq_vcvtuqq2pd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtqq2ps128 => (
                "x86_avx512dq_vcvtqq2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2ps128 => (
                "x86_avx512dq_vcvtuqq2ps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2qq128 => (
                "x86_avx512dq_vcvtpd2qq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2uqq128 => (
                "x86_avx512dq_vcvtpd2uqq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2qq128 => (
                "x86_avx512dq_vcvtps2qq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2uqq128 => (
                "x86_avx512dq_vcvtps2uqq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2qq128 => (
                "x86_avx512dq_vcvttpd2qq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2uqq128 => (
                "x86_avx512dq_vcvttpd2uqq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2qq128 => (
                "x86_avx512dq_vcvttps2qq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2uqq128 => (
                "x86_avx512dq_vcvttps2uqq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtqq2pd256 => (
                "x86_avx512dq_vcvtqq2pd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2pd256 => (
                "x86_avx512dq_vcvtuqq2pd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtqq2ps256 => (
                "x86_avx512dq_vcvtqq2ps256",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2ps256 => (
                "x86_avx512dq_vcvtuqq2ps256",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2qq256 => (
                "x86_avx512dq_vcvtpd2qq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2uqq256 => (
                "x86_avx512dq_vcvtpd2uqq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2qq256 => (
                "x86_avx512dq_vcvtps2qq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2uqq256 => (
                "x86_avx512dq_vcvtps2uqq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2qq256 => (
                "x86_avx512dq_vcvttpd2qq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2uqq256 => (
                "x86_avx512dq_vcvttpd2uqq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2qq256 => (
                "x86_avx512dq_vcvttps2qq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2uqq256 => (
                "x86_avx512dq_vcvttps2uqq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtqq2pd512 => (
                "x86_avx512dq_vcvtqq2pd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2pd512 => (
                "x86_avx512dq_vcvtuqq2pd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtqq2ps512 => (
                "x86_avx512dq_vcvtqq2ps512",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtuqq2ps512 => (
                "x86_avx512dq_vcvtuqq2ps512",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2qq512 => (
                "x86_avx512dq_vcvtpd2qq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtpd2uqq512 => (
                "x86_avx512dq_vcvtpd2uqq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2qq512 => (
                "x86_avx512dq_vcvtps2qq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvtps2uqq512 => (
                "x86_avx512dq_vcvtps2uqq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2qq512 => (
                "x86_avx512dq_vcvttpd2qq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttpd2uqq512 => (
                "x86_avx512dq_vcvttpd2uqq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2qq512 => (
                "x86_avx512dq_vcvttps2qq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqVcvttps2uqq512 => (
                "x86_avx512dq_vcvttps2uqq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqMovd2m128 => (
                "x86_avx512dq_movd2m128",
                &[Type::Vector(VectorType::I32x4)],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512DqMovq2m128 => (
                "x86_avx512dq_movq2m128",
                &[Type::Vector(VectorType::I64x2)],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2d128 => (
                "x86_avx512dq_movm2d128",
                &[Type::I8],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2q128 => (
                "x86_avx512dq_movm2q128",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqMovd2m256 => (
                "x86_avx512dq_movd2m256",
                &[Type::Vector(VectorType::I32x8)],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512DqMovq2m256 => (
                "x86_avx512dq_movq2m256",
                &[Type::Vector(VectorType::I64x4)],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2d256 => (
                "x86_avx512dq_movm2d256",
                &[Type::I8],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2q256 => (
                "x86_avx512dq_movm2q256",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqMovd2m512 => (
                "x86_avx512dq_movd2m512",
                &[Type::Vector(VectorType::I32x16)],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512DqMovq2m512 => (
                "x86_avx512dq_movq2m512",
                &[Type::Vector(VectorType::I64x8)],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2d512 => (
                "x86_avx512dq_movm2d512",
                &[Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqMovm2q512 => (
                "x86_avx512dq_movm2q512",
                &[Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti32x2128 => (
                "x86_avx512dq_broadcasti32x2_128",
                &[
                    Type::Vector(VectorType::I32x4),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcastf32x2256 => (
                "x86_avx512dq_broadcastf32x2_256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcastf64x2256 => (
                "x86_avx512dq_broadcastf64x2_256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti32x2256 => (
                "x86_avx512dq_broadcasti32x2_256",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti64x2256 => (
                "x86_avx512dq_broadcasti64x2_256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqExtractf64x2256 => (
                "x86_avx512dq_extractf64x2_256",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqExtracti64x2256 => (
                "x86_avx512dq_extracti64x2_256",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqInsertf64x2256 => (
                "x86_avx512dq_insertf64x2_256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqInserti64x2256 => (
                "x86_avx512dq_inserti64x2_256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::I8,
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcastf32x2512 => (
                "x86_avx512dq_broadcastf32x2_512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x4),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcastf32x8512 => (
                "x86_avx512dq_broadcastf32x8_512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x8),
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcastf64x2512 => (
                "x86_avx512dq_broadcastf64x2_512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti32x2512 => (
                "x86_avx512dq_broadcasti32x2_512",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x4),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti32x8512 => (
                "x86_avx512dq_broadcasti32x8_512",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x8),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqBroadcasti64x2512 => (
                "x86_avx512dq_broadcasti64x2_512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x2),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqExtractf32x8512 => (
                "x86_avx512dq_extractf32x8_512",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqExtractf64x2512 => (
                "x86_avx512dq_extractf64x2_512",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqExtracti32x8512 => (
                "x86_avx512dq_extracti32x8_512",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx512DqExtracti64x2512 => (
                "x86_avx512dq_extracti64x2_512",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx512DqInsertf32x8512 => (
                "x86_avx512dq_insertf32x8_512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqInsertf64x2512 => (
                "x86_avx512dq_insertf64x2_512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqInserti32x8512 => (
                "x86_avx512dq_inserti32x8_512",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512DqInserti64x2512 => (
                "x86_avx512dq_inserti64x2_512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512DqRangeps128 => (
                "x86_avx512dq_rangeps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqRangepd128 => (
                "x86_avx512dq_rangepd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqReduceps128 => (
                "x86_avx512dq_reduceps128",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqReducepd128 => (
                "x86_avx512dq_reducepd128",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqFpclassps128 => (
                "x86_avx512dq_fpclassps128",
                &[Type::Vector(VectorType::F32x4), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqFpclasspd128 => (
                "x86_avx512dq_fpclasspd128",
                &[Type::Vector(VectorType::F64x2), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqRangeps256 => (
                "x86_avx512dq_rangeps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512DqRangepd256 => (
                "x86_avx512dq_rangepd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqReduceps256 => (
                "x86_avx512dq_reduceps256",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512DqReducepd256 => (
                "x86_avx512dq_reducepd256",
                &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx512DqFpclassps256 => (
                "x86_avx512dq_fpclassps256",
                &[Type::Vector(VectorType::F32x8), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqFpclasspd256 => (
                "x86_avx512dq_fpclasspd256",
                &[Type::Vector(VectorType::F64x4), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqRangeps512 => (
                "x86_avx512dq_rangeps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512DqRangepd512 => (
                "x86_avx512dq_rangepd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqReduceps512 => (
                "x86_avx512dq_reduceps512",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512DqReducepd512 => (
                "x86_avx512dq_reducepd512",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512DqFpclassps512 => (
                "x86_avx512dq_fpclassps512",
                &[Type::Vector(VectorType::F32x16), Type::I16, Type::I32],
                &[Type::I16],
                false,
            ),
            Intrinsic::X86Avx512DqFpclasspd512 => (
                "x86_avx512dq_fpclasspd512",
                &[Type::Vector(VectorType::F64x8), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqRangess => (
                "x86_avx512dq_rangess",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqRangesd => (
                "x86_avx512dq_rangesd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqReducess => (
                "x86_avx512dq_reducess",
                &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx512DqReducesd => (
                "x86_avx512dq_reducesd",
                &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx512DqFpclassss => (
                "x86_avx512dq_fpclassss",
                &[Type::Vector(VectorType::F32x4), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512DqFpclasssd => (
                "x86_avx512dq_fpclasssd",
                &[Type::Vector(VectorType::F64x2), Type::I8, Type::I32],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512Vcvtdq2ps => (
                "x86_avx512_vcvtdq2ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtudq2ps => (
                "x86_avx512_vcvtudq2ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtdq2pd => (
                "x86_avx512_vcvtdq2pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtudq2pd => (
                "x86_avx512_vcvtudq2pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtps2dq => (
                "x86_avx512_vcvtps2dq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtps2udq => (
                "x86_avx512_vcvtps2udq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvttps2dq => (
                "x86_avx512_vcvttps2dq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvttps2udq => (
                "x86_avx512_vcvttps2udq",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtpd2dq => (
                "x86_avx512_vcvtpd2dq",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtpd2udq => (
                "x86_avx512_vcvtpd2udq",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvttpd2dq => (
                "x86_avx512_vcvttpd2dq",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvttpd2udq => (
                "x86_avx512_vcvttpd2udq",
                &[
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtps2pd => (
                "x86_avx512_vcvtps2pd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtpd2ps => (
                "x86_avx512_vcvtpd2ps",
                &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtph2ps => (
                "x86_avx512_vcvtph2ps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::I16x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vcvtps2ph => (
                "x86_avx512_vcvtps2ph",
                &[
                    Type::Vector(VectorType::I16x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I16x16)],
                false,
            ),
            Intrinsic::X86Avx512Vpgatherdd => (
                "x86_avx512_vpgatherdd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vpgatherdq => (
                "x86_avx512_vpgatherdq",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vpgatherqd => (
                "x86_avx512_vpgatherqd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vpgatherqq => (
                "x86_avx512_vpgatherqq",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vgatherdps => (
                "x86_avx512_vgatherdps",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512Vgatherdpd => (
                "x86_avx512_vgatherdpd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vgatherqps => (
                "x86_avx512_vgatherqps",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx512Vgatherqpd => (
                "x86_avx512_vgatherqpd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512Vpscatterdd => (
                "x86_avx512_vpscatterdd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vpscatterdq => (
                "x86_avx512_vpscatterdq",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vpscatterqd => (
                "x86_avx512_vpscatterqd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vpscatterqq => (
                "x86_avx512_vpscatterqq",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vscatterdps => (
                "x86_avx512_vscatterdps",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vscatterdpd => (
                "x86_avx512_vscatterdpd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I32x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vscatterqps => (
                "x86_avx512_vscatterqps",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Vscatterqpd => (
                "x86_avx512_vscatterqpd",
                &[
                    Type::Ptr,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Cmpps => (
                "x86_avx512_cmpps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::I32,
                ],
                &[Type::I16],
                false,
            ),
            Intrinsic::X86Avx512Cmppd => (
                "x86_avx512_cmppd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512CmpRoundps => (
                "x86_avx512_cmproundps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I16],
                false,
            ),
            Intrinsic::X86Avx512CmpRoundpd => (
                "x86_avx512_cmproundpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I8],
                false,
            ),
            Intrinsic::X86Avx512MaskAddps => (
                "x86_avx512_maskaddps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskAddpd => (
                "x86_avx512_maskaddpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskSubps => (
                "x86_avx512_masksubps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskSubpd => (
                "x86_avx512_masksubpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskMulps => (
                "x86_avx512_maskmulps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskMulpd => (
                "x86_avx512_maskmulpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskDivps => (
                "x86_avx512_maskdivps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskDivpd => (
                "x86_avx512_maskdivpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskSqrtps => (
                "x86_avx512_masksqrtps",
                &[
                    Type::Vector(VectorType::F32x16),
                    Type::I16,
                    Type::Vector(VectorType::F32x16),
                ],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskSqrtpd => (
                "x86_avx512_masksqrtpd",
                &[
                    Type::Vector(VectorType::F64x8),
                    Type::I8,
                    Type::Vector(VectorType::F64x8),
                ],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadps => (
                "x86_avx512_maskloadps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[Type::Vector(VectorType::F32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadpd => (
                "x86_avx512_maskloadpd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[Type::Vector(VectorType::F64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskStoreps => (
                "x86_avx512_maskstoreps",
                &[Type::Ptr, Type::Vector(VectorType::F32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStorepd => (
                "x86_avx512_maskstorepd",
                &[Type::Ptr, Type::Vector(VectorType::F64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Avx512Cmpd => (
                "x86_avx512_cmpd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Cmpq => (
                "x86_avx512_cmpq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512Cmpud => (
                "x86_avx512_cmpud",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                    Type::I32,
                ],
                &[Type::I16],
                true,
            ),
            Intrinsic::X86Avx512Cmpuq => (
                "x86_avx512_cmpuq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::I8],
                true,
            ),
            Intrinsic::X86Avx512MaskAddd => (
                "x86_avx512_maskaddd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskAddq => (
                "x86_avx512_maskaddq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskSubd => (
                "x86_avx512_masksubd",
                &[
                    Type::Vector(VectorType::I32x16),
                    Type::I16,
                    Type::Vector(VectorType::I32x16),
                    Type::Vector(VectorType::I32x16),
                ],
                &[Type::Vector(VectorType::I32x16)],
                true,
            ),
            Intrinsic::X86Avx512MaskSubq => (
                "x86_avx512_masksubq",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::I8,
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Avx512MaskLoadd => (
                "x86_avx512_maskloadd",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[Type::Vector(VectorType::I32x16)],
                false,
            ),
            Intrinsic::X86Avx512MaskLoadq => (
                "x86_avx512_maskloadq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[Type::Vector(VectorType::I64x8)],
                false,
            ),
            Intrinsic::X86Avx512MaskStored => (
                "x86_avx512_maskstored",
                &[Type::Ptr, Type::Vector(VectorType::I32x16), Type::I16],
                &[],
                false,
            ),
            Intrinsic::X86Avx512MaskStoreq => (
                "x86_avx512_maskstoreq",
                &[Type::Ptr, Type::Vector(VectorType::I64x8), Type::I8],
                &[],
                false,
            ),
            Intrinsic::X86Sha256msg1 => (
                "x86_sha256msg1",
                &[Type::Vector(VectorType::I32x4); 2],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sha256msg2 => (
                "x86_sha256msg2",
                &[Type::Vector(VectorType::I32x4); 2],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sha256rnds2 => (
                "x86_sha256rnds2",
                &[Type::Vector(VectorType::I32x4); 3],
                &[Type::Vector(VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Vpclmulqdq256 => (
                "x86_vpclmulqdq256",
                &[
                    Type::Vector(VectorType::I64x4),
                    Type::Vector(VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Vpclmulqdq512 => (
                "x86_vpclmulqdq512",
                &[
                    Type::Vector(VectorType::I64x8),
                    Type::Vector(VectorType::I64x8),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x8)],
                true,
            ),
            Intrinsic::X86Pclmulqdq128 => (
                "x86_pclmulqdq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesenc128 => (
                "x86_aesenc128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesenclast128 => (
                "x86_aesenclast128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesdec128 => (
                "x86_aesdec128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesdeclast128 => (
                "x86_aesdeclast128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesimc128 => (
                "x86_aesimc128",
                &[Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aeskeygenassist128 => (
                "x86_aeskeygenassist128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I32],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Ins8 => ("x86_ins8", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Ins16 => ("x86_ins16", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Ins32 => ("x86_ins32", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs8 => ("x86_outs8", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs16 => ("x86_outs16", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs32 => ("x86_outs32", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86RepIns8 => (
                "x86_rep_ins8",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86RepIns16 => (
                "x86_rep_ins16",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86RepIns32 => (
                "x86_rep_ins32",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts8 => (
                "x86_rep_outs8",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts16 => (
                "x86_rep_outs16",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts32 => (
                "x86_rep_outs32",
                &[Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64],
                false,
            ),
            Intrinsic::X86Ins8OwnedTerminal => (
                "x86_ins8_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86Ins16OwnedTerminal => (
                "x86_ins16_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86Ins32OwnedTerminal => (
                "x86_ins32_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86Outs8OwnedTerminal => (
                "x86_outs8_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86Outs16OwnedTerminal => (
                "x86_outs16_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86Outs32OwnedTerminal => (
                "x86_outs32_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr],
                &[Type::Ptr, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepIns8OwnedTerminal => (
                "x86_rep_ins8_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepIns16OwnedTerminal => (
                "x86_rep_ins16_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepIns32OwnedTerminal => (
                "x86_rep_ins32_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts8OwnedTerminal => (
                "x86_rep_outs8_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts16OwnedTerminal => (
                "x86_rep_outs16_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),
            Intrinsic::X86RepOuts32OwnedTerminal => (
                "x86_rep_outs32_owned_terminal",
                &[Type::I64, Type::I16, Type::Ptr, Type::I64],
                &[Type::Ptr, Type::I64, Type::I64, Type::I64],
                false,
            ),

            Intrinsic::X86Rdpmc => ("x86_rdpmc", &[Type::I32], &[Type::I64], false),
            Intrinsic::X86X87Ffree0 => ("x86_x87_ffree0", &[], &[], false),
            Intrinsic::X86X87Ffree1 => ("x86_x87_ffree1", &[], &[], false),
            Intrinsic::X86X87Ffree2 => ("x86_x87_ffree2", &[], &[], false),
            Intrinsic::X86X87Ffree3 => ("x86_x87_ffree3", &[], &[], false),
            Intrinsic::X86X87Ffree4 => ("x86_x87_ffree4", &[], &[], false),
            Intrinsic::X86X87Ffree5 => ("x86_x87_ffree5", &[], &[], false),
            Intrinsic::X86X87Ffree6 => ("x86_x87_ffree6", &[], &[], false),
            Intrinsic::X86X87Ffree7 => ("x86_x87_ffree7", &[], &[], false),
            Intrinsic::X86X87Fdecstp => ("x86_x87_fdecstp", &[], &[], false),
            Intrinsic::X86X87Fincstp => ("x86_x87_fincstp", &[], &[], false),
            Intrinsic::X86X87Fnop => ("x86_x87_fnop", &[], &[], false),

            Intrinsic::X86RflagsRead => ("x86_rflags_read", &[], &[Type::I64], false),
            Intrinsic::X86RflagsSwap => ("x86_rflags_swap", &[Type::I64], &[Type::I64], false),
            Intrinsic::X86DivU8 => ("x86_div_u8", &[Type::I8; 3], &[Type::I8; 2], false),
            Intrinsic::X86DivU16 => ("x86_div_u16", &[Type::I16; 3], &[Type::I16; 2], false),
            Intrinsic::X86DivU32 => ("x86_div_u32", &[Type::I32; 3], &[Type::I32; 2], false),
            Intrinsic::X86DivU64 => ("x86_div_u64", &[Type::I64; 3], &[Type::I64; 2], false),
            Intrinsic::X86DivS8 => ("x86_div_s8", &[Type::I8; 3], &[Type::I8; 2], false),
            Intrinsic::X86DivS16 => ("x86_div_s16", &[Type::I16; 3], &[Type::I16; 2], false),
            Intrinsic::X86DivS32 => ("x86_div_s32", &[Type::I32; 3], &[Type::I32; 2], false),
            Intrinsic::X86DivS64 => ("x86_div_s64", &[Type::I64; 3], &[Type::I64; 2], false),
            Intrinsic::X86X87Region => (
                "x86_x87_region",
                &[Type::Ptr, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Sse2Packsswb128 => (
                "x86_sse2_packsswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Packssdw128 => (
                "x86_sse2_packssdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Packuswb128 => (
                "x86_sse2_packuswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse41Packusdw128 => (
                "x86_sse41_packusdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpacksswb128 => (
                "x86_avx_vpacksswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpackssdw128 => (
                "x86_avx_vpackssdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpackuswb128 => (
                "x86_avx_vpackuswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpackusdw128 => (
                "x86_avx_vpackusdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpacksswb256 => (
                "x86_avx2_vpacksswb256",
                &[Type::Vector(crate::VectorType::I16x16); 2],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpackssdw256 => (
                "x86_avx2_vpackssdw256",
                &[Type::Vector(crate::VectorType::I32x8); 2],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpackuswb256 => (
                "x86_avx2_vpackuswb256",
                &[Type::Vector(crate::VectorType::I16x16); 2],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpackusdw256 => (
                "x86_avx2_vpackusdw256",
                &[Type::Vector(crate::VectorType::I32x8); 2],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddw128 => (
                "x86_ssse3_phaddw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddd128 => (
                "x86_ssse3_phaddd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddsw128 => (
                "x86_ssse3_phaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubw128 => (
                "x86_ssse3_phsubw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubd128 => (
                "x86_ssse3_phsubd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubsw128 => (
                "x86_ssse3_phsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Paddsb128 => (
                "x86_sse2_paddsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Paddsw128 => (
                "x86_sse2_paddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Paddusb128 => (
                "x86_sse2_paddusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Paddusw128 => (
                "x86_sse2_paddusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psubsb128 => (
                "x86_sse2_psubsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psubsw128 => (
                "x86_sse2_psubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psubusb128 => (
                "x86_sse2_psubusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psubusw128 => (
                "x86_sse2_psubusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pavgb128 => (
                "x86_sse2_pavgb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Pavgw128 => (
                "x86_sse2_pavgw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Psignb128 => (
                "x86_ssse3_psignb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Ssse3Psignw128 => (
                "x86_ssse3_psignw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Psignd128 => (
                "x86_ssse3_psignd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsb128 => (
                "x86_ssse3_pabsb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsw128 => (
                "x86_ssse3_pabsw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsd128 => (
                "x86_ssse3_pabsd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphaddw128 => (
                "x86_avx_vphaddw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphaddd128 => (
                "x86_avx_vphaddd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphaddsw128 => (
                "x86_avx_vphaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphsubw128 => (
                "x86_avx_vphsubw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphsubd128 => (
                "x86_avx_vphsubd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphsubsw128 => (
                "x86_avx_vphsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpaddsb128 => (
                "x86_avx_vpaddsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpaddsw128 => (
                "x86_avx_vpaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpaddusb128 => (
                "x86_avx_vpaddusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpaddusw128 => (
                "x86_avx_vpaddusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsubsb128 => (
                "x86_avx_vpsubsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsubsw128 => (
                "x86_avx_vpsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsubusb128 => (
                "x86_avx_vpsubusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsubusw128 => (
                "x86_avx_vpsubusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpavgb128 => (
                "x86_avx_vpavgb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpavgw128 => (
                "x86_avx_vpavgw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsignb128 => (
                "x86_avx_vpsignb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsignw128 => (
                "x86_avx_vpsignw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsignd128 => (
                "x86_avx_vpsignd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpabsb128 => (
                "x86_avx_vpabsb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpabsw128 => (
                "x86_avx_vpabsw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpabsd128 => (
                "x86_avx_vpabsd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddw256 => (
                "x86_avx2_vphaddw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddd256 => (
                "x86_avx2_vphaddd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddsw256 => (
                "x86_avx2_vphaddsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubw256 => (
                "x86_avx2_vphsubw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubd256 => (
                "x86_avx2_vphsubd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubsw256 => (
                "x86_avx2_vphsubsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddsb256 => (
                "x86_avx2_vpaddsb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddsw256 => (
                "x86_avx2_vpaddsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddusb256 => (
                "x86_avx2_vpaddusb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddusw256 => (
                "x86_avx2_vpaddusw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubsb256 => (
                "x86_avx2_vpsubsb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubsw256 => (
                "x86_avx2_vpsubsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubusb256 => (
                "x86_avx2_vpsubusb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubusw256 => (
                "x86_avx2_vpsubusw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpavgb256 => (
                "x86_avx2_vpavgb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpavgw256 => (
                "x86_avx2_vpavgw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignb256 => (
                "x86_avx2_vpsignb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignw256 => (
                "x86_avx2_vpsignw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignd256 => (
                "x86_avx2_vpsignd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsb256 => (
                "x86_avx2_vpabsb256",
                &[Type::Vector(crate::VectorType::I8x32)],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsw256 => (
                "x86_avx2_vpabsw256",
                &[Type::Vector(crate::VectorType::I16x16)],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsd256 => (
                "x86_avx2_vpabsd256",
                &[Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Sse41Blendvps128 => (
                "x86_sse41_blendvps128",
                &[Type::Vector(crate::VectorType::F32x4); 3],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Sse41Blendvpd128 => (
                "x86_sse41_blendvpd128",
                &[Type::Vector(crate::VectorType::F64x2); 3],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Sse41Pblendvb128 => (
                "x86_sse41_pblendvb128",
                &[Type::Vector(crate::VectorType::I8x16); 3],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVblendvps128 => (
                "x86_avx_vblendvps128",
                &[Type::Vector(crate::VectorType::F32x4); 3],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86AvxVblendvpd128 => (
                "x86_avx_vblendvpd128",
                &[Type::Vector(crate::VectorType::F64x2); 3],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86AvxVpblendvb128 => (
                "x86_avx_vpblendvb128",
                &[Type::Vector(crate::VectorType::I8x16); 3],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVblendvps256 => (
                "x86_avx_vblendvps256",
                &[Type::Vector(crate::VectorType::F32x8); 3],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVblendvpd256 => (
                "x86_avx_vblendvpd256",
                &[Type::Vector(crate::VectorType::F64x4); 3],
                &[Type::Vector(crate::VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpblendvb256 => (
                "x86_avx2_vpblendvb256",
                &[Type::Vector(crate::VectorType::I8x32); 3],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Sse41Mpsadbw128 => (
                "x86_sse41_mpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVmpsadbw128 => (
                "x86_avx_vmpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx2Vmpsadbw256 => (
                "x86_avx2_vmpsadbw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Sse41Phminposuw128 => (
                "x86_sse41_phminposuw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphminposuw128 => (
                "x86_avx_vphminposuw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmaddwd128 => (
                "x86_sse2_pmaddwd128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Pmaddubsw128 => (
                "x86_ssse3_pmaddubsw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmulhw128 => (
                "x86_sse2_pmulhw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmulhuw128 => (
                "x86_sse2_pmulhuw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pmulhrsw128 => (
                "x86_ssse3_pmulhrsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse41Pmuldq128 => (
                "x86_sse41_pmuldq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Pmuludq128 => (
                "x86_sse2_pmuludq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psadbw128 => (
                "x86_sse2_psadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpmaddwd128 => (
                "x86_avx_vpmaddwd128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpmaddubsw128 => (
                "x86_avx_vpmaddubsw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhw128 => (
                "x86_avx_vpmulhw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhuw128 => (
                "x86_avx_vpmulhuw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhrsw128 => (
                "x86_avx_vpmulhrsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmuldq128 => (
                "x86_avx_vpmuldq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpmuludq128 => (
                "x86_avx_vpmuludq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsadbw128 => (
                "x86_avx_vpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpmaddwd256 => (
                "x86_avx2_vpmaddwd256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpmaddubsw256 => (
                "x86_avx2_vpmaddubsw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhw256 => (
                "x86_avx2_vpmulhw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhuw256 => (
                "x86_avx2_vpmulhuw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhrsw256 => (
                "x86_avx2_vpmulhrsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmuldq256 => (
                "x86_avx2_vpmuldq256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpmuludq256 => (
                "x86_avx2_vpmuludq256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsadbw256 => (
                "x86_avx2_vpsadbw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Sse2Pslldq128 => (
                "x86_sse2_pslldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psrldq128 => (
                "x86_sse2_psrldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpslldq128 => (
                "x86_avx_vpslldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsrldq128 => (
                "x86_avx_vpsrldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpslldq256 => (
                "x86_avx2_vpslldq256",
                &[Type::Vector(crate::VectorType::I8x32), Type::I32],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrldq256 => (
                "x86_avx2_vpsrldq256",
                &[Type::Vector(crate::VectorType::I8x32), Type::I32],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Sse2Psllw128 => (
                "x86_sse2_psllw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pslld128 => (
                "x86_sse2_pslld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sse2Psllq128 => (
                "x86_sse2_psllq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psrlw128 => (
                "x86_sse2_psrlw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psrld128 => (
                "x86_sse2_psrld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sse2Psrlq128 => (
                "x86_sse2_psrlq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psraw128 => (
                "x86_sse2_psraw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psrad128 => (
                "x86_sse2_psrad128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsllw128 => (
                "x86_avx_vpsllw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpslld128 => (
                "x86_avx_vpslld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsllq128 => (
                "x86_avx_vpsllq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsrlw128 => (
                "x86_avx_vpsrlw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsrld128 => (
                "x86_avx_vpsrld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsrlq128 => (
                "x86_avx_vpsrlq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsraw128 => (
                "x86_avx_vpsraw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsrad128 => (
                "x86_avx_vpsrad128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllw256 => (
                "x86_avx2_vpsllw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpslld256 => (
                "x86_avx2_vpslld256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllq256 => (
                "x86_avx2_vpsllq256",
                &[Type::Vector(crate::VectorType::I64x4), Type::I64],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlw256 => (
                "x86_avx2_vpsrlw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrld256 => (
                "x86_avx2_vpsrld256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlq256 => (
                "x86_avx2_vpsrlq256",
                &[Type::Vector(crate::VectorType::I64x4), Type::I64],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsraw256 => (
                "x86_avx2_vpsraw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrad256 => (
                "x86_avx2_vpsrad256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvd128 => (
                "x86_avx2_vpsllvd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvq128 => (
                "x86_avx2_vpsllvq128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvd128 => (
                "x86_avx2_vpsrlvd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvq128 => (
                "x86_avx2_vpsrlvq128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpsravd128 => (
                "x86_avx2_vpsravd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvd256 => (
                "x86_avx2_vpsllvd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvq256 => (
                "x86_avx2_vpsllvq256",
                &[
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvd256 => (
                "x86_avx2_vpsrlvd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvq256 => (
                "x86_avx2_vpsrlvq256",
                &[
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsravd256 => (
                "x86_avx2_vpsravd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pshufb128 => (
                "x86_ssse3_pshufb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpshufb128 => (
                "x86_avx_vpshufb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpshufb256 => (
                "x86_avx2_vpshufb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),

            Intrinsic::X86Xrstor64 => ("x86_xrstor64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86X87ReadCwNowait => ("x86_x87_read_cw_nowait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadSwNowait => ("x86_x87_read_sw_nowait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadCwWait => ("x86_x87_read_cw_wait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadSwWait => ("x86_x87_read_sw_wait", &[], &[Type::I16], false),
            Intrinsic::X86X87ExchangeCw => {
                ("x86_x87_exchange_cw", &[Type::I16], &[Type::I16], false)
            }
            Intrinsic::X86X87ClexNowait => ("x86_x87_clex_nowait", &[], &[], false),
            Intrinsic::X86X87ClexWait => ("x86_x87_clex_wait", &[], &[], false),
            Intrinsic::X86X87InitNowait => ("x86_x87_init_nowait", &[], &[], false),
            Intrinsic::X86X87InitWait => ("x86_x87_init_wait", &[], &[], false),
            Intrinsic::X86X87Fist16 => (
                "x86_x87_fist16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fist32 => (
                "x86_x87_fist32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp16 => (
                "x86_x87_fistp16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp32 => (
                "x86_x87_fistp32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp64 => (
                "x86_x87_fistp64",
                &[Type::F80],
                &[Type::I64, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp16 => (
                "x86_x87_fisttp16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp32 => (
                "x86_x87_fisttp32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp64 => (
                "x86_x87_fisttp64",
                &[Type::F80],
                &[Type::I64, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild16 => (
                "x86_x87_fild16",
                &[Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild32 => (
                "x86_x87_fild32",
                &[Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild64 => (
                "x86_x87_fild64",
                &[Type::I64],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fbld => ("x86_x87_fbld", &[Type::Ptr], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fbstp => (
                "x86_x87_fbstp",
                &[Type::F80, Type::Ptr],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fldpi => ("x86_x87_fldpi", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldl2e => ("x86_x87_fldl2e", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldl2t => ("x86_x87_fldl2t", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldlg2 => ("x86_x87_fldlg2", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldln2 => ("x86_x87_fldln2", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fiadd16 => (
                "x86_x87_fiadd16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fiadd32 => (
                "x86_x87_fiadd32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fimul16 => (
                "x86_x87_fimul16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fimul32 => (
                "x86_x87_fimul32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisub16 => (
                "x86_x87_fisub16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisub32 => (
                "x86_x87_fisub32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisubr16 => (
                "x86_x87_fisubr16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisubr32 => (
                "x86_x87_fisubr32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidiv16 => (
                "x86_x87_fidiv16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidiv32 => (
                "x86_x87_fidiv32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidivr16 => (
                "x86_x87_fidivr16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidivr32 => (
                "x86_x87_fidivr32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcom => ("x86_x87_fcom", &[Type::F80, Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fcomp => (
                "x86_x87_fcomp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcompp => (
                "x86_x87_fcompp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucom => (
                "x86_x87_fucom",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomp => (
                "x86_x87_fucomp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucompp => (
                "x86_x87_fucompp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcomi => (
                "x86_x87_fcomi",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcomip => (
                "x86_x87_fcomip",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomi => (
                "x86_x87_fucomi",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomip => (
                "x86_x87_fucomip",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87FaddSt1 => (
                "x86_x87_fadd_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FmulSt1 => (
                "x86_x87_fmul_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FsubSt1 => (
                "x86_x87_fsub_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FsubrSt1 => (
                "x86_x87_fsubr_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FdivSt1 => (
                "x86_x87_fdiv_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FdivrSt1 => (
                "x86_x87_fdivr_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fadd => (
                "x86_x87_fadd",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fmul => (
                "x86_x87_fmul",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsub => (
                "x86_x87_fsub",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubr => (
                "x86_x87_fsubr",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdiv => (
                "x86_x87_fdiv",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivr => (
                "x86_x87_fdivr",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Faddp => (
                "x86_x87_faddp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fmulp => (
                "x86_x87_fmulp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubp => (
                "x86_x87_fsubp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubrp => (
                "x86_x87_fsubrp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivp => (
                "x86_x87_fdivp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivrp => (
                "x86_x87_fdivrp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fabs => ("x86_x87_fabs", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fchs => ("x86_x87_fchs", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fsqrt => (
                "x86_x87_fsqrt",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Frndint => (
                "x86_x87_frndint",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Ftst => ("x86_x87_ftst", &[Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fxam => ("x86_x87_fxam", &[Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fsincos => (
                "x86_x87_fsincos",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fptan => (
                "x86_x87_fptan",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fxtract => (
                "x86_x87_fxtract",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsin => ("x86_x87_fsin", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fcos => ("x86_x87_fcos", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87F2xm1 => (
                "x86_x87_f2xm1",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fpatan => (
                "x86_x87_fpatan",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fyl2x => (
                "x86_x87_fyl2x",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fyl2xp1 => (
                "x86_x87_fyl2xp1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fscale => (
                "x86_x87_fscale",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fprem => (
                "x86_x87_fprem",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fprem1 => (
                "x86_x87_fprem1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Wait => ("x86_x87_wait", &[], &[], false),
            Intrinsic::X86MmxEmms => ("x86_mmx_emms", &[], &[], false),
            Intrinsic::X86Lahf => ("x86_lahf", &[], &[Type::I8], false),
            Intrinsic::X86SahfExchange => ("x86_sahf_exchange", &[Type::I8], &[Type::I8], false),
            Intrinsic::X86X87Fnstenv16 => ("x86_x87_fnstenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnstenv32 => ("x86_x87_fnstenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fstenv16 => ("x86_x87_fstenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fstenv32 => ("x86_x87_fstenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fldenv16 => ("x86_x87_fldenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fldenv32 => ("x86_x87_fldenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnsave16 => ("x86_x87_fnsave16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnsave32 => ("x86_x87_fnsave32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fsave16 => ("x86_x87_fsave16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fsave32 => ("x86_x87_fsave32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Frstor16 => ("x86_x87_frstor16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Frstor32 => ("x86_x87_frstor32", &[Type::Ptr], &[], false),
            Intrinsic::X86Fxsave64 => ("x86_fxsave64", &[Type::Ptr], &[], false),
            Intrinsic::X86Fxrstor64 => ("x86_fxrstor64", &[Type::Ptr], &[], false),
            Intrinsic::X86Xsave64 => ("x86_xsave64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86CpuXcr0 => ("x86_cpu_xcr0", &[], &[Type::I64], false),
            Intrinsic::X86Xgetbv0 => ("x86_xgetbv0", &[], &[Type::I64], false),
            Intrinsic::X86Clflush => ("x86_clflush", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetchnta => ("x86_prefetchnta", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht0 => ("x86_prefetcht0", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht1 => ("x86_prefetcht1", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht2 => ("x86_prefetcht2", &[Type::Ptr], &[], false),
            Intrinsic::X86SseMovntps128 => (
                "x86_sse_movntps128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x4)],
                &[],
                false,
            ),
            Intrinsic::X86SseMovntpd128 => (
                "x86_sse_movntpd128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x2)],
                &[],
                false,
            ),
            Intrinsic::X86SseMovntdq128 => (
                "x86_sse_movntdq128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntps128 => (
                "x86_avx_vmovntps128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x4)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntps256 => (
                "x86_avx_vmovntps256",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x8)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntpd128 => (
                "x86_avx_vmovntpd128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntpd256 => (
                "x86_avx_vmovntpd256",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x4)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntdq128 => (
                "x86_avx_vmovntdq128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntdq256 => (
                "x86_avx_vmovntdq256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[],
                false,
            ),
            Intrinsic::X86Movnti32 => ("x86_movnti32", &[Type::Ptr, Type::I32], &[], false),
            Intrinsic::X86Movnti64 => ("x86_movnti64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86Sse41Movntdqa128 => (
                "x86_sse41_movntdqa128",
                &[Type::Ptr],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vmovntdqa256 => (
                "x86_avx2_vmovntdqa256",
                &[Type::Ptr],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Cpuid => ("x86_cpuid", &[Type::I32, Type::I32], &[Type::I128], false),
            Intrinsic::X86Rdtsc => ("x86_rdtsc", &[], &[Type::I64], false),
            Intrinsic::X86Lfence => ("x86_lfence", &[], &[], false),
            Intrinsic::X86Sfence => ("x86_sfence", &[], &[], false),
            Intrinsic::X86Mfence => ("x86_mfence", &[], &[], false),
            Intrinsic::X86Pause => ("x86_pause", &[], &[], false),

            Intrinsic::X86AvxVzeroall => ("x86_avx_vzeroall", &[], &[], false),
            Intrinsic::X86Cx16Cmpxchg16b => (
                "x86_cx16_cmpxchg16b",
                &[Type::Ptr, Type::I128, Type::I128],
                &[Type::I128],
                false,
            ),
            Intrinsic::Memcpy => ("memcpy", MEM, &[], false),
            Intrinsic::Memmove => ("memmove", MEM, &[], false),
            Intrinsic::Memcmp => ("memcmp", MEM, &[Type::I32], false),
            Intrinsic::Memset => ("memset", &[Type::Ptr, Type::I8, Type::I64], &[], false),
            Intrinsic::Debugtrap => ("debugtrap", &[], &[], false),
            Intrinsic::VaStart => ("va_start", &[Type::Ptr], &[], false),
            Intrinsic::StackAlloc => ("stack_alloc", &[Type::I64], &[Type::Ptr], false),
            Intrinsic::StackSave => ("stack_save", &[], &[Type::Ptr], false),
            Intrinsic::StackRestore => ("stack_restore", &[Type::Ptr], &[], false),
            Intrinsic::LifetimeStart => ("lifetime_start", &[Type::Ptr], &[], false),
            Intrinsic::LifetimeEnd => ("lifetime_end", &[Type::Ptr], &[], false),
            Intrinsic::FrameAddress => ("frame_address", &[], &[Type::Ptr], false),
            Intrinsic::X86F16cPhToPs128 => (
                "x86_f16c_ph_to_ps128",
                &[Type::I64],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86F16cPhToPs256 => (
                "x86_f16c_ph_to_ps256",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86F16cPsToPh128 => (
                "x86_f16c_ps_to_ph128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86F16cPsToPh256 => (
                "x86_f16c_ps_to_ph256",
                &[Type::Vector(crate::VectorType::F32x8), Type::I32],
                &[Type::Vector(crate::VectorType::I16x8)],
                false,
            ),
            Intrinsic::X86Bmi1Bextr32 => (
                "x86_bmi1_bextr32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi1Bextr64 => (
                "x86_bmi1_bextr64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Bmi2Pdep32 => (
                "x86_bmi2_pdep32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi2Pdep64 => (
                "x86_bmi2_pdep64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Bmi2Pext32 => (
                "x86_bmi2_pext32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi2Pext64 => (
                "x86_bmi2_pext64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Avx2Vgatherdps128 => (
                "x86_avx2_vgatherdps128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdps256 => (
                "x86_avx2_vgatherdps256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdpd128 => (
                "x86_avx2_vgatherdpd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdpd256 => (
                "x86_avx2_vgatherdpd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqps128 => (
                "x86_avx2_vgatherqps128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqps256 => (
                "x86_avx2_vgatherqps256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqpd128 => (
                "x86_avx2_vgatherqpd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqpd256 => (
                "x86_avx2_vgatherqpd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdd128 => (
                "x86_avx2_vpgatherdd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdd256 => (
                "x86_avx2_vpgatherdd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdq128 => (
                "x86_avx2_vpgatherdq128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdq256 => (
                "x86_avx2_vpgatherdq256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqd128 => (
                "x86_avx2_vpgatherqd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqd256 => (
                "x86_avx2_vpgatherqd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqq128 => (
                "x86_avx2_vpgatherqq128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqq256 => (
                "x86_avx2_vpgatherqq256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsLoad128 => (
                "x86_avx_vmaskmovps_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsStore128 => (
                "x86_avx_vmaskmovps_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsLoad256 => (
                "x86_avx_vmaskmovps_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsStore256 => (
                "x86_avx_vmaskmovps_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdLoad128 => (
                "x86_avx_vmaskmovpd_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdStore128 => (
                "x86_avx_vmaskmovpd_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdLoad256 => (
                "x86_avx_vmaskmovpd_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdStore256 => (
                "x86_avx_vmaskmovpd_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdLoad128 => (
                "x86_avx2_vpmaskmovd_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdStore128 => (
                "x86_avx2_vpmaskmovd_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdLoad256 => (
                "x86_avx2_vpmaskmovd_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdStore256 => (
                "x86_avx2_vpmaskmovd_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqLoad128 => (
                "x86_avx2_vpmaskmovq_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqStore128 => (
                "x86_avx2_vpmaskmovq_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqLoad256 => (
                "x86_avx2_vpmaskmovq_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqStore256 => (
                "x86_avx2_vpmaskmovq_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2pd128 => (
                "x86_avx_vcvtdq2pd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2pd256 => (
                "x86_avx_vcvtdq2pd256",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2ps128 => (
                "x86_avx_vcvtdq2ps128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2ps256 => (
                "x86_avx_vcvtdq2ps256",
                &[Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2dq128 => (
                "x86_avx_vcvtpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2dq256 => (
                "x86_avx_vcvtpd2dq256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2ps128 => (
                "x86_avx_vcvtpd2ps128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2ps256 => (
                "x86_avx_vcvtpd2ps256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2dq128 => (
                "x86_avx_vcvtps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2dq256 => (
                "x86_avx_vcvtps2dq256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2pd128 => (
                "x86_avx_vcvtps2pd128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2pd256 => (
                "x86_avx_vcvtps2pd256",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttpd2dq128 => (
                "x86_avx_vcvttpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttpd2dq256 => (
                "x86_avx_vcvttpd2dq256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttps2dq128 => (
                "x86_avx_vcvttps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttps2dq256 => (
                "x86_avx_vcvttps2dq256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtss2si32 => {
                ("x86_avx_vcvtss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvttss2si32 => {
                ("x86_avx_vcvttss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvtss2si64 => {
                ("x86_avx_vcvtss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvttss2si64 => {
                ("x86_avx_vcvttss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvtsd2si32 => {
                ("x86_avx_vcvtsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvttsd2si32 => {
                ("x86_avx_vcvttsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvtsd2si64 => {
                ("x86_avx_vcvtsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvttsd2si64 => {
                ("x86_avx_vcvttsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvtsi2ss32 => {
                ("x86_avx_vcvtsi2ss32", &[Type::I32], &[Type::F32], false)
            }
            Intrinsic::X86AvxVcvtsi2ss64 => {
                ("x86_avx_vcvtsi2ss64", &[Type::I64], &[Type::F32], false)
            }
            Intrinsic::X86AvxVcvtsi2sd32 => {
                ("x86_avx_vcvtsi2sd32", &[Type::I32], &[Type::F64], false)
            }
            Intrinsic::X86AvxVcvtsi2sd64 => {
                ("x86_avx_vcvtsi2sd64", &[Type::I64], &[Type::F64], false)
            }
            Intrinsic::X86AvxVcvtsd2ss => ("x86_avx_vcvtsd2ss", &[Type::F64], &[Type::F32], false),
            Intrinsic::X86AvxVcvtss2sd => ("x86_avx_vcvtss2sd", &[Type::F32], &[Type::F64], false),
            Intrinsic::X86AvxVcmpps128 => (
                "x86_avx_vcmpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcmpps256 => (
                "x86_avx_vcmpps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcmppd128 => (
                "x86_avx_vcmppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86AvxVcmppd256 => (
                "x86_avx_vcmppd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86AvxVcmpss => (
                "x86_avx_vcmpss",
                &[Type::F32, Type::F32, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVcmpsd => (
                "x86_avx_vcmpsd",
                &[Type::F64, Type::F64, Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86AvxVpermilps128 => (
                "x86_avx_vpermilps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86AvxVpermilps256 => (
                "x86_avx_vpermilps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVpermilpd128 => (
                "x86_avx_vpermilpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86AvxVpermilpd256 => (
                "x86_avx_vpermilpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpermd256 => (
                "x86_avx2_vpermd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpermps256 => (
                "x86_avx2_vpermps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVmovmskps128 => (
                "x86_avx_vmovmskps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskps256 => (
                "x86_avx_vmovmskps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskpd128 => (
                "x86_avx_vmovmskpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskpd256 => (
                "x86_avx_vmovmskpd256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVpmovmskb128 => (
                "x86_avx_vpmovmskb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Avx2Vpmovmskb256 => (
                "x86_avx2_vpmovmskb256",
                &[Type::Vector(crate::VectorType::I8x32)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestps128 => (
                "x86_avx_vtestps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestps256 => (
                "x86_avx_vtestps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestpd128 => (
                "x86_avx_vtestpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestpd256 => (
                "x86_avx_vtestpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVptest128 => (
                "x86_avx_vptest128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVptest256 => (
                "x86_avx_vptest256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVroundps128 => (
                "x86_avx_vroundps128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVroundps256 => (
                "x86_avx_vroundps256",
                &[Type::Vector(crate::VectorType::F32x8), Type::I32],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVroundpd128 => (
                "x86_avx_vroundpd128",
                &[Type::Vector(crate::VectorType::F64x2), Type::I32],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVroundpd256 => (
                "x86_avx_vroundpd256",
                &[Type::Vector(crate::VectorType::F64x4), Type::I32],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVroundss => (
                "x86_avx_vroundss",
                &[Type::F32, Type::I32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVroundsd => (
                "x86_avx_vroundsd",
                &[Type::F64, Type::I32],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVminps128 => (
                "x86_avx_vminps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVminps256 => (
                "x86_avx_vminps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVminpd128 => (
                "x86_avx_vminpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVminpd256 => (
                "x86_avx_vminpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVminss => (
                "x86_avx_vminss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVminsd => (
                "x86_avx_vminsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVmaxps128 => (
                "x86_avx_vmaxps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmaxps256 => (
                "x86_avx_vmaxps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmaxpd128 => (
                "x86_avx_vmaxpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmaxpd256 => (
                "x86_avx_vmaxpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaxss => (
                "x86_avx_vmaxss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVmaxsd => (
                "x86_avx_vmaxsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVrcpps128 => (
                "x86_avx_vrcpps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVrcpps256 => (
                "x86_avx_vrcpps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVrcpss => ("x86_avx_vrcpss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVrsqrtps128 => (
                "x86_avx_vrsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVrsqrtps256 => (
                "x86_avx_vrsqrtps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVrsqrtss => ("x86_avx_vrsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVcomiss => (
                "x86_avx_vcomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVucomiss => (
                "x86_avx_vucomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVcomisd => (
                "x86_avx_vcomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVucomisd => (
                "x86_avx_vucomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVmaskmovdqu => (
                "x86_avx_vmaskmovdqu",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[],
                false,
            ),
            Intrinsic::X86SseReadMxcsr => ("x86_sse_read_mxcsr", &[], &[Type::I32], false),
            Intrinsic::X86SseSwapMxcsr => ("x86_sse_swap_mxcsr", &[Type::I32], &[Type::I32], false),
            Intrinsic::X86SseMmxvalueCvtpi2ps => (
                "x86_sse_mmxvalue_cvtpi2ps",
                &[Type::Vector(crate::VectorType::F32x4), Type::I64],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtps2pi => (
                "x86_sse_mmxvalue_cvtps2pi",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvttps2pi => (
                "x86_sse_mmxvalue_cvttps2pi",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtpi2pd => (
                "x86_sse_mmxvalue_cvtpi2pd",
                &[Type::I64],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtpd2pi => (
                "x86_sse_mmxvalue_cvtpd2pi",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvttpd2pi => (
                "x86_sse_mmxvalue_cvttpd2pi",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Ssecvtdq2pd128 => (
                "x86_sse_cvtdq2pd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Ssecvtdq2ps128 => (
                "x86_sse_cvtdq2ps128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtpd2dq128 => (
                "x86_sse_cvtpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtpd2ps128 => (
                "x86_sse_cvtpd2ps128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtps2dq128 => (
                "x86_sse_cvtps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtps2pd128 => (
                "x86_sse_cvtps2pd128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Ssecvttpd2dq128 => (
                "x86_sse_cvttpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvttps2dq128 => (
                "x86_sse_cvttps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtss2si32 => {
                ("x86_sse_cvtss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvttss2si32 => {
                ("x86_sse_cvttss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvtss2si64 => {
                ("x86_sse_cvtss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvttss2si64 => {
                ("x86_sse_cvttss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvtsd2si32 => {
                ("x86_sse_cvtsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvttsd2si32 => {
                ("x86_sse_cvttsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvtsd2si64 => {
                ("x86_sse_cvtsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvttsd2si64 => {
                ("x86_sse_cvttsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvtsi2ss32 => {
                ("x86_sse_cvtsi2ss32", &[Type::I32], &[Type::F32], false)
            }
            Intrinsic::X86Ssecvtsi2ss64 => {
                ("x86_sse_cvtsi2ss64", &[Type::I64], &[Type::F32], false)
            }
            Intrinsic::X86Ssecvtsi2sd32 => {
                ("x86_sse_cvtsi2sd32", &[Type::I32], &[Type::F64], false)
            }
            Intrinsic::X86Ssecvtsi2sd64 => {
                ("x86_sse_cvtsi2sd64", &[Type::I64], &[Type::F64], false)
            }
            Intrinsic::X86Ssecvtsd2ss => ("x86_sse_cvtsd2ss", &[Type::F64], &[Type::F32], false),
            Intrinsic::X86Ssecvtss2sd => ("x86_sse_cvtss2sd", &[Type::F32], &[Type::F64], false),
            Intrinsic::X86SseMinps128 => (
                "x86_sse_minps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMinpd128 => (
                "x86_sse_minpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMinss => (
                "x86_sse_minss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMinsd => (
                "x86_sse_minsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseMaxps128 => (
                "x86_sse_maxps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMaxpd128 => (
                "x86_sse_maxpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMaxss => (
                "x86_sse_maxss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMaxsd => (
                "x86_sse_maxsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseRcpps128 => (
                "x86_sse_rcpps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseRcpss => ("x86_sse_rcpss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseRsqrtps128 => (
                "x86_sse_rsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseRsqrtss => ("x86_sse_rsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseComiss => (
                "x86_sse_comiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseUcomiss => (
                "x86_sse_ucomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseComisd => (
                "x86_sse_comisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseUcomisd => (
                "x86_sse_ucomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseCmpps128 => (
                "x86_sse_cmpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86SseCmppd128 => (
                "x86_sse_cmppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86SseCmpss => (
                "x86_sse_cmpss",
                &[Type::F32, Type::F32, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseCmpsd => (
                "x86_sse_cmpsd",
                &[Type::F64, Type::F64, Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseAddps128 => (
                "x86_sse_addps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseAddpd128 => (
                "x86_sse_addpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseAddss => (
                "x86_sse_addss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseAddsd => (
                "x86_sse_addsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseSubps128 => (
                "x86_sse_subps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseSubpd128 => (
                "x86_sse_subpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseSubss => (
                "x86_sse_subss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseSubsd => (
                "x86_sse_subsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseMulps128 => (
                "x86_sse_mulps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMulpd128 => (
                "x86_sse_mulpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMulss => (
                "x86_sse_mulss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMulsd => (
                "x86_sse_mulsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseDivps128 => (
                "x86_sse_divps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseDivpd128 => (
                "x86_sse_divpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseDivss => (
                "x86_sse_divss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseDivsd => (
                "x86_sse_divsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseSqrtps128 => (
                "x86_sse_sqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseSqrtpd128 => (
                "x86_sse_sqrtpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseSqrtss => ("x86_sse_sqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseSqrtsd => ("x86_sse_sqrtsd", &[Type::F64], &[Type::F64], false),
            Intrinsic::X86SseMovmskps128 => (
                "x86_sse_movmskps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SseMovmskpd128 => (
                "x86_sse_movmskpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SsePmovmskb128 => (
                "x86_sse_pmovmskb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SseMaskmovdqu => (
                "x86_sse_maskmovdqu",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Sse3Haddps => (
                "x86_sse3_haddps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Haddpd => (
                "x86_sse3_haddpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse3Hsubps => (
                "x86_sse3_hsubps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Hsubpd => (
                "x86_sse3_hsubpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse3Addsubps => (
                "x86_sse3_addsubps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Addsubpd => (
                "x86_sse3_addsubpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Roundps128 => (
                "x86_sse41_roundps128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse41Roundpd128 => (
                "x86_sse41_roundpd128",
                &[Type::Vector(crate::VectorType::F64x2), Type::I32],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Roundss => (
                "x86_sse41_roundss",
                &[Type::F32, Type::I32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86Sse41Roundsd => (
                "x86_sse41_roundsd",
                &[Type::F64, Type::I32],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86Sse41Dpps => (
                "x86_sse41_dpps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse41Dppd => (
                "x86_sse41_dppd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Ptest128 => (
                "x86_sse41_ptest128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32b => (
                "x86_sse42_crc32b",
                &[Type::I32, Type::I8],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32w => (
                "x86_sse42_crc32w",
                &[Type::I32, Type::I16],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32d => (
                "x86_sse42_crc32d",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32q => (
                "x86_sse42_crc32q",
                &[Type::I32, Type::I64],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpistriIndex => (
                "x86_sse42_pcmpistri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Pcmpistri64Index => (
                "x86_sse42_pcmpistri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Sse42PcmpistrmMask => (
                "x86_sse42_pcmpistrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42PcmpistriFlags => (
                "x86_sse42_pcmpistri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpestriIndex => (
                "x86_sse42_pcmpestri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpestrmMask => (
                "x86_sse42_pcmpestrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42PcmpestriFlags => (
                "x86_sse42_pcmpestri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestri64Index => (
                "x86_sse42_pcmpestri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestrm64Mask => (
                "x86_sse42_pcmpestrm64_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestri64Flags => (
                "x86_sse42_pcmpestri64_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86FmaVfmadd132pd128 => (
                "x86_fma_vfmadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132pd256 => (
                "x86_fma_vfmadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ps128 => (
                "x86_fma_vfmadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ps256 => (
                "x86_fma_vfmadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132sd128 => (
                "x86_fma_vfmadd132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ss128 => (
                "x86_fma_vfmadd132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmadd213pd128 => (
                "x86_fma_vfmadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213pd256 => (
                "x86_fma_vfmadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ps128 => (
                "x86_fma_vfmadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ps256 => (
                "x86_fma_vfmadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213sd128 => (
                "x86_fma_vfmadd213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ss128 => (
                "x86_fma_vfmadd213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmadd231pd128 => (
                "x86_fma_vfmadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231pd256 => (
                "x86_fma_vfmadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ps128 => (
                "x86_fma_vfmadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ps256 => (
                "x86_fma_vfmadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231sd128 => (
                "x86_fma_vfmadd231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ss128 => (
                "x86_fma_vfmadd231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132pd128 => (
                "x86_fma_vfmaddsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132pd256 => (
                "x86_fma_vfmaddsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213pd128 => (
                "x86_fma_vfmaddsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213pd256 => (
                "x86_fma_vfmaddsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231pd128 => (
                "x86_fma_vfmaddsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231pd256 => (
                "x86_fma_vfmaddsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132ps128 => (
                "x86_fma_vfmaddsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132ps256 => (
                "x86_fma_vfmaddsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213ps128 => (
                "x86_fma_vfmaddsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213ps256 => (
                "x86_fma_vfmaddsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231ps128 => (
                "x86_fma_vfmaddsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231ps256 => (
                "x86_fma_vfmaddsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132pd128 => (
                "x86_fma_vfmsubadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132pd256 => (
                "x86_fma_vfmsubadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213pd128 => (
                "x86_fma_vfmsubadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213pd256 => (
                "x86_fma_vfmsubadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231pd128 => (
                "x86_fma_vfmsubadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231pd256 => (
                "x86_fma_vfmsubadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132ps128 => (
                "x86_fma_vfmsubadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132ps256 => (
                "x86_fma_vfmsubadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213ps128 => (
                "x86_fma_vfmsubadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213ps256 => (
                "x86_fma_vfmsubadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231ps128 => (
                "x86_fma_vfmsubadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231ps256 => (
                "x86_fma_vfmsubadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132pd128 => (
                "x86_fma_vfmsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132pd256 => (
                "x86_fma_vfmsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ps128 => (
                "x86_fma_vfmsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ps256 => (
                "x86_fma_vfmsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132sd128 => (
                "x86_fma_vfmsub132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ss128 => (
                "x86_fma_vfmsub132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmsub213pd128 => (
                "x86_fma_vfmsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213pd256 => (
                "x86_fma_vfmsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ps128 => (
                "x86_fma_vfmsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ps256 => (
                "x86_fma_vfmsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213sd128 => (
                "x86_fma_vfmsub213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ss128 => (
                "x86_fma_vfmsub213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmsub231pd128 => (
                "x86_fma_vfmsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231pd256 => (
                "x86_fma_vfmsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ps128 => (
                "x86_fma_vfmsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ps256 => (
                "x86_fma_vfmsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231sd128 => (
                "x86_fma_vfmsub231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ss128 => (
                "x86_fma_vfmsub231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132pd128 => (
                "x86_fma_vfnmadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132pd256 => (
                "x86_fma_vfnmadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ps128 => (
                "x86_fma_vfnmadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ps256 => (
                "x86_fma_vfnmadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132sd128 => (
                "x86_fma_vfnmadd132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ss128 => (
                "x86_fma_vfnmadd132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213pd128 => (
                "x86_fma_vfnmadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213pd256 => (
                "x86_fma_vfnmadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ps128 => (
                "x86_fma_vfnmadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ps256 => (
                "x86_fma_vfnmadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213sd128 => (
                "x86_fma_vfnmadd213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ss128 => (
                "x86_fma_vfnmadd213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231pd128 => (
                "x86_fma_vfnmadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231pd256 => (
                "x86_fma_vfnmadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ps128 => (
                "x86_fma_vfnmadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ps256 => (
                "x86_fma_vfnmadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231sd128 => (
                "x86_fma_vfnmadd231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ss128 => (
                "x86_fma_vfnmadd231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132pd128 => (
                "x86_fma_vfnmsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132pd256 => (
                "x86_fma_vfnmsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ps128 => (
                "x86_fma_vfnmsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ps256 => (
                "x86_fma_vfnmsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132sd128 => (
                "x86_fma_vfnmsub132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ss128 => (
                "x86_fma_vfnmsub132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213pd128 => (
                "x86_fma_vfnmsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213pd256 => (
                "x86_fma_vfnmsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ps128 => (
                "x86_fma_vfnmsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ps256 => (
                "x86_fma_vfnmsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213sd128 => (
                "x86_fma_vfnmsub213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ss128 => (
                "x86_fma_vfnmsub213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231pd128 => (
                "x86_fma_vfnmsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231pd256 => (
                "x86_fma_vfnmsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ps128 => (
                "x86_fma_vfnmsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ps256 => (
                "x86_fma_vfnmsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231sd128 => (
                "x86_fma_vfnmsub231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ss128 => (
                "x86_fma_vfnmsub231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxPcmpistriIndex => (
                "x86_avx_vpcmpistri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpistri64Index => (
                "x86_avx_vpcmpistri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86AvxPcmpistrmMask => (
                "x86_avx_vpcmpistrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpistriFlags => (
                "x86_avx_vpcmpistri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestriIndex => (
                "x86_avx_vpcmpestri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestrmMask => (
                "x86_avx_vpcmpestrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpestriFlags => (
                "x86_avx_vpcmpestri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestri64Index => (
                "x86_avx_vpcmpestri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86AvxPcmpestrm64Mask => (
                "x86_avx_vpcmpestrm64_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpestri64Flags => (
                "x86_avx_vpcmpestri64_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVaddps128 => (
                "x86_avx_vaddps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVaddpd128 => (
                "x86_avx_vaddpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVaddss => (
                "x86_avx_vaddss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVaddsd => (
                "x86_avx_vaddsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVaddps256 => (
                "x86_avx_vaddps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVaddpd256 => (
                "x86_avx_vaddpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVsubps128 => (
                "x86_avx_vsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVsubpd128 => (
                "x86_avx_vsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVsubss => (
                "x86_avx_vsubss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVsubsd => (
                "x86_avx_vsubsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVsubps256 => (
                "x86_avx_vsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVsubpd256 => (
                "x86_avx_vsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmulps128 => (
                "x86_avx_vmulps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmulpd128 => (
                "x86_avx_vmulpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmulss => (
                "x86_avx_vmulss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVmulsd => (
                "x86_avx_vmulsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVmulps256 => (
                "x86_avx_vmulps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmulpd256 => (
                "x86_avx_vmulpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVdivps128 => (
                "x86_avx_vdivps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVdivpd128 => (
                "x86_avx_vdivpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVdivss => (
                "x86_avx_vdivss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVdivsd => (
                "x86_avx_vdivsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVdivps256 => (
                "x86_avx_vdivps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVdivpd256 => (
                "x86_avx_vdivpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVsqrtps128 => (
                "x86_avx_vsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVsqrtpd128 => (
                "x86_avx_vsqrtpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVsqrtss => ("x86_avx_vsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVsqrtsd => ("x86_avx_vsqrtsd", &[Type::F64], &[Type::F64], false),
            Intrinsic::X86AvxVsqrtps256 => (
                "x86_avx_vsqrtps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVsqrtpd256 => (
                "x86_avx_vsqrtpd256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVhaddps128 => (
                "x86_avx_vhaddps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVhaddps256 => (
                "x86_avx_vhaddps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVhaddpd128 => (
                "x86_avx_vhaddpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVhaddpd256 => (
                "x86_avx_vhaddpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVhsubps128 => (
                "x86_avx_vhsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVhsubps256 => (
                "x86_avx_vhsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVhsubpd128 => (
                "x86_avx_vhsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVhsubpd256 => (
                "x86_avx_vhsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVaddsubps128 => (
                "x86_avx_vaddsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVaddsubps256 => (
                "x86_avx_vaddsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVaddsubpd128 => (
                "x86_avx_vaddsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVaddsubpd256 => (
                "x86_avx_vaddsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVdpps128 => (
                "x86_avx_vdpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVdpps256 => (
                "x86_avx_vdpps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVdppd128 => (
                "x86_avx_vdppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
        };
        IntrinsicInfo {
            name,
            params,
            returns,
            pure,
        }
    }

    /// These target operations observe MXCSR and may set floating exception
    /// flags. They are pinned even when their SSA result is unused.
    pub fn is_x86_f16c(self) -> bool {
        matches!(
            self,
            Self::X86F16cPhToPs128
                | Self::X86F16cPhToPs256
                | Self::X86F16cPsToPh128
                | Self::X86F16cPsToPh256
        )
    }
    pub fn has_f16c_rounding(self) -> bool {
        matches!(self, Self::X86F16cPsToPh128 | Self::X86F16cPsToPh256)
    }

    pub fn is_x86_bmi(self) -> bool {
        matches!(
            self,
            Self::X86Bmi1Bextr32
                | Self::X86Bmi1Bextr64
                | Self::X86Bmi2Pdep32
                | Self::X86Bmi2Pdep64
                | Self::X86Bmi2Pext32
                | Self::X86Bmi2Pext64
        )
    }
    pub fn is_x86_cache_stream(self) -> bool {
        matches!(
            self,
            Self::X86Clflush
                | Self::X86Prefetchnta
                | Self::X86Prefetcht0
                | Self::X86Prefetcht1
                | Self::X86Prefetcht2
                | Self::X86SseMovntps128
                | Self::X86SseMovntpd128
                | Self::X86SseMovntdq128
                | Self::X86AvxVmovntps128
                | Self::X86AvxVmovntps256
                | Self::X86AvxVmovntpd128
                | Self::X86AvxVmovntpd256
                | Self::X86AvxVmovntdq128
                | Self::X86AvxVmovntdq256
                | Self::X86Movnti32
                | Self::X86Movnti64
                | Self::X86Sse41Movntdqa128
                | Self::X86Avx2Vmovntdqa256
        )
    }
    pub fn is_x86_byte_shuffle(self) -> bool {
        matches!(
            self,
            Self::X86Ssse3Pshufb128 | Self::X86AvxVpshufb128 | Self::X86Avx2Vpshufb256
        )
    }
    /// Opcode, per-lane-count form, and feature tier (SSE2=0/AVX=1/AVX2=2).
    pub fn x86_packed_shift(self) -> Option<(&'static str, bool, u8)> {
        match self {
            Self::X86Sse2Psllw128 => Some(("psllw", false, 0)),
            Self::X86Sse2Pslld128 => Some(("pslld", false, 0)),
            Self::X86Sse2Psllq128 => Some(("psllq", false, 0)),
            Self::X86Sse2Psrlw128 => Some(("psrlw", false, 0)),
            Self::X86Sse2Psrld128 => Some(("psrld", false, 0)),
            Self::X86Sse2Psrlq128 => Some(("psrlq", false, 0)),
            Self::X86Sse2Psraw128 => Some(("psraw", false, 0)),
            Self::X86Sse2Psrad128 => Some(("psrad", false, 0)),
            Self::X86AvxVpsllw128 => Some(("vpsllw", false, 1)),
            Self::X86AvxVpslld128 => Some(("vpslld", false, 1)),
            Self::X86AvxVpsllq128 => Some(("vpsllq", false, 1)),
            Self::X86AvxVpsrlw128 => Some(("vpsrlw", false, 1)),
            Self::X86AvxVpsrld128 => Some(("vpsrld", false, 1)),
            Self::X86AvxVpsrlq128 => Some(("vpsrlq", false, 1)),
            Self::X86AvxVpsraw128 => Some(("vpsraw", false, 1)),
            Self::X86AvxVpsrad128 => Some(("vpsrad", false, 1)),
            Self::X86Avx2Vpsllw256 => Some(("vpsllw", false, 2)),
            Self::X86Avx2Vpslld256 => Some(("vpslld", false, 2)),
            Self::X86Avx2Vpsllq256 => Some(("vpsllq", false, 2)),
            Self::X86Avx2Vpsrlw256 => Some(("vpsrlw", false, 2)),
            Self::X86Avx2Vpsrld256 => Some(("vpsrld", false, 2)),
            Self::X86Avx2Vpsrlq256 => Some(("vpsrlq", false, 2)),
            Self::X86Avx2Vpsraw256 => Some(("vpsraw", false, 2)),
            Self::X86Avx2Vpsrad256 => Some(("vpsrad", false, 2)),
            Self::X86Avx2Vpsllvd128 => Some(("vpsllvd", true, 2)),
            Self::X86Avx2Vpsllvq128 => Some(("vpsllvq", true, 2)),
            Self::X86Avx2Vpsrlvd128 => Some(("vpsrlvd", true, 2)),
            Self::X86Avx2Vpsrlvq128 => Some(("vpsrlvq", true, 2)),
            Self::X86Avx2Vpsravd128 => Some(("vpsravd", true, 2)),
            Self::X86Avx2Vpsllvd256 => Some(("vpsllvd", true, 2)),
            Self::X86Avx2Vpsllvq256 => Some(("vpsllvq", true, 2)),
            Self::X86Avx2Vpsrlvd256 => Some(("vpsrlvd", true, 2)),
            Self::X86Avx2Vpsrlvq256 => Some(("vpsrlvq", true, 2)),
            Self::X86Avx2Vpsravd256 => Some(("vpsravd", true, 2)),
            _ => None,
        }
    }
    /// Byte-lane shifts take a verified immediate0..255; halves stay independent.
    pub fn x86_byte_shift(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse2Pslldq128 => Some(("pslldq", 0)),
            Self::X86Sse2Psrldq128 => Some(("psrldq", 0)),
            Self::X86AvxVpslldq128 => Some(("vpslldq", 1)),
            Self::X86AvxVpsrldq128 => Some(("vpsrldq", 1)),
            Self::X86Avx2Vpslldq256 => Some(("vpslldq", 2)),
            Self::X86Avx2Vpsrldq256 => Some(("vpsrldq", 2)),
            _ => None,
        }
    }
    /// Pure numerical register contract: eight sliding four-byte SADs per 128-bit
    /// half, or unsigned minimum/lowest index with zero result padding. SAD imm8
    /// uses bits 2:0 in the low half and 5:3 in the high half; other bits ignored.
    /// Returns opcode, independent feature tier (2=SSE41,3=AVX,4=AVX+AVX2), SAD.
    pub fn x86_packed_search(self) -> Option<(&'static str, u8, bool)> {
        match self {
            Self::X86Sse41Mpsadbw128 => Some(("mpsadbw", 2, true)),
            Self::X86AvxVmpsadbw128 => Some(("vmpsadbw", 3, true)),
            Self::X86Avx2Vmpsadbw256 => Some(("vmpsadbw", 4, true)),
            Self::X86Sse41Phminposuw128 => Some(("phminposuw", 2, false)),
            Self::X86AvxVphminposuw128 => Some(("vphminposuw", 3, false)),
            _ => None,
        }
    }
    /// Exact pairwise/sliced integer operation; tier SSE2/SSSE3/SSE41/AVX/AVX2.
    /// Opcode and feature tier: SSE4.1 / AVX / AVX+AVX2.
    pub fn x86_sign_blend(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse41Blendvps128 => Some(("blendvps", 0)),
            Self::X86Sse41Blendvpd128 => Some(("blendvpd", 0)),
            Self::X86Sse41Pblendvb128 => Some(("pblendvb", 0)),
            Self::X86AvxVblendvps128 => Some(("vblendvps", 1)),
            Self::X86AvxVblendvpd128 => Some(("vblendvpd", 1)),
            Self::X86AvxVpblendvb128 => Some(("vpblendvb", 1)),
            Self::X86AvxVblendvps256 => Some(("vblendvps", 1)),
            Self::X86AvxVblendvpd256 => Some(("vblendvpd", 1)),
            Self::X86Avx2Vpblendvb256 => Some(("vpblendvb", 2)),
            _ => None,
        }
    }
    pub fn x86_packed_pair(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse2Packsswb128 => Some(("packsswb", 0)),
            Self::X86Sse2Packssdw128 => Some(("packssdw", 0)),
            Self::X86Sse2Packuswb128 => Some(("packuswb", 0)),
            Self::X86Sse41Packusdw128 => Some(("packusdw", 2)),
            Self::X86AvxVpacksswb128 => Some(("vpacksswb", 3)),
            Self::X86AvxVpackssdw128 => Some(("vpackssdw", 3)),
            Self::X86AvxVpackuswb128 => Some(("vpackuswb", 3)),
            Self::X86AvxVpackusdw128 => Some(("vpackusdw", 3)),
            Self::X86Avx2Vpacksswb256 => Some(("vpacksswb", 4)),
            Self::X86Avx2Vpackssdw256 => Some(("vpackssdw", 4)),
            Self::X86Avx2Vpackuswb256 => Some(("vpackuswb", 4)),
            Self::X86Avx2Vpackusdw256 => Some(("vpackusdw", 4)),
            Self::X86Ssse3Phaddw128 => Some(("phaddw", 1)),
            Self::X86Ssse3Phaddd128 => Some(("phaddd", 1)),
            Self::X86Ssse3Phaddsw128 => Some(("phaddsw", 1)),
            Self::X86Ssse3Phsubw128 => Some(("phsubw", 1)),
            Self::X86Ssse3Phsubd128 => Some(("phsubd", 1)),
            Self::X86Ssse3Phsubsw128 => Some(("phsubsw", 1)),
            Self::X86Sse2Paddsb128 => Some(("paddsb", 0)),
            Self::X86Sse2Paddsw128 => Some(("paddsw", 0)),
            Self::X86Sse2Paddusb128 => Some(("paddusb", 0)),
            Self::X86Sse2Paddusw128 => Some(("paddusw", 0)),
            Self::X86Sse2Psubsb128 => Some(("psubsb", 0)),
            Self::X86Sse2Psubsw128 => Some(("psubsw", 0)),
            Self::X86Sse2Psubusb128 => Some(("psubusb", 0)),
            Self::X86Sse2Psubusw128 => Some(("psubusw", 0)),
            Self::X86Sse2Pavgb128 => Some(("pavgb", 0)),
            Self::X86Sse2Pavgw128 => Some(("pavgw", 0)),
            Self::X86Ssse3Psignb128 => Some(("psignb", 1)),
            Self::X86Ssse3Psignw128 => Some(("psignw", 1)),
            Self::X86Ssse3Psignd128 => Some(("psignd", 1)),
            Self::X86Ssse3Pabsb128 => Some(("pabsb", 1)),
            Self::X86Ssse3Pabsw128 => Some(("pabsw", 1)),
            Self::X86Ssse3Pabsd128 => Some(("pabsd", 1)),
            Self::X86AvxVphaddw128 => Some(("vphaddw", 3)),
            Self::X86AvxVphaddd128 => Some(("vphaddd", 3)),
            Self::X86AvxVphaddsw128 => Some(("vphaddsw", 3)),
            Self::X86AvxVphsubw128 => Some(("vphsubw", 3)),
            Self::X86AvxVphsubd128 => Some(("vphsubd", 3)),
            Self::X86AvxVphsubsw128 => Some(("vphsubsw", 3)),
            Self::X86AvxVpaddsb128 => Some(("vpaddsb", 3)),
            Self::X86AvxVpaddsw128 => Some(("vpaddsw", 3)),
            Self::X86AvxVpaddusb128 => Some(("vpaddusb", 3)),
            Self::X86AvxVpaddusw128 => Some(("vpaddusw", 3)),
            Self::X86AvxVpsubsb128 => Some(("vpsubsb", 3)),
            Self::X86AvxVpsubsw128 => Some(("vpsubsw", 3)),
            Self::X86AvxVpsubusb128 => Some(("vpsubusb", 3)),
            Self::X86AvxVpsubusw128 => Some(("vpsubusw", 3)),
            Self::X86AvxVpavgb128 => Some(("vpavgb", 3)),
            Self::X86AvxVpavgw128 => Some(("vpavgw", 3)),
            Self::X86AvxVpsignb128 => Some(("vpsignb", 3)),
            Self::X86AvxVpsignw128 => Some(("vpsignw", 3)),
            Self::X86AvxVpsignd128 => Some(("vpsignd", 3)),
            Self::X86AvxVpabsb128 => Some(("vpabsb", 3)),
            Self::X86AvxVpabsw128 => Some(("vpabsw", 3)),
            Self::X86AvxVpabsd128 => Some(("vpabsd", 3)),
            Self::X86Avx2Vphaddw256 => Some(("vphaddw", 4)),
            Self::X86Avx2Vphaddd256 => Some(("vphaddd", 4)),
            Self::X86Avx2Vphaddsw256 => Some(("vphaddsw", 4)),
            Self::X86Avx2Vphsubw256 => Some(("vphsubw", 4)),
            Self::X86Avx2Vphsubd256 => Some(("vphsubd", 4)),
            Self::X86Avx2Vphsubsw256 => Some(("vphsubsw", 4)),
            Self::X86Avx2Vpaddsb256 => Some(("vpaddsb", 4)),
            Self::X86Avx2Vpaddsw256 => Some(("vpaddsw", 4)),
            Self::X86Avx2Vpaddusb256 => Some(("vpaddusb", 4)),
            Self::X86Avx2Vpaddusw256 => Some(("vpaddusw", 4)),
            Self::X86Avx2Vpsubsb256 => Some(("vpsubsb", 4)),
            Self::X86Avx2Vpsubsw256 => Some(("vpsubsw", 4)),
            Self::X86Avx2Vpsubusb256 => Some(("vpsubusb", 4)),
            Self::X86Avx2Vpsubusw256 => Some(("vpsubusw", 4)),
            Self::X86Avx2Vpavgb256 => Some(("vpavgb", 4)),
            Self::X86Avx2Vpavgw256 => Some(("vpavgw", 4)),
            Self::X86Avx2Vpsignb256 => Some(("vpsignb", 4)),
            Self::X86Avx2Vpsignw256 => Some(("vpsignw", 4)),
            Self::X86Avx2Vpsignd256 => Some(("vpsignd", 4)),
            Self::X86Avx2Vpabsb256 => Some(("vpabsb", 4)),
            Self::X86Avx2Vpabsw256 => Some(("vpabsw", 4)),
            Self::X86Avx2Vpabsd256 => Some(("vpabsd", 4)),
            Self::X86Sse2Pmaddwd128 => Some(("pmaddwd", 0)),
            Self::X86Ssse3Pmaddubsw128 => Some(("pmaddubsw", 1)),
            Self::X86Sse2Pmulhw128 => Some(("pmulhw", 0)),
            Self::X86Sse2Pmulhuw128 => Some(("pmulhuw", 0)),
            Self::X86Ssse3Pmulhrsw128 => Some(("pmulhrsw", 1)),
            Self::X86Sse41Pmuldq128 => Some(("pmuldq", 2)),
            Self::X86Sse2Pmuludq128 => Some(("pmuludq", 0)),
            Self::X86Sse2Psadbw128 => Some(("psadbw", 0)),
            Self::X86AvxVpmaddwd128 => Some(("vpmaddwd", 3)),
            Self::X86AvxVpmaddubsw128 => Some(("vpmaddubsw", 3)),
            Self::X86AvxVpmulhw128 => Some(("vpmulhw", 3)),
            Self::X86AvxVpmulhuw128 => Some(("vpmulhuw", 3)),
            Self::X86AvxVpmulhrsw128 => Some(("vpmulhrsw", 3)),
            Self::X86AvxVpmuldq128 => Some(("vpmuldq", 3)),
            Self::X86AvxVpmuludq128 => Some(("vpmuludq", 3)),
            Self::X86AvxVpsadbw128 => Some(("vpsadbw", 3)),
            Self::X86Avx2Vpmaddwd256 => Some(("vpmaddwd", 4)),
            Self::X86Avx2Vpmaddubsw256 => Some(("vpmaddubsw", 4)),
            Self::X86Avx2Vpmulhw256 => Some(("vpmulhw", 4)),
            Self::X86Avx2Vpmulhuw256 => Some(("vpmulhuw", 4)),
            Self::X86Avx2Vpmulhrsw256 => Some(("vpmulhrsw", 4)),
            Self::X86Avx2Vpmuldq256 => Some(("vpmuldq", 4)),
            Self::X86Avx2Vpmuludq256 => Some(("vpmuludq", 4)),
            Self::X86Avx2Vpsadbw256 => Some(("vpsadbw", 4)),
            _ => None,
        }
    }
    /// Single-step architectural numerical operations, with raw post-op status.
    pub fn is_x86_x87_constant(self) -> bool {
        matches!(
            self,
            Self::X86X87Fldpi
                | Self::X86X87Fldl2e
                | Self::X86X87Fldl2t
                | Self::X86X87Fldlg2
                | Self::X86X87Fldln2
        )
    }

    pub fn is_x86_x87_bcd(self) -> bool {
        matches!(self, Self::X86X87Fbld | Self::X86X87Fbstp)
    }

    pub fn is_x86_x87_integer(self) -> bool {
        matches!(
            self,
            Self::X86X87Fist16
                | Self::X86X87Fist32
                | Self::X86X87Fistp16
                | Self::X86X87Fistp32
                | Self::X86X87Fistp64
                | Self::X86X87Fisttp16
                | Self::X86X87Fisttp32
                | Self::X86X87Fisttp64
                | Self::X86X87Fild16
                | Self::X86X87Fild32
                | Self::X86X87Fild64
        )
    }
    pub fn is_x86_x87_fi_arithmetic(self) -> bool {
        matches!(
            self,
            Self::X86X87Fiadd16
                | Self::X86X87Fiadd32
                | Self::X86X87Fimul16
                | Self::X86X87Fimul32
                | Self::X86X87Fisub16
                | Self::X86X87Fisub32
                | Self::X86X87Fisubr16
                | Self::X86X87Fisubr32
                | Self::X86X87Fidiv16
                | Self::X86X87Fidiv32
                | Self::X86X87Fidivr16
                | Self::X86X87Fidivr32
        )
    }
    pub fn is_x86_x87_fisttp(self) -> bool {
        matches!(
            self,
            Self::X86X87Fisttp16 | Self::X86X87Fisttp32 | Self::X86X87Fisttp64
        )
    }
    /// Pinned raw relation/flags operations, with immediate post-op status.
    pub fn is_x86_x87_compare(self) -> bool {
        matches!(
            self,
            Self::X86X87Fcom
                | Self::X86X87Fcomp
                | Self::X86X87Fcompp
                | Self::X86X87Fucom
                | Self::X86X87Fucomp
                | Self::X86X87Fucompp
                | Self::X86X87Fcomi
                | Self::X86X87Fcomip
                | Self::X86X87Fucomi
                | Self::X86X87Fucomip
        )
    }
    pub fn is_x86_x87_binary(self) -> bool {
        matches!(
            self,
            Self::X86X87FaddSt1
                | Self::X86X87FmulSt1
                | Self::X86X87FsubSt1
                | Self::X86X87FsubrSt1
                | Self::X86X87FdivSt1
                | Self::X86X87FdivrSt1
                | Self::X86X87Fadd
                | Self::X86X87Fmul
                | Self::X86X87Fsub
                | Self::X86X87Fsubr
                | Self::X86X87Fdiv
                | Self::X86X87Fdivr
                | Self::X86X87Faddp
                | Self::X86X87Fmulp
                | Self::X86X87Fsubp
                | Self::X86X87Fsubrp
                | Self::X86X87Fdivp
                | Self::X86X87Fdivrp
        )
    }
    pub fn is_x86_x87_basic(self) -> bool {
        matches!(
            self,
            Self::X86X87Fabs
                | Self::X86X87Fchs
                | Self::X86X87Fsqrt
                | Self::X86X87Frndint
                | Self::X86X87Ftst
                | Self::X86X87Fxam
        )
    }
    pub fn is_x86_x87_numerical(self) -> bool {
        matches!(
            self,
            Self::X86X87Fsincos
                | Self::X86X87Fptan
                | Self::X86X87Fxtract
                | Self::X86X87Fsin
                | Self::X86X87Fcos
                | Self::X86X87F2xm1
                | Self::X86X87Fpatan
                | Self::X86X87Fyl2x
                | Self::X86X87Fyl2xp1
                | Self::X86X87Fscale
                | Self::X86X87Fprem
                | Self::X86X87Fprem1
        ) || self.is_x86_x87_basic()
            || self.is_x86_x87_binary()
            || self.is_x86_x87_integer()
            || self.is_x86_x87_bcd()
            || self.is_x86_x87_constant()
            || self.is_x86_x87_fi_arithmetic()
            || self.is_x86_x87_compare()
    }
    /// (opcode, operand-size bits, exact memory footprint). These are pinned raw
    /// operations under docs/targets/x86_64/f80/README.md, not FXSAVE-format adapters.
    pub fn x86_legacy_x87_image(self) -> Option<(&'static str, u16, usize)> {
        Some(match self {
            Self::X86X87Fnstenv16 => ("fnstenv", 16, 14),
            Self::X86X87Fnstenv32 => ("fnstenv", 32, 28),
            Self::X86X87Fstenv16 => ("fstenv", 16, 14),
            Self::X86X87Fstenv32 => ("fstenv", 32, 28),
            Self::X86X87Fldenv16 => ("fldenv", 16, 14),
            Self::X86X87Fldenv32 => ("fldenv", 32, 28),
            Self::X86X87Fnsave16 => ("fnsave", 16, 94),
            Self::X86X87Fnsave32 => ("fnsave", 32, 108),
            Self::X86X87Fsave16 => ("fsave", 16, 94),
            Self::X86X87Fsave32 => ("fsave", 32, 108),
            Self::X86X87Frstor16 => ("frstor", 16, 94),
            Self::X86X87Frstor32 => ("frstor", 32, 108),
            _ => return None,
        })
    }

    /// Raw 2w/w architectural division, quotient then remainder. See docs/targets/x86_64/intrinsics.md.
    pub fn x86_raw_div(self) -> Option<(u16, bool)> {
        match self {
            Self::X86DivU8 => Some((8, false)),
            Self::X86DivU16 => Some((16, false)),
            Self::X86DivU32 => Some((32, false)),
            Self::X86DivU64 => Some((64, false)),
            Self::X86DivS8 => Some((8, true)),
            Self::X86DivS16 => Some((16, true)),
            Self::X86DivS32 => Some((32, true)),
            Self::X86DivS64 => Some((64, true)),
            _ => None,
        }
    }

    /// (output, element bits, REP, explicit owned terminal FLAGS context).
    pub fn x86_string_port_io(self) -> Option<(bool, u16, bool, bool)> {
        Some(match self {
            Self::X86Ins8 => (false, 8, false, false),
            Self::X86Ins16 => (false, 16, false, false),
            Self::X86Ins32 => (false, 32, false, false),
            Self::X86Outs8 => (true, 8, false, false),
            Self::X86Outs16 => (true, 16, false, false),
            Self::X86Outs32 => (true, 32, false, false),
            Self::X86RepIns8 => (false, 8, true, false),
            Self::X86RepIns16 => (false, 16, true, false),
            Self::X86RepIns32 => (false, 32, true, false),
            Self::X86RepOuts8 => (true, 8, true, false),
            Self::X86RepOuts16 => (true, 16, true, false),
            Self::X86RepOuts32 => (true, 32, true, false),
            Self::X86Ins8OwnedTerminal => (false, 8, false, true),
            Self::X86Ins16OwnedTerminal => (false, 16, false, true),
            Self::X86Ins32OwnedTerminal => (false, 32, false, true),
            Self::X86Outs8OwnedTerminal => (true, 8, false, true),
            Self::X86Outs16OwnedTerminal => (true, 16, false, true),
            Self::X86Outs32OwnedTerminal => (true, 32, false, true),
            Self::X86RepIns8OwnedTerminal => (false, 8, true, true),
            Self::X86RepIns16OwnedTerminal => (false, 16, true, true),
            Self::X86RepIns32OwnedTerminal => (false, 32, true, true),
            Self::X86RepOuts8OwnedTerminal => (true, 8, true, true),
            Self::X86RepOuts16OwnedTerminal => (true, 16, true, true),
            Self::X86RepOuts32OwnedTerminal => (true, 32, true, true),
            _ => return None,
        })
    }

    pub fn is_x86_target(self) -> bool {
        if matches!(self, x86_system_intrinsics!(x86_64)) {
            return true;
        }
        matches!(
            self,
            Self::X86RflagsRead
                | Self::X86RflagsSwap
                | Self::X86X87Region
                | Self::X86Xrstor64
                | Self::X86Xsave64
                | Self::X86Lahf
                | Self::X86SahfExchange
                | Self::X86Fxsave64
                | Self::X86Fxrstor64
                | Self::X86X87ReadCwNowait
                | Self::X86X87ReadSwNowait
                | Self::X86X87ReadCwWait
                | Self::X86X87ReadSwWait
                | Self::X86X87ExchangeCw
                | Self::X86X87ClexNowait
                | Self::X86X87ClexWait
                | Self::X86X87InitNowait
                | Self::X86X87InitWait
                | Self::X86X87Wait
                | Self::X86MmxEmms
                | Self::X86CpuXcr0
                | Self::X86Xgetbv0
                | Self::X86Cpuid
                | Self::X86Rdtsc
                | Self::X86Rdpmc
                | Self::X86Lfence
                | Self::X86Sfence
                | Self::X86Mfence
                | Self::X86Pause
        ) || self.x86_system_register().is_some()
            || self.x86_system_write().is_some()
            || self.x86_descriptor_query().is_some()
            || self.x86_selector_query().is_some()
            || self.x86_port_io().is_some()
            || self.x86_string_port_io().is_some()
            || self.x86_raw_div().is_some()
            || self.x86_legacy_x87_image().is_some()
            || self.x86_owned_x87_stack().is_some()
            || self.is_x86_x87_numerical()
            || self.x86_packed_search().is_some()
            || self.x86_sign_blend().is_some()
            || self.x86_packed_pair().is_some()
            || self.x86_packed_shift().is_some()
            || self.x86_byte_shift().is_some()
            || self.is_x86_byte_shuffle()
            || self.is_x86_cache_stream()
            || self == Self::X86AvxVzeroall
            || matches!(self, Self::X86SseReadMxcsr | Self::X86SseSwapMxcsr)
            || self.is_x86_mmx_value_convert()
            || self.is_x86_raw_fma()
            || self == Self::X86Cx16Cmpxchg16b
            || self.is_x86_f16c()
            || self.is_x86_bmi()
            || self.is_x86_gather()
            || self.is_x86_masked_mem()
            || self.is_x86_raw_fp_convert()
            || self.is_x86_fp_compare()
            || self.is_x86_sha256()
            || self.x86_avx512_bw_core().is_some()
            || self.x86_avx512_bw_mask().is_some()
            || self.x86_avx512_vl_shift().is_some()
            || self.x86_avx512_bw_shift().is_some()
            || self.x86_avx512_bw_pack().is_some()
            || self.x86_avx512_bw_mul().is_some()
            || self.is_x86_avx512_mask()
            || self.is_x86_avx512_indexed()
            || self.x86_avx512_special().is_some()
            || self.x86_avx512_dq_special().is_some()
            || self.x86_avx512_dq_lane().is_some()
            || self.x86_avx512_dq_bridge().is_some()
            || self.x86_avx512_dq_convert().is_some()
            || self.x86_avx512_vl_convert().is_some()
            || self.is_x86_avx512_convert()
            || self.is_x86_avx512_scalar_move()
            || self.x86_avx512_packed_opcode().is_some()
            || self.x86_avx512_scalar_convert().is_some()
            || self.x86_avx512_scalar().is_some()
            || self.x86_avx512_fma_opcode().is_some()
            || self.x86_avx512_scalar_fma_opcode().is_some()
            || self.x86_avx512_permute().is_some()
            || self.is_x86_avx512_pack()
            || self.is_x86_avx512_integer()
            || self.is_x86_avx512_mask_alu()
            || self.is_x86_avx512_integer_convert()
            || self.is_x86_avx512_lane()
            || self.x86_aes_opcode().is_some()
            || self.is_x86_pure_bits()
            || self.is_x86_fp_misc()
            || self == Self::X86AvxVmaskmovdqu
            || self.is_x86_legacy_raw_fp_convert()
            || self.is_x86_legacy_fp_misc()
            || self.is_x86_legacy_fp_compare()
            || self.is_x86_legacy_sign_mask()
            || self == Self::X86SseMaskmovdqu
            || self.is_x86_legacy_fp_v2()
            || self.is_x86_pclmul()
            || self.is_x86_crc32()
            || self.is_x86_legacy_ptest()
            || self.is_x86_string()
            || self.is_x86_fp_horizontal_dot()
    }

    /// Inactive sign-mask lanes do not access memory. Active lanes read exactly
    /// one scalar; faulting accesses may partially complete in hardware. SSA
    /// input vectors remain unchanged; no signal-frame register ABI is promised.
    /// QPS/QD128 gather two lanes and zero the result's upper64 bits.
    pub fn is_x86_gather(self) -> bool {
        matches!(
            self,
            Self::X86Avx2Vgatherdps128
                | Self::X86Avx2Vgatherdps256
                | Self::X86Avx2Vgatherdpd128
                | Self::X86Avx2Vgatherdpd256
                | Self::X86Avx2Vgatherqps128
                | Self::X86Avx2Vgatherqps256
                | Self::X86Avx2Vgatherqpd128
                | Self::X86Avx2Vgatherqpd256
                | Self::X86Avx2Vpgatherdd128
                | Self::X86Avx2Vpgatherdd256
                | Self::X86Avx2Vpgatherdq128
                | Self::X86Avx2Vpgatherdq256
                | Self::X86Avx2Vpgatherqd128
                | Self::X86Avx2Vpgatherqd256
                | Self::X86Avx2Vpgatherqq128
                | Self::X86Avx2Vpgatherqq256
        )
    }

    pub fn is_x86_masked_mem(self) -> bool {
        matches!(
            self,
            Self::X86AvxVmaskmovpsLoad128
                | Self::X86AvxVmaskmovpsStore128
                | Self::X86AvxVmaskmovpsLoad256
                | Self::X86AvxVmaskmovpsStore256
                | Self::X86AvxVmaskmovpdLoad128
                | Self::X86AvxVmaskmovpdStore128
                | Self::X86AvxVmaskmovpdLoad256
                | Self::X86AvxVmaskmovpdStore256
                | Self::X86Avx2VpmaskmovdLoad128
                | Self::X86Avx2VpmaskmovdStore128
                | Self::X86Avx2VpmaskmovdLoad256
                | Self::X86Avx2VpmaskmovdStore256
                | Self::X86Avx2VpmaskmovqLoad128
                | Self::X86Avx2VpmaskmovqStore128
                | Self::X86Avx2VpmaskmovqLoad256
                | Self::X86Avx2VpmaskmovqStore256
        )
    }
    pub fn is_x86_masked_store(self) -> bool {
        matches!(
            self,
            Self::X86AvxVmaskmovpsStore128
                | Self::X86AvxVmaskmovpsStore256
                | Self::X86AvxVmaskmovpdStore128
                | Self::X86AvxVmaskmovpdStore256
                | Self::X86Avx2VpmaskmovdStore128
                | Self::X86Avx2VpmaskmovdStore256
                | Self::X86Avx2VpmaskmovqStore128
                | Self::X86Avx2VpmaskmovqStore256
        )
    }

    pub fn is_x86_raw_fp_convert(self) -> bool {
        self.x86_raw_fp_opcode().is_some()
    }
    pub fn x86_raw_fp_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVcvtdq2pd128 => Some("vcvtdq2pd"),
            Self::X86AvxVcvtdq2pd256 => Some("vcvtdq2pd"),
            Self::X86AvxVcvtdq2ps128 => Some("vcvtdq2ps"),
            Self::X86AvxVcvtdq2ps256 => Some("vcvtdq2ps"),
            Self::X86AvxVcvtpd2dq128 => Some("vcvtpd2dq"),
            Self::X86AvxVcvtpd2dq256 => Some("vcvtpd2dq"),
            Self::X86AvxVcvtpd2ps128 => Some("vcvtpd2ps"),
            Self::X86AvxVcvtpd2ps256 => Some("vcvtpd2ps"),
            Self::X86AvxVcvtps2dq128 => Some("vcvtps2dq"),
            Self::X86AvxVcvtps2dq256 => Some("vcvtps2dq"),
            Self::X86AvxVcvtps2pd128 => Some("vcvtps2pd"),
            Self::X86AvxVcvtps2pd256 => Some("vcvtps2pd"),
            Self::X86AvxVcvttpd2dq128 => Some("vcvttpd2dq"),
            Self::X86AvxVcvttpd2dq256 => Some("vcvttpd2dq"),
            Self::X86AvxVcvttps2dq128 => Some("vcvttps2dq"),
            Self::X86AvxVcvttps2dq256 => Some("vcvttps2dq"),
            Self::X86AvxVcvtss2si32 => Some("vcvtss2si"),
            Self::X86AvxVcvttss2si32 => Some("vcvttss2si"),
            Self::X86AvxVcvtss2si64 => Some("vcvtss2si"),
            Self::X86AvxVcvttss2si64 => Some("vcvttss2si"),
            Self::X86AvxVcvtsd2si32 => Some("vcvtsd2si"),
            Self::X86AvxVcvttsd2si32 => Some("vcvttsd2si"),
            Self::X86AvxVcvtsd2si64 => Some("vcvtsd2si"),
            Self::X86AvxVcvttsd2si64 => Some("vcvttsd2si"),
            Self::X86AvxVcvtsi2ss32 => Some("vcvtsi2ss"),
            Self::X86AvxVcvtsi2ss64 => Some("vcvtsi2ss"),
            Self::X86AvxVcvtsi2sd32 => Some("vcvtsi2sd"),
            Self::X86AvxVcvtsi2sd64 => Some("vcvtsi2sd"),
            Self::X86AvxVcvtsd2ss => Some("vcvtsd2ss"),
            Self::X86AvxVcvtss2sd => Some("vcvtss2sd"),
            _ => None,
        }
    }

    pub fn is_x86_fp_compare(self) -> bool {
        self.x86_fp_compare_opcode().is_some()
    }
    pub fn x86_fp_compare_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVcmpps128 => Some("vcmpps"),
            Self::X86AvxVcmpps256 => Some("vcmpps"),
            Self::X86AvxVcmppd128 => Some("vcmppd"),
            Self::X86AvxVcmppd256 => Some("vcmppd"),
            Self::X86AvxVcmpss => Some("vcmpss"),
            Self::X86AvxVcmpsd => Some("vcmpsd"),
            _ => None,
        }
    }

    pub fn x86_aes_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Aesenc128 => Some("aesenc"),
            Self::X86Aesenclast128 => Some("aesenclast"),
            Self::X86Aesdec128 => Some("aesdec"),
            Self::X86Aesdeclast128 => Some("aesdeclast"),
            Self::X86Aesimc128 => Some("aesimc"),
            Self::X86Aeskeygenassist128 => Some("aeskeygenassist"),
            _ => None,
        }
    }
    pub fn is_x86_pure_bits(self) -> bool {
        self.x86_pure_bits_opcode().is_some()
    }
    pub fn x86_pure_bits_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVpermilps128 => Some("vpermilps"),
            Self::X86AvxVpermilps256 => Some("vpermilps"),
            Self::X86AvxVpermilpd128 => Some("vpermilpd"),
            Self::X86AvxVpermilpd256 => Some("vpermilpd"),
            Self::X86Avx2Vpermd256 => Some("vpermd"),
            Self::X86Avx2Vpermps256 => Some("vpermps"),
            Self::X86AvxVmovmskps128 => Some("vmovmskps"),
            Self::X86AvxVmovmskps256 => Some("vmovmskps"),
            Self::X86AvxVmovmskpd128 => Some("vmovmskpd"),
            Self::X86AvxVmovmskpd256 => Some("vmovmskpd"),
            Self::X86AvxVpmovmskb128 => Some("vpmovmskb"),
            Self::X86Avx2Vpmovmskb256 => Some("vpmovmskb"),
            Self::X86AvxVtestps128 => Some("vtestps"),
            Self::X86AvxVtestps256 => Some("vtestps"),
            Self::X86AvxVtestpd128 => Some("vtestpd"),
            Self::X86AvxVtestpd256 => Some("vtestpd"),
            Self::X86AvxVptest128 => Some("vptest"),
            Self::X86AvxVptest256 => Some("vptest"),
            _ => None,
        }
    }

    pub fn is_x86_fp_misc(self) -> bool {
        self.x86_fp_misc_opcode().is_some()
    }
    pub fn has_x86_round_control(self) -> bool {
        self.x86_fp_misc_opcode()
            .is_some_and(|s| s.starts_with("vround"))
    }
    pub fn x86_fp_misc_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVaddps128 => Some("vaddps"),
            Self::X86AvxVaddpd128 => Some("vaddpd"),
            Self::X86AvxVaddss => Some("vaddss"),
            Self::X86AvxVaddsd => Some("vaddsd"),
            Self::X86AvxVaddps256 => Some("vaddps"),
            Self::X86AvxVaddpd256 => Some("vaddpd"),
            Self::X86AvxVsubps128 => Some("vsubps"),
            Self::X86AvxVsubpd128 => Some("vsubpd"),
            Self::X86AvxVsubss => Some("vsubss"),
            Self::X86AvxVsubsd => Some("vsubsd"),
            Self::X86AvxVsubps256 => Some("vsubps"),
            Self::X86AvxVsubpd256 => Some("vsubpd"),
            Self::X86AvxVmulps128 => Some("vmulps"),
            Self::X86AvxVmulpd128 => Some("vmulpd"),
            Self::X86AvxVmulss => Some("vmulss"),
            Self::X86AvxVmulsd => Some("vmulsd"),
            Self::X86AvxVmulps256 => Some("vmulps"),
            Self::X86AvxVmulpd256 => Some("vmulpd"),
            Self::X86AvxVdivps128 => Some("vdivps"),
            Self::X86AvxVdivpd128 => Some("vdivpd"),
            Self::X86AvxVdivss => Some("vdivss"),
            Self::X86AvxVdivsd => Some("vdivsd"),
            Self::X86AvxVdivps256 => Some("vdivps"),
            Self::X86AvxVdivpd256 => Some("vdivpd"),
            Self::X86AvxVsqrtps128 => Some("vsqrtps"),
            Self::X86AvxVsqrtpd128 => Some("vsqrtpd"),
            Self::X86AvxVsqrtss => Some("vsqrtss"),
            Self::X86AvxVsqrtsd => Some("vsqrtsd"),
            Self::X86AvxVsqrtps256 => Some("vsqrtps"),
            Self::X86AvxVsqrtpd256 => Some("vsqrtpd"),
            Self::X86AvxVroundps128 => Some("vroundps"),
            Self::X86AvxVroundps256 => Some("vroundps"),
            Self::X86AvxVroundpd128 => Some("vroundpd"),
            Self::X86AvxVroundpd256 => Some("vroundpd"),
            Self::X86AvxVroundss => Some("vroundss"),
            Self::X86AvxVroundsd => Some("vroundsd"),
            Self::X86AvxVminps128 => Some("vminps"),
            Self::X86AvxVminps256 => Some("vminps"),
            Self::X86AvxVminpd128 => Some("vminpd"),
            Self::X86AvxVminpd256 => Some("vminpd"),
            Self::X86AvxVminss => Some("vminss"),
            Self::X86AvxVminsd => Some("vminsd"),
            Self::X86AvxVmaxps128 => Some("vmaxps"),
            Self::X86AvxVmaxps256 => Some("vmaxps"),
            Self::X86AvxVmaxpd128 => Some("vmaxpd"),
            Self::X86AvxVmaxpd256 => Some("vmaxpd"),
            Self::X86AvxVmaxss => Some("vmaxss"),
            Self::X86AvxVmaxsd => Some("vmaxsd"),
            Self::X86AvxVrcpps128 => Some("vrcpps"),
            Self::X86AvxVrcpps256 => Some("vrcpps"),
            Self::X86AvxVrcpss => Some("vrcpss"),
            Self::X86AvxVrsqrtps128 => Some("vrsqrtps"),
            Self::X86AvxVrsqrtps256 => Some("vrsqrtps"),
            Self::X86AvxVrsqrtss => Some("vrsqrtss"),
            Self::X86AvxVcomiss => Some("vcomiss"),
            Self::X86AvxVucomiss => Some("vucomiss"),
            Self::X86AvxVcomisd => Some("vcomisd"),
            Self::X86AvxVucomisd => Some("vucomisd"),
            _ => None,
        }
    }

    /// Numerical/MXCSR equivalence to the six MMX conversions, without their
    /// physical MMX or x87 tag/register state changes. All values are immutable.
    pub fn is_x86_mmx_value_convert(self) -> bool {
        matches!(
            self,
            Self::X86SseMmxvalueCvtpi2ps
                | Self::X86SseMmxvalueCvtps2pi
                | Self::X86SseMmxvalueCvttps2pi
                | Self::X86SseMmxvalueCvtpi2pd
                | Self::X86SseMmxvalueCvtpd2pi
                | Self::X86SseMmxvalueCvttpd2pi
        )
    }

    pub fn is_x86_legacy_raw_fp_convert(self) -> bool {
        self.x86_legacy_raw_fp_opcode().is_some()
    }
    pub fn x86_legacy_raw_fp_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Ssecvtdq2pd128 => Some("cvtdq2pd"),
            Self::X86Ssecvtdq2ps128 => Some("cvtdq2ps"),
            Self::X86Ssecvtpd2dq128 => Some("cvtpd2dq"),
            Self::X86Ssecvtpd2ps128 => Some("cvtpd2ps"),
            Self::X86Ssecvtps2dq128 => Some("cvtps2dq"),
            Self::X86Ssecvtps2pd128 => Some("cvtps2pd"),
            Self::X86Ssecvttpd2dq128 => Some("cvttpd2dq"),
            Self::X86Ssecvttps2dq128 => Some("cvttps2dq"),
            Self::X86Ssecvtss2si32 => Some("cvtss2si"),
            Self::X86Ssecvttss2si32 => Some("cvttss2si"),
            Self::X86Ssecvtss2si64 => Some("cvtss2si"),
            Self::X86Ssecvttss2si64 => Some("cvttss2si"),
            Self::X86Ssecvtsd2si32 => Some("cvtsd2si"),
            Self::X86Ssecvttsd2si32 => Some("cvttsd2si"),
            Self::X86Ssecvtsd2si64 => Some("cvtsd2si"),
            Self::X86Ssecvttsd2si64 => Some("cvttsd2si"),
            Self::X86Ssecvtsi2ss32 => Some("cvtsi2ss"),
            Self::X86Ssecvtsi2ss64 => Some("cvtsi2ss"),
            Self::X86Ssecvtsi2sd32 => Some("cvtsi2sd"),
            Self::X86Ssecvtsi2sd64 => Some("cvtsi2sd"),
            Self::X86Ssecvtsd2ss => Some("cvtsd2ss"),
            Self::X86Ssecvtss2sd => Some("cvtss2sd"),
            _ => None,
        }
    }

    pub fn is_x86_legacy_fp_misc(self) -> bool {
        self.x86_legacy_fp_misc_opcode().is_some()
    }
    pub fn x86_legacy_fp_misc_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseAddps128 => Some("addps"),
            Self::X86SseAddpd128 => Some("addpd"),
            Self::X86SseAddss => Some("addss"),
            Self::X86SseAddsd => Some("addsd"),
            Self::X86SseSubps128 => Some("subps"),
            Self::X86SseSubpd128 => Some("subpd"),
            Self::X86SseSubss => Some("subss"),
            Self::X86SseSubsd => Some("subsd"),
            Self::X86SseMulps128 => Some("mulps"),
            Self::X86SseMulpd128 => Some("mulpd"),
            Self::X86SseMulss => Some("mulss"),
            Self::X86SseMulsd => Some("mulsd"),
            Self::X86SseDivps128 => Some("divps"),
            Self::X86SseDivpd128 => Some("divpd"),
            Self::X86SseDivss => Some("divss"),
            Self::X86SseDivsd => Some("divsd"),
            Self::X86SseSqrtps128 => Some("sqrtps"),
            Self::X86SseSqrtpd128 => Some("sqrtpd"),
            Self::X86SseSqrtss => Some("sqrtss"),
            Self::X86SseSqrtsd => Some("sqrtsd"),
            Self::X86SseMinps128 => Some("minps"),
            Self::X86SseMinpd128 => Some("minpd"),
            Self::X86SseMinss => Some("minss"),
            Self::X86SseMinsd => Some("minsd"),
            Self::X86SseMaxps128 => Some("maxps"),
            Self::X86SseMaxpd128 => Some("maxpd"),
            Self::X86SseMaxss => Some("maxss"),
            Self::X86SseMaxsd => Some("maxsd"),
            Self::X86SseRcpps128 => Some("rcpps"),
            Self::X86SseRcpss => Some("rcpss"),
            Self::X86SseRsqrtps128 => Some("rsqrtps"),
            Self::X86SseRsqrtss => Some("rsqrtss"),
            Self::X86SseComiss => Some("comiss"),
            Self::X86SseUcomiss => Some("ucomiss"),
            Self::X86SseComisd => Some("comisd"),
            Self::X86SseUcomisd => Some("ucomisd"),
            _ => None,
        }
    }
    pub fn is_x86_legacy_fp_compare(self) -> bool {
        self.x86_legacy_fp_compare_opcode().is_some()
    }
    pub fn x86_legacy_fp_compare_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseCmpps128 => Some("cmpps"),
            Self::X86SseCmppd128 => Some("cmppd"),
            Self::X86SseCmpss => Some("cmpss"),
            Self::X86SseCmpsd => Some("cmpsd"),
            _ => None,
        }
    }

    pub fn is_x86_legacy_sign_mask(self) -> bool {
        self.x86_legacy_sign_mask_opcode().is_some()
    }
    pub fn x86_legacy_sign_mask_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseMovmskps128 => Some("movmskps"),
            Self::X86SseMovmskpd128 => Some("movmskpd"),
            Self::X86SsePmovmskb128 => Some("pmovmskb"),
            _ => None,
        }
    }
    pub fn is_x86_legacy_fp_v2(self) -> bool {
        self.x86_legacy_fp_v2_opcode().is_some()
    }
    pub fn x86_legacy_fp_v2_control(self) -> Option<(usize, i64)> {
        match self {
            Self::X86Sse41Roundps128 => Some((1, 16)),
            Self::X86Sse41Roundpd128 => Some((1, 16)),
            Self::X86Sse41Roundss => Some((1, 16)),
            Self::X86Sse41Roundsd => Some((1, 16)),
            Self::X86Sse41Dpps => Some((2, 256)),
            Self::X86Sse41Dppd => Some((2, 256)),
            _ => None,
        }
    }
    pub fn x86_legacy_fp_v2_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Sse3Haddps => Some("haddps"),
            Self::X86Sse3Haddpd => Some("haddpd"),
            Self::X86Sse3Hsubps => Some("hsubps"),
            Self::X86Sse3Hsubpd => Some("hsubpd"),
            Self::X86Sse3Addsubps => Some("addsubps"),
            Self::X86Sse3Addsubpd => Some("addsubpd"),
            Self::X86Sse41Roundps128 => Some("roundps"),
            Self::X86Sse41Roundpd128 => Some("roundpd"),
            Self::X86Sse41Roundss => Some("roundss"),
            Self::X86Sse41Roundsd => Some("roundsd"),
            Self::X86Sse41Dpps => Some("dpps"),
            Self::X86Sse41Dppd => Some("dppd"),
            _ => None,
        }
    }
    pub fn is_x86_crc32(self) -> bool {
        self.x86_crc32_opcode().is_some()
    }
    pub fn is_x86_legacy_ptest(self) -> bool {
        self == Self::X86Sse41Ptest128
    }
    pub fn x86_crc32_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Sse42Crc32b => Some("crc32"),
            Self::X86Sse42Crc32w => Some("crc32"),
            Self::X86Sse42Crc32d => Some("crc32"),
            Self::X86Sse42Crc32q => Some("crc32"),
            _ => None,
        }
    }
    pub fn is_x86_string(self) -> bool {
        self.x86_string_info().is_some()
    }
    /// Native opcode, immediate operand index, result kind (index0/mask1/flags2).
    pub fn x86_string_info(self) -> Option<(&'static str, usize, u8)> {
        match self {
            Self::X86AvxPcmpistriIndex => Some(("vpcmpistri", 2, 0)),
            Self::X86AvxPcmpistri64Index => Some(("vpcmpistri64", 2, 0)),
            Self::X86AvxPcmpistrmMask => Some(("vpcmpistrm", 2, 1)),
            Self::X86AvxPcmpistriFlags => Some(("vpcmpistri", 2, 2)),
            Self::X86AvxPcmpestriIndex => Some(("vpcmpestri", 4, 0)),
            Self::X86AvxPcmpestrmMask => Some(("vpcmpestrm", 4, 1)),
            Self::X86AvxPcmpestriFlags => Some(("vpcmpestri", 4, 2)),
            Self::X86AvxPcmpestri64Index => Some(("vpcmpestri64", 4, 0)),
            Self::X86AvxPcmpestrm64Mask => Some(("vpcmpestrm64", 4, 1)),
            Self::X86AvxPcmpestri64Flags => Some(("vpcmpestri64", 4, 2)),
            Self::X86Sse42PcmpistriIndex => Some(("pcmpistri", 2, 0)),
            Self::X86Sse42Pcmpistri64Index => Some(("pcmpistri64", 2, 0)),
            Self::X86Sse42PcmpistrmMask => Some(("pcmpistrm", 2, 1)),
            Self::X86Sse42PcmpistriFlags => Some(("pcmpistri", 2, 2)),
            Self::X86Sse42PcmpestriIndex => Some(("pcmpestri", 4, 0)),
            Self::X86Sse42PcmpestrmMask => Some(("pcmpestrm", 4, 1)),
            Self::X86Sse42PcmpestriFlags => Some(("pcmpestri", 4, 2)),
            Self::X86Sse42Pcmpestri64Index => Some(("pcmpestri64", 4, 0)),
            Self::X86Sse42Pcmpestrm64Mask => Some(("pcmpestrm64", 4, 1)),
            Self::X86Sse42Pcmpestri64Flags => Some(("pcmpestri64", 4, 2)),
            _ => None,
        }
    }
    pub fn is_x86_raw_fma(self) -> bool {
        self.x86_raw_fma_opcode().is_some()
    }
    /// Exact architectural opcode; no commuting, sign rewriting or order selection.
    pub fn x86_raw_fma_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86FmaVfmadd132pd128 => Some("vfmadd132pd"),
            Self::X86FmaVfmadd132pd256 => Some("vfmadd132pd"),
            Self::X86FmaVfmadd132ps128 => Some("vfmadd132ps"),
            Self::X86FmaVfmadd132ps256 => Some("vfmadd132ps"),
            Self::X86FmaVfmadd132sd128 => Some("vfmadd132sd"),
            Self::X86FmaVfmadd132ss128 => Some("vfmadd132ss"),
            Self::X86FmaVfmadd213pd128 => Some("vfmadd213pd"),
            Self::X86FmaVfmadd213pd256 => Some("vfmadd213pd"),
            Self::X86FmaVfmadd213ps128 => Some("vfmadd213ps"),
            Self::X86FmaVfmadd213ps256 => Some("vfmadd213ps"),
            Self::X86FmaVfmadd213sd128 => Some("vfmadd213sd"),
            Self::X86FmaVfmadd213ss128 => Some("vfmadd213ss"),
            Self::X86FmaVfmadd231pd128 => Some("vfmadd231pd"),
            Self::X86FmaVfmadd231pd256 => Some("vfmadd231pd"),
            Self::X86FmaVfmadd231ps128 => Some("vfmadd231ps"),
            Self::X86FmaVfmadd231ps256 => Some("vfmadd231ps"),
            Self::X86FmaVfmadd231sd128 => Some("vfmadd231sd"),
            Self::X86FmaVfmadd231ss128 => Some("vfmadd231ss"),
            Self::X86FmaVfmaddsub132pd128 => Some("vfmaddsub132pd"),
            Self::X86FmaVfmaddsub132pd256 => Some("vfmaddsub132pd"),
            Self::X86FmaVfmaddsub213pd128 => Some("vfmaddsub213pd"),
            Self::X86FmaVfmaddsub213pd256 => Some("vfmaddsub213pd"),
            Self::X86FmaVfmaddsub231pd128 => Some("vfmaddsub231pd"),
            Self::X86FmaVfmaddsub231pd256 => Some("vfmaddsub231pd"),
            Self::X86FmaVfmaddsub132ps128 => Some("vfmaddsub132ps"),
            Self::X86FmaVfmaddsub132ps256 => Some("vfmaddsub132ps"),
            Self::X86FmaVfmaddsub213ps128 => Some("vfmaddsub213ps"),
            Self::X86FmaVfmaddsub213ps256 => Some("vfmaddsub213ps"),
            Self::X86FmaVfmaddsub231ps128 => Some("vfmaddsub231ps"),
            Self::X86FmaVfmaddsub231ps256 => Some("vfmaddsub231ps"),
            Self::X86FmaVfmsubadd132pd128 => Some("vfmsubadd132pd"),
            Self::X86FmaVfmsubadd132pd256 => Some("vfmsubadd132pd"),
            Self::X86FmaVfmsubadd213pd128 => Some("vfmsubadd213pd"),
            Self::X86FmaVfmsubadd213pd256 => Some("vfmsubadd213pd"),
            Self::X86FmaVfmsubadd231pd128 => Some("vfmsubadd231pd"),
            Self::X86FmaVfmsubadd231pd256 => Some("vfmsubadd231pd"),
            Self::X86FmaVfmsubadd132ps128 => Some("vfmsubadd132ps"),
            Self::X86FmaVfmsubadd132ps256 => Some("vfmsubadd132ps"),
            Self::X86FmaVfmsubadd213ps128 => Some("vfmsubadd213ps"),
            Self::X86FmaVfmsubadd213ps256 => Some("vfmsubadd213ps"),
            Self::X86FmaVfmsubadd231ps128 => Some("vfmsubadd231ps"),
            Self::X86FmaVfmsubadd231ps256 => Some("vfmsubadd231ps"),
            Self::X86FmaVfmsub132pd128 => Some("vfmsub132pd"),
            Self::X86FmaVfmsub132pd256 => Some("vfmsub132pd"),
            Self::X86FmaVfmsub132ps128 => Some("vfmsub132ps"),
            Self::X86FmaVfmsub132ps256 => Some("vfmsub132ps"),
            Self::X86FmaVfmsub132sd128 => Some("vfmsub132sd"),
            Self::X86FmaVfmsub132ss128 => Some("vfmsub132ss"),
            Self::X86FmaVfmsub213pd128 => Some("vfmsub213pd"),
            Self::X86FmaVfmsub213pd256 => Some("vfmsub213pd"),
            Self::X86FmaVfmsub213ps128 => Some("vfmsub213ps"),
            Self::X86FmaVfmsub213ps256 => Some("vfmsub213ps"),
            Self::X86FmaVfmsub213sd128 => Some("vfmsub213sd"),
            Self::X86FmaVfmsub213ss128 => Some("vfmsub213ss"),
            Self::X86FmaVfmsub231pd128 => Some("vfmsub231pd"),
            Self::X86FmaVfmsub231pd256 => Some("vfmsub231pd"),
            Self::X86FmaVfmsub231ps128 => Some("vfmsub231ps"),
            Self::X86FmaVfmsub231ps256 => Some("vfmsub231ps"),
            Self::X86FmaVfmsub231sd128 => Some("vfmsub231sd"),
            Self::X86FmaVfmsub231ss128 => Some("vfmsub231ss"),
            Self::X86FmaVfnmadd132pd128 => Some("vfnmadd132pd"),
            Self::X86FmaVfnmadd132pd256 => Some("vfnmadd132pd"),
            Self::X86FmaVfnmadd132ps128 => Some("vfnmadd132ps"),
            Self::X86FmaVfnmadd132ps256 => Some("vfnmadd132ps"),
            Self::X86FmaVfnmadd132sd128 => Some("vfnmadd132sd"),
            Self::X86FmaVfnmadd132ss128 => Some("vfnmadd132ss"),
            Self::X86FmaVfnmadd213pd128 => Some("vfnmadd213pd"),
            Self::X86FmaVfnmadd213pd256 => Some("vfnmadd213pd"),
            Self::X86FmaVfnmadd213ps128 => Some("vfnmadd213ps"),
            Self::X86FmaVfnmadd213ps256 => Some("vfnmadd213ps"),
            Self::X86FmaVfnmadd213sd128 => Some("vfnmadd213sd"),
            Self::X86FmaVfnmadd213ss128 => Some("vfnmadd213ss"),
            Self::X86FmaVfnmadd231pd128 => Some("vfnmadd231pd"),
            Self::X86FmaVfnmadd231pd256 => Some("vfnmadd231pd"),
            Self::X86FmaVfnmadd231ps128 => Some("vfnmadd231ps"),
            Self::X86FmaVfnmadd231ps256 => Some("vfnmadd231ps"),
            Self::X86FmaVfnmadd231sd128 => Some("vfnmadd231sd"),
            Self::X86FmaVfnmadd231ss128 => Some("vfnmadd231ss"),
            Self::X86FmaVfnmsub132pd128 => Some("vfnmsub132pd"),
            Self::X86FmaVfnmsub132pd256 => Some("vfnmsub132pd"),
            Self::X86FmaVfnmsub132ps128 => Some("vfnmsub132ps"),
            Self::X86FmaVfnmsub132ps256 => Some("vfnmsub132ps"),
            Self::X86FmaVfnmsub132sd128 => Some("vfnmsub132sd"),
            Self::X86FmaVfnmsub132ss128 => Some("vfnmsub132ss"),
            Self::X86FmaVfnmsub213pd128 => Some("vfnmsub213pd"),
            Self::X86FmaVfnmsub213pd256 => Some("vfnmsub213pd"),
            Self::X86FmaVfnmsub213ps128 => Some("vfnmsub213ps"),
            Self::X86FmaVfnmsub213ps256 => Some("vfnmsub213ps"),
            Self::X86FmaVfnmsub213sd128 => Some("vfnmsub213sd"),
            Self::X86FmaVfnmsub213ss128 => Some("vfnmsub213ss"),
            Self::X86FmaVfnmsub231pd128 => Some("vfnmsub231pd"),
            Self::X86FmaVfnmsub231pd256 => Some("vfnmsub231pd"),
            Self::X86FmaVfnmsub231ps128 => Some("vfnmsub231ps"),
            Self::X86FmaVfnmsub231ps256 => Some("vfnmsub231ps"),
            Self::X86FmaVfnmsub231sd128 => Some("vfnmsub231sd"),
            Self::X86FmaVfnmsub231ss128 => Some("vfnmsub231ss"),
            _ => None,
        }
    }
    pub fn is_x86_fp_horizontal_dot(self) -> bool {
        self.x86_fp_horizontal_dot_opcode().is_some()
    }
    pub fn has_x86_dot_control(self) -> bool {
        matches!(
            self,
            Self::X86AvxVdpps128 | Self::X86AvxVdpps256 | Self::X86AvxVdppd128
        )
    }
    pub fn x86_fp_horizontal_dot_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVhaddps128 => Some("vhaddps"),
            Self::X86AvxVhaddps256 => Some("vhaddps"),
            Self::X86AvxVhaddpd128 => Some("vhaddpd"),
            Self::X86AvxVhaddpd256 => Some("vhaddpd"),
            Self::X86AvxVhsubps128 => Some("vhsubps"),
            Self::X86AvxVhsubps256 => Some("vhsubps"),
            Self::X86AvxVhsubpd128 => Some("vhsubpd"),
            Self::X86AvxVhsubpd256 => Some("vhsubpd"),
            Self::X86AvxVaddsubps128 => Some("vaddsubps"),
            Self::X86AvxVaddsubps256 => Some("vaddsubps"),
            Self::X86AvxVaddsubpd128 => Some("vaddsubpd"),
            Self::X86AvxVaddsubpd256 => Some("vaddsubpd"),
            Self::X86AvxVdpps128 => Some("vdpps"),
            Self::X86AvxVdpps256 => Some("vdpps"),
            Self::X86AvxVdppd128 => Some("vdppd"),
            _ => None,
        }
    }
    pub fn from_name(name: &str) -> Option<Intrinsic> {
        // Sorted by name, keeping `ALL` order among equal names, so the
        // first match is the one a linear scan of `ALL` would find.
        static BY_NAME: std::sync::OnceLock<Vec<Intrinsic>> = std::sync::OnceLock::new();
        let by_name = BY_NAME.get_or_init(|| {
            let mut all = Intrinsic::ALL.to_vec();
            all.sort_by_key(|i| i.info().name);
            all
        });
        let first = by_name.partition_point(|i| i.info().name < name);
        by_name
            .get(first)
            .copied()
            .filter(|i| i.info().name == name)
    }
}

impl fmt::Display for Intrinsic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.info().name)
    }
}

#[cfg(test)]
mod from_name_tests {
    use super::Intrinsic;

    #[test]
    fn matches_a_linear_scan() {
        for &intrinsic in Intrinsic::ALL.iter() {
            let name = intrinsic.info().name;
            let scan = Intrinsic::ALL
                .iter()
                .copied()
                .find(|i| i.info().name == name);
            assert_eq!(Intrinsic::from_name(name), scan);
        }
        assert_eq!(Intrinsic::from_name(""), None);
        assert_eq!(Intrinsic::from_name("no_such_intrinsic"), None);
    }
}
