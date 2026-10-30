//! All BW predicates and full scalar masks agree with independent scalar/vendor oracles.
#[path = "../common/avx512_bw_mask.rs"]
mod common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{InstData, MemoryEffects, Type};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn ir(cases: &[common::Case], with_chain: bool) -> String {
    let mut source = String::new();
    for case in cases {
        let name = case.name();
        let shape = case.vector;
        let width = shape.bytes() * 8;
        let lane = if shape.lane_type() == Type::I8 {
            "b"
        } else {
            "w"
        };
        let features = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,i64) target_features(\"{features}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i64):\n"
        );
        let mask = if case.mask == Type::I64 {
            "v3"
        } else {
            source += &format!(" v4 = trunc.{} v3\n", case.mask);
            "v4"
        };
        source += &format!(" v5 = load.{shape} align 1 v0\n v6 = load.{shape} align 1 v1\n");
        let b = if case.alias { "v5" } else { "v6" };
        let args = match case.operation {
            0 | 1 => {
                source += &format!(" v7 = iconst.i32 {}\n", case.predicate);
                format!("v5,{b},{mask},v7")
            }
            2 | 3 => format!("v5,{b},{mask}"),
            4 => "v5".into(),
            5 => mask.into(),
            _ => unreachable!(),
        };
        source += &format!(" v8 = intrinsic {}({args})\n", case.id.info().name);
        let value = if case.operation == 5 || case.mask == Type::I64 {
            "v8"
        } else {
            source += " v9 = uext.i64 v8\n";
            "v9"
        };
        source += &format!(" store align 1 {value},v2\n");
        let chain = if !with_chain {
            "v3"
        } else if case.operation == 5 {
            source += &format!(" v10 = intrinsic x86_avx512bw_mov{lane}2m{width}(v8)\n");
            if case.mask == Type::I64 {
                "v10"
            } else {
                source += " v11 = uext.i64 v10\n";
                "v11"
            }
        } else {
            source += &format!(" v10 = intrinsic x86_avx512bw_move{lane}{width}(v5,v8,v6)\n");
            "v10"
        };
        for (offset, value) in [(64, "v5"), (128, "v6"), (192, "v3"), (256, chain)] {
            if offset == 256 && !with_chain {
                continue;
            }
            source += &format!(
                " n{offset} = iconst.i64 {offset}\n p{offset} = ptradd v2,n{offset}\n store align 1 {value},p{offset}\n"
            );
        }
        source += " return\n}\n";
    }
    source
}
#[test]
fn bw_mask_contracts_predicates_and_feature_boundaries() {
    let cases = common::cases();
    let mut unique = std::collections::HashSet::new();
    for case in &cases {
        let info = case.id.info();
        assert!(info.pure && case.id.is_x86_target());
        let descriptor = case.id.x86_avx512_bw_mask().unwrap();
        assert_eq!(descriptor.vector, case.vector);
        assert_eq!(descriptor.mask, case.mask);
        let expected = match case.operation {
            0 | 1 => vec![
                Type::Vector(case.vector),
                Type::Vector(case.vector),
                case.mask,
                Type::I32,
            ],
            2 | 3 => vec![
                Type::Vector(case.vector),
                Type::Vector(case.vector),
                case.mask,
            ],
            4 => vec![Type::Vector(case.vector)],
            5 => vec![case.mask],
            _ => unreachable!(),
        };
        assert_eq!(info.params, expected);
        assert_eq!(
            info.returns,
            if case.operation == 5 {
                vec![Type::Vector(case.vector)]
            } else {
                vec![case.mask]
            }
        );
        let inst = InstData::Intrinsic {
            id: case.id,
            args: Default::default(),
        };
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
        assert!(!inst.may_trap());
        if !unique.insert(case.id) {
            continue;
        }
        let source = ir(&[*case], false);
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::x86_64::compile(&module).unwrap();
        let no_bw = source
            .replace(
                "target_features(\"avx512bw,avx512vl\")",
                "target_features(\"avx512f,avx512vl\")",
            )
            .replace(
                "target_features(\"avx512bw\")",
                "target_features(\"avx512f\")",
            );
        assert!(keel_codegen::x86_64::compile(&keel_ir::parse(&no_bw).unwrap()).is_err());
        if case.vector.bytes() < 64 {
            assert!(
                keel_codegen::x86_64::compile(
                    &keel_ir::parse(&source.replace(",avx512vl", "")).unwrap()
                )
                .is_err()
            );
        }
        if case.operation < 2 {
            for predicate in [-1, 8, 255] {
                let bad =
                    source.replace("v7 = iconst.i32 0", &format!("v7 = iconst.i32 {predicate}"));
                let module = keel_ir::parse(&bad).unwrap();
                assert!(keel_ir::verify_module(&module).is_err());
                assert!(keel_codegen::x86_64::compile(&module).is_err());
            }
            let dynamic = source.replace("v7 = iconst.i32 0", "v7 = trunc.i32 v3");
            assert!(keel_ir::verify_module(&keel_ir::parse(&dynamic).unwrap()).is_err());
        }
    }
    assert_eq!(unique.len(), 36);
    // A locally disabled feature cannot inherit authorization from the baseline.
    let source = ir(&[cases[0]], false).replace(
        "target_features(\"avx512bw,avx512vl\")",
        "target_features(\"-avx512bw\")",
    );
    let module = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::parse("avx512bw,avx512vl").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &pipeline
        )
        .is_err()
    );
}
#[test]
fn bw_mask_native_all_predicates_high_bits_aliases_and_spills() {
    let cases = common::cases();
    let mut module = keel_ir::parse(&ir(&cases, true)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-bw-mask-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let output = Command::new("clang")
            .arg("-O2")
            .arg("-c")
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for (spilled, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            assert!(text.contains("kmovq") && text.contains("kmovd"));
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
            );
            let name = format!("{allocation:?}-{spilled}");
            let path = dir.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if native {
                let exe = dir.join(name);
                let linked = Command::new("clang")
                    .arg(dir.join("runner.o"))
                    .arg(path)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    linked.status.success(),
                    "{}",
                    String::from_utf8_lossy(&linked.stderr)
                );
                let output = Command::new(exe).output().unwrap();
                assert!(
                    output.status.success(),
                    "{allocation:?} spill={spilled}: {output:?}"
                );
            }
        }
    }
}
