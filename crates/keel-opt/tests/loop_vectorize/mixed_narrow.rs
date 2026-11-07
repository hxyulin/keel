use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

const SOURCE: &str = include_str!("mixed_narrow.keel");
fn optimize(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes_for_target(&mut module, &["loop-reduce-multi"], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn vectorized(module: &keel_ir::Module) -> bool {
    module.functions[0]
        .layout_insts()
        .any(|(_, i)| module.functions[0].inst_data(i).is_vector_instruction())
}
fn execute(module: &keel_ir::Module, input: &[i16], n: u64) -> Outcome {
    let mut interp = Interpreter::new(module).with_step_limit(10_000_000);
    let bytes: Vec<_> = input.iter().flat_map(|x| x.to_le_bytes()).collect();
    let data = interp.memory_mut().alloc_bytes(&bytes, 16);
    interp.call("k_narrow", &[Val::Ptr(data), Val::I64(n)])
}
fn reference(input: &[i16]) -> i64 {
    input
        .iter()
        .map(|&v| {
            let c = v.clamp(-128, 127) as i8;
            let m = ((v as u8 as i32) * 3 + ((v >> 8) as i8 as i32) * 5 + ((v as u16) >> 2) as i32)
                as u16;
            (c as i32 * 7 + m as i32 - v.wrapping_mul(3) as i32 / 7 + ((v >> 3) as u8) as i32)
                as i64
        })
        .sum()
}
#[test]
fn narrow_mixed_rings_constant_shifts_and_division_vectorize_exactly() {
    let after = optimize(SOURCE, VectorTarget::Aarch64Neon);
    assert!(vectorized(&after), "{after}");
    let mut again = after.clone();
    keel_opt::run_passes_for_target(
        &mut again,
        &["loop-reduce-multi"],
        VectorTarget::Aarch64Neon,
    )
    .unwrap();
    assert_eq!(again.to_string(), after.to_string());
    // Cover every narrow signed input, including division remainders and wrap
    // boundaries, while retaining exact-size reads and scalar tails.
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for n in [0, 1, 3, 7, 8, 9, 31, 65, 65536] {
        assert_eq!(
            execute(&after, &input[..n], n as u64),
            Outcome::Return(vec![Val::I64(reference(&input[..n]) as u64)]),
            "n={n}"
        );
    }
}
#[test]
fn narrow_recipe_rejects_effects_unbounded_division_and_unsupported_targets() {
    for target in [VectorTarget::Portable, VectorTarget::X86Sse2] {
        let after = optimize(SOURCE, target);
        assert!(!vectorized(&after));
    }
    for source in [
        SOURCE.replace("load.i16 trap_ub", "load.i16 volatile"),
        SOURCE.replace("sdiv v29, v21", "sdiv v24, v21"),
        SOURCE.replace("iconst.i32 7", "iconst.i32 0"),
        SOURCE.replace("iconst.i32 7", "iconst.i32 -1"),
    ] {
        let after = optimize(&source, VectorTarget::Aarch64Neon);
        assert!(!vectorized(&after), "{after}");
    }
}
#[test]
fn narrow_division_handles_negative_divisors_and_large_quotient_boundaries() {
    let input: Vec<_> = [
        i16::MIN,
        -32767,
        -22938,
        -128,
        -8,
        -7,
        -6,
        -1,
        0,
        1,
        6,
        7,
        8,
        127,
        22937,
        32767,
    ]
    .into_iter()
    .cycle()
    .take(65)
    .collect();
    for narrow in ["i8", "i16"] {
        for divisor in [-32768, -127, -7, -2, 2, 3, 7, 127, 32768] {
            let source = SOURCE
                .replace("v28 = trunc.i16", &format!("v28 = trunc.{narrow}"))
                .replace(
                    "v31 = sdiv v29, v21",
                    &format!("v90 = iconst.i32 {divisor}\n    v31 = sdiv v29, v90"),
                );
            let before = keel_ir::parse(&source).unwrap();
            let after = optimize(&source, VectorTarget::Aarch64Neon);
            assert!(vectorized(&after), "d={divisor}: {after}");
            assert_eq!(
                execute(&before, &input, 65),
                execute(&after, &input, 65),
                "d={divisor}"
            );
        }
    }
}
