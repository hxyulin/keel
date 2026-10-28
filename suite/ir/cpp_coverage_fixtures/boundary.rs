#![no_std]
// Optional independent compiler boundary: same ABI and wrapping operations as C.
unsafe extern "C" {
    fn cpp_case(a: u64, b: u64, p: *mut u64) -> u64;
}
#[unsafe(no_mangle)]
pub extern "C" fn boundary(a: u64) -> u64 {
    (a ^ 0x123456789abcdef0).wrapping_add(17)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn through_c(a: u64, b: u64, p: *mut u64) -> u64 {
    unsafe { cpp_case(a, b, p) }
}
