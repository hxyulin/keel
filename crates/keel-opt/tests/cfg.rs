mod support;
use support::execute;

fn optimize(src: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["simplify-cfg"]).unwrap();
    module
}

#[test]
fn forwarding_substitutes_permutations_and_repeated_arguments() {
    let src = "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
    brif v2, block1(v0, v1), block2(v0, v1)
block1(v3: i64, v4: i64):
    jump block3(v4, v3)
block2(v5: i64, v6: i64):
    jump block3(v5, v5)
block3(v7: i64, v8: i64):
    v9 = imul v7, v7
    v10 = iadd v9, v8
    return v10
}";
    let original = keel_ir::parse(src).unwrap();
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 2);
    for a in [0, 1, 7, u64::MAX] {
        for b in [0, 2, 9, u64::MAX] {
            for condition in [0, 1] {
                assert_eq!(
                    execute(&original.functions[0], &[a, b, condition]),
                    execute(&optimized.functions[0], &[a, b, condition])
                );
            }
        }
    }
    let mut second = optimized.clone();
    assert!(
        !keel_opt::find_pass("simplify-cfg")
            .unwrap()
            .run(&mut second.functions[0])
    );
}

#[test]
fn externally_used_parameter_keeps_its_definition() {
    let src = "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
    brif v2, block1(v0), block1(v1)
block1(v3: i64):
    jump block2
block2:
    return v3
}";
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 2);
    assert_eq!(execute(&optimized.functions[0], &[40, 99, 0]), 99);
    assert_eq!(execute(&optimized.functions[0], &[40, 99, 1]), 40);
}

#[test]
fn forwarding_cycle_terminates_and_retains_nontermination() {
    let src = "func @f(i64) {
block0(v0: i64):
    jump block1(v0)
block1(v1: i64):
    jump block2(v1)
block2(v2: i64):
    jump block1(v2)
}";
    let mut module = optimize(src);
    assert_eq!(module.functions[0].blocks().count(), 2);
    assert!(
        !keel_opt::find_pass("simplify-cfg")
            .unwrap()
            .run(&mut module.functions[0])
    );
    assert!(module.functions[0].layout_insts().all(|(_, i)| matches!(
        module.functions[0].inst_data(i),
        keel_ir::InstData::Jump { .. }
    )));
}

#[test]
fn forwarding_preserves_parameterized_loop_backedges() {
    let src = "func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 0
    jump block1(v1)
block1(v2: i64):
    v3 = iconst.i64 1
    v4 = iadd v2, v3
    v5 = icmp.ult v4, v0
    brif v5, block2(v4), block3(v4)
block2(v6: i64):
    jump block1(v6)
block3(v7: i64):
    return v7
}";
    let original = keel_ir::parse(src).unwrap();
    for subset in 0..32 {
        let other = ["const-fold", "sccp", "gvn", "dce", "gcm"];
        let mut names = vec!["simplify-cfg"];
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
        for input in 0..32 {
            assert_eq!(
                execute(&module.functions[0], &[input]),
                execute(&original.functions[0], &[input]),
                "passes {names:?}, input {input}"
            );
        }
    }
}
