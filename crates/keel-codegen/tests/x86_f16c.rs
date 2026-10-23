//! Architectural F16C access uses raw half bits and explicit rounding controls.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::new();
    for wide in [false, true] {
        let (width, raw, fp) = if wide {
            (256, "i16x8", "f32x8")
        } else {
            (128, "i64", "f32x4")
        };
        s += &format!(
            "func @from_half{width}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{raw} align 1 v0\n v3 = intrinsic x86_f16c_ph_to_ps{width}(v2)\n store align 1 v3,v1\n return\n}}\n"
        );
        for mode in 0..8 {
            s += &format!(
                "func @to_half{width}_{mode}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{fp} align 1 v0\n v3 = iconst.i32 {mode}\n v4 = intrinsic x86_f16c_ps_to_ph{width}(v2,v3)\n store align 1 v4,v1\n return\n}}\n"
            );
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);\nextern int memcmp(const void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from(
        "int main(void){unsigned saved=_mm_getcsr();_mm_setcsr((saved|0x1f80)&~0x8040);\n",
    );
    for wide in [false, true] {
        let (width, n, rawbytes) = if wide { (256, 8, 16) } else { (128, 4, 8) };
        c += &format!("extern void from_half{width}(const void*,void*);\n");
        for mode in 0..8 {
            c += &format!("extern void to_half{width}_{mode}(const void*,void*);\n");
        }
        let from = if wide {
            "_mm256_cvtph_ps(_mm_loadu_si128((const __m128i*)a))"
        } else {
            "_mm_cvtph_ps(_mm_loadl_epi64((const __m128i*)a))"
        };
        let vtype = if wide { "__m256" } else { "__m128" };
        main += &format!(
            "{{uint16_t a[8];unsigned char out[66];for(unsigned k=0;k<65536;k+={n}){{for(unsigned i=0;i<{n};i++)a[i]=k+i;for(unsigned i=0;i<66;i++)out[i]=0x5a;unsigned csr=_mm_getcsr()&~63u;_mm_setcsr(csr);volatile {vtype} e={from};unsigned ef=_mm_getcsr()&63;_mm_setcsr(csr);from_half{width}(a,out+1);unsigned rf=_mm_getcsr()&63;if(memcmp(out+1,(const void*)&e,{n}*4)||ef!=rf)return 10;if(out[0]!=0x5a||out[1+{n}*4]!=0x5a)return 11;}}}}\n"
        );
        for mode in 0..8 {
            let to = if wide {
                format!("_mm256_cvtps_ph(_mm256_loadu_ps(a),{mode})")
            } else {
                format!("_mm_cvtps_ph(_mm_loadu_ps(a),{mode})")
            };
            main += &format!(
                "{{float a[8];unsigned char out[34];uint32_t edges[]={{0,0x80000000,0x7f800000,0xff800000,0x7fc00001,0x7f800001,0x33800000,0x33000000,0x477fe000,0x477ff000,0x387fc000,0x38800000,0x3f801000,0x3f803000,0x80000001,1}};uint32_t rng=12345;for(unsigned round=0;round<4;round++)for(unsigned k=0;k<2048;k++){{for(unsigned i=0;i<{n};i++){{rng=rng*1664525u+1013904223u;uint32_t b=k<16?edges[(k+i)%16]:rng;memcpy(&a[i],&b,4);}}for(unsigned i=0;i<34;i++)out[i]=0x5a;unsigned csr=((saved|0x1f80)&~(0x8040|0x603f))|(round<<13);_mm_setcsr(csr);volatile __m128i e={to};unsigned ef=_mm_getcsr()&63;_mm_setcsr(csr);to_half{width}_{mode}(a,out+1);unsigned rf=_mm_getcsr()&63;if(memcmp(out+1,(const void*)&e,{rawbytes})||ef!=rf)return 20+{mode};if(out[0]!=0x5a||out[1+{rawbytes}]!=0x5a)return 30;}}}}\n"
            );
        }
    }
    c + &main + "_mm_setcsr(saved);return 0;}\n"
}
fn object(
    source: &str,
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    features: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if empty {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    Ok((b, text))
}
#[test]
fn f16c_typed_storage_rounding_and_exports() {
    let source = source();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(
        m.to_string(),
        keel_ir::parse(&m.to_string()).unwrap().to_string()
    );
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("f16c.ir"), &source).unwrap();
        fs::write(d.join("f16c.c"), harness()).unwrap();
    }
    let d = common::temp("f16c-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("f16c.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mf16c",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("f16c.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) =
                    object(&source, s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                assert!(text.contains("vcvtph2ps"));
                assert!(text.contains("vcvtps2ph"));
                if let Some(d) = &export {
                    let name = format!("f16c-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn f16c_requires_features_and_well_typed_constant_rounding() {
    let s = source();
    let mut minimal = Features::NONE;
    minimal.avx = true;
    minimal.f16c = true;
    object(&s, SelectionTier::Full, AllocationTier::Full, true, minimal).unwrap();
    for missing in [0, 1] {
        let mut features = Features::for_level(CpuLevel::V3);
        if missing == 0 {
            features.f16c = false;
        } else {
            features.avx = false;
            features.avx2 = false;
            features.fma = false;
        }
        assert!(
            object(
                &s,
                SelectionTier::Baseline,
                AllocationTier::Full,
                false,
                features
            )
            .is_err()
        );
    }
    for bad in [
        s.replace("iconst.i32 0", "iconst.i32 8"),
        s.replace("iconst.i32 0", "iconst.i32 -1"),
        s.replace("load.i64 align 1", "load.i32 align 1"),
    ] {
        let m = keel_ir::parse(&bad).unwrap();
        assert!(keel_ir::verify_module(&m).is_err());
    }
    let dynamic = "func @bad(f32x4,i32)->i64 {\nblock0(v0:f32x4,v1:i32):\n v2 = intrinsic x86_f16c_ps_to_ph128(v0,v1)\n return v2\n}\n";
    assert!(keel_ir::verify_module(&keel_ir::parse(dynamic).unwrap()).is_err());
    let narrow = s.split("func @from_half256").next().unwrap();
    assert!(
        keel_codegen::aarch64::compile(&keel_ir::parse(narrow).unwrap())
            .unwrap_err()
            .message
            .contains("F16C")
    );
    let mut m = keel_ir::parse(narrow).unwrap();
    let count=m.functions.iter().map(|f|f.layout_insts().filter(|(_,i)|matches!(f.inst_data(*i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_f16c())).count()).sum::<usize>();
    keel_opt::optimize(&mut m).unwrap();
    assert_eq!(count,m.functions.iter().map(|f|f.layout_insts().filter(|(_,i)|matches!(f.inst_data(*i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_f16c())).count()).sum::<usize>());
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_f16c_all_half_patterns_rounding_and_flags() {
    if !std::is_x86_feature_detected!("f16c") || !std::is_x86_feature_detected!("avx") {
        return;
    }
    let d = common::temp("f16c-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("f16c.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) =
                    object(&source(), s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("f16c.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args([
                            "-O2",
                            "-mavx",
                            "-mf16c",
                            "-fno-fast-math",
                            "-ffp-contract=off",
                        ])
                        .arg(d.join("f16c.c"))
                        .arg(d.join("f16c.o"))
                        .arg("-o")
                        .arg(d.join("f16c")),
                );
                common::command(&mut Command::new(d.join("f16c")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
