//! Cross-target assembly coverage; execution lives in the native codegen matrix.
use keel_codegen::{SelectionTier, Target, regalloc::AllocationTier};
use std::path::Path;

#[test]
fn every_x86_backend_fixture_assembles() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let mut count = 0;
    for path in std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
    {
        let source = std::fs::read_to_string(&path).unwrap();
        if source
            .lines()
            .find_map(|s| s.strip_prefix("; target: "))
            .is_some_and(|t| t != Target::X86_64LinuxGnu.triple())
        {
            continue;
        }
        for (opt, selection, allocation) in [
            (false, SelectionTier::Baseline, AllocationTier::Full),
            (true, SelectionTier::Full, AllocationTier::Full),
            (true, SelectionTier::Full, AllocationTier::Fast),
        ] {
            let mut module = keel_ir::parse(&source).unwrap();
            if opt {
                keel_opt::optimize(&mut module).unwrap()
            }
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let text =
                keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let object = keel_asm::x86::assemble_object(&text).unwrap_or_else(|e| {
                panic!(
                    "{} opt={opt} {selection:?}/{allocation:?}: {e}\n{text}",
                    path.display()
                )
            });
            let program = keel_codegen::compile_program_with_pipeline(
                &module,
                Target::X86_64LinuxGnu,
                &pipeline,
            )
            .unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_program_object(&program).unwrap(),
                "{}",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count > 100, "fixture discovery failed: {count}");
}

#[test]
#[cfg(target_os = "linux")]
fn c_metadata_and_debug_fixtures_assemble() {
    use std::process::Command;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-c/tests/programs");
    let mut count = 0;
    for path in std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
    {
        let preprocessed = Command::new("clang")
            .args(["-E", "-std=c11"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(preprocessed.status.success());
        let source = String::from_utf8(preprocessed.stdout).unwrap();
        for debug_info in [false, true] {
            for opt in [false, true] {
                let mut module = keel_c::compile(
                    &source,
                    path.to_str().unwrap(),
                    keel_c::Options {
                        debug_info,
                        target: Target::X86_64LinuxGnu,
                    },
                )
                .unwrap();
                if opt {
                    keel_opt::optimize(&mut module).unwrap()
                }
                let program =
                    keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
                keel_asm::x86::assemble_program_object(&program).unwrap_or_else(|e| {
                    panic!("{} debug={debug_info} opt={opt}: {e}", path.display())
                });
                count += 1;
            }
        }
    }
    assert!(count >= 80, "fixture discovery failed: {count}");
}
