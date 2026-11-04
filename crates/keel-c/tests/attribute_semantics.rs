//! Fail closed on selected attributes with unimplemented observable semantics;
//! implement hidden and internal visibility.
use keel_ir::Target;

const WEAK: &str = "weak applies only to functions and variables with external linkage";

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "attributes.c",
        keel_c::Options {
            target,
            debug_info: false,
            ..Default::default()
        },
    )
}

#[test]
fn linkage_and_lifetime_attributes_are_diagnosed_on_declarations_and_redeclarations() {
    for target in Target::NATIVE {
        for (attribute, args) in [
            ("weakref", "(\"implementation\")"),
            ("weak_import", ""),
            ("weak_odr", ""),
            ("weak_unknown", ""),
            ("ifunc", "(\"resolver\")"),
        ] {
            for name in [attribute.to_string(), format!("__{attribute}__")] {
                let attr = format!("__attribute__(({name}{args}))");
                for source in [
                    format!("{attr} int f(void);"),
                    format!("int f(void) {attr};"),
                    format!("int f(void) {attr} {{ return 7; }}"),
                    format!("int f(void), g(void) {attr};"),
                    format!("int f(void); int f(void) {attr};"),
                    format!("int f(void) {{ return 7; }} int f(void) {attr};"),
                    format!("int g(void) {{ extern int f(void) {attr}; return f(); }}"),
                ] {
                    let error = compile(&source, target).unwrap_err();
                    assert_eq!(
                        error.message,
                        format!(
                            "attribute `{attribute}` requires semantics that are not supported"
                        ),
                        "{source}"
                    );
                }
            }
        }
    }
}

#[test]
fn cleanup_is_accepted_but_undefined_retention_stays_diagnosed() {
    for target in Target::NATIVE {
        for attr in ["cleanup(release)", "__cleanup__(release)"] {
            for source in [
                format!(
                    "void release(int *); int f(void) {{ int x __attribute__(({attr})) = 1; return x; }}"
                ),
                format!(
                    "void release(int *); static inline int f(void) {{ __attribute__(({attr})) int x = 1; return x; }}"
                ),
            ] {
                compile(&source, target).unwrap();
            }
        }
        for attr in ["alias(\"other\")", "retain"] {
            let source = format!(
                "extern int other; extern int value; extern int value __attribute__(({attr}));"
            );
            assert!(compile(&source, target).is_err(), "{source}");
        }
        let section = if target == Target::Aarch64AppleDarwin {
            "__DATA,__registry"
        } else {
            "registry"
        };
        compile(
            &format!("extern int value __attribute__((section(\"{section}\")));"),
            target,
        )
        .unwrap();
    }
}

