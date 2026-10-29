//! Direct edges and nested ternaries, independent of block layout order.
use crate::support;
use keel_ir::{EntityRef, InstData as D, Value};
fn apply(src: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["if-convert"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
fn branches(m: &keel_ir::Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), D::Brif { .. }))
        .count()
}
const TRIANGLE: &str = "func @f(i8,i64,i64) -> i64 {
block0(v0: i8,v1: i64,v2: i64):
 brif v0,block1(v2,v1),block2(v1,v2)
block1(v3: i64,v4: i64):
 v5 = isub v3,v4
 jump block2(v5,v3)
block2(v6: i64,v7: i64):
 v8 = xor v6,v7
 return v8
}";
#[test]
fn triangles_both_polarities_and_permuted_arguments() {
    for reverse in [false, true] {
        let src = if reverse {
            TRIANGLE.replace("block1(v2,v1),block2(v1,v2)", "block2(v1,v2),block1(v2,v1)")
        } else {
            TRIANGLE.to_owned()
        };
        let before = keel_ir::parse(&src).unwrap();
        let after = apply(&src);
        assert_eq!(branches(&after), 0);
        for c in [0, 1, 2, 128, 255] {
            for a in [0, 1, 127, 128, 1 << 63, u64::MAX] {
                for b in [0, 1, 128, 255, 1 << 63, u64::MAX] {
                    assert_eq!(
                        support::execute(&before.functions[0], &[c, a, b]),
                        support::execute(&after.functions[0], &[c, a, b])
                    );
                }
            }
        }
    }
}
// Deliberately put the outer join before the nested branch in layout. The
// inner join forwards through a parameter permutation to the outer join.
const NESTED: &str = "func @f(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 -128
 v2 = icmp.slt v0,v1
 v3 = iconst.i64 0
 brif v2,block4(v3),block1(v0)
block4(v14: i64):
 return v14
block1(v4: i64):
 v5 = iconst.i64 127
 v6 = icmp.sgt v4,v5
 v7 = iconst.i64 255
 brif v6,block3(v7,v4),block2(v4)
block2(v8: i64):
 v9 = iconst.i64 128
 v10 = iadd v8,v9
 jump block3(v10,v8)
