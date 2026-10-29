#[path = "if_convert/if_convert_nested.rs"]
mod if_convert_nested;

mod support;
use keel_ir::{InstData as D, Type};
fn apply(src: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["if-convert"]).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
fn branches(m: &keel_ir::Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), D::Brif { .. }))
        .count()
}
fn diamond(ty: Type) -> String {
    format!(
        "func @f(i8, {ty}, {ty}) -> {ty} {{
block0(v0: i8, v1: {ty}, v2: {ty}):
 brif v0, block1(v1,v2), block2(v2,v1)
block1(v3: {ty}, v4: {ty}):
 v5 = iadd v3, v4
 jump block3(v5,v3)
block2(v6: {ty}, v7: {ty}):
 v8 = isub v6, v7
 jump block3(v8,v6)
block3(v9: {ty}, v10: {ty}):
 v11 = xor v9, v10
 return v11
}}"
    )
}
#[test]
fn wrapping_widths_polarity_and_simultaneous_parameter_substitution() {
    for ty in [Type::I8, Type::I16, Type::I32, Type::I64] {
        let src = diamond(ty);
        let m = apply(&src);
        assert_eq!(branches(&m), 0);
        assert_eq!(m.functions[0].blocks().count(), 2);
        let before = m.to_string();
        let mut twice = m.clone();
        keel_opt::run_passes(&mut twice, &["if-convert"]).unwrap();
        assert_eq!(before, twice.to_string());
        for tier in [
            keel_opt::OptimizationTier::Fast,
            keel_opt::OptimizationTier::Full,
        ] {
            let mut m = m.clone();
            keel_opt::optimize_with_tier(&mut m, tier).unwrap();
            let mask = u64::MAX >> (64 - ty.bytes().unwrap() * 8);
            for cond in [0, 1, 2, 128, 255] {
                for a in [0u64, 1, 127, 128, 255, 256, 65535, 1 << 63, u64::MAX] {
                    for b in [0u64, 1, 127, 255, 1 << 63, u64::MAX] {
                        let (a, b) = (a & mask, b & mask);
                        let expected = if cond != 0 {
                            a.wrapping_add(b) ^ a
                        } else {
                            b.wrapping_sub(a) ^ b
                        } & mask;
                        assert_eq!(support::execute(&m.functions[0], &[cond, a, b]), expected);
                    }
                }
            }
        }
    }
}
#[test]
fn signed_and_unsigned_comparisons_remain_distinct() {
    for (cc, signed) in [("slt", true), ("ult", false)] {
        let src =
            diamond(Type::I64).replace("brif v0", &format!("v12 = icmp.{cc} v1, v2\n brif v12"));
        let m = apply(&src);
        assert_eq!(branches(&m), 0);
        for a in [0u64, 1, 1 << 63, u64::MAX] {
            for b in [0u64, 1, 1 << 63, u64::MAX] {
                let cond = if signed {
                    (a as i64) < (b as i64)
                } else {
                    a < b
                };
                let expected = if cond {
                    a.wrapping_add(b) ^ a
                } else {
                    b.wrapping_sub(a) ^ b
                };
                assert_eq!(support::execute(&m.functions[0], &[0, a, b]), expected);
            }
        }
    }
}
#[test]
fn empty_arms_duplicate_edges_and_pointer_edge_values() {
    let src = "func @f(i8, i64, i64) -> i64 {
block0(v0: i8, v1: i64, v2: i64):
 brif v0, block1(v1,v2), block2(v2,v1)
block1(v3: i64, v4: i64):
 jump block3(v3,v4)
block2(v5: i64, v6: i64):
 jump block3(v5,v6)
block3(v7: i64, v8: i64):
 v9 = isub v7,v8
 return v9
}";
    let m = apply(src);
    assert_eq!(branches(&m), 0);
    for cond in [0, 1, 255] {
        assert_eq!(
            support::execute(&m.functions[0], &[cond, 3, 7]),
            if cond != 0 { 3u64.wrapping_sub(7) } else { 4 }
        );
    }
    let src = "func @f(i8, ptr, ptr) -> ptr {
block0(v0: i8, v1: ptr, v2: ptr):
 brif v0, block1(v1), block1(v2)
