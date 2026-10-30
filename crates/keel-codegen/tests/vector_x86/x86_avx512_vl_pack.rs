//! VL packed prefixes access only selected consecutive elements, including none.
#[path = "../common/avx512_vl_pack.rs"]
mod common;
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
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
    for (width, suffix, shape, _, _) in common::SHAPES {
        let bits = shape.lane_type().bytes().unwrap() * 8;
        let integer = format!("i{bits}x{}", shape.lanes());
        let zero = if shape.lane_type().is_int() {
            "v7"
        } else {
            "v8"
        };
        let initialization =
            format!(" v6=iconst.i{bits} 0\n v7=vsplat.{integer} v6\n v8=bitcast.{shape} v7\n");
        for op in ["compress", "expand"] {
            for mode in 0..3 {
                let name = format!("{op}_{suffix}{width}_{mode}");
                let merge = if mode == 2 { zero } else { "v4" };
                let input = if mode == 1 { "v4" } else { "v5" };
                source += &format!(
                    "func @{name}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{shape} align 1 v0\n v5=load.{shape} align 1 v1\n{initialization}"
                );
                if mode == 1 {
                    source += " v9=load.i64x8 align 1 v0\n";
                }
                source += &format!(
                    " v10=intrinsic x86_avx512vl_{op}{suffix}{width}({merge},v3,{input})\n store align 1 v10,v2\n v11=iconst.i64 64\n v12=ptradd v2,v11\n store align 1 v4,v12\n v13=ptradd v12,v11\n store align 1 v5,v13\n v14=ptradd v13,v11\n store align 1 v3,v14\n"
                );
                if mode == 1 {
                    source += " v15=ptradd v14,v11\n store align 1 v9,v15\n";
                }
                source += " return\n}\n";
            }
        }
        source += &format!(
            "func @compressstore_{suffix}{width}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{shape} align 1 v0\n intrinsic x86_avx512vl_compressstore{suffix}{width}(v1,v4,v3)\n store align 1 v4,v2\n v5=iconst.i64 64\n v6=ptradd v2,v5\n store align 1 v3,v6\n return\n}}\n"
        );
        for mode in 0..2 {
            let merge = if mode == 1 { zero } else { "v4" };
            source += &format!(
                "func @expandload_{suffix}{width}_{mode}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{shape} align 1 v0\n{initialization} v10=intrinsic x86_avx512vl_expandload{suffix}{width}(v1,{merge},v3)\n store align 1 v10,v2\n v11=iconst.i64 64\n v12=ptradd v2,v11\n store align 1 v4,v12\n v13=ptradd v12,v11\n store align 1 v3,v13\n return\n}}\n"
            );
        }
    }
    source
}
#[test]
fn vl_pack_registers_prefix_memory_guard_pages_aliases_and_spills() {
    let mut module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512bw && !features.avx512dq);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "keel-vl-pack-native-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), common::harness()).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512vl")
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
            fs::write(directory.join("pack.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("pack.o"))
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
fn vl_pack_effects_exact_types_and_per_function_permissions() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_vl_pack().is_some())
        .collect();
    assert_eq!(ids.len(), 32);
    for id in ids {
        let store = id.is_x86_avx512_compress_store();
        let load = id.is_x86_avx512_expand_load();
        let instruction = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(
            instruction.memory_effects(),
            if store {
                MemoryEffects::Write
            } else if load {
                MemoryEffects::Read
            } else {
                MemoryEffects::None
            }
        );
        assert_eq!(instruction.may_trap(), store || load);
        assert_eq!(id.info().pure, !store && !load);
    }
    let source = "func @operation(i32x4,i8,i32x4)->i32x4 {\nblock0(v0:i32x4,v1:i8,v2:i32x4):\n v3=intrinsic x86_avx512vl_compressd128(v0,v1,v2)\n return v3\n}\n";
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
    for features in [
        Features::NONE,
        Features::NONE.with_features("avx512f").unwrap(),
        Features::NONE.with_features("avx2").unwrap(),
    ] {
        assert!(compile(source, features).is_err());
    }
    compile(
        &source.replace("->i32x4 {", "->i32x4 target_features(\"avx512vl\") {"),
        Features::NONE,
    )
    .unwrap();
    assert!(
        compile(
            &source.replace("->i32x4 {", "->i32x4 target_features(\"-avx512vl\") {"),
            Features::NONE.with_features("avx512vl").unwrap()
        )
        .is_err()
    );
    assert!(
        keel_ir::verify_module(&keel_ir::parse(&source.replace("v0,v1,v2", "v0,v2,v1")).unwrap())
            .is_err()
    );
}
