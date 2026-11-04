//! Differential checks on frontend output: `keel cc --emit-ir` lowers C test
//! programs to IR, and every tier must keep what their exported functions
//! compute, by the rules in `differential.rs`.
//!
//! The normal run lowers the programs in `DEFAULT`. When `KEEL_FUZZ_ITERS`
//! is set, for the qualification lane, it lowers every program under the
//! directories in `QUALIFICATION`. The test needs the `keel` binary named by
//! `KEEL_TEST_KEEL`, which `tools/test.py` builds.

use super::differential::{Config, TIERS, Tally, compare, optimize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Programs, relative to the workspace root, with exported functions that
/// the interpreter can run: most programs only print from `main`, and a call
/// to a declared function such as `printf` ends the run unsupported.
const DEFAULT: [&str; 6] = [
    "crates/keel-c/tests/programs/old_style.c",
    "crates/keel-c/tests/programs/vla.c",
    "crates/keel-c/tests/programs/vm_parameters.c",
    "crates/keel-codegen/tests/programs/globals.c",
    "crates/keel-codegen/tests/programs/i128.c",
    "crates/keel-codegen/tests/programs/structs.c",
];

/// Directories of C programs, relative to the workspace root.
const QUALIFICATION: [&str; 2] = [
    "crates/keel-c/tests/programs",
    "crates/keel-codegen/tests/programs",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources() -> Vec<PathBuf> {
    let root = root();
    if std::env::var_os("KEEL_FUZZ_ITERS").is_none() {
        return DEFAULT.iter().map(|p| root.join(p)).collect();
    }
    let mut paths: Vec<PathBuf> = QUALIFICATION
        .iter()
        .flat_map(|dir| std::fs::read_dir(root.join(dir)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "c"))
        .collect();
    paths.sort();
    paths
}

/// `keel cc --emit-ir` output for `source`.
fn emit_ir(keel: &Path, source: &Path, out: &Path) -> Result<keel_ir::Module, String> {
    let output = Command::new(keel)
        .args(["cc", "--emit-ir"])
        .arg(source)
        .arg("-o")
        .arg(out)
        .output()
        .map_err(|e| format!("{}: {e}", keel.display()))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let text = std::fs::read_to_string(out).unwrap();
    let module = keel_ir::parse(&text).map_err(|e| format!("does not parse: {e}"))?;
    keel_ir::verify_module(&module).map_err(|e| format!("does not verify: {e:?}"))?;
    Ok(module)
}

#[test]
fn tiers_preserve_emitted_c_results() {
    let Some(keel) = std::env::var_os("KEEL_TEST_KEEL") else {
        keel_test_support::skip!("Keel executable not given; set KEEL_TEST_KEEL");
    };
    let keel = PathBuf::from(keel);
    let dir = keel_test_support::TempDir::new("emitted-c");
    let mut failures = Vec::new();
    let mut tally = Tally::default();
    for (seed, source) in sources().iter().enumerate() {
        let name = source.strip_prefix(root()).unwrap().display().to_string();
        let module = match emit_ir(&keel, source, &dir.join(format!("{seed}.keel"))) {
            Ok(module) => module,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        for tier in TIERS {
            let result = optimize(&module, Config::Tier(tier))
                .and_then(|after| compare(&module, &after, seed as u64, &mut tally));
            if let Err(e) = result {
                failures.push(format!("{tier:?} on {name}: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guards against a driver that silently skips everything.
    assert!(tally.compared >= 1_000, "{tally:?}");
}
