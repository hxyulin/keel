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
            if !binding && matches!(name, "inline-noninterposable" | "inline-cheap-reserve") {
                "inline"
            } else {
                name
            }
        })
        .collect();
    if binding
        && !names
            .iter()
            .any(|n| matches!(*n, "inline-noninterposable" | "inline-cheap-reserve"))
    {
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

#[test]
fn c_binding_flags_preserve_defaults_order_exports_and_noinline() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-function-binding-flags");
    fs::create_dir_all(&dir).unwrap();
    let source = "int helper(int x) { return x+1; } int caller(int x) { return helper(x); } __attribute__((noinline)) int stay(int x) { return x+2; } int stay_call(int x) { return stay(x); }";
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("source.i"), source).unwrap();
    for target in [
        "aarch64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
    ] {
        for (index, flags, admitted) in [
            (0, vec![], false),
            (1, vec!["-fno-semantic-interposition"], true),
            (2, vec!["-fsemantic-interposition"], false),
            (
                3,
                vec!["-fno-semantic-interposition", "-fsemantic-interposition"],
                false,
            ),
            (
                4,
                vec!["-fsemantic-interposition", "-fno-semantic-interposition"],
                true,
            ),
        ] {
            let mut args = vec!["cc", "--target", target, "-O", "--emit-ir", "source.c"];
            args.extend(&flags);
            let ir = keel(&dir, &args);
            assert_eq!(function(&ir, "caller").contains("call @helper("), !admitted);
            assert!(function(&ir, "stay_call").contains("call @stay("));
            let m = keel_ir::parse(&ir).unwrap();
            keel_ir::verify_module(&m).unwrap();
            let helper = m.functions.iter().find(|f| f.name == "helper").unwrap();
            assert_eq!(helper.attrs.no_semantic_interposition, admitted);
            assert!(!helper.attrs.noninterposable);
            assert!(!helper.local);
            assert!(!helper.hidden);
            // Print/parse must preserve the promise for separate IR compilation.
            fs::write(dir.join("source.keel"), ir).unwrap();
            let obj = format!("{target}-{index}.o");
            keel(
                &dir,
                &[
                    "compile",
                    "--target",
                    target,
                    "--emit-object",
                    "source.keel",
                    "-o",
                    &obj,
                ],
            );
            let symbols = global_functions(&dir.join(&obj));
            assert!(symbols.iter().any(|s| s.ends_with("helper")));
            assert!(symbols.iter().any(|s| s.ends_with("caller")));
        }
    }
    for args in [
        vec!["opt", "-fno-semantic-interposition", "missing.keel"],
        vec!["compile", "-fsemantic-interposition", "missing.keel"],
        vec![
            "cc",
            "--target",
            "wasm32-unknown-unknown",
            "-fno-semantic-interposition",
            "source.c",
        ],
        vec![
            "cc",
            "--target",
            "wasm32-unknown-unknown",
            "-fsemantic-interposition",
            "source.c",
        ],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn c_binding_mode_preserves_native_abi_effects_and_faults() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-function-binding-native");
    fs::create_dir_all(&dir).unwrap();
    // Exercise the new mode rather than the stronger source-level promise.
    fs::write(
        dir.join("source.c"),
        SOURCE.replace(
            "#define BIND __attribute__((keel_noninterposable))",
            "#define BIND",
        ),
    )
    .unwrap();
    fs::write(dir.join("oracle.c"), ORACLE).unwrap();
    for (index, flags) in [
        vec!["-fsemantic-interposition"],
        vec!["-fno-semantic-interposition"],
        vec!["--link-intent", "executable"],
        vec!["--link-intent", "shared"],
    ]
    .into_iter()
    .enumerate()
    {
        let obj = format!("source-{index}.o");
        let exe = format!("oracle-{index}");
        let mut args = vec!["cc", "-O", "--emit-object", "source.c", "-o", &obj];
        args.extend(flags);
        keel(&dir, &args);
        command(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "-fno-strict-aliasing",
            "oracle.c",
            &obj,
            "-o",
            &exe,
        ]));
        command(&mut Command::new(dir.join(&exe)));
        command(Command::new(dir.join(&exe)).arg("faults"));
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn elf_hidden_inlining_preserves_public_interposition() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-function-binding-preload");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), "int public_helper(int x) { return x+1; } __attribute__((visibility(\"hidden\"))) int hidden_helper(int x) { return x+2; } int public_call(int x) { return public_helper(x); } int hidden_call(int x) { return hidden_helper(x); }").unwrap();
    fs::write(
        dir.join("override.c"),
        "int public_helper(int x) { return x+100; } int hidden_helper(int x) { return x+200; }",
    )
    .unwrap();
    fs::write(dir.join("main.c"), "int public_call(int); int hidden_call(int); int main(void) { return public_call(10)==110 && hidden_call(10)==12 ? 0 : 1; }").unwrap();
    keel(
        &dir,
        &["cc", "-O", "--emit-object", "source.c", "-o", "source.o"],
    );
    command(Command::new("clang").current_dir(&dir).args([
        "-shared",
        "source.o",
        "-o",
        "library.so",
    ]));
    command(Command::new("clang").current_dir(&dir).args([
        "-shared",
        "-fPIC",
        "override.c",
        "-o",
        "override.so",
    ]));
    command(Command::new("clang").current_dir(&dir).args([
        "main.c",
        "./library.so",
        "-Wl,-rpath,$ORIGIN",
        "-o",
        "oracle",
    ]));
    command(
        Command::new(dir.join("oracle"))
            .current_dir(&dir)
            .env("LD_PRELOAD", dir.join("override.so")),
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn semantic_mode_keeps_interposed_function_addresses() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-function-binding-addresses");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), "int helper(int x) { return x+1; } int caller(int x) { return helper(x); } int (*address)(int)=helper;").unwrap();
    // Equivalent replacement is allowed by this mode. Its address must still win.
    fs::write(dir.join("override.c"), "int helper(int x) { return 1+x; }").unwrap();
    fs::write(dir.join("main.c"), "int helper(int); int caller(int); extern int (*address)(int); int main(void) { return address==helper && caller(10)==11 && address(10)==11 ? 0 : 1; }").unwrap();
    keel(
        &dir,
        &[
            "cc",
            "-O",
            "-fno-semantic-interposition",
            "--emit-object",
            "source.c",
            "-o",
            "source.o",
        ],
    );
    command(Command::new("clang").current_dir(&dir).args([
        "-shared",
        "source.o",
        "-o",
        "library.so",
    ]));
    command(Command::new("clang").current_dir(&dir).args([
        "-shared",
        "-fPIC",
        "override.c",
        "-o",
        "override.so",
    ]));
    command(Command::new("clang").current_dir(&dir).args([
        "main.c",
        "./library.so",
        "-Wl,-rpath,$ORIGIN",
        "-o",
        "oracle",
    ]));
    command(
        Command::new(dir.join("oracle"))
            .current_dir(&dir)
            .env("LD_PRELOAD", dir.join("override.so")),
    );
}

#[test]
fn executable_link_intent_selects_binding_without_changing_output_kind() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-link-intent");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.i"), "int helper(int x) { return x+1; } __attribute__((weak)) int weak_helper(int x) { return x+2; } int caller(int x) { return helper(x)+weak_helper(x); }").unwrap();
    for target in [
        "aarch64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
    ] {
        for (flags, inlined) in [
            (vec!["--link-intent", "executable"], true),
            (vec!["--link-intent", "shared"], false),
            (
                vec!["--link-intent", "executable", "-fsemantic-interposition"],
                false,
            ),
            (
                vec!["-fsemantic-interposition", "--link-intent", "executable"],
                false,
            ),
            (
                vec!["--link-intent", "shared", "-fno-semantic-interposition"],
                true,
            ),
            (
                vec!["--link-intent", "executable", "--link-intent", "shared"],
                false,
            ),
            (
                vec!["--link-intent", "shared", "--link-intent", "executable"],
                true,
            ),
        ] {
            let mut args = vec!["cc", "--target", target, "-O", "--emit-ir", "source.i"];
            args.extend(flags.clone());
            let ir = keel(&dir, &args);
            assert_eq!(function(&ir, "caller").contains("call @helper("), !inlined);
            assert!(function(&ir, "caller").contains("call @weak_helper("));
            let mut args = vec![
                "cc",
                "--target",
                target,
                "-O",
                "--emit-object",
                "source.i",
                "-o",
                "source.o",
            ];
            args.extend(flags);
            keel(&dir, &args);
            let symbols = global_functions(&dir.join("source.o"));
            assert!(symbols.iter().any(|s| s.ends_with("helper")));
            assert!(symbols.iter().any(|s| s.ends_with("caller")));
        }
    }
    for flags in [
        vec!["cc", "--link-intent"],
        vec!["cc", "--link-intent", "invalid", "source.i"],
        vec!["compile", "--link-intent", "executable", "source.i"],
        vec![
            "cc",
            "--target",
            "wasm32-unknown-unknown",
            "--link-intent",
            "executable",
            "source.i",
        ],
        vec![
            "cc",
            "--target",
            "wasm32-unknown-unknown",
            "--link-intent",
            "shared",
            "source.i",
        ],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args(flags)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
