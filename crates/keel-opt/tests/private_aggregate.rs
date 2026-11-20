use keel_ir::{InstData, Purpose, Type, parse, verify_module};
use keel_opt::private_aggregate;

const SOURCE: &str =
    "func @pair(ptr byval [2 x i64], ptr byval {i64,i64}, ptr) -> i64 noinline local {
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = iconst.i64 8
 v4 = ptradd v0,v3
 v5 = load.i64 v4
 v6 = iconst.i64 99
 store v6,v0
 store v6,v2
 v7 = load.i64 v1
 v8 = iadd v5,v7
 return v8
}
func @entry(ptr) -> i64 {
block0(v0:ptr):
 v1 = call @pair(v0,v0,v0)
 return v1
}";
fn check(s: &str, expected: usize) -> keel_ir::Module {
    let mut m = parse(s).unwrap();
    verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(private_aggregate::run(&mut m), expected);
    verify_module(&m).unwrap();
    if expected == 0 {
        assert_eq!(before, m.to_string());
    }
    assert_eq!(private_aggregate::run(&mut m), 0);
    m
}
#[test]
fn independent_homes_and_call_snapshots() {
    let mut m = check(SOURCE, 1);
    let f = &m.functions[0];
    assert_eq!(
        f.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
        [Type::I64, Type::I64, Type::I64, Type::I64, Type::Ptr]
    );
    assert!(f.sig.params.iter().all(|p| p.purpose == Purpose::Normal));
    assert_eq!(
        f.stack_slots()
            .map(|(_, s)| (s.size, s.align))
            .collect::<Vec<_>>(),
        [(16, 8), (16, 8)]
    );
    let loads: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| {
            matches!(f.inst_data(i), InstData::Load { ty: Type::I64, .. }).then_some(i)
        })
        .collect();
    // The initial high-word read is gone. The read after mutation stays.
    assert_eq!(loads.len(), 1);
    assert_eq!(
        f.resolve(f.first_result(loads[0])),
        f.first_result(loads[0])
    );
    assert_eq!(
        m.functions[1]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[1].inst_data(*i), InstData::Load { .. }))
            .count(),
        4
    );
    for (_, i) in m.functions[1].layout_insts() {
        if let InstData::Load { flags, .. } = m.functions[1].inst_data(i) {
            assert_eq!(flags.align, 1);
        }
    }
    keel_opt::run_passes(
        &mut m,
        &[
            "stack-forward",
            "stack-promote",
            "dce",
            "internal-fast-calls",
        ],
    )
    .unwrap();
    verify_module(&m).unwrap();
}
#[test]
fn public_addresses_padding_constraints_and_observers_are_unchanged() {
    for s in [
        SOURCE.replace(" noinline local", ""),
        SOURCE.replace(" noinline local", " weak_odr"),
        SOURCE.replace("[2 x i64]", "{i8,i64}"),
        SOURCE.replace("[2 x i64]", "[3 x i64]"),
        SOURCE.replace("store v6,v0", "store align 16 v6,v0"),
        format!("{SOURCE}\ndata @fp = align 8 {{ptr @pair}}"),
        format!(
            "{SOURCE}\nfunc @address()->ptr {{\nblock0:\n v0 = symbol_addr @pair\n return v0\n}}"
        ),
        SOURCE.replace(" return v8", " asm \"nop\", \"\"()\n return v8"),
        SOURCE.replace(" return v8", " v9 = intrinsic frame_address()\n return v8"),
    ] {
        check(&s, 0);
    }
    let mut debug = parse(SOURCE).unwrap();
    debug.files.push("fixture.c".into());
    let before = debug.to_string();
    assert_eq!(private_aggregate::run(&mut debug), 0);
    assert_eq!(before, debug.to_string());
}
/// Lifetime markers scope the function's own slots and are no frame
/// facility, so they leave both definitions eligible.
#[test]
fn lifetime_markers_keep_definitions_eligible() {
    let marked = SOURCE
        .replace(
            "block0(v0:ptr,v1:ptr,v2:ptr):",
            " ss0 = stack_slot 8, align 8\nblock0(v0:ptr,v1:ptr,v2:ptr):",
        )
        .replace(
            " return v8\n}\nfunc @entry",
            " v9 = stack_addr ss0\n intrinsic lifetime_start(v9)\n store v8,v9\n \
             intrinsic lifetime_end(v9)\n return v8\n}\nfunc @entry",
        );
    check(&marked, 1);
}
#[test]
fn recursive_calls_and_sret_preserve_result_contract() {
    let s = "func @rec(ptr sret [2 x i64], ptr byval [2 x i64], i64) noinline local {
block0(v0:ptr,v1:ptr,v2:i64):
 v3 = iconst.i64 0
 v4 = icmp.eq v2,v3
 brif v4,block1,block2
block1:
 v5 = load.i64 v1
 store v5,v0
 return
block2:
 call @rec(v0,v1,v3)
 return
}";
    let m = check(s, 1);
    assert!(matches!(
        m.functions[0].sig.params[0].purpose,
        Purpose::Sret(_)
    ));
    assert_eq!(m.functions[0].sig.params.len(), 4);
}
#[test]
fn cleanup_policy_is_full_only() {
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert_eq!(
            tier.pipeline().contains(&"private-aggregate-cleanup"),
            tier == keel_opt::OptimizationTier::Full
        );
    }
    let full = keel_opt::OptimizationTier::Full.pipeline();
    assert_eq!(
        &full[..7],
        [
            "specialize-zero-flags",
            "private-aggregate-cleanup",
            "known-bits",
            "const-fold",
            "devirtualize-const-table",
            "loop-matmul",
            "inline-cheap-reserve"
        ]
    );
    assert_eq!(
        full.iter()
            .filter(|&&p| p == "private-aggregate-cleanup")
            .count(),
        2
    );
    assert!(full.ends_with(&[
        "private-aggregate-cleanup",
        "fast-math",
        "internal-fast-calls",
        "tail-calls",
        "branch-facts",
        "outline-cold",
        "loop-unroll-words",
    ]));
    let mut m = parse(SOURCE).unwrap();
    assert_ne!(private_aggregate::run(&mut m), 0);
    verify_module(&m).unwrap();
    assert!(
        m.functions[0]
            .sig
            .params
            .iter()
            .all(|p| p.purpose == Purpose::Normal)
    );
}

