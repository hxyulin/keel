#[path = "x86_aggregate_boundary/x86_aggregate_branch.rs"]
mod x86_aggregate_branch;
#[path = "x86_aggregate_boundary/x86_sret_address.rs"]
mod x86_sret_address;

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
const SOURCE: &str = include_str!("fixtures/aggregate-call-wrapper.keel");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn scalarized_wrapper_keeps_native_object_parity_and_the_external_c_abi() {
    let shifts = ["shl", "ushr", "sshr"]
        .map(|op| {
            format!(
                "
func @{op}64(i128) -> i128 cc(c) {{
block0(v0: i128):
 v1 = iconst.i128 64
 v2 = {op} v0, v1
 return v2
}}
"
            )
        })
        .join("");
    let definition = SOURCE.split("func @JS_Call(").nth(1).unwrap();
    let modified = format!("func @JS_Call_mut({definition}").replace(
        "    call @JS_CallInternal",
        "    store v31, v7\n    call @JS_CallInternal",
    );
    let alias = format!("func @JS_Call_alias({definition}")
        .replace("v6 = stack_addr ss4", "v6 = stack_addr ss5");
    let module = keel_ir::parse(&format!("{SOURCE}{modified}{alias}{shifts}")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
                #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                execute(&bytes);
            }
        }
    }
}
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn execute(bytes: &[u8]) {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-aggregate-boundary-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("wrapper.o"), bytes).unwrap();
    fs::write(dir.join("oracle.c"),r#"
#include <stdint.h>
#include <stdlib.h>
typedef struct { uint64_t lo,hi; } P;
extern P JS_Call(void *,P,P,int,P *), JS_Call_mut(void *,P,P,int,P *), JS_Call_alias(void *,P,P,int,P *);
typedef unsigned __int128 U;
extern U shl64(U), ushr64(U);
extern __int128 sshr64(__int128);
P JS_CallInternal(void *ctx,P fn,P self,P target,int argc,P *argv,int flags) {
 if(target.lo || target.hi!=3 || flags!=2) abort();
 return (P){fn.lo^self.hi^argv[0].lo^(uint32_t)argc,
            fn.hi+self.lo+argv[0].hi+flags+(uintptr_t)ctx};
}
int main(void) {
 uint64_t x=0xfedcba9876543210ULL;
 const uint64_t edges[]={0,1,UINT64_MAX,UINT64_C(1)<<63,(UINT64_C(1)<<63)-1};
 for(unsigned i=0;i<512;i++) {
  x=x*6364136223846793005ULL+1;
  if(i<sizeof(edges)/sizeof(edges[0])) x=edges[i];
  U wide=((U)x<<64)|~x;
  if(shl64(wide)!=(wide<<64) || ushr64(wide)!=(wide>>64)
     || sshr64((__int128)wide)!=(__int128)(int64_t)x) abort();
  P a={x,~x},b={x>>11,x<<29},v={~(x<<5),x>>7};
  void *ctx=(void *)(uintptr_t)(i+17); int argc=(int)(x>>32);
  P p=JS_Call(ctx,a,b,argc,&v);
  P q=JS_CallInternal(ctx,a,b,(P){0,3},argc,&v,2);
  if(p.lo!=q.lo || p.hi!=q.hi) abort();
  p=JS_Call_alias(ctx,a,b,argc,&v);
  if(p.lo!=q.lo || p.hi!=q.hi) abort();
  p=JS_Call_mut(ctx,a,b,argc,&v);
  q=JS_CallInternal(ctx,(P){3,a.hi},b,(P){0,3},argc,&v,2);
  if(p.lo!=q.lo || p.hi!=q.hi) abort();
 }
 return 0;
}
"#).unwrap();
    let result = Command::new("clang")
        .args(["-O2", "oracle.c", "wrapper.o", "-o", "oracle"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(Command::new(dir.join("oracle")).status().unwrap().success());
    fs::remove_dir_all(dir).unwrap();
}

#[path = "x86_aggregate_boundary/x86_zmm_wrappers.rs"]
mod x86_zmm_wrappers;
