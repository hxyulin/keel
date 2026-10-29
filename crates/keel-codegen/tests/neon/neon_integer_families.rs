//! Independent extended-integer oracle, including FPSR.QC preservation.
use crate::common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::VectorType;
use std::{fs, process::Command};
const OPS: [&str; 16] = [
    "vsmin",
    "vsmax",
    "vumin",
    "vumax",
    "vsabd",
    "vuabd",
    "vshadd",
    "vuhadd",
    "vsrhadd",
    "vurhadd",
    "vshsub",
    "vuhsub",
    "vsadd_sat",
    "vuadd_sat",
    "vssub_sat",
    "vusub_sat",
];
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        r#"
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
static __int128 half(__int128 x){return x/2-(x<0&&x%2!=0);}
static uint64_t oracle(unsigned op,uint64_t x,uint64_t y,unsigned w){
 __int128 modulus=(__int128)1<<w, sign=modulus/2;
 __int128 a=x>=sign?(__int128)x-modulus:x,b=y>=sign?(__int128)y-modulus:y,r=0;
 switch(op){
 case 0:r=a<b?a:b;break; case 1:r=a>b?a:b;break;
 case 2:r=x<y?x:y;break;case 3:r=x>y?x:y;break;
 case 4:r=a<b?b-a:a-b;break;case 5:r=x<y?(__int128)y-x:(__int128)x-y;break;
 case 6:r=half(a+b);break;case 7:r=half((__int128)x+y);break;
 case 8:r=half(a+b+1);break;case 9:r=half((__int128)x+y+1);break;
 case 10:r=half(a-b);break;case 11:r=half((__int128)x-y);break;
 case 12:case 14:r=op==12?a+b:a-b;if(r < -sign)r=-sign;if(r>=sign)r=sign-1;break;
 case 13:case 15:r=op==13?(__int128)x+y:(__int128)x-y;if(r<0)r=0;if(r>=modulus)r=modulus-1;break;
 }return (uint64_t)r & (uint64_t)(modulus-1);
}
struct Case{void(*f)(const void*,const void*,void*);unsigned bytes,op;const char *name;};
"#,
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for shape in VectorType::ALL_128
        .into_iter()
        .filter(|s| s.lane_type().is_int())
    {
        for (op, name) in OPS.iter().enumerate() {
            let symbol = format!("{name}_{shape}");
            ir += &format!(
                "func @{symbol}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = {name} v3,v4\n store align 1 v5,v2\n return\n}}\n"
            );
            c += &format!("extern void {symbol}(const void*,const void*,void*);\n");
            cases += &format!(
                "{{{symbol},{},{op},\"{symbol}\"}},\n",
                shape.lane_type().bytes().unwrap()
            );
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
int main(void){
 unsigned char a[17],b[17],r[18];uint64_t seed=0x123456789abcdef;
 const uint64_t edges[]={0,1,7,8,15,16,31,32,63,64,127,128,129,255,256,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){
  const struct Case *c=cases+ci;unsigned w=c->bytes*8;
  for(unsigned k=0;k<1024;k++){
   for(unsigned i=0;i<16/c->bytes;i++){
    seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=k<576?edges[(k/24+i)%24]:seed;
    seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t y=k<576?edges[(k+i)%24]:seed;
    memcpy(a+1+i*c->bytes,&x,c->bytes);memcpy(b+1+i*c->bytes,&y,c->bytes);
   }
   for(unsigned qc=0;qc<2;qc++){
    uint64_t saved,status,wanted=(uint64_t)qc<<27;
    __asm__ volatile("mrs %0,fpsr":"=r"(saved));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");
    memset(r,0xcd,sizeof(r));c->f(a+1,b+1,r+1);
    __asm__ volatile("mrs %0,fpsr":"=r"(status));
    __asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
    if((status & (1ull<<27))!=wanted){fprintf(stderr,"QC %s\n",c->name);return 1;}
    if(r[0]!=0xcd||r[17]!=0xcd)return 2;
    for(unsigned i=0;i<16/c->bytes;i++){
     uint64_t x=0,y=0,z=0;memcpy(&x,a+1+i*c->bytes,c->bytes);memcpy(&y,b+1+i*c->bytes,c->bytes);memcpy(&z,r+1+i*c->bytes,c->bytes);
     uint64_t expected=oracle(c->op,x,y,w);
     if(z!=expected){fprintf(stderr,"%s k=%u i=%u got=%llx expected=%llx\n",c->name,k,i,(unsigned long long)z,(unsigned long long)expected);return 3;}
    }
   }
  }
  // Exact 16-byte loads, immediately before a protected page.
  size_t page=sysconf(_SC_PAGESIZE);unsigned char *p=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
  if(p==MAP_FAILED||mprotect(p+page,page,PROT_NONE))return 4;
  memcpy(p+page-16,a+1,16);c->f(p+page-16,b+1,r+1);c->f(a+1,p+page-16,r+1);munmap(p,2*page);
 }
 return 0;
}
"#;
    (ir, c)
}
#[test]
fn integer_families_preserve_qc_and_match_extended_oracle() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let again = keel_ir::parse(&m.to_string()).unwrap();
    assert_eq!(m.to_string(), again.to_string());
    let dir = common::temp("neon-integer-families");
    fs::write(dir.join("oracle.c"), c).unwrap();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    common::command(
        Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o")),
    );
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for (spilled, pools) in [(false, &keel_codegen::aarch64::REGISTERS), (true, &SPILL)] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                assert!(!text.contains(" = stack_addr"), "scalar compound fallback");
                assert!(
                    !text.contains("sqadd\t")
                        && !text.contains("uqadd\t")
                        && !text.contains("sqsub\t")
                        && !text.contains("uqsub\t"),
                    "pure operation changed QC"
                );
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                assert_eq!(program.stats().text_bytes, 0);
                let direct =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                let parsed = keel_asm::assemble_object(&text).unwrap();
                assert_eq!(direct, parsed);
                if common::clang_parity(selector, allocator, spilled) {
                    fs::write(dir.join("test.s"), &text).unwrap();
                    common::command(
                        Command::new("clang")
                            .args(["-target", "arm64-apple-macos11", "-c"])
                            .arg(dir.join("test.s"))
                            .arg("-o")
                            .arg(dir.join("clang.o")),
                    );
                    let reference = fs::read(dir.join("clang.o")).unwrap();
                    let file = object::File::parse(&*reference).unwrap();
                    assert_eq!(
                        keel_asm::assemble_program(&program, Default::default())
                            .unwrap()
                            .text,
                        file.section_by_name("__text").unwrap().data().unwrap()
                    );
                }
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    // Object parity is checked above; execute the identical image once.
                    fs::write(dir.join("test.o"), direct).unwrap();
                    common::command(
                        Command::new("clang")
                            .arg(dir.join("oracle.o"))
                            .arg(dir.join("test.o"))
                            .arg("-o")
                            .arg(dir.join("direct")),
                    );
                    common::command(&mut Command::new(dir.join("direct")));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
