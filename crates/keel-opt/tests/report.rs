use keel_ir::{Function, InstData, Module};
use keel_opt::{OptimizationReport, Pass, VectorTarget};

fn observed(source: &str, passes: &[&str]) -> (Module, OptimizationReport) {
    let mut plain = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&plain).unwrap();
    let mut instrumented = plain.clone();
    keel_opt::run_passes_for_target(&mut plain, passes, VectorTarget::Portable).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut instrumented,
        passes,
        VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    assert_eq!(plain.to_string(), instrumented.to_string());
    keel_ir::verify_module(&instrumented).unwrap();
    (instrumented, report)
}
fn caller(helper: &str) -> String {
    format!("{helper}\nfunc @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = call @h(v0)\n return v1\n}}")
}
#[test]
fn direct_sites_record_actual_outcomes_and_categories() {
    for attrs in ["", "alwaysinline"] {
        for (helper, expected) in [
            (
                format!("func @h(i64) -> i64 {attrs} local {{\nblock0(v0: i64):\n return v0\n}}"),
                "inlined",
            ),
            (
                format!("func @h(i64) -> i64 {attrs} {{\nblock0(v0: i64):\n return v0\n}}"),
                "public_definition",
            ),
            (
                format!(
                    "func @h(i64) -> i64 {attrs} noinline local {{\nblock0(v0: i64):\n return v0\n}}"
                ),
                "noinline",
            ),
            (format!("decl @h(i64) -> i64 {attrs}"), "no_definition"),
            (
                format!(
                    "func @h(i64) -> i64 {attrs} local {{\nblock0(v0: i64):\n v1 = call @h(v0)\n return v1\n}}"
                ),
                "recursive_callee",
            ),
            (
                format!(
                    "func @h(i64) -> i64 {attrs} local {{\nblock0(v0: i64):\n v1 = intrinsic stack_alloc(v0)\n return v0\n}}"
                ),
                "unsupported_callee_intrinsic",
            ),
        ] {
            // Conflicting alwaysinline/noinline attributes are invalid IR.
            if !attrs.is_empty() && expected == "noinline" {
                continue;
            }
            let (_, report) = observed(&caller(&helper), &["inline"]);
            let p = &report.invocations[0];
            assert_eq!(p.function, None);
            assert_eq!(p.changed, expected == "inlined");
            let site = p.inline_sites.iter().find(|s| s.caller == "f").unwrap();
            assert_eq!(site.outcome, expected);
            assert_eq!(
                site.category,
                if attrs.is_empty() {
                    "ordinary"
                } else {
                    "alwaysinline"
                }
            );
        }
    }
}

struct Noop;
impl Pass for Noop {
    fn name(&self) -> &'static str {
        "noop"
    }
    fn description(&self) -> &'static str {
        "test canonical entry"
    }
    fn requires_canonical_form(&self) -> bool {
        true
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
}
#[test]
fn changed_false_is_separate_from_canonical_restoration() {
    let mut module = keel_ir::parse("func @f(i64, i64) -> i64 {\n ss0 = stack_slot 17, align 8\nblock0(v0: i64, v1: i64):\n v2 = iadd v1, v0\n return v2\n}").unwrap();
    let mut plain = module.clone();
    keel_opt::run_pipeline(&mut plain, &[&Noop, &Noop]).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_pipeline_for_target_with_report(
        &mut module,
        &[&Noop, &Noop],
        VectorTarget::X86Sse2,
        &mut report,
    )
    .unwrap();
    assert_eq!(plain.to_string(), module.to_string());
    let p = &report.invocations[0];
    assert!(!p.changed);
    assert_eq!(p.restoration_changed, Some(true));
    assert_eq!(p.before, p.after);
    assert_eq!(p.before.instructions, 2);
    assert_eq!(p.before.block_parameters, 2);
    assert_eq!(p.before.stack_declarations, 1);
    assert_eq!(p.before.stack_declaration_bytes, 17);
    assert_eq!(p.target, VectorTarget::X86Sse2);
    assert_eq!(report.invocations[1].pipeline_index, 1);
    assert_eq!(report.invocations[1].restoration_changed, None);
    assert_eq!(report.invocations[1].restoration_ns, 0);
    let json = report.to_json();
    assert!(json.starts_with("{\"schema_version\":2,\"invocations\":["));
    assert!(json.contains("\"target\":\"x86-sse2\""));
    assert!(!json.contains("frame_bytes"));
}

