mod support;
use keel_opt::OptimizationTier;

fn pair(ty: &str, signed: bool, reverse: bool) -> String {
    let (div, rem) = if signed {
        ("sdiv", "srem")
    } else {
        ("udiv", "urem")
    };
    let operations = if reverse {
        format!("v3 = {rem} v0, v1\n v2 = {div} v0, v1")
    } else {
        format!("v2 = {div} v0, v1\n v3 = {rem} v0, v1")
    };
    format!(
        "func @f({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n {operations}\n v4 = iadd v2, v3\n return v4\n}}"
    )
}

#[test]
fn earlier_quotients_preserve_arithmetic_and_traps_in_each_tier() {
    for signed in [false, true] {
        for (ty, width) in [("i8", 8), ("i16", 16), ("i32", 32), ("i64", 64)] {
            let original = keel_ir::parse(&pair(ty, signed, false)).unwrap();
            let mask = u64::MAX >> (64 - width);
            let min = 1u64 << (width - 1);
            for tier in [
                OptimizationTier::None,
                OptimizationTier::Fast,
                OptimizationTier::Basic,
                OptimizationTier::Full,
            ] {
                let mut m = original.clone();
                keel_opt::optimize_with_tier(&mut m, tier).unwrap();
                keel_ir::verify_module(&m).unwrap();
                let text = m.to_string();
                assert!(
                    text.contains(if signed { "sdiv" } else { "udiv" }),
                    "{text}"
                );
                assert_eq!(
                    text.contains(if signed { "srem" } else { "urem" }),
                    tier == OptimizationTier::None,
                    "{text}"
                );
                let inputs: Vec<_> = if width == 8 {
                    (0..256).collect()
                } else {
                    vec![0, 1, 2, 3, min - 1, min, min + 1, mask - 1, mask]
                };
                for &a in &inputs {
                    for &b in &inputs {
                        if b == 0 || (signed && a == min && b == mask) {
                            continue;
                        }
                        assert_eq!(
                            support::execute(&original.functions[0], &[a, b]),
                            support::execute(&m.functions[0], &[a, b]),
                            "{ty} signed={signed} {a}/{b} {tier:?}"
                        );
                    }
                }
                for (a, b) in [(1, 0), (min, mask)] {
                    if b != 0 && !signed {
                        continue;
                    }
                    assert!(
                        std::panic::catch_unwind(|| support::execute(&m.functions[0], &[a, b]))
                            .is_err()
                    );
                }
            }
        }
    }
}

#[test]
fn remainder_by_minus_one_is_zero_including_min() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        let src = format!(
            "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} -1\n v2 = srem v0, v1\n return v2\n}}"
        );
        let mut m = keel_ir::parse(&src).unwrap();
        keel_opt::run_passes(&mut m, &["instcombine", "dce"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(!m.to_string().contains("srem"), "{m}");
        if ty != "i128" {
            let width = ty[1..].parse::<u32>().unwrap();
            for a in [0, 1, u64::MAX, 1u64 << (width - 1)] {
                assert_eq!(support::execute(&m.functions[0], &[a]), 0);
            }
        }
    }
}

#[test]
fn later_sibling_and_differently_signed_divisions_are_not_reused() {
    let sibling = "func @f(i64, i64, i8) -> i64 {\nblock0(v0: i64, v1: i64, v2: i8):\n brif v2, block1, block2\nblock1:\n v3 = sdiv v0, v1\n return v3\nblock2:\n v4 = srem v0, v1\n return v4\n}";
    for src in [
        pair("i64", true, true),
        pair("i64", true, false).replace("v2 = sdiv", "v2 = udiv"),
        sibling.into(),
    ] {
        for passes in [&["instcombine"][..], OptimizationTier::Fast.pipeline()] {
            let mut m = keel_ir::parse(&src).unwrap();
            keel_opt::run_passes(&mut m, passes).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert!(m.to_string().contains("srem"), "{m}");
        }
    }
}

#[test]
fn inserted_product_keeps_remainder_location_and_pass_is_idempotent() {
    let mut m = keel_ir::parse(&pair("i64", true, false)).unwrap();
    let file = m.add_file("remainder.c");
    let f = &mut m.functions[0];
    let rem = f
        .layout_insts()
        .find(|(_, i)| {
            matches!(
                f.inst_data(*i),
                keel_ir::InstData::Binary {
                    op: keel_ir::BinaryOp::Srem,
                    ..
                }
            )
        })
        .unwrap()
        .1;
    let loc = Some(keel_ir::SourceLoc {
        file,
        line: 12,
        col: 4,
        scope: None,
    });
    f.set_inst_loc(rem, loc);
    let pass = keel_opt::find_pass("instcombine").unwrap();
    assert!(pass.run(f));
    let mul = f
        .layout_insts()
        .find(|(_, i)| {
            matches!(
                f.inst_data(*i),
                keel_ir::InstData::Binary {
                    op: keel_ir::BinaryOp::Imul,
                    ..
                }
            )
        })
        .unwrap()
        .1;
    assert_eq!(f.inst_loc(mul), loc);
    assert_eq!(f.inst_loc(rem), loc);
    assert!(!pass.run(f));
    keel_ir::verify_module(&m).unwrap();
}
