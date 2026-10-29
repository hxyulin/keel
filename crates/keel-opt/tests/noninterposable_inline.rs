use keel_ir::{InstData, Module};

fn optimize(source: &str, pass: &str) -> (Module, keel_opt::OptimizationReport) {
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
    (m, report)
}
fn calls(m: &Module, name: &str) -> usize {
    let f = m.functions.iter().find(|f| f.name == name).unwrap();
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
        .count()
}
fn scalar(attrs: &str, sig: &str, body: &str) -> String {
    format!(
        "func @helper{sig} {attrs} {{\nblock0(v0: i8):\n {body}\n}}\nfunc @caller(i8 uext) -> i8 sext {{\nblock0(v0: i8):\n v1 = call @helper(v0)\n return v1\n}}\n"
    )
}

#[test]
fn admission_requires_both_explicit_pass_and_definition_contract() {
    for pass in [
        "inline",
        "inline-aggregate-reserve",
        "inline-noninterposable",
    ] {
        for attrs in [
            "",
            "alwaysinline",
            "noninterposable",
            "noninterposable alwaysinline",
            "noninterposable noinline",
            "no_semantic_interposition",
            "no_semantic_interposition alwaysinline",
            "no_semantic_interposition noinline",
            "local",
        ] {
            let source = scalar(attrs, "(i8 uext) -> i8 sext", "return v0");
            let (m, report) = optimize(&source, pass);
            let admitted = attrs == "local"
                || (pass == "inline-noninterposable"
                    && (attrs.contains("noninterposable")
                        || attrs.contains("no_semantic_interposition"))
                    && !attrs.contains("noinline"));
            assert_eq!(
                calls(&m, "caller"),
                usize::from(!admitted),
                "{pass}: {attrs}"
            );
            let site = report
                .invocations
                .iter()
                .flat_map(|p| &p.inline_sites)
                .find(|s| s.caller == "caller")
                .unwrap();
            assert_eq!(
                site.outcome,
                if admitted {
                    "inlined"
                } else if attrs.contains("noinline") {
                    "noinline"
                } else {
                    "public_definition"
                }
            );
            assert_eq!(
                site.category,
                if attrs.contains("alwaysinline") {
                    "alwaysinline"
                } else {
                    "ordinary"
                }
            );
            assert_eq!(m.functions.len(), 2);
            assert_eq!(m.functions[0].local, attrs == "local");
            assert_eq!(
                m.functions[0].sig,
                keel_ir::parse(&source).unwrap().functions[0].sig
            );
        }
    }
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert_eq!(
            tier.pipeline().contains(&"inline-cheap-reserve"),
            tier == keel_opt::OptimizationTier::Full
        );
    }
}

#[test]
fn declarations_and_indirect_calls_do_not_supply_a_definition() {
    let source = "decl @helper(i8 uext) -> i8 sext noninterposable alwaysinline\nfunc @caller(i8 uext) -> i8 sext {\nblock0(v0: i8):\n v1 = call @helper(v0)\n return v1\n}\nfunc @indirect(ptr, i8) -> i8 {\nblock0(v0: ptr, v1: i8):\n v2 = call_indirect v0(v1) : (i8) -> i8\n return v2\n}";
    let (m, _) = optimize(source, "inline-noninterposable");
    assert_eq!(calls(&m, "caller"), 1);
    assert!(
        m.functions[1]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[1].inst_data(i), InstData::CallIndirect { .. }))
    );
    // A manually stale callee-only promise must not admit an ordinary definition.
    let mut m =
        keel_ir::parse(&scalar("alwaysinline", "(i8 uext) -> i8 sext", "return v0")).unwrap();
    let r = m.functions[1].callees().next().unwrap().0;
    m.functions[1].callee_mut(r).attrs.noninterposable = true;
    let pass = keel_opt::PASSES
        .iter()
        .find(|p| p.name() == "inline-noninterposable")
        .unwrap();
    assert_eq!(pass.run_module(&mut m), Some(false));
    assert_eq!(calls(&m, "caller"), 1);
}

#[test]
fn binding_does_not_override_legality_noinline_recursion_or_budgets() {
    for (attrs, sig, body) in [
        (
            "noninterposable noinline",
            "(i8 uext) -> i8 sext",
            "return v0",
        ),
        (
            "noninterposable alwaysinline",
            "(i8 uext, ...) -> i8 sext",
            "return v0",
        ),
        (
            "noninterposable alwaysinline",
            "(i8 uext) -> i8 sext returns_twice",
            "return v0",
        ),
        (
            "noninterposable alwaysinline",
            "(i8 uext) -> i8 sext",
            "v1 = call @helper(v0)\n return v1",
        ),
        (
            "noninterposable alwaysinline",
            "(i8 uext) -> i8 sext",
            "v1 = intrinsic frame_address()\n return v0",
        ),
    ] {
        let (m, _) = optimize(&scalar(attrs, sig, body), "inline-noninterposable");
        assert_eq!(calls(&m, "caller"), 1, "{attrs} {sig} {body}");
    }
    // Six instructions cost more than the call, so the call-count cap applies.
    let mut source = format!(
        "func @helper() noninterposable alwaysinline {{\nblock0:\n{} return\n}}\nfunc @caller() {{\nblock0:\n",
        (0..5)
            .map(|n| format!(" v{n} = iconst.i64 {n}\n"))
            .collect::<String>()
    );
    for _ in 0..130 {
        source.push_str(" call @helper()\n");
    }
    source.push_str(" return\n}");
    let (m, _) = optimize(&source, "inline-noninterposable");
    assert_eq!(calls(&m, "caller"), 2);
}

