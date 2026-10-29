mod support;
use keel_ir::{Function, InsertPoint, Signature, Type};
use support::execute;

#[test]
fn high_fanout_and_repeated_operands_keep_the_same_schedule() {
    let mut func = Function::new("fanout", Signature::new(&[Type::I64], &[Type::I64]));
    let entry = func.create_block();
    let input = func.append_block_param(entry, Type::I64);
    let one = func.ins(InsertPoint::End(entry)).iconst(Type::I64, 1);
    let mut sum = input;
    for _ in 0..1024 {
        let twice = func.ins(InsertPoint::End(entry)).iadd(one, one);
        sum = func.ins(InsertPoint::End(entry)).iadd(sum, twice);
    }
    func.ins(InsertPoint::End(entry)).return_(&[sum]);
    let before = func.to_string();
    assert!(!keel_opt::find_pass("gcm").unwrap().run(&mut func));
    keel_ir::verify(&func).unwrap();
    assert_eq!(before, func.to_string());
    for input in [0, 1, u64::MAX] {
        assert_eq!(execute(&func, &[input]), input.wrapping_add(2048));
    }
}

#[test]
fn shared_constants_preserve_each_blocks_first_use() {
    let source = "func @f(i64, i8) -> i64 {
block0(v0: i64, v1: i8):
    v2 = iconst.i64 7
    v7 = iconst.i64 11
    brif v1, block1, block2
block1:
    v3 = iadd v2, v7
    v8 = iadd v0, v3
    return v8
block2:
    v4 = imul v2, v7
    v9 = iadd v0, v4
    return v9
}";
    let original = keel_ir::parse(source).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["gcm"]).unwrap();
    assert_eq!(
        module.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(
                module.functions[0].inst_data(*i),
                keel_ir::InstData::Iconst { .. }
            ))
            .count(),
        4
    );
    for x in [0, 1, 17, u64::MAX] {
        for condition in [0, 1] {
            assert_eq!(
                execute(&module.functions[0], &[x, condition]),
                execute(&original.functions[0], &[x, condition])
            );
        }
    }
    assert!(
        !keel_opt::find_pass("gcm")
            .unwrap()
            .run(&mut module.functions[0])
    );
}

#[test]
fn shared_users_of_both_overflow_results_are_scheduled_once() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iconst.i64 0
    jump block1(v2)
block1(v3: i64):
    v4, v5 = sadd_overflow v0, v0
    v6 = select v5, v4, v0
    v7 = iconst.i64 1
    v8 = iadd v3, v7
    v9 = icmp.ult v8, v1
    brif v9, block1(v8), block2(v6)
block2(v10: i64):
    return v10
}";
    let original = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    for subset in 0..32 {
        let other = ["const-fold", "sccp", "simplify-cfg", "gvn", "dce"];
        let mut names = vec!["gcm"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        for x in [0, 1, 1 << 62, 1 << 63, u64::MAX] {
            for n in 0..4 {
                assert_eq!(
                    execute(&module.functions[0], &[x, n]),
                    execute(&original.functions[0], &[x, n]),
                    "passes {names:?}, input {x}, {n}"
                );
            }
        }
    }
}

#[test]
fn late_placement_and_loop_exit_follow_the_dominator_tree() {
    // The first product has users in both arms, so it stays at their common
    // dominator. The sum's second user is dominated by its first, so the sum
    // sinks into block1. The product in block5 is invariant in the loop.
    let source = "func @f(i64, i8, i64) -> i64 {
block0(v0: i64, v1: i8, v20: i64):
    v2 = imul v0, v0
    v3 = iadd v0, v0
    brif v1, block1, block2
block1:
    v4 = iadd v2, v3
    jump block3(v4)
block2:
    v5 = isub v2, v0
    return v5
block3(v6: i64):
    v7 = iadd v6, v3
    v9 = iconst.i64 0
    jump block4(v9, v7)
block4(v10: i64, v11: i64):
    v12 = icmp.ult v10, v20
    brif v12, block5, block6
block5:
    v8 = imul v0, v20
    v13 = iadd v11, v8
    v14 = iconst.i64 1
    v15 = iadd v10, v14
    jump block4(v15, v13)
block6:
    return v11
}
";
    let mut module = keel_ir::parse(source).unwrap();
    let func = &mut module.functions[0];
    assert!(keel_opt::find_pass("gcm").unwrap().run(func));
    keel_ir::verify(func).unwrap();
    // The invariant product goes to block3, the latest block outside the loop.
    let expected = "func @f(i64, i8, i64) -> i64 cc(c) {
block0(v0: i64, v1: i8, v2: i64):
    v3 = imul v0, v0
    brif v1, block1, block2
block1:
    v4 = iadd v0, v0
    v5 = iadd v3, v4
    jump block3(v5)
block2:
    v6 = isub v3, v0
    return v6
block3(v7: i64):
    v8 = iadd v7, v4
    v9 = iconst.i64 0
    v13 = imul v0, v2
    jump block4(v9, v8)
block4(v10: i64, v11: i64):
    v12 = icmp.ult v10, v2
    brif v12, block5, block6
block5:
    v14 = iadd v11, v13
    v15 = iconst.i64 1
    v16 = iadd v10, v15
    jump block4(v16, v14)
block6:
    return v11
}
";
    assert_eq!(func.to_string(), expected);
}
