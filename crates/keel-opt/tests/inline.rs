use keel_ir::{InstData, Module};
fn run(s: &str, passes: &[&str]) -> Module {
    let mut m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, passes).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn calls(m: &Module, name: &str) -> usize {
    m.functions
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .layout_insts()
        .filter(|(_, i)| {
            matches!(
                m.functions
                    .iter()
                    .find(|f| f.name == name)
                    .unwrap()
                    .inst_data(*i),
                InstData::Call { .. }
            )
        })
        .count()
}
#[test]
fn branches_multiple_results_and_repeated_calls() {
    let m = run(
        "func @pick(i8, i64, i64) -> (i64, i64) local {
block0(v0: i8, v1: i64, v2: i64):
 brif v0, block1, block2
block1:
 return v1, v2
block2:
 return v2, v1
}
func @test(i8, i64, i64) -> i64 {
block0(v0: i8, v1: i64, v2: i64):
 v3, v4 = call @pick(v0, v1, v2)
 v5, v6 = call @pick(v0, v3, v4)
 v7 = iadd v5, v6
 return v7
}",
        &["inline", "simplify-cfg", "dce"],
    );
    assert_eq!(calls(&m, "test"), 0);
}
#[test]
fn snapshots_and_callee_slots_are_distinct_per_site() {
    let m = run(
        "func @mut(ptr byval {i64}) -> i64 local {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = iconst.i64 4
 store v1, v0
 v2 = stack_addr ss0
 store v1, v2
 v3 = load.i64 v0
 return v3
}
func @f(ptr) -> i64 {
block0(v0: ptr):
 v1 = call @mut(v0)
 v2 = call @mut(v0)
 v3 = iadd v1, v2
 return v3
}",
        &["inline"],
    );
    let f = &m.functions[1];
    assert_eq!(f.num_stack_slots(), 4);
    assert_eq!(calls(&m, "f"), 0);
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count(),
        4
    );
}
#[test]
fn rejects_recursion_variadics_dynamic_frames_and_public_definitions() {
    for (signature, body, local) in [
        (
            "(i64) -> i64",
            "v1 = call @helper(v0)\n return v1",
            " local",
        ),
        ("(i64, ...) -> i64", "return v0", " local"),
        (
            "(i64) -> i64",
            "v1 = intrinsic stack_alloc(v0)\n return v0",
            " local",
        ),
        (
            "(i64) -> i64",
            "v1 = intrinsic frame_address()\n return v0",
            " local",
        ),
        ("(i64) -> i64", "return v0", ""),
    ] {
        let s = format!(
            "func @helper{signature}{local} {{\nblock0(v0: i64):\n {body}\n}}\nfunc @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}}"
        );
        let m = run(&s, &["inline"]);
        assert_eq!(calls(&m, "f"), 1, "{s}");
    }
    let m = run(
        "func @a(i64) -> i64 local {\nblock0(v0: i64):\n v1 = call @b(v0)\n return v1\n }
func @b(i64) -> i64 local {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n }
func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n}",
        &["inline"],
    );
    assert_eq!(calls(&m, "f"), 1);
}
#[test]
fn handles_out_of_layout_order_and_loop_block_parameters() {
    let m = run(
        "func @sum(i64) -> i64 local {
block0(v0: i64):
 v1 = iconst.i64 0
 jump block2(v1, v1)
block1(v2: i64):
 return v2
block2(v3: i64, v4: i64):
 v5 = icmp.ult v3, v0
 brif v5, block3, block1(v4)
block3:
 v6 = iconst.i64 1
 v7 = iadd v3, v6
 v8 = iadd v4, v3
 jump block2(v7, v8)
}
func @f(i64) -> i64 {
block0(v0: i64):
 v1 = call @sum(v0)
 return v1
}",
        &["inline"],
    );
    assert_eq!(calls(&m, "f"), 0);
}
#[test]
fn deterministic_growth_budget_and_frozen_templates() {
    let mut s = String::from(
        "func @tiny(i64) -> i64 local {\nblock0(v0: i64):\n v1 = iconst.i64 1\n v2 = iadd v0, v1\n\n return v2\n}\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n",
    );
    for n in 1..=140 {
        s.push_str(&format!("v{n} = call @tiny(v{})\n", n - 1));
    }
    s.push_str("return v140\n }");
    let m = run(&s, &["inline"]);
    assert_eq!(calls(&m, "f"), 12);
    let m = run(
        "func @a(i64) -> i64 local {\nblock0(v0: i64):\n\n return v0\n}
func @b(i64) -> i64 local {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n}
func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @b(v0)\n return v1\n}",
        &["inline"],
    );
    assert_eq!(
        calls(&m, "f"),
        1,
        "Only original callsites expand in one invocation"
    );
}
#[test]
fn full_only_and_explicit_ablation() {
    let source = "func @id(i64) -> i64 local {\nblock0(v0: i64):\n\n return v0\n}\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @id(v0)\n return v1\n}";
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Basic,
    ] {
        assert_eq!(calls(&run(source, tier.pipeline()), "f"), 1);
    }
    assert_eq!(calls(&run(source, keel_opt::DEFAULT_PIPELINE), "f"), 0);
    let off: Vec<_> = keel_opt::DEFAULT_PIPELINE
        .iter()
        .copied()
        .filter(|p| *p != "inline")
        .collect();
    assert_eq!(calls(&run(source, &off), "f"), 1);
}

