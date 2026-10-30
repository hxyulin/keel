//! Raw packed VL FMA descriptors preserve encoded operand order.
use super::{Intrinsic, IntrinsicInfo, Type, VectorType};
fn descriptor(id: Intrinsic) -> Option<(&'static str, &'static str, VectorType)> {
    Some(match id {
        Intrinsic::X86Avx512VlVfmadd132ps128 => (
            "vfmadd132ps",
            "x86_avx512vl_vfmadd132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmadd132pd128 => (
            "vfmadd132pd",
            "x86_avx512vl_vfmadd132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmadd213ps128 => (
            "vfmadd213ps",
            "x86_avx512vl_vfmadd213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmadd213pd128 => (
            "vfmadd213pd",
            "x86_avx512vl_vfmadd213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmadd231ps128 => (
            "vfmadd231ps",
            "x86_avx512vl_vfmadd231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmadd231pd128 => (
            "vfmadd231pd",
            "x86_avx512vl_vfmadd231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsub132ps128 => (
            "vfmsub132ps",
            "x86_avx512vl_vfmsub132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsub132pd128 => (
            "vfmsub132pd",
            "x86_avx512vl_vfmsub132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsub213ps128 => (
            "vfmsub213ps",
            "x86_avx512vl_vfmsub213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsub213pd128 => (
            "vfmsub213pd",
            "x86_avx512vl_vfmsub213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsub231ps128 => (
            "vfmsub231ps",
            "x86_avx512vl_vfmsub231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsub231pd128 => (
            "vfmsub231pd",
            "x86_avx512vl_vfmsub231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmadd132ps128 => (
            "vfnmadd132ps",
            "x86_avx512vl_vfnmadd132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd132pd128 => (
            "vfnmadd132pd",
            "x86_avx512vl_vfnmadd132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmadd213ps128 => (
            "vfnmadd213ps",
            "x86_avx512vl_vfnmadd213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd213pd128 => (
            "vfnmadd213pd",
            "x86_avx512vl_vfnmadd213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmadd231ps128 => (
            "vfnmadd231ps",
            "x86_avx512vl_vfnmadd231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd231pd128 => (
            "vfnmadd231pd",
            "x86_avx512vl_vfnmadd231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmsub132ps128 => (
            "vfnmsub132ps",
            "x86_avx512vl_vfnmsub132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub132pd128 => (
            "vfnmsub132pd",
            "x86_avx512vl_vfnmsub132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmsub213ps128 => (
            "vfnmsub213ps",
            "x86_avx512vl_vfnmsub213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub213pd128 => (
            "vfnmsub213pd",
            "x86_avx512vl_vfnmsub213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfnmsub231ps128 => (
            "vfnmsub231ps",
            "x86_avx512vl_vfnmsub231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub231pd128 => (
            "vfnmsub231pd",
            "x86_avx512vl_vfnmsub231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmaddsub132ps128 => (
            "vfmaddsub132ps",
            "x86_avx512vl_vfmaddsub132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub132pd128 => (
            "vfmaddsub132pd",
            "x86_avx512vl_vfmaddsub132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmaddsub213ps128 => (
            "vfmaddsub213ps",
            "x86_avx512vl_vfmaddsub213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub213pd128 => (
            "vfmaddsub213pd",
            "x86_avx512vl_vfmaddsub213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmaddsub231ps128 => (
            "vfmaddsub231ps",
            "x86_avx512vl_vfmaddsub231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub231pd128 => (
            "vfmaddsub231pd",
            "x86_avx512vl_vfmaddsub231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsubadd132ps128 => (
            "vfmsubadd132ps",
            "x86_avx512vl_vfmsubadd132ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd132pd128 => (
            "vfmsubadd132pd",
            "x86_avx512vl_vfmsubadd132pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsubadd213ps128 => (
            "vfmsubadd213ps",
            "x86_avx512vl_vfmsubadd213ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd213pd128 => (
            "vfmsubadd213pd",
            "x86_avx512vl_vfmsubadd213pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmsubadd231ps128 => (
            "vfmsubadd231ps",
            "x86_avx512vl_vfmsubadd231ps128",
            VectorType::F32x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd231pd128 => (
            "vfmsubadd231pd",
            "x86_avx512vl_vfmsubadd231pd128",
            VectorType::F64x2,
        ),
        Intrinsic::X86Avx512VlVfmadd132ps256 => (
            "vfmadd132ps",
            "x86_avx512vl_vfmadd132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmadd132pd256 => (
            "vfmadd132pd",
            "x86_avx512vl_vfmadd132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmadd213ps256 => (
            "vfmadd213ps",
            "x86_avx512vl_vfmadd213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmadd213pd256 => (
            "vfmadd213pd",
            "x86_avx512vl_vfmadd213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmadd231ps256 => (
            "vfmadd231ps",
            "x86_avx512vl_vfmadd231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmadd231pd256 => (
            "vfmadd231pd",
            "x86_avx512vl_vfmadd231pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsub132ps256 => (
            "vfmsub132ps",
            "x86_avx512vl_vfmsub132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsub132pd256 => (
            "vfmsub132pd",
            "x86_avx512vl_vfmsub132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsub213ps256 => (
            "vfmsub213ps",
            "x86_avx512vl_vfmsub213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsub213pd256 => (
            "vfmsub213pd",
            "x86_avx512vl_vfmsub213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsub231ps256 => (
            "vfmsub231ps",
            "x86_avx512vl_vfmsub231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsub231pd256 => (
            "vfmsub231pd",
            "x86_avx512vl_vfmsub231pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd132ps256 => (
            "vfnmadd132ps",
            "x86_avx512vl_vfnmadd132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmadd132pd256 => (
            "vfnmadd132pd",
            "x86_avx512vl_vfnmadd132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd213ps256 => (
            "vfnmadd213ps",
            "x86_avx512vl_vfnmadd213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmadd213pd256 => (
            "vfnmadd213pd",
            "x86_avx512vl_vfnmadd213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmadd231ps256 => (
            "vfnmadd231ps",
            "x86_avx512vl_vfnmadd231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmadd231pd256 => (
            "vfnmadd231pd",
            "x86_avx512vl_vfnmadd231pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub132ps256 => (
            "vfnmsub132ps",
            "x86_avx512vl_vfnmsub132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmsub132pd256 => (
            "vfnmsub132pd",
            "x86_avx512vl_vfnmsub132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub213ps256 => (
            "vfnmsub213ps",
            "x86_avx512vl_vfnmsub213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmsub213pd256 => (
            "vfnmsub213pd",
            "x86_avx512vl_vfnmsub213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfnmsub231ps256 => (
            "vfnmsub231ps",
            "x86_avx512vl_vfnmsub231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfnmsub231pd256 => (
            "vfnmsub231pd",
            "x86_avx512vl_vfnmsub231pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub132ps256 => (
            "vfmaddsub132ps",
            "x86_avx512vl_vfmaddsub132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmaddsub132pd256 => (
            "vfmaddsub132pd",
            "x86_avx512vl_vfmaddsub132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub213ps256 => (
            "vfmaddsub213ps",
            "x86_avx512vl_vfmaddsub213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmaddsub213pd256 => (
            "vfmaddsub213pd",
            "x86_avx512vl_vfmaddsub213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmaddsub231ps256 => (
            "vfmaddsub231ps",
            "x86_avx512vl_vfmaddsub231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmaddsub231pd256 => (
            "vfmaddsub231pd",
            "x86_avx512vl_vfmaddsub231pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd132ps256 => (
            "vfmsubadd132ps",
            "x86_avx512vl_vfmsubadd132ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsubadd132pd256 => (
            "vfmsubadd132pd",
            "x86_avx512vl_vfmsubadd132pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd213ps256 => (
            "vfmsubadd213ps",
            "x86_avx512vl_vfmsubadd213ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsubadd213pd256 => (
            "vfmsubadd213pd",
            "x86_avx512vl_vfmsubadd213pd256",
            VectorType::F64x4,
        ),
        Intrinsic::X86Avx512VlVfmsubadd231ps256 => (
            "vfmsubadd231ps",
            "x86_avx512vl_vfmsubadd231ps256",
            VectorType::F32x8,
        ),
        Intrinsic::X86Avx512VlVfmsubadd231pd256 => (
            "vfmsubadd231pd",
            "x86_avx512vl_vfmsubadd231pd256",
            VectorType::F64x4,
        ),
        _ => return None,
    })
}
impl Intrinsic {
    /// Encoded FMA opcode and packed width, requiring AVX512F and AVX512VL.
    pub fn x86_avx512_vl_fma(self) -> Option<(&'static str, VectorType)> {
        descriptor(self).map(|(opcode, _, shape)| (opcode, shape))
    }
}
pub(super) fn info(id: Intrinsic) -> IntrinsicInfo {
    let (_, name, shape) = descriptor(id).expect("VL FMA descriptor");
    let (params, returns): (&'static [Type], &'static [Type]) = match shape {
        VectorType::F32x4 => (
            &[
                Type::Vector(VectorType::F32x4),
                Type::I8,
                Type::Vector(VectorType::F32x4),
                Type::Vector(VectorType::F32x4),
                Type::Vector(VectorType::F32x4),
            ],
            &[Type::Vector(VectorType::F32x4)],
        ),
        VectorType::F64x2 => (
            &[
                Type::Vector(VectorType::F64x2),
                Type::I8,
                Type::Vector(VectorType::F64x2),
                Type::Vector(VectorType::F64x2),
                Type::Vector(VectorType::F64x2),
            ],
            &[Type::Vector(VectorType::F64x2)],
        ),
        VectorType::F32x8 => (
            &[
                Type::Vector(VectorType::F32x8),
                Type::I8,
                Type::Vector(VectorType::F32x8),
                Type::Vector(VectorType::F32x8),
                Type::Vector(VectorType::F32x8),
            ],
            &[Type::Vector(VectorType::F32x8)],
        ),
        VectorType::F64x4 => (
            &[
                Type::Vector(VectorType::F64x4),
                Type::I8,
                Type::Vector(VectorType::F64x4),
                Type::Vector(VectorType::F64x4),
                Type::Vector(VectorType::F64x4),
            ],
            &[Type::Vector(VectorType::F64x4)],
        ),
        _ => unreachable!("VL FMA shape"),
    };
    IntrinsicInfo {
        name,
        params,
        returns,
        pure: false,
    }
}
