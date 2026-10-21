mod support;
// Keep the pass independently testable without changing root-owned registration.
#[path = "../src/adjacent_abs.rs"]
mod adjacent_abs;
use keel_ir::{EntityRef, InstData as D, Type};

fn triangle(ty: Type) -> String {
    format!(
        "func @abs({ty}) -> {ty} {{
block0(v0: {ty}):
 v1 = iconst.{ty} 0
 v2 = icmp.slt v0, v1
 brif v2, block1, block2(v0)
block1:
 v3 = ineg v0
 jump block2(v3)
block2(v4: {ty}):
 return v4
}}"
    )
}
fn apply(src: &str, expected: bool) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(adjacent_abs::run(&mut m.functions[0]), expected);
    keel_ir::verify_module(&m).unwrap();
    let once = m.to_string();
    assert!(!adjacent_abs::run(&mut m.functions[0]));
    assert_eq!(once, m.to_string());
    keel_ir::verify_module(&keel_ir::parse(&once).unwrap()).unwrap();
    m
}
#[test]
fn exhaustive_i8_and_wide_wrapping_extremes() {
    for ty in [Type::I8, Type::I16, Type::I32, Type::I64] {
        let src = triangle(ty);
        let before = keel_ir::parse(&src).unwrap();
        let after = apply(&src, true);
        let bits = ty.bytes().unwrap() * 8;
        let mask = u64::MAX >> (64 - bits);
        for x in (0..256).chain([1u64 << (bits - 1), (1u64 << (bits - 1)) - 1, mask, mask - 1]) {
            let x = x & mask;
            let expected = if x & (1 << (bits - 1)) != 0 {
                x.wrapping_neg() & mask
            } else {
                x
            };
            assert_eq!(support::execute(&before.functions[0], &[x]), expected);
            assert_eq!(support::execute(&after.functions[0], &[x]), expected);
        }
    }
}
#[test]
fn signed_i32_difference_is_never_narrowed() {
    let src = triangle(Type::I64)
        .replace("@abs(i64)", "@abs(i32, i32)")
        .replace(
            "block0(v0: i64):",
            "block0(v5: i32, v6: i32):\n v7 = sext.i64 v5\n v8 = sext.i64 v6\n v0 = isub v7, v8",
        );
    let m = apply(&src, true);
    for a in [i32::MIN, i32::MAX, -1, 0, 1] {
        for b in [i32::MIN, i32::MAX, -1, 0, 1] {
            assert_eq!(
                support::execute(&m.functions[0], &[a as u32 as u64, b as u32 as u64]),
                (i64::from(a) - i64::from(b)).unsigned_abs()
            );
        }
    }
}

