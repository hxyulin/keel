#![cfg(unix)]

use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Driver(PathBuf);
impl Driver {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-system-args-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let clang = dir.join("clang");
        std::fs::write(
            &clang,
            "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$KEEL_TEST_ARGS\"\nprintf '%s\\0' \"$CCC_OVERRIDE_OPTIONS\" \"$CL\" \"$_CL_\" > \"$KEEL_TEST_ARGS.env\"\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(clang, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(dir)
    }
    fn invoke(&self, args: &[&str]) -> Output {
        let record = self.0.join("args");
        let _ = std::fs::remove_file(&record);
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&self.0)
            .arg("link")
            .args(args)
            .env("PATH", &self.0)
            .env("KEEL_TEST_ARGS", record)
            .env_remove("CCC_OVERRIDE_OPTIONS")
            .output()
            .unwrap()
    }
    fn forwarded(&self, args: &[&str], expected: &[&str]) {
        let result = self.invoke(args);
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = std::fs::read(self.0.join("args")).unwrap();
        let actual: Vec<_> = bytes
            .strip_suffix(&[0])
            .unwrap()
            .split(|b| *b == 0)
            .map(|b| std::str::from_utf8(b).unwrap())
            .collect();
        assert_eq!(actual, expected, "{args:?}");
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn system_forwards_all_target_spellings_and_repeated_selections() {
    let driver = Driver::new();
    for target in ["wasm32-unknown-unknown", "x86_64-unknown-linux-gnu"] {
        for flags in [
            vec!["--target", target],
            vec!["-target", target],
            vec![&format!("--target={target}")],
            vec![&format!("-target={target}")],
        ] {
            let mut args = vec!["--system"];
            args.extend_from_slice(&flags);
            driver.forwarded(&args, &flags);
        }
    }
    for targets in [
        [
            "--target=wasm32-unknown-unknown",
            "--target=x86_64-unknown-linux-gnu",
        ],
        [
            "--target=x86_64-unknown-linux-gnu",
            "--target=wasm32-unknown-unknown",
        ],
    ] {
        driver.forwarded(&["--system", targets[0], targets[1]], &targets);
    }
    driver.forwarded(
        &["--target", "wasm32-unknown-unknown", "--system", "input.o"],
        &["--target", "wasm32-unknown-unknown", "input.o"],
    );
}

#[test]
fn system_keeps_operands_end_of_options_and_only_removes_selector() {
    let driver = Driver::new();
    for option in ["-o", "-Xlinker", "-Xclang", "-I", "-include"] {
        for value in ["--target=wasm32-unknown-unknown", "--system"] {
            driver.forwarded(
                &["--system", option, value, "input.o"],
                &[option, value, "input.o"],
            );
        }
    }
    driver.forwarded(
        &["-o", "--system", "--system", "input.o"],
        &["-o", "--system", "input.o"],
    );
    driver.forwarded(
        &[
            "--system",
            "--",
            "--system",
            "--sysroot",
            "--target=wasm32-unknown-unknown",
        ],
        &[
            "--",
            "--system",
            "-isysroot",
            "--target=wasm32-unknown-unknown",
        ],
    );
    driver.forwarded(
        &[
            "--system",
            "--sysroot",
            "sdk",
            "-syslibroot",
            "other",
            "--config=custom.cfg",
            "--driver-mode=cl",
        ],
        &[
            "-isysroot",
            "sdk",
            "-isysroot",
            "other",
            "--config=custom.cfg",
            "--driver-mode=cl",
        ],
    );
}

#[test]
fn response_files_are_opaque_even_when_nested_missing_malformed_or_cyclic() {
    let driver = Driver::new();
    for (name, contents) in [
        ("direct.rsp", "--target=wasm32-unknown-unknown"),
        ("outer.rsp", "@\"nested quoted.rsp\""),
        (
            "nested quoted.rsp",
            "-target wasm32-unknown-unknown -o 'quoted output' escaped\\ path",
        ),
        ("malformed.rsp", "'unterminated"),
        ("cycle.rsp", "@cycle.rsp"),
        ("config.cfg", "--target=wasm32-unknown-unknown"),
    ] {
        std::fs::write(driver.0.join(name), contents).unwrap();
    }
    for response in [
        "@direct.rsp",
        "@outer.rsp",
        "@nested quoted.rsp",
        "@missing.rsp",
        "@malformed.rsp",
        "@cycle.rsp",
        "--config=config.cfg",
    ] {
        driver.forwarded(&["--system", response], &[response]);
        driver.forwarded(&["--system", "--", response], &["--", response]);
    }
}

#[test]
fn system_inherits_driver_argument_environment_without_interpreting_it() {
    let driver = Driver::new();
    let result = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&driver.0)
        .args(["link", "--system", "input.o"])
        .env("PATH", &driver.0)
        .env("KEEL_TEST_ARGS", driver.0.join("args"))
        .env("CCC_OVERRIDE_OPTIONS", "+--target=wasm32-unknown-unknown")
        .env("CL", "/clang:--target=wasm32-unknown-unknown")
        .env("_CL_", "@driver.rsp")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(std::fs::read(driver.0.join("args")).unwrap(), b"input.o\0");
    assert_eq!(
        std::fs::read(driver.0.join("args.env")).unwrap(),
        b"+--target=wasm32-unknown-unknown\0/clang:--target=wasm32-unknown-unknown\0@driver.rsp\0"
    );
}

