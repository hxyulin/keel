//! Full reserve bounds and fairness, without relying on cleanup or names.
use keel_ir::Module;
use keel_opt::{InlineSite, OptimizationReport};

const PASS: &str = "inline-cheap-reserve";
fn run(source: &str, pass: &str) -> (Module, Vec<InlineSite>) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut m,
        &[pass],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    (m, report.invocations.remove(0).inline_sites)
}
fn leaf(name: &str, cost: usize, attrs: &str, predicate: bool) -> String {
    let mut s = format!("func @{name}() {attrs} {{\nblock0:\n v0 = iconst.i32 0\n");
    if predicate {
        s += " v1 = icmp.eq v0, v0\n";
    }
    for i in (if predicate { 2 } else { 1 })..cost - 1 {
        s += &format!(" v{i} = iconst.i32 {i}\n");
    }
    s + " return\n}\n"
}
fn caller(name: &str, callee: &str, count: usize) -> String {
    format!(
        "func @{name}() {{\nblock0:\n{} return\n}}\n",
        format!(" call @{callee}()\n").repeat(count)
    )
}
fn saturated() -> String {
    let mut s = leaf("constructor", 16, "alwaysinline local", false);
    for i in 0..32 {
        s += &caller(&format!("budget{i}"), "constructor", 128);
    }
    s
}
fn count(m: &Module) -> usize {
    m.functions.iter().map(|f| f.layout_insts().count()).sum()
}
fn admitted(sites: &[InlineSite]) -> Vec<&InlineSite> {
    sites
        .iter()
        .filter(|s| s.outcome == "cheap_reserve_inlined")
        .collect()
}

#[test]
fn module_growth_is_exactly_bounded_and_late_callers_get_a_turn() {
    let source = saturated()
        + &leaf("predicate", 32, "local", true)
        + &caller("early", "predicate", 100)
        + &caller("late", "predicate", 1)
        + &caller("middle", "predicate", 100);
    let before = keel_ir::parse(&source).unwrap();
    let (baseline, _) = run(&source, "inline-noninterposable");
    let (m, sites) = run(&source, PASS);
    let reserve = admitted(&sites);
    assert_eq!(reserve.len(), 128);
    assert_eq!(reserve[0].caller, "early");
    assert_eq!(reserve[1].caller, "late");
    assert!(
        reserve
            .iter()
            .all(|s| s.experimental_net_growth == Some(32))
    );
    assert_eq!(count(&baseline) - count(&before), 16_384);
    assert_eq!(count(&m) - count(&before), 65_536 + 4096);
    assert_eq!(
        sites
            .iter()
            .filter(|s| s.callee == "constructor" && s.outcome != "inlined")
            .count(),
        0
    );
}

#[test]
fn module_site_count_is_a_separate_exact_bound() {
    let mut source = saturated() + &leaf("predicate", 3, "local", true);
    for i in 0..257 {
        source += &caller(&format!("late{i}"), "predicate", 1);
    }
    let (_, sites) = run(&source, PASS);
    assert_eq!(admitted(&sites).len(), 256);
    assert_eq!(
        sites
            .iter()
            .find(|s| s.caller == "late256")
            .unwrap()
            .outcome,
        "module_instruction_growth_budget"
    );
}

#[test]
fn reserve_has_no_caller_growth_count_or_stack_exemption() {
    for (cost, calls, reason) in [
        (16, 128, "caller_call_count_budget"),
        (32, 64, "caller_instruction_growth_budget"),
    ] {
        // A 6-instruction predicate costs more than its call, so the
        // call-count cap applies to it.
        let source = leaf("constructor", cost, "alwaysinline local", false)
            + &leaf("predicate", 6, "local", true)
            + &caller("caller", "constructor", calls)
                .replace(" return\n}", " call @predicate()\n return\n}");
        let (_, sites) = run(&source, PASS);
        assert_eq!(sites.last().unwrap().outcome, reason);
        assert!(admitted(&sites).is_empty());
    }
    // Stack ledger is independent of instruction growth: 64 * 1024 bytes.
    let source = leaf("stack", 3, "alwaysinline local", false)
        .replace("block0:", " ss0 = stack_slot 1024, align 8\nblock0:")
        .replace(" v1 = iconst.i32 1", " v1 = stack_addr ss0")
        + &leaf("predicate", 3, "local", true)
            .replace("block0:", " ss0 = stack_slot 1, align 1\nblock0:")
            .replace(" return", " v2 = stack_addr ss0\n return")
        + &caller("caller", "stack", 64).replace(" return\n}", " call @predicate()\n return\n}");
    let (_, sites) = run(&source, PASS);
    // Count and instruction growth still have room.
    assert_eq!(sites.last().unwrap().outcome, "caller_stack_growth_budget");
    assert!(admitted(&sites).is_empty());
}

