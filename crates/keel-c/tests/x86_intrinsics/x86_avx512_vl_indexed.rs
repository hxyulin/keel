//! Scalar-mask gather names remain distinct from legacy AVX2 vector-mask APIs.
#[path = "../../../keel-codegen/tests/common/avx512_vl_indexed.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
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
fn source() -> String {
    let mut source = String::from("#include <immintrin.h>\n");
    for shape in common::SHAPES {
        let width = shape.bits;
        let tag = shape.tag;
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let index_bits = shape.index.bytes() * 8;
        let data_bits = shape.data.bytes() * 8;
        let iprefix = if index_bits == 128 { "_mm" } else { "_mm256" };
        let dprefix = if data_bits == 128 { "_mm" } else { "_mm256" };
        let index = format!("__m{index_bits}i");
        let memory = if shape.data.lane_type().is_int() {
            format!("si{data_bits}")
        } else if shape.data.lane_type().bytes() == Some(8) {
            "pd".into()
        } else {
            "ps".into()
        };
        let data = if shape.data.lane_type().is_int() {
            format!("__m{data_bits}i")
        } else {
            format!("__m{data_bits}{}", if memory == "pd" { "d" } else { "" })
        };
        let scatter = shape.api.replace("gather", "scatter");
        for scale in [1, 2, 4, 8] {
            for alias in [false, true] {
                let suffix = format!("{tag}{width}_s{scale}_a{}", u8::from(alias));
                let scale = if alias {
                    format!("(0x100000000ULL+{scale})")
                } else {
                    scale.to_string()
                };
                let overlap = if alias && index_bits == data_bits {
                    format!("i=({index})d;")
                } else {
                    String::new()
                };
                let wide_load = if alias {
                    "__m512i wide=_mm512_loadu_si512(dp);"
                } else {
                    ""
                };
                let wide_store = if alias {
                    "_mm512_storeu_si512((void*)((char*)out+256),wide);"
                } else {
                    ""
                };
                for op in ["gather", "scatter", "scatterall"] {
                    let name = format!("{op}_{suffix}");
                    let operation = match op {
                        "gather" => format!(
                            "{data} result={prefix}_mmask_{}(d,k,i,base,{scale});",
                            shape.api
                        ),
                        "scatter" => format!("{prefix}_mask_{scatter}(base,k,i,d,{scale});"),
                        _ => format!("{prefix}_{scatter}(base,i,d,{scale});"),
                    };
                    let result = if op == "gather" { "result" } else { "d" };
                    let raw_opcode = if op == "gather" {
                        shape.gather
                    } else {
                        shape.scatter
                    };
                    source += &format!(
                        "/* {raw_opcode}, encoded width {width}. */\n__attribute__((target(\"avx512vl\"))) void {name}(void*base,void*ip,void*dp,void*out,__mmask8 k){{{index} i={iprefix}_loadu_si{index_bits}(ip);{data} d={dprefix}_loadu_{memory}(dp);{overlap}{wide_load}{operation}{dprefix}_storeu_{memory}(out,{result});{iprefix}_storeu_si{index_bits}((void*)((char*)out+64),i);{dprefix}_storeu_{memory}((void*)((char*)out+128),d);*(unsigned char*)((char*)out+192)=k;{wide_store}}}\n"
                    );
                }
            }
        }
    }
    source
}
#[test]
fn vl_indexed_wrappers_scalar_system_guards_mixed_widths_and_scales() {
    let source = source();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.avx2 && !features.avx512bw && !features.avx512dq);
    let missing = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&missing, &EMPTY, &pipeline).is_err()
    );
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("keel-vl-indexed-c-{}-{unique}", std::process::id()));
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
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
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
                    .success()
            );
        }
    }
}
#[test]
fn vl_indexed_c_scales_reject_invalid_or_runtime_values() {
    for scatter in [false, true] {
        for scale in ["1", "0", "3", "16", "-1", "n"] {
            let expression = if scatter {
                format!("_mm256_mask_i64scatter_ps(p,-1,idx,old,{scale});return old;")
            } else {
                format!("return _mm256_mmask_i64gather_ps(old,-1,idx,p,{scale});")
            };
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512vl\"))) __m128 f(__m128 old,__m256i idx,void*p,int n){{{expression}}}\n"
            );
            match parse(&source, Target::X86_64LinuxGnu) {
                Err(error) => assert_ne!(scale, "1", "{error}"),
                Ok(module) => assert_eq!(keel_ir::verify_module(&module).is_ok(), scale == "1"),
            }
        }
    }
}
