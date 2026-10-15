//! AArch64 backend profiles on preprocessed C or textual IR. Frontend work and
//! optimization are outside timing. Both output paths must yield identical objects.
//! Usage: profile_backend INPUT.i|INPUT.keel ROUNDS [--opt] [--debug] [--selection-all] [--regalloc-fast]
use keel_codegen::{
    Target,
    backend_profile::{self, Stage, Timing},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, hint::black_box, time::Instant};

fn timings(stages: &[Timing]) -> Value {
    Stage::ALL
        .iter()
        .map(|stage| {
            let timing = stages[*stage as usize];
            (
                stage.name().to_owned(),
                json!({"self_ms": timing.self_time.as_secs_f64()*1000.,
            "inclusive_ms": timing.inclusive.as_secs_f64()*1000., "calls": timing.calls}),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}
#[derive(Default)]
struct AllocationMetrics {
    spill_slots: u64,
    spilled_value_halves: u64,
    callee_saves: u64,
}
struct CountingAllocator {
    tier: keel_codegen::regalloc::AllocationTier,
    metrics: RefCell<AllocationMetrics>,
}
impl keel_codegen::regalloc::RegisterAllocator for CountingAllocator {
    fn name(&self) -> &'static str {
        "metrics"
    }
    fn allocate(
        &self,
        func: &keel_ir::Function,
        pools: &keel_codegen::Pools,
        is_call: &dyn Fn(&keel_ir::InstData) -> bool,
        returns_twice: &dyn Fn(&keel_ir::InstData) -> bool,
        selection: &keel_codegen::select::Selection,
    ) -> keel_codegen::regalloc::Allocation {
        let allocation = self
            .tier
            .allocate(func, pools, is_call, returns_twice, selection);
        let mut metrics = self.metrics.borrow_mut();
        metrics.spill_slots += u64::from(allocation.spill_slots);
        metrics.spilled_value_halves += allocation
            .locs
            .iter()
            .flatten()
            .filter(|loc| matches!(loc, Some(keel_codegen::regalloc::Loc::Stack(_))))
            .count() as u64;
        metrics.callee_saves +=
            (allocation.callee_saved_used.len() + allocation.float_callee_saved_used.len()) as u64;
        allocation
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or("expected INPUT.i|INPUT.keel ROUNDS")?;
    let rounds: usize = args.get(2).ok_or("expected ROUNDS")?.parse()?;
    if rounds < 3 {
        return Err("use at least 3 rounds".into());
    }
    for flag in args.iter().skip(3) {
        if !matches!(
            flag.as_str(),
            "--opt" | "--debug" | "--selection-all" | "--regalloc-fast"
        ) {
            return Err(format!("unknown flag: {flag}").into());
        }
    }
    let opt = args.iter().any(|arg| arg == "--opt");
    let debug = args.iter().any(|arg| arg == "--debug");
    let selection_all = args.iter().any(|arg| arg == "--selection-all");
    let allocation = if args.iter().any(|arg| arg == "--regalloc-fast") {
        keel_codegen::regalloc::AllocationTier::Fast
    } else {
        keel_codegen::regalloc::AllocationTier::Full
    };
    let source = std::fs::read_to_string(path)?;
    let mut module = if path.ends_with(".keel") {
        if debug {
            keel_ir::parse_with_ir_lines(&source, path)
        } else {
            keel_ir::parse(&source)
        }
        .map_err(|e| e.to_string())?
    } else {
        keel_c::compile(
            &source,
            path,
            keel_c::Options {
                debug_info: debug,
                target: Target::Aarch64AppleDarwin,
            },
        )?
    };
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    if opt {
        keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
    }
    let selector = keel_codegen::OptionsSelector(if selection_all {
        keel_codegen::SelectionOptions::ALL
    } else {
        Default::default()
    });
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    let target = Target::Aarch64AppleDarwin;
    let features = keel_asm::TargetFeatures::default();
    let text = keel_codegen::compile_with_pipeline(&module, target, &pipeline)?;
    let program = keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)?;
    let (captured_text, _) = backend_profile::capture(|| {
        keel_codegen::compile_with_pipeline(&module, target, &pipeline)
    });
    assert_eq!(captured_text?, text, "profiling changed text output");
    let (captured_program, captured_profile) = backend_profile::capture(|| {
        keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
    });
    assert_eq!(
        captured_program?, program,
        "profiling changed structured output"
    );
    let allocator_metrics = CountingAllocator {
        tier: allocation,
        metrics: RefCell::new(AllocationMetrics::default()),
    };
    let counted = keel_codegen::compile_program_with_pipeline(
        &module,
        target,
        &keel_codegen::CodegenPipeline {
            selector: &selector,
            allocator: &allocator_metrics,
        },
    )?;
    assert_eq!(counted, program, "metric collection changed output");
    let allocation_metrics = allocator_metrics.metrics.into_inner();
    let spill_counters: serde_json::Map<String, Value> = backend_profile::Counter::ALL
        .iter()
        .map(|counter| {
            (
                counter.name().to_owned(),
                json!(captured_profile.counters[*counter as usize]),
            )
        })
        .collect();
    let assembly = keel_asm::assemble_program(&program, features)?;
    let expected = assembly.to_macho()?;
    assert_eq!(
        keel_asm::assemble_object(&text)?,
        expected,
        "text and structured objects differ"
    );
    let mut runs = Vec::new();
    let mut functions: BTreeMap<(String, String), [Timing; Stage::ALL.len()]> = BTreeMap::new();
    for round in 0..rounds + 2 {
        for structured in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut profile = None;
            let mut plain_ms = 0.;
            let mut profiled_ms = 0.;
            for capture in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let compile = || -> Result<(), keel_codegen::CodegenError> {
                    // Timed outer wall time includes result destruction; the
                    // codegen root scope ends before the returned result drops.
                    if structured {
                        black_box(keel_codegen::compile_program_with_pipeline(
                            black_box(&module),
                            target,
                            &pipeline,
                        )?);
                    } else {
                        black_box(keel_codegen::compile_with_pipeline(
                            black_box(&module),
                            target,
                            &pipeline,
                        )?);
                    }
                    Ok(())
                };
                let start = Instant::now();
                if capture {
                    let (result, report) = backend_profile::capture(compile);
                    profiled_ms = start.elapsed().as_secs_f64() * 1000.;
                    result?;
                    profile = Some(report);
                } else {
                    compile()?;
                    plain_ms = start.elapsed().as_secs_f64() * 1000.;
                }
            }
            let start = Instant::now();
            if structured {
                black_box(keel_asm::assemble_program(&program, features)?);
            } else {
                black_box(keel_asm::assemble(&text)?);
            }
            let assemble_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            black_box(assembly.to_macho()?);
            let macho_ms = start.elapsed().as_secs_f64() * 1000.;
            if round < 2 {
                continue;
            }
            let profile = profile.unwrap();
            let mode = if structured { "structured" } else { "text" };
            for function in profile.functions {
                let totals = functions
                    .entry((mode.to_owned(), function.name))
                    .or_insert([Timing::default(); Stage::ALL.len()]);
                for (total, timing) in totals.iter_mut().zip(function.stages) {
                    total.inclusive += timing.inclusive;
                    total.self_time += timing.self_time;
                    total.calls += timing.calls;
                }
            }
            runs.push(
                json!({"mode":mode,"plain_codegen_ms":plain_ms,"profiled_codegen_ms":profiled_ms,
                "stages":timings(&profile.stages),"assemble_ms":assemble_ms,"macho_ms":macho_ms}),
            );
        }
    }
    let mut hot_functions = serde_json::Map::new();
    for mode in ["text", "structured"] {
        let mut ranked: Vec<_> = functions.iter().filter(|((m, _), _)| m == mode).collect();
        ranked.sort_by_key(|(_, stages)| {
            std::cmp::Reverse(stages[Stage::Function as usize].inclusive)
        });
        hot_functions.insert(mode.into(), ranked.into_iter().take(10).map(|((_,name),stages)| {
            json!({"name":name,"stages_total_over_rounds":timings(stages)})
        }).collect::<Vec<_>>().into());
    }
    println!(
        "{}",
        json!({"input":path,"rounds":rounds,"optimized":opt,"debug":debug,"selection_all":selection_all,"regalloc_fast":allocation == keel_codegen::regalloc::AllocationTier::Fast,
        "functions":module.functions.len(),"ir_instructions":module.functions.iter().map(|f|f.layout_insts().count()).sum::<usize>(),
        "object_bytes":expected.len(),"code_bytes":assembly.text.len(),"native_instructions":program.stats().instructions,
        "allocation_metrics":{"spill_slots":allocation_metrics.spill_slots,"spilled_value_halves":allocation_metrics.spilled_value_halves,"callee_saves":allocation_metrics.callee_saves},"explicit_spill_helper_calls":spill_counters,"objects_identical":true,"runs":runs,"function_profiles":hot_functions})
    );
    Ok(())
}
