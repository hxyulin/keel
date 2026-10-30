//! Short conversion tails and memory footprints follow encoded widths.
#[path = "../common/avx512_bw_convert.rs"]
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
        let input = case.input();
        let output = case.output();
        let features = if case.encoded == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,i64) target_features(\"{features}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i64):\n v4 = trunc.{} v3\n",
            case.mask
        );
        let (original, src) = if case.kind == 3 {
            source += &format!(
                " v5 = load.{input} align 1 v0\n intrinsic {}(v1,v5,v4)\n",
                case.id.info().name
            );
            ("v5", None)
        } else {
            source += &format!(" v5 = load.{output} align 1 v0\n");
            if case.kind == 1 {
                source += &format!(" v7 = intrinsic {}(v1,v5,v4)\n", case.id.info().name);
            } else {
                let pointer = if case.alias { "v0" } else { "v1" };
                if case.alias && input.bytes() == output.bytes() {
                    source += &format!(" v6 = bitcast.{input} v5\n");
                } else {
                    source += &format!(" v6 = load.{input} align 1 {pointer}\n");
                }
                source += &format!(" v7 = intrinsic {}(v5,v4,v6)\n", case.id.info().name);
            }
            source += " store align 1 v7,v2\n";
            ("v5", if case.kind == 1 { None } else { Some("v6") })
        };
        source +=
            &format!(" n64 = iconst.i64 64\n p64 = ptradd v2,n64\n store align 1 {original},p64\n");
        if let Some(src) = src {
            source += &format!(
                " n128 = iconst.i64 128\n p128 = ptradd v2,n128\n store align 1 {src},p128\n"
            );
        }
        source +=
            " n192 = iconst.i64 192\n p192 = ptradd v2,n192\n store align 1 v3,p192\n return\n}\n";
    }
    source
}
#[test]
fn bw_convert_encoded_width_signatures_effects_and_feature_gates() {
    let cases = common::cases();
    let mut unique = std::collections::HashSet::new();
    for case in &cases {
        let d = case.id.x86_avx512_bw_convert().unwrap();
        assert_eq!(d.encoded_bits, case.encoded);
        assert_eq!(d.byte, case.byte);
        assert_eq!(d.word, case.word);
        assert_eq!(d.mask, case.mask);
        assert_eq!(d.memory_bytes, case.encoded / 16);
        let info = case.id.info();
        assert!(case.id.is_x86_target());
        let input = Type::Vector(case.input());
        let output = Type::Vector(case.output());
        let expected = match case.kind {
            0 | 2 => vec![output, case.mask, input],
            1 => vec![Type::Ptr, output, case.mask],
            3 => vec![Type::Ptr, input, case.mask],
            _ => unreachable!(),
        };
        assert_eq!(info.params, expected);
        assert_eq!(
            info.returns,
            if case.kind == 3 { vec![] } else { vec![output] }
        );
        let effect = match case.kind {
            1 => MemoryEffects::Read,
            3 => MemoryEffects::Write,
            _ => MemoryEffects::None,
        };
        let inst = InstData::Intrinsic {
            id: case.id,
            args: Default::default(),
        };
        assert_eq!(inst.memory_effects(), effect);
        assert_eq!(inst.may_trap(), case.kind == 1 || case.kind == 3);
        assert_eq!(info.pure, case.kind == 0 || case.kind == 2);
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
        if case.encoded < 512 {
            assert!(
                keel_codegen::x86_64::compile(
                    &keel_ir::parse(&source.replace(",avx512vl", "")).unwrap()
                )
                .is_err()
            );
        }
    }
    assert_eq!(unique.len(), 30);
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
fn bw_convert_native_short_tails_exact_memory_guards_and_overlaps() {
    let cases = common::cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = common::directory("keel-bw-convert");
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
