mod support;
use keel_ir::{BinaryOp, InstData};
use support::execute;

fn parse(src: &str) -> keel_ir::Module {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

fn folded(src: &str) -> keel_ir::Module {
    let mut module = parse(src);
    keel_opt::run_passes(&mut module, &["sccp"]).unwrap();
    module
}

fn count(module: &keel_ir::Module, predicate: impl Fn(&InstData) -> bool) -> usize {
    module
        .functions
        .iter()
        .flat_map(|f| f.layout_insts().map(|(_, i)| f.inst_data(i)))
        .filter(|data| predicate(data))
        .count()
}

const LOOP: &str = "func @loop(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 0
    v2 = iconst.i64 7
    jump block1(v1, v2)
block1(v3: i64, v4: i64):
    v5 = iconst.i64 1
    v6 = iadd v3, v5
    v7 = iadd v4, v5
    v8 = icmp.ult v6, v0
    brif v8, block1(v6, v4), block2(v7)
block2(v9: i64):
    return v9
}";

#[test]
fn invariant_loop_parameter_folds_but_induction_variable_does_not() {
    let module = folded(LOOP);
    assert_eq!(
        count(&module, |d| matches!(
            d,
            InstData::Binary {
                op: BinaryOp::Iadd,
                ..
            }
        )),
        1
    );
    assert!(module.to_string().contains("iconst.i64 8"));
    let mut twice = module.clone();
    keel_opt::run_passes(&mut twice, &["sccp"]).unwrap();
    assert_eq!(module.to_string(), twice.to_string(), "pass is idempotent");
    assert!(
        !keel_opt::find_pass("sccp")
            .unwrap()
            .run(&mut twice.functions[0])
    );
}

#[test]
fn only_executable_edges_contribute_to_parameters() {
    let module = folded(
        "func @f() -> i64 {
block0:
    v0 = iconst.i8 1
    v1 = iconst.i64 40
    v2 = iconst.i64 99
    brif v0, block1(v1), block1(v2)
block1(v3: i64):
    v4 = iconst.i64 2
    v5 = iadd v3, v4
    return v5
}",
    );
    assert_eq!(count(&module, |d| matches!(d, InstData::Binary { .. })), 0);
    assert!(module.to_string().contains("iconst.i64 42"));
}

#[test]
fn different_arguments_on_two_executable_arms_are_not_constant() {
    let module = folded(
        "func @f(i8) -> i64 {
block0(v0: i8):
    v1 = iconst.i64 40
    v2 = iconst.i64 99
    brif v0, block1(v1), block1(v2)
block1(v3: i64):
    v4 = iconst.i64 2
    v5 = iadd v3, v4
    return v5
}",
    );
    assert_eq!(count(&module, |d| matches!(d, InstData::Binary { .. })), 1);
    for x in [0, 1, 255] {
        assert_eq!(
            execute(&module.functions[0], &[x]),
            if x == 0 { 101 } else { 42 }
        );
    }
}

#[test]
fn effectful_results_expose_both_arms_and_preserve_calls() {
    let module = folded(
        "decl @condition() -> i8
func @f() -> i64 {
block0:
    v0 = call @condition()
    v1 = iconst.i64 40
    v2 = iconst.i64 99
    brif v0, block1(v1), block1(v2)
block1(v3: i64):
    v4 = iconst.i64 2
    v5 = iadd v3, v4
    return v5
}",
    );
    assert_eq!(count(&module, |d| matches!(d, InstData::Call { .. })), 1);
    assert_eq!(count(&module, |d| matches!(d, InstData::Binary { .. })), 1);
}

#[test]
fn equal_narrow_bit_patterns_meet_and_switch_cases_are_masked() {
    let module = folded(
        "func @f(i8) -> i8 {
block0(v0: i8):
    v1 = iconst.i8 -1
    v2 = iconst.i8 255
    brif v0, block1(v1), block1(v2)
block1(v3: i8):
    switch v3, block2, [255: block3]
block2:
    v4 = iconst.i8 9
    jump block4(v4)
block3:
    v5 = iconst.i8 10
    jump block4(v5)
block4(v6: i8):
    v7 = iconst.i8 2
    v8 = iadd v6, v7
    return v8
}",
    );
    assert_eq!(count(&module, |d| matches!(d, InstData::Binary { .. })), 0);
    for input in [0, 1] {
        assert_eq!(execute(&module.functions[0], &[input]), 12);
    }
}