#[test]
fn weak_declarations_merge_and_invalid_placements_are_rejected() {
    for target in Target::NATIVE {
        for name in ["weak", "__weak__"] {
            let attr = format!("__attribute__(({name}))");
            // Nothing is emitted for these, so weak binding is unobservable.
            for source in [
                format!("{attr} int f(void);"),
                format!("int f(void) {attr};"),
                format!("int f(void), g(void) {attr};"),
                format!("int f(void); int f(void) {attr};"),
                format!("int f(void) {attr}; int f(void);"),
                format!("extern int v {attr};"),
                format!("extern int v; extern int v {attr};"),
                format!("int g(void) {{ extern int f(void) {attr}; return 0; }}"),
                format!("extern void u(void *) __attribute__((__noreturn__)) {attr};"),
                // Earlier references that are never emitted.
                format!("int f(void); static int g(void) {{ return f(); }} int f(void) {attr};"),
            ] {
                let module = compile(&source, target).unwrap();
                assert!(
                    module.decls.iter().all(|d| d.name != "f" && d.name != "u")
                        && module.data.iter().all(|d| d.name != "v"),
                    "{source}"
                );
                keel_codegen::compile(&module, target).unwrap();
            }
            // Definitions, before or after, and references of any kind.
            for source in [
                format!("int f(void) {attr} {{ return 7; }}"),
                format!("{attr} int f(void) {{ return 7; }}"),
                format!("int f(void) {attr}; int f(void) {{ return 7; }}"),
                format!("int f(void) {{ return 7; }} int f(void) {attr};"),
                format!("int f(void), g(void) {attr}; int g(void) {{ return 1; }}"),
                format!("int f(void) {attr}; int g(void) {{ return f(); }}"),
                format!("int f(void) {attr}; int (*p)(void) = f;"),
                format!("int f(void) {attr}; int (*g(void))(void) {{ return &f; }}"),
                format!("int f(void); int g(void) {{ return f(); }} int f(void) {attr};"),
                format!("int g(void) {{ extern int f(void) {attr}; return f(); }}"),
                format!(
                    "int g(void) {{ extern int f(void) {attr}; return 0; }} int h(void) {{ return f(); }}"
                ),
                format!("int v {attr};"),
                format!("int v {attr} = 1;"),
                format!("extern int v {attr}; int v;"),
                format!("extern int v {attr}; int v = 1;"),
                format!("int v; extern int v {attr};"),
                format!("extern int v {attr}; int *p = &v;"),
                format!("extern int v {attr}; int g(void) {{ return v; }}"),
                format!("extern int v {attr}; void g(void) {{ v = 1; }}"),
                format!("extern int v {attr}; int g(void) {{ return sizeof(int[v]); }}"),
            ] {
                let module = compile(&source, target).unwrap();
                keel_ir::verify_module(&module).unwrap();
                keel_codegen::compile(&module, target).unwrap();
            }
            for source in [
                format!("void g(void) {{ static int x {attr}; }}"),
                format!("static int f(void) {attr}; static int f(void) {{ return 1; }}"),
                // Placements that declare no symbol.
                format!("void g(void) {{ int x {attr}; }}"),
                format!("typedef int T {attr};"),
                format!("struct S {{ int x {attr}; }};"),
                format!("{attr} struct S {{ int x; }};"),
                format!("void g(int x {attr});"),
                format!("int g(void) {{ return (int {attr})1; }}"),
            ] {
                let error = compile(&source, target).unwrap_err();
                assert!(
                    error.message == WEAK || error.message.contains("weak requires"),
                    "{source}: {error}"
                );
            }
        }
        for source in [
            "int f(void) __attribute__((weak()));",
            "int f(void) __attribute__((weak(1)));",
        ] {
            assert!(compile(source, target).is_err(), "{source}");
        }
    }
}

#[test]
fn visibility_rejects_protected_and_unknown_values_and_preserves_error_location() {
    for target in Target::NATIVE {
        for visibility in ["protected", "unknown", ""] {
            for declaration in ["int f(void);", "int f(void) { return 1; }"] {
                let source = format!(
                    "# 42 \"library.h\"\n{declaration}\nint f(void) __attribute__((__visibility__(\"{visibility}\"))); "
                );
                let error = compile(&source, target).unwrap_err();
                assert!(error.message.contains("non-default visibility"), "{error}");
                assert_eq!(error.file, "library.h");
                assert_eq!(error.line, 43);
                assert_eq!(error.col, 28);
            }
        }
        for visibility in ["hidden", "internal"] {
            for declaration in ["int f(void);", "int f(void) { return 1; }"] {
                let source = format!(
                    "# 42 \"library.h\"\n{declaration}\nint f(void) __attribute__((__visibility__(\"{visibility}\"))); int g(void) {{ return f(); }}"
                );
                let module = compile(&source, target).unwrap();
                let defined = module.functions.iter().find(|f| f.name == "f");
                assert_eq!(
                    defined.map(|f| f.hidden),
                    declaration.contains('{').then_some(true)
                );
            }
        }
        for argument in ["", "()", "(hidden)", "(\"default\", 1)"] {
            let source = format!("int f(void) __attribute__((visibility{argument}));");
            assert!(compile(&source, target).is_err(), "{source}");
        }
        let module = compile(
            r#"
            int f(void) __attribute__((visibility("def" "ault")));
            __attribute__((__visibility__("default"))) int f(void) { return 42; }
            int data __attribute__((visibility("default"))) = 7;
            static int local(void) __attribute__((visibility("default")));
            static int local(void) { return 1; }
            int caller(void) { return local(); }
        "#,
            target,
        )
        .unwrap();
        assert!(
            !module
                .functions
                .iter()
                .find(|f| f.name == "f")
                .unwrap()
                .local
        );
        assert!(
            module
                .functions
                .iter()
                .find(|f| f.name == "local")
                .unwrap()
                .local
        );
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, target).unwrap();
    }
}

