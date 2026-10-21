//! Scalar C oracle and both direct/text object emission paths for nested selects.
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};
fn run(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn keel(dir: &Path, args: &[&str]) {
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args));
}
fn text(file: &Path) -> Vec<u8> {
    let bytes = fs::read(file).unwrap();
    let object = object::File::parse(bytes.as_slice()).unwrap();
    object
        .sections()
        .find(|s| matches!(s.name(), Ok(".text" | "__text")))
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split(&format!("func @{name}("))
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap()
}
#[test]
fn nested_c_ternaries_preserve_scalar_results_and_both_object_paths() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("if-convert-nested");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("kernels.c"),
        include_str!("if_convert_nested/kernels.c"),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("if_convert_nested/oracle.c"),
    )
    .unwrap();
    run(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-vectorize",
        "-fno-slp-vectorize",
        "-c",
        "oracle.c",
        "-o",
        "oracle.o",
    ]));
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "--verify-each",
                "--opt-tier",
                // Keep the pass oracle scalar: Full can now convert these
                // shapes privately during successful loop vectorization.
                "basic",
                "--emit-ir",
                "kernels.c",
                "-o",
                "before.keel",
            ],
        );
        keel(
            &dir,
            &[
                "opt",
                "--target",
                target,
                "--verify-each",
                "-p",
                "gvn,load-reuse,if-convert,simplify-cfg,phi-simplify,dce",
                "before.keel",
                "-o",
                "after.keel",
            ],
        );
        let before = fs::read_to_string(dir.join("before.keel")).unwrap();
        let after = fs::read_to_string(dir.join("after.keel")).unwrap();
        for name in ["positive", "clamped"] {
            assert!(
                function(&after, name).matches("brif ").count()
                    < function(&before, name).matches("brif ").count(),
                "{name}: {after}"
            );
            assert!(function(&after, name).contains("select "));
        }
        assert!(function(&after, "guarded").contains("sdiv "));
        assert!(function(&after, "guarded").matches("brif ").count() >= 2);
        assert!(
            function(&after, "volatile_read")
                .matches("load.i32 volatile")
                .count()
                >= 2
        );
        for isel in ["none", "all"] {
            let stem = format!("{target}-{isel}");
            keel(
                &dir,
                &[
                    "compile",
                    "--target",
                    target,
                    "--verify-each",
                    "--isel",
                    isel,
                    "--emit-object",
                    "after.keel",
                    "-o",
                    &format!("{stem}.o"),
                ],
            );
            keel(
                &dir,
                &[
                    "compile",
                    "--target",
                    target,
                    "--verify-each",
                    "--isel",
                    isel,
                    "after.keel",
                    "-o",
                    &format!("{stem}.s"),
                ],
            );
            keel(
                &dir,
                &[
                    "asm",
                    "--target",
                    target,
                    &format!("{stem}.s"),
                    "-o",
                    &format!("{stem}-text.o"),
                ],
            );
            assert_eq!(
                text(&dir.join(format!("{stem}.o"))),
                text(&dir.join(format!("{stem}-text.o")))
            );
            let native = if cfg!(target_arch = "aarch64") {
                "aarch64-apple-darwin"
            } else {
                "x86_64-unknown-linux-gnu"
            };
            if target == native {
                for suffix in ["", "-text"] {
                    let exe = format!("{stem}{suffix}-oracle");
                    run(Command::new("clang").current_dir(&dir).args([
                        "oracle.o",
                        &format!("{stem}{suffix}.o"),
                        "-o",
                        &exe,
                    ]));
                    run(&mut Command::new(dir.join(exe)));
                }
            }
        }
    }
}
