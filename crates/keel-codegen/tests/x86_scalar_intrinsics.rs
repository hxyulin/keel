//! x86 scalar extension intrinsics: AES-NI, PCLMUL, CRC32, BMI and CMPXCHG16B.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[path = "x86_scalar_intrinsics/x86_aes.rs"]
mod x86_aes;
#[path = "x86_scalar_intrinsics/x86_bmi_semantics.rs"]
mod x86_bmi_semantics;
#[path = "x86_scalar_intrinsics/x86_crc32.rs"]
mod x86_crc32;
#[path = "x86_scalar_intrinsics/x86_cx16.rs"]
mod x86_cx16;
#[path = "x86_scalar_intrinsics/x86_pclmul.rs"]
mod x86_pclmul;
