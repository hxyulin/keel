use keel_codegen::{CodegenPipeline, SelectionTier, Target, parallel, regalloc::AllocationTier};
use std::num::NonZeroUsize;

struct Threads;
impl Threads {
    fn execute(&self, jobs: Vec<parallel::Job<'_>>) {
        std::thread::scope(|scope| {
            // Reverse scheduling deliberately differs from consumer order.
            for job in jobs.into_iter().rev() {
                scope.spawn(move || job.run());
            }
        });
    }
}

#[test]
fn ordered_chunks_match_serial_for_both_backends_and_tiers() {
    let module = keel_ir::parse(include_str!(
        "../../../suite/ir/llvm_import_fast_fixtures/fast.keel"
    ))
    .unwrap();
    for target in [
        Target::X86_64LinuxGnu,
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
    ] {
        for selector in [SelectionTier::Covers, SelectionTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let mut reference = Vec::new();
            keel_codegen::Request::new(target, pipeline)
                .program_chunks(&module, &mut |p| reference.push(format!("{p:?}")))
                .unwrap();
            for budget in [1, 8] {
                let mut actual = Vec::new();
                parallel::program_chunks(
                    &module,
                    parallel::Options {
                        target,
                        pipeline: parallel::Pipeline {
                            selector: &selector,
                            allocator: &AllocationTier::Full,
                        },
                        emission: Default::default(),
                        pools: None,
                        budget: NonZeroUsize::new(budget).unwrap(),
                    },
                    &|jobs| Threads.execute(jobs),
                    &mut |p| actual.push(format!("{p:?}")),
                )
                .unwrap();
                assert_eq!(actual, reference, "target {target:?}, budget {budget}");
            }
        }
    }
}

fn compare(source: &str, target: Target) {
    let module = keel_ir::parse(source).unwrap();
    compare_module(&module, target);
}

fn compare_module(module: &keel_ir::Module, target: Target) {
    compare_policy(module, target, Default::default(), AllocationTier::Full);
}

fn compare_policy(
    module: &keel_ir::Module,
    target: Target,
    emission: keel_codegen::EmissionOptions,
    allocator: AllocationTier,
) {
    let selector = SelectionTier::Full;
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocator,
    };
    let mut reference = Vec::new();
    let mut reference_program = keel_asm_ir::Program::new();
    let result = keel_codegen::Request::new(target, pipeline)
        .emission(emission)
        .program_chunks(module, &mut |p| {
            reference.push(format!("{p:?}"));
            reference_program.append(p);
        });
    for budget in [1, 8] {
        let mut actual = Vec::new();
        let mut program = keel_asm_ir::Program::new();
        let got = parallel::program_chunks(
            module,
            parallel::Options {
                target,
                pipeline: parallel::Pipeline {
                    selector: &selector,
                    allocator: &allocator,
                },
                emission,
                pools: None,
                budget: NonZeroUsize::new(budget).unwrap(),
            },
            &|jobs| Threads.execute(jobs),
            &mut |p| {
                actual.push(format!("{p:?}"));
                program.append(p);
            },
        );
        assert_eq!(got, result, "error order on {target:?}");
        assert_eq!(actual, reference, "callback order on {target:?}");
        if got.is_ok() {
            assert_eq!(
                keel_asm::assemble_program_for(target, &program),
                keel_asm::assemble_program_for(target, &reference_program),
                "object output on {target:?}"
            );
        }
    }
}

#[test]
fn snapshot_corpus_preserves_chunks_errors_and_objects() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
        .collect();
    files.sort();
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
            compare(&source, target);
        }
    }
}

