//! Raw conversion bits and MXCSR match native AVX512F, including SAE and masks.
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
// Opcode, exact input/output shape, native API suffix, rounding recipe.
const FORMS: [(&str, &str, &str, &str, &str); 16] = [
    ("vcvtdq2ps", "i32x16", "f32x16", "cvt_roundepi32_ps", "rc"),
    ("vcvtudq2ps", "i32x16", "f32x16", "cvt_roundepu32_ps", "rc"),
    ("vcvtdq2pd", "i32x8", "f64x8", "cvtepi32_pd", "exact"),
    ("vcvtudq2pd", "i32x8", "f64x8", "cvtepu32_pd", "exact"),
    ("vcvtps2dq", "f32x16", "i32x16", "cvt_roundps_epi32", "rc"),
    ("vcvtps2udq", "f32x16", "i32x16", "cvt_roundps_epu32", "rc"),
    (
        "vcvttps2dq",
        "f32x16",
        "i32x16",
        "cvtt_roundps_epi32",
        "sae",
    ),
    (
        "vcvttps2udq",
        "f32x16",
        "i32x16",
        "cvtt_roundps_epu32",
        "sae",
    ),
    ("vcvtpd2dq", "f64x8", "i32x8", "cvt_roundpd_epi32", "rc"),
    ("vcvtpd2udq", "f64x8", "i32x8", "cvt_roundpd_epu32", "rc"),
    ("vcvttpd2dq", "f64x8", "i32x8", "cvtt_roundpd_epi32", "sae"),
    ("vcvttpd2udq", "f64x8", "i32x8", "cvtt_roundpd_epu32", "sae"),
    ("vcvtps2pd", "f32x8", "f64x8", "cvt_roundps_pd", "sae"),
    ("vcvtpd2ps", "f64x8", "f32x8", "cvt_roundpd_ps", "rc"),
    ("vcvtph2ps", "i16x16", "f32x16", "cvt_roundph_ps", "sae"),
    ("vcvtps2ph", "f32x16", "i16x16", "cvt_roundps_ph", "half"),
];
fn ctype(shape: &str) -> (&'static str, usize) {
    match shape {
        "i32x16" => ("__m512i", 64),
        "i32x8" | "i16x16" => ("__m256i", 32),
        "f32x16" => ("__m512", 64),
        "f32x8" => ("__m256", 32),
        "f64x8" => ("__m512d", 64),
        _ => unreachable!(),
    }
}
fn sources() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nstatic const uint32_t floats[]={0,0x80000000,1,0x007fffff,0x3fc00000,0xbfc00000,0x4f000000,0x4f800000,0x7f800000,0xff800000,0x7fc12345,0x7f800001,0x477ff000,0xc7000000,0x3f000001,0x00800000};\nstatic const uint64_t doubles[]={0,0x8000000000000000ULL,1,0x000fffffffffffffULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x41e0000000000000ULL,0x41f0000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0000000000001ULL,0x40effe0000000000ULL,0xc0e0000000000000ULL,0x3ff0000010000000ULL,0x0010000000000000ULL};\nstatic const uint16_t halves[]={0,0x8000,1,0x3ff,0x400,0x3c00,0xbc00,0x7bff,0x7c00,0xfc00,0x7e55,0x7c01,0x3555,0x8001,0x3c01,0x0401};\nstatic const uint32_t integers[]={0,1,0xffffffff,0x80000000,0x7fffffff,0x1000001,0x1000003,0xfffffeff};\n",
    );
    let mut checks = String::new();
    for (opcode, input, output, api, recipe) in FORMS {
        let (itype, ibytes) = ctype(input);
        let (otype, obytes) = ctype(output);
        let lanes = if output == "i16x16" || output == "f32x16" || output == "i32x16" {
            16
        } else {
            8
        };
        let (mask, cmask) = if lanes == 16 {
            ("i16", "__mmask16")
        } else {
            ("i8", "__mmask8")
        };
        let controls: &[i32] = match recipe {
            "rc" => &[4, 8, 9, 10, 11],
            "sae" => &[4, 8],
            "half" => &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 31, 255,
            ],
            _ => &[4],
        };
        for &control in controls {
            let name = format!("{opcode}_{control}");
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4 = load.{output} align 1 v0\n v5 = load.{input} align 1 v1\n v6 = iconst.i32 {control}\n v7 = intrinsic x86_avx512_{opcode}(v4,v3,v5,v6)\n store align 1 v7,v2\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v5,v9\n v10 = ptradd v9,v8\n store align 1 v4,v10\n v11 = ptradd v10,v8\n store align 1 v3,v11\n return\n}}\n"
            );
            let rounding = if recipe == "exact" {
                String::new()
            } else {
                format!(",{control}")
            };
            if recipe == "half" {
                // Clang treats noncanonical controls such as 13 as raw imm8
                // without SAE. Use explicit assembly for the declared bit 3
                // contract, while retaining the standard-header value oracle.
                let sae = if control & 8 != 0 { "%{sae%}," } else { "" };
                c += &format!(
                    "extern void {name}(void*,void*,void*,{cmask});\n__attribute__((noinline)) static void ref_{name}(void*o,const void*s,const void*a,{cmask} k){{{itype} x;{otype} result;memcpy(&x,a,{ibytes});memcpy(&result,s,{obytes});__asm__ volatile(\"vcvtps2ph ${control},{sae}%2,%0%{{%1%}}\":\"+v\"(result):\"Yk\"(k),\"v\"(x));memcpy(o,&result,{obytes});}}\n__attribute__((noinline)) static void std_{name}(void*o,const void*s,const void*a,{cmask} k){{{itype} x;{otype} merge;memcpy(&x,a,{ibytes});memcpy(&merge,s,{obytes});{otype} result=_mm512_mask_{api}(merge,k,x{rounding});memcpy(o,&result,{obytes});}}\n"
                );
            } else {
                c += &format!(
                    "extern void {name}(void*,void*,void*,{cmask});\n__attribute__((noinline)) static void ref_{name}(void*o,const void*s,const void*a,{cmask} k){{{itype} x;{otype} merge;memcpy(&x,a,{ibytes});memcpy(&merge,s,{obytes});{otype} result=_mm512_mask_{api}(merge,k,x{rounding});memcpy(o,&result,{obytes});}}\n"
                );
            }
            let fill = if input.starts_with("f32") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/4;i++)memcpy(a+4*i,&floats[(i+sample*5)%16],4);"
                )
            } else if input.starts_with("f64") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/8;i++)memcpy(a+8*i,&doubles[(i+sample*5)%16],8);"
                )
            } else if input.starts_with("i16") {
                format!(
                    "for(unsigned i=0;i<{ibytes}/2;i++)memcpy(a+2*i,&halves[(i+sample*5)%16],2);"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{ibytes}/4;i++)memcpy(a+4*i,&integers[(i+sample*3)%8],4);"
                )
            };
            let vendor_check = if recipe == "half" {
                format!(
                    "_mm_setcsr(csr);std_{name}(expected,s,a,k);if(memcmp(out,expected,{obytes}))return 13;"
                )
            } else {
                String::new()
            };
            checks += &format!(
                "{{unsigned char a[{ibytes}],s[{obytes}],out[208],expected[{obytes}];for(unsigned i=0;i<{obytes};i++)s[i]=(unsigned char)(i*73+0xa5);for(unsigned sample=0;sample<3;sample++){{{fill}for(unsigned round=0;round<4;round++)for(unsigned pattern=0;pattern<3;pattern++){{{cmask} k=pattern==0?0:pattern==1?({cmask})~0u:({cmask})0xaaaa;unsigned csr=0x1f80|(round<<13);memset(out,0xa5,sizeof(out));_mm_setcsr(csr);{name}(s,a,out,k);unsigned observed=_mm_getcsr();unsigned status=observed&63;if((observed&~63u)!=(csr&~63u))return 14;_mm_setcsr(csr);ref_{name}(expected,s,a,k);unsigned ref_status=_mm_getcsr()&63;if(memcmp(out,expected,{obytes})||status!=ref_status||(!k&&status)||({control}&8&&status)){{fprintf(stderr,\"{name} sample%u round%u mask%u status%x ref%x\\n\",sample,round,pattern,status,ref_status);return 10;}}{vendor_check}if(memcmp(out+64,a,{ibytes})||memcmp(out+128,s,{obytes})||memcmp(out+192,&k,sizeof(k)))return 11;for(unsigned i={obytes};i<64;i++)if(out[i]!=0xa5)return 12;}}}}}}\n"
            );
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn avx512_conversion_bits_mxcsr_masks_and_spills() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-convert-raw-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    #[cfg(target_arch = "x86_64")]
    let native = cfg!(target_os = "linux") && std::is_x86_feature_detected!("avx512f");
    #[cfg(not(target_arch = "x86_64"))]
    let native = false;
    if native {
        let output = Command::new("cc")
            .args(["-O1", "-mavx512f", "-frounding-math", "-c"])
            .arg(dir.join("harness.c"))
            .arg("-o")
            .arg(dir.join("harness.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
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
fn avx512_conversion_contracts() {
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_convert())
    {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(!id.info().pure);
        assert!(data.is_pinned() && data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::Barrier);
        assert!(id.valid_x86_avx512_convert_control(4));
        let half = id == Intrinsic::X86Avx512Vcvtps2ph;
        assert_eq!(id.valid_x86_avx512_convert_control(7), half);
        assert_eq!(id.valid_x86_avx512_convert_control(16), half);
        assert!(!id.valid_x86_avx512_convert_control(-1));
        assert!(!id.valid_x86_avx512_convert_control(256));
    }
    let (source, _) = sources();
    let bad = source.replace("iconst.i32 4", "iconst.i32 7");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let m = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
}