#[test]
fn hidden_and_internal_visibility_mark_external_symbols_hidden() {
    let source = r#"
        __attribute__((visibility("internal"))) extern int internal_fn(int);
        extern int internal_data __attribute__((visibility("internal")));
        int internal_fn(int x) { return x + internal_data; }
        int internal_data = 1;
        int late_fn(void) { return 2; }
        int late_fn(void) __attribute__((__visibility__("hidden")));
        __attribute__((visibility("hidden"))) int tentative;
        __attribute__((visibility("hidden"))) const int constant = 3;
        __attribute__((visibility("hidden"))) _Thread_local int tls = 4;
        __attribute__((visibility("hidden"))) int declared_only(void);
        static int local_fn(void) __attribute__((visibility("hidden")));
        static int local_fn(void) { return 5; }
        int caller(void) {
            static int counter __attribute__((visibility("hidden")));
            return declared_only() + local_fn() + ++counter + tentative + constant + tls;
        }
    "#;
    let hidden = [
        "internal_fn",
        "internal_data",
        "late_fn",
        "tentative",
        "constant",
        "tls",
    ];
    for target in Target::NATIVE {
        let mut module = compile(source, target).unwrap();
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let flagged: Vec<&str> = module
                .functions
                .iter()
                .filter(|f| f.hidden)
                .map(|f| f.name.as_str())
                .chain(
                    module
                        .data
                        .iter()
                        .filter(|d| d.hidden)
                        .map(|d| d.name.as_str()),
                )
                .collect();
            for name in hidden {
                assert!(flagged.contains(&name), "{name}: {flagged:?}");
            }
            assert_eq!(flagged.len(), hidden.len(), "{flagged:?}");
            let text = keel_codegen::compile(&module, target).unwrap();
            let directives: Vec<&str> = text
                .lines()
                .filter_map(|l| {
                    l.trim()
                        .strip_prefix(".private_extern\t_")
                        .or_else(|| l.trim().strip_prefix(".hidden\t"))
                })
                .map(|name| name.trim_matches('"'))
                .collect();
            assert_eq!(directives.len(), hidden.len(), "{directives:?}");
            for name in hidden {
                assert!(directives.contains(&name), "{name}: {directives:?}");
            }
        }
    }
}

#[test]
fn visibility_conflicts_and_symbolless_placements_are_rejected() {
    for target in Target::NATIVE {
        for source in [
            r#"int f(void) __attribute__((visibility("hidden"))); int f(void) __attribute__((visibility("internal")));"#,
            r#"int f(void) __attribute__((visibility("default"))); int f(void) __attribute__((visibility("hidden"))) { return 1; }"#,
            r#"extern int v __attribute__((visibility("hidden"))); int v __attribute__((visibility("default")));"#,
            r#"__attribute__((visibility("hidden"))) int f(void) __attribute__((visibility("default")));"#,
            r#"int f(void) __attribute__((visibility("hidden"), visibility("internal")));"#,
        ] {
            let error = compile(source, target).unwrap_err();
            assert!(error.message.contains("visibility"), "{source}: {error}");
        }
        for source in [
            r#"typedef int T __attribute__((visibility("hidden")));"#,
            r#"void g(void) { int x __attribute__((visibility("internal"))); }"#,
            r#"void g(int x __attribute__((visibility("hidden"))));"#,
            r#"struct S { int x __attribute__((visibility("hidden"))); };"#,
            r#"struct __attribute__((visibility("hidden"))) S { int x; };"#,
            r#"__attribute__((visibility("hidden"))) struct S { int x; };"#,
            r#"enum __attribute__((visibility("hidden"))) E { A };"#,
            r#"int g(void) { return sizeof(int __attribute__((visibility("hidden")))); }"#,
        ] {
            let error = compile(source, target).unwrap_err();
            assert_eq!(
                error.message,
                "non-default visibility applies only to functions and variables with linkage",
                "{source}"
            );
        }
        compile(
            r#"typedef int T __attribute__((visibility("default")));
            int f(void) __attribute__((visibility("hidden"))); int f(void); int f(void) { return 1; }"#,
            target,
        )
        .unwrap();
    }
}

