//! In-process stage timings on preprocessed C. Build before collecting samples.
//! Usage: profile_compile ROUNDS none|fast|basic|balanced|full|size text|object
//! [--isel-tier baseline|trees|full] [--regalloc fast|full] INPUT.i ...
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let rounds: usize = args.get(1).ok_or("missing rounds")?.parse()?;
    if rounds < 3 {
        return Err("use at least three measured rounds".into());
    }
    let tier = match args.get(2).map(String::as_str) {
        Some("none") => keel_opt::OptimizationTier::None,
        Some("fast") => keel_opt::OptimizationTier::Fast,
        Some("basic") => keel_opt::OptimizationTier::Basic,
        Some("balanced") => keel_opt::OptimizationTier::Balanced,
        Some("full") => keel_opt::OptimizationTier::Full,
        Some("size") => keel_opt::OptimizationTier::Size,
        _ => return Err("expected none|fast|basic|balanced|full|size".into()),
    };
    let object = match args.get(3).map(String::as_str) {
        Some("text") => false,
        Some("object") => true,
        _ => return Err("expected text|object".into()),
    };
    let target = keel_ir::Target::host().ok_or("unsupported host")?;
    let mut selection = keel_codegen::SelectionTier::Full;
    let mut allocation = keel_codegen::regalloc::AllocationTier::Full;
    let mut inputs = Vec::new();
    let mut remaining = args[4..].iter();
    while let Some(arg) = remaining.next() {
        match arg.as_str() {
            "--isel-tier" => {
                selection = match remaining.next().map(String::as_str) {
                    Some("baseline") => keel_codegen::SelectionTier::Baseline,
                    Some("trees") => keel_codegen::SelectionTier::Trees,
                    Some("full") => keel_codegen::SelectionTier::Full,
                    _ => return Err("expected baseline|trees|full".into()),
                }
            }
            "--regalloc" => {
                allocation = match remaining.next().map(String::as_str) {
                    Some("fast") => keel_codegen::regalloc::AllocationTier::Fast,
                    Some("full") => keel_codegen::regalloc::AllocationTier::Full,
                    _ => return Err("expected fast|full".into()),
                }
            }
            _ if arg.starts_with('-') => return Err(format!("unknown flag {arg}").into()),
            _ => inputs.push(arg),
        }
    }
    if inputs.is_empty() {
        return Err("provide at least one preprocessed C input".into());
    }
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let vector_target = match target {
        keel_ir::Target::X86_64LinuxGnu => keel_opt::VectorTarget::X86Sse2,
        keel_ir::Target::Aarch64AppleDarwin | keel_ir::Target::Aarch64LinuxGnu => {
            keel_opt::VectorTarget::Aarch64Neon
        }
        _ => return Err("unsupported native target".into()),
    };
    for path in inputs {
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
            keel_opt::run_passes_for_target(&mut module, tier.pipeline(), vector_target)
                .map_err(|e| e.to_string())?;
            let optimize = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let (text, program) = if object {
                (
                    None,
                    Some(keel_codegen::compile_program_with_pipeline(
                        &module, target, &pipeline,
                    )?),
                )
            } else {
                (
                    Some(keel_codegen::compile_with_pipeline(
                        &module, target, &pipeline,
                    )?),
                    None,
                )
            };
            let codegen = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let output = match &program {
                Some(p) => Some(match target {
                    keel_ir::Target::Aarch64AppleDarwin => {
                        keel_asm::assemble_program_object(p, Default::default())?
                    }
                    keel_ir::Target::Aarch64LinuxGnu => {
                        keel_asm::assemble_elf_program_object(p, Default::default())?
                    }
                    keel_ir::Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(p)?,
                    _ => unreachable!("validated native target"),
                }),
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
        println!(
            "{{\"input\":{path:?},\"optimization\":\"{tier:?}\",\"selection\":\"{selection:?}\",\"allocation\":\"{allocation:?}\",\"stage_order\":[\"frontend\",\"verify\",\"optimize\",\"codegen\",\"assemble\",\"destroy\"],\"stages_ms\":{samples:?}}}"
        );
    }
    Ok(())
}
