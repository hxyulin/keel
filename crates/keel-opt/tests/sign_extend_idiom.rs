mod support;

use keel_ir::{ConvertOp, InstData, Type};

const BRIDGE: &str = include_str!("fixtures/sign_extend_idiom/bridge_sext8.ir");
const SIMPLE: &str = "func @f(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 255
 v2 = and v0, v1
 v3 = iconst.i64 128
 v4 = icmp.ult v2, v3
 brif v4, block2(v2), block1
block1:
 v5 = iconst.i64 256
 v6 = isub v2, v5
 jump block2(v6)
block2(v7: i64):
 return v7
}";

fn apply(source: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["sign-extend-idiom"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn conversions(module: &keel_ir::Module) -> Vec<(ConvertOp, Type)> {
    module.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| match module.functions[0].inst_data(i) {
            InstData::Convert { op, ty, .. } => Some((*op, *ty)),
            _ => None,
        })
        .collect()
}
fn unchanged(source: &str) {
    let before = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&before).unwrap();
    assert_eq!(apply(source).to_string(), before.to_string());
}

#[test]
fn retained_actual_optimized_c_bridge_activates_and_preserves_recurrence() {
    let before = keel_ir::parse(BRIDGE).unwrap();
    let mut after = apply(BRIDGE);
    assert_eq!(
        conversions(&after),
        [(ConvertOp::Trunc, Type::I8), (ConvertOp::Sext, Type::I64)]
    );
    assert_eq!(
        after.functions[0].blocks().count() + 1,
        before.functions[0].blocks().count()
    );
    // The shared integer interpreter has no pointer ABI model. The actual
    // pointer-typed modules above are verified; neither function reads v2.
    // Project only that ignored ABI slot to i64 for execution. Fresh native
    // validation retains the real pointer ABI and exercises NULL at n=0.
    let model_before = keel_ir::parse(&before.to_string().replace("ptr", "i64")).unwrap();
    let model_after = keel_ir::parse(&after.to_string().replace("ptr", "i64")).unwrap();
    for byte in 0..=255u64 {
        for high in [0, 0x1234_5678_9abc_de00, !255] {
            let seed = high | byte;
            for n in [0, 1, 2, 17] {
                let mut oracle = seed;
                for _ in 0..n {
                    let low = oracle & 255;
                    oracle = if low < 128 {
                        low
                    } else {
                        low.wrapping_sub(256)
                    }
                    .wrapping_add(0x9e37_79b9_7f4a_7c15);
                }
                // n0 includes a null data pointer and maximal ignored mask.
                for f in [&model_before.functions[0], &model_after.functions[0]] {
                    assert_eq!(support::execute(f, &[n, seed, 0, u64::MAX]), oracle);
                }
            }
        }
    }
    let once = after.to_string();
    keel_opt::run_passes(&mut after, &["sign-extend-idiom"]).unwrap();
    assert_eq!(after.to_string(), once);
}

#[test]
fn all_bytes_high_bits_polarity_and_commuted_mask() {
    for source in [
        SIMPLE.to_string(),
        SIMPLE.replace("and v0, v1", "and v1, v0"),
        SIMPLE
            .replace("icmp.ult", "icmp.uge")
            .replace("block2(v2), block1", "block1, block2(v2)"),
    ] {
        let after = apply(&source);
        assert_eq!(conversions(&after).len(), 2);
        for byte in 0..=255u64 {
            for high in [0, 0x8000_0000_0000_0000, !255] {
                let seed = high | byte;
                assert_eq!(
                    support::execute(&after.functions[0], &[seed]),
                    (byte as u8 as i8 as i64) as u64
                );
            }
        }
    }
}

#[test]
fn rejects_predicate_bound_width_subtraction_and_shared_definitions() {
    for source in [
        SIMPLE.replace("icmp.ult", "icmp.slt"),
        SIMPLE.replace("icmp.ult", "icmp.ule"),
        SIMPLE.replace("128", "127"),
        SIMPLE.replace("256", "255"),
        SIMPLE.replace("255\n", "511\n"),
        SIMPLE.replace("isub v2, v5", "isub v5, v2"),
        SIMPLE.replace("i64", "i32"),
        SIMPLE.replace("i64", "i16"),
        SIMPLE.replace("return v7", "v8 = iadd v7, v2\n return v8"),
        SIMPLE.replace(
            "v5 = iconst.i64 256",
            "v8 = iconst.i64 1\n v5 = iconst.i64 256",
        ),
        SIMPLE.replace(
            "block2(v7: i64):",
            "block3:\n jump block1\nblock2(v7: i64):",
        ),
        SIMPLE.replace(
            "block2(v7: i64):",
            "block3:\n v8 = iconst.i64 0\n jump block2(v8)\nblock2(v7: i64):",
        ),
    ] {
        unchanged(&source);
    }
}

