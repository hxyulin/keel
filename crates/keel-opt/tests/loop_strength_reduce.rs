mod support;
use keel_ir::{BinaryOp, InstData, Type};

fn source(ty: Type, step: i64) -> String {
    format!(
        "func @hot({ty}, {ty}, {ty}, {ty}) -> {ty} cc(c) {{
block0(v0: {ty}, v1: {ty}, v2: {ty}, v3: {ty}):
 v4 = iconst.{ty} 0
 jump block1(v2, v3, v4)
block1(v5: {ty}, v6: {ty}, v7: {ty}):
 v8 = icmp.ult v7, v0
 brif v8, block2, block3(v6)
block2:
 v9 = imul v5, v1
 v10 = iadd v6, v9
 v11 = iconst.{ty} {step}
 v12 = iadd v5, v11
 v13 = iconst.{ty} 1
 v14 = iadd v7, v13
 jump block1(v12, v10, v14)
block3(v15: {ty}):
 return v15
}}"
    )
}
fn optimize(s: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["loop-strength-reduce"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}

#[test]
fn modular_recurrences_match_independent_oracle_across_wrap_and_zero_trip() {
    for ty in [Type::I32, Type::I64] {
        let mask = if ty == Type::I32 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };
        for step in [-7, -1, 0, 1, 3] {
            let s = source(ty, step);
            let before = keel_ir::parse(&s).unwrap();
            let after = optimize(&s);
            let f = &after.functions[0];
            let body = f.blocks().nth(2).unwrap();
            assert!(!f.block_insts(body).iter().any(|&i| matches!(
                f.inst_data(i),
                InstData::Binary {
                    op: BinaryOp::Imul,
                    ..
                }
            )));
            for stride in [0, 1, 3, 7, u32::MAX as u64, 1 << 31, 1 << 63, u64::MAX] {
                for start in [0u64, 1, u32::MAX as u64 - 1, (1 << 63) - 1, u64::MAX] {
                    for n in [0, 1, 2, 3, 7, 16, 31] {
                        let mut x = start & mask;
                        let mut expected = 17u64;
                        for _ in 0..n {
                            expected = expected.wrapping_add(x.wrapping_mul(stride)) & mask;
                            x = x.wrapping_add(step as u64) & mask;
                        }
                        let args = [n, stride, start, 17];
                        assert_eq!(support::execute(&before.functions[0], &args), expected);
                        assert_eq!(support::execute(f, &args), expected);
                    }
                }
            }
            let mut twice = after.clone();
            keel_opt::run_passes(&mut twice, &["loop-strength-reduce"]).unwrap();
            assert_eq!(twice.to_string(), after.to_string());
        }
    }
}

#[test]
fn varying_multiplier_dynamic_steps_and_unsupported_shapes_are_unchanged() {
    let s = source(Type::I64, 1);
    for input in [
        s.replace("imul v5, v1", "imul v5, v6"),
        s.replace("iadd v5, v11", "iadd v5, v1"),
        s.replace("brif v8, block2, block3(v6)", "brif v8, block3(v6), block2"),
        s.replace(
            " jump block1(v2, v3, v4)",
            " brif v0, block1(v2, v3, v4), block3(v3)",
        ),
        source(Type::I16, 1),
    ] {
        let before = keel_ir::parse(&input).unwrap();
        let after = optimize(&input);
        assert_eq!(after.to_string(), before.to_string());
    }
}

#[test]
fn body_division_stays_guarded_on_zero_trip_and_traps_on_taken_path() {
    let s = source(Type::I64, 1).replace(
        " v10 = iadd v6, v9",
        " v16 = udiv v9, v4\n v10 = iadd v6, v16",
    );
    let before = keel_ir::parse(&s).unwrap();
    let after = optimize(&s);
    for m in [&before, &after] {
        assert_eq!(support::execute(&m.functions[0], &[0, 9, 3, 17]), 17);
        assert!(
            std::panic::catch_unwind(|| support::execute(&m.functions[0], &[1, 9, 3, 17])).is_err()
        );
    }
}
