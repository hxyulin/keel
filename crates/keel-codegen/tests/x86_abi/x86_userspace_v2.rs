//! Strict v2 joins cumulative vector/ABI coverage and representative v2 operations.
//! Native execution is finite userspace evidence, not physical FLAGS certification.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use object::{Object, ObjectSymbol};
use std::fs;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::process::Command;

fn representative() -> String {
    let mut ir = String::new();
    for (name, ty, op) in [
        ("horizontal", "f32x4", "x86_sse3_haddps"),
        ("shuffle", "i8x16", "x86_ssse3_pshufb128"),
        ("multiply", "i32x4", "x86_sse41_pmuldq128"),
        ("compare", "i64x2", "vicmp.sgt"),
    ] {
        let expr = if op.starts_with("x86_") {
            format!("intrinsic {op}(v3,v4)")
        } else {
            format!("{op} v3,v4")
        };
        // Keep both inputs live after the operation, and allow output/input aliasing.
        ir += &format!(
            "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{ty} align 1 v0\n v4 = load.{ty} align 1 v1\n v5 = {expr}\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n"
        );
    }
    ir
}

use crate::userspace;
fn compile(source: &str, s: SelectionTier, a: AllocationTier, spilled: bool) -> (Vec<u8>, String) {
    userspace::compile(source, CpuLevel::V2, s, a, spilled)
}

