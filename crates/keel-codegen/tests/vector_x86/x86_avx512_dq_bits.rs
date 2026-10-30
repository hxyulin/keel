//! DQ operations preserve bit payloads and wrap integer products.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const FORMS: &[(&str, &str, u16)] = &[
    ("mulloq", "i64x2", 128),
    ("andps", "f32x4", 128),
    ("andpd", "f64x2", 128),
    ("andnps", "f32x4", 128),
    ("andnpd", "f64x2", 128),
    ("orps", "f32x4", 128),
    ("orpd", "f64x2", 128),
    ("xorps", "f32x4", 128),
    ("xorpd", "f64x2", 128),
    ("mulloq", "i64x4", 256),
    ("andps", "f32x8", 256),
    ("andpd", "f64x4", 256),
    ("andnps", "f32x8", 256),
    ("andnpd", "f64x4", 256),
    ("orps", "f32x8", 256),
    ("orpd", "f64x4", 256),
    ("xorps", "f32x8", 256),
    ("xorpd", "f64x4", 256),
    ("mulloq", "i64x8", 512),
    ("andps", "f32x16", 512),
    ("andpd", "f64x8", 512),
    ("andnps", "f32x16", 512),
    ("andnpd", "f64x8", 512),
    ("orps", "f32x16", 512),
    ("orpd", "f64x8", 512),
    ("xorps", "f32x16", 512),
    ("xorpd", "f64x8", 512),
];
fn fixtures() -> (String, String) {
    let mut source = String::new();
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for &(op, shape, width) in FORMS {
        let bytes = width / 8;
        let lanes = if shape.starts_with("f32") {
            width / 32
        } else {
            width / 64
        };
        let uint = if lanes == width / 32 {
            "uint32_t"
        } else {
            "uint64_t"
        };
        let mask = if lanes == 16 { "i16" } else { "i8" };
        let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
        let scalar = match op {
            "mulloq" => "a[i]*b[i]",
            "andps" | "andpd" => "a[i]&b[i]",
            "andnps" | "andnpd" => "~a[i]&b[i]",
            "orps" | "orpd" => "a[i]|b[i]",
            "xorps" | "xorpd" => "a[i]^b[i]",
            _ => unreachable!(),
        };
        for alias in [false, true] {
            let name = format!("{op}_{width}_{}", u8::from(alias));
            let merge = if alias { "v4" } else { "v6" };
            source += &format!(
                "func @{name}(ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v7:{mask}):\n v4 = load.{shape} align 1 v0\n v5 = load.{shape} align 1 v1\n v6 = load.{shape} align 1 v2\n v8 = intrinsic x86_avx512dq_{op}{width}({merge},v7,v4,v5)\n store align 1 v8,v3\n v9 = iconst.i64 64\n v10 = ptradd v3,v9\n store align 1 v4,v10\n v11 = ptradd v10,v9\n store align 1 v5,v11\n v12 = ptradd v11,v9\n store align 1 v6,v12\n v13 = ptradd v12,v9\n store align 1 v7,v13\n return\n}}\n"
            );
            harness += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,{cmask});\n"
            );
            let merge = if alias { "a[i]" } else { "s[i]" };
            checks += &format!(
                "{{{uint} a[{lanes}],b[{lanes}],s[{lanes}];unsigned char out[288];for(unsigned i=0;i<{lanes};i++){{a[i]=({uint})edges[i%16];b[i]=({uint})edges[(i*5+3)%16];s[i]=({uint})edges[(i+7)%16];}}{cmask} masks[]={{0,1,({cmask})~0u,0x55,({cmask})(1u<<({lanes}-1)),({cmask})~((1u<<{lanes})-1)}};for(unsigned mi=0;mi<6;mi++){{{cmask} k=masks[mi];unsigned csr=(saved|63)&~0x80u;memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(a,b,s,out,k);if(_mm_getcsr()!=csr)return 1;for(unsigned i=0;i<{lanes};i++){{{uint} expected=(k&(1u<<i))?({uint})({scalar}):{merge},got;memcpy(&got,out+i*sizeof(got),sizeof got);if(got!=expected){{fprintf(stderr,\"{name} mask%u lane%u\\n\",mi,i);return 2;}}}}if(memcmp(out+64,a,{bytes})||memcmp(out+128,b,{bytes})||memcmp(out+192,s,{bytes})||memcmp(out+256,&k,sizeof k))return 3;for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 4;_mm_setcsr(saved);}}}}\n"
            );
        }
    }
    harness += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t edges[16]={{0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff),UINT64_C(0xffffffff80000000),UINT64_C(0x7ff000007f800001),UINT64_C(0xfff00000ff800001),UINT64_C(0x7ff0000000000001),UINT64_C(0xfff0000000000001),UINT64_C(0x7ff812347fc12345),UINT64_C(0x8000000080000000),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x3ff000003f800000),UINT64_C(0x0000000100000001),UINT64_C(0xffffffff7fffffff)}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
