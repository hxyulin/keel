//! Standard VL wrappers keep typed widths and function-local permissions.
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\n",
    );
    let mut cases = String::new();
    for (width, prefix, ivector) in [(128, "_mm", "__m128i"), (256, "_mm256", "__m256i")] {
        for (suffix, vector, lanes, lane_bytes) in [
            ("epi32", ivector, width / 32, 4),
            ("epi64", ivector, width / 64, 8),
            (
                "ps",
                if width == 128 { "__m128" } else { "__m256" },
                width / 32,
                4,
            ),
            (
                "pd",
                if width == 128 { "__m128d" } else { "__m256d" },
                width / 64,
                8,
            ),
        ] {
            let bytes = width / 8;
            let name = format!("consumer_{suffix}{width}");
            let floating = suffix == "ps" || suffix == "pd";
            let store = if floating {
                format!("{prefix}_storeu_{suffix}")
            } else {
                format!("{prefix}_storeu_si{width}")
            };
            let unsigned = if suffix == "epi32" { "epu32" } else { "epu64" };
            let mut body = format!(
                "{vector} s=*(const {vector}*)sp,b=*(const {vector}*)bp;{vector} x={prefix}_mask_loadu_{suffix}(s,k,ap);{vector} y={prefix}_mask_add_{suffix}(s,k,x,b);{vector} z={prefix}_mask_sub_{suffix}(s,k,y,x);{prefix}_mask_storeu_{suffix}(out,k,z);((unsigned char*)out)[64]={prefix}_mask_cmp_{suffix}_mask(k,y,x,{});",
                if floating { 3 } else { 0 }
            );
            if !floating {
                body += &format!(
                    "((unsigned char*)out)[65]={prefix}_mask_cmp_{unsigned}_mask(k,y,x,6);"
                );
            }
            body += &format!("{store}((void*)((char*)out+96),s);");
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void {name}(const void*sp,const void*ap,const void*bp,void*out,__mmask8 k){{{body}}}\n"
            );
            oracle += &format!(
                "extern void {name}(const void*,const void*,const void*,void*,uint8_t);\n__attribute__((noinline)) static void oracle_{name}(const void*sp,const void*ap,const void*bp,void*out,uint8_t k){{{body}}}\n"
            );
            source += &format!(
                "__attribute__((target(\"avx512vl\"))) void zero_{suffix}{width}(const void*p,void*out,__mmask8 k){{{vector} x={prefix}_maskz_loadu_{suffix}(k,p);{vector} y={prefix}_maskz_add_{suffix}(k,x,x);{store}(out,{prefix}_maskz_sub_{suffix}(k,y,x));}}\n"
            );
            oracle += &format!("extern void zero_{suffix}{width}(const void*,void*,uint8_t);\n");
            let initialize = if suffix == "ps" {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{((uint32_t*)s)[i]=0x7f812345u+i;((uint32_t*)a)[i]=0x3f800001u+i;((uint32_t*)b)[i]=i%2?0x7fc54321u:0x80000000u;}}"
                )
            } else if suffix == "pd" {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{s[i]=UINT64_C(0x7ff0123456789abc)+i;a[i]=UINT64_C(0x3ff0000000000001)+i;b[i]=i%2?UINT64_C(0x7ff8543212345678):UINT64_C(0x8000000000000000);}}"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{};i++){{s[i]=UINT64_C(0x80000000ffffffff)+i*17;a[i]=UINT64_C(0xdeadbeef01234567)+i*173;b[i]=UINT64_C(0xa5a5a5a5fedcba98)-i*37;}}",
                    bytes / 8
                )
            };
            let check = if floating {
                format!("memcmp(out,want,65)")
            } else {
                "memcmp(out,want,66)".into()
            };
            cases += &format!(
                "{{uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),out[24],want[24];{initialize}uint8_t high=(uint8_t)~((1u<<{lanes})-1);uint8_t masks[]={{0,1,0x55,0xff,high}};for(unsigned i=0;i<5;i++){{memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);unsigned csr=saved&~0x3f;_mm_setcsr(csr);oracle_{name}(s,a,b,want,masks[i]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{name}(s,a,b,out,masks[i]);if({check}||(_mm_getcsr()&0x3f)!=expected||memcmp((char*)out+96,s,{bytes}))return 1;}}memset(out,0xa5,sizeof out);zero_{suffix}{width}(m+page,out,high);for(unsigned i=0;i<{bytes};i++)if(((unsigned char*)out)[i])return 2;unsigned char*p=m+page-{lane_bytes};memcpy(p,a,{lane_bytes});memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);oracle_{name}(s,p,b,want,high|1);{name}(s,p,b,out,high|1);if({check})return 3;_mm_setcsr(saved);}}\n"
            );
        }
    }
    oracle += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m,page,PROT_READ|PROT_WRITE))return 4;{cases}_mm_setcsr(saved);munmap(m,page*2);return 0;}}\n"
    );
    (source, oracle)
}
#[test]
fn vl_standard_wrappers_keep_width_masks_fault_suppression_and_permissions() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for id in Intrinsic::ALL
        .iter()
        .filter(|id| id.is_x86_avx512_vl_core())
    {
        assert!(ir.contains(id.info().name), "missing {}", id.info().name);
    }
    let features = Features::NONE.with_features("avx512f").unwrap();
    let directory = std::env::temp_dir().join(format!("keel-c-vl-core-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    let without = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&without, &EMPTY, &pipeline).is_err()
    );
    let encoding = module_features(&module, features).unwrap();
    assert!(encoding.avx512vl && !features.avx512vl);
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
        fs::write(directory.join("consumer.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native VL C wrappers: AVX512VL unavailable");
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
    for (suffix, maximum) in [("epi32", 7), ("ps", 31)] {
        for predicate in ["p", "-1", if maximum == 7 { "8" } else { "32" }] {
            let container = if suffix == "ps" { "__m128" } else { "__m128i" };
            let bad = format!(
                "#include <immintrin.h>\n__mmask8 f({container} a,{container} b,int p){{return _mm_mask_cmp_{suffix}_mask(0xff,a,b,{predicate});}}"
            );
            assert!(parse(&bad, Target::X86_64LinuxGnu).is_err(), "{predicate}");
        }
    }
}