#[test]
fn remaps_debug_scopes_slots_and_callee_tables() {
    use keel_ir::{
        DebugScope, DebugTypeData, DebugVar, Encoding, EntityRef, Function, InsertPoint, Signature,
        SourceLoc, Type,
    };
    let mut m = Module::default();
    let file = m.add_file("inline.c");
    m.debug_types.push(DebugTypeData::Base {
        name: "int".into(),
        encoding: Encoding::Signed,
        size: 4,
    });
    let mut helper = Function::new("helper", Signature::new(&[Type::I32], &[Type::I32]));
    helper.local = true;
    let block = helper.create_block();
    let arg = helper.append_block_param(block, Type::I32);
    let slot = helper.create_stack_slot(4, 4);
    helper.debug_scopes.push(None);
    helper.debug_vars.push(DebugVar {
        name: "x".into(),
        ty: keel_ir::DebugType::new(0),
        slot,
        indirect: false,
        param: true,
        scope: Some(DebugScope::new(0)),
    });
    let address = helper.ins(InsertPoint::End(block)).stack_addr(slot);
    helper.ins(InsertPoint::End(block)).store(arg, address);
    let result = helper.ins(InsertPoint::End(block)).load(Type::I32, address);
    let ret = helper.append_inst(block, InstData::Return { args: vec![result] });
    helper.set_inst_loc(
        ret,
        Some(SourceLoc {
            file,
            line: 4,
            col: 2,
            scope: Some(DebugScope::new(0)),
        }),
    );
    let mut caller = Function::new("caller", Signature::new(&[], &[Type::I32]));
    let block = caller.create_block();
    let decl = caller.import_function("helper", helper.sig.clone());
    let value = caller.ins(InsertPoint::End(block)).iconst(Type::I32, 12);
    let call = caller.append_inst(
        block,
        InstData::Call {
            func: decl,
            args: vec![value],
        },
    );
    let value = caller.first_result(call);
    caller.append_inst(block, InstData::Return { args: vec![value] });
    m.functions = vec![helper, caller];
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["inline"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[1];
    assert_eq!(f.debug_scopes.len(), 2);
    assert_eq!(f.debug_vars.len(), 1);
    assert!(!f.debug_vars[0].param);
    assert_eq!(f.debug_vars[0].scope, Some(DebugScope::new(1)));
    let text = m.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
}

fn large_helper(insts: usize, attrs: &str, callers: usize, sites: usize) -> String {
    let mut s = format!("func @helper(i64) -> i64 {attrs} local {{\nblock0(v0: i64):\n");
    for n in 1..insts {
        s.push_str(&format!("v{n} = iadd v{}, v0\n", n - 1));
    }
    s.push_str(&format!("return v{}\n}}\n", insts - 1));
    for caller in 0..callers {
        s.push_str(&format!(
            "func @f{caller}(i64) -> i64 {{\nblock0(v0: i64):\n"
        ));
        for n in 1..=sites {
            s.push_str(&format!("v{n} = call @helper(v{})\n", n - 1));
        }
        s.push_str(&format!("return v{sites}\n}}\n"));
    }
    s
}

#[test]
fn inline_policies_at_instruction_thresholds() {
    for (insts, attrs, remaining) in [
        (64, "", 0),
        (65, "", 1),
        (128, "inlinehint", 0),
        (129, "inlinehint", 1),
        (129, "alwaysinline", 0),
        (1, "noinline", 1),
        (1, "inlinehint noinline", 1),
        (2048, "alwaysinline", 0),
        (2049, "alwaysinline", 1),
    ] {
        let m = run(&large_helper(insts, attrs, 1, 1), &["inline"]);
        assert_eq!(calls(&m, "f0"), remaining, "{insts} {attrs}");
    }
}

#[test]
fn explicit_requests_report_unsupported_recursion_and_external_sites() {
    for (helper, reason) in [
        (
            "decl @helper(i64) -> i64 alwaysinline\n",
            "no private definition",
        ),
        (
            "func @helper(i64) -> i64 alwaysinline {\nblock0(v0: i64):\n return v0\n}\n",
            "unsupported callee or hard stack bound",
        ),
        (
            "func @helper(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}\n",
            "recursive callee",
        ),
        (
            "func @helper(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n v1 = intrinsic stack_alloc(v0)\n return v0\n}\n",
            "unsupported callee or hard stack bound",
        ),
    ] {
        let source = format!(
            "{helper}func @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}}\n"
        );
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let report = keel_opt::inline_with_report(&mut m);
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(calls(&m, "f"), 1);
        assert!(
            report
                .skipped
                .iter()
                .any(|s| s.caller == "f" && s.reason == reason),
            "{report:?}"
        );
    }
    let mut m = keel_ir::parse("func @helper(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n return v0\n}\nfunc @f(i64) -> i64 returns_twice {\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}\n").unwrap();
    let report = keel_opt::inline_with_report(&mut m);
    assert_eq!(report.skipped[0].reason, "unsupported caller");
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn alwaysinline_obeys_call_and_module_growth_safeguards() {
    let m = run(&large_helper(1, "alwaysinline", 1, 140), &["inline"]);
    assert_eq!(calls(&m, "f0"), 12);
    let mut m = keel_ir::parse(&large_helper(254, "alwaysinline", 9, 8)).unwrap();
    let report = keel_opt::inline_with_report(&mut m);
    keel_ir::verify_module(&m).unwrap();
    for n in 0..8 {
        assert_eq!(calls(&m, &format!("f{n}")), 0);
    }
    assert_eq!(calls(&m, "f8"), 8);
    assert_eq!(report.skipped.len(), 8);
    assert!(
        report
            .skipped
            .iter()
            .all(|s| s.reason == "hard growth safeguard")
    );
}

#[test]
fn alwaysinline_still_excludes_mutual_recursion_and_tail_sites() {
    let mut m = keel_ir::parse("func @a(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n v1 = call @b(v0)\n return v1\n}\nfunc @b(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n}\n").unwrap();
    let report = keel_opt::inline_with_report(&mut m);
    assert!(!report.changed);
    assert_eq!(report.skipped.len(), 2);
    assert!(
        report
            .skipped
            .iter()
            .all(|s| s.reason == "recursive callee")
    );
    let mut m = keel_ir::parse("func @h(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n return v0\n}\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n return_call @h(v0)\n}\n").unwrap();
    keel_ir::verify_module(&m).unwrap();
    let report = keel_opt::inline_with_report(&mut m);
    assert_eq!(report.skipped[0].reason, "unsupported call instruction");
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn block_stack_thresholds_and_hard_stack_budget() {
    for (blocks, attrs, remaining) in [
        (8, "", 0),
        (9, "", 1),
        (16, "inlinehint", 0),
        (17, "inlinehint", 1),
        (17, "alwaysinline", 0),
    ] {
        let mut source = format!("func @h() {attrs} local {{\n");
        for n in 0..blocks {
            source.push_str(&format!("block{n}:\n"));
            if n + 1 < blocks {
                source.push_str(&format!("jump block{}\n", n + 1));
            } else {
                source.push_str("return\n");
            }
        }
        source.push_str("}\nfunc @f() {\nblock0:\n call @h()\n return\n}\n");
        assert_eq!(
            calls(&run(&source, &["inline"]), "f"),
            remaining,
            "{blocks} {attrs}"
        );
    }
    for (size, attrs, remaining) in [
        (512, "", 0),
        (513, "", 2),
        (1024, "inlinehint", 0),
        (1025, "inlinehint", 2),
        (1025, "alwaysinline", 0),
        (32768, "alwaysinline", 0),
        (32769, "alwaysinline", 1),
        (65537, "alwaysinline", 2),
    ] {
        let source = format!(
            "func @h() {attrs} local {{\n ss0 = stack_slot {size}, align 1\nblock0:\n return\n}}\nfunc @f() {{\nblock0:\n call @h()\n call @h()\n return\n}}\n"
        );
        assert_eq!(
            calls(&run(&source, &["inline"]), "f"),
            remaining,
            "{size} {attrs}"
        );
    }
}

#[test]
fn cloning_keeps_external_callee_promises_and_indirect_calls() {
    let m = run(
        "decl @external() nounwind noinline\nfunc @h(ptr) alwaysinline local {\nblock0(v0: ptr):\n call @external()\n call_indirect v0() : ()\n return\n}\nfunc @f(ptr) {\nblock0(v0: ptr):\n call @h(v0)\n return\n}\n",
        &["inline"],
    );
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    let external = f.callees().find(|(_, d)| d.name == "external").unwrap().1;
    assert!(external.attrs.nounwind && external.attrs.noinline);
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::CallIndirect { .. }))
    );
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
}