#[test]
fn conditional_cleanup_matches_the_explicit_pipeline_and_skips_unrelated_modules() {
    let mut expected = parse(SOURCE).unwrap();
    assert_ne!(private_aggregate::run(&mut expected), 0);
    keel_opt::run_passes(
        &mut expected,
        &[
            "stack-forward",
            "stack-promote",
            "canonicalize",
            "instcombine",
            "dce",
        ],
    )
    .unwrap();
    let mut actual = parse(SOURCE).unwrap();
    keel_opt::run_passes(&mut actual, &["private-aggregate-cleanup"]).unwrap();
    verify_module(&actual).unwrap();
    assert_eq!(actual.to_string(), expected.to_string());

    let mut unrelated = parse("func @plain(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 0\n v2 = iadd v0,v1\n return v2\n}").unwrap();
    let before = unrelated.to_string();
    assert!(!private_aggregate::run_with_cleanup(&mut unrelated));
    assert_eq!(unrelated.to_string(), before);
    // Demonstrate that an unconditional cleanup would alter this module.
    keel_opt::run_passes(&mut unrelated, &["canonicalize", "instcombine", "dce"]).unwrap();
    assert_ne!(unrelated.to_string(), before);
}

#[test]
fn definition_restrict_contracts_are_conservatively_excluded() {
    let mut m = parse(SOURCE).unwrap();
    m.functions[0].restrict_params.push(2);
    verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(private_aggregate::run(&mut m), 0);
    assert_eq!(before, m.to_string());
}
#[test]
fn tail_and_returns_twice_endpoints_are_excluded() {
    let s = SOURCE.replace(
        " v1 = call @pair(v0,v0,v0)\n return v1",
        " return_call @pair(v0,v0,v0)",
    );
    check(&s, 0);
    check(
        &SOURCE.replace("noinline local", "returns_twice noinline local"),
        0,
    );
}