#[test]
fn never_activates_by_default() {
    assert!(!keel_opt::DEFAULT_PIPELINE.contains(&"sign-extend-idiom"));
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"sign-extend-idiom"));
    }
}

#[test]
fn rejects_deleted_arm_locations_and_aliased_results_in_memory() {
    let mut m = keel_ir::parse(SIMPLE).unwrap();
    let f = &mut m.functions[0];
    let blocks: Vec<_> = f.blocks().collect();
    let mask = f.block_insts(blocks[0])[1];
    let input = f.block_params(blocks[0])[0];
    f.replace_all_uses(f.first_result(mask), input);
    keel_ir::verify(f).unwrap();
    let before = format!("{f:?}");
    assert!(!keel_opt::find_pass("sign-extend-idiom").unwrap().run(f));
    assert_eq!(format!("{f:?}"), before);
}

#[test]
fn rejects_effects_volatile_calls_and_traps_in_either_region() {
    for effect in [
        "v9 = load.i64 volatile v8",
        "v9 = load.i64 v8",
        "store volatile v0, v8",
        "call @effect()",
        "v9 = udiv v0, v0",
        "intrinsic debugtrap()",
    ] {
        let source = SIMPLE
            .replace("func @f(i64)", "decl @effect()\nfunc @f(i64, ptr)")
            .replace("block0(v0: i64):", "block0(v0: i64, v8: ptr):");
        for anchor in ["v1 = iconst.i64 255", "v5 = iconst.i64 256"] {
            unchanged(&source.replace(anchor, &format!("{effect}\n {anchor}")));
        }
    }
}

#[test]
fn preserves_branch_location_and_rejects_arm_location_loss() {
    for annotate_arm in [false, true] {
        let mut m = keel_ir::parse(SIMPLE).unwrap();
        let file = m.add_file("sext.c");
        let f = &mut m.functions[0];
        let blocks: Vec<_> = f.blocks().collect();
        let branch = *f.block_insts(blocks[0]).last().unwrap();
        let arm = f.block_insts(blocks[1])[1];
        let loc = keel_ir::SourceLoc {
            file,
            line: 7,
            col: 3,
            scope: None,
        };
        f.set_inst_loc(branch, Some(loc));
        if annotate_arm {
            f.set_inst_loc(arm, Some(loc));
        }
        let before = format!("{f:?}");
        let changed = keel_opt::find_pass("sign-extend-idiom").unwrap().run(f);
        keel_ir::verify(f).unwrap();
        assert_eq!(changed, !annotate_arm);
        if annotate_arm {
            assert_eq!(format!("{f:?}"), before);
        } else {
            assert_eq!(f.inst_loc(branch), Some(loc));
            for (_, inst) in f.layout_insts() {
                if matches!(f.inst_data(inst), InstData::Convert { .. }) {
                    assert_eq!(f.inst_loc(inst), Some(loc));
                }
            }
        }
    }
}

#[test]
fn resolves_aliased_operands_without_publishing_alias_result_keys() {
    let source = SIMPLE
        .replace("func @f(i64)", "func @f(i64, i64)")
        .replace("block0(v0: i64):", "block0(v0: i64, v8: i64):");
    let mut m = keel_ir::parse(&source).unwrap();
    let f = &mut m.functions[0];
    let entry = f.entry_block().unwrap();
    let params = f.block_params(entry).to_vec();
    f.replace_all_uses(params[0], params[1]);
    keel_ir::verify(f).unwrap();
    assert!(keel_opt::find_pass("sign-extend-idiom").unwrap().run(f));
    keel_ir::verify(f).unwrap();
    for (_, i) in f.layout_insts() {
        if matches!(f.inst_data(i), InstData::Convert { .. }) {
            let result = f.first_result(i);
            assert_eq!(f.resolve(result), result);
            assert!(
                matches!(f.value_def(result), keel_ir::ValueDef::Result(owner, 0) if owner == i)
            );
        }
    }
    for seed in [0, 127, 128, 255, 256, u64::MAX] {
        assert_eq!(
            support::execute(f, &[seed, seed]),
            (seed as u8 as i8 as i64) as u64
        );
    }
}
