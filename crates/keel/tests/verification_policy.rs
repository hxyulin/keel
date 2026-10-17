use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn flags_reports_and_initial_validation_work_in_every_build_mode() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("verification-policy");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("in.keel"),
        "func @f() -> i64 {\nblock0:\n v0 = iconst.i64 42\n return v0\n}\n",
    )
    .unwrap();
    fs::write(dir.join("in.c"), "int f(void) { return 42; }").unwrap();
    // Parses successfully but returns the wrong type: initial verification must reject.
    fs::write(
        dir.join("bad.keel"),
        "func @f() -> i64 {\nblock0:\n v0 = iconst.i32 42\n return v0\n}\n",
    )
    .unwrap();
    for command in ["opt", "compile", "cc"] {
        let input = if command == "cc" { "in.c" } else { "in.keel" };
        let base = [command, input, "-O", "--target", "x86_64-unknown-linux-gnu"];
        let mut outputs = Vec::new();
        for (flags, enabled) in [
            (
                vec![],
                cfg!(debug_assertions) || cfg!(feature = "verify-each"),
            ),
            (vec!["--verify-each"], true),
            (vec!["--no-verify-each"], false),
            (vec!["--no-verify-each", "--no-verify-each"], false),
        ] {
            let mut args = base.to_vec();
            args.extend(flags);
            let plain = keel(&dir, &args);
            assert!(
                plain.status.success(),
                "{}",
                String::from_utf8_lossy(&plain.stderr)
            );
            args.extend(["--opt-report", "report.json"]);
            let observed = keel(&dir, &args);
            assert!(
                observed.status.success(),
                "{}",
                String::from_utf8_lossy(&observed.stderr)
            );
            assert_eq!(plain.stdout, observed.stdout);
            let report = fs::read_to_string(dir.join("report.json")).unwrap();
            assert!(report.contains(&format!("\"verification_enabled\":{enabled}")));
            assert!(!report.contains(&format!("\"verification_enabled\":{}", !enabled)));
            if !enabled {
                // Every post-pass and restoration time must be exact zero.
                for field in ["verification_ns", "restoration_verification_ns"] {
                    for suffix in report.split(&format!("\"{field}\":")).skip(1) {
                        assert!(suffix.starts_with("0,"), "{field}: {suffix}");
                    }
                }
            }
            fs::remove_file(dir.join("report.json")).unwrap();
            outputs.push(plain.stdout);
        }
        assert!(outputs.windows(2).all(|w| w[0] == w[1]));
        for flags in [
            ["--verify-each", "--no-verify-each"],
            ["--no-verify-each", "--verify-each"],
        ] {
            let mut args = base.to_vec();
            args.extend(flags);
            let out = keel(&dir, &args);
            assert!(!out.status.success());
            assert!(String::from_utf8_lossy(&out.stderr).contains("mutually exclusive"));
        }
    }
    for command in ["opt", "compile"] {
        for flag in ["--verify-each", "--no-verify-each"] {
            for report in [false, true] {
                let mut args = vec![command, "bad.keel", flag, "--opt-tier", "none"];
                if report {
                    args.extend(["--opt-report", "invalid.json"]);
                }
                let out = keel(&dir, &args);
                assert!(!out.status.success());
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains("verifier errors"),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(!dir.join("invalid.json").exists());
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
