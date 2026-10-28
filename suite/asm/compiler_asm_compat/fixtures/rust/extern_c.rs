// extern "C" exports: scalars, loops, slices via (ptr, len), repr(C) enums and matches.
use std::hint::black_box;

#[repr(C)]
#[derive(Clone, Copy)]
pub enum Op {
    Add = 1,
    Mul = 2,
    Xor = 7,
}

#[no_mangle]
pub extern "C" fn keel_mix(a: u64, b: u32, c: i16, d: f64) -> i64 {
    (a.rotate_left(b & 63) as i64) ^ (c as i64 * 3) ^ (d * 1000.0) as i64
}

#[no_mangle]
pub extern "C" fn keel_apply(op: Op, a: i64, b: i64) -> i64 {
    match op {
        Op::Add => a.wrapping_add(b),
        Op::Mul => a.wrapping_mul(b),
        Op::Xor => a ^ b,
    }
}

#[no_mangle]
pub unsafe extern "C" fn keel_sum(ptr: *const i32, len: usize) -> i64 {
    let values = unsafe { std::slice::from_raw_parts(ptr, len) };
    values.iter().map(|&v| v as i64).sum()
}

#[no_mangle]
pub unsafe extern "C" fn keel_scale(ptr: *mut f32, len: usize, k: f32) {
    let values = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
    for v in values {
        *v = *v * k + 1.0;
    }
}

#[no_mangle]
pub extern "C" fn keel_collatz(mut n: u64) -> u32 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    let ints: [i32; 37] = std::array::from_fn(|i| (i as i32 * 7919) % 101 - 50);
    let mut floats = [0.5f32; 19];
    unsafe { keel_scale(black_box(floats.as_mut_ptr()), floats.len(), black_box(2.5)) };
    let mut acc = 0i64;
    for (i, op) in [Op::Add, Op::Mul, Op::Xor].into_iter().enumerate() {
        acc = keel_apply(black_box(op), acc, i as i64 + 3);
    }
    let sum = unsafe { keel_sum(black_box(ints.as_ptr()), ints.len()) };
    println!(
        "extern_c mix={} acc={} sum={} scaled={} collatz={}",
        keel_mix(black_box(0x1234_5678_9abc), 13, -7, 2.25),
        acc,
        sum,
        floats.iter().sum::<f32>(),
        keel_collatz(black_box(27))
    );
}