#[test]
fn explicit_custom_pipeline_verifies_module_rewrite() {
    let mut m = parse(SOURCE).unwrap();
    keel_opt::run_pipeline(&mut m, &[&private_aggregate::PrivateAggregateCleanup]).unwrap();
    verify_module(&m).unwrap();
    assert_eq!(m.functions[0].sig.params.len(), 5);
}

// Probe the restricted ARM argument-register projection, including scalar
// exhaustion between pairs. SysV's six GPRs do not impose this exclusion:
// both its register and stack aggregate reads already follow parameter order.
#[test]
fn source_order_eligibility_accounts_for_scalar_banks_and_sret() {
    fn signature(params: &[&str]) -> String {
        let args = params
            .iter()
            .enumerate()
            .map(|(i, p)| format!("v{i}:{}", p.split_whitespace().next().unwrap()))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "func @probe({}) noinline local {{\nblock0({args}):\n return\n}}",
            params.join(",")
        )
    }
    let pair = "ptr byval [2 x i64]";
    for (params, expected) in [
        (vec![pair; 4], 1), // x86 mixed register/stack; ARM all-register
        (vec![pair; 5], 0),
        (vec![pair; 10], 0),
        ([vec!["i64"; 8], vec![pair; 3]].concat(), 1), // all-stack
        ([vec!["i64"; 7], vec![pair; 2]].concat(), 1), // closes ARM GPR bank
        ([vec!["i64"; 6], vec![pair; 2]].concat(), 0),
        ([vec!["f64"; 8], vec![pair; 4]].concat(), 1),
        ([vec!["f64"; 8], vec![pair; 5]].concat(), 0),
        ([vec!["i128"; 3], vec![pair; 2]].concat(), 0),
        ([vec!["i128"; 4], vec![pair; 2]].concat(), 1),
        ([vec![pair], vec!["i128"; 3], vec![pair]].concat(), 0),
        ([vec!["ptr sret [3 x i64]"], vec![pair; 4]].concat(), 1),
        ([vec!["ptr sret [3 x i64]"], vec![pair; 5]].concat(), 0),
    ] {
        check(&signature(&params), expected);
    }
}

#[test]
fn conditional_cleanup_is_reported_and_obeys_controls() {
    use keel_opt::{OptimizationReport, OptimizeOptions, Pipeline};
    let passes = [keel_opt::find_pass("private-aggregate-cleanup").unwrap()];
    let mut module = parse(SOURCE).unwrap();
    let mut report = OptimizationReport::default();
    Pipeline::new(&passes)
        .report(&mut report)
        .run(&mut module)
        .unwrap();
    let mut names: Vec<_> = report.invocations.iter().map(|item| item.pass).collect();
    names.dedup();
    assert_eq!(
        names,
        [
            "private-aggregate-cleanup",
            "stack-forward",
            "stack-promote",
            "canonicalize",
            "instcombine",
            "dce"
        ]
    );
    assert!(
        report
            .invocations
            .iter()
            .all(|item| item.pipeline_index == 0 && item.verification_enabled)
    );

    let mut module = parse(SOURCE).unwrap();
    let mut report = OptimizationReport::default();
    Pipeline::new(&passes)
        .options(OptimizeOptions {
            disabled: vec!["dce".into()],
            ..Default::default()
        })
        .report(&mut report)
        .run(&mut module)
        .unwrap();
    assert!(!report.invocations.iter().any(|item| item.pass == "dce"));

    let mut module = parse(SOURCE).unwrap();
    let mut report = OptimizationReport::default();
    Pipeline::new(&passes)
        .options(OptimizeOptions {
            bisect_limit: Some(1),
            ..Default::default()
        })
        .report(&mut report)
        .run(&mut module)
        .unwrap();
    assert_eq!(report.invocations.len(), 1);
    assert_eq!(report.invocations[0].pass, "private-aggregate-cleanup");
    verify_module(&module).unwrap();

    let mut module = parse(
        "func @plain() {
entry:
 return
}
",
    )
    .unwrap();
    let mut report = OptimizationReport::default();
    Pipeline::new(&passes)
        .report(&mut report)
        .run(&mut module)
        .unwrap();
    assert_eq!(report.invocations.len(), 1);
    assert!(!report.invocations[0].changed);
}

