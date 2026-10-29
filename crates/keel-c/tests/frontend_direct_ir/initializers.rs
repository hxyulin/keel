//! Regression coverage for the flattened initializer replication boundary.
use keel_ir::Target;

const SOURCE: &str = include_str!("../fixtures/initializer_snapshot.c");

#[test]
fn initializer_snapshot_layout_and_execution() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug_info in [false, true] {
            let module = keel_c::compile(
                SOURCE,
                "initializer_snapshot.c",
                keel_c::Options { target, debug_info },
            )
            .unwrap();
            keel_ir::verify_module(&module).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                keel_ir::verify_module(&module).unwrap();
                let asm = keel_codegen::compile(&module, target).unwrap();
                if cfg!(all(target_arch = "x86_64", target_os = "linux"))
                    && target == Target::X86_64LinuxGnu
                {
                    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join(format!("initializer-snapshot-{debug_info}-{optimize}"));
                    std::fs::create_dir_all(&dir).unwrap();
                    let assembly = dir.join("test.s");
                    let executable = dir.join("test");
                    std::fs::write(&assembly, asm).unwrap();
                    let link = std::process::Command::new("clang")
                        .arg(&assembly)
                        .arg("-o")
                        .arg(&executable)
                        .output()
                        .unwrap();
                    assert!(
                        link.status.success(),
                        "{}",
                        String::from_utf8_lossy(&link.stderr)
                    );
                    let output = std::process::Command::new(executable).output().unwrap();
                    assert_eq!(output.status.code(), Some(0), "{debug_info} {optimize}");
                }
            }
        }
    }
}

#[test]
fn initializer_diagnostics_keep_designator_locations() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug_info in [false, true] {
            for (declaration, column, message) in [
                (
                    "int a[2] = {[1 ... 0] = 3};",
                    13,
                    "array designator out of range",
                ),
                (
                    "int a[2] = {[0 ... 2] = 3};",
                    13,
                    "array designator out of range",
                ),
                ("int a[1] = {1, 2};", 16, "too many initializers"),
                (
                    "int a[2] = {.x = 1};",
                    13,
                    "`.` designator for a non-struct",
                ),
            ] {
                let source = format!("# 41 \"initializer-errors.c\"\n{declaration}\n");
                let error =
                    keel_c::compile(&source, "input.i", keel_c::Options { target, debug_info })
                        .unwrap_err();
                assert_eq!(error.file, "initializer-errors.c");
                assert_eq!(
                    (error.line, error.col),
                    (41, column),
                    "{declaration}: {error}"
                );
                assert_eq!(error.message, message);
            }
        }
    }
}

#[test]
fn sparse_and_empty_large_array_initializers_do_not_enumerate_children() {
    // The objects are unevaluated, so neither parsing nor IR generation needs
    // storage proportional to their declared bounds.
    let source = r#"
        _Static_assert(sizeof((int[2000000000]){}) == 8000000000UL, "empty");
        _Static_assert(sizeof((int[2000000000]){[1999999999] = 7}) == 8000000000UL, "sparse");
        _Static_assert(sizeof((int[2][1000000000]){1, 2}) == 8000000000UL, "elided");
        int value(void) { return 7; }
    "#;
    for target in Target::NATIVE {
        let module = keel_c::compile(
            source,
            "large-initializers.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}