fn detailed_report(source: &str) -> keel_opt::OptimizationReport {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let mut plain = module.clone();
    keel_opt::run_passes(&mut plain, &["inline"]).unwrap();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut module,
        &["inline"],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    assert_eq!(plain.to_string(), module.to_string());
    keel_ir::verify_module(&module).unwrap();
    report
}

#[test]
fn byval_copy_cost_admits_sites_up_to_unchanged_budgets() {
    for (size, site_count, inlined, reason) in [
        (16, 140, 128, "caller_call_count_budget"),
        (128, 30, 25, "caller_instruction_growth_budget"),
    ] {
        let fields = vec!["i8"; size].join(", ");
        let mut source = format!(
            "func @h(ptr byval {{{fields}}}) local {{\nblock0(v0: ptr):\n return\n}}\nfunc @f(ptr) {{\nblock0(v0: ptr):\n"
        );
        for _ in 0..site_count {
            source.push_str(" call @h(v0)\n");
        }
        source.push_str(" return\n}\n");
        // Also verifies that observation leaves the resulting IR identical.
        let report = detailed_report(&source);
        let sites = &report.invocations[0].inline_sites;
        assert_eq!(sites.len(), site_count);
        assert!(sites[..inlined].iter().all(|s| s.outcome == "inlined"));
        assert!(sites[inlined..].iter().all(|s| s.outcome == reason));
    }
}

