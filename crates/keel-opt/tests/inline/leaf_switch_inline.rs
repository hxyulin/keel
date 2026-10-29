use keel_ir::InstData;

fn source(arms: usize, attrs: &str, first: &str) -> String {
    let cases = (1..arms)
        .map(|i| format!("{i}: block{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut s = format!(
        "func @helper(i64) -> i64 {attrs} {{\nblock0(v0: i64):\n switch v0, block{arms}, [{cases}]\n"
    );
    for i in 1..=arms {
        s.push_str(&format!("block{i}:\n"));
        s.push_str(if i == 1 { first } else { " return v0\n" });
    }
    s.push_str(
        "}\nfunc @caller(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}\n",
    );
    s
}

fn outcome(source: &str, pass: &str) -> String {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut m,
        &[pass],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    let site = report
        .invocations
        .iter()
        .flat_map(|p| &p.inline_sites)
        .find(|s| s.caller == "caller")
        .unwrap();
    let caller = m.functions.iter().find(|f| f.name == "caller").unwrap();
    assert_eq!(
        caller
            .layout_insts()
            .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. })),
        site.outcome != "inlined"
    );
    site.outcome.to_owned()
}

#[test]
fn leaf_switch_admission_preserves_binding_and_noinline() {
    for (attrs, pass, expected) in [
        ("local", "inline", "inlined"),
        ("noninterposable", "inline", "public_definition"),
        ("", "inline-noninterposable", "public_definition"),
        ("noninterposable", "inline-noninterposable", "inlined"),
        (
            "noninterposable noinline",
            "inline-noninterposable",
            "noinline",
        ),
    ] {
        assert_eq!(outcome(&source(12, attrs, " return v0\n"), pass), expected);
    }
}

#[test]
fn hard_block_instruction_edge_and_weight_limits() {
    assert_eq!(
        outcome(&source(15, "local", " return v0\n"), "inline"),
        "inlined"
    );
    assert_eq!(
        outcome(&source(16, "local", " return v0\n"), "inline"),
        "profitability_blocks"
    );
    // A single return arm may contain cheap work, but every instruction counts.
    for (constants, expected) in [
        (39, "inlined"),
        (40, "profitability_blocks"),
        (65, "profitability_blocks"),
    ] {
        // 12 returns + 12 switch edges + 1 switch + constants = 25 + constants.
        let body = (1..=constants)
            .map(|i| format!(" v{i} = iconst.i64 {i}\n"))
            .collect::<String>()
            + " return v0\n";
        assert_eq!(outcome(&source(12, "local", &body), "inline"), expected);
    }
    // Many destinations to the same block still pay switch comparison cost.
    for (cases, expected) in [(31, "inlined"), (32, "profitability_blocks")] {
        let many = (1..=cases)
            .map(|i| format!("{i}: block1"))
            .collect::<Vec<_>>()
            .join(", ");
        let s = source(12, "local", " return v0\n");
        let start = s.find('[').unwrap();
        let end = s.find(']').unwrap();
        let s = format!("{}{}{}", &s[..start + 1], many, &s[end..]);
        assert_eq!(outcome(&s, "inline"), expected);
    }
}

#[test]
fn no_memory_calls_division_fp_or_cycles_in_block_exception() {
    for body in [
        " v1 = iconst.i64 1\n v2 = sdiv v0, v1\n return v2\n",
        " v1 = call @external(v0)\n return v1\n",
        " v1 = fconst.f64 1.0\n return v0\n",
        " jump block1\n",
        " v1 = symbol_addr @object\n v2 = load.i64 v1\n return v2\n",
    ] {
        let s = "data @object = align 8 { zero 8 }\ndecl @external(i64) -> i64\n".to_owned()
            + &source(12, "local", body);
        assert_eq!(outcome(&s, "inline"), "profitability_blocks", "{body}");
    }
    let s = source(12, "local", " return v0\n")
        .replace("local {", "local {\n ss0 = stack_slot 8, align 8");
    assert_eq!(outcome(&s, "inline"), "profitability_blocks");
    // An unreachable cycle must not be hidden by an entry-only DFS.
    let s = source(12, "local", " return v0\n")
        .replace("1: block1,", "1: block2,")
        .replace("block1:\n return v0", "block1:\n jump block1");
    assert_eq!(outcome(&s, "inline"), "profitability_blocks");
}

#[test]
fn leaf_switch_still_pays_ordinary_caller_growth_and_site_budgets() {
    // Sixteen cloned instructions per site: 128 sites exhaust both existing
    // limits exactly (2048 instructions); no reserve is available here.
    let mut s = source(15, "local", " return v0\n");
    let call = " v1 = call @helper(v0)\n";
    let calls = (1..=130)
        .map(|i| format!(" v{i} = call @helper(v{})\n", i - 1))
        .collect::<String>();
    s = s
        .replace(call, &calls)
        .replace(" return v1\n}", " return v130\n}");
    let mut m = keel_ir::parse(&s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["inline"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = m.functions.iter().find(|f| f.name == "caller").unwrap();
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
            .count(),
        2
    );
}