#[test]
fn structural_boundary_and_definition_binding_are_required() {
    for (cost, attrs, predicate, expected) in [
        (48, "local", true, true),
        (49, "local", true, false),
        (16, "local", false, false),
        (16, "", true, false),
        (16, "noninterposable", true, true),
        (16, "noinline local", true, false),
    ] {
        let source =
            saturated() + &leaf("helper", cost, attrs, predicate) + &caller("late", "helper", 1);
        assert_eq!(
            !admitted(&run(&source, PASS).1).is_empty(),
            expected,
            "{cost} {attrs} {predicate}"
        );
    }
    for body in [
        "func @helper() local {\nblock0:\n v0 = iconst.i32 0\n v1 = icmp.eq v0, v0\n call @helper()\n return\n}\n",
        "func @helper() local {\nblock0:\n v0 = iconst.i32 0\n v1 = icmp.eq v0, v0\n jump block1\nblock1:\n brif v1, block1, block2\nblock2:\n return\n}\n",
    ] {
        let source = saturated() + body + &caller("late", "helper", 1);
        assert!(admitted(&run(&source, PASS).1).is_empty());
    }
}

#[test]
fn full_uses_the_reserve_and_smaller_tiers_keep_their_pipelines() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
    ] {
        assert!(!tier.pipeline().contains(&PASS));
    }
    assert!(keel_opt::OptimizationTier::Full.pipeline().contains(&PASS));
    let source = saturated() + &leaf("helper", 16, "local", true) + &caller("late", "helper", 1);
    assert!(admitted(&run(&source, "inline-noninterposable").1).is_empty());
    assert!(admitted(&run(&source, "inline-aggregate-reserve").1).is_empty());
}

