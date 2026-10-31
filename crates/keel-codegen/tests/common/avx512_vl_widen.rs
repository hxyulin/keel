//! Scalar extension and independent system headers agree on low-element inputs.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const XMM12: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
pub fn fixtures() -> (String, String, String) {
    let mut ir = String::new();
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    let mut n = 0;
    for width in [128, 256] {
        let (pre, vec) = if width == 128 {
            ("_mm", "__m128i")
        } else {
            ("_mm256", "__m256i")
        };
        let bytes = width / 8;
        for signed in [true, false] {
            for (shape, inbits, outbits) in [
                ("bd", 8, 32),
                ("bq", 8, 64),
                ("wd", 16, 32),
                ("wq", 16, 64),
                ("dq", 32, 64),
            ] {
                let mode = if signed { "sx" } else { "zx" };
                let dest = format!("i{outbits}x{}", width / outbits);
                let input = format!("i{inbits}x{}", 128 / inbits);
                let api = format!(
                    "cvtep{}{inbits}_epi{outbits}",
                    if signed { "i" } else { "u" }
                );
                let lanes = width / outbits;
                let used = lanes * inbits / 8;
                let size = inbits / 8;
                let outsize = outbits / 8;
                for zero in [false, true] {
                    for alias in 0..if width == 128 && !zero { 2 } else { 1 } {
                        let name = format!("vl_widen_{n}");
                        n += 1;
                        let merge = if zero {
                            "v7"
                        } else if alias == 1 {
                            "v8"
                        } else {
                            "v4"
                        };
                        ir += &format!(
                            "func @{name}(ptr,ptr,ptr,i8) target_features(\"avx512vl\") {{\nentry(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.{dest} align 1 v0\n v5=load.{input} align 1 v1\n v6=iconst.i{outbits} 0\n v7=vsplat.{dest} v6\n"
                        );
                        if alias == 1 {
                            ir += &format!(" v8=bitcast.{dest} v5\n");
                        }
                        ir += &format!(
                            " v9=intrinsic x86_avx512vl_pmov{mode}{shape}{width}({merge},v3,v5)\n store align 1 v9,v2\n v10=iconst.i64 64\n v11=ptradd v2,v10\n store align 1 v4,v11\n v12=iconst.i64 96\n v13=ptradd v2,v12\n store align 1 v5,v13\n v14=iconst.i64 128\n v15=ptradd v2,v14\n store align 1 v3,v15\n return\n}}\n"
                        );
                        let merge = if alias == 1 { "a" } else { "m" };
                        let expr = if zero {
                            format!("{pre}_maskz_{api}(k,a)")
                        } else {
                            format!("{pre}_mask_{api}({merge},k,a)")
                        };
                        source += &format!(
                            "__attribute__((target(\"avx512vl\"))) void {name}(void*mp,void*ap,unsigned char*out,unsigned char k){{{vec} m;__m128i a;__builtin_memcpy(&m,mp,{bytes});__builtin_memcpy(&a,ap,16);{vec} r={expr};__builtin_memcpy(out,&r,{bytes});__builtin_memcpy(out+64,&m,{bytes});__builtin_memcpy(out+96,&a,16);out[128]=k;}}\n"
                        );
                        oracle += &format!(
                            "extern void {name}(void*,void*,unsigned char*,unsigned char);static void vendor_{name}(void*mp,void*ap,unsigned char*out,unsigned char k){{{vec} m;__m128i a;memcpy(&m,mp,{bytes});memcpy(&a,ap,16);{vec} r={expr};memcpy(out,&r,{bytes});memcpy(out+64,&m,{bytes});memcpy(out+96,&a,16);out[128]=k;}}\n"
                        );
                        let inactive = if zero {
                            "0".to_owned()
                        } else {
                            format!(
                                "load({}+lane*{outsize},{outsize})",
                                if alias == 1 { "a" } else { "m" }
                            )
                        };
                        cases += &format!(
                            "{{unsigned char m[32],a[16],out[160],vendor[160],expected[160];for(unsigned sample=0;sample<16;sample++){{for(unsigned i=0;i<{bytes};i++)m[i]=(unsigned char)(i*19+sample);for(unsigned variant=0;variant<2;variant++){{memset(a,variant?0x80:0x7f,16);for(unsigned lane=0;lane<{lanes};lane++){{uint64_t value=edges[(sample+lane*3)%16];memcpy(a+lane*{size},&value,{size});}}for(unsigned i={used};i<16;i++)a[i]^=(unsigned char)(sample+i*7);for(unsigned mi=0;mi<6;mi++){{unsigned char k=masks[mi];memset(out,0xa5,160);memset(vendor,0xa5,160);memset(expected,0xa5,160);for(unsigned lane=0;lane<{lanes};lane++){{uint64_t x=load(a+lane*{size},{size});if({signed}&&(x&(UINT64_C(1)<<({inbits}-1))))x|=UINT64_MAX<<{inbits};uint64_t value=((k>>lane)&1)?x:({inactive});memcpy(expected+lane*{outsize},&value,{outsize});}}memcpy(expected+64,m,{bytes});memcpy(expected+96,a,16);expected[128]=k;unsigned csr=_mm_getcsr();{name}(m,a,out,k);if(_mm_getcsr()!=csr)return 1;vendor_{name}(m,a,vendor,k);if(memcmp(out,expected,160)||memcmp(out,vendor,160)){{fprintf(stderr,\"{name} width={width} op={mode}{shape} zero={zero} alias={alias} sample=%u high=%u k=%x\\n\",sample,variant,k);return 2;}}}}}}}}}}\n",
                            signed = if signed { 1 } else { 0 },
                            zero = u8::from(zero)
                        );
                    }
                }
            }
        }
    }
    oracle += &format!(
        "static uint64_t load(const unsigned char*p,unsigned n){{uint64_t r=0;memcpy(&r,p,n);return r;}}int main(void){{uint64_t edges[16]={{0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,0x55,0xaa,0x80000001,0x7ffffffe,17}};unsigned char masks[6]={{0,1,0x80,0xfc,0x85,255}};{cases}return 0;}}\n"
    );
    (ir, source, oracle)
}
pub fn check(module: &keel_ir::Module, stem: &str) {
    let features = Features::parse("avx512vl").unwrap();
    assert!(!features.avx2 && !features.fma && !features.avx512bw && !features.avx512dq);
    let directory = std::env::temp_dir().join(format!("keel-{stem}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), fixtures().2).unwrap();
    #[cfg(target_arch = "x86_64")]
    let native = cfg!(target_os = "linux")
        && std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(target_arch = "x86_64"))]
    let native = false;
    if native {
        let output = Command::new("cc")
            .args([
                "-O2",
                "-mavx512f",
                "-mavx512vl",
                "-fno-tree-vectorize",
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
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &XMM12] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(directory.join("consumer.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("consumer.o"))
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
                    "{stem} {allocator:?}"
                );
            }
        }
    }
}
