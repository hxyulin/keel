use keel_ir::{Module, parse, verify_module};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;
const SOURCE: &str = include_str!("columns.keel");
fn apply(source: &str, target: VectorTarget) -> Module {
    let mut m = parse(source).unwrap();
    verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-column-reduce"], target).unwrap();
    verify_module(&m).unwrap();
    m
}
#[test]
fn column_tiles_keep_order_and_alias_fallback() {
    for ty in ["f32", "f64"] {
        let source = if ty == "f32" {
            SOURCE
                .replace("f64", "f32")
                .replace("iconst.i64 3", "iconst.i64 2")
        } else {
            SOURCE.to_owned()
        };
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            let mut m = apply(&source, target);
            assert_eq!(m.functions.len(), 2, "{m}");
            let once = m.to_string();
            keel_opt::run_passes_for_target(&mut m, &["loop-column-reduce"], target).unwrap();
            assert_eq!(m.to_string(), once);
            for (n, alias) in [
                (0, false),
                (1, false),
                (127, false),
                (128, false),
                (129, false),
                (128, true),
                (129, true),
            ] {
                let bytes = if ty == "f32" { 4 } else { 8 };
                let offset = if alias { 1 } else { n * n + 1 };
                let large = if bytes == 4 {
                    16777216.0
                } else {
                    9007199254740992.0
                };
                let mut values: Vec<f64> = (0..n * n + n + 3)
                    .map(|i| match i % 3 {
                        0 => large,
                        1 => 1.0,
                        _ => -large,
                    })
                    .collect();
                for c in 0..n {
                    let mut sum = 0.;
                    for r in 0..n {
                        sum = if bytes == 4 {
                            (sum as f32 + values[r * n + c] as f32) as f64
                        } else {
                            sum + values[r * n + c]
                        };
                    }
                    values[offset + c] = sum;
                }
                let raw: Vec<u8> = (0..n * n + n + 3)
                    .flat_map(|i| {
                        let x = match i % 3 {
                            0 => large,
                            1 => 1.0,
                            _ => -large,
                        };
                        if bytes == 4 {
                            (x as f32).to_le_bytes().to_vec()
                        } else {
                            x.to_le_bytes().to_vec()
                        }
                    })
                    .collect();
                let mut interp = Interpreter::new(&m);
                let base = interp.memory_mut().alloc_bytes(&raw, 16);
                let result = interp.call(
                    "columns",
                    &[
                        Val::Ptr(base),
                        Val::Ptr(base + (offset * bytes) as u64),
                        Val::I64(n as u64),
                    ],
                );
                assert_eq!(result, Outcome::Return(vec![]), "{ty} n={n} alias={alias}");
                let expected: Vec<u8> = values
                    .iter()
                    .flat_map(|&x| {
                        if bytes == 4 {
                            (x as f32).to_le_bytes().to_vec()
                        } else {
                            x.to_le_bytes().to_vec()
                        }
                    })
                    .collect();
                assert_eq!(
                    interp.memory().read(base, raw.len() as u64).unwrap(),
                    expected,
                    "{ty} n={n} alias={alias}"
                );
            }
        }
    }
}
#[test]
fn column_recipe_declines_unmatched_effects_and_contracts() {
    assert_eq!(apply(SOURCE, VectorTarget::Portable).functions.len(), 1);
    for source in [
        SOURCE.replace("load.f64 trap_ub", "load.f64"),
        SOURCE.replace("load.f64 trap_ub", "load.f64 volatile"),
        SOURCE.replace("store trap_ub", "store"),
        SOURCE.replace("store trap_ub", "store volatile"),
        SOURCE.replace("v21 = fadd v12, v20", "v21 = fadd v20, v12"),
        SOURCE.replace("fconst.f64 0.0", "fconst.f64 -0.0"),
        SOURCE.replace("v14 = imul v2, v9", "v14 = iadd v2, v9"),
        SOURCE.replace(
            "v21 = fadd v12, v20",
            "store trap_ub v20, v0\n    v21 = fadd v12, v20",
        ),
    ] {
        let before = parse(&source).unwrap();
        let after = apply(&source, VectorTarget::Aarch64Neon);
        assert_eq!(before.to_string(), after.to_string(), "{source}");
    }
}

#[test]
fn column_tiles_remain_opt_in() {
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Balanced,
        keel_opt::OptimizationTier::Size,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"loop-column-reduce"));
    }
}
