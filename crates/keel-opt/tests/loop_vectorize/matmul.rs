use keel_ir::{Module, parse, verify_module};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

const SOURCE: &str = include_str!("matmul.keel");
fn apply(m: &mut Module, target: VectorTarget) {
    verify_module(m).unwrap();
    keel_opt::run_passes_for_target(m, &["loop-matmul"], target).unwrap();
    verify_module(m).unwrap();
}
#[test]
fn tiled_matrix_keeps_wrapping_arithmetic_and_alias_order() {
    let mut m = parse(SOURCE).unwrap();
    apply(&mut m, VectorTarget::Aarch64Neon);
    assert_eq!(m.functions.len(), 2, "{m}");
    let once = m.to_string();
    apply(&mut m, VectorTarget::Aarch64Neon);
    assert_eq!(once, m.to_string());
    for n in [0, 1, 3, 4, 5, 8, 9] {
        let cells = n * n;
        for mode in 0..6 {
            let a = 1;
            let b = if mode == 5 { a } else { cells + 3 };
            let c = match mode {
                1 => a,
                2 => a + 1,
                3 => b,
                4 => b + 1,
                _ => 2 * cells + 5,
            };
            let mut expected: Vec<u32> = (0..3 * cells + 9)
                .map(|i| (i as u32).wrapping_mul(0x9e3779b9).rotate_left(13) ^ 0x87654321)
                .collect();
            let bytes: Vec<_> = expected.iter().flat_map(|v| v.to_le_bytes()).collect();
            // This oracle uses the original store order even when an output
            // overwrites an input that a later dot product will read.
            for i in 0..n {
                for j in 0..n {
                    let mut sum = 0u32;
                    for k in 0..n {
                        sum = sum.wrapping_add(
                            expected[a + i * n + k].wrapping_mul(expected[b + k * n + j]),
                        );
                    }
                    expected[c + i * n + j] = sum;
                }
            }
            let mut interp = Interpreter::new(&m);
            let memory = interp.memory_mut().alloc_bytes(&bytes, 4);
            let outcome = interp.call(
                "k_matmul",
                &[
                    Val::Ptr(memory + 4 * a as u64),
                    Val::Ptr(memory + 4 * b as u64),
                    Val::Ptr(memory + 4 * c as u64),
                    Val::I32(n as u32),
                ],
            );
            assert_eq!(outcome, Outcome::Return(vec![]), "n={n} mode={mode}");
            let want: Vec<_> = expected.iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(
                interp.memory().read(memory, want.len() as u64).unwrap(),
                want,
                "n={n} mode={mode}"
            );
        }
    }
    assert_eq!(
        Interpreter::new(&m).call(
            "k_matmul",
            &[Val::Ptr(0), Val::Ptr(0), Val::Ptr(0), Val::I32(u32::MAX)]
        ),
        Outcome::Return(vec![])
    );
}
#[test]
fn matrix_transform_rejects_unsupported_contracts() {
    for target in [VectorTarget::Portable, VectorTarget::X86Sse2] {
        let mut m = parse(SOURCE).unwrap();
        let before = m.to_string();
        apply(&mut m, target);
        assert_eq!(m.to_string(), before);
    }
    for source in [
        SOURCE.replace("load.i32 trap_ub", "load.i32"),
        SOURCE.replace("store trap_ub", "store"),
        SOURCE.replace("load.i32 trap_ub", "load.i32 volatile"),
        SOURCE.replace("v36 = imul", "v36 = xor"),
        SOURCE.replace("v12 = iconst.i32 0", "v12 = iconst.i32 1"),
        SOURCE.replace("v30 = iadd v28, v9", "v30 = iadd v28, v5"),
    ] {
        let mut m = parse(&source).unwrap();
        let before = m.to_string();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert_eq!(m.to_string(), before, "{source}");
    }
    let mut m = parse(SOURCE).unwrap();
    m.functions[0].attrs.optnone = true;
    let before = m.to_string();
    apply(&mut m, VectorTarget::Aarch64Neon);
    assert_eq!(m.to_string(), before);
}

#[test]
fn matrix_policy_layout_and_observation_are_preserved() {
    for attrs in [
        "optnone",
        "optimize(\"O0\")",
        "optimize(\"no-tree-vectorize\")",
    ] {
        let source = SOURCE.replace("cc(c) {", &format!("cc(c) {attrs} {{"));
        let mut m = parse(&source).unwrap();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert_eq!(m.functions.len(), 1, "{attrs}");
    }
    let mut m = parse(SOURCE).unwrap();
    let mut context: keel_opt::OptimizationContext = VectorTarget::Aarch64Neon.into();
    context.layout = None;
    let before = m.to_string();
    keel_opt::Pipeline::named(&["loop-matmul"])
        .unwrap()
        .context(context)
        .run(&mut m)
        .unwrap();
    assert_eq!(m.to_string(), before);
    let mut observed = parse(SOURCE).unwrap();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut observed,
        &["loop-matmul"],
        VectorTarget::Aarch64Neon,
        &mut report,
    )
    .unwrap();
    let mut plain = parse(SOURCE).unwrap();
    apply(&mut plain, VectorTarget::Aarch64Neon);
    assert_eq!(observed.to_string(), plain.to_string());
    assert!(report.invocations.iter().any(|i| i.pass == "loop-matmul"));
}
