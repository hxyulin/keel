//! Post-capture stack caching: unknown initializer and aliases observe every write.
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};
const IR: &str = include_str!("../../keel-opt/tests/stack_cache/fields.keel");
const C: &str = include_str!("../../keel-opt/tests/stack_cache/oracle.c");
fn run(cmd: &mut Command) -> String {
    let o = cmd.output().unwrap();
    assert!(
        o.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}
fn keel(dir: &Path, args: &[&str]) -> String {
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args))
}
fn text(p: &Path) -> Vec<u8> {
    let b = fs::read(p).unwrap();
    let f = object::File::parse(b.as_slice()).unwrap();
    f.sections()
        .find(|s| matches!(s.name(), Ok(".text" | "__text")))
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn variants(dir: &Path) -> Vec<&'static str> {
    fs::write(dir.join("before.keel"), IR).unwrap();
    keel(
        dir,
        &[
            "opt",
            "--verify-each",
            "-p",
            "stack-cache,phi-simplify,dce",
            "before.keel",
            "-o",
            "cached.keel",
        ],
    );
    for tier in ["fast", "full"] {
        keel(
            dir,
            &[
                "opt",
                "--verify-each",
                "--opt-tier",
                tier,
                "cached.keel",
                "-o",
                &format!("{tier}.keel"),
            ],
        );
    }
    vec!["before", "cached", "fast", "full"]
}
#[test]
fn both_targets_preserve_object_routes_and_cached_memory_operations() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stack-cache-emission");
    fs::create_dir_all(&dir).unwrap();
    for v in variants(&dir) {
        for (target, isel) in [
            ("aarch64-apple-darwin", "none"),
            ("aarch64-apple-darwin", "all"),
            ("x86_64-unknown-linux-gnu", "none"),
            ("x86_64-unknown-linux-gnu", "all"),
        ] {
            let stem = format!("{v}-{target}-{isel}");
            let source = format!("{v}.keel");
            keel(
                &dir,
                &[
                    "compile",
                    "--target",
                    target,
                    "--isel",
                    isel,
                    "--emit-object",
                    &source,
                    "-o",
                    &format!("{stem}.o"),
                ],
            );
            keel(
                &dir,
                &[
                    "compile",
                    "--target",
                    target,
                    "--isel",
                    isel,
                    &source,
                    "-o",
                    &format!("{stem}.s"),
                ],
            );
            keel(
                &dir,
                &[
                    "asm",
                    "--target",
                    target,
                    &format!("{stem}.s"),
                    "-o",
                    &format!("{stem}-text.o"),
                ],
            );
            assert_eq!(
                text(&dir.join(format!("{stem}.o"))),
                text(&dir.join(format!("{stem}-text.o")))
            );
        }
    }
}
#[test]
fn native_alias_zero_trip_and_wrapping_oracle() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stack-cache-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), C).unwrap();
    run(Command::new("clang")
        .current_dir(&dir)
        .args(["-O2", "-fno-lto", "-c", "oracle.c", "-o", "oracle.o"]));
    for v in variants(&dir) {
        for isel in ["none", "all"] {
            for allocation in ["fast", "full"] {
                // The unoptimized input is a control for the oracle; one
                // backend is enough for it.
                if v == "before" && (isel, allocation) != ("all", "full") {
                    continue;
                }
                let source = format!("{v}.keel");
                let stem = format!("{v}-{isel}-{allocation}");
                keel(
                    &dir,
                    &[
                        "compile",
                        "--isel",
                        isel,
                        "--regalloc",
                        allocation,
                        "--emit-object",
                        &source,
                        "-o",
                        &format!("{stem}.o"),
                    ],
                );
                run(Command::new("clang").current_dir(&dir).args([
                    "oracle.o",
                    &format!("{stem}.o"),
                    "-o",
                    &stem,
                ]));
                assert_eq!(
                    run(&mut Command::new(dir.join(stem))).trim(),
                    "448 alias/disjoint, zero-trip and wrapping cases passed"
                );
            }
        }
    }
}