// This oracle asks whether a nonempty path returns to its starting vertex.
// It deliberately uses transitive closure rather than SCC traversal.
fn graph_recursion(edges: &[Vec<usize>]) -> Vec<bool> {
    let n = edges.len();
    let mut reach = vec![vec![false; n]; n];
    for (from, tos) in edges.iter().enumerate() {
        for &to in tos {
            reach[from][to] = true;
        }
    }
    for via in 0..n {
        for from in 0..n {
            for to in 0..n {
                reach[from][to] |= reach[from][via] && reach[via][to];
            }
        }
    }
    (0..n).map(|i| reach[i][i]).collect()
}

fn graph_source(edges: &[Vec<usize>], attrs: &[&str]) -> String {
    let mut source = String::new();
    for (from, tos) in edges.iter().enumerate() {
        source.push_str(&format!("func @g{from}() {} {{\nblock0:\n", attrs[from]));
        for to in tos {
            source.push_str(&format!(" call @g{to}()\n"));
        }
        source.push_str(" return\n}\n");
    }
    source.push_str("func @probe() {\nblock0:\n");
    for to in 0..edges.len() {
        source.push_str(&format!(" call @g{to}()\n"));
    }
    source.push_str(" return\n}\n");
    source
}

fn check_graph(edges: &[Vec<usize>], attrs: &[&str]) {
    let report = detailed_report(&graph_source(edges, attrs));
    let recursive = graph_recursion(edges);
    let sites: Vec<_> = report.invocations[0]
        .inline_sites
        .iter()
        .filter(|s| s.caller == "probe")
        .collect();
    assert_eq!(sites.len(), edges.len());
    for (i, site) in sites.iter().enumerate() {
        let expected = if attrs[i].contains("noinline") {
            "noinline"
        } else if recursive[i] {
            "recursive_callee"
        } else if !attrs[i].contains("local") {
            "public_definition"
        } else {
            "inlined"
        };
        assert_eq!(site.callee, format!("g{i}"));
        assert_eq!(site.outcome, expected, "{edges:?}, {attrs:?}");
    }
}

