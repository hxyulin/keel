//! Copy to crates/keel/examples/width_comparison.rs in a frozen snapshot.
//! TARGET PROFILE WIDTH INPUT OUTPUT_PREFIX. No textual IR is reparsed.
use keel_codegen::{
    CodegenPipeline, SelectionOptions, Target,
    regalloc::AllocationTier,
    tuning::{TunedSelector, TuningCpu},
    x86_features::{CpuLevel, FeatureSelector, Features, vector_capabilities},
};
use std::{fs, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() == 6 && a[3] == "pair" {
        // Two frontend invocations in one library process, with no IR round trip.
        for width in ["128", "256"] {
            let mut arm = a.clone();
            arm[3] = width.into();
            arm[5] = format!("{}-{width}", a[5]);
            compile(&arm)?;
        }
        return Ok(());
    }
    compile(&a)
}
fn compile(a: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if a.len() != 6 {
        return Err("TARGET PROFILE 128|256 INPUT OUTPUT_PREFIX".into());
    }
    let target = match a[1].as_str() {
        "x86_64-unknown-linux-gnu" => Target::X86_64LinuxGnu,
        "aarch64-apple-darwin" => Target::Aarch64AppleDarwin,
        _ => return Err("unsupported target".into()),
    };
    let features = match a[2].as_str() {
        "v1" => Features::for_level(CpuLevel::V1),
        "v2" => Features::for_level(CpuLevel::V2),
        "v3" => Features::for_level(CpuLevel::V3),
        "avx" => Features::for_level(CpuLevel::V3).with_features("-avx2")?,
        "arm" => Features::default(),
        _ => return Err("unsupported profile".into()),
    };
    let wide = match a[3].as_str() {
        "128" => false,
        "256" => true,
        _ => return Err("invalid width".into()),
    };
    let start = Instant::now();
    let source = fs::read_to_string(&a[4])?;
    let pre = keel_c::preprocess(
        &source,
        &a[4],
        &keel_c::PreprocessOptions {
            include_dirs: vec![],
            system_include_dirs: vec![],
            sysroot: None,
            darwin_deployment: None,
            defines: vec![],
            target,
            dump_macros: false,
        },
    )?;
    let mut module = keel_c::compile(
        &pre,
        &a[4],
        keel_c::Options {
            debug_info: false,
            target,
        },
    )?;
    keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
    let selector = TunedSelector {
        options: SelectionOptions::ALL,
        cpu: TuningCpu::Generic,
    };
    let selector = FeatureSelector {
        inner: &selector,
        features,
    };
    let context = keel_opt::OptimizationContext {
        target: if target == Target::X86_64LinuxGnu {
            keel_opt::VectorTarget::X86Sse2
        } else {
            keel_opt::VectorTarget::Aarch64Neon
        },
        capabilities: vector_capabilities(target, &selector)?.with_wide_preference(wide),
    };
    keel_opt::run_passes_with_context_and_options(
        &mut module,
        keel_opt::DEFAULT_PIPELINE,
        context,
        keel_opt::OptimizeOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let mut x86 = keel_asm::x86::ProgramAssembler::new(features);
    let mut arm = keel_asm::ProgramAssembler::new(keel_asm::TargetFeatures::default());
    keel_codegen::compile_program_chunks_with_pipeline(&module, target, &pipeline, &mut |p| {
        if target == Target::X86_64LinuxGnu {
            x86.push(p)
        } else {
            arm.push(p)
        }
    })?;
    let object = if target == Target::X86_64LinuxGnu {
        x86.finish()?
    } else {
        arm.finish()?
    };
    let elapsed = start.elapsed().as_nanos();
    // Retained captures and final verification are outside compile timing.
    keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
    fs::write(format!("{}.o", a[5]), object)?;
    fs::write(format!("{}.ir", a[5]), module.to_string())?;
    fs::write(
        format!("{}.s", a[5]),
        keel_codegen::compile_with_pipeline(&module, target, &pipeline)?,
    )?;
    println!("{{\"compile_ns\":{elapsed}}}");
    Ok(())
}
