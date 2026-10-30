//! Native narrow conversion bits, MXCSR, short tails and live operands.
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
    let mut ir = String::new();
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
            let tail = if width == 512 { ",v6" } else { "" };
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4 = load.{output} align 1 v0\n v5 = load.{input} align 1 v1\n v6 = iconst.i32 {control}\n v7 = intrinsic x86_avx512dq_{opcode}{width}(v4,v3,v5{tail})\n store align 1 v7,v2\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v5,v9\n v10 = ptradd v9,v8\n store align 1 v4,v10\n v11 = ptradd v10,v8\n store align 1 v3,v11\n return\n}}\n"
            );
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
                "{{unsigned char a[{ibytes}],s[{obytes}],out[208],expected[{obytes}];for(unsigned i=0;i<{obytes};i++)s[i]=(unsigned char)(i*73+0xa5);for(unsigned sample=0;sample<8;sample++){{{fill}for(unsigned round=0;round<4;round++)for(unsigned pattern=0;pattern<4;pattern++){{unsigned char k=pattern==0?0:pattern==1?255:pattern==2?0xaa:0x80;unsigned csr=0x1f80|(round<<13)|(sample&1?0x8040:0)|(sample==3?0x21:0);memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(s,a,out,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(csr);ref_{name}(expected,s,a,k);unsigned ref_status=_mm_getcsr()&63;if(memcmp(out,expected,{obytes})||status!=ref_status){{fprintf(stderr,\"{name} sample%u round%u mask%u status%x ref%x\\n\",sample,round,pattern,status,ref_status);return 10;}}if(memcmp(out+64,a,{ibytes})||memcmp(out+128,s,{obytes})||out[192]!=k)return 11;for(unsigned i={obytes};i<64;i++)if(out[i]!=0xa5)return 12;{tail_check}memcpy(out,a,{ibytes});{name}(s,out,out,k);if(memcmp(out,expected,{obytes})||memcmp(out+64,a,{ibytes}))return 15;memcpy(out,s,{obytes});{name}(out,a,out,k);if(memcmp(out,expected,{obytes})||memcmp(out+128,s,{obytes}))return 16;}}}}}}\n"
            );
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn dq_qword_fp_conversion_bits_mxcsr_masks_and_spills() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE
        .with_features("avx512f,avx512dq,avx512vl")
        .unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-dq-convert-{}", std::process::id()));
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
fn dq_qword_fp_conversion_contracts() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_dq_convert().is_some())
        .collect();
    assert_eq!(ids.len(), 36);
    for id in ids {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(!id.info().pure);
        assert!(data.is_pinned() && data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::Barrier);
        let (_, width, _, _) = id.x86_avx512_dq_convert().unwrap();
        assert_eq!(id.info().params.len(), if width == 512 { 4 } else { 3 });
    }
    let (source, _) = sources();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for n in [-1, 0, 7, 12, 255] {
        let bad = source.replace("iconst.i32 4", &format!("iconst.i32 {n}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    for flags in ["avx512f,avx512vl", "avx512f,avx512dq"] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features(flags).unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                .is_err()
        );
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