#[test]
fn recursion_matches_independent_reachability_on_generated_graphs() {
    // Exhaust all directed graphs on three vertices (including self edges).
    for bits in 0..1u32 << 9 {
        let edges: Vec<Vec<_>> = (0..3)
            .map(|from| {
                (0..3)
                    .filter(|to| bits & (1 << (from * 3 + to)) != 0)
                    .collect()
            })
            .collect();
        check_graph(&edges, &["alwaysinline local"; 3]);
    }
    // Larger deterministic graphs exercise shared suffixes and disconnected SCCs.
    let mut seed = 0x4b65_656cu32;
    for _ in 0..48 {
        let edges: Vec<Vec<_>> = (0..9)
            .map(|_| {
                (0..9)
                    .filter(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        seed >> 28 == 0
                    })
                    .collect()
            })
            .collect();
        check_graph(&edges, &["local"; 9]);
    }
}

#[test]
fn recursion_includes_ineligible_intermediates_but_not_incoming_callers() {
    // Diamond/shared suffix, incoming acyclic caller, two-node cycle, self
    // cycle and disconnected leaf. Duplicate edges must not affect membership.
    let edges = vec![
        vec![1, 2, 1],
        vec![3],
        vec![3],
        vec![4],
        vec![3],
        vec![5],
        vec![],
    ];
    for intermediate in ["noinline local", "", "local"] {
        check_graph(
            &edges,
            &[
                "local",
                "local",
                "local",
                "alwaysinline local",
                intermediate,
                "local",
                "local",
            ],
        );
    }
    check_graph(&[vec![1, 2], vec![3], vec![3], vec![]], &["local"; 4]);
}

#[test]
fn recursion_membership_includes_tail_and_invoke_edges() {
    for (body, attrs, extra) in [
        (" return_call @a()\n", "local", ""),
        (
            " invoke @a(), block1, block2\nblock1:\n return\nblock2:\n v0, v1 = landingpad cleanup\n trap\n",
            "local personality @personality",
            "decl @personality()\n",
        ),
    ] {
        let source = format!(
            "{extra}func @a() alwaysinline local {{\nblock0:\n call @b()\n return\n}}\nfunc @b() {attrs} {{\nblock0:\n{body}}}\nfunc @probe() {{\nblock0:\n call @a()\n return\n}}"
        );
        let report = detailed_report(&source);
        let site = report.invocations[0]
            .inline_sites
            .iter()
            .find(|s| s.caller == "probe")
            .unwrap();
        assert_eq!(site.outcome, "recursive_callee");
    }
}

