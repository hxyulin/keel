mod support;

use keel_ir::{BinaryOp, InstData};
use keel_opt::OptimizationTier;

fn source(ty: &str, d: u64, op: &str) -> String {
    format!(
        "func @f({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} {}\n v2 = {op} v0, v1\n return v2\n}}",
        d as i64
    )
}

#[test]
fn verified_wide_ir_matches_division_and_remainder_in_every_tier() {
    let mut random = 0xa931_1b21_c578_d901u64;
    for (ty, width) in [("i32", 32), ("i64", 64)] {
        let mask = u64::MAX >> (64 - width);
        for d in [
            1,
            2,
            3,
            5,
            7,
            10,
            31,
            37,
            63,
            127,
            255,
            641,
            mask >> 1,
            (mask >> 1) + 1,
            (mask >> 1) + 2,
            mask,
        ] {
            for op in ["udiv", "urem"] {
                let original = keel_ir::parse(&source(ty, d, op)).unwrap();
                for tier in [
                    OptimizationTier::None,
                    OptimizationTier::Basic,
                    OptimizationTier::Full,
                ] {
                    let mut module = original.clone();
                    keel_opt::optimize_with_tier(&mut module, tier).unwrap();
                    keel_ir::verify_module(&module).unwrap();
                    let text = module.to_string();
                    if tier != OptimizationTier::None {
                        assert!(!text.contains(op), "{tier:?}: {text}");
                    }
                    let boundary = (mask / d) * d;
                    let mut inputs = vec![
                        0,
                        1,
                        d - 1,
                        d,
                        d.wrapping_add(1) & mask,
                        mask,
                        boundary,
                        boundary.saturating_sub(1),
                    ];
                    inputs.extend(0..=255);
                    for _ in 0..512 {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        inputs.push(random & mask);
                    }
                    for n in inputs {
                        let expected = if op == "udiv" { n / d } else { n % d };
                        assert_eq!(
                            support::execute(&module.functions[0], &[n]),
                            expected,
                            "{ty} {op} {n}/{d}, {tier:?}"
                        );
                    }
                }
            }
        }
    }
}

/// Every divisor up to 257, every power of two and seeded random divisors,
/// all in one module through the full pipeline. Each function is checked
/// around low, final and random quotient multiples.
#[test]
fn full_pipeline_matches_division_for_dense_power_and_random_divisors() {
    let mut random = 0x00de_c1a1_u64;
    let mut next = move || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random
    };
    let mut cases = Vec::new();
    let mut text = String::new();
    for (ty, width) in [("i32", 32u32), ("i64", 64)] {
        let mask = u64::MAX >> (64 - width);
        let mut divisors: std::collections::BTreeSet<u64> = (1..=257).collect();
        divisors.extend((0..width).map(|bit| 1 << bit));
        divisors.extend([mask >> 1, (mask >> 1) + 2, mask]);
        divisors.extend((0..64).map(|_| (next() & mask).max(1)));
        for d in divisors {
            for op in ["udiv", "urem"] {
                let name = format!("f{}", cases.len());
                text += &source(ty, d, op).replacen("@f(", &format!("@{name}("), 1);
                text.push('\n');
                cases.push((name, mask, d, op));
            }
        }
    }
    let mut module = keel_ir::parse(&text).unwrap();
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let optimized = module.to_string();
    assert!(!optimized.contains("udiv") && !optimized.contains("urem"));
    for (name, mask, d, op) in cases {
        let function = module.functions.iter().find(|f| f.name == name).unwrap();
        let mut inputs = vec![0, 1, d - 1, d, d.wrapping_add(1) & mask, mask];
        for q in [1, 2, mask / d, next() % (mask / d).saturating_add(1)] {
            let multiple = q.wrapping_mul(d);
            if multiple / d == q {
                inputs.extend([multiple.wrapping_sub(1), multiple, multiple.wrapping_add(1)]);
            }
        }
        inputs.extend((0..8).map(|_| next()));
        for n in inputs {
            let n = n & mask;
            let expected = if op == "udiv" { n / d } else { n % d };
            assert_eq!(
                support::execute(function, &[n]),
                expected,
                "{op} {n} by {d} (mask {mask:#x})"
            );
        }
    }
}

#[test]
fn full_pipeline_shares_quotient_and_preserves_locations() {
    for d in [3, 7, 10, u64::MAX] {
        let input = format!(
            "func @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 {}\n v2 = udiv v0, v1\n v3 = urem v0, v1\n v4 = iadd v2, v3\n return v4\n}}",
            d as i64
        );
        let mut module = keel_ir::parse(&input).unwrap();
        keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
        assert_eq!(module.to_string().matches("umulhi").count(), 1, "{module}");
    }
    // All instructions inserted for a single division retain its location.
    let mut module = keel_ir::parse(&source("i64", 7, "udiv")).unwrap();
    let file = module.add_file("division.c");
    let f = &mut module.functions[0];
    let division = f
        .layout_insts()
        .map(|(_, i)| i)
        .find(|&i| {
            matches!(
                f.inst_data(i),
                InstData::Binary {
                    op: BinaryOp::Udiv,
                    ..
                }
            )
        })
        .unwrap();
    let loc = Some(keel_ir::SourceLoc {
        file,
        line: 12,
        col: 3,
        scope: None,
    });
    f.set_inst_loc(division, loc);
    let old_count = f.num_insts();
    keel_opt::find_pass("instcombine")
        .unwrap()
        .run(f, &Default::default());
    for (_, i) in f.layout_insts() {
        use keel_ir::EntityRef;
        if i.index() >= old_count || i == division {
            assert_eq!(f.inst_loc(i), loc);
        }
    }
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn unsupported_widths_signed_division_and_trapping_zero_are_unchanged() {
    for ty in ["i8", "i16", "i128"] {
        for op in ["udiv", "urem"] {
            let mut module = keel_ir::parse(&source(ty, 10, op)).unwrap();
            keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
            assert!(module.to_string().contains(op));
        }
    }
    for ty in ["i32", "i64"] {
        for (op, d) in [
            ("udiv", 0),
            ("urem", 0),
            ("sdiv", 10),
            ("srem", 10),
            ("sdiv", u64::MAX),
        ] {
            let mut module = keel_ir::parse(&source(ty, d, op)).unwrap();
            keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
            assert!(module.to_string().contains(op), "{module}");
        }
    }
}
