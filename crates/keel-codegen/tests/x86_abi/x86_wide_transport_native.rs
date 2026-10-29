//! Width, realigned-frame and parallel-copy oracles independent of the wide ABI.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::from("decl @clobber(ptr)\n");
    for shape in keel_ir::VectorType::ALL_256 {
        s += &format!(
            r#"
func @transport_{shape}(ptr, ptr, i64, i64) -> i64 {{
 ss0 = stack_slot 64, align 32
block0(v0: ptr, v1: ptr, v2: i64, v3: i64):
 v4 = stack_addr ss0
 v5 = iconst.i64 123456789
 store v5, v4
 v6 = iconst.i64 32
 v7 = ptradd v0, v6
 v8 = load.{shape} align 1 v0
 v9 = load.{shape} align 1 v7
 v10 = load.i64x2 align 1 v0
 jump block1(v2, v8, v9)
block1(v11: i64, v12: {shape}, v13: {shape}):
 v14 = iconst.i64 0
 v15 = icmp.eq v11, v14
 brif v15, block3, block2
block2:
 v16 = iconst.i64 1
 v17 = isub v11, v16
 jump block1(v17, v13, v12)
block3:
 v18 = intrinsic stack_alloc(v3)
 store align 1 v12, v18
 call @clobber(v18)
 v19 = load.{shape} align 1 v18
 store align 1 v19, v1
 v20 = ptradd v1, v6
 store align 1 v10, v20
 v21 = load.i64 v4
 return v21
}}
"#
        );
    }
    s
}
fn object(s: SelectionTier, a: AllocationTier, empty: bool) -> (Vec<u8>, String) {
    let mut module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut module).unwrap();
    }
    let features = Features::for_level(CpuLevel::V3);
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
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let direct = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        direct,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (direct, text)
}
fn harness() -> String {
    let mut c = String::from(
        r#"
#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <unwind.h>
#include <dlfcn.h>
static volatile unsigned sink;
static uintptr_t active;
static _Unwind_Reason_Code visit(struct _Unwind_Context *context, void *data) {
 Dl_info info; uintptr_t ip=(uintptr_t)_Unwind_GetIP(context);
 if(ip && dladdr((void*)(ip-1),&info) && (uintptr_t)info.dli_saddr==active)
  *(int*)data=1;
 return _URC_NO_REASON;
}
__attribute__((noinline)) void clobber(void *p) {
 if((uintptr_t)p%32)abort();
 sink += ((unsigned char*)p)[0]+((unsigned char*)p)[31];
 int seen=0; _Unwind_Backtrace(visit,&seen);if(!seen)abort();
 __asm__ volatile("vzeroall":::"xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7","xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15");
}
typedef uint64_t(*fn)(const void*,void*,uint64_t,uint64_t);
"#,
    );
    for shape in keel_ir::VectorType::ALL_256 {
        c += &format!("extern uint64_t transport_{shape}(const void*,void*,uint64_t,uint64_t);\n");
    }
    c += "int main(void){fn cases[]={";
    for shape in keel_ir::VectorType::ALL_256 {
        c += &format!("transport_{shape},");
    }
    c += r#"};
 size_t page=(size_t)sysconf(_SC_PAGESIZE);
 unsigned char *src=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 unsigned char *dst=mmap(0,page*2,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(src==MAP_FAILED || dst==MAP_FAILED)return 10;
 if(mprotect(src+page,page,PROT_NONE)||mprotect(dst+page,page,PROT_NONE))return 11;
 for(unsigned f=0;f<sizeof(cases)/sizeof(*cases);f++)for(unsigned offset=0;offset<16;offset++)for(unsigned n=0;n<8;n++){
  unsigned char *in=src+page-64-offset,*out=dst+page-48-offset;
  for(unsigned i=0;i<64;i++)in[i]=(unsigned char)(i*37+n*11+f*19);
  memset(out,0xcc,48);
  active=(uintptr_t)cases[f];
  uint64_t result=cases[f](in,out,n,32+17*n);
  if(result!=123456789 || memcmp(out,in+(n%2)*32,32) || memcmp(out+32,in,16))return 12;
 }
 munmap(src,page*2);munmap(dst,page*2);return 0;
}
"#;
    c
}
#[test]
fn wide_transport_cycles_dynamic_frames_and_mixed_spills_emit() {
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                object(s, a, empty);
            }
        }
    }
    // Catch harness syntax problems on the non-native development host too.
    let path = std::env::temp_dir().join(format!("keel-wide-harness-{}.c", std::process::id()));
    fs::write(&path, harness()).unwrap();
    let r = Command::new("clang")
        .args([
            "-target",
            if cfg!(target_os = "macos") {
                "x86_64-apple-macos11.0.0"
            } else {
                "x86_64-unknown-linux-gnu"
            },
            "-fsyntax-only",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    fs::remove_file(path).unwrap();
}
#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn wide_transport_cycles_dynamic_frames_and_mixed_spills_execute() {
    assert!(
        std::is_x86_feature_detected!("avx2"),
        "native wide gate requires AVX2 and OS vector state"
    );
    let dir = std::env::temp_dir().join(format!("keel-wide-native-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("test.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                fs::write(dir.join("test.o"), object(s, a, empty).0).unwrap();
                let c = Command::new("clang")
                    .args(["-O2", "-mavx2", "-rdynamic"])
                    .arg(dir.join("test.c"))
                    .arg(dir.join("test.o"))
                    .arg("-ldl")
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
                assert!(
                    Command::new(dir.join("test")).status().unwrap().success(),
                    "{s:?}/{a:?}/empty={empty}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
