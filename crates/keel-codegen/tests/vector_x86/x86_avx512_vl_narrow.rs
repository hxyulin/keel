//! Source lane counts, short tails and selected store footprints stay independent.
#[path = "../common/avx512_vl_narrow.rs"]
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
const TWELVE: Pools = Pools {
    float_caller_saved: &[12],
    ..keel_codegen::x86_64::REGISTERS
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let name = case.name();
        let input = case.source();
        let output = case.result();
        let intrinsic = case.intrinsic_name();
        source += &format!(
            "func @{name}(ptr,ptr,ptr,i64) target_features(\"avx512f,avx512vl\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i64):\n v4=trunc.i8 v3\n"
        );
        if case.store {
            source += &format!(
                " v5=load.{input} align 1 v0\n intrinsic {intrinsic}(v1,v5,v4)\n n64=iconst.i64 64\n p64=ptradd v2,n64\n store align 1 v5,p64\n"
            );
        } else {
            source += &format!(" v5=load.{output} align 1 v0\n");
            if case.mode == 1 {
                if input.bytes() == 16 {
                    source += &format!(" v6=bitcast.{input} v5\n");
                } else {
                    source += &format!(" v6=load.{input} align 1 v0\n");
                }
                source += " live=load.i64x8 align 1 v0\n";
            } else {
                source += &format!(" v6=load.{input} align 1 v1\n");
            }
            source += &format!(
                " zero=iconst.{} 0\n z=vsplat.{output} zero\n all=iconst.i8 -1\n",
                output.lane_type()
            );
            let merge = if case.mode == 2 { "z" } else { "v5" };
            let mask = if case.mode == 3 { "all" } else { "v4" };
            source += &format!(
                " v7=intrinsic {intrinsic}({merge},{mask},v6)\n store align 1 v7,v2\n n64=iconst.i64 64\n p64=ptradd v2,n64\n store align 1 v5,p64\n n128=iconst.i64 128\n p128=ptradd v2,n128\n store align 1 v6,p128\n"
            );
            if case.mode == 1 {
                source += " n256=iconst.i64 256\n p256=ptradd v2,n256\n store align 1 live,p256\n";
            }
        }
        source +=
            " n192=iconst.i64 192\n p192=ptradd v2,n192\n store align 1 v3,p192\n return\n}\n";
    }
    source
}
#[test]
fn vl_narrow_signatures_effects_encoded_width_and_function_permissions() {
    let cases = common::cases();
    let mut unique = std::collections::HashSet::new();
    for case in &cases {
        let id = case.id();
        if !unique.insert(id) {
            continue;
        }
        let d = id.x86_avx512_vl_narrow().unwrap();
        let info = id.info();
        assert_eq!(d.encoded_bits, case.width);
        assert_eq!(d.source, case.source());
        assert_eq!(d.result, case.result());
        assert_eq!(
            d.memory_bytes,
            case.width / u16::from(case.source_bits) * u16::from(case.destination_bits) / 8
        );
        assert_eq!(info.pure, !case.store);
        assert!(id.is_x86_target());
        assert_eq!(
            info.params,
            if case.store {
                vec![Type::Ptr, Type::Vector(case.source()), Type::I8]
            } else {
                vec![
                    Type::Vector(case.result()),
                    Type::I8,
                    Type::Vector(case.source()),
                ]
            }
        );
        assert_eq!(
            info.returns,
            if case.store {
                vec![]
            } else {
                vec![Type::Vector(case.result())]
            }
        );
        let inst = InstData::Intrinsic {
            id,
            args: Default::default(),
        };
        assert_eq!(
            inst.memory_effects(),
            if case.store {
                MemoryEffects::Write
            } else {
                MemoryEffects::None
            }
        );
        assert_eq!(inst.may_trap(), case.store);
        let text = source(&[*case]);
        let module = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_codegen::x86_64::compile(&module).unwrap();
        for missing in ["avx512f", "avx512vl,-avx512f", ""] {
            let bad_source = if missing.is_empty() {
                text.replace(" target_features(\"avx512f,avx512vl\")", "")
            } else {
                text.replace("avx512f,avx512vl", missing)
            };
            let bad = keel_ir::parse(&bad_source).unwrap();
            assert!(
                keel_codegen::x86_64::compile(&bad).is_err(),
                "{} missing {missing}",
                info.name
            );
        }
    }
    assert_eq!(unique.len(), 60);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::parse("avx512f,avx512vl").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    for disabled in ["-avx512f", "-avx512vl"] {
        let module =
            keel_ir::parse(&source(&[cases[0]]).replace("avx512f,avx512vl", disabled)).unwrap();
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline
            )
            .is_err()
        );
    }
}
#[test]
fn vl_narrow_native_scalar_system_saturation_tails_guards_and_spills() {
    let cases = common::cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(
        encoding.avx512f
            && encoding.avx512vl
            && !encoding.avx512bw
            && !encoding.avx512dq
            && !encoding.avx2
    );
    let dir = common::directory("keel-vl-narrow-native");
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let result = Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
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
            ("xmm12", &TWELVE),
        ] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
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
                let result = Command::new("clang")
                    .arg(dir.join("runner.o"))
                    .arg(path)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = Command::new(exe).output().unwrap();
                assert!(
                    result.status.success(),
                    "{allocation:?} {label}: {result:?}"
                );
            }
        }
    }
}
