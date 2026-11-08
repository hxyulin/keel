//! loop-unroll on loops that come from C: the scalar pipeline leaves one load
//! per hot loop, loop-unroll makes it three, a second run changes nothing,
//! unsupported loops are left alone, and the unrolled code matches the C
//! driver on the host.
use super::successful;
use std::{fs, process::Command};

const FIXTURES: &str = "../keel-opt/tests/loop_unroll";
/// The scalar part of a Full round, so the loops reach loop-unroll unchanged.
const SCALAR: &str = "inline,simplify-cfg,canonicalize,instcombine,call-contracts,\
    simplify-cfg,gvn,stack-forward,memory-forward,canonicalize,instcombine,dce,gcm";

fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir.find(&format!("func @{name}(")).unwrap();
    let end = ir[start..].find("\n}").unwrap();
    &ir[start..start + end]
}

#[test]
fn c_hot_loops_unroll_once_and_match_the_driver() {
    let dir = keel_test_support::TempDir::new("loop-unroll-c");
    let dir = dir.path();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);
    for name in ["hot.c", "unsupported.c", "driver.c"] {
        fs::copy(fixtures.join(name), dir.join(name)).unwrap();
    }
    let opt = |input: &str, output: &str, passes: &[&str]| {
        let mut args = vec!["opt", input, "-o", output];
        args.extend(passes);
        successful(dir, &args);
        fs::read_to_string(dir.join(output)).unwrap()
    };
    successful(
        dir,
        &[
            "cc",
            "--emit-ir",
            "-p",
            SCALAR,
            "hot.c",
            "-o",
            "before.keel",
        ],
    );
    let before = fs::read_to_string(dir.join("before.keel")).unwrap();
    let after = opt("before.keel", "unroll.keel", &["-p", "loop-unroll"]);
    let disabled = opt(
        "before.keel",
        "disabled.keel",
        &["-p", "loop-unroll", "-fno-unroll-loops"],
    );
    let enabled = opt(
        "before.keel",
        "enabled.keel",
        &["-p", "loop-unroll", "-fno-unroll-loops", "-funroll-loops"],
    );
    for name in ["hot32", "hot64"] {
        assert_eq!(function(&disabled, name).matches(" = load.").count(), 1);
        assert_eq!(function(&enabled, name).matches(" = load.").count(), 3);
    }
    for name in ["hot32", "hot64"] {
        let (original, unrolled) = (function(&before, name), function(&after, name));
        assert_eq!(original.matches(" = load.").count(), 1, "{original}");
        assert_eq!(unrolled.matches(" = load.").count(), 3, "{unrolled}");
        assert!(
            unrolled.contains("icmp.uge") && unrolled.contains(" = isub "),
            "{unrolled}"
        );
    }
    // Parsing renumbers values, so compare with a parse-and-print baseline.
    assert_eq!(
        opt("unroll.keel", "twice.keel", &["-p", "loop-unroll"]),
        opt(
            "unroll.keel",
            "unroll-printed.keel",
            &["--opt-tier", "none"]
        )
    );
    successful(
        dir,
        &[
            "cc",
            "--emit-ir",
            "-p",
            SCALAR,
            "unsupported.c",
            "-o",
            "unsupported.keel",
        ],
    );
    assert_eq!(
        opt(
            "unsupported.keel",
            "unsupported-unroll.keel",
            &["-p", "loop-unroll"]
        ),
        opt(
            "unsupported.keel",
            "unsupported-printed.keel",
            &["--opt-tier", "none"]
        )
    );
    if !cfg!(any(
        all(target_arch = "aarch64", target_os = "macos"),
        all(target_arch = "x86_64", target_os = "linux")
    )) {
        keel_test_support::skip!("the driver runs only on arm64 macOS and x86-64 Linux");
    }
    for variant in ["before", "unroll"] {
        let (input, object) = (format!("{variant}.keel"), format!("{variant}.o"));
        successful(
            dir,
            &[
                "compile",
                "-O",
                "--opt-tier",
                "none",
                "--emit-object",
                &input,
                "-o",
                &object,
            ],
        );
        let link = Command::new("clang")
            .current_dir(dir)
            .args(["-O2", "driver.c", &object, "-o", variant])
            .output()
            .unwrap();
        assert!(
            link.status.success(),
            "{}",
            String::from_utf8_lossy(&link.stderr)
        );
        let run = Command::new(dir.join(variant)).output().unwrap();
        assert!(
            run.status.success(),
            "{variant}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(String::from_utf8_lossy(&run.stdout).contains("cases passed"));
    }
}
