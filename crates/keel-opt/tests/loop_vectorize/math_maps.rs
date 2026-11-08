use super::{kernel, optimize, packed};
use keel_ir::{Module, Type};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

fn normalization(ty: Type) -> String {
    kernel(ty, Type::I64, "fadd", false).replace(
        " v17 = fadd v15,v16",
        " v20 = fmul v15,v15\n v21 = fmul v16,v16\n v22 = fadd v20,v21\n v23 = fsqrt v22\n v17 = fdiv v15,v23",
    )
}

fn execute(m: &Module, ty: Type, n: u64, overlap: Option<u64>) -> Vec<u8> {
    let mut interpreter = Interpreter::new(m);
    let encode = |x: f64| {
        if ty == Type::F32 {
            (x as f32).to_le_bytes().to_vec()
        } else {
            x.to_le_bytes().to_vec()
        }
    };
    let a: Vec<_> = (0..=n)
        .flat_map(|i| encode((i as f64 - 11.0) / 4.0))
        .collect();
    let b: Vec<_> = (0..=n)
        .flat_map(|i| encode((i as f64 + 1.0) / 8.0))
        .collect();
    let memory = interpreter.memory_mut();
    let a = memory.alloc_bytes(&a, 16);
    let b = memory.alloc_bytes(&b, 16);
    let bytes = u64::from(ty.bytes().unwrap());
    let out = match overlap {
        Some(offset) => a + offset * bytes,
        None => memory.alloc((n + 1) * bytes, 16),
    };
    assert!(matches!(
        interpreter.call(
            "map",
            &[Val::Ptr(out), Val::Ptr(a), Val::Ptr(b), Val::I64(n)]
        ),
        Outcome::Return(_)
    ));
    interpreter.memory().read(out, n * bytes).unwrap().to_vec()
}

#[test]
fn sqrt_maps_vectorize_only_with_permission_and_keep_alias_guards_and_tails() {
    for ty in [Type::F32, Type::F64] {
        let strict = normalization(ty);
        let source = strict.replacen(" {\n", " fast_math {\n", 1);
        let original = keel_ir::parse(&source).unwrap();
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            assert_eq!(packed(&optimize(&strict, target)), 0);
            let optimized = optimize(&source, target);
            assert!(optimized.to_string().contains("vfsqrt"), "{optimized}");
            for n in [0, 1, 3, 4, 7, 8, 16, 17, 37] {
                for overlap in [None, Some(0), Some(1)] {
                    assert_eq!(
                        execute(&optimized, ty, n, overlap),
                        execute(&original, ty, n, overlap),
                        "{ty}, {target:?}, n={n}, overlap={overlap:?}"
                    );
                }
            }
            let mut twice = optimized.clone();
            keel_opt::run_passes_for_target(&mut twice, &["loop-vectorize"], target).unwrap();
            assert_eq!(twice.to_string(), optimized.to_string());
        }
    }
}

#[test]
fn packed_arithmetic_chains_unroll_with_guarded_tails_and_respect_policy() {
    for ty in [Type::F32, Type::F64] {
        let source = kernel(ty, Type::I64, "fadd", false)
            .replace(" v14 = ptradd v2,v11\n", "")
            .replace(&format!(" v16 = load.{ty} trap_ub v14\n"), "")
            .replace(" v17 = fadd v15,v16", &format!(" v20 = fconst.{ty} 0.125\n v21 = fconst.{ty} 0.0625\n v22 = fconst.{ty} 0.125\n v23 = fconst.{ty} 0.25\n v24 = fmul v15,v20\n v25 = fadd v24,v21\n v26 = fmul v25,v15\n v27 = fadd v26,v22\n v28 = fmul v27,v15\n v17 = fadd v28,v23"))
            .replacen(" {\n", " fast_math {\n", 1);
        let original = keel_ir::parse(&source).unwrap();
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            let optimized = optimize(&source, target);
            let vector_loads = |m: &Module| {
                m.functions[0].layout_insts().filter(|(_,i)| matches!(m.functions[0].inst_data(*i), keel_ir::InstData::Load {ty,..} if ty.is_vector())).count()
            };
            assert_eq!(
                vector_loads(&optimized),
                if target == VectorTarget::Aarch64Neon {
                    4
                } else {
                    1
                },
                "{optimized}"
            );
            let policy = source.replace("fast_math", "fast_math optimize(\"no-unroll-loops\")");
            assert_eq!(vector_loads(&optimize(&policy, target)), 1);
            for n in [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 65] {
                for overlap in [None, Some(0), Some(1)] {
                    assert_eq!(
                        execute(&optimized, ty, n, overlap),
                        execute(&original, ty, n, overlap),
                        "{ty}, {target:?}, n={n}, overlap={overlap:?}"
                    );
                }
            }
        }
    }
}
