//! Native differential checks for dispatch, pressure and rare-call kernels.
use std::{fs, path::Path, process::Command};

fn run(cmd: &mut Command) {
    let out = cmd.output().expect("execute kernel tool");
    assert!(
        out.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn native_control_and_pressure_match_handwritten_and_clang() {
    if !cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        eprintln!("SKIP: kernel native oracle requires AArch64 macOS or x86-64 Linux");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("backend-control-native");
    fs::create_dir_all(&dir).expect("create kernel directory");
    for (name, text) in [
        ("kernels.c", include_str!("backend_control/kernels.c")),
        ("hand.c", include_str!("backend_control/hand.c")),
        ("oracle.c", include_str!("backend_control/oracle.c")),
    ] {
        fs::write(dir.join(name), text).expect("write kernel fixture");
    }
    run(Command::new("clang").current_dir(&dir).args([
        "-O3",
        "-DPREFIX=clang_",
        "-c",
        "kernels.c",
        "-o",
        "clang.o",
    ]));
    let allocators = ["fast", "full"];
    for tier in ["-O0", "-O1", "-O3"] {
        for allocator in allocators {
            for scheduled in [false, true] {
                run(Command::new(env!("CARGO_BIN_EXE_keel"))
                    .current_dir(&dir)
                    .arg("cc")
                    .args(if scheduled {
                        vec!["--schedule-pressure"]
                    } else {
                        vec![]
                    })
                    .args([
                        tier,
                        "--regalloc",
                        allocator,
                        "--link-intent",
                        "executable",
                        "--verify-each",
                        "--emit-object",
                        "kernels.c",
                        "-o",
                        "keel.o",
                    ]));
                run(Command::new("clang").current_dir(&dir).args([
                    "-O2", "oracle.c", "hand.c", "keel.o", "clang.o", "-o", "oracle",
                ]));
                run(&mut Command::new(dir.join("oracle")));
            }
        }
    }
}
