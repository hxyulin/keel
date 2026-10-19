//! Shared helpers: build objects with each backend, link with Clang, run.
#![allow(dead_code)]
use keel_cranelift::{OptLevel, Options, TargetProfile};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const CONFIGS: [&str; 4] = ["keel-baseline", "keel-full", "clif-none", "clif-speed"];

/// Executable tests need the Apple arm64 host and a Clang to link with.
pub fn host_ok() -> bool {
    let ok = cfg!(all(target_os = "macos", target_arch = "aarch64"))
        && Command::new("clang").arg("--version").output().is_ok();
    if !ok {
        eprintln!("skipping: executable differential tests need macOS arm64 and clang");
    }
    ok
}

pub fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("keel-cranelift-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

pub fn parse(path: &Path) -> keel_ir::Module {
    let text = std::fs::read_to_string(path).unwrap();
    let module = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

/// The native backend exactly as `keel compile --emit-object` composes it,
/// without running any IR passes.
pub fn native(module: &keel_ir::Module, full: bool) -> Vec<u8> {
    let selector = keel_codegen::tuning::TunedSelector {
        options: if full {
            keel_codegen::SelectionOptions::ALL
        } else {
            keel_codegen::SelectionOptions::default()
        },
        cpu: keel_codegen::tuning::TuningCpu::Generic,
    };
    let features = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features: keel_asm::x86::Features::default(),
    };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &features,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline(
        module,
        keel_codegen::Target::Aarch64AppleDarwin,
        &pipeline,
    )
    .unwrap();
    keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap()
}

pub fn clif(module: &keel_ir::Module, opt: OptLevel) -> Vec<u8> {
    keel_cranelift::compile_object(
        module,
        &Options {
            target: TargetProfile::AppleArm64,
            opt,
            verify_clif: true,
        },
    )
    .unwrap()
}

pub fn object(module: &keel_ir::Module, config: &str) -> Vec<u8> {
    match config {
        "keel-baseline" => native(module, false),
        "keel-full" => native(module, true),
        "clif-none" => clif(module, OptLevel::None),
        "clif-speed" => clif(module, OptLevel::Speed),
        _ => unreachable!(),
    }
}

pub fn run(cmd: &mut Command) -> Output {
    let out = cmd.output().unwrap();
    if !out.status.success() {
        panic!(
            "{cmd:?} failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    out
}

/// Link `objects` with a C driver compiled by Clang and return the program.
pub fn link(dir: &Path, name: &str, driver: &Path, cflags: &[&str], objects: &[&Path]) -> PathBuf {
    let exe = dir.join(name);
    run(Command::new("clang")
        .args(cflags)
        .arg(driver)
        .args(objects)
        .arg("-o")
        .arg(&exe));
    exe
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}
