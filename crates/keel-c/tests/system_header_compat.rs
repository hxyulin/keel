//! Reduced glibc declarations from the LinuxHost application-input failures.
use keel_c::{Options, PreprocessOptions};
use keel_ir::Target;

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "system-header.c",
        Options {
            target,
            debug_info: false,
        },
    )
}

#[test]
fn gcc_float32_builtin_is_diagnosed_in_declarations() {
    for target in Target::ALL {
        for source in [
            "extern _Float32 strtof32(const char *restrict, char **restrict);",
            "_Float32 object;",
            "typedef _Float32 F;",
            "void f(_Float32 value);",
            "void f(void) { _Float32 value; }",
            "int f(void) { return sizeof(_Float32); }",
        ] {
            let error = compile(source, target).unwrap_err().to_string();
            assert!(
                error.contains("unsupported extended floating type `_Float32`"),
                "{error}"
            );
        }
    }
}

#[test]
fn header_supplied_float32_typedef_keeps_its_declared_identity() {
    // Clang's glibc path supplies an ordinary typedef. It is the header's
    // choice, not an implementation of GCC's distinct builtin type.
    let source = r#"
        typedef float _Float32;
        _Static_assert(__builtin_types_compatible_p(_Float32, float), "typedef identity");
        _Static_assert(sizeof(_Float32) == 4, "layout");
        _Static_assert(_Alignof(_Float32) == 4, "alignment");
        _Float32 f(_Float32 x); float f(float x) { return x + 1.0f; }
        double promoted(_Float32 x) { return _Generic(+x, float: 1, default: 0); }
        float scoped(void) { typedef double _Float32; _Float32 x = 2.0; return (float)x; }
        int shadowed(void) { int _Float32 = 7; _Float32 += 1; return _Float32; }
    "#;
    for target in Target::ALL {
        let module = compile(source, target).unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}

#[test]
fn complex_binary128_mode_is_preserved_then_diagnosed() {
    let source = "typedef _Complex float C __attribute__((__mode__(__TC__)));";
    for target in Target::ALL {
        let pp = keel_c::preprocess(
            source,
            "system-header.c",
            &PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(pp.contains("__TC__"));
        let error = compile(&pp, target).unwrap_err().to_string();
        assert!(
            error.contains("unsupported integer mode `__TC__`"),
            "{error}"
        );
    }
}

#[test]
fn float32_header_typedef_still_checks_redeclarations() {
    let source = "typedef float _Float32; extern _Float32 object; extern double object;";
    for target in Target::ALL {
        assert!(compile(source, target).is_err());
    }
}