#[test]
fn exported_templates_use_the_same_byval_sret_and_alias_clone_contracts() {
    let source = "func @helper(ptr sret {i64, i64, i64}, ptr byval {i64, i64, i64}, ptr) local {\nblock0(v0: ptr, v1: ptr, v2: ptr):\n v3 = load.i64 v1\n v4 = iconst.i64 99\n store v4, v2\n store v3, v0\n return\n}\nfunc @caller(ptr, ptr) {\nblock0(v0: ptr, v1: ptr):\n call @helper(v0, v1, v1)\n call @helper(v0, v1, v1)\n return\n}";
    let (private, _) = optimize(source, "inline");
    let (public, _) = optimize(
        &source.replace(" local {", " noninterposable {"),
        "inline-noninterposable",
    );
    assert_eq!(calls(&public, "caller"), 0);
    assert_eq!(
        private.functions[1].to_string(),
        public.functions[1].to_string()
    );
    assert_eq!(public.functions[1].num_stack_slots(), 2);
    let mut m = keel_ir::parse("func @helper(i64, i64) -> i64 noninterposable {\nblock0(v0: i64, v1: i64):\n v2 = iadd v0, v1\n return v2\n}\nfunc @caller(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = call @helper(v0, v1)\n return v2\n}").unwrap();
    let h = &mut m.functions[0];
    let params = h.block_params(h.entry_block().unwrap()).to_vec();
    let add = h.layout_insts().next().unwrap().1;
    h.replace_all_uses(params[0], params[1]);
    h.replace_all_uses(h.first_result(add), params[1]);
    h.remove_inst(add);
    keel_opt::run_passes(&mut m, &["inline-noninterposable", "simplify-cfg", "dce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(calls(&m, "caller"), 0);
    let f = &m.functions[1];
    let ret = f.layout_insts().last().unwrap().1;
    let InstData::Return { args } = f.inst_data(ret) else {
        panic!()
    };
    assert_eq!(
        f.resolve(args[0]),
        f.block_params(f.entry_block().unwrap())[1]
    );
}

#[test]
fn exported_binding_keeps_soft_limits_hard_stack_and_exact_abi_checks() {
    for (policy, expected) in [("", 1), ("alwaysinline", 0)] {
        let mut body = String::new();
        for i in 1..=65 {
            body.push_str(&format!("v{i} = iadd v{}, v0\n ", i - 1));
        }
        body.push_str("return v65");
        let source = scalar(
            &format!("noninterposable {policy}"),
            "(i8 uext) -> i8 sext",
            &body,
        );
        let (m, _) = optimize(&source, "inline-noninterposable");
        assert_eq!(calls(&m, "caller"), expected);
    }
    let source = scalar(
        "noninterposable alwaysinline",
        "(i8 uext) -> i8 sext",
        "return v0",
    )
    .replacen(
        "{\nblock0",
        "{\n ss0 = stack_slot 65537, align 1\nblock0",
        1,
    );
    let (m, _) = optimize(&source, "inline-noninterposable");
    assert_eq!(calls(&m, "caller"), 1);
    // Direct API users cannot bypass the full ABI comparison (here ArgExt).
    let mut m = keel_ir::parse(&scalar(
        "noninterposable alwaysinline",
        "(i8 uext) -> i8 sext",
        "return v0",
    ))
    .unwrap();
    let r = m.functions[1].callees().next().unwrap().0;
    m.functions[1].callee_mut(r).sig.params[0].ext = keel_ir::ArgExt::Sext;
    let pass = keel_opt::find_pass("inline-noninterposable").unwrap();
    assert_eq!(pass.run_module(&mut m), Some(false));
    assert_eq!(calls(&m, "caller"), 1);
}

#[test]
fn semantic_body_contract_cannot_override_weak_binding() {
    for binding in ["weak", "weak_odr"] {
        let source = scalar(
            &format!("no_semantic_interposition alwaysinline {binding}"),
            "(i8 uext) -> i8 sext",
            "return v0",
        );
        let (m, _) = optimize(&source, "inline-noninterposable");
        assert_eq!(calls(&m, "caller"), 1);
    }
}
