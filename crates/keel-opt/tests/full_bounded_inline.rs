use keel_ir::{InstData, Module};

const LUA: &str = include_str!("fixtures/full-inline-index2value.keel");

fn run(m: &mut Module, pass: &str) -> keel_opt::OptimizationReport {
    keel_ir::verify_module(m).unwrap();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        m,
        &[pass],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    keel_ir::verify_module(m).unwrap();
    report
}
fn outcome(source: &str, pass: &str, caller: &str, callee: &str) -> String {
    let mut m = keel_ir::parse(source).unwrap();
    let r = run(&mut m, pass);
    r.invocations
        .iter()
        .flat_map(|p| &p.inline_sites)
        .find(|s| s.caller == caller && s.callee == callee)
        .unwrap()
        .outcome
        .to_owned()
}

#[test]
fn retained_lua_leaf_is_full_only_and_keeps_load_flags() {
    let original = keel_ir::parse(LUA).unwrap();
    assert_eq!(original.functions[0].blocks().count(), 14);
    assert_eq!(original.functions[0].layout_insts().count(), 82);
    for pass in ["inline", "inline-aggregate-reserve"] {
        assert_eq!(
            outcome(LUA, pass, "lookup", "index2value"),
            "profitability_blocks"
        );
    }
    assert_eq!(
        outcome(LUA, "inline-noninterposable", "lookup", "index2value"),
        "inlined"
    );
    let flagged = LUA.replace("load.ptr v3", "load.ptr volatile align 8 v3");
    let mut m = keel_ir::parse(&flagged).unwrap();
    run(&mut m, "inline-noninterposable");
    assert!(m.functions[1].layout_insts().any(|(_, i)| matches!(
        m.functions[1].inst_data(i), InstData::Load { flags, .. } if flags.volatile && flags.align == 8 && !flags.notrap
    )));
}

#[test]
fn readonly_allowance_does_not_bypass_binding_effect_or_cfg_limits() {
    for (source, expected) in [
        (LUA.replace(" cc(c) local", " cc(c)"), "public_definition"),
        (
            LUA.replace(" cc(c) local", " cc(c) noinline local"),
            "noinline",
        ),
        (
            LUA.replacen("    v2 =", "    store v1, v0\n    v2 =", 1),
            "profitability_blocks",
        ),
        (
            LUA.replace("    return v11", "    jump block5"),
            "profitability_blocks",
        ),
        (
            LUA.replacen(
                "    v2 =",
                "    v900 = intrinsic frame_address()\n    v2 =",
                1,
            ),
            "unsupported_callee_intrinsic",
        ),
    ] {
        assert_eq!(
            outcome(&source, "inline-noninterposable", "lookup", "index2value"),
            expected
        );
    }
    let mut many = LUA.to_owned();
    many = many.replacen(
        "    v2 =",
        &((900..915)
            .map(|i| format!("    v{i} = iconst.i64 1\n"))
            .collect::<String>()
            + "    v2 ="),
        1,
    );
    assert_eq!(
        outcome(&many, "inline-noninterposable", "lookup", "index2value"),
        "profitability_blocks"
    );
}

