use keel_ir::{Target, Type};
use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &str = include_str!("programs/old_style.c");
const TARGETS: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];

const INTEROP_CASES: [(&str, &str); 6] = [
    (SOURCE, ""),
    (
        r#"int printf(const char *,...); struct S{int x;}; struct S make(); struct S (*address(void))(){return make;} int main(void){struct S (*p)()=address(); struct S s=make((char)7); struct S t=p((short)8); printf("%d %d\n",s.x,t.x);return 0;}"#,
        "struct S{int x;}; struct S make(x) char x; {struct S s={x+2}; return s;}",
    ),
    (
        "int printf(const char *,...); unsigned long u(int,int,unsigned int,double); int main(void){ unsigned long (*p)(int,int,unsigned int,double)=u; printf(\"%lu %lu\\n\",u(511,65535,4294967295U,16777217.0),p(511,65535,4294967295U,16777217.0));return 0;}",
        "unsigned long u(a,b,c,f) float f; unsigned int c; unsigned short b; unsigned char a; {return a+1000UL*b+c+100000UL*(f==16777216.0f);}",
    ),
    (
        "unsigned long u(a,b,c,f) float f; unsigned int c; unsigned short b; unsigned char a; {unsigned char *p=&a; unsigned short *q=&b; float *r=&f; return *p+1000UL**q+c+100000UL*(*r==16777216.0f);}",
        "int printf(const char *,...); unsigned long u(); int main(void){unsigned long (*p)(int,int,unsigned int,double)=u; printf(\"%lu %lu\\n\",u((unsigned char)255,(unsigned short)65535,4294967295U,16777217.0),p(511,65535,4294967295U,16777217.0));return 0;}",
    ),
    (
        "int printf(const char *,...); int old(); int new(int,int,double); int (*address(void))(){return old;} int main(void){int (*p)()=address(); printf(\"%d %d %d\\n\", old((char)-9,(short)-257,1.25f), p((char)-3,(short)-200,2.5f),new(-7,312,3.5)); return 0;}",
        "int old(c,s,f) char c; short s; float f; { return c+s+(int)(f*8); } int new(c,s,f) char c; short s; float f; { char *p=&c; short *q=&s; float *r=&f; return *p+*q+(int)(*r*8); }",
    ),
    (
        "int old(c,s,f) char c; short s; float f; { char *p=&c; short *q=&s; float *r=&f; *p+=2; *q-=3; *r+=0.5f; return *p+*q+(int)(*r*8); }",
        "int printf(const char *,...); int old(); int main(void){int (*p)(int,int,double)=old; printf(\"%d %d\\n\",old((char)-9,(short)-257,1.25f),p(-3,-200,2.5));return 0;}",
    ),
];

