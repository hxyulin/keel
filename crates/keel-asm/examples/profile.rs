//! In-process stage timings; build with --release. Input is read before timing.
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let source = std::fs::read_to_string(args.get(1).ok_or("expected input.s [iterations]")?)?;
    let iterations: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(20);
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let assembly = keel_asm::assemble(&source)?;
    black_box(assembly.to_macho()?);
    let mut parse = Vec::new();
    let mut write = Vec::new();
    for i in 0..iterations {
        // Alternate stage order to reduce systematic temperature/frequency bias.
        let mut measure_parse = || -> Result<(), keel_asm::Error> {
            let start = Instant::now();
            let result = keel_asm::assemble(black_box(&source))?;
            parse.push(start.elapsed().as_secs_f64() * 1000.0);
            drop(black_box(result));
            Ok(())
        };
        let mut measure_write = || -> Result<(), keel_asm::Error> {
            let start = Instant::now();
            let result = black_box(&assembly).to_macho()?;
            write.push(start.elapsed().as_secs_f64() * 1000.0);
            drop(black_box(result));
            Ok(())
        };
        if i % 2 == 0 {
            measure_parse()?;
            measure_write()?;
        } else {
            measure_write()?;
            measure_parse()?;
        }
    }
    println!(
        "{{\"assemble_ms\":{parse:?},\"macho_ms\":{write:?},\"text_bytes\":{},\"symbols\":{},\"relocations\":{}}}",
        assembly.text.len(),
        assembly.symbols.len(),
        assembly.relocations.len()
    );
    Ok(())
}
