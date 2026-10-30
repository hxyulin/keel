//! Public mask, mask3 and mask-zero VL FMA wrappers preserve their merge source.
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
fn vl_fma_wrappers_match_system_intrinsics_and_feature_contract() {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut checks = String::new();
    for (width, prefix) in [(128, "_mm"), (256, "_mm256")] {
        for family in ["fmadd", "fmsub", "fnmadd", "fnmsub", "fmaddsub", "fmsubadd"] {
            for suffix in ["ps", "pd"] {
                for mask_kind in ["mask", "mask3", "maskz"] {
                    let bits = if suffix == "ps" { 32 } else { 64 };
                    let lanes = width / bits;
                    let container = if bits == 32 {
                        format!("__m{width}")
                    } else {
                        format!("__m{width}d")
                    };
                    let scalar = if bits == 32 { "uint32_t" } else { "uint64_t" };
                    let name = format!("consumer_{mask_kind}_{family}_{suffix}_{width}");
                    let args = match mask_kind {
                        "mask" => "a,k,b,c",
                        "mask3" => "a,b,c,k",
                        _ => "k,a,b,c",
                    };
                    let body = format!(
                        "{container} a=*(const {container}*)ap,b=*(const {container}*)bp,c=*(const {container}*)cp;{prefix}_storeu_{suffix}(out,{prefix}_{mask_kind}_{family}_{suffix}({args}));"
                    );
                    source += &format!(
                        "__attribute__((target(\"avx512vl\"))) void {name}(const void*ap,const void*bp,const void*cp,void*out,__mmask8 k){{{body}}}\n"
                    );
                    oracle += &format!(
                        "extern void {name}(const void*,const void*,const void*,void*,uint8_t);\n__attribute__((noinline)) static void oracle_{name}(const void*ap,const void*bp,const void*cp,void*out,__mmask8 k){{{body}}}\n"
                    );
                    let edges = if bits == 32 { "edges32" } else { "edges64" };
                    checks += &format!(
                        "{{{scalar} a[{lanes}] __attribute__((aligned(32))),b[{lanes}] __attribute__((aligned(32))),c[{lanes}] __attribute__((aligned(32)));unsigned char out[64],want[64];uint8_t masks[]={{0,1,0x55,0xaa,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<8;sample++){{for(unsigned lane=0;lane<{lanes};lane++){{a[lane]={edges}[(lane+sample)%16];b[lane]={edges}[(lane+sample+5)%16];c[lane]={edges}[(lane+sample+11)%16];}}for(unsigned rounding=0;rounding<4;rounding++)for(unsigned denormal=0;denormal<4;denormal++)for(unsigned mi=0;mi<6;mi++){{unsigned csr=0x1f80|(rounding<<13)|((denormal&1)<<6)|((denormal>>1)<<15);memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);_mm_setcsr(csr);oracle_{name}(a,b,c,want,masks[mi]);unsigned expected=_mm_getcsr();_mm_setcsr(csr);{name}(a,b,c,out,masks[mi]);unsigned actual=_mm_getcsr();if(actual!=expected||memcmp(out,want,sizeof out)){{fprintf(stderr,\"{name} sample=%u round=%u denormal=%u mask=%x csr=%x expected=%x\\n\",sample,rounding,denormal,masks[mi],actual,expected);return 1;}}}}_mm_setcsr(0x1f00);{name}(a,b,c,out,0);if(_mm_getcsr()!=0x1f00)return 2;}}}}\n"
                    );
                }
            }
        }
    }
    oracle += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint32_t edges32[16]={{0x3f800001,0x3f7ffffe,0xbf800000,0x00800000,1,0x7f7fffff,0x80000000,0,0x7f800000,0xff800000,0x7fc12345,0x7f800001,0x3f000000,0xbf000000,0x40000000,0xc0000000}};uint64_t edges64[16]={{UINT64_C(0x3ff0000000000001),UINT64_C(0x3feffffffffffffe),UINT64_C(0xbff0000000000000),UINT64_C(0x0010000000000000),1,UINT64_C(0x7fefffffffffffff),UINT64_C(0x8000000000000000),0,UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0000000000001),UINT64_C(0x3fe0000000000000),UINT64_C(0xbfe0000000000000),UINT64_C(0x4000000000000000),UINT64_C(0xc000000000000000)}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.fma && !features.avx512dq);
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
    let directory = std::env::temp_dir().join(format!("keel-c-vl-fma-{}", std::process::id()));
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
                eprintln!("SKIP native C VL FMA: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-mavx512f", "-mavx512vl", "-frounding-math"])
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