#[test]
fn native_wasm_rejection_preserves_output_and_never_spawns_driver() {
    let driver = Driver::new();
    let output = driver.0.join("keep");
    std::fs::write(&output, b"sentinel").unwrap();
    for flags in [
        vec!["--target", "wasm32-unknown-unknown"],
        vec!["-target", "wasm32-unknown-unknown"],
        vec!["--target=wasm32-unknown-unknown"],
        vec!["-target=wasm32-unknown-unknown"],
        vec![
            "--target=wasm32-unknown-unknown",
            "--target",
            "x86_64-unknown-linux-gnu",
        ],
    ] {
        let mut args = flags;
        args.extend(["-o", "keep", "missing.o"]);
        let result = driver.invoke(&args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("native linking is unavailable"));
        assert!(!driver.0.join("args").exists());
        assert_eq!(std::fs::read(&output).unwrap(), b"sentinel");
    }
}

#[test]
fn native_option_operands_and_double_dash_do_not_select_system_driver() {
    let driver = Driver::new();
    for option in [
        "-o",
        "-e",
        "--entry",
        "--dynamic-linker",
        "--rpath",
        "-rpath",
        "--sysroot",
        "-syslibroot",
        "-L",
        "-l",
        "--require-export",
        "--debug-map",
        "--dsym",
        "-mmacosx-version-min",
        "--target",
        "-target",
    ] {
        assert!(!driver.invoke(&[option, "--system"]).status.success());
        assert!(!driver.0.join("args").exists(), "{option}");
    }
    for args in [
        vec!["-o", "--system", "missing.o"],
        vec!["--target", "x86_64-unknown-linux-gnu", "--", "--system"],
        vec!["-Xlinker", "--system"],
    ] {
        assert!(!driver.invoke(&args).status.success());
        assert!(!driver.0.join("args").exists(), "{args:?}");
    }
    // Exercise the real native linker, including operand spellings that the old
    // pre-scan mistook for a target/selector. No external tools are available.
    let object = keel_asm::x86::assemble_object(".text\n.globl _start\n_start:\nret\n").unwrap();
    std::fs::write(driver.0.join("input.o"), &object).unwrap();
    for output in ["--system", "--target=wasm32-unknown-unknown"] {
        let result = driver.invoke(&[
            "--target",
            "x86_64-unknown-linux-gnu",
            "-o",
            output,
            "input.o",
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!driver.0.join("args").exists());
        assert_eq!(
            &std::fs::read(driver.0.join(output)).unwrap()[..4],
            b"\x7fELF"
        );
        std::fs::write(driver.0.join(output), &object).unwrap();
        let result = driver.invoke(&[
            "--target",
            "x86_64-unknown-linux-gnu",
            "-o",
            "native",
            "--",
            output,
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!driver.0.join("args").exists());
    }
}
