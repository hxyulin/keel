use keel_ir::{BinaryOp, EntityRef, InstData, Value};

fn source(ty: &str, left: i64, right: i64, op: &str, operands: &str, rhs: &str) -> String {
    format!(
        "func @f({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = iconst.{ty} {left}\n v3 = shl v0, v2\n v4 = iconst.{ty} {right}\n v5 = {op} {rhs}, v4\n v6 = or {operands}\n return v6\n}}"
    )
}

fn canonical(source: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_opt::run_passes(&mut m, &["canonicalize"]).unwrap();
    assert!(
        !keel_opt::find_pass("canonicalize")
            .unwrap()
            .run(&mut m.functions[0])
    );
    m
}

#[test]
fn all_constant_counts_and_commuted_operands() {
    for (ty, width) in [("i32", 32), ("i64", 64)] {
        for count in 1..width {
            for operands in ["v3, v5", "v5, v3"] {
                let m = canonical(&source(ty, count, width - count, "ushr", operands, "v0"));
                let f = &m.functions[0];
                let inst = f
                    .layout_insts()
                    .find(|(_, i)| f.inst_results(*i).contains(&Value::new(6)))
                    .unwrap()
                    .1;
                assert_eq!(
                    f.inst_data(inst),
                    &InstData::Binary {
                        op: BinaryOp::Rotl,
                        args: [Value::new(0), Value::new(2)]
                    }
                );
                for x in [
                    0,
                    1,
                    2,
                    0x80000000,
                    u32::MAX as u64,
                    1 << 63,
                    u64::MAX,
                    0x0123456789abcdef,
                ] {
                    let original = if width == 32 {
                        let x = x as u32;
                        ((x << count) | (x >> (width - count))) as u64
                    } else {
                        (x << count) | (x >> (width - count))
                    };
                    let rotated = if width == 32 {
                        (x as u32).rotate_left(count as u32) as u64
                    } else {
                        x.rotate_left(count as u32)
                    };
                    assert_eq!(original, rotated);
                }
            }
        }
    }
}

#[test]
fn negative_width_count_source_and_shift_cases() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        let width = ty[1..].parse::<i64>().unwrap();
        for (left, right, op, rhs) in [
            (0, width, "ushr", "v0"),
            (width, 0, "ushr", "v0"),
            (-1, width + 1, "ushr", "v0"),
            (1, width, "ushr", "v0"),
            (1, width - 2, "ushr", "v0"),
            (1, width - 1, "sshr", "v0"),
            (1, width - 1, "ushr", "v1"),
        ] {
            let m = canonical(&source(ty, left, right, op, "v3, v5", rhs));
            assert!(!m.to_string().contains("rotl"), "{m}");
        }
        if !["i32", "i64"].contains(&ty) {
            assert!(
                !canonical(&source(ty, 1, width - 1, "ushr", "v3, v5", "v0"))
                    .to_string()
                    .contains("rotl")
            );
        }
    }
    let dynamic = source("i64", 51, 13, "ushr", "v3, v5", "v0").replace("shl v0, v2", "shl v0, v1");
    assert!(!canonical(&dynamic).to_string().contains("rotl"));
}

#[test]
fn resolves_shift_source_and_constant_aliases_without_publishing_dead_keys() {
    let mut m = keel_ir::parse(&source("i64", 51, 13, "ushr", "v3, v5", "v1")).unwrap();
    let f = &mut m.functions[0];
    // Leave the aliased instruction in layout, exactly as optimizer clients do.
    f.replace_all_uses(Value::new(1), Value::new(0));
    f.replace_all_uses(Value::new(4), Value::new(2));
    // Restore a complementary count via a new constant already present in IR.
    // This first case must reject two shifts with identical resolved counts.
    keel_opt::run_passes(&mut m, &["canonicalize"]).unwrap();
    assert!(!m.to_string().contains("rotl"));
    let mut m = keel_ir::parse(&source("i64", 51, 13, "ushr", "v3, v5", "v1")).unwrap();
    m.functions[0].replace_all_uses(Value::new(1), Value::new(0));
    keel_opt::run_passes(&mut m, &["canonicalize"]).unwrap();
    assert!(m.to_string().contains("rotl v0, v2"));
    assert_eq!(m.functions[0].resolve(Value::new(1)), Value::new(0));
}

#[test]
fn alias_definitions_and_source_locations_survive_in_memory() {
    let s = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iconst.i64 51
 v3 = shl v1, v2
 v4 = iconst.i64 13
 v5 = ushr v0, v4
 v6 = iconst.i64 7
 v7 = iconst.i64 51
 v8 = shl v0, v7
 v9 = or v3, v5
 return v9
}";
    let mut m = keel_ir::parse(s).unwrap();
    let f = &mut m.functions[0];
    f.replace_all_uses(Value::new(3), Value::new(8));
    f.replace_all_uses(Value::new(7), Value::new(2));
    let before: Vec<_> = f
        .layout_insts()
        .map(|(_, i)| (i, f.inst_loc(i), f.inst_results(i).to_vec()))
        .collect();
    keel_opt::run_passes(&mut m, &["canonicalize"]).unwrap();
    let f = &m.functions[0];
    for (i, loc, results) in before {
        assert_eq!(f.inst_loc(i), loc);
        assert_eq!(f.inst_results(i), results);
    }
    assert_eq!(f.resolve(Value::new(3)), Value::new(8));
    let i = f
        .layout_insts()
        .find(|(_, i)| f.inst_results(*i).contains(&Value::new(9)))
        .unwrap()
        .1;
    assert_eq!(
        f.inst_data(i),
        &InstData::Binary {
            op: BinaryOp::Rotl,
            args: [Value::new(0), Value::new(2)]
        }
    );
}
