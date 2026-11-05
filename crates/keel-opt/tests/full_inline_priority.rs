//! Scheduling regressions: no cleanup or machine-dependent profitability involved.
use keel_ir::{InstData, Module};
use keel_opt::{InlineSite, OptimizationReport};

const FULL: &str = "inline-noninterposable";
// Full's own inliner, which also gives tiny hinted leaves priority.
const RESERVE: &str = "inline-cheap-reserve";
const CONSTRUCTORS: &str = include_str!("fixtures/full-inline-priority-constructors.keel");

fn optimize(m: &mut Module, pass: &str) -> Vec<InlineSite> {
    keel_ir::verify_module(m).unwrap();
    let mut report = OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        m,
        &[pass],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    keel_ir::verify_module(m).unwrap();
    report.invocations.remove(0).inline_sites
}
// Deliberately stale call-site attributes are rejected by the inliner too.
fn unchecked(m: &mut Module, name: &str) -> Vec<InlineSite> {
    let mut report = OptimizationReport::default();
    keel_opt::Pipeline::named(&[name])
        .unwrap()
        .options(keel_opt::OptimizeOptions {
            verification: keel_opt::VerificationPolicy::Disabled,
            ..Default::default()
        })
        .report(&mut report)
        .run(m)
        .unwrap();
    report.invocations.remove(0).inline_sites
}
fn run(source: &str, pass: &str) -> (Module, Vec<InlineSite>) {
    let mut m = keel_ir::parse(source).unwrap();
    let sites = optimize(&mut m, pass);
    (m, sites)
}
fn outcomes<'a>(sites: &'a [InlineSite], caller: &str, callee: &str) -> Vec<&'a str> {
    sites
        .iter()
        .filter(|s| s.caller == caller && s.callee == callee)
        .map(|s| s.outcome)
        .collect()
}
fn leaf(name: &str, cost: usize, stack: usize, attrs: &str) -> String {
    let mut s = format!("func @{name}() {attrs} {{\n");
    if stack != 0 {
        s += &format!(" ss0 = stack_slot {stack}, align 1\n");
    }
    s += "block0:\n";
    for i in 1..cost {
        if stack != 0 && i == 1 {
            s += " v1 = stack_addr ss0\n";
        } else {
            s += &format!(" v{i} = iconst.i64 {i}\n");
        }
    }
    s + " return\n}\n"
}
fn caller(name: &str, target: &str, count: usize) -> String {
    format!(
        "func @{name}() {{\nblock0:\n{} return\n}}\n",
        format!(" call @{target}()\n").repeat(count)
    )
}
fn wrappers() -> String {
    // 64 instructions, one recursive slow edge. The return_call contributes to
    // SCC detection but cannot itself be expanded, so it spends no budget.
    let mut s = "func @wrap() inlinehint local {\nblock0:\n".to_owned();
    for i in 0..62 {
        s += &format!(" v{i} = iconst.i64 {i}\n");
    }
    s + " call @slow()\n return\n}\nfunc @slow() noinline {\nblock0:\n return_call @wrap()\n}\n"
}
fn saturated(tiny_cost: usize, tiny_stack: usize, attrs: &str) -> String {
    wrappers()
        + &leaf("tiny", tiny_cost, tiny_stack, attrs)
        + &caller("caller", "wrap", 32).replace(" return\n}", " call @tiny()\n return\n}")
}
fn calls(m: &Module, name: &str, target: &str) -> usize {
    let f = m.functions.iter().find(|f| f.name == name).unwrap();
    f.layout_insts().filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { func, .. } if f.callee(*func).name == target)).count()
}