fn object(m: &keel_ir::Module, target: Target) -> Vec<u8> {
    let p = keel_codegen::compile_program(m, target).unwrap();
    match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin => {
            keel_asm::assemble_program_object(&p, Default::default()).unwrap()
        }
        Target::Aarch64LinuxGnu => {
            keel_asm::assemble_elf_program_object(&p, Default::default()).unwrap()
        }
        Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(&p).unwrap(),
    }
}
fn clang_syntax(source: &str, target: Target) -> std::process::Output {
    let triple = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin => "arm64-apple-macos11",
        Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
        Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
    };
    let mut p = Command::new("clang")
        .args([
            "-target",
            triple,
            "-std=c11",
            "-pedantic-errors",
            "-Wno-strict-prototypes",
            "-Wno-deprecated-non-prototype",
            "-x",
            "c",
            "-fsyntax-only",
            "-",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    p.stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    p.wait_with_output().unwrap()
}
#[test]
fn promoted_signatures_ir_and_actual_objects() {
    for target in TARGETS {
        let out = clang_syntax(SOURCE, target);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for debug_info in [false, true] {
            let original = keel_c::compile(
                SOURCE,
                "old-style.c",
                keel_c::Options { target, debug_info },
            )
            .unwrap();
            let f = original
                .functions
                .iter()
                .find(|f| f.name == "promoted")
                .unwrap();
            assert_eq!(
                f.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
                [Type::I32, Type::I32, Type::I32, Type::I32, Type::F64]
            );
            let attributed = original
                .functions
                .iter()
                .find(|f| f.name == "attributed")
                .unwrap();
            assert!(attributed.attrs.noinline && attributed.attrs.nounwind);
            let adjusted = original
                .functions
                .iter()
                .find(|f| f.name == "adjusted")
                .unwrap();
            assert_eq!(adjusted.restrict_params, [0]);
            for optimize in [false, true] {
                let mut m = original.clone();
                keel_ir::verify_module(&m).unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                assert!(!object(&m, target).is_empty());
            }
        }
    }
}
#[test]
fn interop_units_ir_and_objects_for_both_targets() {
    for (source, peer) in INTEROP_CASES.iter().skip(1) {
        for source in [source, peer] {
            for target in TARGETS {
                assert!(clang_syntax(source, target).status.success());
                for debug_info in [false, true] {
                    for optimize in [false, true] {
                        let mut m = keel_c::compile(
                            source,
                            "interop.c",
                            keel_c::Options { target, debug_info },
                        )
                        .unwrap();
                        keel_ir::verify_module(&m).unwrap();
                        if optimize {
                            keel_opt::optimize(&mut m).unwrap();
                        }
                        keel_ir::verify_module(&m).unwrap();
                        assert!(!object(&m, target).is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn declarations_scopes_and_prototype_clang_differential() {
    let accepted = [
        "int f(const int *); int f(a) const int a[]; {return a[0];} int f(const int *);",
        "typedef const int A[3]; int f(const int *); int f(a) A a; {return a[0];}",
        "int f(int *); int f(a) int *const a; {return a[0];} int f(int *);",
        "int f(int *); int f(a) int *restrict a; {return a[0];}",
        "int f(const int (*)[3]); int f(a) const int a[][3]; {return a[0][0];}",
        "enum E { K=3 }; int f(a) int a[sizeof(enum E)]; { return a[0]+K; }",
        "int f(int,double); int f(c,x) unsigned char c; float x; { return c+x; } int f(int,double);",
        "int f(x) unsigned short x; {return x;} int f(int); int f();",
        "unsigned f(x) unsigned x; {return x;} unsigned f(unsigned);",
        "int f(cb) int cb(int a[sizeof(enum {N=3})]); {enum E {N=4}; return N;}",
        "int f(x) int x; {return x;} int f(int); int g(void){return f(3.5);}",
        "int f(a,b) short b; char a; { return a+b; } int f(int,int);",
        "int f(int,double); int f(c,x) float x; char c; { return c+x; }",
        "int f(); int f(a) register int a; { return a; } int f(int);",
        "int f(a,b) int a,b; { return a+b; }",
        "int f(x,a) char x; int a[2][sizeof x]; { return sizeof *a; }",
        "long x; int f(x,a) int a[2][sizeof x]; char x; { return sizeof *a; }",
        "int f(cb) int cb(int x); { return cb(3); }",
        "int f(cb) int cb(int n,int a[n]); { int a[1]={4}; return cb(1,a); }",
        "int f(cb) int cb(struct S {int x;} *); { struct S {long y;} s; return sizeof s; }",
        "struct S{int x;}; struct S f(x) int x; { struct S s={x}; return s; }",
        "typedef int A[3]; int f(a) A a; { return a[2]; }",
        "typedef int I; int f(x) I x; { return x; } I x;",
        "int f(p) const int *restrict p; { return *p; }",
        "int f(p) int (*p)[3]; { return (*p)[2]; }",
        "struct S { int x; }; int f(p) struct S *p; { return p->x; }",
        "enum E { K=3 }; int f(x) enum E x; { return x+K; }",
    ];
    let rejected = [
        "int f(const int *); int f(a) int a[]; {return a[0];}",
        "int f(a) const int a[]; {return a[0];} int f(int *);",
        "int f(volatile int *); int f(a) int *a; {return a[0];}",
        "int f(const int **); int f(a) int **a; {return **a;}",
        "int f(const int (*)[3]); int f(a) int a[][3]; {return a[0][0];}",
        "int f(int (*)(const int *)); int f(cb) int cb(int *); {return 0;}",
        "int f(a) const int a[]; {a[0]=2; return 0;}",
        "typedef const int A[2]; int f(a) A a; {a[0]=2; return 0;}",
        "int f(x) int x; {return x;} int f(int); int g(void){return f();}",
        "int f(x) int x; {return x;} int f(int); int g(void){return f(1,2);}",
        "int f(x) unsigned short x; {return x;} int f(unsigned);",
        "int f(x) unsigned int x; {return x;} int f(int);",
        "int f(x) unsigned char x; {return x;} int f(unsigned char);",
        "int f(a,x) int x; int a[sizeof(enum {x=3})]; {return x;}",
        "int f(a) int a[sizeof(enum { K=3 })]; {return K;}",
        "int f(a) int (*a)[sizeof(struct S {int x;})]; {struct S s; return sizeof s;}",
        "struct S; int f(a) struct S a[]; { return 0; }",
        "int f(a) void a[2]; { return 0; }",
        "int f(a) int a[2][]; { return 0; }",
        "int f(x) enum { K=3 } x; { return x+K; }",
        "int f(x,a) int a[2][sizeof x]; char x; { return sizeof *a; }",
        "typedef int T; int f(x,T) int x,T; { return x+T; }",
        "int f(cb) int cb(int n,int a[n]); { return n; }",
        "int f(a,a) int a; { return a; }",
        "int f(a) int a; char a; { return a; }",
        "int f(a) int b; { return b; }",
        "int f(a) int a=1; { return a; }",
        "int f(a) extern int a; { return a; }",
        "int f(a) static int a; { return a; }",
        "int f(a) typedef int a; { return 0; }",
        "int f(a) auto int a; { return a; }",
        "int f(a) void a; { return 0; }",
        "int f(a) int; { return 0; }",
        "int f(a);",
        "int (*p)(a);",
        "typedef int T; int f(T) int T; { return T; }",
        "typedef int T; int f(T,x) int x; { return x; }",
        "int f(a) int a; { int a; return a; }",
        "int f(a) int a; { typedef int a; return 0; }",
        "int f(a) const int a; { a++; return a; }",
        "int f(a) int a; { return a; } int x=a;",
        "int f(int); int f() { return 0; }",
        "int f(char); int f(c) char c; { return c; }",
        "int f(float); int f(x) float x; { return x; }",
        "int f(int,int); int f(x) int x; { return x; }",
        "int f(x) int x; { return x; } int f(double);",
        "int f(x) float x; { return x; } int f(float);",
        "int f(); int f(char);",
        "int f(float); int f();",
        "int f(); int f(int,...);",
        "int f(int) int x; { return x; }",
    ];
    // Clang's verdict follows C prototype and scope rules, which do not depend
    // on the target, so one triple is the oracle for every keel target.
    for source in &accepted {
        let out = clang_syntax(source, TARGETS[0]);
        assert!(
            out.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for source in &rejected {
        let out = clang_syntax(source, TARGETS[0]);
        assert!(!out.status.success(), "Clang accepted {source}");
    }
    for target in TARGETS {
        for source in &accepted {
            let m = keel_c::compile(
                source,
                "accepted.c",
                keel_c::Options {
                    target,
                    debug_info: true,
                },
            )
            .unwrap_or_else(|e| panic!("{source}: {e}"));
            keel_ir::verify_module(&m).unwrap();
            assert!(!object(&m, target).is_empty());
        }
        for source in &rejected {
            assert!(
                keel_c::compile(
                    source,
                    "rejected.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "keel accepted {source}"
            );
        }
    }
}
#[test]
fn explicit_subset_limits() {
    // Some limits below are accepted by Clang extensions; keep them separate
    // from the strict C11 syntax rejection oracle.
    for (source, message) in [
        (
            "int f(a) int a[const 3]; {return a[0];}",
            "array static and qualifiers",
        ),
        (
            "int f(a) int a[static 3]; {return a[0];}",
            "array static and qualifiers",
        ),
        (
            "int f(int); int f(x) int x; {return x;} int g(void){return f();}",
            "too few arguments",
        ),
        (
            "int f(int); int f(x) int x; {return x;} int g(void){return f(1,2);}",
            "too many arguments",
        ),
        (
            "int f(int,...); int f(x) int x; { return x; }",
            "incompatible types",
        ),
        (
            "int f(a,b) int a; { return a; }",
            "explicit type declaration",
        ),
        ("int f() { return 0; } int f(int);", "incompatible types"),
        (
            "int f(a) const a; { return a; }",
            "explicit type declaration",
        ),
        (
            "typedef int T; int f(T) { return 1; }",
            "parameter needs a name",
        ),
        (
            "int f(n,a) int n; int a[n]; { return a[0]; }",
            "runtime array bounds",
        ),
        ("int f(a) int a[*]; { return a[0]; }", "star bounds"),
        (
            "struct S { int x; }; int f(x) struct S x; { return x.x; }",
            "unsupported old-style parameter type",
        ),
        (
            "int f(p) struct S { int x; } *p; { return p->x; }",
            "new tags and enumerators",
        ),
        (
            "int f(x) __int128 x; { return 0; }",
            "unsupported old-style parameter type",
        ),
        (
            "int f(x) _Atomic int x; { return x; }",
            "unsupported old-style parameter type",
        ),
    ] {
        let e = keel_c::compile(source, "subset.c", Default::default()).unwrap_err();
        assert!(e.to_string().contains(message), "{source}: {e}");
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn run(cmd: &mut Command) -> std::process::Output {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_runtime_and_clang_interoperability_both_directions() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("old-style-native");
    std::fs::create_dir_all(&dir).unwrap();
    for (index, (source, peer)) in INTEROP_CASES.iter().enumerate() {
        let c = dir.join(format!("case{index}.c"));
        let peer_c = dir.join(format!("peer{index}.c"));
        std::fs::write(&c, source).unwrap();
        std::fs::write(&peer_c, peer).unwrap();
        let out = |cmd: &mut Command| run(cmd);
        // Every link below reuses one clang object for the peer.
        let peer_o = dir.join(format!("peer{index}.o"));
        out(Command::new("clang")
            .args(["-std=c11", "-Wno-deprecated-non-prototype", "-c"])
            .arg(&peer_c)
            .arg("-o")
            .arg(&peer_o));
        let reference = dir.join(format!("reference{index}"));
        out(Command::new("clang")
            .args(["-std=c11", "-Wno-deprecated-non-prototype"])
            .arg(&c)
            .arg(&peer_o)
            .arg("-o")
            .arg(&reference));
        let want = out(&mut Command::new(&reference)).stdout;
        for debug_info in [false, true] {
            for optimize in [false, true] {
                let mut m = keel_c::compile(
                    source,
                    "interop.c",
                    keel_c::Options {
                        debug_info,
                        ..Default::default()
                    },
                )
                .unwrap();
                keel_ir::verify_module(&m).unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                let obj = dir.join(format!("case{index}-{debug_info}-{optimize}.o"));
                std::fs::write(&obj, object(&m, Target::Aarch64AppleDarwin)).unwrap();
                let exe = obj.with_extension("exe");
                out(Command::new("clang")
                    .arg(&obj)
                    .arg(&peer_o)
                    .arg("-o")
                    .arg(&exe));
                assert_eq!(
                    out(&mut Command::new(&exe)).stdout,
                    want,
                    "case={index} debug={debug_info} opt={optimize}"
                );
            }
        }
    }
}

#[test]
fn unsupported_unprototyped_calls_diagnose_before_invalid_ir() {
    for source in [
        "int f(x) int x; { return x; } int g(void){return f(1.0);}",
        "int f(); int g(void){return f(1.0);} int f(x) int x; {return x;}",
        "int f(x) int x; {return x;} int g(void){return f();}",
        "int f(x) int x; {return x;} int g(void){return f(1,2);}",
        "int f(); int g(void){return f(1)+f(1.0);}",
        "int f(); int g(void){return f()+f(1);}",
        "int f(); int g(void){return f();} int h(void){return f(1);}",
        "int f(); int g(void){return f(1.0);} int f(int);",
    ] {
        for target in TARGETS {
            match keel_c::compile(
                source,
                "bad-call.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            ) {
                Err(e) => assert!(
                    e.to_string().contains("unprototyped call arguments"),
                    "{source}: {e}"
                ),
                Ok(m) => panic!(
                    "accepted unsupported call: {source}; verify={:?}",
                    keel_ir::verify_module(&m)
                ),
            }
        }
    }
}
