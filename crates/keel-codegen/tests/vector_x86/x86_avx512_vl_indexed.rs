//! Signed mixed-width VSIB addressing, short tails and destructive K masks.
#[path = "../common/avx512_vl_indexed.rs"]
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
    for shape in common::SHAPES {
        let width = shape.bits;
        let tag = shape.tag;
        let index = shape.index;
        let data = shape.data;
        for scale in [1, 2, 4, 8] {
            for alias in [false, true] {
                let suffix = format!("{tag}{width}_s{scale}_a{}", u8::from(alias));
                let overlap = alias && index.bytes() == data.bytes();
                let index_arg = if overlap { "v9" } else { "v5" };
                for operation in ["gather", "scatter", "scatterall"] {
                    let name = format!("{operation}_{suffix}");
                    let opcode = if operation == "gather" {
                        shape.gather
                    } else {
                        shape.scatter
                    };
                    let mask = if operation == "scatterall" {
                        "v8"
                    } else {
                        "v4"
                    };
                    source += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{index} align 1 v1\n v6=load.{data} align 1 v2\n v7=iconst.i32 {scale}\n v8=iconst.i8 -1\n"
                    );
                    if overlap {
                        source += &format!(" v9=bitcast.{index} v6\n");
                    }
                    if alias {
                        source += " v10=load.i64x8 align 1 v2\n";
                    }
                    let result = if operation == "gather" {
                        source += &format!(
                            " v11=intrinsic x86_avx512vl_{opcode}{width}(v0,{index_arg},v6,{mask},v7)\n"
                        );
                        "v11"
                    } else {
                        source += &format!(
                            " intrinsic x86_avx512vl_{opcode}{width}(v0,{index_arg},v6,{mask},v7)\n"
                        );
                        "v6"
                    };
                    source += &format!(
                        " store align 1 {result},v3\n v12=iconst.i64 64\n v13=ptradd v3,v12\n store align 1 {index_arg},v13\n v14=ptradd v13,v12\n store align 1 v6,v14\n v15=ptradd v14,v12\n store align 1 v4,v15\n"
                    );
                    if alias {
                        source += " v16=ptradd v15,v12\n store align 1 v10,v16\n";
                    }
                    source += " return\n}\n";
                }
            }
        }
    }
    source
}
#[test]
fn vl_indexed_scalar_system_guards_short_tails_aliases_and_spills() {
    let mut module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512dq && !features.avx512bw);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "keel-vl-indexed-native-{}-{unique}",
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
            fs::write(directory.join("indexed.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("indexed.o"))
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
fn vl_indexed_explicit_width_signatures_scale_and_local_features() {
    let mut count = 0;
    for shape in common::SHAPES {
        for opcode in [shape.gather, shape.scatter] {
            let id = Intrinsic::from_name(&format!("x86_avx512vl_{opcode}{}", shape.bits)).unwrap();
            let descriptor = id.x86_avx512_vl_indexed().unwrap();
            assert_eq!(descriptor.encoded_bits, shape.bits);
            assert_eq!(descriptor.index, shape.index);
            assert_eq!(descriptor.data, shape.data);
            assert_eq!(descriptor.active_lanes, shape.lanes);
            let inst = InstData::Intrinsic { id, args: vec![] };
            assert!(!id.info().pure && inst.may_trap());
            assert_eq!(
                inst.memory_effects(),
                if id.is_x86_avx512_scatter() {
                    MemoryEffects::Write
                } else {
                    MemoryEffects::Read
                }
            );
            count += 1;
        }
    }
    assert_eq!(count, 32);
    let source = "func @operation(ptr,i64x2,i32x4,i8)->i32x4 {\nblock0(v0:ptr,v1:i64x2,v2:i32x4,v3:i8):\n v4=iconst.i32 1\n v5=intrinsic x86_avx512vl_vpgatherqd128(v0,v1,v2,v3,v4)\n return v5\n}\n";
    for scale in [0, 3, 16, -1] {
        let invalid = source.replace("iconst.i32 1", &format!("iconst.i32 {scale}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
    }
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
}
