//! Native narrow conversion bits, MXCSR, short tails and live operands.
use super::pclmul::parse;
use keel_codegen::x86_features::{Features, module_features};
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
    ("vcvtqq2pd", "i64x2", "f64x2", 128),
    ("vcvtuqq2pd", "i64x2", "f64x2", 128),
    ("vcvtqq2ps", "i64x2", "f32x4", 128),
    ("vcvtuqq2ps", "i64x2", "f32x4", 128),
    ("vcvtpd2qq", "f64x2", "i64x2", 128),
    ("vcvtpd2uqq", "f64x2", "i64x2", 128),
    ("vcvtps2qq", "f32x4", "i64x2", 128),
    ("vcvtps2uqq", "f32x4", "i64x2", 128),
    ("vcvttpd2qq", "f64x2", "i64x2", 128),
    ("vcvttpd2uqq", "f64x2", "i64x2", 128),
    ("vcvttps2qq", "f32x4", "i64x2", 128),
    ("vcvttps2uqq", "f32x4", "i64x2", 128),
    ("vcvtqq2pd", "i64x4", "f64x4", 256),
    ("vcvtuqq2pd", "i64x4", "f64x4", 256),
    ("vcvtqq2ps", "i64x4", "f32x4", 256),
    ("vcvtuqq2ps", "i64x4", "f32x4", 256),
    ("vcvtpd2qq", "f64x4", "i64x4", 256),
    ("vcvtpd2uqq", "f64x4", "i64x4", 256),
    ("vcvtps2qq", "f32x4", "i64x4", 256),
    ("vcvtps2uqq", "f32x4", "i64x4", 256),
    ("vcvttpd2qq", "f64x4", "i64x4", 256),
    ("vcvttpd2uqq", "f64x4", "i64x4", 256),
    ("vcvttps2qq", "f32x4", "i64x4", 256),
    ("vcvttps2uqq", "f32x4", "i64x4", 256),
    ("vcvtqq2pd", "i64x8", "f64x8", 512),
    ("vcvtuqq2pd", "i64x8", "f64x8", 512),
    ("vcvtqq2ps", "i64x8", "f32x8", 512),
    ("vcvtuqq2ps", "i64x8", "f32x8", 512),
    ("vcvtpd2qq", "f64x8", "i64x8", 512),
    ("vcvtpd2uqq", "f64x8", "i64x8", 512),
    ("vcvtps2qq", "f32x8", "i64x8", 512),
    ("vcvtps2uqq", "f32x8", "i64x8", 512),
    ("vcvttpd2qq", "f64x8", "i64x8", 512),
    ("vcvttpd2uqq", "f64x8", "i64x8", 512),
    ("vcvttps2qq", "f32x8", "i64x8", 512),
    ("vcvttps2uqq", "f32x8", "i64x8", 512),
];
fn ctype(shape: &str) -> (&'static str, usize) {
    match shape {
        "i64x2" => ("__m128i", 16),
        "i64x4" => ("__m256i", 32),
        "i64x8" => ("__m512i", 64),
        "f32x4" => ("__m128", 16),
        "f32x8" => ("__m256", 32),
        "f64x2" => ("__m128d", 16),
        "f64x4" => ("__m256d", 32),
        "f64x8" => ("__m512d", 64),
        _ => unreachable!(),
    }
}
fn sources() -> (String, String) {
    let mut ir = String::from("#include <immintrin.h>\n");
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\nstatic const uint32_t floats[]={0,0x80000000,0x7f800001,0x7fc12345,0x3fc00000,0xbfc00000,0x5f000000,0x5f800000,0x7f800000,0xff800000,0xdf000000,1,0x5effffff,0x5f7fffff,0xbf000000,0x80000001};\nstatic const uint64_t doubles[]={0,0x8000000000000000ULL,0x7ff0000000000001ULL,0x7ff8123456789abcULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x43e0000000000000ULL,0x43f0000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0xc3e0000000000000ULL,1,0x43dfffffffffffffULL,0x43efffffffffffffULL,0xbfe0000000000000ULL,0x8000000000000001ULL};\nstatic const uint64_t integers[]={0,1,0x7fffffffffffffffULL,0x8000000000000000ULL,0xffffffffffffffffULL,0x1000001,0x1000003,0x1fffffffffffffULL,0x20000000000001ULL,0x20000000000003ULL,0x8000000000000001ULL,0xfffffffffeffffffULL,0xfffffffffffffffdULL,0x55555555aaaaaaaaULL,0x100000001ULL,0x7ffffffffffffffeULL};\n",
    );
    let mut checks = String::new();
    for &(opcode, input, output, width) in FORMS {
        let (itype, ibytes) = ctype(input);
        let (otype, obytes) = ctype(output);
        let controls: &[i32] = if width != 512 {
            &[4]
        } else if opcode.starts_with("vcvtt") {
            &[4, 8]
        } else {
            &[4, 8, 9, 10, 11]
        };
        for &control in controls {
            let name = format!("{opcode}_{width}_{control}");
            let prefix = match width {
                128 => "_mm",
                256 => "_mm256",
                _ => "_mm512",
            };
            let api = match opcode {
                "vcvtqq2pd" => "cvtepi64_pd",
                "vcvtuqq2pd" => "cvtepu64_pd",
                "vcvtqq2ps" => "cvtepi64_ps",
                "vcvtuqq2ps" => "cvtepu64_ps",
                "vcvtpd2qq" => "cvtpd_epi64",
                "vcvtpd2uqq" => "cvtpd_epu64",
                "vcvtps2qq" => "cvtps_epi64",
                "vcvtps2uqq" => "cvtps_epu64",
                "vcvttpd2qq" => "cvttpd_epi64",
                "vcvttpd2uqq" => "cvttpd_epu64",
                "vcvttps2qq" => "cvttps_epi64",
                "vcvttps2uqq" => "cvttps_epu64",
                _ => unreachable!(),
            };
            let rounded = if api.starts_with("cvtt") {
                api.replacen("cvtt", "cvtt_round", 1)
            } else {
                api.replacen("cvt", "cvt_round", 1)
            };
            let selected = if width == 512 && control != 4 {
                rounded.as_str()
            } else {
                api
            };
            let imm = if width == 512 && control != 4 {
                format!(",{control}")
            } else {
                String::new()
            };
            ir += &format!(
                "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void {name}(const void*s,const void*a,void*o,unsigned char k){{{otype} merge=*(const {otype}*)s;{itype} input=*(const {itype}*)a;int ns=0,nk=0,na=0;{otype} result;if(k==255)result={prefix}_{selected}((na++,input){imm});else result={prefix}_mask_{selected}((ns++,merge),(nk++,k),(na++,input){imm});if(na!=1||ns!=(k==255?0:1)||nk!=(k==255?0:1))return;*({otype}*)o=result;*({itype}*)((char*)o+64)=input;*({otype}*)((char*)o+128)=merge;*((unsigned char*)o+192)=k;}}\n"
            );
            ir += &format!(
                "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void zero_{name}(const void*a,void*o,unsigned char k){{*({otype}*)o={prefix}_maskz_{selected}(k,*(const {itype}*)a{imm});}}\n"
            );
            if width == 512 && control == 4 {
                ir += &format!(
                    "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void explicit_{name}(const void*s,const void*a,void*o,unsigned char k){{{itype} x=*(const {itype}*)a;{otype} r={prefix}_mask_{rounded}(*(const {otype}*)s,k,x,4);*({otype}*)o=r;}}\n__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void explicit_zero_{name}(const void*a,void*o,unsigned char k){{*({otype}*)o={prefix}_maskz_{rounded}(k,*(const {itype}*)a,4);}}\n__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) void explicit_all_{name}(const void*a,void*o){{*({otype}*)o={prefix}_{rounded}(*(const {itype}*)a,4);}}\n"
                );
            }
            // Direct system assembly avoids vendor convert-then-select FP flag artifacts.
            let rounding = if width == 512 && control != 4 {
                if opcode.starts_with("vcvtt") {
                    Some("sae")
                } else {
                    Some(match control & 3 {
                        0 => "rn-sae",
                        1 => "rd-sae",
                        2 => "ru-sae",
                        _ => "rz-sae",
                    })
                }
            } else {
                None
            };
            let decorator = rounding.map_or(String::new(), |name| format!("%{{{name}%}},"));
            let asm = format!("{opcode} {decorator}%2,%0%{{%1%}}");
            c += &format!(
                "extern void {name}(void*,void*,void*,unsigned char);\n__attribute__((noinline)) static void ref_{name}(void*o,const void*s,const void*a,unsigned char k){{{itype} x;{otype} result;memcpy(&x,a,{ibytes});memcpy(&result,s,{obytes});__asm__ volatile(\"{asm}\":\"+v\"(result):\"Yk\"(k),\"v\"(x));memcpy(o,&result,{obytes});}}\n"
            );
            c += &format!(
                "__attribute__((noinline)) static void std_{name}(void*o,const void*s,const void*a,unsigned char k){{{itype} x;{otype} merge;memcpy(&x,a,{ibytes});memcpy(&merge,s,{obytes});{otype} result={prefix}_mask_{selected}(merge,k,x{imm});memcpy(o,&result,{obytes});}}\n"
            );
            let fill = if input.starts_with("f32") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/4;i++)memcpy(a+4*i,&floats[(i+2*sample)%16],4);"
                )
            } else if input.starts_with("f64") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/8;i++)memcpy(a+8*i,&doubles[(i+2*sample)%16],8);"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{ibytes}/8;i++)memcpy(a+8*i,&integers[(i+2*sample)%16],8);"
                )
            };
            let tail_check = if input == "i64x2" && output == "f32x4" {
                "for(unsigned i=8;i<16;i++)if(out[i])return 14;"
            } else {
                ""
            };
            checks += &format!(
                "{{unsigned char a[{ibytes}],s[{obytes}],out[208],expected[{obytes}],standard[{obytes}];for(unsigned i=0;i<{obytes};i++)s[i]=(unsigned char)(i*73+0xa5);for(unsigned sample=0;sample<8;sample++){{{fill}for(unsigned round=0;round<4;round++)for(unsigned pattern=0;pattern<4;pattern++){{unsigned char k=pattern==0?0:pattern==1?255:pattern==2?0xaa:0x80;unsigned csr=0x1f80|(round<<13)|(sample&1?0x8040:0)|(sample==3?0x21:0);memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(s,a,out,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(csr);ref_{name}(expected,s,a,k);unsigned ref_status=_mm_getcsr()&63;_mm_setcsr(csr);std_{name}(standard,s,a,k);if(memcmp(standard,expected,{obytes}))return 13;if(memcmp(out,expected,{obytes})||status!=ref_status){{fprintf(stderr,\"{name} sample%u round%u mask%u status%x ref%x\\n\",sample,round,pattern,status,ref_status);return 10;}}if(memcmp(out+64,a,{ibytes})||memcmp(out+128,s,{obytes})||out[192]!=k)return 11;for(unsigned i={obytes};i<64;i++)if(out[i]!=0xa5)return 12;{tail_check}memcpy(out,a,{ibytes});{name}(s,out,out,k);if(memcmp(out,expected,{obytes})||memcmp(out+64,a,{ibytes}))return 15;memcpy(out,s,{obytes});{name}(out,a,out,k);if(memcmp(out,expected,{obytes})||memcmp(out+128,s,{obytes}))return 16;}}}}}}\n"
            );
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn dq_qword_fp_conversion_c_interfaces() {
    let (source, harness) = sources();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-dq-convert-{}", std::process::id()));
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
            features: base,
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
fn dq_conversion_c_feature_controls() {
    let (source, _) = sources();
    for n in [7, 12] {
        let bad = source.replace(",8)", &format!(",{n})"));
        assert!(parse(&bad, Target::X86_64LinuxGnu).is_err());
    }
    let module = parse(
        &source.replace(
            "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) ",
            "",
        ),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx512f").unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).is_err()
    );
}
