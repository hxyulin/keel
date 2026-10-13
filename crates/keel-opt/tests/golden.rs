//! Pass tests: each `tests/golden/*.keel` starts with a `; passes: a, b`
//! line, is parsed, verified, run through those passes, and the printed
//! result (or the pass error) is compared with the `.out` file. The pass
//! manager verifies after every pass, and the output must parse and verify
//! again. Run `BLESS=1 cargo test` to rewrite `.out` files.

use std::path::Path;

fn render(src: &str) -> String {
    let passes: Vec<&str> = src
        .lines()
        .find_map(|l| l.strip_prefix("; passes:"))
        .expect("missing `; passes:` line")
        .split(',')
        .map(str::trim)
        .collect();
    let mut module = keel_ir::parse(src).unwrap_or_else(|e| panic!("input does not parse: {e}"));
    keel_ir::verify_module(&module).unwrap_or_else(|e| panic!("input does not verify: {e:?}"));
    if let Err(e) = keel_opt::run_passes(&mut module, &passes) {
        return format!("error: {e}\n");
    }
    let printed = module.to_string();
    let reparsed = keel_ir::parse(&printed)
        .unwrap_or_else(|e| panic!("optimized IR does not parse: {e}\n{printed}"));
    keel_ir::verify_module(&reparsed)
        .unwrap_or_else(|e| panic!("optimized IR does not verify: {e:?}\n{printed}"));
    printed
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
