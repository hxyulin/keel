//! Mask domains follow multiply/pairwise output lanes, not input lane counts.
#[path = "../common/avx512_bw_mul.rs"]
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
const ONE: Pools = Pools {
    float_caller_saved: &[12],
    ..keel_codegen::x86_64::REGISTERS
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let name = case.name();
        let input = case.input;
        let result = case.result;
        let features = if result.bytes() == 64 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,ptr,i64) target_features(\"{features}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i64):\n"
        );
        let mask = if case.mask == Type::I64 {
            "v4"
        } else {
            source += &format!(" v5 = trunc.{} v4\n", case.mask);
            "v5"
        };
        source += &format!(
            " v6 = load.{result} align 1 v0\n v7 = load.{input} align 1 v1\n v8 = load.{input} align 1 v2\n"
        );
        let merge = if case.alias == 2 {
            if input == result {
                "v7"
            } else {
                source += &format!(" v9 = bitcast.{result} v7\n");
                "v9"
            }
        } else {
            "v6"
        };
        let b = if case.alias == 1 { "v7" } else { "v8" };
        source += &format!(
            " v10 = intrinsic {}({merge},{mask},v7,{b})\n store align 1 v10,v3\n",
            case.id.info().name
        );
        for (offset, value) in [(64, "v6"), (128, "v7"), (192, "v8"), (256, "v4")] {
            source += &format!(
                " n{offset} = iconst.i64 {offset}\n p{offset} = ptradd v3,n{offset}\n store align 1 {value},p{offset}\n"
            );
        }
        source += " return\n}\n";
    }
    source
}
#[test]
fn bw_mul_contracts_output_masks_and_local_features() {
    let cases = common::cases();
    let mut unique = std::collections::HashSet::new();
    for case in &cases {
        let d = case.id.x86_avx512_bw_mul().unwrap();
        assert_eq!(d.input, case.input);
        assert_eq!(d.result, case.result);
        assert_eq!(d.mask, case.mask);
        assert_eq!(d.input.bytes(), d.result.bytes());
        let info = case.id.info();
        assert!(info.pure && case.id.is_x86_target());
        assert_eq!(
            info.params,
            [
                Type::Vector(case.result),
                case.mask,
                Type::Vector(case.input),
                Type::Vector(case.input)
            ]
        );
        assert_eq!(info.returns, [Type::Vector(case.result)]);
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
        if case.result.bytes() < 64 {
            assert!(
                keel_codegen::x86_64::compile(
                    &keel_ir::parse(&source.replace(",avx512vl", "")).unwrap()
                )
                .is_err()
            );
        }
    }
    assert_eq!(unique.len(), 24);
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
fn bw_mul_native_corner_cases_lane_boundaries_aliases_and_spills() {
    let cases = common::cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-bw-mul-{}", std::process::id()));
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
        for (label, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("spill", &EMPTY),
            ("xmm12", &ONE),
        ] {
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
            let name = format!("{allocation:?}-{label}");
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
                    "{allocation:?} pool={label}: {output:?}"
                );
            }
        }
    }
}
