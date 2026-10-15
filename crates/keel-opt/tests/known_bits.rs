mod support;
use support::execute;

fn apply(source: &str, passes: &[&str]) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, passes).unwrap();
    (original, module)
}
fn check(original: &keel_ir::Module, module: &keel_ir::Module, inputs: &[u64]) {
    assert_eq!(
        execute(&original.functions[0], inputs),
        execute(&module.functions[0], inputs),
        "inputs={inputs:?}\n{module}"
    );
}

#[test]
fn masks_extensions_and_signed_unsigned_comparisons_cover_every_i8_input() {
    let predicates = [
        "eq", "ne", "ult", "ule", "ugt", "uge", "slt", "sle", "sgt", "sge",
    ];
    for mask in [0, 1, 15, 63, 127, 128, 192, 255] {
        for constant in [0, 1, 15, 127, 128, 255] {
            for predicate in predicates {
                let source = format!(
                    "func @f(i8) -> i8 {{
block0(v0: i8):
 v1 = iconst.i8 {mask}
 v2 = and v0, v1
 v3 = iconst.i8 {constant}
 v4 = icmp.{predicate} v2, v3
 return v4
}}"
                );
                let (original, module) = apply(&source, &["known-bits", "instcombine", "dce"]);
                for x in 0..256 {
                    check(&original, &module, &[x]);
                }
            }
        }
    }
    for extension in ["uext", "sext"] {
        let source = format!(
            "func @f(i8) -> i16 {{
block0(v0: i8):
 v1 = {extension}.i16 v0
 v2 = iconst.i16 255
 v3 = and v1, v2
 return v3
}}"
        );
        let (original, module) = apply(&source, &["known-bits", "dce"]);
        assert_eq!(module.to_string().contains(" = and"), extension == "sext");
        for x in 0..256 {
            check(&original, &module, &[x]);
        }
    }
}

#[test]
fn shifts_boolean_algebra_and_selects_preserve_all_i8_pairs() {
    for op in ["shl", "ushr", "sshr"] {
        for shift in [0, 1, 7, 8, 9, -1] {
            let source = format!(
                "func @f(i8, i8) -> i8 {{
block0(v0: i8, v1: i8):
 v2 = iconst.i8 15
 v3 = and v0, v2
 v4 = iconst.i64 {shift}
 v5 = {op} v3, v4
 v6 = bnot v1
 v7 = or v5, v6
 v8 = bnot v7
 v9 = and v8, v5
 v10 = icmp.ne v0, v1
 v11 = select v10, v9, v8
 return v11
}}"
            );
            let (original, module) = apply(&source, &["known-bits", "instcombine", "dce"]);
            for x in 0..256 {
                for y in 0..256 {
                    check(&original, &module, &[x, y]);
                }
            }
        }
    }
}

#[test]
fn block_parameters_merge_all_edges_and_loop_backedges_conservatively() {
    let source = "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 128
 v2 = iconst.i8 0
 v3 = iconst.i8 1
 jump block1(v1, v0)
block1(v4: i8, v5: i8):
 v6 = icmp.eq v5, v2
 brif v6, block3(v4), block2
block2:
 v7 = or v4, v1
 v8 = isub v5, v3
 jump block1(v7, v8)
block3(v9: i8):
 v10 = icmp.ult v9, v1
 return v10
}";
    let (original, module) = apply(source, &["known-bits", "instcombine", "dce"]);
    assert!(!module.to_string().contains("icmp.ult"));
    for x in 0..256 {
        check(&original, &module, &[x]);
    }
    // Both edges of one predecessor must participate in the merge.
    let source = "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 0
 v2 = iconst.i8 128
 brif v0, block1(v1), block1(v2)
block1(v3: i8):
 v4 = icmp.ult v3, v2
 return v4
}";
    let (original, module) = apply(source, &["known-bits"]);
    assert!(module.to_string().contains("icmp.ult"));
    for x in 0..256 {
        check(&original, &module, &[x]);
    }
}

#[test]
fn sccp_aliases_layout_order_and_i128_immediates_are_safe() {
    let source = "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 15
 jump block2(v1)
block1:
 v4 = and v0, v3
 return v4
block2(v2: i8):
 v3 = or v2, v1
 jump block1
}";
    let (original, module) = apply(
        source,
        &["sccp", "known-bits", "instcombine", "simplify-cfg", "dce"],
    );
    for x in 0..256 {
        check(&original, &module, &[x]);
    }
    let source = "func @f(i128) -> i128 {
