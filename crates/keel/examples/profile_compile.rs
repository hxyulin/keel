//! In-process stage timings on preprocessed C. Build before collecting samples.
//! Usage: profile_compile ROUNDS none|full text|object INPUT.i ...
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let rounds: usize = args.get(1).ok_or("missing rounds")?.parse()?;
    if rounds < 3 {
        return Err("use at least three measured rounds".into());
    }
    let tier = match args.get(2).map(String::as_str) {
        Some("none") => keel_opt::OptimizationTier::None,
        Some("full") => keel_opt::OptimizationTier::Full,
        _ => return Err("expected none|full".into()),
    };
    let object = match args.get(3).map(String::as_str) {
        Some("text") => false,
        Some("object") => true,
        _ => return Err("expected text|object".into()),
    };
    let target = keel_ir::Target::host().ok_or("unsupported host")?;
    if object && target != keel_ir::Target::Aarch64AppleDarwin {
        return Err("direct object output currently requires AArch64 Mach-O".into());
    }
    if args.len() < 5 {
        return Err("provide at least one preprocessed C input".into());
    }
    for path in &args[4..] {
        let source = std::fs::read_to_string(path)?;
        let mut samples = Vec::new();
        for round in 0..rounds + 2 {
            let start = Instant::now();
            let mut module = keel_c::compile(
                black_box(&source),
                path,
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )?;
            let frontend = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
            let verify = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            keel_opt::optimize_with_tier(&mut module, tier).map_err(|e| e.to_string())?;
            let optimize = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let (text, program) = if object {
                (None, Some(keel_codegen::compile_program(&module, target)?))
            } else {
                (Some(keel_codegen::compile(&module, target)?), None)
            };
            let codegen = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let output = match &program {
                Some(p) => Some(keel_asm::assemble_program_object(
                    p,
                    keel_asm::TargetFeatures::default(),
                )?),
                None => None,
            };
            let assemble = start.elapsed().as_secs_f64() * 1000.;
            let output_bytes = output
                .as_ref()
                .map_or_else(|| text.as_ref().unwrap().len(), Vec::len);
            let start = Instant::now();
            drop(black_box(output));
            drop(black_box(text));
            drop(black_box(program));
            drop(black_box(module));
            let destroy = start.elapsed().as_secs_f64() * 1000.;
            if round >= 2 {
                samples.push([frontend, verify, optimize, codegen, assemble, destroy]);
            }
            black_box(output_bytes);
        }
        println!("{{\"input\":{path:?},\"stages_ms\":{samples:?}}}");
    }
    Ok(())
}
