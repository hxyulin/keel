use keel_ir::Target;
const SOURCE: &str = include_str!("programs/atomic_representations.c");
#[test]
fn representations_both_targets() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug_info in [false, true] {
            for optimize in [false, true] {
                let mut m =
                    keel_c::compile(SOURCE, "atomic.c", keel_c::Options { target, debug_info })
                        .unwrap();
                keel_ir::verify_module(&m).unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                keel_codegen::compile(&m, target).unwrap();
                keel_codegen::compile_program(&m, target).unwrap();
            }
        }
    }
}
#[test]
fn unsupported_representations() {
    for src in [
        "_Atomic(__int128) a;",
        "void f(__int128 *p, __int128 *q){__atomic_load(p,q,5);}",
        "void f(int *p){__atomic_load(p,5);}",
        "void f(void){__atomic_thread_fence(6);}",
        "void f(void){__c11_atomic_signal_fence(-1);}",
        "void f(_Atomic float *p){__c11_atomic_load(p,3);}",
        "void f(_Atomic float *p, float *e){__c11_atomic_compare_exchange_strong(p,e,1.0f,0,2);}",
        "void f(int *p,int *q){__atomic_compare_exchange(p,q,q,0,5,3);}",
        "_Atomic(struct { int x; }) a;",
        "struct S { char x[3]; }; void f(struct S *p, struct S *q){__atomic_load(p,q,5);}",
        "struct S { char x[8]; }; void f(struct S *p, struct S *q){__atomic_store(p,q,5);}",
        "struct __attribute__((packed)) S { char c; int x; }; void f(struct S *p, struct S *q){__atomic_exchange(p,q,q,5);}",
        "void f(int *p, short *q){__atomic_load(p,q,5);}",
        "void f(int *p, _Atomic int *q){__atomic_load(p,q,5);}",
        "void f(_Atomic int *p, _Atomic int *q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        "struct __attribute__((packed)) S { char c; int x; }; void f(struct S *p, int *q){__atomic_load(&p->x,q,5);}",
    ] {
        assert!(
            keel_c::compile(src, "reject.c", Default::default()).is_err(),
            "{src}"
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
    let dir =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("atomic-representations-native");
    std::fs::create_dir_all(&dir).unwrap();
    let src = SOURCE;
    let c = dir.join("atomic_representations.c");
    std::fs::write(&c, src).unwrap();
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
            let mut m = keel_c::compile(src, "atomic_representations.c", opts).unwrap();
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
            let got = run(&mut Command::new(path));
            assert_eq!(
                got.stdout, want.stdout,
                "debug={debug_info}, optimize={optimize}"
            );
        }
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_mixed_clang_keel_contention() {
    use std::process::Command;
    fn run(cmd: &mut Command) -> std::process::Output {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}: {} {}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        out
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("atomic-contention");
    std::fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    std::fs::write(&driver, include_str!("fixtures/atomic_stress_driver.c")).unwrap();
    for optimize in [false, true] {
        let mut m = keel_c::compile(
            include_str!("fixtures/atomic_stress.c"),
            "stress.c",
            Default::default(),
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        if optimize {
            keel_opt::optimize(&mut m).unwrap();
        }
        keel_ir::verify_module(&m).unwrap();
        let asm = keel_codegen::compile(&m, Target::Aarch64AppleDarwin).unwrap();
        let path = dir.join(format!("stress-{optimize}"));
        let assembly = path.with_extension("s");
        std::fs::write(&assembly, asm).unwrap();
        run(Command::new("clang")
            .arg("-O2")
            .arg("-pthread")
            .arg(&assembly)
            .arg(&driver)
            .arg("-o")
            .arg(&path));
        for _ in 0..3 {
            assert_eq!(
                run(&mut Command::new(&path)).stdout,
                b"80000 80000 80000 80000\n"
            );
        }
    }
}

#[test]
fn volatile_expected_is_written_only_on_failure() {
    use keel_ir::InstData;
    let source = "int scalar(_Atomic float *p, volatile float *e) { return __c11_atomic_compare_exchange_strong(p,e,1.0f,4,2); }\n\
                  int generic(int *p, volatile int *e, int *d) { return __atomic_compare_exchange(p,e,d,0,4,2); }";
    for target in Target::NATIVE {
        let mut m = keel_c::compile(
            source,
            "volatile-cas.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for f in &m.functions {
                let stores: Vec<_> = f
                    .layout_insts()
                    .filter_map(|(block, i)| match f.inst_data(i) {
                        InstData::Store { flags, .. } if flags.volatile => Some(block),
                        _ => None,
                    })
                    .collect();
                assert_eq!(stores.len(), 1);
                assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Load { flags, .. } if flags.volatile)));
                // Success bypasses the observable write; failure enters it.
                assert!(f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Brif { dests, .. } if dests[0].block != stores[0] && dests[1].block == stores[0])));
            }
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

