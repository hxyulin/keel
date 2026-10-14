mod support;
use support::execute;

fn parse(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

const LOOP: &str = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iconst.i64 0
    v3 = iconst.i64 1
    jump block1(v0, v2)
block1(v4: i64, v5: i64):
    v6 = icmp.ult v5, v1
    brif v6, block2, block3(v4)
block2:
    v7 = iadd v5, v3
    jump block1(v4, v7)
block3(v8: i64):
    return v8
}";

#[test]
fn loop_copies_and_exit_copies_are_removed_but_induction_and_abi_stay() {
    let original = parse(LOOP);
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["phi-simplify"]).unwrap();
    use keel_ir::{Block, EntityRef};
    let func = &module.functions[0];
    assert_eq!(func.block_params(Block::new(0)).len(), 2);
    assert_eq!(func.block_params(Block::new(1)).len(), 1);
    assert_eq!(func.block_params(Block::new(3)).len(), 0);
    assert!(
        !keel_opt::find_pass("phi-simplify")
            .unwrap()
            .run(&mut module.clone().functions[0])
    );
    for input in [0, 1, u64::MAX] {
        for count in 0..16 {
            assert_eq!(
                execute(func, &[input, count]),
                execute(&original.functions[0], &[input, count])
            );
        }
    }
}

#[test]
fn distinct_edges_from_one_predecessor_are_not_collapsed() {
    let mut module = parse(
        "func @f(i8, i64, i64) -> i64 {
block0(v0: i8, v1: i64, v2: i64):
    brif v0, block1(v1), block1(v2)
block1(v3: i64):
    return v3
}",
    );
    assert!(
        !keel_opt::find_pass("phi-simplify")
            .unwrap()
            .run(&mut module.functions[0])
    );
    assert_eq!(execute(&module.functions[0], &[0, 10, 20]), 20);
    assert_eq!(execute(&module.functions[0], &[1, 10, 20]), 10);
}

#[test]
fn substitution_revisits_users_even_in_reverse_layout() {
    let mut module = parse(
        "func @f(i64) -> i64 {
block0(v0: i64):
    jump block2(v0)
block1(v1: i64):
    return v1
block2(v2: i64):
    jump block1(v2)
}",
    );
    keel_opt::run_passes(&mut module, &["phi-simplify"]).unwrap();
    assert!(
        module.functions[0]
            .blocks()
            .skip(1)
            .all(|b| module.functions[0].block_params(b).is_empty())
    );
}

#[test]
fn existing_sccp_aliases_and_multiple_parameter_removals_stay_resolved() {
    let mut module = parse(
        "func @f(i8, i64) -> i64 {
block0(v0: i8, v1: i64):
    v2 = iconst.i64 42
    brif v0, block1(v1, v2, v2), block1(v1, v2, v2)
block1(v3: i64, v4: i64, v5: i64):
    v6 = iadd v4, v5
    v7 = iadd v3, v6
    return v7
}",
    );
    keel_opt::run_passes(&mut module, &["sccp", "phi-simplify", "instcombine", "dce"]).unwrap();
    for condition in [0, 1] {
        assert_eq!(execute(&module.functions[0], &[condition, 100]), 184);
    }
}

#[test]
fn every_existing_pass_subset_preserves_loop_semantics() {
    let original = parse(LOOP);
    for subset in 0..128 {
        let other = [
            "const-fold",
            "sccp",
            "instcombine",
            "simplify-cfg",
            "gvn",
            "dce",
            "gcm",
        ];
        let mut names = vec!["phi-simplify"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        names.push("phi-simplify");
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        for input in [0, 1, u64::MAX] {
            for count in 0..4 {
                assert_eq!(
                    execute(&module.functions[0], &[input, count]),
                    input,
                    "{names:?}"
                );
            }
        }
    }
}
