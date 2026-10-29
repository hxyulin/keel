//! x86 packed integer, mask, shuffle, gather, masked memory and xstate intrinsics.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[allow(dead_code)]
#[path = "common/matrix.rs"]
mod matrix;
#[path = "x86_simd_intrinsics/x86_byte_shuffle.rs"]
mod x86_byte_shuffle;
#[path = "x86_simd_intrinsics/x86_gather.rs"]
mod x86_gather;
#[path = "x86_simd_intrinsics/x86_legacy_byte_mask.rs"]
mod x86_legacy_byte_mask;
#[path = "x86_simd_intrinsics/x86_legacy_ptest.rs"]
mod x86_legacy_ptest;
#[path = "x86_simd_intrinsics/x86_legacy_sign_mask.rs"]
mod x86_legacy_sign_mask;
#[path = "x86_simd_intrinsics/x86_masked_memory.rs"]
mod x86_masked_memory;
#[path = "x86_simd_intrinsics/x86_packed_integer.rs"]
mod x86_packed_integer;
#[path = "x86_simd_intrinsics/x86_packed_narrow.rs"]
mod x86_packed_narrow;
#[path = "x86_simd_intrinsics/x86_packed_pair.rs"]
mod x86_packed_pair;
#[path = "x86_simd_intrinsics/x86_packed_shift.rs"]
mod x86_packed_shift;
#[path = "x86_simd_intrinsics/x86_xstate.rs"]
mod x86_xstate;
#[path = "x86_simd_intrinsics/x86_zeroall.rs"]
mod x86_zeroall;