#[test]
fn representative_library_macros_keep_harmless_annotations_and_capability_fallbacks() {
    let source = include_str!("fixtures/attribute-library.c");
    for target in Target::NATIVE {
        let preprocessed = keel_c::preprocess(
            source,
            "attribute-library.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let mut module = compile(&preprocessed, target).unwrap();
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

/// Builds `inputs` with Clang in a fresh directory and returns the program's
/// output, panicking unless it exits zero.
fn run_native(
    dir: &std::path::Path,
    name: &str,
    inputs: &[(&str, &str)],
    flags: &[&str],
) -> String {
    use std::process::Command;
    for (file, text) in inputs {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let built = Command::new("clang")
        .current_dir(dir)
        .args(["-std=c11", "-Wno-deprecated-declarations"])
        .args(inputs.iter().map(|(file, _)| file))
        .args(flags)
        .args(["-o", name])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(dir.join(name)).output().unwrap();
    assert!(
        run.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    format!(
        "{}\n--- stderr\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn library_macros_and_weak_symbols_run_like_clang() {
    let Some(target) = Target::host().filter(|t| Target::NATIVE.contains(t)) else {
        keel_test_support::skip!("requires an arm64 macOS or x86-64 Linux host");
    };
    let dir = keel_test_support::TempDir::new("attribute-library");
    let source = include_str!("fixtures/attribute-library.c");
    let expected = run_native(dir.path(), "reference", &[("reference.c", source)], &[]);
    let preprocessed = keel_c::preprocess(
        source,
        "attribute-library.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    // Clang's preprocessor output takes the path of a .i file given to the CLI.
    std::fs::write(dir.join("library.c"), source).unwrap();
    let clang_preprocessed = std::process::Command::new("clang")
        .current_dir(dir.path())
        .args(["-E", "library.c"])
        .output()
        .unwrap();
    assert!(clang_preprocessed.status.success());
    let clang_preprocessed = String::from_utf8(clang_preprocessed.stdout).unwrap();
    for (input, text) in [
        ("keel-preprocessed", &*preprocessed),
        ("clang-preprocessed", &*clang_preprocessed),
    ] {
        let mut module = compile(text, target).unwrap();
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            let name = format!("{input}-{optimized}");
            let assembly = keel_codegen::compile(&module, target).unwrap();
            let file = format!("{name}.s");
            let actual = run_native(dir.path(), &name, &[(&file, &assembly)], &[]);
            assert_eq!(actual, expected, "{name}");
        }
    }
    let weak = "int hook(void) __attribute__((weak)); extern int absent __attribute__((weak)); \
        int value __attribute__((weak)) = 7; \
        int main(void) { return (hook ? hook() : 0) || (&absent ? absent : 0) || value != 7; }\n";
    let flags: &[&str] = if target == Target::Aarch64AppleDarwin {
        &["-Wl,-U,_hook,-U,_absent"]
    } else {
        &[]
    };
    for optimized in [false, true] {
        let mut module = compile(weak, target).unwrap();
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        let name = format!("weak-{optimized}");
        let assembly = keel_codegen::compile(&module, target).unwrap();
        let file = format!("{name}.s");
        run_native(dir.path(), &name, &[(&file, &assembly)], flags);
    }
    for (name, text) in [
        ("weak", "static int hook(void) __attribute__((weak));"),
        (
            "alias",
            "int hook(void) __attribute__((alias(\"missing_implementation\")));",
        ),
        (
            "cleanup",
            "void release(int *); int x __attribute__((cleanup(release)));",
        ),
        (
            "visibility",
            "int hook(void); int hook(void) __attribute__((visibility(\"protected\")));",
        ),
    ] {
        for target in Target::NATIVE {
            let error = compile(text, target).unwrap_err().to_string();
            assert!(error.contains(name), "{target:?} {name}: {error}");
        }
    }
}
