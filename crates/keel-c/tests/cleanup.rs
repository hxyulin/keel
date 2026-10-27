use keel_ir::Target;

const SOURCE: &str = include_str!("fixtures/cleanup.c");

fn object(m: &keel_ir::Module, target: Target) -> Vec<u8> {
    let program = keel_codegen::compile_program(m, target).unwrap();
    match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
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
fn cleanup_ir_and_both_targets() {
    for target in Target::NATIVE {
        for debug_info in [false, true] {
            for optimize in [false, true] {
                let mut m =
                    keel_c::compile(SOURCE, "cleanup.c", keel_c::Options { target, debug_info })
                        .unwrap();
                keel_ir::verify_module(&m).unwrap();
                let f = m.functions.iter().find(|f| f.name == "vlas").unwrap();
                let actions: Vec<&str> = f
                    .layout_insts()
                    .filter_map(|(_, i)| match f.inst_data(i) {
                        keel_ir::InstData::Call { func, .. } => Some(f.callee(*func).name.as_str()),
                        keel_ir::InstData::Intrinsic {
                            id: keel_ir::Intrinsic::StackRestore,
                            ..
                        } => Some("restore"),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    actions,
                    [
                        "record",
                        "restore",
                        "with_vla",
                        "array_cleanup",
                        "restore",
                        "before_vla"
                    ]
                );
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
    for (name, src) in [("cleanup-native", SOURCE)] {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let c = dir.join("cleanup.c");
        std::fs::write(&c, &src).unwrap();
        let reference = dir.join("clang");
        // Clang elides the return copy for named C struct locals by
        // default, letting their cleanup mutate the result. This milestone
        // explicitly freezes return values before callbacks; disable that
        // optional elision in the oracle (also at -O2).
        run(Command::new("clang")
            .arg("-fno-elide-constructors")
            .arg("-O0")
            .arg(&c)
            .arg("-o")
            .arg(&reference));
        let want = run(&mut Command::new(&reference));
        run(Command::new("clang")
            .arg("-fno-elide-constructors")
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
                let mut m = keel_c::compile(&src, "cleanup.c", opts).unwrap();
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
fn invalid_cleanup_declarations_and_unmodeled_exits_are_diagnosed() {
    let cases = [
        (
            "void c(int*); int x __attribute__((cleanup(c)));",
            "automatic local",
        ),
        (
            "void c(int*); int f(void) __attribute__((cleanup(c)));",
            "automatic local",
        ),
        (
            "void c(int*); int f(void){static int x __attribute__((cleanup(c))); return 0;}",
            "automatic local",
        ),
        (
            "void c(int*); int f(void){extern int x __attribute__((cleanup(c))); return 0;}",
            "automatic local",
        ),
        (
            "void c(int*); int f(void){_Thread_local int x __attribute__((cleanup(c))); return 0;}",
            "automatic local",
        ),
        (
            "void c(int*); typedef int T __attribute__((cleanup(c)));",
            "automatic local",
        ),
        (
            "void c(int*); int f(int x __attribute__((cleanup(c)))) {return x;}",
            "automatic local",
        ),
        (
            "void c(int*); struct S {int x __attribute__((cleanup(c)));};",
            "automatic local",
        ),
        (
            "void c(int*); int f(void){return sizeof(int __attribute__((cleanup(c))));}",
            "automatic local",
        ),
        (
            "int f(void){ int x __attribute__((cleanup(missing)))=0; return 0;}",
            "declared function",
        ),
        (
            "int c; int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "declared function",
        ),
        (
            "void (*c)(int*); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "declared function",
        ),
        (
            "void c(int*); int f(void){ void (*p)(int*)=c; int x __attribute__((cleanup(p)))=0; return 0;}",
            "declared function",
        ),
        (
            "void c(void); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int*,int); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int*,...); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(double*); int f(void){ int x __attribute__((cleanup(c)))=0; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int*); int f(void){ int a[2] __attribute__((cleanup(c)))={0}; return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int*); int f(void){ int x __attribute__((cleanup(c)))=0; int x=1; return 0;}",
            "duplicate local or parameter name",
        ),
        (
            "void c(int*); int f(void){int * __attribute__((cleanup(c))) p=0;return 0;}",
            "exactly one compatible",
        ),
        (
            "void c(int**); int f(void){__attribute__((cleanup(c))) int * __attribute__((cleanup(c))) p=0;return 0;}",
            "multiple cleanup",
        ),
        (
            "void c(int*); int f(void){ int x=1; int x __attribute__((cleanup(c)))=0; return 0;}",
            "duplicate local or parameter name",
        ),
        (
            "void c(int*); int f(void){ int x __attribute__((cleanup(c),cleanup(c)))=0; return 0;}",
            "multiple cleanup",
        ),
        (
            "void c(int*); int f(void){ __attribute__((cleanup(c))) int x __attribute__((cleanup(c)))=0; return 0;}",
            "multiple cleanup",
        ),
        (
            "struct S; struct S c(int*); static int f(void){int x __attribute__((cleanup(c)))=1;return 0;}",
            "incomplete type",
        ),
        (
            "void c(int*) __attribute__((noreturn)); int f(void){int x __attribute__((cleanup(c)))=1;return 0;}",
            "return normally",
        ),
        (
            "void c(int*) __attribute__((returns_twice)); int f(void){int x __attribute__((cleanup(c)))=1;return 0;}",
            "return normally",
        ),
        (
            "void c(int*); static inline int f(void){int x __attribute__((cleanup(c)))=1;return 0;} void c(int*) __attribute__((noreturn));",
            "return normally",
        ),
        (
            "void c(int*); static inline int f(void){int x __attribute__((cleanup(c)))=1;return 0;} void c(int*) __attribute__((returns_twice));",
            "return normally",
        ),
        (
            "void c(int*); static inline int f(void){goto end; int x __attribute__((cleanup(c)))=1;end:return 0;}",
            "goto in a function",
        ),
        (
            "void c(int*); int twice(void*); static inline int f(void){int x __attribute__((cleanup(c)))=1;return twice(0);} int twice(void*) __attribute__((returns_twice));",
            "returns-twice",
        ),
        (
            "void c(int*); int twice(void*); static int f(void){int x __attribute__((cleanup(c)))=1;return ({ twice(0); 1; });} int twice(void*) __attribute__((returns_twice));",
            "returns-twice",
        ),
        (
            "struct S {int x;}; void c(int*); int f(void){int x __attribute__((cleanup(c)))=1;return x;} struct S c(int *p){return (struct S){*p};}",
            "incompatible return-type redeclarations",
        ),
        (
            "struct S {int x;}; void c(int*); struct S c(int*); static int f(void){int x __attribute__((cleanup(c)))=1;return x;}",
            "incompatible return-type redeclarations",
        ),
        (
            "struct S {int x;}; struct S c(int*); int f(void){int x __attribute__((cleanup(c)))=1;return x;} void c(int *p){}",
            "incompatible return-type redeclarations",
        ),
        (
            "void c(int*); int f(void){int x __attribute__((cleanup(c)))=1;goto end;end:return 0;}",
            "goto in a function",
        ),
        (
            "void c(int*); int f(void){int x __attribute__((cleanup(c)))=1;goto *&&end;end:return 0;}",
            "computed goto",
        ),
        (
            "void c(int*); void longjmp(void*,int); int f(void){int x __attribute__((cleanup(c)))=1;longjmp(0,1);return 0;}",
            "nonlocal exit",
        ),
        (
            "int c(void*) __asm__(\"_setjmp\"); static int f(void){int x __attribute__((cleanup(c)))=1;return 0;}",
            "return normally",
        ),
        (
            "void c(int*); int twice(void*); static int f(void){int x __attribute__((cleanup(c)))=1;return twice(0);} int twice(void*) __asm__(\"_setjmp\");",
            "returns-twice",
        ),
        (
            "void c(int*); void jump(void*,int) __asm__(\"_longjmp\"); static int f(void){int x __attribute__((cleanup(c)))=1;jump(0,1);return 0;}",
            "nonlocal exit",
        ),
        (
            "void c(int*); int setjmp(void*); static int f(void){setjmp(0); int x __attribute__((cleanup(c)))=1;return 0;}",
            "returns-twice",
        ),
        (
            "void c(int*); int twice(void*) __attribute__((returns_twice)); int f(void){twice(0); int x __attribute__((cleanup(c)))=1;return 0;}",
            "returns-twice",
        ),
    ];
    for target in Target::NATIVE {
        for (source, expected) in cases {
            let error = keel_c::compile(
                source,
                "reject-cleanup.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(
                error.message.contains(expected),
                "{source}: expected {expected}, got {error}"
            );
        }
    }
}

#[test]
fn unused_locals_keep_hidden_address_and_callback_reference() {
    let source =
        "static void c(int *p){*p=9;} int f(void){int x __attribute__((cleanup(c)))=1;return 0;}";
    for target in Target::NATIVE {
        let mut m = keel_c::compile(
            source,
            "hidden-address.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        let f = m.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(
            f.layout_insts()
                .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Call { .. }))
        );
        assert!(m.functions.iter().any(|f| f.name == "c"));
        keel_ir::verify_module(&m).unwrap();
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(!object(&m, target).is_empty());
    }
}

#[test]
fn raw_attribute_macros_work_but_general_capability_query_keeps_fallback() {
    let source = r#"
        #if __has_attribute(cleanup) || __has_attribute(__cleanup__)
        #error general GNU cleanup control flow is not implemented
        #endif
        #define CLEANUP(f) __attribute__((__cleanup__(f)))
        static void done(int *p) { *p=7; }
        int run(void) { int value CLEANUP(done)=3; return value; }
    "#;
    for target in Target::NATIVE {
        let text = keel_c::preprocess(
            source,
            "cleanup-macro.c",
            &keel_c::PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let m = keel_c::compile(
            &text,
            "cleanup-macro.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(m.functions.iter().any(|f| f.name == "done"));
        assert!(!object(&m, target).is_empty());
    }
}

#[test]
fn callbacks_keep_ordinary_weak_and_visibility_semantics() {
    for target in Target::NATIVE {
        for late in [false, true] {
            let attrs = "void c(int*) __attribute__((weak, visibility(\"default\")));";
            let body =
                "void c(int *p){*p=9;} int f(void){int x __attribute__((cleanup(c)))=1;return x;}";
            let source = if late {
                format!("void c(int*); {body} {attrs}")
            } else {
                format!("{attrs} {body}")
            };
            let mut m = keel_c::compile(
                &source,
                "weak-cleanup.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            assert!(
                m.functions
                    .iter()
                    .find(|f| f.name == "c")
                    .unwrap()
                    .attrs
                    .weak
            );
            keel_ir::verify_module(&m).unwrap();
            keel_opt::optimize(&mut m).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert!(!object(&m, target).is_empty());
        }
        let source = "void c(int*); int f(void){int x __attribute__((cleanup(c)))=1;return x;} void c(int*) __attribute__((visibility(\"hidden\"))); void c(int *p){*p=9;}";
        let m = keel_c::compile(
            source,
            "hidden-cleanup.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        assert!(m.functions.iter().find(|f| f.name == "c").unwrap().hidden);
        assert!(!object(&m, target).is_empty());
    }
}