block3(v11: i64,v12: i64):
 jump block4(v11)
}";
#[test]
fn nested_clamp_uses_wrapping_add_and_is_layout_independent() {
    let m = apply(NESTED);
    assert_eq!(branches(&m), 0);
    assert_eq!(m.functions[0].blocks().count(), 2);
    for x in [
        i64::MIN,
        -129,
        -128,
        -127,
        -1,
        0,
        1,
        126,
        127,
        128,
        i64::MAX,
    ] {
        assert_eq!(
            support::execute(&m.functions[0], &[x as u64]),
            (i128::from(x) + 128).clamp(0, 255) as u64
        );
    }
    let mut again = m.clone();
    keel_opt::run_passes(&mut again, &["if-convert"]).unwrap();
    assert_eq!(again.to_string(), m.to_string());
}
#[test]
fn triangle_rejects_effects_traps_fp_and_noncanonical_definitions() {
    for (decl, op) in [
        ("", "v5 = sdiv v3,v4"),
        ("", "v5 = urem v3,v4"),
        ("decl @effect(i64) -> i64\n", "v5 = call @effect(v3)"),
        (
            "",
            "v9 = fconst.f64 1.0\n v10 = fadd v9,v9\n v5 = isub v3,v4",
        ),
        ("", "trap"),
    ] {
        let body = if op == "trap" {
            TRIANGLE.replace("v5 = isub v3,v4\n jump block2(v5,v3)", op)
        } else {
            TRIANGLE.replace("v5 = isub v3,v4", op)
        };
        let src = format!("{decl}{body}");
        let before = keel_ir::parse(&src).unwrap();
        assert_eq!(apply(&src).to_string(), before.to_string());
    }
    for value in [3, 5, 6] {
        let mut m = keel_ir::parse(TRIANGLE).unwrap();
        m.functions[0].replace_all_uses(Value::new(value), Value::new(1));
        let before = m.to_string();
        assert!(
            !keel_opt::find_pass("if-convert")
                .unwrap()
                .run(&mut m.functions[0])
        );
        assert_eq!(m.to_string(), before);
    }
}
#[test]
fn outside_uses_and_unreachable_predecessors_prevent_motion() {
    for tail in ["block9:\n jump block1(v1,v2)", "block9:\n return v5"] {
        let src = TRIANGLE.replace("\n}", &format!("\n{tail}\n}}"));
        let before = keel_ir::parse(&src).unwrap();
        // Outside uses in unreachable blocks are deliberately inspected even
        // if a dominance checker ignores those blocks.
        let mut after = before.clone();
        assert!(
            !keel_opt::find_pass("if-convert")
                .unwrap()
                .run(&mut after.functions[0])
        );
        assert_eq!(after.to_string(), before.to_string());
    }
}
#[test]
fn nested_conversion_does_not_cross_a_trap_guard() {
    let src = NESTED.replace("v10 = iadd v8,v9", "v10 = sdiv v8,v9");
    let m = apply(&src);
    assert_eq!(branches(&m), 2);
    // The divisor is zero only on an untaken arm for these inputs.
    let src = src.replace("v9 = iconst.i64 128", "v9 = iconst.i64 0");
    let m = apply(&src);
    for x in [i64::MIN, -129, 128, i64::MAX] {
        assert_eq!(
            support::execute(&m.functions[0], &[x as u64]),
            if x < 0 { 0 } else { 255 }
        );
    }
}
#[test]
fn private_chain_budget_and_inner_select_cost_are_bounded() {
    let extra = (20..28)
        .map(|n| format!("v{n} = iadd v8,v8\n "))
        .collect::<String>();
    let src = NESTED.replace(
        "v9 = iconst.i64 128",
        &format!("{extra}v9 = iconst.i64 128"),
    );
    assert_eq!(branches(&apply(&src)), 2);
    let chain = (10..20)
        .map(|n| format!("block{n}:\n jump block{}\n", n + 1))
        .collect::<String>();
    let src = TRIANGLE
        .replace("jump block2(v5,v3)", "jump block10")
        .replace(
            "\n}",
            &format!("\n{chain}block20:\n jump block2(v5,v3)\n}}"),
        );
    assert_eq!(branches(&apply(&src)), 1);
}

#[test]
fn forwarding_chains_rewrite_each_parameter_permutation_simultaneously() {
    let src = "func @f(i8,i64,i64) -> i64 {
block0(v0: i8,v1: i64,v2: i64):
 brif v0,block1(v2,v1),block4(v1,v2)
block1(v3: i64,v4: i64):
 jump block2(v4,v3)
block2(v5: i64,v6: i64):
 v7 = isub v6,v5
 jump block3(v6,v7)
block3(v8: i64,v9: i64):
 jump block4(v9,v8)
block4(v10: i64,v11: i64):
 v12 = xor v10,v11
 return v12
}";
    for alias in [false, true] {
        let mut before = keel_ir::parse(src).unwrap();
        if alias {
            before.functions[0].replace_all_uses(Value::new(2), Value::new(1));
        }
        let mut after = before.clone();
        keel_opt::run_passes(&mut after, &["if-convert"]).unwrap();
        keel_ir::verify_module(&after).unwrap();
        assert_eq!(branches(&after), 0);
        for c in [0, 1, 255] {
            for a in [0, 1, 1 << 63, u64::MAX] {
                for b in [0, 7, 1 << 63, u64::MAX] {
                    assert_eq!(
                        support::execute(&before.functions[0], &[c, a, b]),
                        support::execute(&after.functions[0], &[c, a, b])
                    );
                }
            }
        }
    }
}

#[test]
fn triangles_do_not_speculate_memory_or_volatile_accesses() {
    let src = "func @f(i8,ptr,i64) -> i64 {
block0(v0: i8,v1: ptr,v2: i64):
 brif v0,block1,block2(v2)
block1:
 OP
 jump block2(v3)
block2(v4: i64):
 return v4
}";
    for op in [
        "v3 = load.i64 v1",
        "v3 = load.i64 volatile v1",
        "store v2,v1\n v3 = iconst.i64 0",
        "v3 = atomic_load.i64 relaxed v1",
    ] {
        let src = src.replace("OP", op);
        let before = keel_ir::parse(&src).unwrap();
        assert_eq!(apply(&src).to_string(), before.to_string());
    }
}