block0(v0: i128):
 v1 = iconst.i128 -1
 v2 = iconst.i64 0
 v3 = shl v1, v2
 return v3
}";
    let (_, mut module) = apply(source, &["known-bits"]);
    assert!(module.to_string().contains("iconst.i128 -1"));
    assert!(
        !keel_opt::find_pass("known-bits")
            .unwrap()
            .run(&mut module.functions[0])
    );
    let source = source.replace("iconst.i64 0", "iconst.i64 64");
    let (_, module) = apply(&source, &["known-bits"]);
    assert!(
        module.to_string().contains("shl"),
        "unrepresentable i128 constant must remain an operation"
    );
}

#[test]
fn late_combining_and_cfg_cleanup_finish_what_gvn_exposes() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iadd v0, v1
 v3 = iadd v1, v0
 v4 = icmp.eq v2, v3
 brif v4, block1, block2
block1:
 v5 = iconst.i64 7
 return v5
block2:
 v6 = iconst.i64 9
 return v6
}";
    let (original, old) = apply(source, keel_opt::DEFAULT_PIPELINE);
    assert!(old.to_string().contains("icmp.eq"));
    let (_, improved) = apply(
        source,
        &[
            "simplify-cfg",
            "instcombine",
            "simplify-cfg",
            "gvn",
            "instcombine",
            "simplify-cfg",
            "dce",
            "gcm",
        ],
    );
    assert_eq!(improved.functions[0].blocks().count(), 1);
    assert!(!improved.to_string().contains("icmp"));
    for x in [0, 1, u64::MAX, 1 << 63] {
        for y in [0, 1, u64::MAX] {
            check(&original, &improved, &[x, y]);
        }
    }
}

#[test]
fn memory_and_trapping_operations_remain_even_when_their_consumers_fold() {
    let (_, module) = apply(
        "func @f(ptr, i64) -> i8 {
block0(v0: ptr, v1: i64):
 v2 = load.i8 volatile v0
 v3 = iconst.i8 0
 v4 = and v2, v3
 v5 = iconst.i64 0
 v6 = sdiv v5, v1
 return v4
}",
        &["known-bits", "dce"],
    );
    assert!(module.to_string().contains("load.i8 volatile"));
    assert!(module.to_string().contains("sdiv"));
}

#[test]
fn reasoning_survives_pass_subsets_and_multiple_cleanup_rounds() {
    let source = "func @f(i8, i8) -> i8 {
block0(v0: i8, v1: i8):
 v2 = iconst.i8 15
 v3 = and v0, v2
 v4 = iconst.i8 16
 v5 = icmp.ult v3, v4
 brif v1, block1, block2
block1:
 v6 = iconst.i8 1
 v7 = or v3, v6
 jump block3(v7)
block2:
 v8 = iconst.i8 2
 v9 = or v3, v8
 jump block3(v9)
block3(v10: i8):
 v11 = icmp.ult v10, v4
 v12 = and v5, v11
 v13 = select v12, v10, v0
 return v13
}";
    let original = keel_ir::parse(source).unwrap();
    for subset in 0..64 {
        let passes = [
            "instcombine",
            "simplify-cfg",
            "gvn",
            "phi-simplify",
            "sccp",
            "dce",
        ];
        let mut sequence = vec!["known-bits"];
        sequence.extend(
            passes
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &p)| p),
        );
        sequence.extend(["known-bits", "instcombine", "simplify-cfg", "dce"]);
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &sequence).unwrap();
        for x in 0..256 {
            for cond in [0, 1] {
                check(&original, &module, &[x, cond]);
            }
        }
    }
}

#[test]
fn cfg_pruning_can_require_another_scalar_cleanup_round() {
    let source = "func @f(i8) -> i8 {
block0(v0: i8):
 v1 = iconst.i8 15
 v2 = and v0, v1
 v3 = iconst.i8 16
 v4 = icmp.ult v2, v3
 v5 = iconst.i8 1
 v6 = iconst.i8 128
 brif v4, block1(v5), block1(v6)
block1(v7: i8):
 v8 = icmp.ult v7, v3
 return v8
}";
    let (original, first) = apply(
        source,
        &["known-bits", "instcombine", "simplify-cfg", "dce"],
    );
    assert!(
        first.to_string().contains("icmp.ult"),
        "the join became constant only after CFG pruning"
    );
    let mut second = first.clone();
    keel_opt::run_passes(
        &mut second,
        &["known-bits", "instcombine", "simplify-cfg", "dce"],
    )
    .unwrap();
    assert!(!second.to_string().contains("icmp"));
    for input in 0..256 {
        check(&original, &second, &[input]);
    }
}