#[test]
fn strict_v2_cumulative_vectors_abi_and_representative_operations() {
    let export = std::env::var_os("KEEL_V2_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
    }
    let (abi, abi_c) = common::abi_fixture(false);
    let source = common::fixture() + &abi;
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (ops, text) = compile(&representative(), s, a, spilled);
                for opcode in ["haddps", "pshufb", "pmuldq"] {
                    assert!(text.contains(opcode), "{text}");
                }
                assert!(
                    object::File::parse(&*ops)
                        .unwrap()
                        .symbols()
                        .all(|sym| !sym.is_undefined() || sym.name().unwrap().is_empty())
                );
                let (abi_object, abi_text) = compile(&source, s, a, spilled);
                // `source` already compiles the vector corpus. The separate
                // vector object only feeds the export and the native harness.
                let vectors = (export.is_some()
                    || cfg!(all(target_os = "linux", target_arch = "x86_64")))
                .then(|| compile(&common::fixture(), s, a, spilled));
                if let Some(dir) = &export {
                    let (vectors, vector_text) = vectors.as_ref().unwrap();
                    for (kind, object, assembly) in [
                        ("ops", &ops, &text),
                        ("abi", &abi_object, &abi_text),
                        ("vectors", vectors, vector_text),
                    ] {
                        let stem = format!("{kind}-{s:?}-{a:?}-{spilled}");
                        fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                        fs::write(dir.join(format!("{stem}.s")), assembly).unwrap();
                    }
                }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                {
                    let name = format!("{s:?}-{a:?}-{spilled}");
                    execute(&format!("ops-{name}"), &ops, FOOTPRINTS);
                    let (vectors, _) = vectors.as_ref().unwrap();
                    execute(&format!("vectors-{name}"), vectors, &common::harness());
                    execute(&format!("abi-{name}"), &abi_object, &abi_c);
                }
                #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
                let _ = (vectors, abi_object, &abi_c);
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute(name: &str, object: &[u8], c: &str) {
    let dir = common::temp(&format!("v2-{name}"));
    fs::write(dir.join("test.o"), object).unwrap();
    fs::write(dir.join("test.c"), c).unwrap();
    common::command(
        Command::new("cc")
            .args(["-O2", "-march=x86-64-v2", "-ffp-contract=off"])
            .arg(dir.join("test.c"))
            .arg(dir.join("test.o"))
            .arg("-lm")
            .arg("-o")
            .arg(dir.join("test")),
    );
    common::command(&mut Command::new(dir.join("test")));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const FOOTPRINTS: &str = r#"
#define _GNU_SOURCE
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern void horizontal(void*,void*,void*),shuffle(void*,void*,void*),multiply(void*,void*,void*),compare(void*,void*,void*);
int main(void) {
 long page=sysconf(_SC_PAGESIZE); if(page<64)return 1;
 unsigned char *m[3];
 for(unsigned i=0;i<3;i++){m[i]=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m[i]==MAP_FAILED||mprotect(m[i]+page,page,PROT_READ|PROT_WRITE))return 2;}
 void (*ops[])(void*,void*,void*)={horizontal,shuffle,multiply,compare};
 for(unsigned op=0;op<4;op++)for(unsigned edge=0;edge<2;edge++)for(unsigned alias=0;alias<3;alias++)for(unsigned n=0;n<256;n++){
  unsigned char *a=m[0]+page+(edge?page-16:0),*b=m[1]+page+(edge?page-16:0),*o=m[2]+page+(edge?page-48:0),e[48];
  // In-place cases need 48 writable bytes; guard-page reads remain tested separately.
  if(alias==1){a=m[0]+page+1;o=a;}if(alias==2){b=m[1]+page+1;o=b;}
  for(unsigned j=0;j<16;j++){a[j]=(unsigned char)(n+j*19);b[j]=(unsigned char)(n*7+j*11);}
  if(op==0){float x[4]={(float)n,-1.5f,2.25f,-3},y[4]={1,2,-(float)n,4};memcpy(a,x,16);memcpy(b,y,16);float z[4]={x[0]+x[1],x[2]+x[3],y[0]+y[1],y[2]+y[3]};memcpy(e,z,16);}
  if(op==1)for(unsigned j=0;j<16;j++)e[j]=(b[j]&128)?0:a[b[j]&15];
  if(op==2)for(unsigned j=0;j<2;j++){int32_t x,y;int64_t z;memcpy(&x,a+8*j,4);memcpy(&y,b+8*j,4);z=(int64_t)x*y;memcpy(e+8*j,&z,8);}
  if(op==3)for(unsigned j=0;j<2;j++){int64_t x,y;uint64_t z;memcpy(&x,a+8*j,8);memcpy(&y,b+8*j,8);z=x>y?UINT64_MAX:0;memcpy(e+8*j,&z,8);}
  memcpy(e+16,a,16);memcpy(e+32,b,16);
  if(o>m[alias?alias-1:2]+page)o[-1]=0xa5;
  if(o+48<m[alias?alias-1:2]+2*page)o[48]=0xa5;
  ops[op](a,b,o);if(memcmp(o,e,48))return 10+op;
  if(o>m[alias?alias-1:2]+page&&o[-1]!=0xa5)return 20;
  if(o+48<m[alias?alias-1:2]+2*page&&o[48]!=0xa5)return 21;
 }
 for(unsigned i=0;i<3;i++)munmap(m[i],3*page);
 return 0;
}
"#;

#[test]
fn long_mode_lahf_sahf_low_status_bits() {
    // A self-contained instruction sequence with scalar SysV input/output.
    // This promises no ambient FLAGS value across a call or an IR boundary.
    let assembly = ".intel_syntax noprefix\n.text\n.globl status_bits\nstatus_bits:\n mov eax,edi\n shl eax,8\n sahf\n lahf\n movzx eax,ah\n ret\n";
    assert!(
        keel_asm::x86::assemble_object_with_features(assembly, Features::for_level(CpuLevel::V1))
            .is_err()
    );
    let object =
        keel_asm::x86::assemble_object_with_features(assembly, Features::for_level(CpuLevel::V2))
            .unwrap();
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    execute(
        "status-bits",
        &object,
        "extern unsigned status_bits(unsigned); int main(void){for(unsigned i=0;i<256;i++)if(status_bits(i)!=((i&0xd5)|2))return 1;return 0;}",
    );
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    let _ = object;
}
