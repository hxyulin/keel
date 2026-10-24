use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "keel-wasm-routing-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn wasm_cli_rejects_before_loading_inputs_or_writing_outputs() {
    let temp = Temp::new();
    let output = temp.0.join("keep-output");
    let report = temp.0.join("report.json");
    let missing = temp.0.join("missing-input");
    let flags: &[(&str, &[&[&str]])] = &[
        (
            "cc",
            &[
                &[],
                &["-E"],
                &["-dM"],
                &["--emit-ir"],
                &["--emit-object"],
                &["-O"],
                &["-p", "dce"],
                &["-g"],
                &["--tune", "apple-m5"],
                &["--cpu-level", "v3"],
                &["--target-features", "+avx"],
                &["--sysroot", "/missing-sdk"],
                &["--regalloc", "fast"],
                &["--isel-tier", "full"],
            ],
        ),
        (
            "compile",
            &[
                &[],
                &["--emit-object"],
                &["-O"],
                &["-p", "dce"],
                &["-g"],
                &["--tune", "apple-m5"],
                &["--cpu-level", "v3"],
                &["--target-features", "+avx"],
            ],
        ),
        (
            "opt",
            &[
                &[],
                &["-O"],
                &["-p", "dce"],
                &["--cpu-level", "v3"],
                &["--target-features", "+avx"],
            ],
        ),
        (
            "asm",
            &[&[], &["--cpu", "apple-m5"], &["--cpu-level", "v3"]],
        ),
        ("link", &[&[]]),
        ("archive", &[&[]]),
    ];
    for (command, cases) in flags {
        for extra in *cases {
            // Try both orders so an explicit native flag cannot override Wasm.
            for target_first in [false, true] {
                std::fs::write(&output, b"caller-owned sentinel").unwrap();
                let mut process = Command::new(env!("CARGO_BIN_EXE_keel"));
                process.arg(command);
                if target_first {
                    process.args(["--target", "wasm32-unknown-unknown"]);
                }
                process.args(*extra);
                if !target_first {
                    process.args(["--target", "wasm32-unknown-unknown"]);
                }
                if matches!(*command, "cc" | "compile" | "opt")
                    && !extra.contains(&"-E")
                    && !extra.contains(&"-dM")
                {
                    process.arg("--opt-report").arg(&report);
                }
                let result = process
                    .arg(&missing)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .unwrap();
                let error = String::from_utf8_lossy(&result.stderr);
                assert!(!result.status.success());
                assert!(
                    error.contains("unsupported") && error.contains("wasm32-unknown-unknown"),
                    "{command} {extra:?}: {error}"
                );
                assert!(result.stdout.is_empty(), "{command} {extra:?}");
                assert_eq!(std::fs::read(&output).unwrap(), b"caller-owned sentinel");
                assert!(!report.exists());
            }
        }
    }
    let dir = temp.0.join("batch-output");
    let result = Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(["asm", "--target", "wasm32-unknown-unknown", "--out-dir"])
        .arg(&dir)
        .arg(&missing)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("unsupported target wasm32-unknown-unknown")
    );
    assert!(!dir.exists());
}

#[test]
fn target_free_opt_and_native_cli_controls_remain_available() {
    let temp = Temp::new();
    let input = temp.0.join("control.keel");
    let output = temp.0.join("control.out");
    std::fs::write(
        &input,
        "func @answer() -> i32 {\nblock0:\n v0 = iconst.i32 42\n return v0\n}\n",
    )
    .unwrap();
    for target in [
        None,
        Some("aarch64-apple-darwin"),
        Some("x86_64-unknown-linux-gnu"),
        Some("aarch64-unknown-linux-gnu"),
    ] {
        let mut process = Command::new(env!("CARGO_BIN_EXE_keel"));
        process.args(["opt", "-p", "dce"]);
        if let Some(target) = target {
            process.args(["--target", target]);
        }
        let result = process.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        keel_ir::verify_module(
            &keel_ir::parse(&std::fs::read_to_string(&output).unwrap()).unwrap(),
        )
        .unwrap();
    }
    for target in keel_ir::Target::NATIVE {
        let result = Command::new(env!("CARGO_BIN_EXE_keel"))
            .args(["compile", "--target", target.triple(), "--emit-object"])
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!std::fs::read(&output).unwrap().is_empty());
    }
}
