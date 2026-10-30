//! CD wrappers cover merge, zero mask and unmasked forms at each packed width.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[test]
fn cd_wrappers_match_system_intrinsics_and_require_features() {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut checks = String::new();
    for (width, prefix) in [(128, "_mm"), (256, "_mm256"), (512, "_mm512")] {
        for bits in [32, 64] {
            for op in ["conflict", "lzcnt"] {
                let name = format!("consumer_{op}{bits}_{width}");
                let container = format!("__m{width}i");
                let lanes = width / bits;
                let mask = if lanes == 16 { "__mmask16" } else { "__mmask8" };
                let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
                let target = if width == 512 {
                    "avx512cd"
                } else {
                    "avx512cd,avx512vl"
                };
                let body = format!(
                    "{container} s=*(const {container}*)sp,a=*(const {container}*)ap;{prefix}_storeu_si{width}(out,{prefix}_mask_{op}_epi{bits}(s,k,a));{prefix}_storeu_si{width}((void*)((char*)out+64),{prefix}_maskz_{op}_epi{bits}(k,a));{prefix}_storeu_si{width}((void*)((char*)out+128),{prefix}_{op}_epi{bits}(a));"
                );
                source += &format!(
                    "__attribute__((target(\"{target}\"))) void {name}(const void*sp,const void*ap,void*out,{mask} k){{{body}}}\n"
                );
                oracle += &format!(
                    "extern void {name}(const void*,const void*,void*,{cmask});\nstatic void oracle_{name}(const void*sp,const void*ap,void*out,{mask} k){{{body}}}\n"
                );
                checks += &format!(
                    "{{uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64)));unsigned char out[192],want[192];{cmask} masks[]={{0,1,2,0x55,({cmask})~0u,({cmask})~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<4;sample++){{for(unsigned i=0;i<8;i++){{s[i]=UINT64_C(0x8000000000000000)+i;a[i]=sample==0?0:sample==1?7:sample==2?(i%2):UINT64_C(0xffffffff00000001);}}for(unsigned mi=0;mi<6;mi++){{memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);unsigned csr=saved|0x3f;_mm_setcsr(csr);oracle_{name}(s,a,want,masks[mi]);{name}(s,a,out,masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,sizeof out)){{fprintf(stderr,\"{name} sample=%u mask=%x\\n\",sample,masks[mi]);return 1;}}_mm_setcsr(saved);}}}}}}\n"
                );
            }
        }
    }
    for (width, prefix) in [(128, "_mm"), (256, "_mm256"), (512, "_mm512")] {
        for (api, mask, cmask, uint, maximum) in [
            ("broadcastmb_epi64", "__mmask8", "uint8_t", "uint64_t", 255),
            (
                "broadcastmw_epi32",
                "__mmask16",
                "uint16_t",
                "uint32_t",
                65535,
            ),
        ] {
            let name = format!("consumer_{api}_{width}");
            let target = if width == 512 {
                "avx512cd"
            } else {
                "avx512cd,avx512vl"
            };
            source += &format!(
                "__attribute__((target(\"{target}\"))) void {name}(void*out,{mask} k){{{prefix}_storeu_si{width}(out,{prefix}_{api}(k));}}\n"
            );
            oracle += &format!("extern void {name}(void*,{cmask});\n");
            checks += &format!(
                "{{unsigned masks[]={{0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff}};for(unsigned mi=0;mi<8;mi++){{{cmask} k=({cmask})masks[mi];unsigned char out[64];__m{width}i want={prefix}_{api}(k);memset(out,0xa5,sizeof out);unsigned csr=saved|0x3f;_mm_setcsr(csr);{name}(out,k);if(_mm_getcsr()!=csr||memcmp(out,&want,{width}/8))return 2;for(unsigned lane=0;lane<{width}/(8*sizeof({uint}));lane++){{{uint} got;memcpy(&got,out+lane*sizeof got,sizeof got);if(got!=({uint})(k&{maximum}u))return 3;}}for(unsigned i={width}/8;i<64;i++)if(out[i]!=0xa5)return 4;_mm_setcsr(saved);}}}}\n"
            );
        }
    }
    oracle += &format!("int main(void){{unsigned saved=_mm_getcsr();{checks}return 0;}}\n");
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512cd && features.avx512vl && !features.lzcnt);
    let missing = source
        .replace("__attribute__((target(\"avx512cd\"))) ", "")
        .replace("__attribute__((target(\"avx512cd,avx512vl\"))) ", "");
    let missing = parse(&missing, Target::X86_64LinuxGnu).unwrap();
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
    let directory = std::env::temp_dir().join(format!("keel-c-cd-{}", std::process::id()));
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
            if !std::is_x86_feature_detected!("avx512cd")
                || !std::is_x86_feature_detected!("avx512vl")
            {
                eprintln!("SKIP native C CD: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O2", "-mavx512f", "-mavx512cd", "-mavx512vl"])
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
            assert!(
                Command::new(directory.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}