struct Break;
impl Pass for Break {
    fn name(&self) -> &'static str {
        "break"
    }
    fn description(&self) -> &'static str {
        "test verification failure"
    }
    fn run(&self, f: &mut Function) -> bool {
        let inst = f.layout_insts().last().unwrap().1;
        f.remove_inst(inst);
        true
    }
}
#[test]
fn errors_match_and_stop_pipeline_without_extra_passes() {
    let source = "func @f() {\nblock0:\n return\n}";
    let mut plain = keel_ir::parse(source).unwrap();
    let mut observed = plain.clone();
    let mut report = OptimizationReport::default();
    let options = keel_opt::OptimizeOptions {
        verification: keel_opt::VerificationPolicy::Enabled,
    };
    let expected = keel_opt::run_pipeline_for_target_with_options(
        &mut plain,
        &[&Break, &Noop],
        VectorTarget::Portable,
        options,
    )
    .unwrap_err();
    let error = keel_opt::run_pipeline_for_target_with_report_and_options(
        &mut observed,
        &[&Break, &Noop],
        VectorTarget::Portable,
        &mut report,
        options,
    )
    .unwrap_err();
    assert_eq!(expected.to_string(), error.to_string());
    assert_eq!(report.invocations.len(), 1);
    assert!(report.invocations[0].changed);
    assert!(report.invocations[0].after.instructions < report.invocations[0].before.instructions);
    let mut unknown = OptimizationReport::default();
    assert!(
        keel_opt::run_passes_for_target_with_report(
            &mut observed,
            &["missing"],
            VectorTarget::Portable,
            &mut unknown
        )
        .is_err()
    );
    assert!(unknown.invocations.is_empty());
}

#[test]
fn signature_and_caller_restrictions_are_site_decisions() {
    let source = caller("func @h(i64) -> i64 local {\nblock0(v0: i64):\n return v0\n}");
    for (restriction, expected) in [
        (0, "abi_signature_mismatch"),
        (1, "returns_twice_caller"),
        (2, "caller_personality"),
        (3, "unsupported_call_instruction"),
    ] {
        let mut module = keel_ir::parse(&source).unwrap();
        let f = &mut module.functions[1];
        match restriction {
            0 => {
                let r = f.callees().next().unwrap().0;
                f.callee_mut(r).sig.returns_twice = true;
            }
            1 => f.sig.returns_twice = true,
            2 => f.personality = Some("personality".into()),
            _ => {
                let call = f
                    .layout_insts()
                    .find(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
                    .unwrap()
                    .1;
                let InstData::Call { func, args } = f.inst_data(call).clone() else {
                    unreachable!()
                };
                *f.inst_data_mut(call) = InstData::ReturnCall { func, args };
            }
        }
        // Exercise decisions directly: a deliberately mismatched ABI or replaced
        // tail call need not constitute valid whole-module input for verification.
        let mut sites = Vec::new();
        keel_opt::find_pass("inline")
            .unwrap()
            .run_module_with_report(&mut module, &mut sites);
        assert_eq!(sites[0].outcome, expected);
    }
}

#[test]
fn schema_order_is_repeatable_when_timings_are_removed() {
    let source = caller("func @h(i64) -> i64 local {\nblock0(v0: i64):\n return v0\n}");
    let normalize = |mut report: OptimizationReport| {
        for p in &mut report.invocations {
            p.pass_ns = 0;
            p.verification_ns = 0;
            p.restoration_ns = 0;
            p.restoration_verification_ns = 0;
        }
        report.to_json()
    };
    let pipeline = &["inline", "instcombine", "dce", "inline"];
    assert_eq!(
        normalize(observed(&source, pipeline).1),
        normalize(observed(&source, pipeline).1)
    );
}

#[test]
fn selective_stack_cfg_is_one_composite_observation_for_every_target() {
    let source = "decl @yes() -> i64
func @f() -> i64 {
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = iconst.i8 1
 store v1, v0
 v2 = load.i8 v0
 brif v2, block1, block2
block1:
 v3 = call @yes()
 return v3
block2:
 trap
}";
    for target in [
        VectorTarget::Portable,
        VectorTarget::X86Sse2,
        VectorTarget::Aarch64Neon,
    ] {
        for names in [
            &["stack-forward-cfg", "stack-forward-cfg"][..],
            keel_opt::OptimizationTier::Full.pipeline(),
        ] {
            let mut plain = keel_ir::parse(source).unwrap();
            keel_ir::verify_module(&plain).unwrap();
            let mut instrumented = plain.clone();
            keel_opt::run_passes_for_target(&mut plain, names, target).unwrap();
            let mut report = OptimizationReport::default();
            keel_opt::run_passes_for_target_with_report(
                &mut instrumented,
                names,
                target,
                &mut report,
            )
            .unwrap();
            keel_ir::verify_module(&instrumented).unwrap();
            assert_eq!(plain.to_string(), instrumented.to_string());
            assert_eq!(report.invocations.len(), names.len());
            for (index, p) in report.invocations.iter().enumerate() {
                assert_eq!(p.pipeline_index, index);
                assert_eq!(p.pass, names[index]);
                assert_eq!(p.target, target);
            }
            let mut sf = report
                .invocations
                .iter()
                .filter(|p| p.pass == "stack-forward-cfg" || p.pass == "stack-forward");
            let first = sf.next().unwrap();
            assert_eq!(first.function.as_deref(), Some("f"));
            assert!(first.changed);
            assert_eq!(first.before.blocks, 3);
            assert_eq!(
                first.after.blocks,
                if names[0] == "stack-forward-cfg" {
                    1
                } else {
                    3
                }
            );
            assert!(first.after.instructions < first.before.instructions);
            assert_eq!(first.restoration_changed, None);
            for later in sf {
                assert!(!later.changed);
                assert_eq!(later.before, later.after);
            }
            assert_eq!(
                report
                    .invocations
                    .iter()
                    .filter(|p| p.pass == "simplify-cfg")
                    .count(),
                names.iter().filter(|&&name| name == "simplify-cfg").count()
            );
        }
    }
}