#[test]
fn retained_constructors_precede_scc_growth_in_full_only() {
    let mut s = wrappers() + CONSTRUCTORS;
    s += "func @vm(ptr, ptr, i32) {\nblock0(v0: ptr, v1: ptr, v2: i32):\n";
    s += &" call @wrap()\n".repeat(32);
    s += " call @JS_NewInt32(v0, v1, v2)\n call @JS_NewBool(v0, v1, v2)\n return\n}\n";
    let (m, sites) = run(&s, FULL);
    assert_eq!(outcomes(&sites, "vm", "JS_NewInt32"), ["inlined"]);
    assert_eq!(outcomes(&sites, "vm", "JS_NewBool"), ["inlined"]);
    assert_eq!(calls(&m, "vm", "wrap"), 1);
    assert_eq!(calls(&m, "vm", "slow"), 31);
    let vm: Vec<_> = sites.iter().filter(|s| s.caller == "vm").collect();
    assert_eq!(vm[0].callee, "JS_NewInt32");
    assert_eq!(vm[1].callee, "JS_NewBool");
    assert_eq!(
        vm.last().unwrap().outcome,
        "caller_instruction_growth_budget"
    );
}

#[test]
fn priority_is_bounded_by_cost_stack_and_explicit_request() {
    for (cost, stack, attrs, pass, priority) in [
        (32, 64, "inlinehint local", RESERVE, true),
        (33, 64, "inlinehint local", RESERVE, false),
        (32, 65, "inlinehint local", RESERVE, false),
        (8, 0, "inlinehint local", FULL, false),
        (8, 0, "local", RESERVE, false),
        // Every alwaysinline site precedes the ordinary sites.
        (33, 64, "alwaysinline local", FULL, true),
        (32, 65, "alwaysinline local", FULL, true),
    ] {
        let (_, sites) = run(&saturated(cost, stack, attrs), pass);
        assert_eq!(
            outcomes(&sites, "caller", "tiny"),
            [if priority {
                "inlined"
            } else {
                "caller_instruction_growth_budget"
            }]
        );
        assert_eq!(
            outcomes(&sites, "caller", "wrap")
                .iter()
                .filter(|&&s| s == "inlined")
                .count(),
            if priority { 31 } else { 32 }
        );
    }
    // A nonleaf hinted wrapper is not part of the priority class.
    let s = saturated(8, 0, "inlinehint local")
        .replace(
            "func @tiny() inlinehint local {",
            "decl @external()\nfunc @tiny() inlinehint local {",
        )
        .replace(
            "func @tiny() inlinehint local {\nblock0:\n v1 = iconst.i64 1",
            "func @tiny() inlinehint local {\nblock0:\n call @external()",
        );
    assert_eq!(
        outcomes(&run(&s, RESERVE).1, "caller", "tiny"),
        ["caller_instruction_growth_budget"]
    );
}

#[test]
fn priority_is_module_wide_and_alwaysinline_spends_no_module_budget() {
    let mut s = wrappers() + &leaf("tiny", 8, 0, "alwaysinline local");
    for i in 0..8 {
        s += &caller(&format!("early{i}"), "wrap", 32);
    }
    s += &caller("late", "tiny", 1);
    s += &caller("last", "wrap", 1);
    let (m, sites) = run(&s, FULL);
    assert_eq!(outcomes(&sites, "late", "tiny"), ["inlined"]);
    assert_eq!(sites[0].caller, "late");
    // 256 wrappers of 64 instructions fill the module budget exactly.
    assert_eq!(outcomes(&sites, "early7", "wrap").last(), Some(&"inlined"));
    assert_eq!(
        outcomes(&sites, "last", "wrap"),
        ["module_instruction_growth_budget"]
    );
    assert_eq!(calls(&m, "early7", "slow"), 32);
    let before = keel_ir::parse(&s).unwrap();
    let count = |m: &Module| {
        m.functions
            .iter()
            .map(|f| f.layout_insts().count())
            .sum::<usize>()
    };
    assert_eq!(count(&m) - count(&before), 256 * 64 + 8);
}

#[test]
fn caller_growth_and_call_count_remain_hard_for_priority_sites() {
    for (cost, count, admitted, reason) in [
        (32, 65, 64, "caller_instruction_growth_budget"),
        // Cost 6 exceeds a zero-argument call's own cost of 5.
        (6, 129, 128, "caller_call_count_budget"),
    ] {
        let s = leaf("tiny", cost, 0, "alwaysinline local") + &caller("caller", "tiny", count);
        let (_, sites) = run(&s, FULL);
        let o = outcomes(&sites, "caller", "tiny");
        assert_eq!(o.iter().filter(|&&s| s == "inlined").count(), admitted);
        assert_eq!(o.last(), Some(&reason));
    }
    let mut s = leaf("tiny", 32, 0, "alwaysinline local");
    for i in 0..9 {
        s += &caller(&format!("caller{i}"), "tiny", 64);
    }
    // The module budget does not apply to alwaysinline sites.
    let (_, sites) = run(&s, FULL);
    assert!(
        outcomes(&sites, "caller8", "tiny")
            .iter()
            .all(|&s| s == "inlined")
    );
}

