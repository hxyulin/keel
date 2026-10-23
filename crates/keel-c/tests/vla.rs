use keel_ir::{InstData, Intrinsic, Target};

const SOURCE: &str = include_str!("programs/vla.c");
const PARAM_SOURCE: &str = include_str!("programs/vm_parameters.c");
const VM_SOURCE: &str = include_str!("programs/vm_arrays.c");

fn object(m: &keel_ir::Module, target: Target) -> Vec<u8> {
    let program = keel_codegen::compile_program(m, target).unwrap();
    match target {
        Target::Aarch64AppleDarwin => {
            keel_asm::assemble_program_object(&program, Default::default()).unwrap()
        }
        Target::Aarch64LinuxGnu => {
            keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap()
        }
        Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(&program).unwrap(),
    }
}

#[test]
fn vla_ir_and_both_targets() {
    for source in [SOURCE, VM_SOURCE, PARAM_SOURCE] {
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            for debug_info in [false, true] {
                for optimize in [false, true] {
                    let mut m =
                        keel_c::compile(source, "vla.c", keel_c::Options { target, debug_info })
                            .unwrap();
                    keel_ir::verify_module(&m).unwrap();
                    let count = |op| {
                        m.functions
                        .iter()
                        .flat_map(|f| f.layout_insts().map(move |(_, i)| f.inst_data(i)))
                        .filter(
                            |i| matches!(i, InstData::Intrinsic { id: found, .. } if *found==op),
                        )
                        .count()
                    };
                    if source != PARAM_SOURCE {
                        assert!(count(Intrinsic::StackAlloc) >= 10);
                    } else {
                        assert_eq!(count(Intrinsic::StackAlloc), 0);
                        assert_eq!(count(Intrinsic::StackSave), 0);
                        assert_eq!(count(Intrinsic::StackRestore), 0);
                    }
                    if source == SOURCE {
                        assert!(count(Intrinsic::StackSave) >= 10);
                        assert!(count(Intrinsic::StackRestore) >= 10);
                        // The function body uses scope None; the first nested block
                        // uses Some(0). Root-only VLAs must still save and restore.
                        for name in ["comma_bound", "nothing", "fallthrough"] {
                            let f = m.functions.iter().find(|f| f.name == name).unwrap();
                            let has = |id| {
                                f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Intrinsic { id: found, .. } if *found == id))
                            };
                            assert!(has(Intrinsic::StackSave), "missing root save: {name}");
                            assert!(has(Intrinsic::StackRestore), "missing root restore: {name}");
                        }
                    }
                    if source == VM_SOURCE {
                        let f = m
                            .functions
                            .iter()
                            .find(|f| f.name == "capture_only")
                            .unwrap();
                        assert!(
                            !f.layout_insts().any(|(_, i)| matches!(
                                f.inst_data(i),
                                InstData::Intrinsic {
                                    id: Intrinsic::StackAlloc
                                        | Intrinsic::StackSave
                                        | Intrinsic::StackRestore,
                                    ..
                                }
                            )),
                            "VM typedefs and pointers must capture bounds without dynamic allocation"
                        );
                    }
                    if optimize {
                        keel_opt::optimize(&mut m).unwrap();
                    }
                    keel_ir::verify_module(&m).unwrap();
                    keel_codegen::compile(&m, target).unwrap();
                    assert!(!object(&m, target).is_empty());
                }
            }
        }
    }
}

#[test]
fn qualified_vm_typedef_elements_remain_volatile() {
    let source = "int row(int n) { typedef int Row[n]; volatile Row a; a[0]=3; a[0]; return a[0]; }
        int matrix(int n) { typedef int Matrix[2][n]; volatile Matrix a; a[1][0]=3; a[1][0]; return a[1][0]; }
        int pointer(int n) { typedef int Row[n]; Row a; a[0]=3; const volatile Row *p=&a; (*p)[0]; return (*p)[0]; }";
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for optimize in [false, true] {
            let mut m = keel_c::compile(
                source,
                "qualified-vm.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for f in &m.functions {
                let loads = f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { flags, .. } if flags.volatile)).count();
                let stores = f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Store { flags, .. } if flags.volatile)).count();
                assert_eq!(loads, 2, "{} optimize={optimize}", f.name);
                assert_eq!(
                    stores,
                    usize::from(f.name != "pointer"),
                    "{} optimize={optimize}",
                    f.name
                );
            }
            assert!(!object(&m, target).is_empty());
        }
    }
}

