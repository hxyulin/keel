//! Lane-local byte/half-word shuffles and full-vector word selects stay distinct.
#[path = "../common/avx512_bw_permute.rs"]
mod common;
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
const TWELVE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut source = String::new();
    for case in common::cases() {
        let name = case.name();
        let shape = case.vector;
        let scalar = shape.lane_type();
        let maskarg = if case.mask == Type::I64 { "v5" } else { "v6" };
        let index = if case.mode == 1 { "v8" } else { "v9" };
        let merge = match case.mode {
            1 => "v8",
            2 => "v12",
            4 => "v9",
            _ => "v7",
        };
        let mask = if case.mode == 3 { "v13" } else { maskarg };
        let control = case.control;
        source += &format!(
            "func @{name}(ptr,ptr,ptr,ptr,ptr,i64) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i64):\n"
        );
        if case.mask != Type::I64 {
            source += &format!(" v6=trunc.{} v5\n", case.mask);
        }
        source += &format!(
            " v7=load.{shape} align 1 v0\n v8=load.{shape} align 1 v1\n v9=load.{shape} align 1 v2\n v10=load.{shape} align 1 v3\n v11=iconst.{scalar} 0\n v12=vsplat.{shape} v11\n v13=iconst.{} -1\n v14=iconst.i32 {control}\n",
            case.mask
        );
        if case.mode == 1 {
            source += " v15=load.i64x8 align 1 v0\n";
        }
        let args = match case.operation {
            0 => format!("{merge},{mask},v8,{index}"),
            1 => format!("{merge},{mask},v8,{index},v14"),
            2 | 3 => format!("{merge},{mask},v8,v14"),
            4 => format!("{merge},{mask},{index},v8"),
            5 => format!("{merge},{mask},v8,{index},v10"),
            _ => "v8,v14".to_string(),
        };
        source += &format!(
            " v16=intrinsic x86_avx512bw_{}{}({args})\n store align 1 v16,v4\n",
            case.stem, case.width
        );
        for (offset, value) in [
            (64, "v7"),
            (128, "v8"),
            (192, "v9"),
            (256, "v10"),
            (320, "v5"),
        ] {
            source += &format!(
                " n{offset}=iconst.i64 {offset}\n p{offset}=ptradd v4,n{offset}\n store align 1 {value},p{offset}\n"
            );
        }
        if case.mode == 1 {
            source += " n384=iconst.i64 384\n p384=ptradd v4,n384\n store align 1 v15,p384\n";
        }
        source += " return\n}\n";
    }
    source
}
#[test]
fn bw_permutations_scalar_system_lane_masks_aliases_and_spills() {
    let mut module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512bw,avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512dq);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "keel-bw-permute-native-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), common::harness()).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args([
                "-O1",
                "-fno-tree-vectorize",
                "-mavx512f",
                "-mavx512bw",
                "-mavx512vl",
                "-c",
            ])
            .arg(directory.join("oracle.c"))
            .arg("-o")
            .arg(directory.join("oracle.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE, &TWELVE] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
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
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(directory.join("permute.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("permute.o"))
                    .arg("-o")
                    .arg(directory.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(directory.join("run"))
                        .status()
                        .unwrap()
                        .success(),
                    "{allocation:?} pools={pools:?}"
                );
            }
        }
    }
}
#[test]
fn bw_permutation_metadata_controls_and_function_permissions() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_bw_permute().is_some())
        .collect();
    assert_eq!(ids.len(), 20);
    for id in ids {
        let d = id.x86_avx512_bw_permute().unwrap();
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure && !inst.may_trap());
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
        let mut params = Vec::new();
        let mut args = Vec::new();
        let mut initialization = String::new();
        for (index, ty) in d.params.iter().enumerate() {
            if Some(index) == d.kind.control_index() {
                initialization += &format!(" v{index}=iconst.i32 1\n");
            } else {
                params.push(format!("v{index}:{ty}"));
            }
            args.push(format!("v{index}"));
        }
        let types = d
            .params
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != d.kind.control_index())
            .map(|(_, ty)| ty.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let body = format!(
            "func @operation({types})->{} {{\nblock0({}):\n{initialization} v9=intrinsic {}({})\n return v9\n}}\n",
            d.vector,
            params.join(","),
            id.info().name,
            args.join(",")
        );
        let compile = |source: &str, features| {
            let module = keel_ir::parse(source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
        };
        assert!(
            compile(
                &body,
                Features::NONE.with_features("avx512f,avx512vl").unwrap()
            )
            .is_err()
        );
        assert_eq!(
            compile(&body, Features::NONE.with_features("avx512bw").unwrap()).is_ok(),
            d.vector.bytes() == 64
        );
        let enabled = body.replace(
            &format!("->{} {{", d.vector),
            &format!("->{} target_features(\"avx512bw,avx512vl\") {{", d.vector),
        );
        compile(&enabled, Features::NONE).unwrap();
        let disabled = body.replace(
            &format!("->{} {{", d.vector),
            &format!("->{} target_features(\"-avx512bw\") {{", d.vector),
        );
        assert!(
            compile(
                &disabled,
                Features::NONE.with_features("avx512bw,avx512vl").unwrap()
            )
            .is_err()
        );
        if d.kind.control_index().is_some() {
            for control in [-1, 256] {
                let invalid = body.replace("iconst.i32 1", &format!("iconst.i32 {control}"));
                assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
            }
        }
    }
}
