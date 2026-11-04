//! Packed VL floating special descriptors have no separate ER or SAE control.
use super::{Intrinsic, IntrinsicInfo, Type, VectorType};
use keel_asm_ir::x86::EvexMnemonic;
fn descriptor(id: Intrinsic) -> Option<(&'static str, EvexMnemonic, VectorType, Option<usize>)> {
    Some(match id {
        Intrinsic::X86Avx512VlGetexpps128 => (
            "x86_avx512vl_getexpps128",
            EvexMnemonic::Vgetexpps,
            VectorType::F32x4,
            None,
        ),
        Intrinsic::X86Avx512VlGetexppd128 => (
            "x86_avx512vl_getexppd128",
            EvexMnemonic::Vgetexppd,
            VectorType::F64x2,
            None,
        ),
        Intrinsic::X86Avx512VlGetmantps128 => (
            "x86_avx512vl_getmantps128",
            EvexMnemonic::Vgetmantps,
            VectorType::F32x4,
            Some(3),
        ),
        Intrinsic::X86Avx512VlGetmantpd128 => (
            "x86_avx512vl_getmantpd128",
            EvexMnemonic::Vgetmantpd,
            VectorType::F64x2,
            Some(3),
        ),
        Intrinsic::X86Avx512VlScalefps128 => (
            "x86_avx512vl_scalefps128",
            EvexMnemonic::Vscalefps,
            VectorType::F32x4,
            None,
        ),
        Intrinsic::X86Avx512VlScalefpd128 => (
            "x86_avx512vl_scalefpd128",
            EvexMnemonic::Vscalefpd,
            VectorType::F64x2,
            None,
        ),
        Intrinsic::X86Avx512VlRcp14ps128 => (
            "x86_avx512vl_rcp14ps128",
            EvexMnemonic::Vrcp14ps,
            VectorType::F32x4,
            None,
        ),
        Intrinsic::X86Avx512VlRcp14pd128 => (
            "x86_avx512vl_rcp14pd128",
            EvexMnemonic::Vrcp14pd,
            VectorType::F64x2,
            None,
        ),
        Intrinsic::X86Avx512VlRsqrt14ps128 => (
            "x86_avx512vl_rsqrt14ps128",
            EvexMnemonic::Vrsqrt14ps,
            VectorType::F32x4,
            None,
        ),
        Intrinsic::X86Avx512VlRsqrt14pd128 => (
            "x86_avx512vl_rsqrt14pd128",
            EvexMnemonic::Vrsqrt14pd,
            VectorType::F64x2,
            None,
        ),
        Intrinsic::X86Avx512VlRoundscaleps128 => (
            "x86_avx512vl_roundscaleps128",
            EvexMnemonic::Vrndscaleps,
            VectorType::F32x4,
            Some(3),
        ),
        Intrinsic::X86Avx512VlRoundscalepd128 => (
            "x86_avx512vl_roundscalepd128",
            EvexMnemonic::Vrndscalepd,
            VectorType::F64x2,
            Some(3),
        ),
        Intrinsic::X86Avx512VlFixupimmps128 => (
            "x86_avx512vl_fixupimmps128",
            EvexMnemonic::Vfixupimmps,
            VectorType::F32x4,
            Some(5),
        ),
        Intrinsic::X86Avx512VlFixupimmpd128 => (
            "x86_avx512vl_fixupimmpd128",
            EvexMnemonic::Vfixupimmpd,
            VectorType::F64x2,
            Some(5),
        ),
        Intrinsic::X86Avx512VlGetexpps256 => (
            "x86_avx512vl_getexpps256",
            EvexMnemonic::Vgetexpps,
            VectorType::F32x8,
            None,
        ),
        Intrinsic::X86Avx512VlGetexppd256 => (
            "x86_avx512vl_getexppd256",
            EvexMnemonic::Vgetexppd,
            VectorType::F64x4,
            None,
        ),
        Intrinsic::X86Avx512VlGetmantps256 => (
            "x86_avx512vl_getmantps256",
            EvexMnemonic::Vgetmantps,
            VectorType::F32x8,
            Some(3),
        ),
        Intrinsic::X86Avx512VlGetmantpd256 => (
            "x86_avx512vl_getmantpd256",
            EvexMnemonic::Vgetmantpd,
            VectorType::F64x4,
            Some(3),
        ),
        Intrinsic::X86Avx512VlScalefps256 => (
            "x86_avx512vl_scalefps256",
            EvexMnemonic::Vscalefps,
            VectorType::F32x8,
            None,
        ),
        Intrinsic::X86Avx512VlScalefpd256 => (
            "x86_avx512vl_scalefpd256",
            EvexMnemonic::Vscalefpd,
            VectorType::F64x4,
            None,
        ),
        Intrinsic::X86Avx512VlRcp14ps256 => (
            "x86_avx512vl_rcp14ps256",
            EvexMnemonic::Vrcp14ps,
            VectorType::F32x8,
            None,
        ),
        Intrinsic::X86Avx512VlRcp14pd256 => (
            "x86_avx512vl_rcp14pd256",
            EvexMnemonic::Vrcp14pd,
            VectorType::F64x4,
            None,
        ),
        Intrinsic::X86Avx512VlRsqrt14ps256 => (
            "x86_avx512vl_rsqrt14ps256",
            EvexMnemonic::Vrsqrt14ps,
            VectorType::F32x8,
            None,
        ),
        Intrinsic::X86Avx512VlRsqrt14pd256 => (
            "x86_avx512vl_rsqrt14pd256",
            EvexMnemonic::Vrsqrt14pd,
            VectorType::F64x4,
            None,
        ),
        Intrinsic::X86Avx512VlRoundscaleps256 => (
            "x86_avx512vl_roundscaleps256",
            EvexMnemonic::Vrndscaleps,
            VectorType::F32x8,
            Some(3),
        ),
        Intrinsic::X86Avx512VlRoundscalepd256 => (
            "x86_avx512vl_roundscalepd256",
            EvexMnemonic::Vrndscalepd,
            VectorType::F64x4,
            Some(3),
        ),
        Intrinsic::X86Avx512VlFixupimmps256 => (
            "x86_avx512vl_fixupimmps256",
            EvexMnemonic::Vfixupimmps,
            VectorType::F32x8,
            Some(5),
        ),
        Intrinsic::X86Avx512VlFixupimmpd256 => (
            "x86_avx512vl_fixupimmpd256",
            EvexMnemonic::Vfixupimmpd,
            VectorType::F64x4,
            Some(5),
        ),
        _ => return None,
    })
}
impl Intrinsic {
    /// Opcode, shape and immediate index for packed AVX512F plus AVX512VL specials.
    pub fn x86_avx512_vl_special(self) -> Option<(EvexMnemonic, VectorType, Option<usize>)> {
        descriptor(self).map(|(_, opcode, shape, immediate)| (opcode, shape, immediate))
    }
}
pub(super) fn info(id: Intrinsic) -> IntrinsicInfo {
    let (name, opcode, shape, _) = descriptor(id).expect("VL special descriptor");
    let (params, returns): (&'static [Type], &'static [Type]) = match shape {
        VectorType::F32x4 => (
            match opcode {
                EvexMnemonic::Vgetmantps
                | EvexMnemonic::Vgetmantpd
                | EvexMnemonic::Vrndscaleps
                | EvexMnemonic::Vrndscalepd => &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::I32,
                ],
                EvexMnemonic::Vscalefps | EvexMnemonic::Vscalefpd => &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                ],
                EvexMnemonic::Vfixupimmps | EvexMnemonic::Vfixupimmpd => &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::F32x4),
                    Type::Vector(VectorType::I32x4),
                    Type::I32,
                ],
                _ => &[
                    Type::Vector(VectorType::F32x4),
                    Type::I8,
                    Type::Vector(VectorType::F32x4),
                ],
            },
            &[Type::Vector(VectorType::F32x4)],
        ),
        VectorType::F64x2 => (
            match opcode {
                EvexMnemonic::Vgetmantps
                | EvexMnemonic::Vgetmantpd
                | EvexMnemonic::Vrndscaleps
                | EvexMnemonic::Vrndscalepd => &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::I32,
                ],
                EvexMnemonic::Vscalefps | EvexMnemonic::Vscalefpd => &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                ],
                EvexMnemonic::Vfixupimmps | EvexMnemonic::Vfixupimmpd => &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::F64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                ],
                _ => &[
                    Type::Vector(VectorType::F64x2),
                    Type::I8,
                    Type::Vector(VectorType::F64x2),
                ],
            },
            &[Type::Vector(VectorType::F64x2)],
        ),
        VectorType::F32x8 => (
            match opcode {
                EvexMnemonic::Vgetmantps
                | EvexMnemonic::Vgetmantpd
                | EvexMnemonic::Vrndscaleps
                | EvexMnemonic::Vrndscalepd => &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::I32,
                ],
                EvexMnemonic::Vscalefps | EvexMnemonic::Vscalefpd => &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                ],
                EvexMnemonic::Vfixupimmps | EvexMnemonic::Vfixupimmpd => &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::F32x8),
                    Type::Vector(VectorType::I32x8),
                    Type::I32,
                ],
                _ => &[
                    Type::Vector(VectorType::F32x8),
                    Type::I8,
                    Type::Vector(VectorType::F32x8),
                ],
            },
            &[Type::Vector(VectorType::F32x8)],
        ),
        VectorType::F64x4 => (
            match opcode {
                EvexMnemonic::Vgetmantps
                | EvexMnemonic::Vgetmantpd
                | EvexMnemonic::Vrndscaleps
                | EvexMnemonic::Vrndscalepd => &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::I32,
                ],
                EvexMnemonic::Vscalefps | EvexMnemonic::Vscalefpd => &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                ],
                EvexMnemonic::Vfixupimmps | EvexMnemonic::Vfixupimmpd => &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::F64x4),
                    Type::Vector(VectorType::I64x4),
                    Type::I32,
                ],
                _ => &[
                    Type::Vector(VectorType::F64x4),
                    Type::I8,
                    Type::Vector(VectorType::F64x4),
                ],
            },
            &[Type::Vector(VectorType::F64x4)],
        ),
        _ => unreachable!("VL floating special shape"),
    };
    IntrinsicInfo {
        name,
        params,
        returns,
        pure: false,
    }
}
