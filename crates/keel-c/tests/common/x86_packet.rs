//! Reused SSE–AVX2 fixture emission and native execution boundary.
use std::{fs, path::PathBuf, process::Command};

use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::Target;

pub fn frontend(source: &str) -> keel_ir::Module {
    let target = Target::X86_64LinuxGnu;
    let preprocessed = keel_c::preprocess(
        source,
        "construct.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &preprocessed,
        "construct.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

pub fn check(
    source: &str,
    runner: &str,
    export_env: &str,
    name: &str,
    reference_flags: Option<&[&str]>,
) {
    let directory = std::env::var_os(export_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-{name}-{}", std::process::id()))
        });
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("kernels.c"), source).unwrap();
    fs::write(directory.join("runner.c"), runner).unwrap();
    let module = frontend(source);
    let features = Features::for_level(CpuLevel::V3);
    let mut optimized = module.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    if !native_supported() {
        eprintln!("native {name} execution skipped: requires x86-64 Linux with AVX2");
    }
    let runner_object = directory.join("runner.o");
    if native_supported() {
        let output = Command::new("clang")
            .args(["-O2", "-c"])
            .arg(directory.join("runner.c"))
            .arg("-o")
            .arg(&runner_object)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let reference = if let Some(flags) = reference_flags.filter(|_| native_supported()) {
        let path = directory.join("reference.o");
        let output = Command::new("clang")
            .arg("-O2")
            .args(flags)
            .arg("-c")
            .arg(directory.join("kernels.c"))
            .arg("-o")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Some(path)
    } else {
        None
    };
    for (name, module, selection, spilled) in [
        ("baseline", &module, SelectionTier::Baseline, false),
        ("trees", &module, SelectionTier::Trees, false),
        ("optimized-spill", &optimized, SelectionTier::Full, true),
    ] {
        let selector = FeatureSelector {
            inner: &selection,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let empty = Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let pools = if spilled {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let bytes =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            bytes,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
            "{name}"
        );
        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
            module,
            pools,
            &pipeline,
            &mut |chunk| chunks.push(chunk),
        )
        .unwrap();
        assert_eq!(bytes, chunks.finish().unwrap(), "{name}");
        let object = directory.join(format!("{name}.o"));
        fs::write(&object, bytes).unwrap();
        if native_supported() {
            let executable = directory.join("run");
            let mut command = Command::new("clang");
            command.arg(&runner_object).arg(&object);
            if let Some(reference) = &reference {
                command.arg(reference);
            }
            let output = command.arg("-o").arg(&executable).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(executable).output().unwrap();
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn native_supported() -> bool {
    std::is_x86_feature_detected!("avx2")
}
#[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
fn native_supported() -> bool {
    false
}
