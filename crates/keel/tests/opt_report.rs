//! CLI report destination and output-channel contract for `--opt-report` and
//! `--codegen-report`. No native toolchain needed.
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
        assert_eq!(
            report.matches(",\"rules\":{").count(),
            report.matches("{\"pipeline_index\":").count()
        );
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
        vec!["cc", "in.c", "--emit-ir", "--codegen-report", "new.json"],
        vec!["cc", "in.c", "-E", "--codegen-report", "new.json"],
        vec!["opt", "in.keel", "--codegen-report", "new.json"],
        vec!["compile", "in.keel", "--codegen-report", "old.json"],
        vec!["compile", "in.keel", "--codegen-report", "-"],
        vec![
            "compile",
            "in.keel",
            "--codegen-report",
            "new.json",
            "-o",
            "new.json",
        ],
        vec![
            "compile",
            "in.keel",
            "--codegen-report",
            "new.json",
            "--opt-report",
            "./new.json",
        ],
        vec![
            "compile",
            "in.keel",
            "--codegen-report",
            "new.json",
            "--codegen-report",
            "second.json",
        ],
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

/// 20 values live across a call: more than either target's callee-saved
/// registers, so `pressure` must spill. `small` needs no spill.
fn pressure_ir() -> String {
    let mut ir = String::from(
        "decl @clobber()\nfunc @small(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\nfunc @pressure(i64) -> i64 {\nblock0(v0: i64):\n",
    );
    for i in 1..=20 {
        ir.push_str(&format!(
            " v{} = iconst.i64 {i}\n v{} = iadd v0, v{}\n",
            2 * i - 1,
            2 * i,
            2 * i - 1
        ));
    }
    ir.push_str(" call @clobber()\n v41 = iadd v2, v4\n");
    for i in 3..=20 {
        ir.push_str(&format!(" v{} = iadd v{}, v{}\n", 39 + i, 38 + i, 2 * i));
    }
    ir.push_str(" return v58\n}\n");
    ir
}
#[test]
fn codegen_reports_count_spills_reloads_and_moves_per_function() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("codegen-report-output");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.keel"), pressure_ir()).unwrap();
    let mut n = 0;
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for extra in [vec![], vec!["-O"], vec!["--emit-object"]] {
            n += 1;
            let output = format!("out-{n}");
            let mut args = vec!["compile", "in.keel", "--target", target, "-o", &output];
            args.extend(&extra);
            let plain = keel(&dir, &args);
            assert!(
                plain.status.success(),
                "{}",
                String::from_utf8_lossy(&plain.stderr)
            );
            let expected = fs::read(dir.join(&output)).unwrap();
            fs::remove_file(dir.join(&output)).unwrap();
            let name = format!("report-{n}.json");
            args.extend(["--codegen-report", &name]);
            let observed = keel(&dir, &args);
            assert!(
                observed.status.success(),
                "{}",
                String::from_utf8_lossy(&observed.stderr)
            );
            assert_eq!(plain.stdout, observed.stdout);
            assert_eq!(plain.stderr, observed.stderr);
            assert_eq!(expected, fs::read(dir.join(&output)).unwrap(), "{args:?}");
            let report: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(dir.join(&name)).unwrap()).unwrap();
            assert_eq!(report["schema_version"], 1);
            assert_eq!(report["target"], target);
            let functions = report["functions"].as_array().unwrap();
            let names: Vec<_> = functions.iter().map(|f| &f["name"]).collect();
            assert_eq!(names, ["small", "pressure"], "{args:?}");
            let counters = |f: &serde_json::Value, key: &str| f["counters"][key].as_u64().unwrap();
            for key in [
                "spill_stores",
                "reloads",
                "register_moves",
                "rematerialized_constants",
                "rematerialized_stack_addresses",
            ] {
                let sum: u64 = functions.iter().map(|f| counters(f, key)).sum();
                assert_eq!(counters(&report, key), sum, "{key} {args:?}");
            }
            assert_eq!(counters(&functions[0], "spill_stores"), 0);
            assert_eq!(counters(&functions[0], "reloads"), 0);
            for key in ["spill_stores", "reloads", "register_moves"] {
                assert!(counters(&functions[1], key) > 0, "{key} {args:?}");
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn codegen_reports_count_cover_firings_per_function() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("codegen-report-covers");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let ir = "func @branch(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = icmp.slt v0, v1\n brif v2, block1, block2\nblock1:\n return v0\nblock2:\n return v1\n}\n\
              func @plain(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\n";
    fs::write(dir.join("in.keel"), ir).unwrap();
    let mut n = 0;
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for (selection, fired) in [("all", 1), ("all,-compare-branch", 0)] {
            n += 1;
            let name = format!("report-{n}.json");
            let output = format!("out-{n}.s");
            let args = [
                "compile",
                "in.keel",
                "--target",
                target,
                "--isel",
                selection,
                "-o",
                &output,
                "--codegen-report",
                &name,
            ];
            let run = keel(&dir, &args);
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let report: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(dir.join(&name)).unwrap()).unwrap();
            let covers = |f: &serde_json::Value| f["covers"].as_object().unwrap().clone();
            let functions = report["functions"].as_array().unwrap();
            assert_eq!(
                covers(&functions[0])
                    .get("compare-branch")
                    .and_then(|v| v.as_u64()),
                (fired != 0).then_some(fired),
                "{args:?}"
            );
            assert!(covers(&functions[1]).is_empty(), "{args:?}");
            assert_eq!(covers(&report), covers(&functions[0]), "{args:?}");
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
