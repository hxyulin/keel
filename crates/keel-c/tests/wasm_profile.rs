use keel_c::{Options, compile};
use keel_ir::{DataLayout, Target};

fn options() -> Options {
    Options {
        target: Target::Wasm32UnknownUnknown,
        debug_info: false,
    }
}

#[test]
fn public_compile_selects_wasm_layout_and_preserves_options() {
    keel_ir::verify_module_with_layout(
        &compile("", "empty.i", options()).unwrap(),
        DataLayout::WASM32,
    )
    .unwrap();
    let source = "_Static_assert(sizeof(long)==4 && sizeof(void*)==4, \"ILP32\"); long f(long x) { volatile long y=x; return y+1; }";
    let module = compile(source, "layout.i", options()).unwrap();
    keel_ir::verify_module_with_layout(&module, DataLayout::WASM32).unwrap();
    assert_eq!(
        module.signature("f").unwrap().returns[0].ty,
        keel_ir::Type::I32
    );
    for target in Target::NATIVE {
        let source = "_Static_assert(sizeof(long)==8 && sizeof(void*)==8, \"LP64\"); long f(long x) { return x+1; }";
        let m = compile(
            source,
            "native.i",
            Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(m.signature("f").unwrap().returns[0].ty, keel_ir::Type::I64);
    }
}

#[test]
fn declaration_only_extended_types_have_wasm_storage() {
    compile("typedef long double LD; typedef __builtin_va_list VA; _Static_assert(sizeof(LD)==16 && _Alignof(LD)==16, \"binary128\"); _Static_assert(sizeof(VA)==4 && _Alignof(VA)==4, \"va declaration\"); struct S { int a; LD b; }; _Static_assert(sizeof(struct S)==32, \"record\");", "decl.i", options()).unwrap();
}

#[test]
fn unsupported_contracts_are_checked_before_pruning() {
    for source in [
        "static int unused(void) { double x; return 0; }",
        "static int unused(void) { long double x; return 0; }",
        "static int unused(void) { __builtin_va_list x; return 0; }",
        "static void unused(void) __attribute__((constructor)); static void unused(void) {}",
        "static void unused(void) __attribute__((destructor)); static void unused(void) {}",
        "static int unused(void) __attribute__((used)); static int unused(void) { return 0; }",
        "static int unused(void) __attribute__((section(\"custom\"))); static int unused(void) { return 0; }",
        "static int unused(void) __attribute__((retain)); static int unused(void) { return 0; }",
        "static int unused(void) __attribute__((weak)); static int unused(void) { return 0; }",
        "static int unused(void) __attribute__((visibility(\"hidden\"))); static int unused(void) { return 0; }",
        "static _Thread_local int unused;",
        "int narrow(char);",
        "double floating(double);",
        "int varargs(int,...);",
        "struct S { int a; }; struct S aggregate(struct S);",
        "int *pointer(int *);",
        "int no_proto();",
        "extern int data;",
        "int external(int); int f(void) { return external(1); }",
        "static int unused(void) { _Atomic int x; return 0; }",
        "static int unused(void) { int a[17]={1}; return 0; }",
        "static int unused(void) { char a[4]=\"abc\"; return 0; }",
        "static int unused(int n) { int a[n]; return 0; }",
        "static int unused(void) { asm(\"\"); return 0; }",
        "static int unused(void) { int (*p)(void)=0; return 0; }",
        "static int x __attribute__((constructor));",
        "typedef int V __attribute__((vector_size(16)));",
        "int f(void) { if (0) return __builtin_popcount(1); return 0; }",
    ] {
        assert!(
            compile(source, "reject.i", options()).is_err(),
            "accepted {source}"
        );
    }
    let mut opts = options();
    opts.debug_info = true;
    assert!(
        compile("int f(void) { return 0; }", "debug.i", opts)
            .unwrap_err()
            .message
            .contains("debug")
    );
}

#[cfg(feature = "profile")]
#[test]
fn compile_profiling_cannot_bypass_wasm_gate() {
    for source in ["int f(void) { return 1; }", "\"unterminated"] {
        assert!(
            keel_c::profile::compile(source, "profile.i", options())
                .unwrap_err()
                .message
                .contains("profiling is unavailable")
        );
    }
}
