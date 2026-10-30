//! VL integer and movement wrappers retain scalar masks and exact payload bits.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{Intrinsic, Target};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    for (width, prefix, integer) in [(128, "_mm", "__m128i"), (256, "_mm256", "__m256i")] {
        let specs = [
            ("mullo_epi32", integer, 32, false),
            ("min_epi32", integer, 32, false),
            ("min_epu32", integer, 32, false),
            ("max_epi32", integer, 32, false),
            ("max_epu32", integer, 32, false),
            ("and_epi32", integer, 32, false),
            ("andnot_epi32", integer, 32, false),
            ("or_epi32", integer, 32, false),
            ("xor_epi32", integer, 32, false),
            ("min_epi64", integer, 64, false),
            ("min_epu64", integer, 64, false),
            ("max_epi64", integer, 64, false),
            ("max_epu64", integer, 64, false),
            ("and_epi64", integer, 64, false),
            ("andnot_epi64", integer, 64, false),
            ("or_epi64", integer, 64, false),
            ("xor_epi64", integer, 64, false),
            ("mov_epi32", integer, 32, true),
            ("mov_epi64", integer, 64, true),
            (
                "mov_ps",
                if width == 128 { "__m128" } else { "__m256" },
                32,
                true,
            ),
            (
                "mov_pd",
                if width == 128 { "__m128d" } else { "__m256d" },
                64,
                true,
            ),
        ];
        for (api, container, lane_bits, unary) in specs {
            let name = format!("consumer_{api}{width}");
            let bytes = width / 8;
            let lanes = width / lane_bits;
            let save = match api {
                "mov_ps" => format!("{prefix}_storeu_ps"),
                "mov_pd" => format!("{prefix}_storeu_pd"),
                _ => format!("{prefix}_storeu_si{width}"),
            };
            let arguments = if unary { "s,k,a" } else { "s,k,a,b" };
            let zero = if unary { "k,a" } else { "k,a,b" };
            let body = format!(
                "{container} s=*(const {container}*)sp,a=*(const {container}*)ap,b=*(const {container}*)bp;{save}(out,{prefix}_mask_{api}({arguments}));{save}((void*)((char*)out+64),{prefix}_maskz_{api}({zero}));"
            );
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void {name}(const void*sp,const void*ap,const void*bp,void*out,__mmask8 k){{{body}}}\n"
            );
            oracle += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,uint8_t);\n__attribute__((noinline)) static void oracle_{name}(const void*sp,const void*ap,const void*bp,void*out,uint8_t k){{{body}}}\n"
            );
            cases += &format!(
                "{{uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),out[16],want[16];for(unsigned sample=0;sample<16;sample++){{for(unsigned i=0;i<{};i++){{s[i]=edges[(i+sample)%16];a[i]=edges[(i+sample+3)%16];b[i]=edges[(i*3+sample+7)%16];}}uint8_t masks[]={{0,1,0x55,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned mi=0;mi<5;mi++){{unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);oracle_{name}(s,a,b,want,masks[mi]);if(_mm_getcsr()!=csr)return 1;{name}(s,a,b,out,masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,{bytes})||memcmp(out+8,want+8,{bytes})){{fprintf(stderr,\"{name} sample=%u mask=%x\\n\",sample,masks[mi]);return 2;}}_mm_setcsr(saved);}}}}}}\n",
                bytes / 8
            );
        }
        for (suffix, container, lane_bits) in [
            ("epi32", integer, 32),
            ("epi64", integer, 64),
            ("ps", if width == 128 { "__m128" } else { "__m256" }, 32),
            ("pd", if width == 128 { "__m128d" } else { "__m256d" }, 64),
        ] {
            let name = format!("blend_{suffix}{width}");
            let lanes = width / lane_bits;
            let uint = if lane_bits == 32 {
                "uint32_t"
            } else {
                "uint64_t"
            };
            let save = if suffix == "ps" || suffix == "pd" {
                format!("{prefix}_storeu_{suffix}")
            } else {
                format!("{prefix}_storeu_si{width}")
            };
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void {name}(const void*ap,const void*bp,void*out,__mmask8 k){{{container} a=*(const {container}*)ap,b=*(const {container}*)bp;{save}(out,{prefix}_mask_blend_{suffix}(k,a,b));}}\n"
            );
            oracle += &format!("extern void {name}(const void*,const void*,void*,uint8_t);\n");
            cases += &format!(
                "{{{uint} a[{lanes}],b[{lanes}],out[{lanes}];for(unsigned i=0;i<{lanes};i++){{a[i]=({uint})(UINT64_C(0x7ff012347f812345)+i*17);b[i]=({uint})(UINT64_C(0x80000000fff85432)-i*31);}}uint8_t masks[]={{0,1,0x55,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned mi=0;mi<5;mi++){{unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);{name}(a,b,out,masks[mi]);if(_mm_getcsr()!=csr)return 3;for(unsigned lane=0;lane<{lanes};lane++)if(out[lane]!=((masks[mi]&(1u<<lane))?b[lane]:a[lane]))return 4;_mm_setcsr(saved);}}}}\n"
            );
        }
    }
    oracle += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t edges[16]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x7fffffff80000000),UINT64_C(0xffffffff00000001),UINT64_C(0x0000000080000001),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x7ff812347fc12345),UINT64_C(0x7ff012347f812345),UINT64_C(0xfff85432ffc54321),UINT64_C(0xfff05432ff812345),UINT64_C(0x3ff000003f800000),UINT64_C(0x8000000080000000),17}};{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, oracle)
}
#[test]
fn vl_integer_move_and_blend_wrappers_match_bits_without_fp_effects() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for id in Intrinsic::ALL
        .iter()
        .filter(|id| id.x86_avx512_vl_integer_move().is_some())
    {
        assert!(ir.contains(id.info().name), "missing {}", id.info().name);
    }
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !base.avx512vl);
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
    let directory =
        std::env::temp_dir().join(format!("keel-c-vl-integer-move-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("consumer.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native VL integer/move C wrappers: AVX512VL unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O2", "-mavx512f", "-mavx512vl"])
                .arg(directory.join("oracle.c"))
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
            let status = Command::new(directory.join("run")).status().unwrap();
            assert!(status.success(), "{status}");
        }
    }
}