#[test]
fn priority_and_remaining_sites_share_call_count_and_stack_ledgers() {
    // Both cost 6, more than their calls, so both count against the cap.
    let s = leaf("ordinary", 6, 0, "local")
        + &leaf("tiny", 6, 0, "alwaysinline local")
        + &caller("caller", "ordinary", 1).replace(
            " return\n}",
            &(" call @tiny()\n".repeat(128) + " return\n}"),
        );
    let (_, sites) = run(&s, FULL);
    assert_eq!(
        outcomes(&sites, "caller", "ordinary"),
        ["caller_call_count_budget"]
    );
    // Priority consumes 64 stack bytes before the large alwaysinline body tries
    // to consume the whole stack ledger. Without a shared ledger both fit.
    let s = leaf("large", 2, 65_536, "alwaysinline local")
        + &leaf("tiny", 2, 64, "alwaysinline local")
        + "func @caller() {\nblock0:\n call @large()\n call @tiny()\n return\n}\n";
    let (_, sites) = run(&s, FULL);
    assert_eq!(outcomes(&sites, "caller", "tiny"), ["inlined"]);
    assert_eq!(
        outcomes(&sites, "caller", "large"),
        ["caller_stack_growth_budget"]
    );
}

#[test]
fn priority_keeps_binding_noinline_signature_and_recursion_checks() {
    for (attrs, expected) in [
        ("alwaysinline", "public_definition"),
        ("noinline local", "noinline"),
        ("alwaysinline noninterposable", "inlined"),
    ] {
        assert_eq!(
            outcomes(&run(&saturated(8, 0, attrs), FULL).1, "caller", "tiny"),
            [expected]
        );
    }
    let source = "func @tiny(i8 uext) -> i8 sext alwaysinline local {\nblock0(v0: i8):\n return v0\n}\nfunc @caller(i8 uext) -> i8 sext {\nblock0(v0: i8):\n v1 = call @tiny(v0)\n return v1\n}";
    let mut m = keel_ir::parse(source).unwrap();
    let f = &mut m.functions[1];
    let reference = f.callees().next().unwrap().0;
    f.callee_mut(reference).attrs.noinline = true;
    f.callee_mut(reference).attrs.alwaysinline = false;
    assert_eq!(
        outcomes(&unchecked(&mut m, FULL), "caller", "tiny"),
        ["noinline"]
    );
    // Signature attributes are invisible to the verifier's value-type check,
    // but must still match exactly at inline admission.
    let mut m = keel_ir::parse(source).unwrap();
    let f = &mut m.functions[1];
    let reference = f.callees().next().unwrap().0;
    f.callee_mut(reference).sig.params[0].ext = keel_ir::ArgExt::Sext;
    assert_eq!(
        outcomes(&unchecked(&mut m, FULL), "caller", "tiny"),
        ["abi_signature_mismatch"]
    );
    let s = "func @tiny() alwaysinline local {\nblock0:\n call @tiny()\n return\n}\n".to_owned()
        + &caller("caller", "tiny", 1);
    assert_eq!(
        outcomes(&run(&s, FULL).1, "caller", "tiny"),
        ["recursive_callee"]
    );
}

