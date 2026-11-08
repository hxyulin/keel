use keel_ir::{InstData, Module, Type};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

fn source(ty: &str) -> String {
    let shift = if ty == "f32" { 2 } else { 3 };
    format!(
        "func @dot(ptr,ptr,i64,{ty}) -> {ty} {{
block0(v0:ptr,v1:ptr,v2:i64,v3:{ty}):
 v4 = iconst.i64 0
 jump block1(v4,v3)
block1(v5:i64,v6:{ty}):
 v7 = icmp.ult v5,v2
 brif v7,block2,block3
block2:
 v8 = iconst.i64 {shift}
 v9 = shl v5,v8
 v10 = ptradd v0,v9
 v11 = load.{ty} trap_ub v10
 v12 = ptradd v1,v9
 v13 = load.{ty} trap_ub v12
 v14 = fmul v11,v13
 v15 = fadd v6,v14
 v16 = iconst.i64 1
 v17 = iadd v5,v16
 jump block1(v17,v15)
block3:
 return v6
}}"
    )
}
fn optimize(src: &str, target: VectorTarget) -> Module {
    let mut module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes_for_target(&mut module, &["loop-dot"], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn execute(module: &Module, left: &[f64], right: &[f64], seed: f64) -> Val {
    let f = &module.functions[0];
    let ty = f.sig.returns[0].ty;
    let mut interpreter = Interpreter::for_function(f);
    let bytes = |xs: &[f64]| -> Vec<u8> {
        xs.iter()
            .flat_map(|x| {
                if ty == Type::F32 {
                    (*x as f32).to_bits().to_le_bytes().to_vec()
                } else {
                    x.to_bits().to_le_bytes().to_vec()
                }
            })
            .collect()
    };
    let a = interpreter.memory_mut().alloc_bytes(&bytes(left), 16);
    let b = interpreter.memory_mut().alloc_bytes(&bytes(right), 16);
    let seed = if ty == Type::F32 {
        Val::f32(seed as f32)
    } else {
        Val::f64(seed)
    };
    match interpreter.call(
        "dot",
        &[Val::Ptr(a), Val::Ptr(b), Val::I64(left.len() as u64), seed],
    ) {
        Outcome::Return(values) => values[0].clone(),
        other => panic!("dot failed: {other:?}"),
    }
}
#[test]
fn ordered_products_preserve_cancellation_signed_zero_and_tails() {
    for ty in ["f32", "f64"] {
        let src = source(ty);
        let original = keel_ir::parse(&src).unwrap();
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            let m = optimize(&src, target);
            assert!(m.to_string().contains("vfmul"), "{m}");
            // A reassociated sum can return 4 here, while the left fold returns 1.
            let large = if ty == "f32" {
                16777216.0
            } else {
                9007199254740992.0
            };
            let inputs = [
                large, 1.0, -large, 1.0, -0.0, 0.0, large, 1.0, -large, 1.0, 3.0,
            ];
            for n in [0, 1, 7, 8, 9, inputs.len()] {
                let left = &inputs[..n];
                let right = vec![1.0; n];
                for seed in [0.0, -0.0, 1.0] {
                    assert_eq!(
                        execute(&m, left, &right, seed),
                        execute(&original, left, &right, seed),
                        "{ty}, n={n}, seed={seed}"
                    );
                }
            }
            let left = vec![1.25; 19];
            let right = vec![-3.5; 19];
            assert_eq!(
                execute(&m, &left, &right, -7.0),
                execute(&original, &left, &right, -7.0)
            );
            let text = m.to_string();
            let mut twice = m.clone();
            keel_opt::run_passes_for_target(&mut twice, &["loop-dot"], target).unwrap();
            assert_eq!(text, twice.to_string());
            assert_eq!(text, optimize(&src, target).to_string());
        }
    }
}
#[test]
fn nonfinite_products_preserve_operand_order() {
    for ty in ["f32", "f64"] {
        let src = source(ty);
        let original = keel_ir::parse(&src).unwrap();
        let m = optimize(&src, VectorTarget::Aarch64Neon);
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut left = vec![1.0; 9];
            left[2] = value;
            let right = vec![2.0; 9];
            assert_eq!(
                execute(&m, &left, &right, 0.0),
                execute(&original, &left, &right, 0.0)
            );
        }
    }
}
#[test]
fn observable_loads_nonzero_starts_and_other_recurrences_stay_scalar() {
    let src = source("f64");
    for variant in [
        src.replace("trap_ub ", ""),
        src.replace("trap_ub ", "volatile "),
        src.replace("v4 = iconst.i64 0", "v4 = iconst.i64 1"),
        src.replace("icmp.ult", "icmp.slt"),
        src.replace("fmul v11,v13", "fadd v11,v13"),
        src.replace("fadd v6,v14", "fadd v14,v6"),
        src.replace("shl v5,v8", "shl v2,v8"),
    ] {
        let m = optimize(&variant, VectorTarget::Aarch64Neon);
        assert!(!m.to_string().contains("vfmul"), "{m}");
    }
    assert!(
        !optimize(&src, VectorTarget::Portable)
            .to_string()
            .contains("vfmul")
    );
    assert!(
        optimize(
            &src.replace("trap_ub ", "notrap "),
            VectorTarget::Aarch64Neon
        )
        .to_string()
        .contains("vfmul")
    );
}
#[test]
fn wrapping_ranges_take_original_scalar_path() {
    let m = optimize(&source("f64"), VectorTarget::Aarch64Neon);
    let f = &m.functions[0];
    // The first multiplication of n*8 wraps. The guard must select the scalar
    // loop, whose first trapping load gives the same result as the original.
    let original = keel_ir::parse(&source("f64")).unwrap();
    for (a, b, n) in [
        (u64::MAX - 7, 0, 8),
        (0, u64::MAX - 7, 8),
        (0, 0, 1u64 << 61),
    ] {
        let args = [Val::Ptr(a), Val::Ptr(b), Val::I64(n), Val::f64(0.0)];
        let got = Interpreter::for_function(f).call("dot", &args);
        let expected = Interpreter::for_function(&original.functions[0]).call("dot", &args);
        assert_eq!(got, expected);
    }
    let vectors = f
        .layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::VectorBinary { .. }))
        .count();
    assert_eq!(vectors, 4);
}

#[test]
fn relaxed_dot_uses_vector_recurrences_and_preserves_seed_and_tails() {
    for ty in ["f32", "f64"] {
        let src = source(ty);
        let src = src.replacen(" {\n", " fast_math {\n", 1);
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            let m = optimize(&src, target);
            assert!(m.to_string().contains("vfadd"), "{m}");
            let left: Vec<_> = (0..101).map(|i| (i as f64 - 50.0) / 8.0).collect();
            let right: Vec<_> = (0..101).map(|i| (i % 7) as f64 / 4.0).collect();
            for n in [0, 1, 7, 8, 9, 16, 17, 101] {
                let expected = 3.5
                    + left[..n]
                        .iter()
                        .zip(&right[..n])
                        .map(|(a, b)| a * b)
                        .sum::<f64>();
                let actual = execute(&m, &left[..n], &right[..n], 3.5).as_f64().unwrap();
                assert_eq!(actual, expected, "{ty}, {target:?}, n={n}");
            }
        }
    }
}
