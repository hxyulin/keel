mod support;
use keel_ir::{Function, InstData as D, Type};

fn optimize(src: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["stack-promote"]).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
fn memory(f: &Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), D::Load { .. } | D::Store { .. }))
        .count()
}
fn fixture(ty: Type) -> String {
    format!(
        "func @f({ty}, {ty}) -> {ty} {{
 ss0 = stack_slot 8, align 8
block0(v0: {ty}, v1: {ty}):
 v2 = stack_addr ss0
 store v1, v2
 v3 = iconst.{ty} 0
 jump block1(v3)
block1(v4: {ty}):
 v5 = icmp.ult v4, v0
 brif v5, block2, block3
block2:
 v6 = load.{ty} v2
 v7 = iadd v6, v4
 store v7, v2
 v8 = iconst.{ty} 1
 v9 = iadd v4, v8
 jump block1(v9)
block3:
 v10 = load.{ty} v2
 return v10
}}"
    )
}
#[test]
fn initialized_loop_recurrences_all_widths_and_tiers() {
    for ty in [Type::I8, Type::I16, Type::I32, Type::I64] {
        let src = fixture(ty);
        let m = optimize(&src);
        assert_eq!(memory(&m.functions[0]), 0, "{m}");
        let before = m.to_string();
        let mut again = m.clone();
        keel_opt::run_passes(&mut again, &["stack-promote"]).unwrap();
        assert_eq!(before, again.to_string());
        for tier in [
            keel_opt::OptimizationTier::Fast,
            keel_opt::OptimizationTier::Full,
        ] {
            let mut m = m.clone();
            keel_opt::optimize_with_tier(&mut m, tier).unwrap();
            for n in [0u64, 1, 2, 17, 63] {
                for seed in [0u64, 1, 127, 255, 65535, u64::MAX] {
                    let mask = u64::MAX >> (64 - ty.bytes().unwrap() * 8);
                    let expected = seed.wrapping_add(n * n.saturating_sub(1) / 2) & mask;
                    assert_eq!(support::execute(&m.functions[0], &[n, seed]), expected);
                }
            }
        }
    }
}
#[test]
fn disjoint_fields_and_duplicate_edges_get_independent_ssa() {
    let src = "func @f(i8, i64, i32) -> i64 {
 ss0 = stack_slot 16, align 8
block0(v0: i8, v1: i64, v2: i32):
 v3 = stack_addr ss0
 v4 = iconst.i64 8
 v5 = ptradd v3, v4
 store v1, v3
 store v2, v5
 brif v0, block1(v1), block1(v1)
block1(v6: i64):
 v7 = load.i64 v3
 v8 = load.i32 v5
 v9 = uext.i64 v8
 v10 = iadd v7, v9
 return v10
}";
    let mut m = optimize(src);
    assert_eq!(memory(&m.functions[0]), 0);
    keel_opt::run_passes(&mut m, &["dce"]).unwrap();
    for cond in [0, 1, 255] {
        assert_eq!(support::execute(&m.functions[0], &[cond, u64::MAX, 17]), 16);
    }
}
#[test]
fn initialization_can_be_in_both_arms_and_pointer_fields_are_exact() {
    let src = "func @f(i8, ptr, ptr) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: i8, v1: ptr, v2: ptr):
 v3 = stack_addr ss0
 brif v0, block1, block2
block1:
 store v1, v3
 jump block3
block2:
 store v2, v3
 jump block3
block3:
 v4 = load.ptr v3
 return v4
}";
    let m = optimize(src);
    assert_eq!(memory(&m.functions[0]), 0);
}
#[test]
fn rejects_partial_initialization_overlap_escape_volatile_atomic_and_nonlocal_calls() {
    let original = fixture(Type::I64);
    for src in [
        original.replace("store v1, v2", ""),
        original.replace("store v1, v2", "v11 = iconst.i8 0\n store v11, v2"),
        original.replace("store v1, v2", "store volatile v1, v2"),
        original.replace("load.i64 v2", "load.i64 volatile v2"),
        format!(
            "decl @memset(ptr, i32, i64) -> ptr\n{}",
            original.replace(
                "store v1, v2",
                "v11 = iconst.i32 0\n v12 = iconst.i64 8\n v13 = call @memset(v2, v11, v12)"
            )
        ),
        format!(
            "decl @save() returns_twice\n{}",
            original.replace("store v1, v2", "store v1, v2\n call @save()")
        ),
        original.replace(
            "store v1, v2",
            "store v1, v2\n v11 = atomic_load.i64 relaxed v2",
        ),
        original.replace(
            "store v1, v2",
            "store v1, v2\n v11 = intrinsic frame_address()",
        ),
        original.replace("return v10", "return v10\nblock4:\n jump block1(v1)"),
    ] {
        let before = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&before).unwrap();
        assert_eq!(before.to_string(), optimize(&src).to_string(), "{src}");
    }
}
#[test]
fn zero_trip_path_must_be_initialized_and_unrelated_calls_remain() {
    let src = fixture(Type::I64).replace("store v1, v2", "");
    let src = src.replace("v6 = load.i64 v2", "store v1, v2\n v6 = load.i64 v2");
    assert_eq!(memory(&optimize(&src).functions[0]), 4);
    let src = format!(
        "decl @effect()\n{}",
        fixture(Type::I64).replace("v6 = load.i64 v2", "call @effect()\n v6 = load.i64 v2")
    );
    let m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), D::Call { .. }))
            .count(),
        1
    );
}
#[test]
fn debugger_homes_and_aliased_load_definitions_are_preserved() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut m = keel_ir::parse(&fixture(Type::I64)).unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "word".into(),
        encoding: Encoding::Unsigned,
        size: 8,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "home".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["stack-promote"]).unwrap();
    assert_eq!(before, m.to_string());
    let mut m = keel_ir::parse(&fixture(Type::I64)).unwrap();
    let f = &mut m.functions[0];
    let i = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Load { .. }))
        .unwrap()
        .1;
    let result = f.first_result(i);
    let replacement = f.block_params(f.entry_block().unwrap())[1];
    f.replace_all_uses(result, replacement);
    keel_opt::find_pass("stack-promote").unwrap().run(f);
    assert_eq!(f.resolve(result), replacement);
    assert_eq!(memory(f), 4);
}
#[test]
fn remains_opt_in() {
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"stack-promote"));
    }
}

