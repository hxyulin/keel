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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
        for debug in [false, true] {
            let module = compile(&source, target, debug).unwrap();
            keel_ir::verify_module(&module).unwrap();
            assert!(!module.decls.iter().any(|d| d.name == "get"));
            let program = keel_codegen::compile_program(&module, target).unwrap();
            match target {
                keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
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
    for target in Target::NATIVE {
        for debug in [false, true] {
            let error = compile(source, target, debug).unwrap_err().to_string();
            assert!(
                error.contains("__float128") && error.contains("not supported"),
                "{error}"
            );
        }
    }
}

#[test]
fn unevaluated_compound_literals_preserve_layout_queries() {
    for target in Target::NATIVE {
        for query in [
            "sizeof((Box){})",
            "sizeof((Q){})",
            "_Alignof((Box){})",
            "sizeof((Box[2]){})",
            "sizeof((struct Holder { Box *p; }){ &(Box){} })",
        ] {
            let source = format!(
                "{TYPES} _Static_assert({query} > 0, \"layout\"); int f(void) {{ return {query}; }} static inline int unused(void) {{ return {query}; }}"
            );
            for debug in [false, true] {
                let module = compile(&source, target, debug).unwrap();
                keel_ir::verify_module(&module).unwrap();
                assert!(module.data.is_empty(), "unevaluated literal emitted data");
                keel_codegen::compile_program(&module, target).unwrap();
            }
        }
        for source in [
            "Box *f(void) { return &(Box){}; }",
            "int f(void) { return (struct Tagged { Q q; int tag; }){}.tag; }",
            "extern Box get(void); static void f(void) { (void)&(struct CB { Box (*p)(void); }){get}; }",
            "extern Box get(void); static void *p = &(struct CB { Box (*p)(void); }){get};",
            "static void f(void) { (void)&(Box){}; }",
            "Box *p = &(Box){};",
            "static Box *p = &(Box){};",
            "struct Holder { Box *p; }; struct Holder h = { &(Box){} };",
            "static inline void f(void) { (void)(Box[2]){}; }",
        ] {
            let error = compile(&format!("{TYPES}{source}"), target, false)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("__float128") && error.contains("not supported"),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn late_completion_cycles_atomics_and_shadowing() {
    for target in Target::NATIVE {
        for source in [
            "struct S; struct S object; struct S { struct S *next; __float128 q; };",
            "struct S; static void f(void) { struct S object; } struct S { __float128 q; };",
            "struct S; extern struct S object; static void f(void) { (void)object; } struct S { __float128 q; };",
            "struct S { struct S *next; __float128 q; }; void f(struct S x) {}",
            "struct S { int tag; __float128 q[]; }; struct S object;",
        ] {
            assert!(compile(source, target, false).is_err(), "{source}");
        }
        let source = r#"
            struct Incomplete;
            typedef __float128 Q;
            struct S { struct S *next; Q q; };
            struct Good { _Atomic(Q*) p; struct Incomplete *i; };
            struct Good copy(struct Good x) { return x; }
            extern struct Incomplete unknown;
            struct Incomplete *address(void) { return &unknown; }
            int f(void) { typedef double Q; Q x = 2; return x; }
            int g(void) { int Q = 3; return Q; }
            extern Q original;
            _Static_assert(sizeof(original)==16,"outer typedef survives");
        "#;
        keel_ir::verify_module(&compile(source, target, false).unwrap()).unwrap();
        for ty in ["__float128", "struct S"] {
            let source = format!("struct S {{ __float128 q; }}; extern _Atomic({ty}) object;");
            let error = compile(&source, target, false).unwrap_err().to_string();
            assert!(error.contains("`_Atomic` requires"), "{error}");
        }
    }
}

#[test]
fn native_type_size_overflow_is_a_diagnostic() {
    for target in Target::NATIVE {
        for source in [
            "unsigned long f(void) { return sizeof(int[4611686018427387904]); }",
            "unsigned long f(void) { return sizeof(char[4294967296][4294967296]); }",
            "struct S { char a[9223372036854775807]; char b[9223372036854775807]; char c[2]; };",
            "struct __attribute__((aligned(16))) S { char a[9223372036854775807]; char b[9223372036854775807]; };",
        ] {
            let error = compile(source, target, false).unwrap_err().to_string();
            assert!(
                error.contains("size") && error.contains("overflow"),
                "{target:?}: {source}: {error}"
            );
        }
    }
}

#[test]
fn aggregate_abi_counts_and_temporary_storage_are_checked_separately() {
    for target in Target::NATIVE {
        for (field, expected) in [
            ("char a[4294967297]", "ABI"),
            ("long a[536870912]", "stack-slot size limit"),
        ] {
            let source = format!(
                "struct S {{ {field}; }}; extern struct S g(void); void f(void) {{ g(); }}"
            );
            let error = compile(&source, target, false).unwrap_err().to_string();
            assert!(error.contains(expected), "{target:?}: {error}");
        }
    }
}

#[test]
fn representable_large_types_do_not_require_ir_storage() {
    let source = "struct S { char a[4294967297]; }; extern struct S object; extern struct S g(void); _Static_assert(sizeof(struct S) == 4294967297UL, \"large type\"); unsigned long f(void) { return sizeof(object); }";
    for target in Target::NATIVE {
        let module = compile(source, target, false).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert!(
            module.to_string().contains("4294967297"),
            "{target:?}: {module}"
        );
        compile("struct ByteWide { char a[9223372036854775807]; char tail; }; _Static_assert(sizeof(struct ByteWide) == 9223372036854775808ULL, \"byte size exceeds u64 bit count\");", target, false).unwrap();
    }
}

#[test]
fn inferred_array_dimensions_are_checked_before_initializer_offsets() {
    for target in Target::NATIVE {
        for source in [
            "struct S { char bytes[4294967296]; }; extern struct S object; void f(void) { struct S array[] = {[4294967296] = object}; }",
            "int array[] = {[4611686018427387904] = 1};",
        ] {
            let error = compile(source, target, false).unwrap_err().to_string();
            assert!(
                error.contains("size") && error.contains("overflow"),
                "{target:?}: {source}: {error}"
            );
        }
    }
}

#[test]
fn overaligned_local_padding_is_included_in_storage_limit() {
    let source =
        "void f(void) { char object[4294967295] __attribute__((aligned(64))); (void)&object; }";
    for target in Target::NATIVE {
        let error = compile(source, target, false).unwrap_err().to_string();
        assert!(
            error.contains("stack-slot size limit") && error.contains("padding"),
            "{target:?}: {error}"
        );
    }
}

#[test]
fn arrays_require_complete_elements_before_late_record_completion() {
    for target in Target::NATIVE {
        for source in [
            "typedef struct S A[4611686018427387904]; struct S { int x; }; unsigned long f(void) { return sizeof(A); }",
            "extern struct S array[4611686018427387904]; struct S { int x; }; unsigned long f(void) { return sizeof(array); }",
        ] {
            let error = compile(source, target, false).unwrap_err().to_string();
            assert!(
                error.contains("complete object type"),
                "{target:?}: {source}: {error}"
            );
        }
        let module = compile("struct S; extern struct S object; struct S { int x; }; int f(void) { return object.x; }", target, false).unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}
