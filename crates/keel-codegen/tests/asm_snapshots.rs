//! Assembly snapshots: each `tests/programs/*.keel` is compiled for AArch64
//! macOS and x86-64 Linux, and the assembly (or the error) is compared with
//! a file next to the program:
//!
//! - `<stem>.<target>.s`: the default (`-O0`) covers selection, no IR
//!   optimization.
//! - `<stem>.<target>.opt.s`: the program's `; opt-passes:` list or the
//!   default optimization pipeline, then every selection pass. These are the
//!   unspilled default and optimized configurations that `run.rs` executes.
//!
//! `<target>` is `aarch64` or `x86_64`. A `; target:` line limits a program to
//! that target. The DWARF compilation directory is written as `<cwd>`. Nothing
//! is assembled or run, so every snapshot is checked on any host.
//!
//! `BLESS=1 cargo test -p keel-codegen --test asm_snapshots` rewrites the
//! snapshots and removes ones whose program is gone.

use keel_codegen::regalloc::AllocationTier;
use keel_codegen::{CodegenPipeline, OptionsSelector, SelectionOptions, Target};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const TARGETS: [(Target, &str); 2] = [
    (Target::Aarch64AppleDarwin, "aarch64"),
    (Target::X86_64LinuxGnu, "x86_64"),
];

fn optimize(source: &str, module: &mut keel_ir::Module) -> Result<(), String> {
    match source
        .lines()
        .find_map(|line| line.strip_prefix("; opt-passes:"))
    {
        Some(names) => {
            let names: Vec<_> = names.split(',').map(str::trim).collect();
            keel_opt::run_passes(module, &names).map_err(|e| e.to_string())
        }
        None => keel_opt::optimize(module).map_err(|e| e.to_string()),
    }
}

/// Snapshot file name to contents for one program.
fn render(path: &Path, cwd: &str) -> BTreeMap<PathBuf, String> {
    let source = std::fs::read_to_string(path).unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy();
    let only = source.lines().find_map(|l| l.strip_prefix("; target: "));
    let raw = keel_ir::parse(&source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    keel_ir::verify_module(&raw).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
    let mut optimized = raw.clone();
    let optimized = optimize(&source, &mut optimized).map(|()| optimized);

    let mut out = BTreeMap::new();
    for (target, name) in TARGETS {
        if only.is_some_and(|t| t != target.triple()) {
            continue;
        }
        for (suffix, module, selection) in [
            ("s", Ok(&raw), SelectionOptions::default()),
            ("opt.s", optimized.as_ref(), SelectionOptions::ALL),
        ] {
            let selector = OptionsSelector(selection);
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let text = match module {
                Ok(module) => {
                    let text = keel_codegen::compile_with_pipeline(module, target, &pipeline);
                    let mut streamed = String::new();
                    let chunks = keel_codegen::Request::new(target, pipeline)
                        .text_chunks(module, &mut |chunk| streamed.push_str(chunk));
                    assert_eq!(chunks.is_ok(), text.is_ok(), "{}", path.display());
                    if let Ok(text) = &text {
                        assert!(
                            streamed == *text,
                            "{}: streamed text differs",
                            path.display()
                        );
                    }
                    text.map(|asm| asm.replace(cwd, "\"<cwd>\""))
                        .unwrap_or_else(|e| format!("error: {e}\n"))
                }
                Err(e) => format!("optimization error: {e}\n"),
            };
            out.insert(path.with_file_name(format!("{stem}.{name}.{suffix}")), text);
        }
    }
    out
}

/// The first differing line, with its number, keeps failures readable.
fn first_difference(expected: &str, actual: &str) -> String {
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    for line in 1.. {
        match (expected_lines.next(), actual_lines.next()) {
            (None, None) => break,
            (e, a) if e == a => {}
            (e, a) => {
                return format!(
                    "line {line}\n  expected: {}\n  actual:   {}",
                    e.unwrap_or("<end of file>"),
                    a.unwrap_or("<end of file>")
                );
            }
        }
    }
    "trailing newline".into()
}

#[test]
fn asm_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let bless = std::env::var_os("BLESS").is_some();
    let mut programs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty(), "no programs in {}", dir.display());

    // Codegen records the process directory as the DWARF compilation directory.
    let cwd = format!("\"{}\"", std::env::current_dir().unwrap().display());
    let mut expected = BTreeMap::new();
    for program in &programs {
        expected.extend(render(program, &cwd));
    }
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "s") && !expected.contains_key(&path) {
            if bless {
                std::fs::remove_file(&path).unwrap();
            } else {
                let name = path.file_name().unwrap().to_string_lossy();
                failures.push(format!("{name}: no program"));
            }
        }
    }
    for (path, actual) in &expected {
        if bless {
            std::fs::write(path, actual).unwrap();
            continue;
        }
        let snapshot = std::fs::read_to_string(path).unwrap_or_default();
        if snapshot != *actual {
            failures.push(format!(
                "{}: {}",
                path.file_name().unwrap().to_string_lossy(),
                first_difference(&snapshot, actual)
            ));
        }
    }
    failures.sort();
    assert!(
        failures.is_empty(),
        "{} assembly snapshot(s) differ (BLESS=1 to update):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
