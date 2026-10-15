//! Stage timings for native x86 ELF assembly, excluding input reads.
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let source = std::fs::read_to_string(args.get(1).ok_or("expected input.s [iterations]")?)?;
    let iterations: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(5);
    for _ in 0..iterations {
        let start = Instant::now();
        let (object, p) = keel_asm::x86::profile_object(black_box(&source))?;
        println!(
            "{{\"total_ms\":{},\"parse_ms\":{},\"layout_ms\":{:?},\"branch_check_ms\":{:?},\"discard_ms\":{:?},\"widened\":{:?},\"object_ms\":{},\"items\":{},\"object_bytes\":{}}}",
            start.elapsed().as_secs_f64() * 1000.0,
            p.parse_ms,
            p.layout_ms,
            p.branch_check_ms,
            p.discard_ms,
            p.widened,
            p.object_ms,
            p.items,
            object.len()
        );
        black_box(object);
    }
    Ok(())
}