#[test]
fn wrapper_body_and_initial_caller_size_stay_frozen_across_phases() {
    // 8192 original instructions; priority cloning raises it over the guard,
    // but the SCC decision must remain the decision at pass entry.
    let mut s = wrappers() + &leaf("tiny", 8, 0, "alwaysinline local");
    s += "func @caller() {\nblock0:\n call @wrap()\n call @tiny()\n";
    for i in 0..8189 {
        s += &format!(" v{} = iconst.i64 0\n", i + 10000);
    }
    s += " return\n}\n";
    assert_eq!(outcomes(&run(&s, FULL).1, "caller", "wrap"), ["inlined"]);
    let s = s.replace(
        " v10000 = iconst.i64 0",
        " v9000 = iconst.i64 0\n v10000 = iconst.i64 0",
    );
    let (_, sites) = run(&s, FULL);
    assert_eq!(
        outcomes(&sites, "caller", "wrap"),
        ["recursive_wrapper_caller_size"]
    );
    assert_eq!(outcomes(&sites, "caller", "tiny"), ["inlined"]);
    let s = wrappers()
        + &leaf("tiny", 1, 0, "alwaysinline local")
        + &caller("middle", "tiny", 1).replace("func @middle()", "func @middle() local")
        + &caller("caller", "middle", 1);
    let original = keel_ir::parse(&s).unwrap().functions[0].to_string();
    let (m, sites) = run(&s, FULL);
    assert_eq!(m.functions[0].to_string(), original); // SCC body stays frozen
    assert_eq!(calls(&m, "middle", "tiny"), 0);
    // The priority phase inlines tiny into middle before templates freeze, so
    // the clone in caller has no call to tiny, and no new site was reported.
    assert_eq!(calls(&m, "caller", "tiny"), 0);
    assert!(outcomes(&sites, "caller", "tiny").is_empty());
}

#[test]
fn ordinary_and_reserve_preserve_layout_order_and_exact_clone_bytes() {
    // No recursive or exported functions: the policies have the same domain.
    // Ordinary inlining must retain the pre-priority layout result.
    let mut s = leaf("ordinary", 64, 0, "local") + &leaf("tiny", 8, 0, "alwaysinline local");
    s += &caller("caller", "ordinary", 32).replace(" return\n}", " call @tiny()\n return\n}");
    let (ordinary, sites) = run(&s, "inline");
    assert_eq!(
        outcomes(&sites, "caller", "tiny"),
        ["caller_instruction_growth_budget"]
    );
    assert_eq!(sites[0].callee, "ordinary");
    // The expected byte result is an ordinary pass with the tiny call's
    // declaration noinline: same original clones, same IDs, same final call.
    let mut expected = keel_ir::parse(&s).unwrap();
    let f = expected
        .functions
        .iter_mut()
        .find(|f| f.name == "caller")
        .unwrap();
    let r = f.callees().find(|(_, d)| d.name == "tiny").unwrap().0;
    f.callee_mut(r).attrs.noinline = true;
    f.callee_mut(r).attrs.alwaysinline = false;
    unchecked(&mut expected, "inline");
    let f = expected
        .functions
        .iter_mut()
        .find(|f| f.name == "caller")
        .unwrap();
    f.callee_mut(r).attrs.noinline = false;
    f.callee_mut(r).attrs.alwaysinline = true;
    keel_ir::verify_module(&expected).unwrap();
    assert_eq!(ordinary.to_string(), expected.to_string());
    let (full, _) = run(&s, FULL);
    assert_eq!(calls(&full, "caller", "tiny"), 0);
    assert_eq!(calls(&full, "caller", "ordinary"), 1);
    assert!(!keel_opt::OptimizationTier::Fast.pipeline().contains(&FULL));
}

#[test]
fn snapshots_count_toward_priority_cost_and_stack_boundaries() {
    for (padding, stack, priority) in [(26, 56, true), (27, 56, false), (26, 57, false)] {
        // load + store + return + padding + three snapshot instructions.
        // One padding instruction roots the local stack object.
        let mut tiny = format!(
            "func @tiny(ptr byval {{i64}}) inlinehint local {{\n ss0 = stack_slot {stack}, align 1\nblock0(v0: ptr):\n v1 = load.i64 v0\n store v1, v0\n v100 = stack_addr ss0\n"
        );
        for i in 0..padding - 1 {
            tiny += &format!(" v{} = iconst.i64 0\n", i + 2);
        }
        tiny += " return\n}\n";
        let s = wrappers()
            + &tiny
            + "func @caller(ptr) {\nblock0(v0: ptr):\n"
            + &" call @wrap()\n".repeat(32)
            + " call @tiny(v0)\n return\n}\n";
        let (m, sites) = run(&s, RESERVE);
        assert_eq!(
            outcomes(&sites, "caller", "tiny"),
            [if priority {
                "inlined"
            } else {
                "caller_instruction_growth_budget"
            }]
        );
        if priority {
            let f = m.functions.iter().find(|f| f.name == "caller").unwrap();
            assert_eq!(
                f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>(),
                64
            );
            assert_eq!(f.num_stack_slots(), 2); // snapshot and cloned local are distinct
        }
    }
}