#[test]
fn resolves_input_aliases_but_rejects_aliased_negation_definitions() {
    for alias_negation in [false, true] {
        let mut m = keel_ir::parse(&triangle(Type::I64)).unwrap();
        let f = &mut m.functions[0];
        let head = f.entry_block().unwrap();
        let x = f.block_params(head)[0];
        let neg = f
            .layout_insts()
            .find(|(_, i)| matches!(f.inst_data(*i), D::Unary { .. }))
            .unwrap()
            .1;
        if alias_negation {
            f.replace_all_uses(f.inst_results(neg)[0], x);
        } else {
            let copy = f.insert_inst(
                keel_ir::InsertPoint::Before(neg),
                D::Unary {
                    op: keel_ir::UnaryOp::Ineg,
                    arg: x,
                },
            );
            // Put the alias definition in the head so the arm stays bounded.
            f.move_inst(
                copy,
                keel_ir::InsertPoint::Before(*f.block_insts(head).last().unwrap()),
            );
            let alias = f.inst_results(copy)[0];
            f.replace_all_uses(alias, x);
            if let D::Unary { arg, .. } = f.inst_data_mut(neg) {
                *arg = alias;
            }
        }
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(adjacent_abs::run(&mut m.functions[0]), !alias_negation);
        keel_ir::verify_module(&m).unwrap();
    }
}
#[test]
fn refuses_other_predicates_values_shared_or_effectful_arms() {
    let base = triangle(Type::I64);
    for src in [
        base.replace("icmp.slt", "icmp.ult"),
        base.replace("iconst.i64 0", "iconst.i64 1"),
        base.replace("ineg v0", "bnot v0"),
        base.replace("block2(v3)", "block2(v0)"),
        base.replace("v3 = ineg v0", "v3 = ineg v0\n v5 = iadd v0, v0"),
        base.replace("return v4", "jump block1"),
        base.replace("@abs(i64)", "@abs(i64, ptr)")
            .replace("block0(v0: i64):", "block0(v0: i64, v5: ptr):")
            .replace("v3 = ineg v0", "v6 = load.i64 v5\n v3 = ineg v0"),
        base.replace("@abs(i64)", "@abs(i64, ptr)")
            .replace("block0(v0: i64):", "block0(v0: i64, v5: ptr):")
            .replace("v3 = ineg v0", "store v0, v5\n v3 = ineg v0"),
    ] {
        let m = apply(&src, false);
        assert_eq!(m.to_string(), keel_ir::parse(&src).unwrap().to_string());
    }
}
#[test]
fn preserves_instruction_locations_and_load_order_in_real_adjacent() {
    let src = include_str!("adjacent_abs/adjacent.keel");
    let mut before = keel_ir::parse(&format!("file f0 = \"kernels.c\"\n{src}")).unwrap();
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 49,
        col: 9,
        scope: None,
    };
    let f = &mut before.functions[0];
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    for &i in &insts {
        f.set_inst_loc(i, Some(loc));
    }
    let neg = insts
        .iter()
        .copied()
        .find(|&i| matches!(f.inst_data(i), D::Unary { .. }))
        .unwrap();
    let branch = insts
        .iter()
        .copied()
        .find(|&i| matches!(f.inst_data(i), D::Brif { dests, .. } if dests[0].args.is_empty()))
        .unwrap();
    let memory = |f: &keel_ir::Function| {
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), D::Load { .. } | D::Store { .. }))
            .map(|(b, i)| (b, i, f.inst_data(i).clone(), f.inst_loc(i)))
            .collect::<Vec<_>>()
    };
    let old_memory = memory(f);
    assert!(adjacent_abs::run(f));
    assert_eq!(memory(f), old_memory);
    assert_eq!(f.inst_loc(neg), Some(loc));
    assert_eq!(f.inst_loc(branch), Some(loc));
    let select = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Select { .. }))
        .unwrap()
        .1;
    assert_eq!(f.inst_loc(select), Some(loc));
    keel_ir::verify_module(&before).unwrap();
    if let Some(dir) = std::env::var_os("KEEL_ADJACENT_EVIDENCE_DIR") {
        std::fs::write(
            std::path::PathBuf::from(dir).join("adjacent-after.keel"),
            before.to_string(),
        )
        .unwrap();
    }
}

#[test]
fn does_not_cross_volatile_atomic_call_or_write_barriers() {
    let base = include_str!("adjacent_abs/adjacent.keel");
    for src in [
        base.replace("load.i32", "load.i32 volatile"),
        base.replace("load.i32", "atomic_load.i32 relaxed"),
        base.replace("v14 = sext.i64 v13", "store v13, v12\n v14 = sext.i64 v13"),
        format!(
            "decl @barrier()\n{}",
            base.replace("v14 = sext.i64 v13", "call @barrier()\n v14 = sext.i64 v13")
        ),
    ] {
        let before = keel_ir::parse(&src).unwrap();
        let after = apply(&src, true);
        // All original instructions outside the removed arm stay in the same
        // block and sequence; only its terminator changes and ineg/select enter.
        let signature = |m: &keel_ir::Module| {
            let f = &m.functions[0];
            f.layout_insts()
                .filter(|(_, i)| {
                    !matches!(
                        f.inst_data(*i),
                        D::Unary { .. } | D::Select { .. } | D::Jump { .. } | D::Brif { .. }
                    )
                })
                .map(|(b, i)| (b, i, f.inst_data(i).clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(&before), signature(&after));
    }
}
