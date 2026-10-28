use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{VectorReduceOp, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
#[allow(dead_code)]
mod common;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,void*);unsigned bytes,op;const char *name;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for shape in VectorType::ALL_128
        .into_iter()
        .filter(|s| s.lane_type().is_int())
    {
        for (index, op) in VectorReduceOp::ALL.into_iter().enumerate() {
            let name = format!("{op}_{shape}");
            ir += &format!(
                "func @{name}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{shape} align 1 v0\n v3 = {op} v2\n store align 1 v3,v1\n return\n}}\n"
            );
            c += &format!("extern void {name}(const void*,void*);\n");
            cases += &format!(
                "{{{name},{},{index},\"{name}\"}},\n",
                shape.lane_type().bytes().unwrap()
            );
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
int main(void){
 uint64_t seed=0x123456789abcdef,edges[]={0,1,7,8,15,16,31,32,63,64,127,128,255,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 unsigned char a[17],r[10];
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){
  const struct Case *c=cases+ci;unsigned w=c->bytes*8;__int128 modulus=(__int128)1<<w,sign=modulus/2;uint64_t mask=(uint64_t)(modulus-1);
  for(unsigned k=0;k<1024;k++){
   uint64_t expected=0;
   for(unsigned i=0;i<16/c->bytes;i++){
    seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=(k<22?edges[(k+i)%22]:seed)&mask;
    memcpy(a+1+i*c->bytes,&x,c->bytes);
    if(!i)expected=x;else {
     __int128 sx=x>=sign?(__int128)x-modulus:x,se=expected>=sign?(__int128)expected-modulus:expected;
     switch(c->op){case 0:expected=(expected+x)&mask;break;case 1:expected&=x;break;case 2:expected|=x;break;case 3:expected^=x;break;case 4:if(sx<se)expected=x;break;case 5:if(sx>se)expected=x;break;case 6:if(x<expected)expected=x;break;case 7:if(x>expected)expected=x;break;}
    }
   }
   memset(r,0xcd,sizeof(r));c->f(a+1,r+1);uint64_t actual=0;memcpy(&actual,r+1,c->bytes);
   if(actual!=expected||r[0]!=0xcd||r[c->bytes+1]!=0xcd){fprintf(stderr,"%s k=%u actual=%llx expected=%llx\n",c->name,k,(unsigned long long)actual,(unsigned long long)expected);return 1;}
  }
  size_t page=sysconf(_SC_PAGESIZE);unsigned char *p=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED||mprotect(p+page,page,PROT_NONE))return 2;
  memcpy(p+page-16,a+1,16);c->f(p+page-16,r+1);munmap(p,2*page);
 }
 return 0;
}
"#;
    (ir, c)
}
#[test]
fn integer_reduction_oracle_spills_and_object_parity() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-reductions");
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
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            // Conservative x86 legalization must remain available for portable reductions.
            let x86 = keel_codegen::x86_64::compile_program_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::x86::assemble_program_object_with_features(&x86, Default::default()).unwrap();
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                assert!(!text.contains(" = stack_addr"));
                assert!(
                    text.contains("addv\t")
                        && text.contains("addp\t")
                        && text.contains("sminv\t")
                        && text.contains("umaxv\t")
                );
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                assert_eq!(program.stats().text_bytes, 0);
                let direct =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                let parsed = keel_asm::assemble_object(&text).unwrap();
                assert_eq!(direct, parsed);
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
