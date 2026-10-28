//! Declaration/redeclaration -> synchronized direct contracts -> optimizer/inliner.
use keel_ir::{InstData, Target};
use keel_opt::OptimizationTier;

#[test]
fn late_contracts_reach_definitions_calls_and_inlined_calls() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug_info in [false, true] {
            for definition in [false, true] {
                for late in [false, true] {
                    let contract = "int stop(void) __attribute__((noreturn,nothrow,noinline));";
                    let body = if definition {
                        "int stop(void) { for (;;) {} }"
                    } else {
                        "int stop(void);"
                    };
                    let helper = "static int helper(void) __attribute__((always_inline)); static int helper(void) { return stop(); } int f(void) { return helper(); }";
                    let source = if late {
                        format!("{body}\n{helper}\n{contract}")
                    } else {
                        format!("{contract}\n{body}\n{helper}")
                    };
                    let original = keel_c::compile(
                        &source,
                        "contracts.c",
                        keel_c::Options { target, debug_info },
                    )
                    .unwrap();
                    for tier in [
                        OptimizationTier::None,
                        OptimizationTier::Basic,
                        OptimizationTier::Full,
                    ] {
                        let mut m = original.clone();
                        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
                        keel_ir::verify_module(&m).unwrap();
                        let attrs = if definition {
                            &m.functions.iter().find(|f| f.name == "stop").unwrap().attrs
                        } else {
                            &m.decls.iter().find(|d| d.name == "stop").unwrap().attrs
                        };
                        assert!(attrs.noreturn && attrs.nounwind && attrs.noinline);
                        for f in &m.functions {
                            for (_, callee) in f.callees().filter(|(_, c)| c.name == "stop") {
                                assert_eq!(&callee.attrs, attrs);
                            }
                        }
                        let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                        let helper_call = f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == "helper"));
                        assert_eq!(helper_call, tier != OptimizationTier::Full, "{m}");
                        let owner = if tier == OptimizationTier::Full {
                            f
                        } else {
                            m.functions.iter().find(|f| f.name == "helper").unwrap()
                        };
                        let call = owner.layout_insts().find_map(|(_, i)| matches!(owner.inst_data(i), InstData::Call { func, .. } if owner.callee(*func).name == "stop").then_some(i)).unwrap();
                        let insts = owner.block_insts(owner.inst_block(call));
                        let pos = insts.iter().position(|&i| i == call).unwrap();
                        assert!(
                            matches!(owner.inst_data(insts[pos + 1]), InstData::Trap),
                            "{m}"
                        );
                        assert_eq!(owner.inst_loc(call).is_some(), debug_info);
                        assert_eq!(owner.inst_loc(insts[pos + 1]).is_some(), debug_info);
                        keel_codegen::compile(&m, target).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn independent_c_contracts_and_returns_twice_survive_optimization() {
    for (attr, noreturn, nounwind, twice) in [
        ("noreturn", true, false, false),
        ("nothrow", false, true, false),
        ("nothrow,returns_twice", false, true, true),
        ("", false, false, false),
    ] {
        let source = format!(
            "int g(void); int f(void) {{ int x = g(); return x+1; }} int g(void) __attribute__(({attr}));"
        );
        for debug_info in [false, true] {
            let original = keel_c::compile(
                &source,
                "independent.c",
                keel_c::Options {
                    debug_info,
                    ..Default::default()
                },
            )
            .unwrap();
            for tier in [OptimizationTier::Basic, OptimizationTier::Full] {
                let mut m = original.clone();
                keel_opt::optimize_with_tier(&mut m, tier).unwrap();
                keel_ir::verify_module(&m).unwrap();
                let g = m.decls.iter().find(|g| g.name == "g").unwrap();
                assert_eq!(
                    (g.attrs.noreturn, g.attrs.nounwind, g.sig.returns_twice),
                    (noreturn, nounwind, twice)
                );
                let f = &m.functions[0];
                assert_eq!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Return { .. })),
                    !noreturn
                );
                assert_eq!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Trap)),
                    noreturn
                );
                keel_codegen::compile(&m, Default::default()).unwrap();
            }
        }
    }
}
