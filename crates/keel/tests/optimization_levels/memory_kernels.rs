use super::successful;
use std::{fs, path::Path, process::Command};

fn command(dir: &Path, program: &str, args: &[&str]) {
    let output = Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn adjacent_byte_loads_and_short_capture_copies_match_native_oracles() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-kernel-oracles");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("kernels.c"),
        include_str!("../fixtures/memory_kernels.c"),
    )
    .unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("../fixtures/memory_kernels_driver.c"),
    )
    .unwrap();
    command(&dir, "clang", &["-O2", "-c", "driver.c", "-o", "driver.o"]);
    for flags in [
        vec!["-O0"],
        vec!["-O"],
        vec!["-O", "-fno-tree-loop-vectorize"],
    ] {
        let mut args = vec!["cc", "kernels.c", "--emit-object", "-o", "kernels.o"];
        args.extend_from_slice(&flags);
        successful(&dir, &args);
        command(&dir, "clang", &["driver.o", "kernels.o", "-o", "check"]);
        command(&dir, "./check", &[]);
    }
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let bytes = successful(&dir, &["cc", "kernels.c", "-O", "--target", target]);
        let asm = String::from_utf8(bytes).unwrap();
        if target.starts_with("aarch64") {
            assert!(asm.matches("\tldrh\t").count() >= 2, "{asm}");
        } else {
            assert!(asm.contains("word ptr"), "{asm}");
        }
    }
}