#[test]
fn debug_clone_scopes_and_site_reports_are_deterministic_after_reordering() {
    use keel_ir::{DebugScope, DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, SourceLoc};
    let s = leaf("ordinary", 1, 0, "local")
        + &leaf("tiny", 2, 8, "alwaysinline local")
        + "func @caller() {\nblock0:\n call @ordinary()\n call @tiny()\n call @tiny()\n return\n}\n";
    let mut input = keel_ir::parse(&s).unwrap();
    let file = input.add_file("priority.c");
    input.debug_types.push(DebugTypeData::Base {
        name: "long".into(),
        encoding: Encoding::Signed,
        size: 8,
    });
    let tiny = &mut input.functions[1];
    tiny.debug_scopes.push(None);
    let slot = tiny.stack_slots().next().unwrap().0;
    tiny.debug_vars.push(DebugVar {
        name: "local".into(),
        ty: DebugType::new(0),
        slot,
        indirect: false,
        param: false,
        scope: Some(DebugScope::new(0)),
    });
    let instructions: Vec<_> = tiny.layout_insts().map(|(_, i)| i).collect();
    for inst in instructions {
        tiny.set_inst_loc(
            inst,
            Some(SourceLoc {
                file,
                line: 7,
                col: 1,
                scope: Some(DebugScope::new(0)),
            }),
        );
    }
    let caller = &mut input.functions[2];
    caller.debug_scopes.push(None);
    let instructions: Vec<_> = caller.layout_insts().map(|(_, i)| i).collect();
    for inst in instructions {
        caller.set_inst_loc(
            inst,
            Some(SourceLoc {
                file,
                line: 20 + inst.index() as u32,
                col: 1,
                scope: Some(DebugScope::new(0)),
            }),
        );
    }
    let mut expected = None;
    for _ in 0..8 {
        let mut m = input.clone();
        let sites = optimize(&mut m, FULL);
        assert_eq!(sites.iter().map(|s| s.site).collect::<Vec<_>>(), [1, 2, 0]);
        let f = &m.functions[2];
        assert_eq!(f.debug_vars.len(), 2);
        assert_ne!(f.debug_vars[0].slot, f.debug_vars[1].slot);
        assert_ne!(f.debug_vars[0].scope, f.debug_vars[1].scope);
        for var in &f.debug_vars {
            let scope = var.scope.unwrap();
            let root = f.debug_scopes[scope.index()].unwrap();
            assert_eq!(f.debug_scopes[root.index()], Some(DebugScope::new(0)));
            assert!(f.layout_insts().any(|(_, i)| {
                f.inst_loc(i)
                    .is_some_and(|loc| loc.line == 7 && loc.scope == Some(scope))
            }));
        }
        let normalized = (m.to_string(), format!("{sites:?}"));
        if let Some(expected) = &expected {
            assert_eq!(&normalized, expected);
        } else {
            expected = Some(normalized);
        }
    }
}

#[test]
fn priority_cfg_domain_is_small_and_acyclic() {
    for (blocks, cyclic, expected) in [
        (4, false, "inlined"),
        (5, false, "caller_instruction_growth_budget"),
        (2, true, "caller_instruction_growth_budget"),
    ] {
        let mut tiny = "func @tiny() inlinehint local {\n".to_owned();
        // Two constants per block keep the leaf larger than its call.
        for i in 0..blocks {
            tiny += &format!(
                "block{i}:\n v{} = iconst.i64 0\n v{} = iconst.i64 1\n",
                2 * i,
                2 * i + 1
            );
            if i + 1 < blocks {
                tiny += &format!(" jump block{}\n", i + 1);
            } else if cyclic {
                tiny += " jump block1\n";
            } else {
                tiny += " return\n";
            }
        }
        tiny += "}\n";
        let s = wrappers()
            + &tiny
            + &caller("caller", "wrap", 32).replace(" return\n}", " call @tiny()\n return\n}");
        assert_eq!(outcomes(&run(&s, RESERVE).1, "caller", "tiny"), [expected]);
    }
}

