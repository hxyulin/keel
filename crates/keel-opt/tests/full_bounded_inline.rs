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
