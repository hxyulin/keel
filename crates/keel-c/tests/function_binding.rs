use keel_c::{FunctionBinding, Options};
use keel_ir::{InstData, Target};

#[test]
fn binding_modes_and_elf_visibility_supply_only_body_substitution() {
    for target in Target::NATIVE {
        for visibility in ["default", "hidden", "internal"] {
            for binding in [
                FunctionBinding::SemanticInterposition,
                FunctionBinding::NoSemanticInterposition,
            ] {
                // Late visibility must apply to the definition and earlier direct calls.
                let source = format!(
                    "int helper(int); int caller(int x) {{ return helper(x); }} int helper(int x) {{ return x+3; }} int helper(int) __attribute__((visibility(\"{visibility}\"))); int external(int); int indirect(int (*p)(int), int x) {{ return p(x); }}"
                );
                let mut m = keel_c::compile_with_binding(
                    &source,
                    "binding.c",
                    Options {
                        target,
                        ..Default::default()
                    },
                    binding,
                )
                .unwrap();
                keel_ir::verify_module(&m).unwrap();
                let eligible = binding == FunctionBinding::NoSemanticInterposition
                    || (target != Target::Aarch64AppleDarwin && visibility != "default");
                let helper = m.functions.iter().find(|f| f.name == "helper").unwrap();
                assert_eq!(helper.attrs.no_semantic_interposition, eligible);
                assert!(!helper.attrs.noninterposable);
                assert!(!helper.local);
                assert_eq!(helper.hidden, visibility != "default");
                let sig = helper.sig.clone();
                let text = m.to_string();
                m = keel_ir::parse(&text).unwrap();
                keel_ir::verify_module(&m).unwrap();
                keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
                keel_ir::verify_module(&m).unwrap();
                let caller = m.functions.iter().find(|f| f.name == "caller").unwrap();
                assert_eq!(
                    caller
                        .layout_insts()
                        .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. })),
                    !eligible,
                    "{target:?} {visibility} {binding:?}"
                );
                let helper = m.functions.iter().find(|f| f.name == "helper").unwrap();
                assert_eq!(helper.sig, sig);
                assert!(!helper.local);
                let indirect = m.functions.iter().find(|f| f.name == "indirect").unwrap();
                assert!(
                    indirect.layout_insts().any(|(_, i)| matches!(
                        indirect.inst_data(i),
                        InstData::CallIndirect { .. }
                    ))
                );
            }
        }
    }
}

#[test]
fn weak_noinline_and_missing_definitions_remain_calls() {
    for target in Target::NATIVE {
        let source = "int weak_fn(int x) { return x+1; } int weak_fn(int) __attribute__((weak)); int stay(int x) __attribute__((noinline)) { return x+2; } int external(int); int caller(int x) { return weak_fn(x)+stay(x)+external(x); }";
        let mut m = keel_c::compile_with_binding(
            source,
            "binding.c",
            Options {
                target,
                ..Default::default()
            },
            FunctionBinding::NoSemanticInterposition,
        )
        .unwrap();
        assert!(
            !m.functions
                .iter()
                .find(|f| f.name == "weak_fn")
                .unwrap()
                .attrs
                .no_semantic_interposition
        );
        keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
        let caller = m.functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(
            caller
                .layout_insts()
                .filter(|(_, i)| matches!(caller.inst_data(*i), InstData::Call { .. }))
                .count(),
            3
        );
        assert!(
            !caller
                .callees()
                .find(|(_, c)| c.name == "external")
                .unwrap()
                .1
                .attrs
                .no_semantic_interposition
        );
    }
}

#[test]
fn wasm_rejects_native_binding_policy() {
    assert!(
        keel_c::compile_with_binding(
            "int f(void) { return 1; }",
            "binding.c",
            Options {
                target: Target::Wasm32UnknownUnknown,
                ..Default::default()
            },
            FunctionBinding::NoSemanticInterposition
        )
        .unwrap_err()
        .message
        .contains("native target")
    );
}

#[cfg(feature = "profile")]
#[test]
fn profiled_frontend_preserves_default_binding_policy() {
    let source = "__attribute__((visibility(\"hidden\"))) int helper(int x) { return x+1; } int caller(int x) { return helper(x); }";
    for target in Target::NATIVE {
        let options = Options {
            target,
            ..Default::default()
        };
        let ordinary = keel_c::compile(source, "binding.c", options).unwrap();
        let (profiled, _) = keel_c::profile::compile(source, "binding.c", options).unwrap();
        assert_eq!(ordinary.to_string(), profiled.to_string());
    }
}
