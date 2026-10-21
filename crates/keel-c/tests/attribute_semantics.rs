//! Fail closed on selected attributes with unimplemented observable semantics.
use keel_ir::Target;

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "attributes.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

#[test]
fn linkage_and_lifetime_attributes_are_diagnosed_on_declarations_and_redeclarations() {
    for target in Target::ALL {
        for (attribute, args) in [
            ("weak", ""),
            ("weakref", "(\"implementation\")"),
            ("alias", "(\"implementation\")"),
            ("ifunc", "(\"resolver\")"),
            ("constructor", ""),
            ("constructor", "(101)"),
            ("destructor", ""),
            ("destructor", "(101)"),
            ("section", "(\"plugins\")"),
            ("used", ""),
            ("retain", ""),
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
fn cleanup_and_data_retention_cannot_disappear_even_in_unused_functions() {
    for target in Target::ALL {
        for attr in ["cleanup(release)", "__cleanup__(release)"] {
            for source in [
                format!(
                    "void release(int *); int f(void) {{ int x __attribute__(({attr})) = 1; return x; }}"
                ),
                format!(
                    "void release(int *); static inline int f(void) {{ __attribute__(({attr})) int x = 1; return x; }}"
                ),
            ] {
                assert!(
                    compile(&source, target)
                        .unwrap_err()
                        .message
                        .contains("attribute `cleanup`")
                );
            }
        }
        for attr in [
            "weak",
            "alias(\"other\")",
            "section(\"registry\")",
            "used",
            "retain",
        ] {
            let source = format!(
                "extern int other; extern int value; extern int value __attribute__(({attr}));"
            );
            assert!(compile(&source, target).is_err(), "{source}");
        }
    }
}

#[test]
fn visibility_accepts_only_explicit_default_and_preserves_error_location() {
    for target in Target::ALL {
        for visibility in ["hidden", "internal", "protected", "unknown", ""] {
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
fn representative_library_macros_keep_harmless_annotations_and_capability_fallbacks() {
    let source = include_str!("fixtures/attribute-library.c");
    for target in Target::ALL {
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
