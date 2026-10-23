//! Fail closed on existing opaque values without rejecting header declarations.
use keel_ir::Target;

fn compile(
    source: &str,
    target: Target,
    debug_info: bool,
) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "unsupported-values.c",
        keel_c::Options { target, debug_info },
    )
}

const TYPES: &str = r#"
    typedef __float128 Q;
    struct Box { Q x; };
    typedef struct Box Box;
    union U { int i; Box b; };
    struct Nested { union U u[2]; };
    extern Box a, b;
    extern struct Nested n;
"#;

#[test]
fn containing_objects_and_abis_are_rejected() {
    for target in Target::ALL {
        for source in [
            "Box pick(Box a, Box b) { return b; }",
            "Box pick(void) { return a; }",
            "void take(Box a) {}",
            "static inline Box unused(Box x) { return x; }",
            "static Box unused(void) {}",
            "void take(struct Nested a) {}",
            "void take(union U a) {}",
            "Box object;",
            "static struct Nested object;",
            "extern Box object = {0};",
            "Box objects[2];",
            "void f(void) { Box object; }",
            "static inline void f(void) { union U object; }",
            "void f(void) { a = b; }",
            "void f(void) { (void)a; }",
            "void f(void) { a; }",
            "void f(void) { (void)n; }",
            "void f(void) { (void)a.x; }",
            "void f(void) { (void)(Box){}; }",
            "extern void sink(Box); void f(void) { sink(a); }",
            "extern Box get(void); void f(void) { get(); }",
            "extern Box get(void); static inline void f(void) { get(); }",
            "extern Q get(void); static void f(void) { get(); }",
            "extern void sink(Box); static void f(void) { sink(a); }",
            "extern void sink(int,...); static void f(void) { sink(0,a); }",
            "extern void sink(); static void f(void) { sink(a); }",
            "extern Box (*fp)(Box); static void f(void) { fp(a); }",
            "extern Box get(void); void *f(void) { return (void*)&get; }",
            "extern Box get(void); static void f(void) { Box (*fp)(void) = get; }",
            "extern Box get(void); static void f(void) { get; }",
            "extern Box get(void); static void f(void) { static Box (*fp)(void) = get; }",
            "extern Box get(void); static Box (*fp)(void) = get;",
            "extern Q scalar; static inline void f(void) { (void)scalar; }",
        ] {
            let input = format!("{TYPES}\n{source}");
            let error = compile(&input, target, false).unwrap_err().to_string();
            assert!(
                error.contains("__float128") && error.contains("not supported"),
                "{target:?}: {source}: {error}"
            );
        }
    }
}

#[test]
fn declarations_layout_and_pointer_transport_remain_usable() {
    let source = format!(
        r#"{TYPES}
        extern Q scalar;
        extern Box get(Box);
        typedef Box (*Callback)(Box);
        extern Callback callback;
        extern Q elements[2];
        static inline void unused_array(void) {{ elements; }}
        struct Link {{ struct Link *next; Q *q; }};
        struct Outer {{ struct Outer *next; Box box; int tag; }};
        _Static_assert(sizeof(Box)==16, "box size");
        _Static_assert(_Alignof(Box)==16, "box align");
        _Static_assert(sizeof(struct Nested)==32, "nested size");
        _Static_assert(__builtin_offsetof(struct Outer,box)==16, "offset");
        _Static_assert(__builtin_types_compatible_p(Q,__float128), "identity");
        Box *transport(Box *p) {{ return p; }}
        Q *field(Box *p) {{ return &p->x; }}
        Box *global_address(void) {{ return &a; }}
        Q *scalar_address(void) {{ return &scalar; }}
        Q *element(Q *p, int i) {{ return &p[i]; }}
        struct Link copy(struct Link x) {{ return x; }}
        Callback pass_callback(Callback p) {{ return &*p; }}
        int tag(struct Outer *p) {{ p->tag = 7; return p->tag; }}
        static inline int unused(Box *p) {{ return sizeof(*p) + sizeof(get(*p)); }}
    "#
    );
    for target in Target::ALL {
        for debug in [false, true] {
            let module = compile(&source, target, debug).unwrap();
            keel_ir::verify_module(&module).unwrap();
            assert!(!module.decls.iter().any(|d| d.name == "get"));
            let program = keel_codegen::compile_program(&module, target).unwrap();
            match target {
                Target::Aarch64AppleDarwin => {
                    keel_asm::assemble_program_object(&program, Default::default())
                }
                Target::Aarch64LinuxGnu => {
                    keel_asm::assemble_elf_program_object(&program, Default::default())
                }
                Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object_with_features(
                    &program,
                    Default::default(),
                ),
            }
            .unwrap();
        }
    }
}

#[test]
fn opaque_numeric_identity_survives_redeclarations() {
    for target in Target::ALL {
        for source in [
            "extern __float128 x; extern double x;",
            "extern double x; extern __float128 x;",
            "typedef __float128 Q; typedef double Q;",
            "extern __float128 f(void); extern double f(void);",
            "extern void f(__float128); extern void f(double);",
            "extern __float128 *x; extern double *x;",
            "extern __float128 x[2]; extern double x[2];",
            "extern __float128 (*f)(void); extern double (*f)(void);",
        ] {
            let error = compile(source, target, false).unwrap_err().to_string();
            assert!(
                error.contains("incompatible types"),
                "{target:?}: {source}: {error}"
            );
        }
        compile("typedef __float128 Q; typedef __float128 Q; extern Q x; extern Q x; extern Q f(Q); extern Q f(Q);",target,false).unwrap();
    }
}

#[test]
fn immediate_unsupported_types_are_also_contained() {
    for target in Target::ALL {
        for ty in ["_Float16", "_Complex long double"] {
            let source =
                format!("struct S {{ {ty} x[2]; }}; struct S f(struct S x) {{ return x; }}");
            let error = compile(&source, target, false).unwrap_err().to_string();
            assert!(
                error.contains(ty) && error.contains("not supported"),
                "{error}"
            );
        }
    }
}

#[test]
fn aggregate_va_arg_stays_rejected() {
    for target in Target::ALL {
        for ty in ["Box", "union U", "Q"] {
            let source = format!(
                "{TYPES} static void f(__builtin_va_list ap) {{ __builtin_va_arg(ap,{ty}); }}"
            );
            let error = compile(&source, target, false).unwrap_err().to_string();
            assert!(
                error.contains("`va_arg` of a non-scalar") || error.contains("not supported"),
                "{error}"
            );
        }
    }
}

#[test]
fn native_aggregate_abi_reproducer_diagnoses() {
    let source = include_str!("fixtures/extended-float-aggregate-abi.c");
    for target in Target::ALL {
        for debug in [false, true] {
            let error = compile(source, target, debug).unwrap_err().to_string();
            assert!(
                error.contains("__float128") && error.contains("not supported"),
                "{error}"
            );
        }
    }
}
