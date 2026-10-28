use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::VectorType;
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
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,void*);unsigned src,dst,mode;const char *name;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    let shapes: Vec<_> = VectorType::ALL_128
        .into_iter()
        .filter(|s| s.lane_type().is_int())
        .collect();
    for src in &shapes {
        for dst in &shapes {
            let a = src.lane_type().bytes().unwrap();
            let b = dst.lane_type().bytes().unwrap();
            let ops: Vec<_> = if a < b {
                let mut o = vec![
                    ("swiden_low", 0),
                    ("uwiden_low", 1),
                    ("swiden_high", 2),
                    ("uwiden_high", 3),
                ];
                if a * 2 == b {
                    o.extend([("spairwise_add_long", 8), ("upairwise_add_long", 9)]);
                }
                o
            } else if a > b {
                vec![
                    ("narrow_low", 4),
                    ("snarrow_sat_low", 5),
                    ("unarrow_sat_low", 6),
                    ("sunarrow_sat_low", 7),
                ]
            } else {
                continue;
            };
            for (op, mode) in ops {
                let name = format!("{op}_{src}_{dst}");
                ir += &format!(
                    "func @{name}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{src} align 1 v0\n v3 = {op}.{dst} v2\n store align 1 v3,v1\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,void*);\n");
                cases += &format!("{{{name},{a},{b},{mode},\"{name}\"}},\n");
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static __int128 signed_lane(uint64_t x,unsigned bytes){__int128 m=(__int128)1<<(bytes*8);return x>=m/2?(__int128)x-m:x;}
int main(void){
 uint64_t seed=0x123456789abcdef,edges[]={0,1,127,128,255,256,32767,32768,65535,65536,0x7fffffff,0x80000000,0xffffffff,0x100000000,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 unsigned char a[17],r[18],expected[16];
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){
  const struct Case *c=cases+ci;uint64_t values[16];__int128 modulus=(__int128)1<<(c->dst*8),sign=modulus/2;
  for(unsigned k=0;k<1024;k++){
   memset(expected,0,16);
   for(unsigned i=0;i<16/c->src;i++){
    seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=k<17?edges[(k+i)%17]:seed;
    memcpy(a+1+i*c->src,&x,c->src);values[i]=0;memcpy(values+i,a+1+i*c->src,c->src);
   }
   unsigned lanes=c->src<c->dst?16/c->dst:16/c->src;
   for(unsigned i=0;i<lanes;i++){
    unsigned index=(c->mode==2||c->mode==3)?16/c->src-lanes+i:i;
    uint64_t x=values[index];__int128 v=(c->mode==0||c->mode==2||c->mode==5||c->mode==7)?signed_lane(x,c->src):(__int128)x;
    if(c->mode==8)v=signed_lane(values[2*i],c->src)+signed_lane(values[2*i+1],c->src);
    if(c->mode==9)v=(__int128)values[2*i]+values[2*i+1];
    if(c->mode==5){if(v < -sign)v=-sign;if(v>=sign)v=sign-1;}
    if(c->mode==6||c->mode==7){if(v<0)v=0;if(v>=modulus)v=modulus-1;}
    uint64_t out=(uint64_t)v;memcpy(expected+i*c->dst,&out,c->dst);
   }
   for(unsigned qc=0;qc<2;qc++){
    uint64_t saved,status,wanted=(uint64_t)qc<<27;
    __asm__ volatile("mrs %0,fpsr":"=r"(saved));__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");
    memset(r,0xcd,sizeof(r));c->f(a+1,r+1);
    __asm__ volatile("mrs %0,fpsr":"=r"(status));__asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
    if(memcmp(r+1,expected,16)||r[0]!=0xcd||r[17]!=0xcd||((status^(uint64_t)wanted)&(1ull<<27))){fprintf(stderr,"%s k=%u qc=%u\n",c->name,k,qc);return 1;}
   }
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
fn width_changes_match_extended_oracle_and_preserve_qc() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-width-changes");
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
                assert!(!text.contains(" = stack_addr"), "width change scalarized");
                assert!(
                    text.contains("sshll2\t")
                        && text.contains("ushll2\t")
                        && text.contains("xtn\t")
                        && text.contains("saddlp\t")
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
