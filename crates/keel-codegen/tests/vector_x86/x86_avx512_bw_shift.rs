//! Word shift counts saturate rather than wrap modulo the lane width.
#[path = "../common/avx512_bw_shift.rs"]
mod common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{InstData, MemoryEffects, Type, VectorType};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let name = case.name();
        let shape = case.vector;
        let count_shape = if case.kind == 1 {
            VectorType::I64x2
        } else {
            shape
        };
        let features = if shape.bytes() == 64 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,ptr,i64) target_features(\"{features}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i64):\n v5 = trunc.{} v4\n",
            case.mask
        );
        if case.kind == 0 {
            source += &format!(" v6 = iconst.i32 {}\n", case.immediate);
        }
        source += &format!(
            " v7 = load.{shape} align 1 v0\n v8 = load.{shape} align 1 v1\n v9 = load.{count_shape} align 1 v2\n"
        );
        let merge = if case.alias { "v8" } else { "v7" };
        let count = match case.kind {
            0 => "v6",
            1 => "v9",
            2 => {
                if case.alias {
                    "v8"
                } else {
                    "v9"
                }
            }
            _ => unreachable!(),
        };
        source += &format!(
            " v10 = intrinsic {}({merge},v5,v8,{count})\n store align 1 v10,v3\n",
            case.id.info().name
        );
        for (offset, value) in [(64, "v7"), (128, "v8"), (192, "v9"), (256, "v4")] {
            source += &format!(
                " n{offset} = iconst.i64 {offset}\n p{offset} = ptradd v3,n{offset}\n store align 1 {value},p{offset}\n"
            );
        }
        source += " return\n}\n";
    }
    source
}
#[test]
fn bw_shift_contracts_immediates_and_local_features() {
    let mut unique = std::collections::HashSet::new();
    let cases = common::cases();
    for case in &cases {
        let info = case.id.info();
        assert!(info.pure && case.id.is_x86_target());
        let d = case.id.x86_avx512_bw_shift().unwrap();
        assert_eq!(d.vector, case.vector);
        assert_eq!(d.mask, case.mask);
        let count = match case.kind {
            0 => Type::I32,
            1 => Type::Vector(VectorType::I64x2),
            2 => Type::Vector(case.vector),
            _ => unreachable!(),
        };
        assert_eq!(
            info.params,
            [
                Type::Vector(case.vector),
                case.mask,
                Type::Vector(case.vector),
                count
            ]
        );
        assert_eq!(info.returns, [Type::Vector(case.vector)]);
        let inst = InstData::Intrinsic {
            id: case.id,
            args: Default::default(),
        };
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
        assert!(!inst.may_trap());
        if !unique.insert(case.id) {
            continue;
        }
        let source = source(&[*case]);
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
        if case.kind == 0 {
            for count in [-1, 256] {
                let bad = source.replace("v6 = iconst.i32 0", &format!("v6 = iconst.i32 {count}"));
                let module = keel_ir::parse(&bad).unwrap();
                assert!(keel_ir::verify_module(&module).is_err());
                assert!(keel_codegen::x86_64::compile(&module).is_err());
            }
            let dynamic = source.replace("v6 = iconst.i32 0", "v6 = trunc.i32 v4");
            assert!(keel_ir::verify_module(&keel_ir::parse(&dynamic).unwrap()).is_err());
        }
    }
    assert_eq!(unique.len(), 27);
    let source = source(&[cases[0]]).replace(
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
fn bw_shift_native_saturating_counts_high_payload_aliases_and_spills() {
    let cases = common::cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-bw-shift-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
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
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for (spilled, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            assert!(text.contains("kmovd"));
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
