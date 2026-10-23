use keel_ir::Target;
fn compile(source: &str, target: Target) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "gnu-types.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}
fn clang_accepts(source: &str, target: Target) {
    clang_checks(source, target, true);
}
fn clang_checks(source: &str, target: Target, accepted: bool) {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let triple = match target {
        Target::Aarch64AppleDarwin => "arm64-apple-macos11",
        Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
        Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
    };
    let mut child = Command::new("clang")
        .args([
            "-target",
            triple,
            "-std=gnu11",
            "-x",
            "c",
            "-fsyntax-only",
            "-",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.success(),
        accepted,
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn integer_modes_layout_identity_and_placements() {
    for target in Target::ALL {
        for (mode, base, bytes) in [
            ("QI", "signed char", 1),
            ("HI", "short", 2),
            ("SI", "int", 4),
            ("DI", "long", 8),
            ("TI", "__int128", 16),
            ("byte", "signed char", 1),
            ("word", "long", 8),
            ("pointer", "long", 8),
        ] {
            let source = format!(
                r#"
                typedef int S __attribute__((__mode__(__{mode}__)));
                typedef unsigned int U __attribute__((mode({mode})));
                _Static_assert(sizeof(S)=={bytes}, "signed size");
                _Static_assert(sizeof(U)=={bytes}, "unsigned size");
                _Static_assert(_Alignof(S)=={bytes}, "alignment");
                _Static_assert(__builtin_types_compatible_p(S,{base}), "identity");
                _Static_assert((S)-1<0 && (U)-1>0, "signedness");
                __attribute__((mode({mode}))) int before;
                int __attribute__((mode({mode}))) middle;
                int after __attribute__((mode({mode})));
                _Static_assert(sizeof(before)=={bytes} && sizeof(middle)=={bytes} && sizeof(after)=={bytes}, "placement");
                struct A {{ int x __attribute__((mode({mode}))); }};
                _Static_assert(sizeof(struct A)=={bytes}, "member");
                extern S object; extern {base} object;
                S f(S x); {base} f({base} x) {{ return x + 1; }}
            "#
            );
            clang_accepts(&source, target);
            compile(&source, target).unwrap_or_else(|e| panic!("{source}\n{e:?}"));
        }
    }
}
#[test]
fn modes_and_calling_conventions_fail_closed() {
    for target in Target::ALL {
        for source in [
            "float x __attribute__((mode(DI)));",
            "int x __attribute__((mode(SF)));",
            "int x __attribute__((mode(V4SI)));",
            "_Bool x __attribute__((mode(QI)));",
            "int x __attribute__((mode(QI),mode(DI)));",
            "extern int x; extern int x __attribute__((mode(DI)));",
            "int f(int); int f(int __attribute__((mode(DI))));",
        ] {
            assert!(compile(source, target).is_err(), "{source}");
        }
        for attr in [
            "ms_abi",
            "__ms_abi__",
            "sysv_abi",
            "stdcall",
            "fastcall",
            "thiscall",
            "vectorcall",
            "regcall",
            "pcs(\"aapcs\")",
            "aarch64_vector_pcs",
            "preserve_most",
            "preserve_all",
            "preserve_none",
            "swiftcall",
            "swiftasync",
            "aarch64_sve_pcs",
            "riscv_vector_cc",
            "cdecl",
            "regparm(2)",
            "sseregparm",
            "interrupt",
            "naked",
        ] {
            for source in [
                format!("int f(void) __attribute__(({attr}));"),
                format!("typedef int (__attribute__(({attr})) *F)(void);"),
            ] {
                assert!(
                    compile(&source, target)
                        .unwrap_err()
                        .message
                        .contains("requires semantics"),
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn vector_layout_identity_and_initializers() {
    for target in Target::ALL {
        for (base, ir) in [
            ("signed char", "i8x16"),
            ("unsigned char", "i8x16"),
            ("short", "i16x8"),
            ("unsigned short", "i16x8"),
            ("int", "i32x4"),
            ("unsigned int", "i32x4"),
            ("long", "i64x2"),
            ("unsigned long", "i64x2"),
            ("float", "f32x4"),
            ("double", "f64x2"),
        ] {
            let source = format!(
                r#"
                typedef {base} V __attribute__((vector_size(16)));
                _Static_assert(sizeof(V)==16 && _Alignof(V)==16,"vector layout");
                _Static_assert(!__builtin_types_compatible_p(V,{base}),"vector identity");
                struct S {{ char x; V v; char y; }};
                _Static_assert(sizeof(struct S)==48,"struct size");
                _Static_assert(__builtin_offsetof(struct S,v)==16,"member offset");
                V global = {{1,2}};
                V identity(V v) {{ V copy=v; return copy; }}
                {base} lane(V v, int i) {{ V init={{1,2}}; init[0]=v[1]; init[1]+=3; return init[i]; }}
                {base} temporary(V v) {{ return identity(v)[0]; }}
            "#
            );
            clang_accepts(&source, target);
            let module = compile(&source, target).unwrap_or_else(|e| panic!("{source}\n{e:?}"));
            let text = module.to_string();
            assert!(text.contains(ir), "{text}");
            keel_codegen::compile(&module, target).unwrap_or_else(|e| panic!("{text}\n{e:?}"));
        }
    }
}
#[test]
fn vector_arithmetic_comparison_and_bitcasts() {
    for target in Target::ALL {
        let source = r#"
            typedef int V __attribute__((vector_size(16)));
            typedef unsigned int U __attribute__((vector_size(16)));
            typedef float F __attribute__((vector_size(16)));
            V add(V a,V b) { a+=b; return -a*b; }
            V bits(V a,V b) { return (~a&b)|((a^b)<<(V){1,2,3,4}); }
            V cmp(V a,V b) { return a<b; }
            V fcmp(F a,F b) { return a!=b; }
            U ucmp(U a,U b) { return (U)(a>b); }
            F fmath(F a,F b) { return -(a+b)*a/b; }
            F cast(V a) { return (F)a; }
            __int128 toint(V a) { return (__int128)a; }
            V fromint(__int128 a) { return (V)a; }
        "#;
        let module = compile(source, target).unwrap();
        let text = module.to_string();
        for op in [
            "viadd",
            "vimul",
            "vand",
            "vicmp.slt",
            "vfcmp.ne",
            "vicmp.ugt",
            "vfdiv",
            "bitcast",
        ] {
            assert!(text.contains(op), "missing {op}: {text}");
        }
        keel_codegen::compile(&module, target).unwrap_or_else(|e| panic!("{text}\n{e:?}"));
    }
}
#[test]
fn unsupported_vectors_are_diagnosed() {
    for target in Target::ALL {
        for source in [
            "typedef int V __attribute__((ext_vector_type(4)));",
            "typedef int V __attribute__((__ext_vector_type__(4)));",
            "typedef union {int x; float y;} U __attribute__((transparent_union));",
            "typedef union {int x; float y;} U __attribute__((__transparent_union__));",
            "typedef int V __attribute__((vector_size(8)));",
            "typedef int V __attribute__((vector_size(64)));",
            "typedef int V __attribute__((vector_size(0)));",
            "typedef int V __attribute__((vector_size(-16)));",
            "int n; typedef int V __attribute__((vector_size(n)));",
            "typedef _Bool V __attribute__((vector_size(16)));",
            "typedef __int128 V __attribute__((vector_size(16)));",
            "typedef struct {int x;} V __attribute__((vector_size(16)));",
            "typedef int V __attribute__((vector_size(16))); V f(V x,V y){return x/y;}",
            "typedef int V __attribute__((vector_size(16))); V f(V x,V y){return x%y;}",
            "typedef float V __attribute__((vector_size(16))); V f(V x,V y){return x&y;}",
            "typedef int V __attribute__((vector_size(16))); V f(V x,long n){return x+n;}",
            "typedef int V __attribute__((vector_size(16))); int f(V x){return !x;}",
            "typedef int V __attribute__((vector_size(16))); int f(V x){return x[4];}",
            "typedef int V __attribute__((vector_size(16))); int f(V x){return x[-1];}",
            "typedef int V __attribute__((vector_size(16))); int *f(V *x){return &(*x)[0];}",
            "typedef int V __attribute__((vector_size(16))); int f(V x){return (int)x;}",
            "typedef int V __attribute__((vector_size(16))); extern V x; extern int x;",
            "typedef int V __attribute__((vector_size(16))); typedef unsigned U __attribute__((vector_size(16))); extern V x; extern U x;",
            "typedef int V __attribute__((vector_size(16))); V x={1,2,3,4,5};",
        ] {
            assert!(compile(source, target).is_err(), "{source}");
        }
    }
}

#[test]
fn vector_attributes_keep_declarator_ownership() {
    for target in Target::ALL {
        let source = r#"
            typedef int V __attribute__((vector_size(16)));
            typedef int __attribute__((vector_size(16))) *P;
            typedef int __attribute__((vector_size(16))) A[3];
            _Static_assert(__builtin_types_compatible_p(P,V*),"pointer element");
            _Static_assert(__builtin_types_compatible_p(A,V[3]),"array element");
            __attribute__((vector_size(16))) int before;
            int __attribute__((vector_size(16))) middle;
            int after __attribute__((vector_size(16)));
            _Static_assert(sizeof(before)==16 && sizeof(middle)==16 && sizeof(after)==16,"placement");
            __attribute__((vector_size(16))) int f(int x __attribute__((vector_size(16)))) { return x; }
            V f(V x);
            V (*pointer)(V)=f;
            V call(V x) { return pointer(x); }
            typedef int Shadow;
            long shadow(void) { typedef int Shadow __attribute__((mode(DI))); Shadow x=1; return x; }
            const int constant __attribute__((vector_size(16)))={1,2};
            struct S { int x __attribute__((vector_size(16))); };
            _Static_assert(sizeof(struct S)==16,"field");
        "#;
        clang_accepts(source, target);
        let module = compile(source, target).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::compile(&module, target).unwrap();
        for source in [
            "struct S { int x:1 __attribute__((vector_size(16))); };",
            "struct S { int x:16 __attribute__((mode(QI))); };",
            "typedef int *P __attribute__((vector_size(16)));",
            "typedef int A[3] __attribute__((vector_size(16)));",
            "int f(void) __attribute__((vector_size(16)));",
            "struct __attribute__((vector_size(16))) S { int x; };",
            "struct S { int x; } __attribute__((vector_size(16)));",
            "typedef int V __attribute__((vector_size(16))); V f(void){__builtin_va_list a;return __builtin_va_arg(a,V);}",
            "typedef int V __attribute__((vector_size(16))); int f(volatile V *p){return (*p)[0];}",
            "typedef int V __attribute__((vector_size(16))); typedef unsigned U __attribute__((vector_size(16))); typedef V A; typedef U A;",
        ] {
            assert!(compile(source, target).is_err(), "{source}");
        }
    }
}

#[test]
fn unsupported_vector_aggregate_abis_are_diagnosed() {
    for target in Target::ALL {
        let source = "typedef int V __attribute__((vector_size(16))); union U { V v; }; union U f(union U x){return x;}";
        assert!(
            compile(source, target)
                .unwrap_err()
                .message
                .contains("unions containing vectors")
        );
    }
    let source = "typedef int V __attribute__((vector_size(16))); struct S { V v; }; struct S f(struct S x){return x;}";
    assert!(
        compile(source, Target::X86_64LinuxGnu)
            .unwrap_err()
            .message
            .contains("SSEUP")
    );
}

#[test]
fn vector_conditions_require_scalars() {
    for target in Target::ALL {
        for source in [
            "typedef int V __attribute__((vector_size(16))); V f(V x,V a,V b){return x?a:b;}",
            "typedef int V __attribute__((vector_size(16))); V f(V x,V a){return x?:a;}",
            "typedef int V __attribute__((vector_size(16))); int f(V x){return (x==x)?1:2;}",
        ] {
            clang_checks(source, target, false);
            let error = compile(source, target).unwrap_err();
            assert!(
                error.message.contains("condition must be a scalar"),
                "{error}"
            );
        }
    }
}
