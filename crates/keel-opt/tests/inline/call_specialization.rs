use keel_ir::{InstData, parse, verify_module};

fn source(attrs: &str) -> String {
    let mut source = format!(
        "func @pick(i64,i32) -> i64 {attrs} {{\nblock0(v0:i64,v1:i32):\n switch v1, block25, ["
    );
    for i in 0..24 {
        if i != 0 {
            source.push_str(", ");
        }
        source.push_str(&format!("{i}: block{}", i + 1));
    }
    source.push_str("]\n");
    for i in 0..24 {
        source.push_str(&format!(
            "block{}:\n v{}=iconst.i64 {}\n v{}=iadd v0,v{}\n return v{}\n",
            i + 1,
            2 + i * 2,
            i + 1,
            3 + i * 2,
            2 + i * 2,
            3 + i * 2
        ));
    }
    source.push_str("block25:\n return v0\n}\n");
    for (name, selector) in [("a", 3), ("b", 17), ("repeat", 3)] {
        source.push_str(&format!("func @{name}(i64)->i64 {{\nblock0(v0:i64):\n v1=iconst.i32 {selector}\n v2=call @pick(v0,v1)\n return v2\n}}\n"));
    }
    source.push_str("func @variable(i64,i32)->i64 {\nblock0(v0:i64,v1:i32):\n v2=call @pick(v0,v1)\n return v2\n}\n");
    source
}

fn calls(f: &keel_ir::Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
        .count()
}

#[test]
fn full_specializes_each_literal_context_without_mutating_the_definition() {
    let mut module = parse(&source("local")).unwrap();
    let original = module.clone();
    let definition = module.functions[0].to_string();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut module,
        &["inline-cheap-reserve", "simplify-cfg", "instcombine", "dce"],
        keel_opt::VectorTarget::Portable,
        &mut report,
    )
    .unwrap();
    verify_module(&module).unwrap();
    assert_eq!(
        module.functions.len(),
        5,
        "specialized templates must not become module definitions"
    );
    assert_eq!(module.functions[0].to_string(), definition);
    for (name, expected) in [("a", 4), ("b", 18), ("repeat", 4)] {
        let f = module.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(calls(f), 0, "@{name} should inline its selected arm");
        assert!(
            f.to_string().contains(&format!("iconst.i64 {expected}")),
            "{f}"
        );
    }
    assert_eq!(
        calls(
            module
                .functions
                .iter()
                .find(|f| f.name == "variable")
                .unwrap()
        ),
        1
    );
    assert_eq!(
        report
            .invocations
            .iter()
            .flat_map(|p| &p.inline_sites)
            .filter(|s| s.outcome == "inlined")
            .count(),
        3
    );
    for name in ["a", "b", "repeat", "variable"] {
        for x in [0, 1, 127, i64::MAX as u64, i64::MIN as u64, u64::MAX] {
            for mode in [0, 3, 17, 23, 24, u32::MAX as u64] {
                let args = [
                    keel_ir_interp::Val::int(keel_ir::Type::I64, x as i128),
                    keel_ir_interp::Val::int(keel_ir::Type::I32, mode as i128),
                ];
                let args = if name == "variable" {
                    &args[..]
                } else {
                    &args[..1]
                };
                assert_eq!(
                    keel_ir_interp::Interpreter::new(&original).call(name, args),
                    keel_ir_interp::Interpreter::new(&module).call(name, args),
                    "{name} {x} {mode}"
                );
            }
        }
    }
}

#[test]
fn binding_math_policy_and_noinline_remain_admission_barriers() {
    for (attrs, caller_attrs) in [
        ("noinline local", ""),
        ("", ""),
        ("fast_math local", ""),
        ("local", "optnone"),
        ("local", "optimize(\"no-inline\")"),
    ] {
        let text = source(attrs);
        // optnone is a caller policy. noinline on the caller alone permits
        // inlining into it; no-inline explicitly disables that family.
        let text = text.replace(
            "func @a(i64)->i64 {",
            &format!("func @a(i64)->i64 {caller_attrs} {{"),
        );
        let mut module = parse(&text).unwrap();
        keel_opt::run_passes(&mut module, &["inline-cheap-reserve"]).unwrap();
        verify_module(&module).unwrap();
        assert_eq!(
            calls(module.functions.iter().find(|f| f.name == "a").unwrap()),
            1,
            "attrs {attrs}, caller {caller_attrs}"
        );
    }
}

#[test]
fn ordinary_and_balanced_inline_policies_keep_their_existing_admission() {
    for pass in ["inline", "inline-noninterposable"] {
        let mut module = parse(&source("local")).unwrap();
        keel_opt::run_passes(&mut module, &[pass]).unwrap();
        assert_eq!(calls(&module.functions[1]), 1, "{pass}");
    }
}

#[test]
fn a_large_selected_path_keeps_its_call() {
    let text = source("local").replace(
        "v9=iadd v0,v8\n return v9",
        "v9=iadd v0,v8\n v60=rotl v9,v8\n v61=xor v60,v0\n v62=imul v61,v9\n v63=rotl v62,v8\n return v63",
    );
    let original = parse(&text).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["inline-cheap-reserve"]).unwrap();
    verify_module(&module).unwrap();
    assert_eq!(
        calls(&module.functions[1]),
        1,
        "larger specialized bodies need a separate profitability policy"
    );
    for x in [0, 1, 17, i64::MIN as u64, u64::MAX] {
        let args = [keel_ir_interp::Val::int(keel_ir::Type::I64, x as i128)];
        assert_eq!(
            keel_ir_interp::Interpreter::new(&original).call("a", &args),
            keel_ir_interp::Interpreter::new(&module).call("a", &args)
        );
    }
}