#[test]
fn profile_capture_uses_serial_executor_fallback() {
    struct MustNotRun;
    impl MustNotRun {
        fn execute(&self, _: Vec<parallel::Job<'_>>) {
            panic!("profile capture must remain serial");
        }
    }
    let module = keel_ir::parse(include_str!(
        "../../../suite/ir/llvm_import_fast_fixtures/fast.keel"
    ))
    .unwrap();
    let (result, report) = keel_codegen::backend_profile::capture(|| {
        parallel::program_chunks(
            &module,
            parallel::Options {
                target: Target::X86_64LinuxGnu,
                pipeline: parallel::Pipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                },
                emission: Default::default(),
                pools: None,
                budget: NonZeroUsize::new(8).unwrap(),
            },
            &|jobs| MustNotRun.execute(jobs),
            &mut |_| {},
        )
    });
    result.unwrap();
    assert!(!report.functions.is_empty());
}

#[test]
fn earlier_source_error_wins_over_later_native_error() {
    compare(
        r#"
func @first() {
entry:
 asm "vpxord %zmm0,%zmm0,%zmm0", "~{xmm0}"()
 return
}
func @later() {
entry:
 asm "", "~{zmm31}"()
 return
}
"#,
        Target::X86_64LinuxGnu,
    );
}

#[test]
fn debug_lifecycle_weak_and_odr_remain_coordinator_owned() {
    let source = include_str!("../../../suite/ir/llvm_import_fast_fixtures/fast.keel");
    let mut module = keel_ir::parse(source).unwrap();
    module.functions[0].attrs.constructor = true;
    module.functions[0].local = false;
    module.functions[0].weak_odr = true;
    for debug in [false, true] {
        if debug {
            module.files.push("parallel.c".into());
        }
        for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
            compare_module(&module, target);
            let mut emission = keel_codegen::EmissionOptions::default();
            emission.function_sections = true;
            compare_policy(&module, target, emission, AllocationTier::Full);
        }
    }
}

#[test]
fn caller_can_reuse_safe_scoped_workers_across_windows() {
    use std::sync::{Arc, Mutex, mpsc};
    type Job<'job> = parallel::Job<'job>;
    struct Pool<'job>(mpsc::Sender<Job<'job>>);
    impl<'job> Pool<'job> {
        fn execute(&self, jobs: Vec<parallel::Job<'job>>) {
            let count = jobs.len();
            let (done, completed) = mpsc::channel();
            for job in jobs {
                let done = done.clone();
                self.0
                    .send(parallel::Job::new(move || {
                        job.run();
                        done.send(()).unwrap();
                    }))
                    .unwrap();
            }
            for _ in 0..count {
                completed.recv().unwrap();
            }
        }
    }
    let module = keel_ir::parse(include_str!(
        "../../../suite/ir/llvm_import_fast_fixtures/fast.keel"
    ))
    .unwrap();
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<Job<'_>>();
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..2 {
            let receiver = Arc::clone(&receiver);
            scope.spawn(move || {
                loop {
                    let job = receiver.lock().unwrap().recv();
                    match job {
                        Ok(job) => job.run(),
                        Err(_) => break,
                    }
                }
            });
        }
        let pool = Pool(sender);
        let mut chunks = 0;
        parallel::program_chunks(
            &module,
            parallel::Options {
                target: Target::Aarch64AppleDarwin,
                pipeline: parallel::Pipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                },
                emission: Default::default(),
                pools: None,
                budget: NonZeroUsize::new(2).unwrap(),
            },
            &|jobs| pool.execute(jobs),
            &mut |_| chunks += 1,
        )
        .unwrap();
        assert!(
            chunks > 2,
            "multiple windows must reuse the same two workers"
        );
        drop(pool);
    });
}

#[test]
fn main_pressure_scheduler_and_adaptive_allocator_preserve_parallel_output() {
    let module = keel_ir::parse(include_str!("programs/compact_frame.keel")).unwrap();
    let mut emission = keel_codegen::EmissionOptions::default();
    emission.pressure_schedule = true;
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        compare_policy(&module, target, emission, AllocationTier::Full);
        #[cfg(feature = "experimental-regalloc")]
        compare_policy(&module, target, emission, AllocationTier::Adaptive);
    }
}