#[test]
fn one_arm_initialization_is_insufficient_even_when_backedge_writes() {
    let src = fixture(Type::I64).replace(
        "store v1, v2",
        "brif v0, block4, block5\nblock4:\n store v1,v2\n jump block5\nblock5:",
    );
    let before = keel_ir::parse(&src).unwrap();
    assert_eq!(before.to_string(), optimize(&src).to_string());
}

#[test]
fn exception_flow_indirect_returns_twice_and_stack_lifetime_are_excluded() {
    let src = fixture(Type::I64);
    for effect in [
        "v11 = intrinsic stack_save()",
        "v11 = intrinsic stack_save()\n intrinsic stack_restore(v11)",
        "v11 = iconst.i64 1\n v12 = ptradd v2,v1\n store v11,v12",
    ] {
        let candidate = src.replace("store v1, v2", &format!("store v1,v2\n {effect}"));
        let before = keel_ir::parse(&candidate).unwrap();
        assert_eq!(before.to_string(), optimize(&candidate).to_string());
    }
    let src = "decl @effect() -> i64\ndecl @personality()\nfunc @f(ptr, i64) -> i64 personality @personality {
 ss0 = stack_slot 8, align 8
block0(v0: ptr,v1: i64):
 v2 = stack_addr ss0
 store v1,v2
 v3 = invoke @effect(),block1,block2
block1:
 v4 = load.i64 v2
 return v4
block2:
 v5,v6 = landingpad cleanup
 trap
}";
    let before = keel_ir::parse(src).unwrap();
    assert_eq!(before.to_string(), optimize(src).to_string());
    let src = "func @f(ptr, i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: ptr,v1: i64):
 v2 = stack_addr ss0
 store v1,v2
 call_indirect v0() : () returns_twice
 jump block1
block1:
 v3 = load.i64 v2
 return v3
}";
    let before = keel_ir::parse(src).unwrap();
    assert_eq!(before.to_string(), optimize(src).to_string());
}