fn clang_syntax(source: &str, target: Target, accepted: bool) {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let triple = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin => "arm64-apple-macos11",
        Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
        Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
    };
    let mut child = Command::new("clang")
        .args([
            "-target",
            triple,
            "-std=gnu11",
            "-Werror=atomic-memory-ordering",
            "-Werror=incompatible-pointer-types-discards-qualifiers",
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
fn scalar_builtin_families_and_writable_buffers_match_clang() {
    for source in [
        "float f(float *p){return __atomic_load_n(p,5);}",
        "void f(double *p){__atomic_store_n(p,1.0,5);}",
        "float f(float *p){return __atomic_exchange_n(p,1.0f,5);}",
        "int f(float *p,float *e){return __atomic_compare_exchange_n(p,e,1.0f,0,5,2);}",
        "float f(float *p){return __sync_val_compare_and_swap(p,0,1);}",
        "int f(int *p){return __c11_atomic_load(p,5);}",
        "typedef int V __attribute__((vector_size(16))); V f(V *p){return __atomic_load_n(p,5);}",
        "void f(const int *p,int *q){__atomic_store(p,q,5);}",
        "void f(int *p,const int *q){__atomic_load(p,q,5);}",
        "void f(int *p,int *q,const int *r){__atomic_exchange(p,q,r,5);}",
        "void f(int *p,const int *e,int *d){__atomic_compare_exchange(p,e,d,0,5,2);}",
        "void f(_Atomic float *p,const float *e){__c11_atomic_compare_exchange_strong(p,e,1,5,2);}",
        "void f(const _Atomic float *p){__c11_atomic_store(p,1.0f,5);}",
        "void f(const int *p){__atomic_clear(p,5);}",
        "void f(int *p){__atomic_clear(p,2);}",
        "void f(int *p){__atomic_clear(p,4);}",
        "void f(int *p){__atomic_test_and_set(p,9);}",
    ] {
        for target in Target::NATIVE {
            clang_syntax(source, target, false);
            assert!(
                keel_c::compile(
                    source,
                    "atomic-review.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {source}"
            );
        }
    }
    for source in [
        "float f(const _Atomic float *p){return __c11_atomic_load(p,2);}",
        "float f(_Atomic float *p,float *e){return __c11_atomic_compare_exchange_strong(p,e,1,5,2);}",
        "void f(const float *p,float *q){__atomic_load(p,q,2);}",
        "typedef struct { int x,y; } S __attribute__((aligned(8))); void f(S *p,S *q){__atomic_load(p,q,5);}",
        "void f(float *p,float *q,float *r){__atomic_store(p,q,3); __atomic_exchange(p,q,r,4); __atomic_compare_exchange(p,q,r,0,5,2);}",
        "int f(int *p){return __atomic_load_n(p,2);}",
        "typedef unsigned int I __attribute__((mode(HI))); I f(I *p){return __atomic_exchange_n(p,1,4);}",
        "void f(int *p){__atomic_clear(p,3); __atomic_test_and_set(p,2);}",
    ] {
        for target in Target::NATIVE {
            clang_syntax(source, target, true);
            let m = keel_c::compile(
                source,
                "atomic-review.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            keel_ir::verify_module(&m).unwrap();
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

#[test]
fn under_aligned_pointer_aliases_fail_closed() {
    for source in [
        "typedef int I __attribute__((aligned(1))); void f(I *p,int *q){__atomic_load(p,q,5);}",
        "typedef int I __attribute__((aligned(1))); int f(I *p){return __atomic_load_n(p,5);}",
        "typedef _Atomic float F __attribute__((aligned(1))); float f(F *p){return __c11_atomic_load(p,5);}",
        "typedef _Atomic float F __attribute__((aligned(1))); float f(F *p){return *p;}",
        "typedef _Atomic float F __attribute__((aligned(1))); void f(F *p){*p=1.0f;}",
        "typedef _Atomic float F __attribute__((aligned(1))); void f(F *p){*p+=1.0f;}",
        "typedef _Atomic float F __attribute__((aligned(1))); void f(F *p){(*p)++;}",
    ] {
        for target in Target::NATIVE {
            let error = keel_c::compile(
                source,
                "atomic-review.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("aligned"), "{source}: {error}");
        }
    }
}

#[test]
fn generic_buffers_preserve_volatile_members() {
    use keel_ir::InstData;
    let source = "struct S { volatile int x; };\n\
        void load(struct S *p, struct S *out){__atomic_load(p,out,5);}\n\
        void store(struct S *p, struct S *in){__atomic_store(p,in,5);}\n\
        void exchange(struct S *p, struct S *in, struct S *out){__atomic_exchange(p,in,out,5);}\n\
        int cas(struct S *p, struct S *e, struct S *d){return __atomic_compare_exchange(p,e,d,0,5,2);}";
    for target in Target::NATIVE {
        clang_syntax(source, target, true);
        let mut m = keel_c::compile(
            source,
            "atomic-volatile-members.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for f in &m.functions {
                let mut loads = 0;
                let mut stores = 0;
                for (_, i) in f.layout_insts() {
                    match f.inst_data(i) {
                        InstData::Load { flags, .. } => {
                            assert!(flags.volatile, "{}", f.name);
                            loads += 1;
                        }
                        InstData::Store { flags, .. } => {
                            assert!(flags.volatile, "{}", f.name);
                            stores += 1;
                        }
                        _ => {}
                    }
                }
                assert_eq!(
                    (loads, stores),
                    match f.name.as_str() {
                        "load" => (0, 1),
                        "store" => (1, 0),
                        "exchange" => (1, 1),
                        "cas" => (2, 1),
                        _ => unreachable!(),
                    }
                );
            }
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}
