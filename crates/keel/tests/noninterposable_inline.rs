//! Explicit exported-definition inlining: both-target emission and native ABI oracle.
use object::{Object, ObjectSymbol, SymbolKind};
use std::{fs, path::Path, process::Command};

fn global_functions(path: &Path) -> Vec<String> {
    let bytes = fs::read(path).unwrap();
    let object = object::File::parse(bytes.as_slice()).unwrap();
    object
        .symbols()
        .filter(|s| s.is_global() && s.is_definition() && s.kind() == SymbolKind::Text)
        .map(|s| s.name().unwrap().to_owned())
        .collect()
}

const SOURCE: &str = include_str!("fixtures/noninterposable.c");
const ORACLE: &str = include_str!("fixtures/noninterposable_oracle.c");
fn command(c: &mut Command) -> String {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}: {}\n{}\n{}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}
fn keel(dir: &Path, args: &[&str]) -> String {
    command(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(dir)
            .args(args),
    )
}
fn pipeline(tier: keel_opt::OptimizationTier, binding: bool) -> String {
    // The negative control disables the binding promise while retaining all
    // other default passes and the same number of inliner invocations.
    let mut names: Vec<_> = tier
        .pipeline()
        .iter()
        .copied()
        .map(|name| {
            if !binding && name == "inline-noninterposable" {
                "inline"
            } else {
                name
            }
        })
        .collect();
    if binding && !names.contains(&"inline-noninterposable") {
        names.insert(0, "inline-noninterposable");
    }
    names.join(",")
}
fn function<'a>(text: &'a str, name: &str) -> &'a str {
    text.split(&format!("func @{name}("))
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap()
}
#[test]
fn explicit_pass_removes_narrow_calls_on_both_targets_and_retains_exports() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("noninterposable-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for tier in [
            keel_opt::OptimizationTier::Fast,
            keel_opt::OptimizationTier::Full,
        ] {
            let before = pipeline(tier, false);
            let after = pipeline(tier, true);
            let original = keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "-p",
                    &before,
                    "--emit-ir",
                    "source.c",
                ],
            );
            let optimized = keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "-p",
                    &after,
                    "--emit-ir",
                    "source.c",
                ],
            );
            for name in ["clamp_i8", "mix_u16"] {
                assert!(function(&original, "k_narrow").contains(&format!("call @{name}(")));
                assert!(!function(&optimized, "k_narrow").contains(&format!("call @{name}(")));
                let header = optimized
                    .lines()
                    .find(|l| l.starts_with(&format!("func @{name}(")))
                    .unwrap();
                assert!(header.contains("noninterposable"));
                assert!(!header.contains("local"));
            }
            assert!(function(&optimized, "stay_call").contains("call @stay("));
            assert!(function(&original, "classify_call").contains("call @classify("));
            assert!(!function(&optimized, "classify_call").contains("call @classify("));
            assert!(!function(&optimized, "aggregate").contains("call @adjust("));
            assert!(!function(&optimized, "effects_call").contains("call @effects("));
            assert!(!function(&optimized, "capture_call").contains("call @captured("));
            assert!(!function(&optimized, "fault_call").contains("call @ordered_fault("));
            let tag = format!("{target}-{tier:?}");
            fs::write(dir.join(format!("{tag}-before.keel")), &original).unwrap();
            fs::write(dir.join(format!("{tag}-after.keel")), &optimized).unwrap();
            for (stage, passes) in [("before", &before), ("after", &after)] {
                let name = format!("{tag}-{stage}.s");
                keel(
                    &dir,
                    &[
                        "cc", "--target", target, "-p", passes, "source.c", "-o", &name,
                    ],
                );
                let asm = fs::read_to_string(dir.join(&name)).unwrap();
                let call = if target.starts_with("aarch64") {
                    "\tbl\t_"
                } else {
                    "\tcall\t\""
                };
                for helper in ["clamp_i8", "mix_u16", "classify"] {
                    assert_eq!(
                        asm.contains(&format!("{call}{helper}")),
                        stage == "before",
                        "{tag}-{stage}: {asm}"
                    );
                    assert!(
                        asm.lines()
                            .any(|l| l.contains(".globl") && l.contains(helper))
                    );
                }
                let obj = format!("{tag}-{stage}.o");
                keel(
                    &dir,
                    &[
                        "cc",
                        "--target",
                        target,
                        "-p",
                        passes,
                        "--emit-object",
                        "source.c",
                        "-o",
                        &obj,
                    ],
                );
                assert!(fs::metadata(dir.join(&obj)).unwrap().len() > 0);
                let symbols = global_functions(&dir.join(&obj));
                for helper in ["clamp_i8", "mix_u16", "classify"] {
                    assert!(symbols.iter().any(|l| l.ends_with(helper)), "{symbols:?}");
                }
            }
        }
    }
}
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_independent_abi_alias_wrapping_volatile_atomic_capture_and_fault_oracles() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("noninterposable-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(dir.join("oracle.c"), ORACLE).unwrap();
    command(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-strict-aliasing",
        "source.c",
        "oracle.c",
        "-o",
        "reference",
    ]));
    command(&mut Command::new(dir.join("reference")));
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        for binding in [false, true] {
            let passes = pipeline(tier, binding);
            let tag = format!("{tier:?}-{binding}");
            let obj = format!("{tag}.o");
            keel(
                &dir,
                &[
                    "cc",
                    "-g",
                    "-p",
                    &passes,
                    "--emit-object",
                    "source.c",
                    "-o",
                    &obj,
                ],
            );
            let symbols = global_functions(&dir.join(&obj));
            for name in [
                "clamp_i8",
                "mix_u16",
                "adjust",
                "wrapping",
                "effects",
                "captured",
                "ordered_fault",
                "stay",
                "classify",
            ] {
                assert!(symbols.iter().any(|l| l.ends_with(name)), "{symbols:?}");
            }
            command(
                Command::new("clang")
                    .current_dir(&dir)
                    .args(["-O2", "oracle.c", &obj, "-o", &tag]),
            );
            command(&mut Command::new(dir.join(&tag)));
            command(Command::new(dir.join(&tag)).arg("faults"));
        }
    }
}
