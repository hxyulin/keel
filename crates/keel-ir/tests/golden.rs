//! Golden tests: each `tests/golden/*.keel` is parsed and verified, and the
//! result (printed IR, verifier errors or a parse error) is compared with the
//! matching `.out` file. Run `BLESS=1 cargo test` to rewrite `.out` files.
//!
//! Every file that parses must also round-trip: printing, re-parsing and
//! printing again gives identical text.

use std::fmt::Write;
use std::path::Path;

fn render(src: &str) -> String {
    let funcs = match keel_ir::parse(src) {
        Ok(funcs) => funcs,
        Err(e) => return format!("parse error: {e}\n"),
    };

    let printed: String = funcs.iter().map(|f| f.to_string()).collect();
    let reparsed = keel_ir::parse(&printed)
        .unwrap_or_else(|e| panic!("printed IR does not parse: {e}\n{printed}"));
    let reprinted: String = reparsed.iter().map(|f| f.to_string()).collect();
    assert_eq!(printed, reprinted, "print -> parse -> print is not stable");

    let mut out = String::new();
    for (i, func) in funcs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match keel_ir::verify(func) {
            Ok(()) => out.push_str(&func.to_string()),
            Err(errors) => {
                writeln!(out, "verifier errors in @{}:", func.name).unwrap();
                for e in errors {
                    writeln!(out, "  {e}").unwrap();
                }
            }
        }
    }
    out
}

#[test]
fn golden() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let bless = std::env::var_os("BLESS").is_some();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no golden tests in {}", dir.display());

    let mut failures = Vec::new();
    for path in paths {
        let actual = render(&std::fs::read_to_string(&path).unwrap());
        let out_path = path.with_extension("out");
        if bless {
            std::fs::write(&out_path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&out_path).unwrap_or_default();
        if actual != expected {
            failures.push(format!(
                "{}\n--- expected\n{expected}--- actual\n{actual}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden test(s) differ (BLESS=1 to update):\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}