#[test]
fn empty_module_keeps_final_callback() {
    compare("", Target::X86_64LinuxGnu);
    compare("", Target::Aarch64AppleDarwin);
}

#[test]
fn unused_signature_is_validated_before_dispatch_or_output() {
    let mut module = keel_ir::parse("func @main() {\nentry:\n return\n}\n").unwrap();
    let declaration = keel_ir::parse("decl @unused(i32x16)\n").unwrap();
    module.functions[0].import_signature(declaration.decls[0].sig.clone());
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        let error = parallel::program_chunks(
            &module,
            parallel::Options {
                target,
                pipeline: parallel::Pipeline {
                    selector: &SelectionTier::Full,
                    allocator: &AllocationTier::Full,
                },
                emission: Default::default(),
                pools: None,
                budget: NonZeroUsize::new(8).unwrap(),
            },
            &|_| panic!("invalid unused signatures must fail before dispatch"),
            &mut |_| panic!("invalid unused signatures must fail before output"),
        )
        .unwrap_err();
        assert!(error.message.contains("SIMD"), "{error}");
    }
}

#[test]
fn executor_windows_are_bounded_and_sinks_stay_on_the_caller() {
    use std::{
        cell::RefCell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    let source: String = (0..17)
        .map(|i| format!("func @f{i}() {{\nentry:\n return\n}}\n"))
        .collect();
    let module = keel_ir::parse(&source).unwrap();
    let caller = std::thread::current().id();
    let windows = RefCell::new(Vec::new());
    let completed = AtomicUsize::new(0);
    let mut chunks = 0;
    parallel::program_chunks(
        &module,
        parallel::Options {
            target: Target::X86_64LinuxGnu,
            pipeline: parallel::Pipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
            emission: Default::default(),
            pools: None,
            budget: NonZeroUsize::new(3).unwrap(),
        },
        &|jobs| {
            assert_eq!(std::thread::current().id(), caller);
            windows.borrow_mut().push(jobs.len());
            assert!(!jobs.is_empty() && jobs.len() <= 3);
            std::thread::scope(|scope| {
                for job in jobs.into_iter().rev() {
                    let completed = &completed;
                    scope.spawn(move || {
                        job.run();
                        completed.fetch_add(1, Ordering::SeqCst);
                    });
                }
            });
        },
        &mut |_| {
            assert_eq!(std::thread::current().id(), caller);
            assert_eq!(
                completed.load(Ordering::SeqCst),
                windows.borrow().iter().sum::<usize>(),
                "the executor must finish the whole window before a sink runs"
            );
            chunks += 1;
        },
    )
    .unwrap();
    assert_eq!(*windows.borrow(), [3, 3, 3, 3, 3, 2]);
    assert_eq!(
        chunks, 18,
        "one chunk per function and final module metadata"
    );
}

#[test]
fn omitted_jobs_and_executor_panics_do_not_emit_partial_windows() {
    let module = keel_ir::parse("func @f() {\nentry:\n return\n}\n").unwrap();
    for omit in [false, true] {
        let mut chunks = 0;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parallel::program_chunks(
                &module,
                parallel::Options {
                    target: Target::X86_64LinuxGnu,
                    pipeline: parallel::Pipeline {
                        selector: &SelectionTier::Full,
                        allocator: &AllocationTier::Full,
                    },
                    emission: Default::default(),
                    pools: None,
                    budget: NonZeroUsize::new(3).unwrap(),
                },
                &|jobs| {
                    if omit {
                        drop(jobs);
                    } else {
                        Threads.execute(jobs);
                        panic!("executor failure after joining workers");
                    }
                },
                &mut |_| chunks += 1,
            )
        }));
        assert!(panic.is_err(), "executor contract failure must propagate");
        assert_eq!(chunks, 0, "a failed first window must not reach the sink");
    }
}
