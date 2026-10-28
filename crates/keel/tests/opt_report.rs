//! CLI report destination and output-channel contract. No native toolchain needed.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
const IR: &str = "func @f() -> i64 {\nblock0:\n v0 = iconst.i64 42\n return v0\n}\n";
fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn reports_are_explicit_and_do_not_change_stdout_or_named_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("opt-report-output");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.keel"), IR).unwrap();
    fs::write(dir.join("in.c"), "int f(void) { return 42; }").unwrap();
    for (n, args) in [
        vec!["opt", "in.keel", "-p", "dce"],
        vec![
            "compile",
            "in.keel",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-O",
        ],
        vec![
            "compile",
            "in.keel",
            "--target",
            "aarch64-apple-darwin",
            "-O",
        ],
        vec![
            "cc",
            "in.c",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-O",
            "--emit-ir",
        ],
        vec!["opt", "in.keel", "--opt-tier", "none"],
    ]
    .into_iter()
    .enumerate()
    {
        let plain = keel(&dir, &args);
        assert!(
            plain.status.success(),
            "{}",
            String::from_utf8_lossy(&plain.stderr)
        );
        let name = format!("report-{n}.json");
        let mut observed = args.clone();
        observed.extend(["--opt-report", &name]);
        let output = keel(&dir, &observed);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(plain.stdout, output.stdout);
        assert_eq!(plain.stderr, output.stderr);
        let report = fs::read_to_string(dir.join(&name)).unwrap();
        assert!(report.starts_with("{\"schema_version\":2,\"invocations\":["));
        if n == 4 {
            assert_eq!(report, "{\"schema_version\":2,\"invocations\":[]}\n");
        }
        fs::remove_file(dir.join(&name)).unwrap();
        observed.extend(["-o", "out"]);
        let named = keel(&dir, &observed);
        assert!(
            named.status.success(),
            "{}",
            String::from_utf8_lossy(&named.stderr)
        );
        assert!(named.stdout.is_empty());
        assert_eq!(plain.stdout, fs::read(dir.join("out")).unwrap());
        fs::remove_file(dir.join(&name)).unwrap();
    }
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn report_errors_preserve_inputs_outputs_and_existing_evidence() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("opt-report-errors");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.keel"), IR).unwrap();
    fs::write(dir.join("in.c"), "int f(void) { return 42; }").unwrap();
    fs::write(dir.join("old.json"), "evidence").unwrap();
    fs::write(dir.join("out"), "output evidence").unwrap();
    for args in [
        vec![
            "cc",
            "in.c",
            "--emit-ir",
            "--emit-object",
            "--opt-report",
            "new.json",
        ],
        vec!["opt", "in.keel", "--opt-report"],
        vec!["opt", "in.keel", "--opt-report", "-"],
        vec!["opt", "in.keel", "--opt-report", ""],
        vec!["opt", "in.keel", "--opt-report", "in.keel"],
        vec!["opt", "in.keel", "--opt-report", "old.json"],
        vec![
            "opt",
            "in.keel",
            "--opt-report",
            "new.json",
            "-o",
            "./new.json",
        ],
        vec!["opt", "in.keel", "--opt-report", "missing/report.json"],
        vec![
            "opt",
            "in.keel",
            "--opt-report",
            "new.json",
            "--opt-report",
            "second.json",
        ],
        vec![
            "opt",
            "in.keel",
            "--opt-report",
            "new.json",
            "-p",
            "missing",
            "-o",
            "out",
        ],
        vec!["cc", "in.keel", "-E", "--opt-report", "new.json"],
    ] {
        let output = keel(&dir, &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
        assert!(!dir.join("new.json").exists());
        assert_eq!(fs::read_to_string(dir.join("in.keel")).unwrap(), IR);
        assert_eq!(
            fs::read_to_string(dir.join("old.json")).unwrap(),
            "evidence"
        );
        assert_eq!(
            fs::read_to_string(dir.join("out")).unwrap(),
            "output evidence"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("old.json", dir.join("alias.json")).unwrap();
        assert!(
            !keel(&dir, &["opt", "in.keel", "--opt-report", "alias.json"])
                .status
                .success()
        );
        std::os::unix::fs::symlink("new.json", dir.join("dangling-output")).unwrap();
        assert!(
            !keel(
                &dir,
                &[
                    "opt",
                    "in.keel",
                    "--opt-report",
                    "new.json",
                    "-o",
                    "dangling-output"
                ]
            )
            .status
            .success()
        );
        assert!(!dir.join("new.json").exists());
    }
    fs::remove_dir_all(dir).unwrap();
}
