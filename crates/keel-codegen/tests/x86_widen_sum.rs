//! Finite signed-load reduction pilot: legality/parity everywhere, native execution on Linux x86.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, vector_capabilities},
};
const SOURCE: &str = include_str!("../../keel-opt/tests/fixtures/signed_widen_sum.ir");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
#[test]
fn signed_sum_all_codegen_paths_features_allocators_and_spills() {
    for features in [
        Features::for_level(CpuLevel::V1),
        Features::for_level(CpuLevel::V2),
        Features::for_level(CpuLevel::V3),
        Features::for_level(CpuLevel::V3)
            .with_features("-avx2")
            .unwrap(),
    ] {
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for spilled in [false, true] {
                    let selector = FeatureSelector {
                        inner: &selection,
                        features,
                    };
                    let context = keel_opt::OptimizationContext {
                        target: keel_opt::VectorTarget::X86Sse2,
                        capabilities: vector_capabilities(Target::X86_64LinuxGnu, &selector)
                            .unwrap()
                            .with_wide_preference(true),
                    };
                    let mut module = keel_ir::parse(SOURCE).unwrap();
                    keel_opt::run_passes_with_context_and_options(
                        &mut module,
                        keel_opt::DEFAULT_PIPELINE,
                        context,
                        keel_opt::OptimizeOptions::default(),
                    )
                    .unwrap();
                    assert_eq!(module.to_string().contains("i64x4"), features.avx2);
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocation,
                    };
                    let pools = if spilled {
                        &EMPTY
                    } else {
                        &keel_codegen::x86_64::REGISTERS
                    };
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    let typed = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    let mut chunks = keel_asm_ir::Program::default();
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |p| chunks.items.extend(p.items),
                    )
                    .unwrap();
                    for p in [&typed, &chunks] {
                        for item in &p.items {
                            if let keel_asm_ir::Item::X86Instruction(i) = item {
                                assert!(features.contains(i.required_features()), "{i}");
                            }
                        }
                    }
                    let object =
                        keel_asm::x86::assemble_program_object_with_features(&typed, features)
                            .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_program_object_with_features(&chunks, features)
                            .unwrap()
                    );
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                    if !features.avx || std::is_x86_feature_detected!("avx2") {
                        execute(&object);
                    }
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute(object: &[u8]) {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-wide-native-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("sum.o"), object).unwrap();
    fs::write(dir.join("test.c"), HARNESS).unwrap();
    let build = Command::new("cc")
        .args(["-O2", "-fno-strict-aliasing"])
        .arg(dir.join("test.c"))
        .arg(dir.join("sum.o"))
        .arg("-o")
        .arg(dir.join("test"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(Command::new(dir.join("test")).status().unwrap().success());
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdio.h>
extern uint64_t widen(const void*,int64_t,int64_t,uint64_t);
int main(void){
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *m=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(m==MAP_FAILED||mprotect(m+page,page,PROT_READ|PROT_WRITE))return 2;
 int32_t values[]={INT32_MIN,INT32_MAX,-1,0,1,-1234567,987654321};
 for(unsigned n=0;n<=81;n++)for(unsigned start=0;start<=5;start++)for(unsigned edge=0;edge<3;edge++){
  unsigned char *p=edge==0?m+page:edge==1?m+2*page-(n+start)*4:m+page+1;
  uint64_t expected=UINT64_C(0xfedcba9876543210);
  for(unsigned i=0;i<n+start;i++){int32_t x=values[i%7];memcpy(p+i*4,&x,4);if(i>=start)expected+=(int64_t)x;}
  uint64_t got=widen(p,start,start+n,UINT64_C(0xfedcba9876543210));
  if(got!=expected){fprintf(stderr,"n=%u start=%u edge=%u\n",n,start,edge);return 3;}
 }
 // No read on reversed or negative-bound ranges, including inaccessible pointers.
 if(widen(m,17,3,123)!=123||widen(m,0,-1,456)!=456)return 4;
 // A negative start falls back to the scalar loop and remains a valid address.
 int32_t x=-31;memcpy(m+page,&x,4);
 if(widen(m+page+4,-1,0,100)!=69)return 5;
 // Guard arithmetic overflows, but the scalar one-element effective address is valid.
 uint64_t start=UINT64_C(1)<<62;
 if(widen(m+page,(int64_t)start,(int64_t)(start+1),100)!=69)return 6;
 return 0;
}
"#;
