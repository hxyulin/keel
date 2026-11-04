use super::{invoke, successful};
use std::{fs, path::Path};

const C: &str = "static int scale(int x) { return x * 3 + 1; }
int sum(const int *a, int n) { int s = 0; for (int i = 0; i < n; i++) s += scale(a[i]); return s; }
int pick(int x) { return x > 4 ? sum(&x, 1) : 0; }";

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

/// Numbers values as a parse does, so in-memory and reparsed runs compare.
fn normalized(ir: &str) -> String {
    keel_ir::parse(ir).unwrap().to_string()
}

/// The command, then the target, then the remaining arguments.
fn command<'a>(target: &[&'a str], args: &[&'a str]) -> Vec<&'a str> {
    let mut all = vec![args[0]];
    all.extend_from_slice(target);
    all.extend_from_slice(&args[1..]);
    all
}

#[test]
fn opt_runs_what_cc_dash_o_runs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("opt-controls-default");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), C).unwrap();
    let host = keel_ir::Target::host();
    for target in [None, Some("x86_64-unknown-linux-gnu")] {
        if target.is_none() && host.is_none() {
            continue;
        }
        let target: &[&str] = match target {
            Some(triple) => &["--target", triple],
            None => &[],
        };
        let with = |args: &[&str]| normalized(&text(successful(&dir, &command(target, args))));
        let raw = with(&["cc", "-O0", "--emit-ir", "in.c"]);
        fs::write(dir.join("raw.keel"), raw).unwrap();
        let cc = with(&["cc", "-O", "--emit-ir", "in.c"]);
        assert_eq!(with(&["opt", "raw.keel"]), cc, "target {target:?}");
        assert_eq!(with(&["opt", "-O3", "raw.keel"]), cc, "target {target:?}");
    }
}

#[test]
fn bisect_disable_and_print_changed_flags() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("opt-controls-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), C).unwrap();
    let base = ["--target", "x86_64-unknown-linux-gnu"];
    let run = |args: &[&str]| {
        let output = invoke(&dir, &command(&base, args));
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        (text(output.stdout), text(output.stderr))
    };
    let (raw, _) = run(&["cc", "-O0", "--emit-ir", "in.c"]);
    fs::write(dir.join("raw.keel"), &raw).unwrap();
    let (unoptimized, _) = run(&["opt", "-O0", "raw.keel"]);

    // The verbose trace numbers every invocation that runs.
    let (full, trace) = run(&["opt", "--opt-bisect-verbose", "raw.keel"]);
    let lines: Vec<&str> = trace.lines().collect();
    assert!(lines.len() > 20, "{trace}");
    for (n, line) in lines.iter().enumerate() {
        assert!(line.starts_with(&format!("bisect: {} ", n + 1)), "{line}");
    }
    assert!(trace.contains(" gvn @sum\n"), "{trace}");
    assert_eq!(run(&["opt", "raw.keel"]).0, full);
    assert_eq!(
        run(&[
            "opt",
            "--opt-bisect-limit",
            &lines.len().to_string(),
            "raw.keel"
        ])
        .0,
        full
    );
    let (none, trace) = run(&["opt", "--opt-bisect-limit", "0", "raw.keel"]);
    assert_eq!(none, unoptimized);
    assert!(trace.is_empty());
    let (_, trace) = run(&[
        "opt",
        "--opt-bisect-limit",
        "7",
        "--opt-bisect-verbose",
        "raw.keel",
    ]);
    assert_eq!(trace.lines().collect::<Vec<_>>(), lines[..7]);

    // Disabling every pass of a pipeline leaves the input as it was.
    let passes = "simplify-cfg,canonicalize,instcombine,dce";
    let (disabled, _) = run(&["opt", "-p", passes, "--opt-disable", passes, "raw.keel"]);
    assert_eq!(disabled, unoptimized);
    let (some, trace) = run(&[
        "opt",
        "-p",
        passes,
        "--opt-disable",
        "instcombine",
        "--opt-disable",
        "dce",
        "--opt-bisect-verbose",
        "raw.keel",
    ]);
    assert_ne!(some, unoptimized);
    assert!(!trace.contains(" instcombine ") && !trace.contains(" dce "));
    assert!(trace.contains(" canonicalize @sum\n"), "{trace}");

    // Each changed function prints after its invocation; the filter selects
    // by name. This pipeline changes @sum and leaves @pick as it is.
    let (_, trace) = run(&["opt", "-p", passes, "--print-changed", "raw.keel"]);
    assert!(
        trace.starts_with("; after simplify-cfg, invocation 1\nfunc @sum("),
        "{trace}"
    );
    assert!(
        trace.contains("; after instcombine, invocation "),
        "{trace}"
    );
    assert!(!trace.contains("func @pick("), "{trace}");
    let filtered = |name| {
        run(&[
            "opt",
            "-p",
            passes,
            "--print-changed",
            "--opt-function",
            name,
            "raw.keel",
        ])
        .1
    };
    assert_eq!(filtered("sum"), trace);
    assert!(filtered("pick").is_empty());

    for (args, error) in [
        (
            vec!["opt", "--opt-disable", "missing", "raw.keel"],
            "unknown pass `missing`",
        ),
        (
            vec!["opt", "--opt-bisect-limit", "x", "raw.keel"],
            "`--opt-bisect-limit` needs",
        ),
        (
            vec!["opt", "--opt-function", "pick", "raw.keel"],
            "needs `--print-changed`",
        ),
    ] {
        let all = command(&base, &args);
        let output = invoke(&dir, &all);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(error), "{all:?}: {stderr}");
    }
}
