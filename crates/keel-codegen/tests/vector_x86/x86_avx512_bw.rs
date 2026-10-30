//! BW arithmetic has independent scalar semantics and exact masked memory.
#[path = "../common/avx512_bw.rs"]
mod common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type, intrinsic::X86Avx512BwKind};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[test]
fn bw_core_contracts_effects_and_function_permissions() {
    let cases = common::cases();
    assert_eq!(cases.iter().filter(|c| !c.alias).count(), 78);
    for case in cases.iter().filter(|c| !c.alias) {
        let d = case.id.x86_avx512_bw_core().unwrap();
        let info = case.id.info();
        let expected_mask = match d.vector.lanes() {
            8 => Type::I8,
            16 => Type::I16,
            32 => Type::I32,
            64 => Type::I64,
            _ => unreachable!(),
        };
        assert_eq!(d.mask, expected_mask);
        let inst = InstData::Intrinsic {
            id: case.id,
            args: Default::default(),
        };
        let effect = match d.kind {
            X86Avx512BwKind::Load => MemoryEffects::Read,
            X86Avx512BwKind::Store => MemoryEffects::Write,
            _ => MemoryEffects::None,
        };
        assert_eq!(inst.memory_effects(), effect);
        assert_eq!(inst.may_trap(), !info.pure);
        assert_eq!(
            info.params[if matches!(d.kind, X86Avx512BwKind::Load | X86Avx512BwKind::Store) {
                2
            } else {
                1
            }],
            expected_mask
        );
        let source = ir(&[*case]);
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
        assert!(
            keel_codegen::x86_64::compile(&keel_ir::parse(&no_bw).unwrap()).is_err(),
            "{}",
            info.name
        );
        if d.vector.bytes() < 64 {
            assert!(
                keel_codegen::x86_64::compile(
                    &keel_ir::parse(&source.replace(",avx512vl", "")).unwrap()
                )
                .is_err(),
                "{}",
                info.name
            );
        }
    }
    let case = cases
        .iter()
        .find(|c| c.id == Intrinsic::X86Avx512BwAddb512)
        .unwrap();
    let source = ir(&[*case]).replace(
        "target_features(\"avx512bw\")",
        "target_features(\"-avx512bw\")",
    );
    let module = keel_ir::parse(&source).unwrap();
    let features = Features::parse("avx512bw,avx512vl").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
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
fn bw_core_native_scalar_alias_spill_and_guard_page_oracles() {
    let cases = common::cases();
    let source = ir(&cases);
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-bw-core-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
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
            #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
            if std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512vl")
            {
                let exe = dir.join(name);
                let linked = Command::new("cc")
                    .arg(dir.join("runner.c"))
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
                    "{allocation:?} spilled={spilled}: {output:?}"
                );
            }
        }
    }
}

fn ir(cases: &[common::Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let d = case.id.x86_avx512_bw_core().unwrap();
        let shape = d.vector;
        let name = case.name();
        let features = if shape.bytes() == 64 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,ptr,i64) target_features(\"{features}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i64):\n"
        );
        let mask = if d.mask == Type::I64 {
            "v4"
        } else {
            source += &format!(" v5 = trunc.{} v4\n", d.mask);
            "v5"
        };
        source += &format!(" v6 = load.{shape} align 1 v0\n");
        if matches!(d.kind, X86Avx512BwKind::Binary | X86Avx512BwKind::Move) {
            source += &format!(" v7 = load.{shape} align 1 v1\n v8 = load.{shape} align 1 v2\n");
            let arguments = if d.kind == X86Avx512BwKind::Move {
                format!("v6,{mask},v7")
            } else if case.alias {
                format!("v7,{mask},v6,v7")
            } else {
                format!("v6,{mask},v7,v8")
            };
            source += &format!(
                " v9 = intrinsic {}({arguments})\n store align 1 v9,v3\n",
                d.name
            );
        } else if d.kind == X86Avx512BwKind::Load {
            source += &format!(
                " v9 = intrinsic {}(v1,v6,{mask})\n store align 1 v9,v3\n",
                d.name
            );
        } else {
            source += &format!(" intrinsic {}(v1,v6,{mask})\n", d.name);
        }
        for (offset, value) in [(64, "v6"), (128, "v7"), (192, "v8"), (256, "v4")] {
            if offset == 128 || offset == 192 {
                if !matches!(d.kind, X86Avx512BwKind::Binary | X86Avx512BwKind::Move) {
                    continue;
                }
            }
            source += &format!(
                " t{offset} = iconst.i64 {offset}\n p{offset} = ptradd v3,t{offset}\n store align 1 {value},p{offset}\n"
            );
        }
        source += " return\n}\n";
    }
    source
}