#[test]
fn long_acyclic_chain_uses_frozen_templates() {
    let n = 2048;
    let edges: Vec<Vec<_>> = (0..n)
        .map(|i| if i + 1 < n { vec![i + 1] } else { vec![] })
        .collect();
    // Public definitions prevent cloning, while still requiring recursion
    // analysis. This keeps the test focused on traversing a deep call graph.
    let report = detailed_report(&graph_source(&edges, &vec![""; n]));
    assert!(
        report.invocations[0]
            .inline_sites
            .iter()
            .all(|s| s.outcome == "public_definition")
    );
}

#[test]
fn detailed_reports_identify_each_budget_without_changing_decisions() {
    for (source, reason, skipped) in [
        (large_helper(1, "", 1, 140), "caller_call_count_budget", 12),
        (large_helper(1, "alwaysinline", 1, 140), "caller_call_count_budget", 12),
        (large_helper(64, "", 1, 33), "caller_instruction_growth_budget", 1),
        (large_helper(2049, "alwaysinline", 1, 1), "caller_instruction_growth_budget", 1),
        (large_helper(254, "alwaysinline", 9, 8), "module_instruction_growth_budget", 8),
        ("func @h() alwaysinline local {\n ss0 = stack_slot 32769, align 1\nblock0:\n return\n}\nfunc @f() {\nblock0:\n call @h()\n call @h()\n return\n}".into(), "caller_stack_growth_budget", 1),
    ] {
        let report = detailed_report(&source);
        let sites = &report.invocations[0].inline_sites;
        assert_eq!(sites.iter().filter(|s| s.outcome == reason).count(), skipped, "{reason}: {sites:?}");
        assert!(sites.iter().all(|s| s.outcome == "inlined" || s.outcome == reason));
    }
}

#[test]
fn detailed_profitability_and_legality_rejections_are_specific() {
    for (source, expected) in [
        (large_helper(65, "", 1, 1), "profitability_instructions"),
        (large_helper(129, "inlinehint", 1, 1), "profitability_instructions"),
        ("func @helper() local {\n ss0 = stack_slot 513, align 1\nblock0:\n return\n}\nfunc @f() {\nblock0:\n call @helper()\n return\n}".into(), "profitability_stack_declaration_bytes"),
        ("func @helper() alwaysinline local {\n ss0 = stack_slot 65537, align 1\nblock0:\n return\n}\nfunc @f() {\nblock0:\n call @helper()\n return\n}".into(), "callee_hard_stack_declaration_bytes"),
        ("func @helper(i64, ...) -> i64 local {\nblock0(v0: i64):\n return v0\n}\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}".into(), "variadic_callee"),
    ] {
        let report = detailed_report(&source);
        assert_eq!(report.invocations[0].inline_sites.last().unwrap().outcome, expected);
    }
    let mut source = String::from("func @helper() local {\n");
    for n in 0..9 {
        source.push_str(&format!("block{n}:\n"));
        if n < 8 {
            source.push_str(&format!("jump block{}\n", n + 1));
        } else {
            source.push_str("return\n");
        }
    }
    source.push_str("}\nfunc @f() {\nblock0:\n call @helper()\n return\n}");
    assert_eq!(
        detailed_report(&source).invocations[0].inline_sites[0].outcome,
        "profitability_blocks"
    );
}

// Growth is the increase in layout instructions, before any cleanup pass.
// The entry jump replaces the call; return-to-jump rewrites are one-for-one.
#[test]
fn growth_budget_counts_emitted_instructions_at_the_boundary() {
    let source = large_helper(64, "", 1, 33);
    let before = keel_ir::parse(&source).unwrap();
    let m = run(&source, &["inline"]);
    assert_eq!(calls(&m, "f0"), 1);
    assert_eq!(
        m.functions[1].layout_insts().count() - before.functions[1].layout_insts().count(),
        2048,
    );
    // The same limit applies even to an explicit alwaysinline request.
    let m = run(&large_helper(2048, "alwaysinline", 1, 2), &["inline"]);
    assert_eq!(calls(&m, "f0"), 1);
    let m = run(&large_helper(2049, "alwaysinline", 1, 1), &["inline"]);
    assert_eq!(calls(&m, "f0"), 1);
}