#[test]
fn unsupported_variably_modified_cases_are_diagnosed() {
    let cases = [
        (
            "int f(int a[*]) { return 0; }",
            "only allowed in function prototypes",
        ),
        (
            "int f(int n, int a[n][*]) { return 0; }",
            "only allowed in function prototypes",
        ),
        ("int a[*];", "star bounds require a prototype"),
        ("int a[static 3];", "require a parameter"),
        ("int f(int n, int a[static]);", "requires a bound"),
        ("int f(int a[static *]);", "without static"),
        ("int f(int a[static static 3]);", "duplicate static"),
        ("int f(int a[2][static 3]);", "outermost parameter array"),
        ("int f(int a[2][const 3]);", "outermost parameter array"),
        ("int f(int (*a)[static 3]);", "outermost parameter array"),
        ("int f(int a[_Atomic 3]);", "atomic array parameter"),
        ("int f(int n, int n);", "duplicate parameter"),
        (
            "int f(int n) { int n; return n; }",
            "duplicate local or parameter",
        ),
        ("int f(int a[n], int n);", "is not declared"),
        ("int f(int n, int a[n]); int x=n;", "is not declared"),
        (
            "int f(int n, int a[n]); int g(int a[n]);",
            "is not declared",
        ),
        ("typedef int T; int f(int T, T a);", "old-style parameter"),
        (
            "int f(int n, int (*cb)(int x), int a[x]);",
            "is not declared",
        ),
        ("int f(int a[][]);", "complete object type"),
        (
            "int f(int a[2][3]); int f(int a[2][4]);",
            "incompatible types",
        ),
        (
            "int f(int n, int a[n]); int f(int n);",
            "incompatible types",
        ),
        ("int f(void x);", "void type"),
        (
            "int f(int n, int a[sizeof(int[n])]);",
            "runtime array bounds",
        ),
        ("int f(int a[static -1]);", "negative"),
        (
            "int f(int n, double a[n][n]); int f(int n, int a[n][n]);",
            "incompatible types",
        ),
        ("int f(static int n);", "storage class"),
        ("int f(auto int n);", "storage class"),
        ("int f(inline int n);", "storage class"),
        (
            "int f(int n) { typedef int n; return 0; }",
            "duplicate local or parameter",
        ),
        (
            "int f(int n, int a[n]) { goto end; end: return 0; }",
            "goto in a function",
        ),
        (
            "int f(int n, int a[n]) { switch(n) { default: return 0; } }",
            "switch in a function",
        ),
        (
            "int f(int n, int a[n]) { __builtin_alloca(8); return 0; }",
            "alloca in a function",
        ),
        (
            "int f(int n, int a[const n]) { a++; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n, const int a[n]) { a[0]=1; return 0; }",
            "const-qualified",
        ),
        (
            "typedef int F(int n, int a[n]); F f { return 0; }",
            "direct parameter list",
        ),
        (
            "int f(int n){ int a[2][n]; int (*const p)[n]=a; p++; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ typedef int Row[n]; Row *const p=0; p+=1; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ typedef int Row[n]; Row *const table[2]={0,0}; --table[0]; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ typedef int Row[n]; const Row a; a[0]=1; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ typedef int Row[n]; Row a; const Row *p=&a; (*p)[0]+=1; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ typedef int Row[n]; Row a; const Row *p=&a; Row *q=&a; (n?p:q)[0][0]=1; return 0; }",
            "const-qualified",
        ),
        (
            "int f(int n){ int a[sizeof(struct {int b[n];})]; return 0; }",
            "runtime array bounds",
        ),
        (
            "int f(int n){ static int a[n]; return 0; }",
            "runtime array bounds",
        ),
        (
            "int f(int n){ extern int a[n]; return 0; }",
            "runtime array bounds",
        ),
        ("int n; int a[n];", "runtime array bounds"),
        (
            "int f(int n){ int a[n][2]={1}; return 0; }",
            "cannot have initializers",
        ),
        (
            "int f(int n){ int a[2][n]={1}; return 0; }",
            "cannot have initializers",
        ),
        (
            "int f(int n){ int a[n]; typeof(a) b; return 0; }",
            "typeof of a variably modified",
        ),
        ("int f(double n){ int a[n]; return 0; }", "integer type"),
        ("int f(int n){ void a[n]; return 0; }", "complete object"),
        ("int f(int n){ int a[n][]; return 0; }", "complete object"),
        (
            "int f(int n){ int a[n][2][]; return 0; }",
            "complete object",
        ),
        (
            "int f(int n){ int (*p)[n]; return _Generic(p,int (*)[3]:1,default:0); }",
            "variably modified `_Generic`",
        ),
        (
            "int f(int n){ int a[n] __attribute__((aligned(32))); return 0; }",
            "VLA alignment",
        ),
        (
            "int f(int n){ int a[2][n] __attribute__((aligned(32))); return 0; }",
            "VLA alignment",
        ),
        (
            "int f(int n){ typedef int A[n]; struct S { A a; }; return 0; }",
            "variably modified structure members",
        ),
        (
            "int f(int n){ typedef int A[n]; struct S { A *p; }; return 0; }",
            "variably modified structure members",
        ),
        (
            "int f(int n){ typedef int A[n]; typedef A F(void); return 0; }",
            "variably modified function return",
        ),
        (
            "int f(int n){ typedef int A[n]; return __builtin_types_compatible_p(A,A); }",
            "variably modified type in this type-name context",
        ),
        (
            "int f(int n){ typedef int A[n]; return _Generic(0,A*:1,default:0); }",
            "variably modified type in this type-name context",
        ),
        (
            "int f(int n){ typedef int A[n]; A *g(void); return 0; }",
            "variably modified function return",
        ),
        (
            "int f(int n){ typedef int A[n]; static A a; return 0; }",
            "automatic block storage",
        ),
        (
            "int f(int n){ typedef int A[n]; static A *p; return 0; }",
            "automatic block storage",
        ),
        (
            "int f(int n){ typedef int A[n]; static __auto_type p=(A*)0; return 0; }",
            "automatic block storage",
        ),
        (
            "int f(int n){ typedef int A[n]; extern A *p; return 0; }",
            "automatic block storage",
        ),
        (
            "int f(int n){ return (int[n]){0}[0]; }",
            "variably modified compound literals",
        ),
        (
            "int f(int n){ int (*_Atomic p)[n]; return 0; }",
            "atomic variably modified",
        ),
        (
            "int f(int n){ goto end; int a[n]; end:return 0; }",
            "goto in a function",
        ),
        (
            "int f(int n){ typedef int A[n]; goto end; end:return 0; }",
            "goto in a function",
        ),
        (
            "int f(int n){ int (*p)[n]; goto end; end:return 0; }",
            "goto in a function",
        ),
        (
            "int f(int n){ int a[n]; goto *&&end; end:return 0; }",
            "computed goto",
        ),
        (
            "int f(int n){ int a[n]; switch(n){case 1:return 1;} return 0; }",
            "switch in a function",
        ),
        (
            "int f(int n){ typedef int A[n]; switch(n){case 1:return 1;} return 0; }",
            "switch in a function",
        ),
        (
            "int f(int n){ int a[n]; void *p=__builtin_alloca(16); return 0; }",
            "alloca in a function",
        ),
        (
            "int f(int n){ typedef int A[n]; void *p=__builtin_alloca(16); return 0; }",
            "alloca in a function",
        ),
    ];
    for (src, message) in cases {
        let err = keel_c::compile(src, "reject-vla.c", Default::default())
            .err()
            .unwrap_or_else(|| panic!("unexpectedly accepted: {src}"))
            .to_string();
        assert!(
            err.contains(message),
            "{src}: expected {message}, got {err}"
        );
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_arm64_matches_clang_with_and_without_optimization() {
    use std::process::Command;
    fn run(cmd: &mut Command) -> std::process::Output {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    for (name, src) in [
        ("vla-native", SOURCE),
        ("vm-native", VM_SOURCE),
        ("vm-parameters", PARAM_SOURCE),
    ] {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let c = dir.join("vla.c");
        std::fs::write(&c, &src).unwrap();
        let reference = dir.join("clang");
        run(Command::new("clang")
            .arg("-O0")
            .arg(&c)
            .arg("-o")
            .arg(&reference));
        let want = run(&mut Command::new(&reference));
        run(Command::new("clang")
            .arg("-O2")
            .arg(&c)
            .arg("-o")
            .arg(&reference));
        assert_eq!(want.stdout, run(&mut Command::new(&reference)).stdout);
        for debug_info in [false, true] {
            for optimize in [false, true] {
                let opts = keel_c::Options {
                    debug_info,
                    ..Default::default()
                };
                let mut m = keel_c::compile(&src, "vla.c", opts).unwrap();
                keel_ir::verify_module(&m).unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                let asm = keel_codegen::compile(&m, opts.target).unwrap();
                let path = dir.join(format!("keel-{debug_info}-{optimize}"));
                let asm_path = path.with_extension("s");
                std::fs::write(&asm_path, asm).unwrap();
                run(Command::new("clang").arg(&asm_path).arg("-o").arg(&path));
                let got = run(&mut Command::new(&path));
                let obj_path = path.with_extension("o");
                std::fs::write(&obj_path, object(&m, opts.target)).unwrap();
                run(Command::new("clang").arg(&obj_path).arg("-o").arg(&path));
                assert_eq!(
                    run(&mut Command::new(&path)).stdout,
                    want.stdout,
                    "direct object debug={debug_info}, optimize={optimize}"
                );
                assert_eq!(
                    got.stdout, want.stdout,
                    "debug={debug_info}, optimize={optimize}"
                );
            }
        }
    }
}

#[test]
fn parameter_scope_and_constraint_differential_clang() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let accepted = [
        "typedef int T; int f(int T, int a[T]); T v; int f(int T, int *a) { return a[T-1]; }",
        "int f(int n, int a[n]); int f(int x, int *p) { goto end; end: return p[x-1]; } int f(int r, int a[r]);",
        "int f(int n) { int g(int x, int a[n++][x]); switch(n) { default: return n; } }",
        "int f(enum { Bound=3 } n, int a[Bound]) { return Bound+a[0]; } int Bound;",
        "int f(struct Tag { int x; } p) { struct Tag q=p; return q.x; } struct Tag { long y; };",
        "typedef void V; int f(V); int f(void __attribute__((unused))) { return 1; }",
        "typedef int Row[3]; long f(int n, Row a[n]) { return sizeof *a+a[n-1][2]; }",
        "int f(int n, int (*cb)(int x, int a[*]), int a[n]) { return cb(n,a); }",
        "int n=3; int f(int n, int a[n]), g(int a[n]); int g(int *a) { return a[0]; }",
        "typedef int T; int f(int T, int a[T]), g(T n, int a[n]);",
        "int f(enum { N=3 } n, int a[N]); int N=4; int g(int a[N]);",
        "int f(struct S { int n; } s); struct S { long n; }; int g(struct S s) { return s.n; }",
        "int f(int n, int a[n][3]); int f(int n, int a[2][n]) { return sizeof *a; } int f(int n, int a[*][3]);",
        "int f(int n, int *a) { int (*p)(int x, int a[x][n++]), (*q)(int y, int a[y]); goto end; end: return n; }",
        "int (*factory(int n, int a[n]))(int x, int a[x]) { return n && a[0] ? factory(0,a) : 0; }",
        "int f(void __attribute__((noinline))); int f(void) { return 0; }",
    ];
    let rejected = [
        "int f(int n, int n);",
        "int f(int n) { typedef int n; return 0; }",
        "int f(int n, int a[n]); int x=n;",
        "typedef int T; int f(int T, T a);",
        "int f(int n, int (*cb)(int x), int a[x]);",
        "int f(int a[2][static 3]);",
        "int f(int a[2][const 3]);",
        "int f(int a[static]);",
        "int f(int n, int a[n][*]) { return 0; }",
        "int f(int a[][]);",
        "int f(int a[2][3]); int f(int a[2][4]);",
        "int f(int n, int a[n]); int f(int n);",
        "int f(void x);",
        "int f(void x __attribute__((unused)));",
        "int f(void, int);",
        "int f(int, void);",
        "int f(void, ...);",
        "int (*factory(int n, int a[n]))(int x, int a[x]) { return x; }",
        "int f(const void);",
        "int f(volatile void __attribute__((unused)));",
        "typedef const void V; int f(V);",
        "typedef volatile void V; int f(V) { return 0; }",
        "int f(auto int n);",
        "int f(int n, int a[const n]) { a++; return 0; }",
        "int f(int n, const int a[n]) { a[0]=1; return 0; }",
    ];
    for (sources, success) in [(&accepted[..], true), (&rejected[..], false)] {
        for &source in sources {
            for target in Target::ALL {
                let triple = match target {
                    Target::Aarch64AppleDarwin => "arm64-apple-macos11",
                    Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
                    Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
                };
                let mut child = Command::new("clang")
                    .args([
                        "-target",
                        triple,
                        "-std=c11",
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
                let out = child.wait_with_output().unwrap();
                assert_eq!(
                    out.status.success(),
                    success,
                    "Clang: {source}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let result = keel_c::compile(
                    source,
                    "parameter-scope.c",
                    keel_c::Options {
                        target,
                        debug_info: true,
                    },
                );
                assert_eq!(result.is_ok(), success, "Keel: {source}: {result:?}");
                if let Ok(module) = result {
                    keel_ir::verify_module(&module).unwrap();
                    assert!(!object(&module, target).is_empty());
                }
            }
        }
    }
}
