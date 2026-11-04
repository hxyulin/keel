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
fn full_only_default_admission() {
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert_eq!(
            tier.pipeline().contains(&"stack-promote"),
            tier == keel_opt::OptimizationTier::Full
        );
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

fn covered_fixture(wide: Type, field: Type, offset: usize, byte: u8) -> String {
    let bits = wide.bytes().unwrap() * 8;
    let imm = ((u64::MAX / 255 * u64::from(byte)) >> (64 - bits)) as i64;
    fixture(field)
        .replace(
            "v2 = stack_addr ss0",
            &format!("v20 = stack_addr ss0\n v21 = iconst.i64 {offset}\n v2 = ptradd v20, v21"),
        )
        .replace(
            "store v1, v2",
            &format!("v22 = iconst.{wide} {imm}\n store v22, v20"),
        )
}

#[test]
fn byte_splat_initialization_promotes_every_subfield_width_and_offset() {
    for wide in [Type::I16, Type::I32, Type::I64] {
        for field in [Type::I8, Type::I16, Type::I32] {
            if field.bytes().unwrap() >= wide.bytes().unwrap() {
                continue;
            }
            for offset in 0..=(wide.bytes().unwrap() - field.bytes().unwrap()) as usize {
                for byte in [0, 1, 127, 128, 255] {
                    let src = covered_fixture(wide, field, offset, byte);
                    let mut m = optimize(&src);
                    assert_eq!(memory(&m.functions[0]), 0, "{src}\n{m}");
                    let once = m.to_string();
                    keel_opt::run_passes(&mut m, &["stack-promote"]).unwrap();
                    assert_eq!(once, m.to_string());
                    keel_opt::run_passes(&mut m, &["dce"]).unwrap();
                    let bits = field.bytes().unwrap() * 8;
                    let mask = u64::MAX >> (64 - bits);
                    let init = (u64::MAX / 255 * u64::from(byte)) & mask;
                    for n in [0u64, 1, 2, 17, 63] {
                        let expected = init.wrapping_add(n * n.saturating_sub(1) / 2) & mask;
                        assert_eq!(support::execute(&m.functions[0], &[n, 99]), expected);
                    }
                    // Exercise the production Full epilogue, not just the named pass.
                    let mut full = keel_ir::parse(&src).unwrap();
                    keel_opt::optimize_with_tier(&mut full, keel_opt::OptimizationTier::Full)
                        .unwrap();
                    assert_eq!(memory(&full.functions[0]), 0, "{full}");
                    assert_eq!(support::execute(&full.functions[0], &[0, 99]), init);
                }
            }
        }
    }
}

#[test]
fn one_wide_initializer_feeds_two_independent_loop_fields() {
    let src = covered_fixture(Type::I64, Type::I32, 0, 255)
        .replace(
            "store v22, v20",
            "store v22, v20\n v23 = iconst.i64 4\n v24 = ptradd v20, v23",
        )
        .replace(
            "v6 = load.i32 v2",
            "v25 = load.i32 v24\n v26 = iadd v25, v4\n store v26, v24\n v6 = load.i32 v2",
        )
        .replace(
            "return v10",
            "v27 = load.i32 v24\n v28 = iadd v10, v27\n return v28",
        );
    let mut m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0, "{m}");
    keel_opt::run_passes(&mut m, &["dce"]).unwrap();
    for n in [0u64, 1, 2, 31] {
        let expected = 2u64.wrapping_mul(u64::from(u32::MAX) + n * n.saturating_sub(1) / 2)
            & u64::from(u32::MAX);
        assert_eq!(support::execute(&m.functions[0], &[n, 0]), expected);
    }
}

#[test]
fn partial_overlaps_unknown_and_nonuniform_initializers_are_not_reinterpreted() {
    let src = covered_fixture(Type::I64, Type::I32, 0, 0);
    for candidate in [
        // Both ends of a partially covering store must be rejected.
        covered_fixture(Type::I16, Type::I32, 0, 0),
        covered_fixture(Type::I64, Type::I32, 6, 0),
        src.replace("v22 = iconst.i64 0", "v22 = iconst.i64 4294967296"),
        src.replace("v22 = iconst.i64 0", "v22 = uext.i64 v1"),
        // An overlapping read would observe the promoted stores.
        src.replace("return v10", "v23 = load.i64 v20\n return v10"),
        // An overlapping partial write must invalidate the entire field.
        src.replace(
            "v6 = load.i32 v2",
            "v23 = iconst.i8 9\n store v23, v2\n v6 = load.i32 v2",
        ),
        // Covering writes with unknown bits later in the loop also disqualify.
        src.replace(
            "v6 = load.i32 v2",
            "v23 = uext.i64 v1\n store v23, v20\n v6 = load.i32 v2",
        ),
        // A loop write cannot define the value on a zero-trip exit.
        src.replace("store v22, v20", "")
            .replace("v6 = load.i32 v2", "store v22, v20\n v6 = load.i32 v2"),
    ] {
        let before = keel_ir::parse(&candidate).unwrap();
        assert_eq!(
            before.to_string(),
            optimize(&candidate).to_string(),
            "{candidate}"
        );
    }
}

