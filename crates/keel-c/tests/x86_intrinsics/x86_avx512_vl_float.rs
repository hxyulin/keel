//! VL floating wrappers preserve raw active lanes and exact inactive bits.
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
    for (width, prefix, container, suffix, lanes) in [
        (128, "_mm", "__m128", "ps", 4),
        (128, "_mm", "__m128d", "pd", 2),
        (256, "_mm256", "__m256", "ps", 8),
        (256, "_mm256", "__m256d", "pd", 4),
    ] {
        let bytes = width / 8;
        for operation in ["mul", "div", "sqrt", "min", "max"] {
            let name = format!("consumer_{operation}{suffix}{width}");
            let unary = operation == "sqrt";
            let a = if unary { "s" } else { "a" };
            let value = format!(
                "{prefix}_mask_{operation}_{suffix}(s,k,{a}{} )",
                if unary { "" } else { ",b" }
            );
            let zero = format!(
                "{prefix}_maskz_{operation}_{suffix}(k,{a}{} )",
                if unary { "" } else { ",b" }
            );
            let body = format!(
                "{container} s=*(const {container}*)sp,a=*(const {container}*)ap,b=*(const {container}*)bp;{container} r={value};{prefix}_storeu_{suffix}(out,r);{container} z={zero};{prefix}_storeu_{suffix}((void*)((char*)out+64),z);"
            );
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void {name}(const void*sp,const void*ap,const void*bp,void*out,__mmask8 k){{{body}}}\n"
            );
            oracle += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,uint8_t);\n__attribute__((noinline)) static void oracle_{name}(const void*sp,const void*ap,const void*bp,void*out,uint8_t k){{{body}}}\n"
            );
            let initialize = if suffix == "ps" {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{((uint32_t*)s)[i]=floats[(i+sample)%8];((uint32_t*)a)[i]=floats[(i+sample+1)%8];((uint32_t*)b)[i]=floats[(i+sample*3+3)%8];}}"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{s[i]=doubles[(i+sample)%8];a[i]=doubles[(i+sample+1)%8];b[i]=doubles[(i+sample*3+3)%8];}}"
                )
            };
            cases += &format!(
                "{{uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),out[16],want[16];uint8_t masks[]={{0,1,0x55,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<8;sample++)for(unsigned mode=0;mode<4;mode++)for(unsigned mi=0;mi<5;mi++){{{initialize}unsigned csr=(saved&~0x603f)|(mode<<13);_mm_setcsr(csr);oracle_{name}(s,a,b,want,masks[mi]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{name}(s,a,b,out,masks[mi]);unsigned actual=_mm_getcsr()&0x3f;if(memcmp(out,want,{bytes})||memcmp(out+8,want+8,{bytes})||actual!=expected){{fprintf(stderr,\"{name} sample=%u mode=%u mask=%x flags=%x/%x\\n\",sample,mode,masks[mi],actual,expected);return 1;}}}}}}\n"
            );
        }
    }
    source += "int argument_count;__m128 argument_vector(void){argument_count++;return (__m128){0};}__mmask8 argument_mask(void){argument_count++;return 1;}__attribute__((target(\"avx512vl\"))) int evaluate_once(void){argument_count=0;__m128 r=_mm_mask_mul_ps(argument_vector(),argument_mask(),argument_vector(),argument_vector());(void)r;return argument_count;}\n";
    oracle += "extern int evaluate_once(void);\n";
    oracle += &format!(
        "int main(void){{if(evaluate_once()!=4)return 2;unsigned saved=_mm_getcsr();uint32_t floats[8]={{0,0x80000000u,0x3fc00000u,0xbfc00000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u}};uint64_t doubles[8]={{0,UINT64_C(0x8000000000000000),UINT64_C(0x3ff8000000000000),UINT64_C(0xbff8000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc)}};{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, oracle)
}
#[test]
fn vl_float_standard_wrappers_match_raw_hardware_bits_and_flags() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for id in Intrinsic::ALL
        .iter()
        .filter(|id| id.x86_avx512_vl_float_arithmetic().is_some())
    {
        assert!(ir.contains(id.info().name), "missing {}", id.info().name);
    }
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !base.avx512vl);
    let without = parse(
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
        keel_codegen::x86_64::compile_program_with_pipeline(&without, &EMPTY, &pipeline).is_err()
    );
    let directory = std::env::temp_dir().join(format!("keel-c-vl-float-{}", std::process::id()));
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
                eprintln!("SKIP native VL floating C wrappers: AVX512VL unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args([
                    "-O2",
                    "-mavx512f",
                    "-mavx512vl",
                    "-frounding-math",
                    "-ffp-contract=off",
                ])
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
    for expression in [
        "_mm_mask_mul_ps(s,k,a,b,8)",
        "_mm_mask_sqrt_ps(s,k,a,8)",
        "_mm256_mask_min_pd(s,k,a,b,8)",
    ] {
        let container = if expression.starts_with("_mm256") {
            "__m256d"
        } else {
            "__m128"
        };
        let bad = format!(
            "#include <immintrin.h>\n{container} f({container} s,{container} a,{container} b,__mmask8 k){{return {expression};}}"
        );
        assert!(parse(&bad, Target::X86_64LinuxGnu).is_err(), "{expression}");
    }
}