block1(v3: ptr):
 return v3
}";
    let m = apply(src);
    assert_eq!(branches(&m), 0);
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), D::Select { .. }))
            .count(),
        1
    );
}
#[test]
fn identical_edges_do_not_introduce_selects() {
    let src = "func @f(i8,i64) -> i64 {
block0(v0: i8,v1: i64):
 brif v0,block1(v1),block1(v1)
block1(v2: i64):
 return v2
}";
    let m = apply(src);
    assert_eq!(branches(&m), 0);
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), D::Select { .. }))
    );
}
#[test]
fn does_not_speculate_loads_calls_division_stores_atomics_or_traps() {
    let simple = diamond(Type::I64);
    for (decl, operation) in [
        ("", "v5 = udiv v3, v4"),
        ("", "v5 = sdiv v3, v4"),
        ("", "v5 = urem v3, v4"),
        ("", "v5 = srem v3, v4"),
        ("decl @effect(i64) -> i64\n", "v5 = call @effect(v3)"),
        (
            "decl @effect(i64) -> i64 returns_twice\n",
            "v5 = call @effect(v3)",
        ),
    ] {
        let src = format!("{decl}{}", simple.replace("v5 = iadd v3, v4", operation));
        let before = keel_ir::parse(&src).unwrap();
        assert_eq!(before.to_string(), apply(&src).to_string());
    }
    let pointer = "func @f(i8,ptr,i64) -> i64 {
block0(v0: i8,v1: ptr,v2: i64):
 brif v0,block1,block2
block1:
 OP
 jump block3(v3)
block2:
 jump block3(v2)
block3(v4: i64):
 return v4
}";
    for op in [
        "v3 = load.i64 v1",
        "v3 = load.i64 volatile v1",
        "store v2,v1\n v3 = iconst.i64 0",
        "v3 = atomic_load.i64 relaxed v1",
    ] {
        let src = pointer.replace("OP", op);
        let before = keel_ir::parse(&src).unwrap();
        assert_eq!(before.to_string(), apply(&src).to_string());
    }
    let src = simple.replace("v5 = iadd v3, v4\n jump block3(v5,v3)", "trap");
    assert_eq!(branches(&apply(&src)), 1);
    // An untaken zero-divisor arm must still be untaken after the pass.
    let src = simple.replace("v5 = iadd v3, v4", "v5 = udiv v3, v4");
    let m = apply(&src);
    assert_eq!(
        support::execute(&m.functions[0], &[0, 17, 0]),
        0u64.wrapping_sub(17)
    );
}
#[test]
fn respects_size_and_single_predecessor_bounds() {
    let src = diamond(Type::I64).replace("v5 = iadd", "v12 = iadd v3,v4\n v13 = iadd v12,v4\n v14 = iadd v13,v4\n v15 = iadd v14,v4\n v16 = iadd v15,v4\n v17 = iadd v16,v4\n v18 = iadd v17,v4\n v5 = iadd");
    assert_eq!(branches(&apply(&src)), 1);
    let src =
        diamond(Type::I64).replace("return v11", "return v11\nblock4:\n jump block1(v11,v11)");
    // An unreachable extra predecessor still prevents moving arm definitions.
    assert_eq!(branches(&apply(&src)), 1);
    let params = (3..12)
        .map(|n| format!("v{n}: i64"))
        .collect::<Vec<_>>()
        .join(",");
    let yes = ["v1"; 9].join(",");
    let no = ["v2"; 9].join(",");
    let src = format!(
        "func @f(i8,i64,i64) -> i64 {{
block0(v0: i8,v1: i64,v2: i64):
 brif v0,block1({yes}),block1({no})
block1({params}):
 return v3
}}"
    );
    assert_eq!(branches(&apply(&src)), 1); // bounded select count as well as arm cost
}
#[test]
fn aliased_definitions_and_join_parameters_are_rejected_without_overwriting() {
    use keel_ir::{EntityRef, Value};
    for alias_join in [false, true] {
        let mut m = keel_ir::parse(&diamond(Type::I64)).unwrap();
        let f = &mut m.functions[0];
        let from = Value::new(if alias_join { 9 } else { 5 });
        let to = Value::new(1);
        f.replace_all_uses(from, to);
        let before = f.to_string();
        assert!(!keel_opt::find_pass("if-convert").unwrap().run(f));
        assert_eq!(f.to_string(), before);
        assert_eq!(f.resolve(from), to);
    }
}
#[test]
fn promotion_then_conversion_activates_on_private_loop_diamond() {
    let src = include_str!("stack_promote/private.keel");
    let mut m = keel_ir::parse(src).unwrap();
    keel_opt::run_passes(
        &mut m,
        &["stack-promote", "phi-simplify", "dce", "if-convert"],
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(branches(&m), 1); // loop exit only
    assert!(
        m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), D::Select { .. }))
    );
}
#[test]
fn remains_opt_in() {
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"if-convert"));
    }
}

#[test]
fn exception_and_indirect_nonlocal_arms_stay_conditional() {
    let src = "decl @effect() -> i64\ndecl @personality()\nfunc @f(i8,ptr,i64) -> i64 personality @personality {
block0(v0: i8,v1: ptr,v2: i64):
 brif v0,block1,block2
block1:
 v3 = invoke @effect(),block3,block4
block2:
 jump block5(v2)
block3:
 jump block5(v3)
block4:
 v4,v5 = landingpad cleanup
 trap
block5(v6: i64):
 return v6
}";
    let before = keel_ir::parse(src).unwrap();
    assert_eq!(before.to_string(), apply(src).to_string());
    let src = "func @f(i8,ptr,i64) -> i64 {
block0(v0: i8,v1: ptr,v2: i64):
 brif v0,block1,block2
block1:
 v3 = call_indirect v1() : () -> i64 returns_twice
 jump block3(v3)
block2:
 jump block3(v2)
block3(v4: i64):
 return v4
}";
    let before = keel_ir::parse(src).unwrap();
    assert_eq!(before.to_string(), apply(src).to_string());
}

#[test]
fn hoisted_instruction_source_locations_are_retained() {
    let mut m = keel_ir::parse(&diamond(Type::I64)).unwrap();
    let file = m.add_file("diamond.c");
    let f = &mut m.functions[0];
    let arm = f
        .layout_insts()
        .find(|(_, i)| {
            matches!(
                f.inst_data(*i),
                D::Binary {
                    op: keel_ir::BinaryOp::Iadd,
                    ..
                }
            )
        })
        .unwrap()
        .1;
    let location = keel_ir::SourceLoc {
        file,
        line: 12,
        col: 5,
        scope: None,
    };
    f.set_inst_loc(arm, Some(location));
    keel_opt::find_pass("if-convert").unwrap().run(f);
    assert_eq!(f.inst_loc(arm), Some(location));
    assert_eq!(f.inst_block(arm), f.entry_block().unwrap());
    keel_ir::verify_module(&m).unwrap();
}