const QUICKJS: &str = include_str!("fixtures/full-inline-quickjs.keel");
fn direct_calls(m: &Module, caller: &str) -> Vec<(String, bool)> {
    let f = m.functions.iter().find(|f| f.name == caller).unwrap();
    f.layout_insts()
        .filter_map(|(_, i)| match f.inst_data(i) {
            InstData::Call { func, .. } => {
                let d = f.callee(*func);
                Some((d.name.clone(), d.attrs.noinline))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn retained_quickjs_wrappers_inline_one_level_over_three_rounds() {
    for name in ["js_poll_interrupts", "JS_FreeValue", "JS_Call"] {
        assert_eq!(outcome(QUICKJS, "inline", "vm", name), "recursive_callee");
        assert_eq!(
            outcome(QUICKJS, "inline-noninterposable", "vm", name),
            "inlined"
        );
    }
    let mut m = keel_ir::parse(QUICKJS).unwrap();
    let wrappers: Vec<_> = m
        .functions
        .iter()
        .take(3)
        .map(ToString::to_string)
        .collect();
    for _ in 0..3 {
        run(&mut m, "inline-noninterposable");
        assert_eq!(
            direct_calls(&m, "vm"),
            vec![
                ("__js_poll_interrupts".into(), false),
                ("__JS_FreeValue".into(), false),
                ("JS_CallInternal".into(), false),
            ]
        );
        assert_eq!(
            m.functions
                .iter()
                .take(3)
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            wrappers
        );
    }
    assert!(m.functions[3].layout_insts().any(|(_, i)| matches!(
        m.functions[3].inst_data(i),
        InstData::Intrinsic {
            id: keel_ir::Intrinsic::FrameAddress,
            ..
        }
    )));
}

#[test]
fn scc_admission_keeps_noinline_binding_and_self_recursion_guards() {
    let public = QUICKJS.replace("cc(c) noninterposable {", "cc(c) {");
    assert_eq!(
        outcome(&public, "inline-noninterposable", "vm", "JS_Call"),
        "recursive_callee"
    );
    let noinline = QUICKJS.replacen("cc(c) inlinehint local", "cc(c) noinline local", 1);
    assert_eq!(
        outcome(
            &noinline,
            "inline-noninterposable",
            "vm",
            "js_poll_interrupts"
        ),
        "noinline"
    );
    let self_recursive = "func @self(i64) -> i64 alwaysinline local {\nblock0(v0: i64):\n v1 = call @self(v0)\n return v1\n}\nfunc @outer(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @self(v0)\n return v1\n}";
    assert_eq!(
        outcome(self_recursive, "inline-noninterposable", "outer", "self"),
        "recursive_callee"
    );
}

#[test]
fn recursive_wrapper_sites_still_consume_ordinary_growth_budget() {
    let mut source = QUICKJS.to_owned();
    source.push_str("\nfunc @many(ptr) {\nblock0(v0: ptr):\n");
    for i in 0..130 {
        source.push_str(&format!(" v{i} = call @js_poll_interrupts(v0)\n"));
    }
    source.push_str(" return\n}\n");
    // Avoid result/parameter ID collision.
    source = source.replace(
        " v0 = call @js_poll_interrupts(v0)",
        " v999 = call @js_poll_interrupts(v0)",
    );
    let mut m = keel_ir::parse(&source).unwrap();
    let r = run(&mut m, "inline-noninterposable");
    let sites: Vec<_> = r
        .invocations
        .iter()
        .flat_map(|p| &p.inline_sites)
        .filter(|s| s.caller == "many")
        .collect();
    assert_eq!(sites.iter().filter(|s| s.outcome == "inlined").count(), 128);
    assert_eq!(
        sites
            .iter()
            .filter(|s| s.outcome == "caller_call_count_budget")
            .count(),
        2
    );
    for _ in 0..2 {
        run(&mut m, "inline-noninterposable");
    }
    assert_eq!(
        direct_calls(&m, "many")
            .iter()
            .filter(|(name, _)| name == "__js_poll_interrupts")
            .count(),
        130
    );
}

#[test]
fn recursive_wrapper_admission_bounds_initial_caller_size() {
    for (padding, expected) in [(8190, "inlined"), (8191, "recursive_wrapper_caller_size")] {
        let mut source = QUICKJS.to_owned();
        source.push_str("\nfunc @large(ptr) {\nblock0(v0: ptr):\n");
        for i in 1..=padding {
            source.push_str(&format!(" v{i} = iconst.i64 0\n"));
        }
        source.push_str(" v9001 = call @js_poll_interrupts(v0)\n return\n}\n");
        assert_eq!(
            outcome(
                &source,
                "inline-noninterposable",
                "large",
                "js_poll_interrupts"
            ),
            expected
        );
        assert_eq!(
            outcome(&source, "inline", "large", "js_poll_interrupts"),
            "recursive_callee"
        );
    }
}

#[test]
fn wrapper_cycles_and_wrapper_to_wrapper_edges_stay_out_of_line() {
    let source = "func @a(i64) -> i64 inlinehint local {\nblock0(v0: i64):\n v1 = call @b(v0)\n return v1\n}\nfunc @b(i64) -> i64 inlinehint local {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n}\nfunc @outer(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @a(v0)\n return v1\n}";
    let mut m = keel_ir::parse(source).unwrap();
    let before = m.to_string();
    for _ in 0..3 {
        run(&mut m, "inline-noninterposable");
    }
    assert_eq!(m.to_string(), before);
}

#[test]
fn readonly_leaf_instruction_block_and_edge_caps_are_exact() {
    fn fixture(arms: usize, padding: usize, extra_edges: usize) -> String {
        let cases = (1..arms + extra_edges)
            .map(|i| format!("{i}: block{}", i % arms + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let mut s =
            "func @leaf(ptr, i64) -> ptr local {\nblock0(v0: ptr, v1: i64):\n v2 = load.ptr v0\n"
                .to_string();
        for i in 0..padding {
            s.push_str(&format!(" v{} = iconst.i64 0\n", i + 3));
        }
        s.push_str(&format!(" switch v1, block1, [{cases}]\n"));
        for i in 1..=arms {
            s.push_str(&format!("block{i}:\n return v2\n"));
        }
        s.push_str("}\nfunc @outer(ptr, i64) -> ptr {\nblock0(v0: ptr, v1: i64):\n v2 = call @leaf(v0, v1)\n return v2\n}");
        s
    }
    for (arms, padding, edges, expected) in [
        (15, 79, 0, "inlined"), // 16 blocks, 96 instructions.
        (15, 80, 0, "profitability_blocks"),
        (16, 0, 0, "profitability_blocks"),
        (15, 0, 17, "inlined"), // Exactly 32 switch edges.
        (15, 0, 18, "profitability_blocks"),
    ] {
        assert_eq!(
            outcome(
                &fixture(arms, padding, edges),
                "inline-noninterposable",
                "outer",
                "leaf"
            ),
            expected
        );
    }
}
