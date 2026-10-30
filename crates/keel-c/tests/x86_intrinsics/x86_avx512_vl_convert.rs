//! Native narrow conversion bits, MXCSR, short tails and live operands.
use super::pclmul::parse;
use keel_codegen::x86_features::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const FORMS: &[(&str, &str, &str, u16)] = &[
    ("vcvtdq2ps", "i32x4", "f32x4", 128),
    ("vcvtdq2ps", "i32x8", "f32x8", 256),
    ("vcvtudq2ps", "i32x4", "f32x4", 128),
    ("vcvtudq2ps", "i32x8", "f32x8", 256),
    ("vcvtdq2pd", "i32x4", "f64x2", 128),
    ("vcvtdq2pd", "i32x4", "f64x4", 256),
    ("vcvtudq2pd", "i32x4", "f64x2", 128),
    ("vcvtudq2pd", "i32x4", "f64x4", 256),
    ("vcvtps2dq", "f32x4", "i32x4", 128),
    ("vcvtps2dq", "f32x8", "i32x8", 256),
    ("vcvtps2udq", "f32x4", "i32x4", 128),
    ("vcvtps2udq", "f32x8", "i32x8", 256),
    ("vcvttps2dq", "f32x4", "i32x4", 128),
    ("vcvttps2dq", "f32x8", "i32x8", 256),
    ("vcvttps2udq", "f32x4", "i32x4", 128),
    ("vcvttps2udq", "f32x8", "i32x8", 256),
    ("vcvtpd2dq", "f64x2", "i32x4", 128),
    ("vcvtpd2dq", "f64x4", "i32x4", 256),
    ("vcvtpd2udq", "f64x2", "i32x4", 128),
    ("vcvtpd2udq", "f64x4", "i32x4", 256),
    ("vcvttpd2dq", "f64x2", "i32x4", 128),
    ("vcvttpd2dq", "f64x4", "i32x4", 256),
    ("vcvttpd2udq", "f64x2", "i32x4", 128),
    ("vcvttpd2udq", "f64x4", "i32x4", 256),
    ("vcvtps2pd", "f32x4", "f64x2", 128),
    ("vcvtps2pd", "f32x4", "f64x4", 256),
    ("vcvtpd2ps", "f64x2", "f32x4", 128),
    ("vcvtpd2ps", "f64x4", "f32x4", 256),
    ("vcvtph2ps", "i16x8", "f32x4", 128),
    ("vcvtph2ps", "i16x8", "f32x8", 256),
    ("vcvtps2ph", "f32x4", "i16x8", 128),
    ("vcvtps2ph", "f32x8", "i16x8", 256),
];
fn ctype(shape: &str) -> (&'static str, usize) {
    match shape {
        "i32x4" | "i16x8" => ("__m128i", 16),
        "i32x8" => ("__m256i", 32),
        "f32x4" => ("__m128", 16),
        "f32x8" => ("__m256", 32),
        "f64x2" => ("__m128d", 16),
        "f64x4" => ("__m256d", 32),
        _ => unreachable!(),
    }
}
fn sources() -> (String, String) {
    let mut ir = String::from("#include <immintrin.h>\n");
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nstatic const uint32_t floats[]={0,0x80000000,1,0x007fffff,0x3fc00000,0xbfc00000,0x4f000000,0x4f800000,0x7f800000,0xff800000,0x7fc12345,0x7f800001,0x477ff000,0xc7000000,0x3f000001,0x00800000};\nstatic const uint64_t doubles[]={0,0x8000000000000000ULL,1,0x000fffffffffffffULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x41e0000000000000ULL,0x41f0000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0000000000001ULL,0x40effe0000000000ULL,0xc0e0000000000000ULL,0x3ff0000010000000ULL,0x0010000000000000ULL};\nstatic const uint16_t halves[]={0,0x8000,1,0x3ff,0x400,0x3c00,0xbc00,0x7bff,0x7c00,0xfc00,0x7e55,0x7c01,0x3555,0x8001,0x3c01,0x0401};\nstatic const uint32_t integers[]={0,1,0xffffffff,0x80000000,0x7fffffff,0x1000001,0x1000003,0xfffffeff};\n",
    );

    let mut checks = String::new();
    for &(opcode, input, output, width) in FORMS {
        let (itype, ibytes) = ctype(input);
        let (otype, obytes) = ctype(output);
        let controls: &[i32] = if opcode == "vcvtps2ph" {
            &[0, 1, 2, 3, 4, 5, 7, 8, 12, 255]
        } else {
            &[4]
        };
        for &control in controls {
            let name = format!("{opcode}_{width}_{control}");
            let prefix = if width == 128 { "_mm" } else { "_mm256" };
            let api = match opcode {
                "vcvtdq2ps" => "cvtepi32_ps",
                "vcvtudq2ps" => "cvtepu32_ps",
                "vcvtdq2pd" => "cvtepi32_pd",
                "vcvtudq2pd" => "cvtepu32_pd",
                "vcvtps2dq" => "cvtps_epi32",
                "vcvtps2udq" => "cvtps_epu32",
                "vcvttps2dq" => "cvttps_epi32",
                "vcvttps2udq" => "cvttps_epu32",
                "vcvtpd2dq" => "cvtpd_epi32",
                "vcvtpd2udq" => "cvtpd_epu32",
                "vcvttpd2dq" => "cvttpd_epi32",
                "vcvttpd2udq" => "cvttpd_epu32",
                "vcvtps2pd" => "cvtps_pd",
                "vcvtpd2ps" => "cvtpd_ps",
                "vcvtph2ps" => "cvtph_ps",
                "vcvtps2ph" => "cvtps_ph",
                _ => unreachable!(),
            };
            let imm = if opcode == "vcvtps2ph" {
                format!(",{control}")
            } else {
                String::new()
            };
            ir += &format!(
                "__attribute__((target(\"avx512f,avx512vl\"))) void {name}(const void*s,const void*a,void*o,unsigned char k){{{otype} merge=*(const {otype}*)s;{itype} input=*(const {itype}*)a;int ns=0,nk=0,na=0;{otype} result={prefix}_mask_{api}((ns++,merge),(nk++,k),(na++,input){imm});if(ns!=1||nk!=1||na!=1)return;*({otype}*)o=result;*({itype}*)((char*)o+64)=input;*({otype}*)((char*)o+128)=merge;*((unsigned char*)o+192)=k;}}\n"
            );
            ir += &format!(
                "__attribute__((target(\"avx512f,avx512vl\"))) void zero_{name}(const void*a,void*o,unsigned char k){{*({otype}*)o={prefix}_maskz_{api}(k,*(const {itype}*)a{imm});}}\n"
            );
            if api.contains("epu32") {
                ir += &format!(
                    "__attribute__((target(\"avx512f,avx512vl\"))) void unmasked_{name}(const void*a,void*o){{*({otype}*)o={prefix}_{api}(*(const {itype}*)a);}}\n"
                );
            }
            if opcode == "vcvtps2ph" {
                ir += &format!(
                    "__attribute__((target(\"avx512f,avx512vl\"))) void round_{name}(const void*s,const void*a,void*o,unsigned char k){{*({otype}*)o={prefix}_mask_cvt_roundps_ph(*(const {otype}*)s,k,*(const {itype}*)a,{control});}}\n__attribute__((target(\"avx512f,avx512vl\"))) void round_zero_{name}(const void*a,void*o,unsigned char k){{*({otype}*)o={prefix}_maskz_cvt_roundps_ph(k,*(const {itype}*)a,{control});}}\n"
                );
            }
            // Direct system assembly avoids vendor convert-then-select FP flag artifacts.
            let asm = if opcode == "vcvtps2ph" {
                format!("{opcode} ${control},%2,%0%{{%1%}}")
            } else {
                format!("{opcode} %2,%0%{{%1%}}")
            };
            c += &format!(
                "extern void {name}(void*,void*,void*,unsigned char);\n__attribute__((noinline)) static void ref_{name}(void*o,const void*s,const void*a,unsigned char k){{{itype} x;{otype} result;memcpy(&x,a,{ibytes});memcpy(&result,s,{obytes});__asm__ volatile(\"{asm}\":\"+v\"(result):\"Yk\"(k),\"v\"(x));memcpy(o,&result,{obytes});}}\n"
            );
            c += &format!(
                "__attribute__((noinline)) static void std_{name}(void*o,const void*s,const void*a,unsigned char k){{{itype} x;{otype} merge;memcpy(&x,a,{ibytes});memcpy(&merge,s,{obytes});{otype} result={prefix}_mask_{api}(merge,k,x{imm});memcpy(o,&result,{obytes});}}\n"
            );
            let fill = if input.starts_with("f32") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/4;i++)memcpy(a+4*i,&floats[(i+(sample==2?9:sample*5))%16],4);"
                )
            } else if input.starts_with("f64") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/8;i++)memcpy(a+8*i,&doubles[(i+(sample==2?9:sample*5))%16],8);"
                )
            } else if input.starts_with("i16") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/2;i++)memcpy(a+2*i,&halves[(i+(sample==2?9:sample*5))%16],2);"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{ibytes}/4;i++)memcpy(a+4*i,&integers[(i+sample*3)%8],4);"
                )
            };
            checks += &format!(
                "{{unsigned char a[{ibytes}],s[{obytes}],out[208],expected[{obytes}],standard[{obytes}];for(unsigned i=0;i<{obytes};i++)s[i]=(unsigned char)(i*73+0xa5);for(unsigned sample=0;sample<4;sample++){{{fill}for(unsigned round=0;round<4;round++)for(unsigned pattern=0;pattern<4;pattern++){{unsigned char k=pattern==0?0:pattern==1?255:pattern==2?0xaa:0x80;unsigned csr=0x1f80|(round<<13)|(sample&1?0x8040:0)|(sample==3?0x21:0);memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(s,a,out,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(csr);ref_{name}(expected,s,a,k);unsigned ref_status=_mm_getcsr()&63;_mm_setcsr(csr);std_{name}(standard,s,a,k);if(memcmp(standard,expected,{obytes}))return 13;if(memcmp(out,expected,{obytes})||status!=ref_status){{fprintf(stderr,\"{name} sample%u round%u mask%u status%x ref%x\\n\",sample,round,pattern,status,ref_status);return 10;}}if(memcmp(out+64,a,{ibytes})||memcmp(out+128,s,{obytes})||out[192]!=k)return 11;for(unsigned i={obytes};i<64;i++)if(out[i]!=0xa5)return 12;}}}}}}\n"
            );
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn vl_conversion_c_wrappers_match_native_flags_and_zero_tails() {
    let (source, harness) = sources();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f,avx512vl").unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-vl-convert-{}", std::process::id()));
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
            if !std::is_x86_feature_detected!("avx512vl") {
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-mavx512f", "-mavx512vl", "-frounding-math"])
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
