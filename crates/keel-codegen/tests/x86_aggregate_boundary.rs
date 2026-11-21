#[path = "x86_aggregate_boundary/x86_aggregate_branch.rs"]
mod x86_aggregate_branch;
#[path = "x86_aggregate_boundary/x86_sret_address.rs"]
mod x86_sret_address;

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
const SOURCE: &str = include_str!("fixtures/aggregate-call-wrapper.keel");
const BYVAL_INPUTS: &str = r#"
func @read_byval_registers(ptr byval [2 x i64], i64, ptr byval [2 x i64], i64, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64, v2: ptr, v3: i64, v4: i64):
 v5 = load.i64 v0
 v6 = iconst.i64 8
 v7 = ptradd v2, v6
 v8 = load.i32 v7
 v9 = uext.i64 v8
 v10 = ptradd v0, v6
 v11 = load.ptr v10
 v12 = ptrtoint.i64 v11
 v13 = xor v5, v9
 v14 = xor v13, v12
 v15 = iadd v14, v1
 v16 = iadd v15, v3
 v17 = iadd v16, v4
 return v17
}
func @read_byval_rolled(i64, i64, i64, i64, i64, ptr byval [2 x i64], i64) -> i64 cc(c) {
block0(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64, v5: ptr, v6: i64):
 v7 = load.i64 v5
 v8 = xor v7, v6
 return v8
}
func @read_byval_word(ptr byval {i32, i32}, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 4
 v3 = ptradd v0, v2
 v4 = load.i32 align 4 v3
 v5 = uext.i64 v4
 v6 = xor v5, v1
 return v6
}
func @write_byval_word(ptr byval {i64}, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 v3 = iadd v1, v2
 store v3, v0
 v4 = load.i64 volatile v0
 return v4
}
"#;
const OUTGOING_PAIRS: &str = r#"
decl @take_outgoing(i64, ptr byval [2 x i64], ptr byval [2 x i64], ptr byval [2 x i64], i64, i64) -> i64 cc(c)
decl @replace_outgoing(ptr, i64) cc(c)
func @outgoing_pair_moves(i64, i64) -> i64 cc(c) {
 ss0 = stack_slot 16, align 8
 ss1 = stack_slot 16, align 8
 ss2 = stack_slot 16, align 8
block0(v0: i64, v1: i64):
 jump block1
block1:
 v2 = stack_addr ss0
 v3 = stack_addr ss1
 v4 = stack_addr ss2
 v5 = iconst.i64 8
 v6 = ptradd v2, v5
 v7 = ptradd v3, v5
 v8 = ptradd v4, v5
 v9 = iadd v0, v1
 v10 = xor v0, v1
 store v0, v2
 store v1, v6
 store v1, v3
 store v9, v7
 store v0, v4
 store v10, v8
 v11 = call @take_outgoing(v9, v2, v3, v4, v10, v1)
 return v11
}
"#;
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
    let escaped = OUTGOING_PAIRS
        .split_once("func @outgoing_pair_moves(")
        .unwrap()
        .1
        .replace(
            " v11 = call",
            " call @replace_outgoing(v4, v9)\n v11 = call",
        );
    let module = keel_ir::parse(&format!(
        "{SOURCE}{modified}{alias}{shifts}{BYVAL_INPUTS}{OUTGOING_PAIRS}func @outgoing_escapes({escaped}"
    ))
    .unwrap();
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
                let (_, word) = text.split_once("\n\"read_byval_word\":").unwrap();
                let (word, written) = word.split_once("\n\"write_byval_word\":").unwrap();
                let (written, _) = written.split_once("\n\"outgoing_pair_moves\":").unwrap();
                if std::ptr::eq(pools, &keel_codegen::x86_64::REGISTERS) {
                    assert!(!word.contains("[rbp + -"), "{word}");
                }
                assert!(written.contains("[rbp + -"), "{written}");
                let outgoing = text.split_once("\n\"outgoing_pair_moves\":").unwrap().1;
                let (outgoing, escaped) = outgoing.split_once("\n\"outgoing_escapes\":").unwrap();
                assert!(!outgoing.contains("stack_addr"), "{outgoing}");
                assert!(escaped.contains("stack_addr"), "{escaped}");
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
extern uint64_t read_byval_registers(P,uint64_t,P,uint64_t,uint64_t);
extern uint64_t read_byval_rolled(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,P,uint64_t);
typedef struct { uint32_t lo,hi; } W;
typedef struct { uint64_t value; } Word;
extern uint64_t read_byval_word(W,uint64_t), write_byval_word(Word,uint64_t);
typedef unsigned __int128 U;
extern U shl64(U), ushr64(U);
extern __int128 sshr64(__int128);
extern uint64_t outgoing_pair_moves(uint64_t,uint64_t), outgoing_escapes(uint64_t,uint64_t);
uint64_t take_outgoing(uint64_t first,P a,P b,P c,uint64_t last_reg,uint64_t last_stack) {
 return first + 3*a.lo + 5*a.hi + 7*b.lo + 11*b.hi + 13*c.lo + 17*c.hi +
        19*last_reg + 23*last_stack;
}
void replace_outgoing(P *p,uint64_t lo) { p->lo=lo; }
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
  if(read_byval_registers(a,b.lo,b,v.lo,v.hi)!=(a.lo^(uint32_t)b.hi^a.hi)+b.lo+v.lo+v.hi)
      abort();
  if(read_byval_rolled(x,x,x,x,x,a,b.lo)!=(a.lo^b.lo)) abort();
  if(read_byval_word((W){(uint32_t)x,(uint32_t)(x>>32)},b.lo)!=((x>>32)^b.lo)) abort();
  if(write_byval_word((Word){x},b.lo)!=x+b.lo) abort();
  uint64_t y=b.lo, sum=x+y, bits=x^y;
  if(outgoing_pair_moves(x,y)!=take_outgoing(sum,(P){x,y},(P){y,sum},(P){x,bits},bits,y))
      abort();
  if(outgoing_escapes(x,y)!=take_outgoing(sum,(P){x,y},(P){y,sum},(P){sum,bits},bits,y))
      abort();
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

#[path = "x86_aggregate_boundary/x86_mixed_simd_abi.rs"]
mod x86_mixed_simd_abi;
