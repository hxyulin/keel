use keel_codegen::{CodegenPipeline, SelectionTier, Target, parallel, regalloc::AllocationTier};
use std::num::NonZeroUsize;

fn options<'a>(
    target: Target,
    selector: &'a (dyn keel_codegen::InstructionSelector + Sync),
    allocator: &'a AllocationTier,
    emission: keel_codegen::EmissionOptions,
    workers: usize,
    budget: usize,
) -> parallel::Options<'a> {
    parallel::Options {
        target,
        pipeline: parallel::Pipeline {
            selector,
            allocator,
        },
        emission,
        pools: None,
        workers: NonZeroUsize::new(workers).unwrap(),
        budget: NonZeroUsize::new(budget).unwrap(),
    }
}

/// [`parallel::program_chunks`] with each job on its own scoped thread.
fn threaded(
    module: &keel_ir::Module,
    options: parallel::Options<'_>,
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), keel_codegen::CodegenError> {
    std::thread::scope(|scope| {
        parallel::program_chunks(
            module,
            options,
            &|job| {
                scope.spawn(move || job.run());
            },
            sink,
        )
    })
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
            for (workers, budget) in [(1, 1), (3, 1), (3, 8)] {
                let mut actual = Vec::new();
                threaded(
                    &module,
                    options(
                        target,
                        &selector,
                        &AllocationTier::Full,
                        Default::default(),
                        workers,
                        budget,
                    ),
                    &mut |p| actual.push(format!("{p:?}")),
                )
                .unwrap();
                assert_eq!(
                    actual, reference,
                    "target {target:?}, {workers} workers, budget {budget}"
                );
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
    for (workers, budget) in [(1, 1), (4, 2), (4, 8)] {
        let mut actual = Vec::new();
        let mut program = keel_asm_ir::Program::new();
        let got = threaded(
            module,
            options(target, &selector, &allocator, emission, workers, budget),
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
    if target == Target::X86_64LinuxGnu {
        let streamed = keel_codegen::Request::new(target, pipeline)
            .emission(emission)
            .assemble_x86(
                module,
                keel_asm::x86::ProgramAssembler::new(Default::default()),
                Box::new(|name: &str| {
                    Err(format!(
                        ".incbin `{name}` requires an explicit binary-input resolver"
                    ))
                }),
            );
        match (&result, streamed) {
            (Ok(()), Ok(assembler)) => assert_eq!(
                assembler.finish(),
                keel_asm::assemble_program_for(target, &reference_program),
                "streamed object output"
            ),
            (Err(error), Err(streamed)) => assert_eq!(&streamed, error, "streamed error"),
            (result, streamed) => panic!("streamed {:?}, chunked {result:?}", streamed.err()),
        }
    }
    let mut reference_text = Vec::new();
    let text_result = keel_codegen::Request::new(target, pipeline)
        .emission(emission)
        .text_chunks(module, &mut |t| reference_text.push(t.to_owned()));
    for (workers, budget) in [(1, 1), (4, 2), (4, 8)] {
        let mut actual = Vec::new();
        let got = std::thread::scope(|scope| {
            parallel::text_chunks(
                module,
                options(target, &selector, &allocator, emission, workers, budget),
                &|job| {
                    scope.spawn(move || job.run());
                },
                &mut |t| actual.push(t.to_owned()),
            )
        });
        assert_eq!(got, text_result, "text error order on {target:?}");
        assert_eq!(actual, reference_text, "text pieces on {target:?}");
    }
}

#[test]
fn snapshot_corpus_preserves_chunks_errors_objects_and_text() {
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
fn profile_capture_uses_serial_route() {
    let module = keel_ir::parse(include_str!(
        "../../../suite/ir/llvm_import_fast_fixtures/fast.keel"
    ))
    .unwrap();
    let (result, report) = keel_codegen::backend_profile::capture(|| {
        parallel::program_chunks(
            &module,
            options(
                Target::X86_64LinuxGnu,
                &SelectionTier::Full,
                &AllocationTier::Full,
                Default::default(),
                4,
                8,
            ),
            &|_| panic!("profile capture must remain serial"),
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
fn caller_owned_pool_can_run_the_workers() {
    use std::sync::{Arc, Mutex, mpsc};
    let module = keel_ir::parse(include_str!(
        "../../../suite/ir/llvm_import_fast_fixtures/fast.keel"
    ))
    .unwrap();
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<parallel::Job<'_>>();
        let receiver = Arc::new(Mutex::new(receiver));
        // Fewer pool threads than workers: a queued worker starts late and
        // finds nothing left to compile.
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
        let mut chunks = 0;
        parallel::program_chunks(
            &module,
            options(
                Target::Aarch64AppleDarwin,
                &SelectionTier::Full,
                &AllocationTier::Full,
                Default::default(),
                3,
                2,
            ),
            &|job| sender.send(job).unwrap(),
            &mut |_| chunks += 1,
        )
        .unwrap();
        assert!(chunks > 2);
        drop(sender);
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
            options(
                target,
                &SelectionTier::Full,
                &AllocationTier::Full,
                Default::default(),
                4,
                8,
            ),
            &|_| panic!("invalid unused signatures must fail before dispatch"),
            &mut |_| panic!("invalid unused signatures must fail before output"),
        )
        .unwrap_err();
        assert!(error.message.contains("SIMD"), "{error}");
    }
}

/// Records which functions selection has started, and panics on one name.
struct Watch {
    started: std::sync::Mutex<std::collections::HashSet<String>>,
    panic_on: Option<&'static str>,
}
impl Watch {
    fn new(panic_on: Option<&'static str>) -> Self {
        Self {
            started: Default::default(),
            panic_on,
        }
    }
    fn start(&self, function: &keel_ir::Function) {
        assert_ne!(
            Some(function.name.as_str()),
            self.panic_on,
            "selector panic"
        );
        self.started.lock().unwrap().insert(function.name.clone());
    }
    fn count(&self) -> usize {
        self.started.lock().unwrap().len()
    }
}
impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Watch")
    }
}
impl keel_codegen::InstructionSelector for Watch {
    fn name(&self) -> &'static str {
        "watch"
    }
    fn select(
        &self,
        function: &keel_ir::Function,
        target: keel_codegen::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        self.start(function);
        keel_codegen::InstructionSelector::select(&SelectionTier::Full, function, target, folds)
    }
    fn select_with_uses(
        &self,
        function: &keel_ir::Function,
        target: keel_codegen::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> keel_codegen::select::Selection {
        self.start(function);
        keel_codegen::InstructionSelector::select_with_uses(
            &SelectionTier::Full,
            function,
            target,
            folds,
            uses,
        )
    }
}

#[test]
fn workers_stay_within_the_budget_and_sinks_stay_on_the_caller() {
    let source: String = (0..17)
        .map(|i| format!("func @f{i}() {{\nentry:\n return\n}}\n"))
        .collect();
    let module = keel_ir::parse(&source).unwrap();
    let caller = std::thread::current().id();
    let watch = Watch::new(None);
    let spawned = std::cell::Cell::new(0);
    let mut chunks = 0;
    std::thread::scope(|scope| {
        parallel::program_chunks(
            &module,
            options(
                Target::X86_64LinuxGnu,
                &watch,
                &AllocationTier::Full,
                Default::default(),
                4,
                3,
            ),
            &|job| {
                assert_eq!(std::thread::current().id(), caller);
                spawned.set(spawned.get() + 1);
                scope.spawn(move || job.run());
            },
            &mut |_| {
                assert_eq!(std::thread::current().id(), caller);
                chunks += 1;
                // The sink's own function and up to 3 more have started.
                assert!(
                    watch.count() <= (chunks + 3).min(17),
                    "{} started after {chunks} chunks",
                    watch.count()
                );
            },
        )
        .unwrap();
    });
    assert_eq!(spawned.get(), 4);
    assert_eq!(watch.count(), 17);
    assert_eq!(
        chunks, 18,
        "one chunk per function and final module metadata"
    );
}

#[test]
fn dropped_jobs_and_worker_panics_propagate() {
    let module = keel_ir::parse("func @f() {\nentry:\n return\n}\n").unwrap();
    for drop_jobs in [false, true] {
        let watch = Watch::new(Some("f"));
        let mut chunks = 0;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::thread::scope(|scope| {
                parallel::program_chunks(
                    &module,
                    options(
                        Target::X86_64LinuxGnu,
                        &watch,
                        &AllocationTier::Full,
                        Default::default(),
                        2,
                        3,
                    ),
                    &|job| {
                        if !drop_jobs {
                            scope.spawn(move || job.run());
                        }
                    },
                    &mut |_| chunks += 1,
                )
            })
        }))
        .unwrap_err();
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        let expected = if drop_jobs {
            "every codegen job must run"
        } else {
            "selector panic"
        };
        assert!(message.contains(expected), "{message}");
        assert_eq!(chunks, 0, "nothing reaches the sink before the failure");
    }
}
