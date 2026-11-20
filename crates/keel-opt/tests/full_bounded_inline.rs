use keel_ir::{InstData, Module};

const LUA: &str = include_str!("fixtures/full-inline-index2value.keel");

fn run(m: &mut Module, pass: &str) -> keel_opt::OptimizationReport {
    run_for_target(m, pass, keel_opt::VectorTarget::Portable)
}

fn run_for_target(
    m: &mut Module,
    pass: &str,
    target: keel_opt::VectorTarget,
) -> keel_opt::OptimizationReport {
    keel_ir::verify_module(m).unwrap();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(m, &[pass], target, &mut report).unwrap();
    keel_ir::verify_module(m).unwrap();
    report
}

#[test]
fn aarch64_full_admits_a_bounded_pointer_scanner() {
    use keel_opt::VectorTarget::{Aarch64Neon, X86Sse2};

    let source = "
decl @error(ptr) -> i32

func @scan(ptr, ptr) -> ptr local {
block0(v0: ptr, v1: ptr):
    v2 = iconst.i64 1
    v3 = ptradd v1, v2
    v4 = load.i8 trap_ub v1
    switch v4, block3, [37: block1, 91: block2]
block1:
    v5 = load.ptr v0
    v6 = icmp.eq v3, v5
    brif v6, block4, block5
block2:
    jump block6(v3)
block3:
    return v3
block4:
    v12 = call @error(v0)
    jump block5
block5:
    v8 = ptradd v3, v2
    return v8
block6(v9: ptr):
    v10 = load.i8 trap_ub v9
    brif v10, block7, block8
block7:
    v11 = ptradd v9, v2
    jump block6(v11)
block8:
    return v9
}

func @caller(ptr, ptr) -> ptr {
block0(v0: ptr, v1: ptr):
    v2 = call @scan(v0, v1)
    return v2
}
";
    let observed = |source: &str, pass, target| {
        let mut module = keel_ir::parse(source).unwrap();
        run_for_target(&mut module, pass, target)
            .invocations
            .into_iter()
            .flat_map(|entry| entry.inline_sites)
            .find(|site| site.caller == "caller" && site.callee == "scan")
            .unwrap()
            .outcome
    };
    assert_eq!(
        observed(source, "inline-cheap-reserve", Aarch64Neon),
        "inlined"
    );
    assert_eq!(
        observed(source, "inline-cheap-reserve", X86Sse2),
        "profitability_blocks"
    );
    assert_eq!(
        observed(source, "inline-noninterposable", Aarch64Neon),
        "profitability_blocks"
    );
    for source in [
        source.replace("load.i8 trap_ub", "load.i8 volatile"),
        source.replace("    jump block5", "    brif v12, block3, block5"),
    ] {
        assert_eq!(
            observed(&source, "inline-cheap-reserve", Aarch64Neon),
            "profitability_blocks"
        );
    }
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
fn x86_full_can_inline_bounded_record_copies_after_loop_optimization() {
    use keel_opt::VectorTarget::{Aarch64Neon, X86Sse2};

    let source = "
decl @grow(ptr) -> i32

func @copy(ptr, ptr, ptr, ptr, i64, i64) -> i32 local {
block0(v0:ptr, v1:ptr, v2:ptr, v3:ptr, v4:i64, v5:i64):
    v6 = iconst.i64 0
    v7 = iconst.i64 8
    v8 = iconst.i32 0
    v9 = icmp.ugt v4, v5
    brif v9, block1, block2
block1:
    v10 = call @grow(v0)
    brif v10, block6, block2
block2:
    brif v4, block3(v6), block4
block3(v11:i64):
    v12 = ptradd v2, v11
    v13 = load.i64 v12
    v14 = ptradd v1, v11
    store v13, v14
    v15 = iadd v11, v7
    v16 = icmp.ult v15, v4
    brif v16, block3(v15), block4
block4:
    brif v5, block5(v6), block8
block5(v17:i64):
    v18 = ptradd v3, v17
    v19 = load.i64 v18
    v20 = ptradd v0, v17
    store v19, v20
    v21 = iadd v17, v7
    v22 = icmp.ult v21, v5
    brif v22, block5(v21), block7
block6:
    return v10
block7:
    jump block8
block8:
    return v8
}

func @caller(ptr, ptr, ptr, ptr, i64, i64) -> i32 {
block0(v0:ptr, v1:ptr, v2:ptr, v3:ptr, v4:i64, v5:i64):
    v6 = call @copy(v0, v1, v2, v3, v4, v5)
    return v6
}
";
    let still_calls = |source: &str, pass, target| {
        let mut module = keel_ir::parse(source).unwrap();
        run_for_target(&mut module, pass, target);
        let caller = &module.functions[1];
        caller.layout_insts().any(|(_, i)| {
            matches!(caller.inst_data(i), InstData::Call { func, .. }
                if caller.callee(*func).name == "copy")
        })
    };
    let late = "inline-late-record-copy";
    assert!(!still_calls(source, late, X86Sse2));
    assert!(still_calls(source, late, Aarch64Neon));
    assert!(still_calls(source, "inline-cheap-reserve", X86Sse2));
    assert!(still_calls(
        &source.replace("load.i64 v12", "load.i64 volatile v12"),
        late,
        X86Sse2
    ));
    assert!(still_calls(
        &source.replace("-> i32 local", "-> i32 noinline local"),
        late,
        X86Sse2
    ));
}

#[test]
fn retained_lua_leaf_is_full_only_and_keeps_load_flags() {
    let original = keel_ir::parse(LUA).unwrap();
    assert_eq!(original.functions[0].blocks().count(), 14);
    assert_eq!(original.functions[0].layout_insts().count(), 82);
    assert_eq!(
        outcome(LUA, "inline", "lookup", "index2value"),
        "profitability_blocks"
    );
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

#[test]
fn full_large_caller_wrapper_allowance_keeps_slow_calls_and_stops_at_its_budget() {
    let mut source = QUICKJS.to_owned();
    source.push_str("\nfunc @large(ptr) {\nblock0(v0: ptr):\n");
    for i in 1..=8192 {
        source.push_str(&format!(" v{i} = iconst.i64 0\n"));
    }
    for i in 9000..9096 {
        source.push_str(&format!(" v{i} = call @js_poll_interrupts(v0)\n"));
    }
    source.push_str(" return\n}\n");
    let mut module = keel_ir::parse(&source).unwrap();
    let report = run(&mut module, "inline-cheap-reserve");
    let sites: Vec<_> = report
        .invocations
        .iter()
        .flat_map(|p| &p.inline_sites)
        .filter(|s| s.caller == "large" && s.callee == "js_poll_interrupts")
        .collect();
    let expanded = sites.iter().filter(|s| s.outcome == "inlined").count();
    assert!(expanded > 0 && expanded < 96, "{sites:?}");
    assert!(
        sites
            .iter()
            .any(|s| s.outcome == "recursive_wrapper_caller_size")
    );
    let calls = direct_calls(&module, "large");
    assert!(calls.iter().any(|(name, _)| name == "js_poll_interrupts"));
    assert!(calls.iter().any(|(name, _)| name == "__js_poll_interrupts"));
    assert_eq!(
        outcome(
            &source,
            "inline-noninterposable",
            "large",
            "js_poll_interrupts"
        ),
        "recursive_wrapper_caller_size"
    );
}

#[test]
fn full_large_wrapper_stack_allowance_does_not_admit_larger_snapshots() {
    for (layout, expected) in [("[2 x i64]", 8), ("[4 x i64]", 0)] {
        let mut source = QUICKJS.replace("[2 x i64]", layout);
        source.push_str(&format!(
            "\nfunc @large(ptr,ptr byval {layout}) {{\nblock0(v0: ptr,v1: ptr):\n"
        ));
        for i in 2..=8194 {
            source.push_str(&format!(" v{i} = iconst.i64 0\n"));
        }
        for _ in 0..16 {
            source.push_str(" call @JS_FreeValue(v0,v1)\n");
        }
        source.push_str(" return\n}\n");
        let mut module = keel_ir::parse(&source).unwrap();
        let report = run(&mut module, "inline-cheap-reserve");
        let expanded = report
            .invocations
            .iter()
            .flat_map(|p| &p.inline_sites)
            .filter(|s| s.caller == "large" && s.callee == "JS_FreeValue" && s.outcome == "inlined")
            .count();
        assert_eq!(expanded, expected, "{layout}");
    }
}