#[test]
fn newly_executable_backedge_invalidates_an_initial_constant() {
    let source = "func @f() -> i64 {
block0:
    v0 = iconst.i64 0
    jump block1(v0)
block1(v1: i64):
    v2 = iconst.i64 1
    v3 = iadd v1, v2
    v4 = iconst.i64 3
    v5 = icmp.ult v3, v4
    brif v5, block1(v3), block2(v3)
block2(v6: i64):
    return v6
}";
    let module = folded(source);
    assert_eq!(count(&module, |d| matches!(d, InstData::Binary { .. })), 1);
    assert_eq!(execute(&module.functions[0], &[]), 3);
}

#[test]
fn nonconstant_condition_selects_equal_constants() {
    let module = folded(
        "func @f(i8) -> i64 {
block0(v0: i8):
    v1 = iconst.i64 42
    v2 = iconst.i64 42
    v3 = select v0, v1, v2
    return v3
}",
    );
    assert_eq!(count(&module, |d| matches!(d, InstData::Select { .. })), 0);
}

#[test]
fn propagated_division_by_zero_and_signed_overflow_still_trap() {
    for (ty, dividend, divisor) in [
        ("i32", "1", "0"),
        ("i8", "-128", "-1"),
        ("i64", "-9223372036854775808", "-1"),
    ] {
        let source = format!(
            "func @f() -> {ty} {{
block0:
    v0 = iconst.{ty} {dividend}
    v1 = iconst.{ty} {divisor}
    jump block1(v0, v1)
block1(v2: {ty}, v3: {ty}):
    v4 = sdiv v2, v3
    return v4
}}"
        );
        let module = folded(&source);
        assert_eq!(
            count(&module, |d| matches!(
                d,
                InstData::Binary {
                    op: BinaryOp::Sdiv,
                    ..
                }
            )),
            1
        );
    }
}

#[test]
fn floats_propagate_bit_exactly_through_parameters() {
    let module = folded(
        "func @f(i8) -> f64 {
block0(v0: i8):
    v1 = fconst.f64 -0.0
    v2 = fconst.f64 0.0
    brif v0, block1(v1), block1(v2)
block1(v3: f64):
    v4 = fneg v3
    return v4
}",
    );
    assert_eq!(
        count(&module, |d| matches!(d, InstData::Unary { .. })),
        1,
        "signed zeros must not meet"
    );
    let module = folded(
        "func @f() -> f64 {
block0:
    v0 = fconst.f64 0x7ff8000000000042
    jump block1(v0)
block1(v1: f64):
    v2 = fneg v1
    return v2
}",
    );
    assert_eq!(
        count(&module, |d| matches!(
            d,
            InstData::Fconst {
                bits: 0xfff8000000000042,
                ..
            }
        )),
        1
    );
}

#[test]
fn pass_subset_matrix_preserves_generated_programs() {
    let passes = ["const-fold", "simplify-cfg", "gvn", "dce", "gcm"];
    for seed in 0..12u64 {
        // Constants meet across a diamond, feed a loop invariant and a
        // wrapping expression; values vary across both branch arms.
        let source = format!(
            "func @f(i64) -> i64 {{
block0(v0: i64):
    v1 = iconst.i64 {seed}
    v2 = iconst.i64 0
    v3 = icmp.eq v0, v2
    brif v3, block1(v1), block2(v1)
block1(v4: i64):
    jump block3(v4, v2)
block2(v5: i64):
    jump block3(v5, v2)
block3(v6: i64, v7: i64):
    v8 = iconst.i64 1
    v9 = iadd v7, v8
    v10 = imul v6, v6
    v11 = icmp.ult v9, v0
    brif v11, block3(v6, v9), block4(v10)
block4(v12: i64):
    v13 = xor v12, v0
    return v13
}}"
        );
        let original = parse(&source);
        for subset in 0..32 {
            let mut names: Vec<_> = passes
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &p)| p)
                .collect();
            names.insert(0, "sccp");
            let mut optimized = original.clone();
            keel_opt::run_passes(&mut optimized, &names).unwrap();
            // Run twice to exercise aliases and constants created by SCCP.
            keel_opt::run_passes(&mut optimized, &names).unwrap();
            for input in 0..16 {
                assert_eq!(
                    execute(&optimized.functions[0], &[input]),
                    execute(&original.functions[0], &[input]),
                    "seed {seed}, input {input}, passes {names:?}"
                );
            }
        }
    }
}
