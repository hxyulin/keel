#![no_std]
#[unsafe(no_mangle)]
pub extern "C" fn mix(a: u64, b: u64) -> u64 {
    a.wrapping_add(b).wrapping_mul(7) ^ (a >> 3)
}
#[unsafe(no_mangle)]
pub extern "C" fn choose(a: u64, b: u64) -> u64 {
    if a < b { a ^ b } else { a.wrapping_add(b) }
}
#[unsafe(no_mangle)]
pub extern "C" fn signed_div(a: i64) -> i64 { a / 3 }
#[unsafe(no_mangle)]
pub extern "C" fn signed_rem(a: i64) -> i64 { a % -5 }
#[unsafe(no_mangle)]
pub extern "C" fn narrow_sign(a: u8) -> i64 { (a as i8) as i64 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memory(p: *mut u64, a: u64) -> u64 {
    let old = unsafe { core::ptr::read_volatile(p) };
    unsafe { core::ptr::write_volatile(p, a) };
    old ^ a
}
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn helper(a: u64, b: u64) -> u64 { (a ^ b).wrapping_add(19) }
#[unsafe(no_mangle)]
pub extern "C" fn callhelper(a: u64, b: u64) -> u64 { helper(a,b) ^ a }
