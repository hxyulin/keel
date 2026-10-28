#[unsafe(no_mangle)]
pub unsafe extern "C" fn compiler_cfg(mut x: u64, p: *mut u64) -> u64 {
    unsafe {
        if x & 1 != 0 {
            p.add(2).write_volatile(x);
            x ^= p.read_volatile();
        } else {
            p.add(1).write_volatile(x.wrapping_add(p.add(3).read_volatile()));
            x = x.wrapping_add(p.add(1).read_volatile());
        }
        x ^ p.add(2).read_volatile()
    }
}
