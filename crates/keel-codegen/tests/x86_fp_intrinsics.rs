//! x86 floating-point intrinsics: compares, rounding, conversions, dot products, FMA and F16C.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[allow(dead_code)]
#[path = "common/matrix.rs"]
mod matrix;
#[path = "x86_fp_intrinsics/x86_avx_horizontal_dot.rs"]
mod x86_avx_horizontal_dot;
#[path = "x86_fp_intrinsics/x86_f16c.rs"]
mod x86_f16c;
#[path = "x86_fp_intrinsics/x86_fma.rs"]
mod x86_fma;
#[path = "x86_fp_intrinsics/x86_fp_compare_controls.rs"]
mod x86_fp_compare_controls;
#[path = "x86_fp_intrinsics/x86_fp_misc.rs"]
mod x86_fp_misc;
#[path = "x86_fp_intrinsics/x86_legacy_fp_compare.rs"]
mod x86_legacy_fp_compare;
#[path = "x86_fp_intrinsics/x86_legacy_fp_misc.rs"]
mod x86_legacy_fp_misc;
#[path = "x86_fp_intrinsics/x86_legacy_raw_fp.rs"]
mod x86_legacy_raw_fp;
#[path = "x86_fp_intrinsics/x86_legacy_v2_fp.rs"]
mod x86_legacy_v2_fp;
#[path = "x86_fp_intrinsics/x86_raw_fp_conversions.rs"]
mod x86_raw_fp_conversions;
