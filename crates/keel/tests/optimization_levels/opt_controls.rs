use super::successful;
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
