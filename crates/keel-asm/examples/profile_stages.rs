//! Independent IR parsing, AArch64 text generation, and assembly timings.
//! Usage: cargo run --release -p keel-asm --example profile_stages -- INPUT.keel ROUNDS
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let source = std::fs::read_to_string(args.get(1).ok_or("expected INPUT.keel ROUNDS")?)?;
    let rounds: usize = args.get(2).ok_or("expected ROUNDS")?.parse()?;
    if rounds == 0 {
        return Err("rounds must be positive".into());
    }
    let module = keel_ir::parse(&source).map_err(|e| e.to_string())?;
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    let text = keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin)?;
    let mut samples = [Vec::new(), Vec::new(), Vec::new()];
    for round in 0..rounds + 2 {
        for offset in 0..3 {
            let stage = (round + offset) % 3;
            let start = Instant::now();
            match stage {
                0 => {
                    black_box(keel_ir::parse(black_box(&source)).map_err(|e| e.to_string())?);
                }
                1 => {
                    black_box(keel_codegen::compile(
                        black_box(&module),
                        keel_codegen::Target::Aarch64AppleDarwin,
                    )?);
                }
                _ => {
                    black_box(keel_asm::assemble(black_box(&text))?);
                }
            }
            if round >= 2 {
                samples[stage].push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
    }
    println!(
        "{{\"parse_ms\":{:?},\"codegen_ms\":{:?},\"assemble_ms\":{:?},\"ir_bytes\":{},\"asm_bytes\":{}}}",
        samples[0],
        samples[1],
        samples[2],
        source.len(),
        text.len()
    );
    Ok(())
}
