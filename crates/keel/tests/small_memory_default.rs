//! Full's implicit pipeline expands bounded intrinsics on both backends.
use std::{fs, path::Path, process::Command};

#[test]
fn implicit_full_expands_16_bytes_and_preserves_fallbacks() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("small-memory-default");
    fs::create_dir_all(&dir).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for length in ["16", "33", "n"] {
            fs::write(dir.join("copy.c"), format!(
                "void copy(char *d, char *s, unsigned long n) {{ __builtin_memcpy(d, s, {length}); }}"
            )).unwrap();
            for tier in ["none", "basic", "full"] {
                let args = if tier == "full" {
                    vec!["-O"]
                } else {
                    vec!["--opt-tier", tier]
                };
                for emit in ["--emit-ir", "--emit-asm", "--emit-object"] {
                    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
                        .current_dir(&dir)
                        .args(["cc", "--target", target])
                        .args(&args)
                        .args([emit, "copy.c"])
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if emit != "--emit-object" {
                        let text = String::from_utf8(output.stdout).unwrap();
                        assert_eq!(
                            text.contains("memcpy"),
                            tier != "full" || length != "16",
                            "{target} {tier} {length} {emit}: {text}"
                        );
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
