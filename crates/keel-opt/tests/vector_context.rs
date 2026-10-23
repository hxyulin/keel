use keel_ir::{Function, VectorCapabilities, parse};
use keel_opt::{OptimizationContext, OptimizationReport, OptimizeOptions, Pass, VectorTarget};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Custom<'a> {
    canonical: bool,
    calls: &'a AtomicUsize,
}
impl Pass for Custom<'_> {
    fn name(&self) -> &'static str {
        "custom"
    }
    fn description(&self) -> &'static str {
        "dispatch test"
    }
    fn run(&self, _: &mut Function) -> bool {
        panic!("target override lost")
    }
    fn run_for_target(&self, _: &mut Function, target: VectorTarget) -> bool {
        assert!(!self.canonical);
        assert_eq!(target, VectorTarget::X86Sse2);
        self.calls.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn run_canonical(&self, _: &mut Function, target: VectorTarget) -> bool {
        assert!(self.canonical);
        assert_eq!(target, VectorTarget::X86Sse2);
        self.calls.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn requires_canonical_form(&self) -> bool {
        self.canonical
    }
}
#[test]
fn context_preserves_custom_dispatch_and_observation() {
    let context = OptimizationContext {
        target: VectorTarget::X86Sse2,
        capabilities: VectorCapabilities::new(true, true, true, 123),
    };
    for canonical in [false, true] {
        let calls = AtomicUsize::new(0);
        let pass = Custom {
            canonical,
            calls: &calls,
        };
        let mut ordinary = parse("func @f() {\nblock0:\n return\n}").unwrap();
        let mut observed = ordinary.clone();
        let mut legacy = ordinary.clone();
        let options = OptimizeOptions::default();
        keel_opt::run_pipeline_with_context_and_options(&mut ordinary, &[&pass], context, options)
            .unwrap();
        let mut report = OptimizationReport::default();
        keel_opt::run_pipeline_with_context_and_report_and_options(
            &mut observed,
            &[&pass],
            context,
            &mut report,
            options,
        )
        .unwrap();
        keel_opt::run_pipeline_for_target(&mut legacy, &[&pass], context.target).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert_eq!(ordinary.to_string(), observed.to_string());
        assert_eq!(ordinary.to_string(), legacy.to_string());
        assert_eq!(report.invocations[0].context, context);
        let json = report.to_json();
        assert!(json.contains("\"signed_i32_sum_bits\":256"), "{json}");
        assert!(json.contains("\"effective_x86_features\":123"));
    }
}

const SUM: &str = include_str!("fixtures/signed_widen_sum.ir");
fn optimized(source: &str, context: OptimizationContext, names: &[&str]) -> String {
    let mut module = parse(source).unwrap();
    let mut observed = module.clone();
    keel_opt::run_passes_with_context_and_options(
        &mut module,
        names,
        context,
        OptimizeOptions::default(),
    )
    .unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_with_context_and_report_and_options(
        &mut observed,
        names,
        context,
        &mut report,
        OptimizeOptions::default(),
    )
    .unwrap();
    assert_eq!(module.to_string(), observed.to_string());
    keel_ir::verify_module(&module).unwrap();
    module.to_string()
}
#[test]
fn only_plain_signed_sum_prefers_wide_complete_recipe() {
    let wide = OptimizationContext {
        target: VectorTarget::X86Sse2,
        capabilities: VectorCapabilities::new(true, true, true, 0),
    };
    let text = optimized(SUM, wide, &["loop-reduce"]);
    assert!(text.contains("swiden_low.i64x4"), "{text}");
    assert_eq!(text.matches("load.i32x4").count(), 4, "{text}");
    assert!(!text.contains("load.i32x8"));
    let module = parse(&text).unwrap();
    let function = &module.functions[0];
    let body = function
        .blocks()
        .find(|&b| {
            function
                .block_insts(b)
                .iter()
                .filter(|&&i| {
                    matches!(
                        function.inst_data(i),
                        keel_ir::InstData::Load {
                            ty: keel_ir::Type::Vector(keel_ir::VectorType::I32x4),
                            ..
                        }
                    )
                })
                .count()
                == 4
        })
        .unwrap();
    assert_eq!(
        function
            .block_params(body)
            .iter()
            .filter(
                |&&v| function.value_type(v) == keel_ir::Type::Vector(keel_ir::VectorType::I64x4)
            )
            .count(),
        4
    );
    for offset in [16, 32, 48, 64] {
        assert!(function.block_insts(body).iter().any(|&i| matches!(function.inst_data(i), keel_ir::InstData::Iconst {imm,..} if *imm == offset)));
    }

    for caps in [
        VectorCapabilities::BASELINE,
        VectorCapabilities::new(true, false, true, 0),
        VectorCapabilities::new(true, true, false, 0),
    ] {
        assert!(
            !optimized(
                SUM,
                OptimizationContext {
                    capabilities: caps,
                    ..wide
                },
                &["loop-reduce"]
            )
            .contains("i64x4")
        );
    }
    for target in [VectorTarget::Portable, VectorTarget::Aarch64Neon] {
        assert!(
            !optimized(
                SUM,
                OptimizationContext { target, ..wide },
                &["loop-reduce"]
            )
            .contains("i64x4")
        );
    }
    for source in [
        SUM.replace("sext.i64", "uext.i64"),
        SUM.replace("load.i32 align 1", "load.i32 volatile align 1"),
        SUM.replace(
            " v11 = sext.i64 v10",
            " v20 = iconst.i32 1\n v21 = iadd v10,v20\n v11 = sext.i64 v21",
        ),
        SUM.replace(
            " v11 = sext.i64 v10",
            " v20 = load.i32 v0\n v11 = sext.i64 v10",
        ),
    ] {
        assert!(!optimized(&source, wide, &["loop-reduce"]).contains("i64x4"));
    }
    assert!(!optimized(SUM, wide, keel_opt::OptimizationTier::Fast.pipeline()).contains("i64x4"));
    assert!(optimized(SUM, wide, keel_opt::DEFAULT_PIPELINE).contains("i64x4"));
}

#[test]
fn context_does_not_widen_predicates_fp_or_multiple_streams() {
    let wide = OptimizationContext {
        target: VectorTarget::X86Sse2,
        capabilities: VectorCapabilities::new(true, true, true, 0),
    };
    let conditional = include_str!("conditional_reduce/loops.keel");
    assert!(!optimized(conditional, wide, keel_opt::DEFAULT_PIPELINE).contains("i64x4"));
    let floating = SUM
        .replace("load.i32 align 1", "load.f32 align 1")
        .replace("sext.i64", "fpext.f64")
        .replace("i64) -> i64", "f64) -> f64")
        .replace("v3: i64", "v3: f64")
        .replace("v5: i64", "v5: f64")
        .replace("iadd v5,v11", "fadd v5,v11");
    assert!(!optimized(&floating, wide, keel_opt::DEFAULT_PIPELINE).contains("f64x4"));
    let multiple = SUM.replace(
        " v11 = sext.i64 v10",
        " v20 = load.i32 v0\n v21 = iadd v10,v20\n v11 = sext.i64 v21",
    );
    assert!(!optimized(&multiple, wide, keel_opt::DEFAULT_PIPELINE).contains("i64x4"));
}
