//! Narrow packed EVEX FMA preserves source order, fusion and independent merge bits.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
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
fn sources() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <math.h>\nstatic const uint32_t words[]={0x3f800001,0x3f7ffffe,0xbf800000,0x00800000,1,0x7f7fffff,0x80000000,0,0x7f800000,0xff800000,0x7fc12345,0x7f800001,0x3f000000,0xbf000000,0x40000000,0xc0000000};\nstatic const uint64_t qwords[]={0x3ff0000000000001ULL,0x3feffffffffffffeULL,0xbff0000000000000ULL,0x0010000000000000ULL,1,0x7fefffffffffffffULL,0x8000000000000000ULL,0,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0000000000001ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x4000000000000000ULL,0xc000000000000000ULL};\n",
    );
    let mut checks = String::new();
    // This public operation matrix is independent of descriptor metadata.
    for width in [128, 256] {
        for family in [
            "vfmadd",
            "vfmsub",
            "vfnmadd",
            "vfnmsub",
            "vfmaddsub",
            "vfmsubadd",
        ] {
            for order in [132, 213, 231] {
                for suffix in ["ps", "pd"] {
                    let bits = if suffix == "ps" { 32 } else { 64 };
                    let lanes = width / bits;
                    let bytes = width / 8;
                    let shape = format!("f{bits}x{lanes}");
                    let (vtype, ctype, float, values, fused) = if bits == 32 {
                        (format!("__m{width}"), "uint32_t", "float", "words", "fmaf")
                    } else {
                        (
                            format!("__m{width}d"),
                            "uint64_t",
                            "double",
                            "qwords",
                            "fma",
                        )
                    };
                    let prefix = if width == 128 { "_mm" } else { "_mm256" };
                    let opcode = format!("{family}{order}{suffix}");
                    let name = format!("{opcode}{width}");
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6=load.{shape} align 1 v0\n v7=load.{shape} align 1 v1\n v8=load.{shape} align 1 v2\n v9=load.{shape} align 1 v3\n v10=load.i64x8 align 1 v0\n v11=intrinsic x86_avx512vl_{opcode}{width}(v6,v5,v7,v8,v9)\n store align 1 v11,v4\n v12=iconst.i64 64\n v13=ptradd v4,v12\n store align 1 v7,v13\n v14=ptradd v13,v12\n store align 1 v8,v14\n v15=ptradd v14,v12\n store align 1 v9,v15\n v16=ptradd v15,v12\n store align 1 v6,v16\n v17=ptradd v16,v12\n store align 1 v5,v17\n v18=ptradd v17,v12\n store align 1 v10,v18\n return\n}}\n"
                    );
                    c += &format!(
                        "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\n__attribute__((noinline)) static void ref_{name}(void*out,void*s,void*a,void*b,void*c,uint8_t k){{{vtype} x,y,z,m;memcpy(&x,a,{bytes});memcpy(&y,b,{bytes});memcpy(&z,c,{bytes});memcpy(&m,s,{bytes});__asm__ volatile(\"{opcode} %[c],%[b],%[a] %{{%[k]%}}\":[a]\"+&v\"(x):[b]\"v\"(y),[c]\"v\"(z),[k]\"Yk\"(k):\"memory\");x={prefix}_mask_mov_{suffix}(m,k,x);memcpy(out,&x,{bytes});}}\n"
                    );
                    let (x, y, z) = match order {
                        132 => ("fa", "fc", "fb"),
                        213 => ("fb", "fa", "fc"),
                        _ => ("fb", "fc", "fa"),
                    };
                    let neg_product = family == "vfnmadd" || family == "vfnmsub";
                    let neg_add = match family {
                        "vfmsub" | "vfnmsub" => "1",
                        "vfmaddsub" => "!(lane&1)",
                        "vfmsubadd" => "lane&1",
                        _ => "0",
                    };
                    checks += &format!(
                        "{{{ctype} a[64/sizeof({ctype})],b[64/sizeof({ctype})],d[64/sizeof({ctype})],s[64/sizeof({ctype})];unsigned char out[448],want[64];uint8_t masks[]={{0,1,0x55,0xaa,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<7;sample++){{for(unsigned lane=0;lane<64/sizeof({ctype});lane++){{a[lane]={values}[(lane+sample)%16];b[lane]={values}[(lane+sample+5)%16];d[lane]={values}[(lane+sample+11)%16];s[lane]=sample==1?a[lane]:sample==2?b[lane]:sample==3?d[lane]:({ctype})UINT64_C(0x80000000fff81234);if(sample==5){{a[lane]={values}[10];b[lane]={values}[10]+1;d[lane]={values}[10]+2;}}if(sample==6){{a[lane]={values}[0];b[lane]={values}[1];d[lane]={values}[2];}}}}for(unsigned rounding=0;rounding<4;rounding++)for(unsigned denormal=0;denormal<4;denormal++)for(unsigned mi=0;mi<6;mi++){{unsigned csr=0x1f80|(rounding<<13)|((denormal&1)<<6)|((denormal>>1)<<15);memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(s,a,b,d,out,masks[mi]);unsigned status=_mm_getcsr();_mm_setcsr(csr);ref_{name}(want,s,a,b,d,masks[mi]);unsigned expected=_mm_getcsr();if(memcmp(out,want,{bytes})||status!=expected){{fprintf(stderr,\"{name} sample=%u round=%u denormal=%u mask=%x status=%x expected=%x\\n\",sample,rounding,denormal,masks[mi],status,expected);return 1;}}for(unsigned j={bytes};j<64;j++)if(out[j]!=0xa5)return 2;if(memcmp(out+64,a,{bytes})||memcmp(out+128,b,{bytes})||memcmp(out+192,d,{bytes})||memcmp(out+256,s,{bytes})||out[320]!=masks[mi]||memcmp(out+384,s,64))return 3;if(sample==6&&!rounding&&!denormal){{for(unsigned lane=0;lane<{lanes};lane++){{if(!(masks[mi]&(1u<<lane)))continue;{float} fa,fb,fc;memcpy(&fa,a+lane,sizeof fa);memcpy(&fb,b+lane,sizeof fb);memcpy(&fc,d+lane,sizeof fc);{float} product_a={x},product_b={y},addend={z};if({neg_product})product_a=-product_a;if({neg_add})addend=-addend;{float} result={fused}(product_a,product_b,addend);{ctype} scalar;memcpy(&scalar,&result,sizeof scalar);if(memcmp(out+lane*sizeof(scalar),&scalar,sizeof scalar))return 4;}}}}}}_mm_setcsr(0x1f00);{name}(s,a,b,d,out,0);if(_mm_getcsr()!=0x1f00||memcmp(out,s,{bytes}))return 5;}}}}\n",
                        neg_product = u8::from(neg_product)
                    );
                }
            }
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn vl_fma_matches_encoded_order_fused_oracle_masks_and_mxcsr() {
    let (source, harness) = sources();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.fma && !features.avx2 && !features.avx512dq);
    let directory = std::env::temp_dir().join(format!("keel-vl-fma-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), harness).unwrap();
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
            .args(["-O1", "-mavx512f", "-mavx512vl", "-frounding-math", "-c"])
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
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE] {
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
            fs::write(directory.join("fma.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("fma.o"))
                    .args(["-lm", "-o"])
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
                    "{allocation:?}"
                );
            }
        }
    }
}
#[test]
fn vl_fma_has_raw_effects_and_requires_vl_per_function() {
    let (source, _) = sources();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_vl_fma().is_some())
    {
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(!id.info().pure && inst.is_pinned() && inst.may_trap());
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
        assert_eq!(id.info().params.len(), 5);
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
        Features::NONE.with_features("avx2,fma").unwrap(),
    ] {
        assert!(compile(&source, features).is_err());
    }
    let local = source.replace("i8) {", "i8) target_features(\"avx512vl\") {");
    compile(&local, Features::NONE).unwrap();
    let disabled = source.replace("i8) {", "i8) target_features(\"-avx512vl\") {");
    assert!(compile(&disabled, Features::NONE.with_features("avx512vl").unwrap()).is_err());
}
