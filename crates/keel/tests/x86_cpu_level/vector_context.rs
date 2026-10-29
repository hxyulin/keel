use std::{fs, process::Command};
#[test]
fn effective_features_preserve_128_bit_default_and_schema2_reports() {
    let dir = std::env::temp_dir().join(format!("keel-wide-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("sum.ir");
    fs::write(
        &input,
        include_str!("../../../keel-opt/tests/fixtures/signed_widen_sum.ir"),
    )
    .unwrap();
    for (index, flags, wide) in [
        (0, vec!["--cpu-level", "v1"], false),
        (1, vec!["--cpu-level", "v2"], false),
        (2, vec!["--cpu-level", "v3"], false),
        (
            3,
            vec!["--cpu-level", "v3", "--target-features", "-avx2"],
            false,
        ),
        (
            4,
            vec!["--cpu-level", "v1", "--target-features", "+avx,+avx2"],
            false,
        ),
        (
            5,
            vec![
                "--cpu-level",
                "v3",
                "--target-features",
                "-avx2",
                "--target-features",
                "+aes",
            ],
            false,
        ),
    ] {
        let mut ordinary = None;
        for observed in [false, true] {
            let report = dir.join(format!("report-{index}.json"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_keel"));
            cmd.args(["opt", "--target", "x86_64-unknown-linux-gnu", "-O"])
                .args(&flags)
                .arg(&input);
            if observed {
                cmd.arg("--opt-report").arg(&report);
            }
            let output = cmd.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert_eq!(text.contains("i64x4"), wide, "{text}");
            if observed {
                assert_eq!(ordinary.as_ref(), Some(&text));
                let json = fs::read_to_string(report).unwrap();
                assert!(json.starts_with("{\"schema_version\":2,"));
                assert!(!json.contains("\"context\""));
            } else {
                ordinary = Some(text);
            }
        }
    }
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .args([
            "opt",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--cpu-level",
            "v1",
            "--target-features",
            "+avx2,-avx",
            "-O",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