#[test]
fn module_growth_budget_remains_bounded_before_cleanup() {
    let source = large_helper(256, "alwaysinline", 9, 8);
    let before = keel_ir::parse(&source).unwrap();
    let before_count: usize = before.functions.iter().map(|f| f.layout_insts().count()).sum();
    let m = run(&source, &["inline"]);
    let after_count: usize = m.functions.iter().map(|f| f.layout_insts().count()).sum();
    assert_eq!(after_count - before_count, 16384);
    for n in 0..8 {
        assert_eq!(calls(&m, &format!("f{n}")), 0);
    }
    assert_eq!(calls(&m, "f8"), 8);
}

#[test]
fn direct_api_aliases_and_restrict_contracts_stay_local_to_the_callee() {
    let mut m = keel_ir::parse("func @h(i64, i64) -> i64 local {\nblock0(v0: i64, v1: i64):\n v2 = iadd v0, v1\n return v2\n}\nfunc @f(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = call @h(v0, v1)\n return v2\n}").unwrap();
    // Mutate the Function API directly: printing/reparsing would erase aliases.
    let h = &mut m.functions[0];
    let params = h.block_params(h.entry_block().unwrap()).to_vec();
    h.replace_all_uses(params[0], params[1]);
    let add = h.layout_insts().next().unwrap().1;
    h.replace_all_uses(h.first_result(add), params[1]);
    h.remove_inst(add);
    let f = &mut m.functions[1];
    let params = f.block_params(f.entry_block().unwrap()).to_vec();
    f.replace_all_uses(params[0], params[1]);
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_opt::inline_with_report(&mut m).changed);
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(calls(&m, "f"), 0);
    keel_opt::run_passes(&mut m, &["simplify-cfg", "dce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[1];
    let ret = f.layout_insts().last().unwrap().1;
    let InstData::Return { args } = f.inst_data(ret) else {
        panic!("missing return")
    };
    assert_eq!(f.resolve(args[0]), f.resolve(params[1]));

    let mut m = keel_ir::parse("func @h(ptr, ptr) local {\nblock0(v0: ptr, v1: ptr):\n v2 = load.i64 v0\n store v2, v1\n return\n}\nfunc @f(ptr, ptr) {\nblock0(v0: ptr, v1: ptr):\n call @h(v0, v1)\n return\n}").unwrap();
    m.functions[0].restrict_params = vec![0];
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_opt::inline_with_report(&mut m).changed);
    keel_ir::verify_module(&m).unwrap();
    assert!(m.functions[1].restrict_params.is_empty());
    assert_eq!(m.functions[0].restrict_params, vec![0]);
}

#[test]
fn quickjs_dup_value_reducer_uses_the_same_bounded_growth() {
    let mut source = include_str!("fixtures/inline-js-dup-value.keel").to_owned();
    source.push_str("func @f(ptr, ptr, ptr) {\nblock0(v0: ptr, v1: ptr, v2: ptr):\n");
    for _ in 0..80 {
        source.push_str(" call @JS_DupValue(v0, v1, v2)\n");
    }
    source.push_str(" return\n}\n");
    let before = keel_ir::parse(&source).unwrap();
    let helper_insts = before.functions[0].layout_insts().count();
    assert_eq!(helper_insts, 20);
    let m = run(&source, &["inline"]);
    // A 16-byte byval snapshot emits 8 instructions. At 28 instructions per
    // expansion, 73 fit; the old 30-instruction estimate admitted only 68.
    assert_eq!(calls(&m, "f"), 7);
    assert_eq!(
        m.functions[1].layout_insts().count() - before.functions[1].layout_insts().count(),
        2044,
    );
    assert_eq!(m.functions[1].num_stack_slots(), 146);
    let report = detailed_report(&source);
    let sites = &report.invocations[0].inline_sites;
    assert!(sites[..73].iter().all(|s| s.outcome == "inlined"));
    assert!(
        sites[73..]
            .iter()
            .all(|s| s.outcome == "caller_instruction_growth_budget")
    );
}
