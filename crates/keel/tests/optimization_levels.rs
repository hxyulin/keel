//! Public presets must preserve behavior across native targets and stage overrides.
use std::{fs, path::Path, process::Command};

fn invoke(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn successful(dir: &Path, args: &[&str]) -> Vec<u8> {
    let output = invoke(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn levels_have_distinct_pipelines_and_last_preset_wins() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("optimization-levels");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "static int helper(int x) { return x * 3 + 1; } int f(int x) { return helper(x) + helper(x + 1); }").unwrap();
    let ir = |flags: &[&str]| {
        let mut args = vec![
            "cc",
            "input.c",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
        ];
        args.extend_from_slice(flags);
        String::from_utf8(successful(&dir, &args)).unwrap()
    };
    let unoptimized = ir(&["-O0"]);
    let full = ir(&["-O3"]);
    let size = ir(&["-Os"]);
    assert!(unoptimized.contains("call @helper"), "{unoptimized}");
    assert!(size.contains("call @helper"), "{size}");
    assert!(!full.contains("call @helper"), "{full}");
    assert_eq!(full, ir(&["-O"]));
    assert_eq!(unoptimized, ir(&["-O3", "-O0"]));
    assert_eq!(full, ir(&["-O0", "-O3"]));
    for level in ["-O0", "-O1", "-O2", "-O3", "-Os"] {
        let report_name = format!("{level}.json");
        let output = successful(
            &dir,
            &[
                "cc",
                "input.c",
                "--target",
                "x86_64-unknown-linux-gnu",
                level,
                "--emit-ir",
                "--opt-report",
                &report_name,
            ],
        );
        assert_eq!(output, ir(&[level]).as_bytes());
        let report = fs::read_to_string(dir.join(&report_name)).unwrap();
        if level == "-Os" {
            assert!(!report.contains("loop-unroll"));
            assert!(!report.contains("inline-cheap-reserve"));
        }
        fs::remove_file(dir.join(report_name)).unwrap();
    }
    assert!(invoke(&dir, &["cc", "input.c", "-O4"]).status.code() == Some(1));
    for (flags, expected) in [
        (vec!["-O0"], "0 0"),
        (vec!["-O1"], "1 0"),
        (vec!["-O2"], "1 0"),
        (vec!["-O3"], "1 0"),
        (vec!["-Os"], "1 1"),
        (vec!["-Os", "-U__OPTIMIZE_SIZE__"], "1 0"),
    ] {
        fs::write(dir.join("macros.c"), "#ifdef __OPTIMIZE__\n#define SPEED 1\n#else\n#define SPEED 0\n#endif\n#ifdef __OPTIMIZE_SIZE__\n#define SIZE 1\n#else\n#define SIZE 0\n#endif\nSPEED SIZE\n").unwrap();
        let mut args = vec![
            "cc",
            "macros.c",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-E",
        ];
        args.extend(flags);
        let result = String::from_utf8(successful(&dir, &args)).unwrap();
        assert!(
            result.lines().any(|line| line.trim() == expected),
            "{args:?}: {result}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_presets_and_stable_backends_execute_the_same_program() {
    let Some(target) = keel_ir::Target::host() else {
        return;
    };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("optimization-levels-native");
    fs::create_dir_all(&dir).unwrap();
    let source = "static unsigned mix(unsigned x) { return (x ^ (x >> 7)) * 33u; }\nunsigned compute(unsigned n) { unsigned a=17, b=3; for(unsigned i=0;i<n;i++){a=mix(a+i); b+=a^(i*13u);} return a^b; }\nint main(void) { unsigned n=100; volatile unsigned *p=&n; return compute(*p) != 1444267299u; }\n";
    fs::write(dir.join("input.c"), source).unwrap();
    let clang = std::env::var("KEEL_CLANG").unwrap_or_else(|_| "clang".into());
    let reference = Command::new(&clang)
        .current_dir(&dir)
        .args(["-O2", "input.c", "-o", "reference"])
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert!(
        Command::new(dir.join("reference"))
            .status()
            .unwrap()
            .success(),
        "invalid independent expected value"
    );
    // Each level runs with its preset backend. The unoptimized and Full IR
    // levels also run with every selector and allocator.
    let mut runs = Vec::new();
    for level in ["-O0", "-O1", "-O2", "-O3", "-Os"] {
        if level == "-O0" || level == "-O3" {
            for selection in ["baseline", "trees", "full"] {
                for allocation in ["fast", "full"] {
                    runs.push(vec![
                        level,
                        "--isel-tier",
                        selection,
                        "--regalloc",
                        allocation,
                    ]);
                }
            }
        } else {
            runs.push(vec![level]);
        }
    }
    for flags in runs {
        let mut args = vec!["cc", "input.c", "--target", target.triple()];
        args.extend(&flags);
        args.extend(["--emit-object", "-o", "output.o"]);
        successful(&dir, &args);
        let result = Command::new(&clang)
            .current_dir(&dir)
            .args(["output.o", "-o", "output"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            Command::new(dir.join("output")).status().unwrap().success(),
            "{flags:?}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn size_preset_controls_codegen_independently_of_pass_and_stage_overrides() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("size-emission-preset");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "long leaf(long x) { return x + 3; }\n").unwrap();
    for (flags, size) in [
        (vec!["-Os"], true),
        (vec!["-Os", "--opt-tier", "none"], true),
        (
            vec![
                "-Os",
                "-p",
                "dce",
                "--isel-tier",
                "baseline",
                "--regalloc",
                "fast",
            ],
            true,
        ),
        (vec!["-O2", "-Os"], true),
        (vec!["-Os", "-O2"], false),
        (vec!["--opt-tier", "size"], false),
    ] {
        let mut args = vec!["cc", "input.c", "--target", "x86_64-unknown-linux-gnu"];
        args.extend(flags);
        let text = String::from_utf8(successful(&dir, &args)).unwrap();
        assert_eq!(text.contains(".p2align\t4"), !size, "{args:?}: {text}");
        assert_eq!(text.contains("push\trbp"), !size, "{args:?}: {text}");
    }
    fs::remove_dir_all(dir).unwrap();
}
