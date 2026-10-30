//! Independent scalar and system-header oracles shared by IR and C consumers.
use keel_codegen::x86_features::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut cases = String::new();
    let mut ordinal = 0;
    for width in [128, 256] {
        let pre = if width == 128 { "_mm" } else { "_mm256" };
        let q = if width == 128 { "__m128i" } else { "__m256i" };
        let bytes = width / 8;
        for op in [
            "pabsd",
            "pabsq",
            "pmuldq",
            "pmuludq",
            "pternlogd",
            "pternlogq",
            "ptestmd",
            "ptestmq",
            "ptestnmd",
            "ptestnmq",
        ] {
            let bits = if op.ends_with('d') { 32 } else { 64 };
            let mul = op == "pmuldq" || op == "pmuludq";
            let test = op.starts_with("ptest");
            let tern = op.starts_with("ptern");
            let shape = format!("i{bits}x{}", width / bits);
            let input = if mul {
                format!("i32x{}", width / 32)
            } else {
                shape.clone()
            };
            let controls = if tern {
                vec![0, 1, 2, 4, 8, 16, 32, 64, 128, 0x96, 0xca, 255]
            } else {
                vec![0]
            };
            for control in controls {
                for alias in if tern { 0..3 } else { 0..1 } {
                    let f = format!("vl_integer_{ordinal}");
                    ordinal += 1;
                    let a = "v7";
                    let b = if alias == 1 { "v7" } else { "v8" };
                    let c = if alias == 1 { "v7" } else { "v9" };
                    let merge = if alias == 2 { "v7" } else { "v6" };
                    let args = if test {
                        format!("{a},{b},v5")
                    } else if tern {
                        format!("{merge},v5,{a},{b},{c},v10")
                    } else if mul {
                        format!("v6,v5,{a},{b}")
                    } else {
                        format!("v6,v5,{a}")
                    };
                    ir += &format!(
                        "func @{f}(ptr,ptr,ptr,ptr,ptr,i8) target_features(\"avx512vl\") {{\nentry(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6=load.{shape} align 1 v0\n v7=load.{input} align 1 v1\n v8=load.{input} align 1 v2\n v9=load.{input} align 1 v3\n v10=iconst.i32 {control}\n v11=intrinsic x86_avx512vl_{op}{width}({args})\n store align 1 v11,v4\n"
                    );
                    for (n, v, offset) in [
                        (12, "v6", 64),
                        (14, "v7", 96),
                        (16, "v8", 128),
                        (18, "v9", 160),
                        (20, "v5", 192),
                    ] {
                        ir += &format!(
                            " v{n}=iconst.i64 {offset}\n v{}=ptradd v4,v{n}\n store align 1 {v},v{}\n",
                            n + 1,
                            n + 1
                        );
                    }
                    ir += " return\n}\n";
                    let b = if alias == 1 { "a" } else { "b" };
                    let c = if alias == 1 { "a" } else { "c" };
                    let merge = if alias == 2 { "a" } else { "m" };
                    let expr = if test {
                        format!(
                            "{pre}_mask_{}_epi{bits}_mask(k,a,{b})",
                            if op.starts_with("ptestnm") {
                                "testn"
                            } else {
                                "test"
                            }
                        )
                    } else if tern {
                        format!(
                            "{pre}_mask_mov_epi{bits}({merge},k,{pre}_maskz_ternarylogic_epi{bits}(k,a,{b},{c},{control}))"
                        )
                    } else if mul {
                        format!(
                            "{pre}_mask_{}(m,k,a,{b})",
                            if op == "pmuldq" {
                                "mul_epi32"
                            } else {
                                "mul_epu32"
                            }
                        )
                    } else {
                        format!("{pre}_mask_abs_epi{bits}(m,k,a)")
                    };
                    let outtype = if test { "unsigned char" } else { q };
                    source += &format!(
                        "__attribute__((target(\"avx512f,avx512vl\"))) void {f}(void*mp,void*ap,void*bp,void*cp,unsigned char*out,unsigned char k){{{q} m,a,b,c;__builtin_memcpy(&m,mp,{bytes});__builtin_memcpy(&a,ap,{bytes});__builtin_memcpy(&b,bp,{bytes});__builtin_memcpy(&c,cp,{bytes});{outtype} r={expr};__builtin_memcpy(out,&r,sizeof r);__builtin_memcpy(out+64,&m,{bytes});__builtin_memcpy(out+96,&a,{bytes});__builtin_memcpy(out+128,&b,{bytes});__builtin_memcpy(out+160,&c,{bytes});out[192]=k;}}\n"
                    );
                    oracle += &format!(
                        "extern void {f}(void*,void*,void*,void*,unsigned char*,unsigned char);\nstatic void reference_{f}(void*mp,void*ap,void*bp,void*cp,unsigned char*out,unsigned char k){{{q} m,a,b,c;memcpy(&m,mp,{bytes});memcpy(&a,ap,{bytes});memcpy(&b,bp,{bytes});memcpy(&c,cp,{bytes});{outtype} r={expr};memcpy(out,&r,sizeof r);memcpy(out+64,mp,{bytes});memcpy(out+96,ap,{bytes});memcpy(out+128,bp,{bytes});memcpy(out+160,cp,{bytes});out[192]=k;}}\n"
                    );
                    let lane_bytes = bits / 8;
                    let lanes = width / bits;
                    let b_array = if alias == 1 { "a" } else { "b" };
                    let c_array = if alias == 1 { "a" } else { "c" };
                    let scalar = if test {
                        format!(
                            "uint64_t value=(x&y){}0;expected[0]|=(unsigned char)((value && ((k>>lane)&1))<<lane);",
                            if op.starts_with("ptestnm") {
                                "=="
                            } else {
                                "!="
                            }
                        )
                    } else {
                        let val = if op.starts_with("pabs") {
                            format!("(x&(UINT64_C(1)<<{}))?((~x+1)&mask):x", bits - 1)
                        } else if mul {
                            format!(
                                "{}",
                                if op == "pmuldq" {
                                    "(uint64_t)((int64_t)(int32_t)load(a+lane*8,4)*(int64_t)(int32_t)load(b+lane*8,4))"
                                } else {
                                    "load(a+lane*8,4)*load(b+lane*8,4)"
                                }
                            )
                        } else {
                            format!("truth(x,y,z,{control},{bits})")
                        };
                        format!(
                            "uint64_t value=((k>>lane)&1)?({val}):load({merge}+lane*{lane_bytes},{lane_bytes});memcpy(expected+lane*{lane_bytes},&value,{lane_bytes});"
                        )
                    };
                    let result_bytes = if test { 1 } else { bytes };
                    cases += &format!(
                        "{{for(unsigned sample=0;sample<16;sample++){{unsigned char m[32],a[32],b[32],c[32];for(unsigned i=0;i<{bytes};i++){{m[i]=(unsigned char)(i*19+sample);a[i]=(unsigned char)(i*7+sample*17);b[i]=(unsigned char)(i*13+sample*11);c[i]=(unsigned char)(i*29+sample*3);}} if(sample<8){{uint64_t edge=edges[sample];memcpy(a,&edge,8);}} for(unsigned mi=0;mi<5;mi++){{unsigned char k=masks[mi],out[224],vendor[224],expected[224];memset(out,0xa5,224);memset(vendor,0xa5,224);memset(expected,0xa5,224);if({test})expected[0]=0;for(unsigned lane=0;lane<{lanes};lane++){{uint64_t x=load(a+lane*{lane_bytes},{lane_bytes}),y=load({b_array}+lane*{lane_bytes},{lane_bytes}),z=load({c_array}+lane*{lane_bytes},{lane_bytes}),mask=UINT64_MAX>>(64-{bits});(void)x;(void)y;(void)z;(void)mask;{scalar}}}memcpy(expected+64,m,{bytes});memcpy(expected+96,a,{bytes});memcpy(expected+128,b,{bytes});memcpy(expected+160,c,{bytes});expected[192]=k;unsigned csr=_mm_getcsr();{f}(m,a,b,c,out,k);if(_mm_getcsr()!=csr)return 1;reference_{f}(m,a,b,c,vendor,k);if(memcmp(out,expected,224)||memcmp(out,vendor,224)){{fprintf(stderr,\"{f} op={op} imm={control} alias={alias} sample=%u k=%x bytes={result_bytes}\\n\",sample,k);return 2;}}}}}}}}\n",
                        test = if test { 1 } else { 0 }
                    );
                }
            }
        }
    }
    oracle += &format!(
        "static uint64_t load(const unsigned char*p,unsigned n){{uint64_t r=0;memcpy(&r,p,n);return r;}}static uint64_t truth(uint64_t a,uint64_t b,uint64_t c,unsigned imm,unsigned bits){{uint64_t r=0;for(unsigned i=0;i<bits;i++)r|=(uint64_t)((imm>>((((a>>i)&1)*4)+(((b>>i)&1)*2)+((c>>i)&1)))&1)<<i;return r;}}int main(void){{uint64_t edges[]={{0,1,0x80000000u,0xffffffffu,UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0xaaaaaaaa55555555),UINT64_C(0x55555555aaaaaaaa)}};unsigned char masks[]={{0,1,0x80,0x85,255}};{cases}return 0;}}\n"
    );
    (ir, source, oracle)
}
pub fn check(module: &keel_ir::Module, stem: &str, oracle: Option<&str>) {
    let prepared;
    let oracle = if let Some(oracle) = oracle {
        oracle
    } else {
        prepared = fixtures().2;
        &prepared
    };
    let features = Features::parse("avx512vl").unwrap();
    assert!(!features.avx2 && !features.fma && !features.avx512dq);
    let directory = std::env::temp_dir().join(format!("keel-{stem}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
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