// Leaves of 40 instructions are too large for the priority class.
fn chain(wrapper_attrs: &str) -> String {
    let mut s = String::new();
    for name in ["get", "copied"] {
        s += &format!("func @{name}() alwaysinline local {{\nblock0:\n");
        for i in 0..39 {
            s += &format!(" v{i} = iconst.i64 {i}\n");
        }
        s += " return\n}\n";
    }
    s += &format!(
        "func @entry() {wrapper_attrs} local {{\nblock0:\n call @get()\n call @copied()\n return\n}}\n"
    );
    s + "func @caller() {\nblock0:\n call @entry()\n return\n}\n"
}

#[test]
fn alwaysinline_callees_are_inlined_before_their_callers() {
    // Layout puts the callers last; the required phase still visits entry
    // before caller, so caller clones entry with both leaves already inlined.
    for (attrs, pass) in [
        ("alwaysinline", FULL),
        ("", FULL),
        ("alwaysinline", RESERVE),
        ("", RESERVE),
    ] {
        let source = chain(attrs);
        let (m, sites) = run(&source, pass);
        assert_eq!(
            outcomes(&sites, "caller", "entry"),
            ["inlined"],
            "{attrs} {pass}"
        );
        for callee in ["get", "copied"] {
            assert_eq!(outcomes(&sites, "entry", callee), ["inlined"]);
            assert_eq!(calls(&m, "caller", callee), 0, "{attrs} {pass}");
        }
        let mut functions: Vec<_> = source.split_inclusive("}\n").collect();
        functions.reverse();
        let reversed = functions.concat();
        let (m, _) = run(&reversed, pass);
        assert_eq!(
            calls(&m, "caller", "get") + calls(&m, "caller", "copied"),
            0
        );
    }
}

#[test]
fn a_site_no_larger_than_its_call_ignores_exhausted_growth_budgets() {
    // 32 wrappers spend the caller's whole growth budget; the accessor costs
    // 5 instructions, no more than a zero-argument call.
    let s = wrappers()
        + &leaf("accessor", 5, 0, "local")
        + &caller("caller", "wrap", 32).replace(" return\n}", " call @accessor()\n return\n}");
    let (_, sites) = run(&s, FULL);
    assert_eq!(outcomes(&sites, "caller", "accessor"), ["inlined"]);
    assert_eq!(
        outcomes(&sites, "caller", "wrap")
            .iter()
            .filter(|&&s| s == "inlined")
            .count(),
        32
    );
}

#[test]
fn small_memory_intrinsics_stay_in_the_priority_class() {
    // A 16-byte memmove is charged 8 instructions, like a 16-byte by-value
    // snapshot, against the class's 32; a 33-byte one stays a call.
    for (bytes, padding, expected) in [
        (16, 21, "inlined"),
        (16, 22, "caller_instruction_growth_budget"),
        (33, 10, "caller_instruction_growth_budget"),
    ] {
        let mut tiny = format!(
            "func @tiny(ptr, ptr) inlinehint local {{\nblock0(v0: ptr, v1: ptr):\n v2 = iconst.i64 {bytes}\n intrinsic memmove(v0, v1, v2)\n"
        );
        for i in 0..padding {
            tiny += &format!(" v{} = iconst.i64 0\n", i + 3);
        }
        tiny += " return\n}\n";
        let s = wrappers()
            + &tiny
            + "func @caller(ptr, ptr) {\nblock0(v0: ptr, v1: ptr):\n"
            + &" call @wrap()\n".repeat(32)
            + " call @tiny(v0, v1)\n return\n}\n";
        assert_eq!(
            outcomes(&run(&s, RESERVE).1, "caller", "tiny"),
            [expected],
            "{bytes} {padding}"
        );
    }
}
