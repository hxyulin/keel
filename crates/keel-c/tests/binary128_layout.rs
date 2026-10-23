//! Declaration/layout capability does not grant binary128 value or call support.
use keel_ir::Target;
const TYPES: &str = r#"
typedef __float128 Q;
typedef _Complex float C __attribute__((__mode__(__TC__)));
typedef _Complex double D __attribute__((mode(TC)));
struct Box { C x; };
union U { struct Box box; Q real; int tag; };
struct Nested { union U u[2]; };
extern C a, b;
extern struct Box box;
"#;
fn compile(s: &str, debug_info: bool) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        s,
        "binary128-layout.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info,
        },
    )
}
#[test]
fn canonical_identity_layout_and_unevaluated_queries_emit_objects() {
    let s = format!(
        r#"{TYPES}
        _Static_assert(sizeof(Q)==16 && _Alignof(Q)==16, "real");
        _Static_assert(sizeof(C)==32 && _Alignof(C)==16, "complex");
        _Static_assert(__builtin_types_compatible_p(__typeof__(__real__ a),Q),"real component");
        _Static_assert(__builtin_types_compatible_p(__typeof__(__imag__ a),Q),"imag component");
        _Static_assert(sizeof(__real__ a)==16 && _Alignof(__imag__ a)==16,"component layout");
        _Static_assert(__builtin_types_compatible_p(C,D), "canonical mode");
        _Static_assert(!__builtin_types_compatible_p(C,Q), "real identity");
        _Static_assert(!__builtin_types_compatible_p(C,_Complex float), "float");
        _Static_assert(!__builtin_types_compatible_p(C,_Complex double), "double");
        _Static_assert(!__builtin_types_compatible_p(C,_Complex long double), "x87");
        _Static_assert(sizeof(_Complex long double)==32 && _Alignof(_Complex long double)==16,"x87 layout");
        struct Wrapper {{ char before; C value; char after; }};
        _Static_assert(__builtin_offsetof(struct Wrapper,value)==16,"offset");
        _Static_assert(__builtin_offsetof(struct Wrapper,after)==48,"footprint");
        _Static_assert(sizeof(struct Wrapper)==64 && _Alignof(struct Wrapper)==16,"wrapper");
        _Static_assert(sizeof(struct Nested)==64 && _Alignof(struct Nested)==16,"nested");
        _Static_assert(sizeof(C[3])==96,"stride");
        _Static_assert(sizeof((struct Box){{}})==32,"literal");
        _Static_assert(_Alignof((C){{}})==16,"literal alignment");
        extern C get(C); extern D get(D);
        extern C array[2]; extern D array[2];
        typedef C (*Callback)(C); extern Callback callback;
        struct Link {{ struct Link *next; C *q; _Atomic(C*) atomic; }};
        struct Outer {{ struct Outer *next; struct Box box; int tag; }};
        C *element(C *p, int i) {{ return &p[i]; }}
        C *field(struct Wrapper *p) {{ return &p->value; }}
        C *address(void) {{ return &a; }}
        struct Link *link(struct Link *p) {{ return p->next; }}
        Callback transport(Callback p) {{ return &*p; }}
        int tag(struct Outer *p) {{ p->tag=7; return p->tag; }}
        int size(void) {{ return sizeof(get(a)) + sizeof((C[2]){{}}); }}
        static inline int unused(void) {{ return sizeof((struct Box){{}}); }}
    "#
    );
    for debug in [false, true] {
        let module = compile(&s, debug).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let p = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_program_object_with_features(&p, Default::default()).unwrap();
        assert!(!module.decls.iter().any(|d| d.name == "get"));
    }
}
#[test]
fn values_and_abis_fail_before_pruning() {
    for body in [
        "C object;",
        "static struct Nested object;",
        "extern C object={0};",
        "struct Box objects[2];",
        "void f(void){ C x; }",
        "static inline void f(void){ struct Box x; }",
        "C f(C x){return x;}",
        "static inline C f(void){}",
        "struct Box f(struct Box x){return x;}",
        "void f(void){a=b;}",
        "void f(void){(void)a;}",
        "void f(void){a+b;}",
        "void f(void){a==b;}",
        "void f(void){(C)1;}",
        "void f(void){(Q)a;}",
        "void f(void){__real__ a;}",
        "void f(void){__imag__ a;}",
        "void f(void){(void)box;}",
        "void f(void){(void)box.x;}",
        "void f(void){(void)&(C){};}",
        "C *p=&(C){};",
        "void f(void){(struct Tagged {C q; int i;}){}.i;}",
        "extern C get(void); static inline void f(void){get();}",
        "extern void sink(C); static void f(void){sink(a);}",
        "extern void sink(struct Box); static void f(void){sink(box);}",
        "extern C (*fp)(void); static void f(void){fp();}",
        "extern void sink(int,...); static void f(void){sink(0,a);}",
        "extern void sink(); static void f(void){sink(a);}",
        "extern C get(void); static void f(void){get;}",
        "extern C get(void); static void *p=(void*)&get;",
        "extern C get(void); static void f(void){C (*p)(void)=get;}",
        "void f(__builtin_va_list p){__builtin_va_arg(p,C);}",
        "_Atomic(C) object;",
        "_Atomic(struct Box) object;",
    ] {
        let s = format!("{TYPES}\n{body}");
        assert!(compile(&s, false).is_err(), "accepted: {body}");
    }
}
#[test]
fn redeclarations_keep_canonical_numeric_identity() {
    for body in [
        "extern C x; extern Q x;",
        "extern C x; extern _Complex double x;",
        "typedef C T; typedef Q T;",
        "extern C *x; extern Q *x;",
        "extern C x[2]; extern Q x[2];",
        "extern C f(void); extern Q f(void);",
        "extern void f(C); extern void f(Q);",
        "extern C (*f)(void); extern Q (*f)(void);",
    ] {
        let e = compile(&format!("{TYPES}\n{body}"), false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("incompatible types"), "{body}: {e}");
    }
    for body in [
        "typedef float T __attribute__((mode(TC)));",
        "typedef int T __attribute__((mode(TC)));",
        "typedef C *T __attribute__((mode(TC)));",
        "typedef _Complex float T __attribute__((mode(TC),mode(DI)));",
    ] {
        assert!(compile(body, false).is_err(), "{body}");
    }
}