#[test]
#[should_panic(expected = "cyclic cleanup sequence")]
fn cyclic_cleanup_is_rejected_before_running_the_parent() {
    struct Cycle;
    impl keel_opt::Pass for Cycle {
        fn name(&self) -> &'static str {
            "cycle"
        }
        fn description(&self) -> &'static str {
            "invalid cleanup graph"
        }
        fn run(&self, _: &mut keel_ir::Function, _: &keel_opt::OptimizationContext) -> bool {
            panic!("invalid pipeline ran a pass")
        }
        fn cleanup_passes(&self) -> &'static [&'static dyn keel_opt::Pass] {
            &[&Cycle]
        }
    }
    let passes: &[&dyn keel_opt::Pass] = &[&Cycle];
    let mut module = keel_ir::Module::default();
    keel_opt::Pipeline::new(passes).run(&mut module).unwrap();
}

#[test]
fn function_policy_wrappers_preserve_conditional_cleanup() {
    use keel_opt::{OptimizationReport, Pipeline};
    let passes = [keel_opt::find_pass("private-aggregate-cleanup").unwrap()];
    let mut module = parse(SOURCE).unwrap();
    for function in &mut module.functions {
        // Trigger scoped scheduling while retaining this custom pass list.
        function.attrs.optimize =
            keel_ir::FunctionOptimization::new([keel_ir::OptimizationOption::Options(
                "no-unroll-loops".into(),
            )]);
    }
    module.sync_function_attrs();
    verify_module(&module).unwrap();
    let mut report = OptimizationReport::default();
    Pipeline::new(&passes)
        .report(&mut report)
        .run(&mut module)
        .unwrap();
    assert!(report.invocations.iter().any(|item| item.pass == "dce"));
    verify_module(&module).unwrap();
}

#[test]
fn cleanup_inherits_parent_scope_and_custom_children_can_be_disabled() {
    use keel_opt::{ModuleContext, OptimizationContext, OptimizeOptions, Pass, Pipeline};
    struct Mark;
    impl Pass for Mark {
        fn name(&self) -> &'static str {
            "mark-cleanup"
        }
        fn description(&self) -> &'static str {
            "mark enabled functions"
        }
        fn run(&self, function: &mut keel_ir::Function, _: &OptimizationContext) -> bool {
            function.attrs.fast_math = true;
            true
        }
    }
    struct Parent;
    impl Pass for Parent {
        fn name(&self) -> &'static str {
            "selected-parent"
        }
        fn description(&self) -> &'static str {
            "select one function"
        }
        fn enabled_for(&self, function: &keel_ir::Function) -> bool {
            function.name == "selected"
        }
        fn run(&self, _: &mut keel_ir::Function, _: &OptimizationContext) -> bool {
            false
        }
        fn run_module(&self, _: &mut keel_ir::Module, _: &mut ModuleContext<'_>) -> Option<bool> {
            Some(true)
        }
        fn cleanup_passes(&self) -> &'static [&'static dyn Pass] {
            &[&Mark]
        }
    }
    let passes: &[&dyn Pass] = &[&Parent];
    let source = "func @selected() {
entry:
 return
}
func @protected() {
entry:
 return
}
";
    let mut module = parse(source).unwrap();
    Pipeline::new(passes).run(&mut module).unwrap();
    assert!(module.functions[0].attrs.fast_math);
    assert!(!module.functions[1].attrs.fast_math);
    let mut module = parse(source).unwrap();
    Pipeline::new(passes)
        .options(OptimizeOptions {
            disabled: vec!["mark-cleanup".into()],
            ..Default::default()
        })
        .run(&mut module)
        .unwrap();
    assert!(
        module
            .functions
            .iter()
            .all(|function| !function.attrs.fast_math)
    );
}
