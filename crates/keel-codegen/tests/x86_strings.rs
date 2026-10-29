//! SSE4.2 and AVX string compare intrinsics: controls, allocation and native results.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[allow(dead_code)]
#[path = "common/matrix.rs"]
mod matrix;
#[allow(dead_code)]
#[path = "common/string_allocation.rs"]
mod string_allocation;
#[path = "x86_strings/x86_avx_strings.rs"]
mod x86_avx_strings;
#[path = "x86_strings/x86_legacy_strings.rs"]
mod x86_legacy_strings;
