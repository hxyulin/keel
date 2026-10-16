use keel_ir::{EntityRef, InstData, Module};
use keel_opt::OptimizationTier;

fn run(src: &str) -> Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["call-contracts"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let once = m.to_string();
    keel_opt::run_passes(&mut m, &["call-contracts"]).unwrap();
    assert_eq!(once, m.to_string());
    m
}

#[test]
fn noreturn_keeps_effects_call_and_debug_trap_but_cuts_normal_continuation() {
    for attrs in ["noreturn", "noreturn nounwind"] {
        let src = format!(
            "decl @stop() -> i32 {attrs}\ndecl @effect()\nfunc @f(ptr, i8) -> i32 {{
block0(v0: ptr, v1: i8):
 v2 = iconst.i32 7
 store volatile v2, v0
 call @effect()
 brif v1, block1, block2
block1:
 v3 = call @stop()
 store volatile v3, v0
 call @effect()
 jump block3(v3)
block2:
 jump block3(v2)
block3(v4: i32):
 return v4
}}"
        );
        let mut original = keel_ir::parse(&src).unwrap();
        let file = original.add_file("contracts.c");
        let f = &mut original.functions[0];
        let stop = f.layout_insts().find_map(|(_, i)| matches!(f.inst_data(i), InstData::Call { func, .. } if f.callee(*func).name == "stop").then_some(i)).unwrap();
        let loc = keel_ir::SourceLoc {
            file: file,
            line: 12,
            col: 3,
            scope: None,
        };
        f.set_inst_loc(stop, Some(loc));
        keel_opt::run_passes(&mut original, &["call-contracts"]).unwrap();
        keel_ir::verify_module(&original).unwrap();
        let f = &original.functions[0];
        assert_eq!(
            f.layout_insts()
                .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
                .count(),
            2
        );
        assert_eq!(
            f.layout_insts()
                .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Store { .. }))
                .count(),
            1
        );
        let trap = f
            .layout_insts()
            .find_map(|(_, i)| matches!(f.inst_data(i), InstData::Trap).then_some(i))
            .unwrap();
        assert_eq!(f.inst_loc(stop), Some(loc));
        assert_eq!(f.inst_loc(trap), Some(loc));
        assert!(
            f.layout_insts()
                .any(|(_, i)| matches!(f.inst_data(i), InstData::Return { .. }))
        );
    }
}

fn invoke_source(attrs: &str) -> String {
    format!("decl @g(i32) -> i32 {attrs}\ndecl @personality()\nfunc @f(i32) -> i32 personality @personality {{
block0(v0: i32):
 v1 = invoke @g(v0), block1, block2
block1:
 jump block3(v1)
block2:
 v2, v3 = landingpad cleanup
 jump block3(v3)
block3(v4: i32):
 return v4
}}")
}

#[test]
fn nounwind_invoke_preserves_result_and_normal_return() {
    for attrs in ["nounwind", "returns_twice nounwind"] {
        let m = run(&invoke_source(attrs));
        let f = &m.functions[0];
        assert!(!m.to_string().contains("invoke @g"));
        assert!(!m.to_string().contains("landingpad"));
        assert!(m.to_string().contains("v1 = call @g(v0)"));
        assert!(m.to_string().contains("jump block3(v1)"));
        assert!(
            f.layout_insts()
                .any(|(_, i)| matches!(f.inst_data(i), InstData::Return { .. }))
        );
        assert!(
            !f.layout_insts()
                .any(|(_, i)| matches!(f.inst_data(i), InstData::Trap))
        );
    }
    let m = run(&invoke_source("noreturn nounwind"));
    assert!(m.to_string().contains("v1 = call @g(v0)\n    trap"));
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::Return { .. }))
    );
}

#[test]
fn noreturn_alone_and_unknown_invoke_retain_unwind_and_normal_edges() {
    for attrs in ["", "noreturn"] {
        let src = invoke_source(attrs);
        let m = run(&src);
        assert_eq!(m.to_string(), keel_ir::parse(&src).unwrap().to_string());
    }
}

#[test]
fn unknown_nounwind_only_and_indirect_calls_keep_continuations() {
    for attrs in ["", "nounwind", "returns_twice", "returns_twice nounwind"] {
        let src = format!(
            "decl @g() -> i32 {attrs}\nfunc @f(ptr) -> i32 {{\nblock0(v0: ptr):\n v1 = call @g()\n v2 = call_indirect v0() : () -> i32\n v3 = iadd v1, v2\n return v3\n}}"
        );
        assert_eq!(
            run(&src).to_string(),
            keel_ir::parse(&src).unwrap().to_string()
        );
    }
}