#[test]
fn deferred_clones_resolve_api_aliases_and_keep_definitions() {
    let source = saturated()
        + "func @helper(i64, i64) -> i64 local {\nblock0(v0: i64, v1: i64):\n v2 = icmp.eq v0, v1\n v3 = iadd v0, v1\n return v3\n}\nfunc @late(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = call @helper(v0, v1)\n return v2\n}\n";
    let mut m = keel_ir::parse(&source).unwrap();
    let helper = m.functions.len() - 2;
    let late = helper + 1;
    let h = &mut m.functions[helper];
    let params = h.block_params(h.entry_block().unwrap()).to_vec();
    let add = h.layout_insts().nth(1).unwrap().1;
    h.replace_all_uses(params[0], params[1]);
    h.replace_all_uses(h.first_result(add), params[1]);
    h.remove_inst(add);
    let original = h.to_string();
    let f = &mut m.functions[late];
    let params = f.block_params(f.entry_block().unwrap()).to_vec();
    f.replace_all_uses(params[0], params[1]);
    keel_ir::verify_module(&m).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut m,
        &[PASS],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    assert_eq!(admitted(&report.invocations[0].inline_sites).len(), 1);
    assert_eq!(m.functions[helper].to_string(), original);
    keel_opt::run_passes(&mut m, &["simplify-cfg", "dce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[late];
    let ret = f.layout_insts().last().unwrap().1;
    let keel_ir::InstData::Return { args } = f.inst_data(ret) else {
        panic!()
    };
    assert_eq!(f.resolve(args[0]), f.resolve(params[1]));
}

#[test]
fn legality_rejections_match_full_even_when_module_is_saturated() {
    for body in [
        "func @helper() local {\nblock0:\n v0 = intrinsic frame_address()\n return\n}\n",
        "func @helper() returns_twice local {\nblock0:\n return\n}\n",
    ] {
        let source = saturated() + body + &caller("late", "helper", 1);
        let (_, ordinary) = run(&source, "inline-noninterposable");
        let (_, candidate) = run(&source, PASS);
        assert_eq!(
            ordinary.last().unwrap().outcome,
            candidate.last().unwrap().outcome
        );
        assert!(admitted(&candidate).is_empty());
    }
}

#[test]
fn snapshot_cost_stack_and_scc_caller_guard_remain_bounded() {
    for (bytes, expected) in [(64, true), (65, false)] {
        let source = saturated()
            + &leaf("helper", 16, "local", true)
                .replace(
                    "block0:",
                    &format!(" ss0 = stack_slot {bytes}, align 1\nblock0:"),
                )
                .replace(" v2 = iconst.i32 2", " v2 = stack_addr ss0")
            + &caller("late", "helper", 1);
        assert_eq!(!admitted(&run(&source, PASS).1).is_empty(), expected);
    }
    // A 16-byte byval snapshot costs 8 IR instructions, which is charged
    // before admission rather than hoping later cleanup removes the copy.
    for (cost, expected) in [(40, true), (41, false)] {
        let helper = leaf("helper", cost, "local", true)
            .replace("@helper()", "@helper(ptr byval [2 x i64])")
            .replace("block0:", "block0(v100: ptr):");
        let source = saturated()
            + &helper
            + "func @late(ptr) {\nblock0(v0: ptr):\n call @helper(v0)\n return\n}\n";
        assert_eq!(!admitted(&run(&source, PASS).1).is_empty(), expected);
    }
    let wrapper = "func @wrap() inlinehint local {\nblock0:\n call @slow()\n return\n}\nfunc @slow() noinline {\nblock0:\n return_call @wrap()\n}\n";
    for (size, expected) in [(8192, "inlined"), (8193, "recursive_wrapper_caller_size")] {
        let mut source = wrapper.to_owned() + "func @large() {\nblock0:\n call @wrap()\n";
        for i in 0..size - 2 {
            source += &format!(" v{i} = iconst.i32 0\n");
        }
        source += " return\n}\n";
        let (_, sites) = run(&source, PASS);
        assert_eq!(
            sites.iter().find(|s| s.caller == "large").unwrap().outcome,
            expected
        );
    }
}

#[test]
fn guarded_hinted_leaves_precede_ordinary_growth_without_expanding_budgets() {
    let helper = "func @value_helper(ptr) inlinehint local {\nblock0(v0: ptr):\n v1 = load.i32 v0\n v2 = iconst.i32 1\n v3 = icmp.ne v1, v2\n brif v3, block1, block2\nblock1:\n v4 = iadd v1, v2\n store v4, v0\n jump block2\nblock2:\n return\n}\n";
    let late = "func @late(ptr) {\nblock0(v0: ptr):\n call @value_helper(v0)\n return\n}\n";
    let source = saturated().replace("alwaysinline local", "local") + helper + late;
    let before = keel_ir::parse(&source).unwrap();
    let (_, baseline_sites) = run(&source, "inline-noninterposable");
    assert_eq!(
        baseline_sites.last().unwrap().outcome,
        "module_instruction_growth_budget"
    );
    let (m, sites) = run(&source, PASS);
    assert_eq!(sites.first().unwrap().caller, "late");
    assert_eq!(sites.first().unwrap().outcome, "inlined");
    assert!(count(&m) - count(&before) <= 65_536);
    assert!(admitted(&sites).is_empty());
    let source = helper.to_owned()
        + &late.replace(
            " call @value_helper(v0)\n",
            &" call @value_helper(v0)\n".repeat(129),
        );
    let (_, sites) = run(&source, PASS);
    assert_eq!(sites.last().unwrap().outcome, "caller_call_count_budget");
}