#[test]
fn maintained_pointer_fixture_emits_debug_and_plain_objects() {
    for debug in [false, true] {
        let m = compile(include_str!("fixtures/binary128-layout.c"), debug).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let p = keel_codegen::compile_program(&m, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_program_object_with_features(&p, Default::default()).unwrap();
    }
}

#[test]
fn native_linux_layout_and_pointer_footprints_match_both_compilers() {
    if !cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        return;
    }
    use std::{fs, process::Command};
    let dir = std::path::PathBuf::from(format!("/tmp/keel-tc-native-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("harness.c"),
        include_str!("fixtures/binary128-layout-harness.c"),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("fixtures/extended-float-layout-oracle.c"),
    )
    .unwrap();
    let run = |cmd: &mut Command| {
        let result = cmd.output().unwrap();
        assert!(
            result.status.success(),
            "{cmd:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    for compiler in ["/usr/bin/gcc", "/usr/bin/clang"] {
        if !Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok_and(|r| r.status.success())
        {
            continue;
        }
        run(Command::new(compiler).current_dir(&dir).args([
            "-std=gnu11",
            "-march=x86-64",
            "oracle.c",
            "-o",
            "oracle",
        ]));
        run(&mut Command::new(dir.join("oracle")));
        for debug in [false, true] {
            let m = compile(include_str!("fixtures/binary128-layout.c"), debug).unwrap();
            let p = keel_codegen::compile_program(&m, Target::X86_64LinuxGnu).unwrap();
            let obj = keel_asm::x86::assemble_program_object_with_features(&p, Default::default())
                .unwrap();
            fs::write(dir.join("keel.o"), obj).unwrap();
            run(Command::new(compiler).current_dir(&dir).args([
                "-std=gnu11",
                "harness.c",
                "keel.o",
                "-o",
                "run",
            ]));
            run(&mut Command::new(dir.join("run")));
        }
    }
}

#[test]
fn mode_positions_aliases_and_long_double_subject_preserve_identity() {
    let source = r#"
        typedef _Complex float C __attribute__((mode(TC)));
        typedef _Complex long double LD;
        typedef LD L __attribute__((mode(TC)));
        typedef __attribute__((mode(TC))) _Complex double Before;
        typedef _Complex __attribute__((mode(TC))) long double Among;
        typedef C Again __attribute__((__mode__(__TC__)));
        typedef _Complex double __attribute__((mode(TC))) *Pointer;
        _Static_assert(__builtin_types_compatible_p(C,L),"long double subject");
        _Static_assert(__builtin_types_compatible_p(C,Before),"before");
        _Static_assert(__builtin_types_compatible_p(C,Among),"among");
        _Static_assert(__builtin_types_compatible_p(C,Again),"idempotent");
        _Static_assert(__builtin_types_compatible_p(C*,Pointer),"pointer base");
        _Static_assert(sizeof(_Complex long double __attribute__((mode(TC))))==32,"type name");
        struct Members {
            char prefix;
            _Complex long double field __attribute__((mode(TC)));
            char suffix;
        };
        _Static_assert(__builtin_offsetof(struct Members,field)==16,"member offset");
        _Static_assert(__builtin_offsetof(struct Members,suffix)==48,"footprint");
        _Static_assert(sizeof(struct Members)==64,"wrapper size");
        extern L object; extern C object;
        extern C call(Among); extern L call(C);
        typedef C (*Callback)(L);
        typedef Among (*Callback)(C);
        C *field(struct Members *p) { return &p->field; }
        unsigned long query(void) { return sizeof(call(object)); }
    "#;
    for debug in [false, true] {
        let m = compile(source, debug).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let p = keel_codegen::compile_program(&m, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_program_object_with_features(&p, Default::default()).unwrap();
    }
}

#[test]
fn completed_recursive_types_and_long_double_tc_values_remain_guarded() {
    for body in [
        "typedef _Complex long double L __attribute__((mode(TC))); L object;",
        "typedef _Complex long double L __attribute__((mode(TC))); static inline L f(L x){return x;}",
        "struct Late; extern struct Late a; static inline void f(void){(void)a;} struct Late { C x; };",
        "struct Late; extern struct Late get(void); static inline void f(void){get();} struct Late {C x;};",
        "struct Late; extern struct Late get(void); static void *p=(void*)&get; struct Late {C x;};",
        "struct Cycle { struct Cycle *next; union {C x; int i;} payload[2]; }; static inline void f(void){struct Cycle x;}",
        "static inline void f(void){(void)&(struct B {C x; int y;}){.y=1};}",
        "void f(void){ _Atomic(C) *p; }",
        "extern C *p; void f(void){ __atomic_load_n(p,0); }",
    ] {
        for debug in [false, true] {
            assert!(
                compile(&format!("{TYPES}\n{body}"), debug).is_err(),
                "accepted: {body}"
            );
        }
    }
}
