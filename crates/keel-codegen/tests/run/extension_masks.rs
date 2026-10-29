//! Execute the mask oracle across IR tiers, selectors, allocators and spills.
use keel_codegen::{CodegenPipeline, Pools, SelectionTier, regalloc::AllocationTier};

#[cfg(target_arch = "aarch64")]
use keel_codegen::aarch64::{REGISTERS, compile_with_pipeline};
#[cfg(target_arch = "x86_64")]
use keel_codegen::x86_64::{REGISTERS, compile_with_pipeline};

#[cfg(target_arch = "aarch64")]
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};
#[cfg(target_arch = "x86_64")]
const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3, 12],
    float_caller_saved: &[8, 9],
    float_callee_saved: &[],
};

#[test]
fn native_extension_masks_match_unsigned_oracle() {
    use std::{fs, path::Path, process::Command};
    let source = include_str!("../programs/extension_masks.keel");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("extension-masks");
    fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.o");
    let asm = dir.join("test.s");
    let exe = dir.join("run");
    fs::write(
        dir.join("driver.c"),
        include_str!("../programs/extension_masks.c"),
    )
    .unwrap();
    let output = Command::new("clang")
        .arg("-c")
        .arg(dir.join("driver.c"))
        .arg("-o")
        .arg(&driver)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut runs = 0;
    for (name, passes) in [
        ("fast", keel_opt::OptimizationTier::Fast.pipeline()),
        ("full", keel_opt::OptimizationTier::Full.pipeline()),
        ("combine", &["instcombine", "dce", "gcm"][..]),
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_opt::run_passes(&mut module, passes).unwrap();
        keel_ir::verify_module(&module).unwrap();
        // Every pairing of selector, allocator and pool appears once.
        for (selector, allocator, pools) in [
            (SelectionTier::Baseline, AllocationTier::Full, &REGISTERS),
            (SelectionTier::Baseline, AllocationTier::Fast, &TINY),
            (SelectionTier::Full, AllocationTier::Full, &TINY),
            (SelectionTier::Full, AllocationTier::Fast, &REGISTERS),
        ] {
            let assembly = compile_with_pipeline(
                &module,
                pools,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                },
            )
            .unwrap();
            fs::write(&asm, assembly).unwrap();
            let output = Command::new("clang")
                .args([&asm, &driver])
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&exe).output().unwrap();
            assert!(
                output.status.success(),
                "{name}, {selector:?}, {allocator:?}, pools={pools:?}: {output:?}"
            );
            runs += 1;
        }
    }
    assert_eq!(runs, 12);
}