#[test]
fn covering_store_is_retained_for_unpromoted_neighbor_bytes() {
    let src = covered_fixture(Type::I64, Type::I32, 0, 127)
        .replace("return v10", "v23 = iconst.i64 6\n v24 = ptradd v20, v23\n v25 = load.i16 v24\n v26 = iconst.i8 9\n store v26, v24\n v27 = uext.i32 v25\n v28 = iadd v10, v27\n return v28");
    let m = optimize(&src);
    // The i32 recurrence is in SSA, while the overlapping i16/i8 neighbor
    // accesses and the complete original i64 initializer remain in memory.
    assert_eq!(memory(&m.functions[0]), 3, "{m}");
    assert!(m.functions[0].layout_insts().any(|(_, i)| matches!(
        m.functions[0].inst_data(i), D::Store { value, .. }
        if m.functions[0].value_type(*value) == Type::I64)));
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), D::Load { ty: Type::I32, .. }))
    );
}

#[test]
fn mixed_width_fields_keep_escape_observer_and_exception_barriers() {
    let exact = fixture(Type::I32);
    let covered = covered_fixture(Type::I64, Type::I32, 0, 0);
    // Reuse every existing barrier regression with the wider initializer.
    // Memory and nonlocal observers still exclude the function.
    for effect in [
        "v23 = load.i8 volatile v20",
        "v23 = atomic_load.i8 relaxed v20",
        "v23 = intrinsic frame_address()",
        "v23 = intrinsic stack_save()",
        "call @twice()",
    ] {
        let candidate = format!(
            "decl @observe(ptr)\ndecl @twice() returns_twice\ndecl @memset(ptr, i32, i64) -> ptr\n{}",
            covered.replace("store v22, v20", &format!("store v22, v20\n {effect}"))
        );
        let before = keel_ir::parse(&candidate).unwrap();
        assert_eq!(
            before.to_string(),
            optimize(&candidate).to_string(),
            "{candidate}"
        );
    }
    // Wider initialization does not bypass the existing debug-home guard.
    let mut m = keel_ir::parse(&covered).unwrap();
    let mut exact_m = keel_ir::parse(&exact).unwrap();
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    for module in [&mut m, &mut exact_m] {
        module.debug_types.push(DebugTypeData::Base {
            name: "field".into(),
            encoding: Encoding::Unsigned,
            size: 4,
        });
        module.functions[0].debug_vars.push(DebugVar {
            name: "home".into(),
            ty: DebugType::new(0),
            slot: StackSlot::new(0),
            indirect: false,
            param: false,
            scope: None,
        });
        let before = module.to_string();
        keel_opt::run_passes(module, &["stack-promote"]).unwrap();
        assert_eq!(before, module.to_string());
    }
}

#[test]
fn covering_writes_inside_loop_reset_the_promoted_value_in_order() {
    for after in [false, true] {
        let src = covered_fixture(Type::I64, Type::I32, 0, 255);
        let src = if after {
            src.replace("store v7, v2", "store v7, v2\n store v22, v20")
        } else {
            src.replace("v6 = load.i32 v2", "store v22, v20\n v6 = load.i32 v2")
        };
        let mut m = optimize(&src);
        assert_eq!(memory(&m.functions[0]), 0, "{m}");
        keel_opt::run_passes(&mut m, &["dce"]).unwrap();
        for n in [0u64, 1, 2, 31] {
            let expected = if after || n == 0 {
                u32::MAX
            } else {
                u32::MAX.wrapping_add(n as u32 - 1)
            };
            assert_eq!(
                support::execute(&m.functions[0], &[n, 0]),
                u64::from(expected)
            );
        }
    }
}

#[test]
fn split_initializer_constants_keep_source_location_and_scope() {
    use keel_ir::EntityRef;
    let mut m = keel_ir::parse(&covered_fixture(Type::I64, Type::I32, 0, 128)).unwrap();
    let f = &mut m.functions[0];
    let store = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Store { .. }))
        .unwrap()
        .1;
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 27,
        col: 5,
        scope: None,
    };
    f.set_inst_loc(store, Some(loc));
    let old: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    keel_opt::run_passes(&mut m, &["stack-promote"]).unwrap();
    let f = &m.functions[0];
    let new: Vec<_> = f.layout_insts().filter(|(_, i)| !old.contains(i)).collect();
    assert_eq!(new.len(), 1);
    assert_eq!(f.inst_loc(new[0].1), Some(loc));
    assert!(matches!(
        f.inst_data(new[0].1),
        D::Iconst {
            ty: Type::I32,
            imm: 2155905152
        }
    ));
}
