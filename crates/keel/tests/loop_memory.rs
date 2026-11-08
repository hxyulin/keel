//! Prefix control flow and scalar memory recurrences from the C application profiles.
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
fn source(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    for (name, text) in [
        ("kernels.c", include_str!("loop_memory/kernels.c")),
        ("hand.c", include_str!("loop_memory/hand.c")),
        ("oracle.c", include_str!("loop_memory/oracle.c")),
    ] {
        fs::write(dir.join(name), text).unwrap();
    }
}
#[test]
fn native_prefix_and_aliasing_hash_oracles() {
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        eprintln!("SKIP: native C kernel oracle requires AArch64 macOS or x86-64 Linux");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-memory-native");
    source(&dir);
    for disabled in [false, true] {
        let mut cc = Command::new(env!("CARGO_BIN_EXE_keel"));
        cc.current_dir(&dir).args([
            "cc",
            "-O",
            "--link-intent",
            "executable",
            "--verify-each",
            "--emit-object",
        ]);
        if disabled {
            cc.args(["--opt-disable", "loop-rotate,loop-unroll-exact,load-reuse"]);
        }
        cc.args(["kernels.c", "-o", "kernels.o"]);
        run(&mut cc);
        run(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "-fno-strict-aliasing",
            "oracle.c",
            "hand.c",
            "kernels.o",
            "-o",
            "oracle",
        ]));
        run(&mut Command::new(dir.join("oracle")));
    }
}
#[test]
fn prefix_and_memory_recurrences_verify_for_both_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("loop-memory-targets");
    source(&dir);
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                "cc",
                "--target",
                target,
                "-O",
                "--verify-each",
                "--emit-object",
                "kernels.c",
                "-o",
                "kernels.o",
            ]));
    }
}
