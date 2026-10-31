//! Mixed XMM/YMM/ZMM calls share one argument bank and preserve wide copy cycles.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::EntityRef;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct PinTailCycle {
    inner: AllocationTier,
    pin: bool,
}
impl RegisterAllocator for PinTailCycle {
    fn name(&self) -> &'static str {
        "mixed-simd-tail-cycle"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        if !self.pin || !input.func.name.contains("tail") {
            return self.inner.allocate(input);
        }
        let empty = AllocationInput {
            pools: &EMPTY,
            ..*input
        };
        let mut allocation = self.inner.allocate(&empty);
        let params = input.func.block_params(input.func.entry_block().unwrap());
        for (index, &value) in params.iter().take(3).enumerate() {
            assert_eq!(
                input.func.value_type(value).bytes(),
                Some([64, 16, 32][index])
            );
            allocation.locs[0][value.index()] = Some(Loc::FReg(index as u8));
        }
        // Incoming ZMM0/XMM1/YMM2 must become outgoing ZMM2/XMM0/YMM1. No value is live
        // after a tail call, so these pinned homes do not evade call spilling.
        assert_eq!(allocation.loc(params[0]), Some(Loc::FReg(0)));
        assert_eq!(allocation.loc(params[1]), Some(Loc::FReg(1)));
        assert_eq!(allocation.loc(params[2]), Some(Loc::FReg(2)));
        allocation
    }
}
fn source() -> String {
    let mut source = String::from(
        "decl @mixed_external(i32x4,i32x8,i32x16)->i32x16 target_features(\"avx512f\")\n",
    );
    for indirect in [false, true] {
        let name = if indirect {
            "mixed_indirect_tail"
        } else {
            "mixed_tail"
        };
        let extra = if indirect { ",ptr" } else { "" };
        let param = if indirect { ",v3:ptr" } else { "" };
        let call = if indirect {
            "return_call_indirect v3(v1,v2,v0):(i32x4,i32x8,i32x16)->i32x16"
        } else {
            "return_call @mixed_external(v1,v2,v0)"
        };
        source += &format!(
            "func @{name}(i32x16,i32x4,i32x8{extra})->i32x16 target_features(\"avx512f\") {{\nentry(v0:i32x16,v1:i32x4,v2:i32x8{param}):\n {call}\n}}\n"
        );
        let name = if indirect {
            "mixed_indirect"
        } else {
            "mixed_direct"
        };
        let extra = if indirect { ",ptr" } else { "" };
        let param = if indirect { ",v4:ptr" } else { "" };
        let call = if indirect {
            "call_indirect v4(v5,v6,v7):(i32x4,i32x8,i32x16)->i32x16"
        } else {
            "call @mixed_external(v5,v6,v7)"
        };
        source += &format!(
            "func @{name}(ptr,ptr,ptr,ptr{extra}) target_features(\"avx512f\") {{\nentry(v0:ptr,v1:ptr,v2:ptr,v3:ptr{param}):\n v5=load.i32x4 align 1 v0\n v6=load.i32x8 align 1 v1\n v7=load.i32x16 align 1 v2\n v8={call}\n store align 1 v8,v3\n v9=iconst.i64 64\n v10=ptradd v3,v9\n store align 1 v5,v10\n v11=iconst.i64 96\n v12=ptradd v3,v11\n store align 1 v6,v12\n v13=iconst.i64 128\n v14=ptradd v3,v13\n store align 1 v7,v14\n return\n}}\n"
        );
    }
    source
}
const RUNNER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef unsigned char X __attribute__((vector_size(16)));
typedef unsigned char Y __attribute__((vector_size(32)));
typedef unsigned char Z __attribute__((vector_size(64)));
typedef Z(*Target)(X,Y,Z);
extern void mixed_direct(const void*,const void*,const void*,void*),mixed_indirect(const void*,const void*,const void*,void*,Target);
extern Z mixed_tail(Z,X,Y),mixed_indirect_tail(Z,X,Y,Target);
__attribute__((noinline)) Z mixed_external(X x,Y y,Z z){
 unsigned char a[16],b[32],c[64],out[64];memcpy(a,&x,16);memcpy(b,&y,32);memcpy(c,&z,64);
 for(unsigned i=0;i<64;i++)out[i]=c[i]^a[(i*3)%16]^b[(i*5)%32];
 memcpy(&z,out,64);return z;
}
int main(void){
 unsigned char a[18],b[34],c[66],expected[256],actual[256];
 for(unsigned sample=0;sample<37;sample++){
  for(unsigned i=0;i<sizeof a;i++)a[i]=(unsigned char)(sample*19+i*31);
  for(unsigned i=0;i<sizeof b;i++)b[i]=(unsigned char)(sample*29+i*67);
  for(unsigned i=0;i<sizeof c;i++)c[i]=(unsigned char)(sample*41+i*13);
  memset(expected,0xa5,sizeof expected);for(unsigned i=0;i<64;i++)expected[i]=c[1+i]^a[1+(i*3)%16]^b[1+(i*5)%32];memcpy(expected+64,a+1,16);memcpy(expected+96,b+1,32);memcpy(expected+128,c+1,64);
  for(unsigned indirect=0;indirect<2;indirect++){
   memset(actual,0xa5,sizeof actual);if(indirect)mixed_indirect(a+1,b+1,c+1,actual,mixed_external);else mixed_direct(a+1,b+1,c+1,actual);
   if(memcmp(actual,expected,sizeof actual)){fprintf(stderr,"ordinary mixed call %u sample%u\n",indirect,sample);return 1;}
   X x;Y y;Z z;memcpy(&x,a+1,16);memcpy(&y,b+1,32);memcpy(&z,c+1,64);Z result=indirect?mixed_indirect_tail(z,x,y,mixed_external):mixed_tail(z,x,y);
   if(memcmp(&result,expected,64)){fprintf(stderr,"tail mixed call %u sample%u\n",indirect,sample);return 2;}
  }
 }
 return 0;
}
"#;
#[test]
fn mixed_simd_external_calls_preserve_originals_and_tail_copy_cycles() {
    let original = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.avx2);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "keel-mixed-simd-abi-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), RUNNER).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512f");
    if native {
        let result = Command::new("clang")
            .args(["-O2", "-mavx512f", "-c"])
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for inner in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            for optimize in [false, true] {
                let mut module = original.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let allocator = PinTailCycle {
                    inner,
                    pin: !pools.float_caller_saved.is_empty(),
                };
                let tier = if optimize {
                    SelectionTier::Full
                } else {
                    SelectionTier::Baseline
                };
                let selector = FeatureSelector {
                    inner: &tier,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                if allocator.pin {
                    for name in ["mixed_tail", "mixed_indirect_tail"] {
                        let body = text
                            .split(&format!("\"{name}\":\n"))
                            .nth(1)
                            .unwrap()
                            .split(".size")
                            .next()
                            .unwrap();
                        assert!(
                            body.lines().any(|line| line
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .contains("vmovups zmm14, zmm0")),
                            "no wide copy-cycle temporary in {name}: {body}"
                        );
                    }
                }
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                );
                fs::write(dir.join("mixed.o"), object).unwrap();
                if native {
                    let result = Command::new("clang")
                        .arg(dir.join("runner.o"))
                        .arg(dir.join("mixed.o"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let result = Command::new(dir.join("run")).output().unwrap();
                    assert!(
                        result.status.success(),
                        "{inner:?} opt{optimize} pinned{}: {} {}",
                        allocator.pin,
                        result.status,
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
            }
        }
    }
}
