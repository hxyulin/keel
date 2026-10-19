//! Copy into a frozen checkout's crates/keel/examples/fast_opt_probe.rs.
//! Run with the shared perf lock: fast_opt_probe REPS OUTDIR INPUT.keel|INPUT.i ...
//! Cloning, I/O and input parsing are excluded from tier timings. C frontend and
//! input verification are measured separately. Backend uses default selection
//! and Full allocation in every arm, with native unwind information retained.
use keel_opt::OptimizationTier::{self, Basic, Fast, Full, None as NoOpt};
use std::{hint::black_box, path::Path, time::Instant};

fn ns(start: Instant) -> u128 {
    start.elapsed().as_nanos()
}
fn name(tier: OptimizationTier) -> &'static str {
    match tier {
        NoOpt => "none",
        Fast => "fast",
        Basic => "basic",
        Full => "full",
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let reps: usize = args.get(1).ok_or("missing reps")?.parse()?;
    if reps < 3 {
        return Err("use at least three samples".into());
    }
    let out = Path::new(args.get(2).ok_or("missing output directory")?);
    std::fs::create_dir_all(out)?;
    let target = keel_ir::Target::Aarch64AppleDarwin;
    for (file, path) in args[3..].iter().enumerate() {
        let text = std::fs::read_to_string(path)?;
        let c = path.ends_with(".i") || path.ends_with(".c");
        let load = || -> Result<keel_ir::Module, Box<dyn std::error::Error>> {
            Ok(if c {
                keel_c::compile(
                    black_box(&text),
                    path,
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                )?
            } else {
                keel_ir::parse(black_box(&text)).map_err(|e| e.to_string())?
            })
        };
        let source = load()?;
        keel_ir::verify_module(&source).map_err(|e| format!("{e:?}"))?;
        for round in 0..reps + 2 {
            let start = Instant::now();
            let prepared = load()?;
            let frontend_ns = ns(start);
            let start = Instant::now();
            keel_ir::verify_module(&prepared).map_err(|e| format!("{e:?}"))?;
            let verify_ns = ns(start);
            for offset in 0..4 {
                let tier = [NoOpt, Fast, Basic, Full][(round + offset) % 4];
                let mut module = prepared.clone();
                let start_total = Instant::now();
                let start = Instant::now();
                keel_opt::optimize_with_tier(&mut module, tier).map_err(|e| e.to_string())?;
                let opt_ns = ns(start);
                let start = Instant::now();
                let program = keel_codegen::compile_program(&module, target)?;
                let codegen_ns = ns(start);
                let start = Instant::now();
                let object = keel_asm::assemble_program_object(
                    &program,
                    keel_asm::TargetFeatures::default(),
                )?;
                let assemble_ns = ns(start);
                let total_ns = ns(start_total);
                if round == 0 {
                    keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
                    let label = format!(
                        "{file}-{}.{}",
                        Path::new(path).file_stem().unwrap().to_str().unwrap(),
                        name(tier)
                    );
                    std::fs::write(out.join(format!("{label}.keel")), module.to_string())?;
                    std::fs::write(out.join(format!("{label}.o")), &object)?;
                }
                if round >= 2 {
                    let instructions = module
                        .functions
                        .iter()
                        .map(|f| f.layout_insts().count())
                        .sum::<usize>();
                    let blocks = module
                        .functions
                        .iter()
                        .map(|f| f.blocks().count())
                        .sum::<usize>();
                    println!(
                        "{{\"input\":{path:?},\"kind\":{:?},\"round\":{},\"tier\":{:?},\"frontend_ns\":{frontend_ns},\"verify_ns\":{verify_ns},\"opt_ns\":{opt_ns},\"codegen_ns\":{codegen_ns},\"assemble_ns\":{assemble_ns},\"opt_backend_ns\":{total_ns},\"instructions\":{instructions},\"blocks\":{blocks},\"object_bytes\":{}}}",
                        if c { "c" } else { "ir" },
                        round - 2,
                        name(tier),
                        object.len()
                    );
                }
                black_box(object);
                black_box(program);
                black_box(module);
            }
        }
    }
    Ok(())
}