#[test]
fn dq_bitwise_multiply_masks_aliases_and_spills() {
    let (source, harness) = fixtures();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE
        .with_features("avx512f,avx512dq,avx512vl")
        .unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-dq-bits-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for (tier, alloc, pools, opt) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
    ] {
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &p).unwrap();
        for opcode in [
            "vpmullq", "vandps", "vandpd", "vandnps", "vandnpd", "vorps", "vorpd", "vxorps",
            "vxorpd",
        ] {
            assert!(text.contains(opcode), "missing DQ opcode {opcode}");
        }
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("convert.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512dq") {
                continue;
            }
            let output = Command::new("cc")
                .args([
                    "-O1",
                    "-mavx512f",
                    "-mavx512vl",
                    "-mavx512dq",
                    "-frounding-math",
                ])
                .arg(dir.join("harness.c"))
                .arg(dir.join("convert.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "conversion oracle: {status}");
        }
    }
}
#[test]
fn dq_bitwise_contracts_and_feature_gates() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_dq_opcode().is_some())
        .collect();
    assert_eq!(ids.len(), 27);
    for id in ids {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure);
        assert!(!data.is_pinned() && !data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::None);
    }
    let (source, _) = fixtures();
    let module = keel_ir::parse(&source).unwrap();
    for flags in ["avx512f,avx512vl", "avx512f,avx512dq"] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features(flags).unwrap(),
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &p).is_err());
    }
    let wide: String = source
        .split("}\n")
        .filter(|body| body.contains("_512_"))
        .map(|body| format!("{body}}}\n"))
        .collect();
    let wide = keel_ir::parse(&wide).unwrap();
    let features = Features::NONE.with_features("avx512f,avx512dq").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&wide, &EMPTY, &pipeline).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
}

#[test]
fn dq_generic_qword_mul_selects_native_and_preserves_fallback() {
    let source = "func @generic_mul(ptr,ptr,ptr) {\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x8 align 1 v0\n v4 = load.i64x8 align 1 v1\n v5 = vimul v3,v4\n store align 1 v5,v2\n return\n}\n";
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let directory =
        std::env::temp_dir().join(format!("keel-generic-dq-mul-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"),"#include <stdint.h>\nextern void generic_mul(const void*,const void*,void*);int main(void){uint64_t a[8]={0,1,~0ULL,0x8000000000000000ULL,0x7fffffffffffffffULL,0xffffffff80000000ULL,0x55555555aaaaaaaaULL,0x100000001ULL},b[8],out[8];for(int i=0;i<8;i++)b[i]=a[(i+3)%8];generic_mul(a,b,out);for(int i=0;i<8;i++)if(out[i]!=a[i]*b[i])return 1;generic_mul(a,b,a);for(int i=0;i<8;i++)if(out[i]!=a[i])return 2;return 0;}\n").unwrap();
    for flags in ["avx512f", "avx512f,avx512dq"] {
        let features = Features::NONE.with_features(flags).unwrap();
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            assert_eq!(text.contains("vpmullq"), features.avx512dq);
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            fs::write(directory.join("mul.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512dq") {
                    continue;
                }
                let output = Command::new("cc")
                    .arg(directory.join("harness.c"))
                    .arg(directory.join("mul.o"))
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
}