#[test]
fn shared_landingpad_remains_for_other_invokes() {
    let src = "decl @g() nounwind\ndecl @h() noreturn\ndecl @personality()\nfunc @f(i8) personality @personality {
block0(v0: i8):
 brif v0, block1, block2
block1:
 invoke @g(), block3, block4
block2:
 invoke @h(), block3, block4
block3:
 return
block4:
 v1, v2 = landingpad cleanup
 trap
}";
    let m = run(src);
    assert!(m.to_string().contains("call @g()"));
    assert!(m.to_string().contains("invoke @h()"));
    assert!(m.to_string().contains("landingpad"));
}

#[test]
fn invoke_result_in_shared_normal_block_requires_call_dominance() {
    // Current verifier permits a result under normal-destination dominance,
    // including this shared normal block. Preserve that representation.
    let src = "decl @g() -> i32 nounwind\ndecl @personality()\nfunc @f(i8) -> i32 personality @personality {
block0(v0: i8):
 brif v0, block1, block2
block1:
 v1 = invoke @g(), block2, block3
block2:
 return v1
block3:
 v2, v3 = landingpad cleanup
 return v3
}";
    assert_eq!(
        run(src).to_string(),
        keel_ir::parse(src).unwrap().to_string()
    );
}

#[test]
fn reachable_definition_after_noreturn_cannot_be_discarded() {
    // Removing the normal edge still leaves the join reachable from unwind.
    // Its invoke result is legal under current normal-destination dominance.
    let src = "decl @stop() noreturn\ndecl @g() -> i32\ndecl @personality()\nfunc @f() -> i32 personality @personality {
block0:
 v4 = invoke @g(), block1, block3
block1:
 call @stop()
 v0 = invoke @g(), block2, block3
block2:
 return v0
block3:
 v1, v2 = landingpad cleanup
 jump block2
}";
    assert_eq!(
        run(src).to_string(),
        keel_ir::parse(src).unwrap().to_string()
    );
}

#[test]
fn reachable_eh_typeid_keeps_its_only_catch_clause() {
    let src = "decl @g() nounwind\ndecl @personality()\ndecl data @type\nfunc @f() -> i32 personality @personality {
block0:
 invoke @g(), block1, block2
block1:
 v0 = eh_typeid @type
 return v0
block2:
 v1, v2 = landingpad catch @type
 trap
}";
    assert_eq!(
        run(src).to_string(),
        keel_ir::parse(src).unwrap().to_string()
    );
}

#[test]
fn tier_policy_and_no_body_inference() {
    let src = invoke_source("nounwind");
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        let mut m = keel_ir::parse(&src).unwrap();
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(
            m.to_string().contains("invoke @g"),
            tier == OptimizationTier::None
        );
    }
    let src = "func @g() local {\nblock0:\n trap\n}\nfunc @f() {\nblock0:\n call @g()\n return\n}";
    assert_eq!(
        run(src).to_string(),
        keel_ir::parse(src).unwrap().to_string()
    );
}

#[test]
fn existing_defensive_trap_keeps_identity_and_location() {
    let mut m =
        keel_ir::parse("decl @stop() noreturn\nfunc @f() {\nblock0:\n call @stop()\n trap\n}")
            .unwrap();
    let file = m.add_file("trap.c");
    let f = &mut m.functions[0];
    let trap = f.layout_insts().last().unwrap().1;
    let loc = keel_ir::SourceLoc {
        file,
        line: 9,
        col: 2,
        scope: None,
    };
    f.set_inst_loc(trap, Some(loc));
    assert!(!keel_opt::find_pass("call-contracts").unwrap().run(f));
    assert!(!f.is_removed(trap));
    assert_eq!(f.inst_loc(trap), Some(loc));
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn invoke_lowering_keeps_multiple_results_and_debug_locations() {
    let mut m = keel_ir::parse("decl @g() -> (i32, i64) nounwind\ndecl @personality()\nfunc @f() -> (i32, i64) personality @personality {\nblock0:\n v0, v1 = invoke @g(), block1, block2\nblock1:\n return v0, v1\nblock2:\n v2, v3 = landingpad cleanup\n trap\n}").unwrap();
    let file = m.add_file("invoke.c");
    let f = &mut m.functions[0];
    f.debug_scopes.push(None);
    let invoke = f.layout_insts().next().unwrap().1;
    let results = f.inst_results(invoke).to_vec();
    let loc = keel_ir::SourceLoc {
        file,
        line: 15,
        col: 8,
        scope: Some(keel_ir::DebugScope::new(0)),
    };
    f.set_inst_loc(invoke, Some(loc));
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["call-contracts"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    assert!(matches!(f.inst_data(invoke), InstData::Call { .. }));
    assert_eq!(f.inst_results(invoke), results);
    assert_eq!(f.inst_loc(invoke), Some(loc));
    let jump = *f.block_insts(f.inst_block(invoke)).last().unwrap();
    assert!(matches!(f.inst_data(jump), InstData::Jump { .. }));
    assert_eq!(f.inst_loc(jump), Some(loc));
}
