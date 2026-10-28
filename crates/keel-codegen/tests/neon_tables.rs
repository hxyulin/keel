use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
#[allow(dead_code)]
mod common;
#[path = "common/matrix.rs"]
mod matrix;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,const void*,const void*,void*);unsigned tables,extend,bytes;const char *name;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for bytes in [8, 16] {
        for &op in N::ALL.iter().filter(|o| o.is_table()) {
            let shape = if bytes == 8 { "i8x8" } else { "i8x16" };
            let name = format!("{op}_{shape}");
            let extension = matches!(op, N::Tbx1 | N::Tbx2 | N::Tbx3 | N::Tbx4);
            let tables = op.arity() - 1 - usize::from(extension);
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
            );
            for i in 0..tables {
                ir += &format!(
                    " v{} = iconst.i64 {}\n v{} = ptradd v0,v{}\n v{} = load.i8x16 align 1 v{}\n",
                    4 + 3 * i,
                    16 * i,
                    5 + 3 * i,
                    4 + 3 * i,
                    6 + 3 * i,
                    5 + 3 * i
                );
            }
            ir += &format!(" v20 = load.{shape} align 1 v1\n");
            if extension {
                ir += &format!(" v21 = load.{shape} align 1 v2\n");
            }
            let mut args = if extension {
                vec!["v21".to_string()]
            } else {
                vec![]
            };
            args.extend((0..tables).map(|i| format!("v{}", 6 + 3 * i)));
            args.push("v20".to_string());
            ir += &format!(
                " v22 = {op}.{shape}({})\n store align 1 v22,v3\n return\n}}\n",
                args.join(",")
            );
            c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
            cases += &format!(
                "{{{name},{tables},{},{bytes},\"{name}\"}},\n",
                u8::from(extension)
            );
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char* edge(void){size_t n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED||mprotect(p+n,n,PROT_NONE))_exit(5);return p+n;}
static void invoke(const struct Case*p,const void*a,const void*b,const void*c,void*out){uint64_t before,after;__asm__ volatile("mrs %0,fpsr":"=r"(before));p->f(a,b,c,out);__asm__ volatile("mrs %0,fpsr":"=r"(after));assert(before==after);}
int main(void){uint64_t saved;__asm__ volatile("mrs %0,fpsr":"=r"(saved));unsigned char table[65],index[17],fallback[17],actual[18],expected[16];uint64_t seed=0x123456789abcdef;
unsigned char*te=edge(),*ie=edge(),*fe=edge(),*oe=edge();
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case *p=cases+ci;unsigned bytes=p->bytes;
  for(unsigned k=0;k<256;k++)for(unsigned data=0;data<2;data++)for(unsigned qc=0;qc<2;qc++){uint64_t wanted=((uint64_t)qc<<27)|0x9full;__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");
   for(unsigned i=0;i<64;i++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;table[1+i]=data==0?0:seed;}
   for(unsigned i=0;i<bytes;i++){index[1+i]=(unsigned char)(k+i*17);fallback[1+i]=(unsigned char)(k^i^0xa5);expected[i]=index[1+i]<16*p->tables?table[1+index[1+i]]:p->extend?fallback[1+i]:0;}
   memset(actual,0xcd,sizeof(actual));invoke(p,table+1,index+1,fallback+1,actual+1);
   if(memcmp(expected,actual+1,bytes)||actual[0]!=0xcd||actual[bytes+1]!=0xcd){fprintf(stderr,"%s k=%u\n",p->name,k);return 1;}
   unsigned char saved[64];memcpy(saved,table+1,64);
   for(unsigned i=0;i<p->tables;i++){invoke(p,table+1,index+1,fallback+1,table+1+16*i);if(memcmp(expected,table+1+16*i,bytes))return 2;memcpy(table+1,saved,64);}
   invoke(p,table+1,index+1,fallback+1,fallback+1);if(memcmp(expected,fallback+1,bytes))return 3;
   for(unsigned i=0;i<bytes;i++)fallback[1+i]=(unsigned char)(k^i^0xa5);
   unsigned char *gt=te-16*p->tables,*gi=ie-bytes,*gf=fe-bytes,*go=oe-bytes;
   memcpy(gt,table+1,16*p->tables);memcpy(gi,index+1,bytes);memcpy(gf,fallback+1,bytes);go[-1]=0x5a;
   invoke(p,gt,gi,gf,go);if(memcmp(expected,go,bytes)||go[-1]!=0x5a)return 6;
   invoke(p,table+1,index+1,fallback+1,index+1);if(memcmp(expected,index+1,bytes))return 4;
  }
 }__asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");return 0;}

"#;
    (ir, c)
}
#[test]
fn table_lookup_extension_all_indices_aliases_and_exact_reads() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let prepared = matrix::PreparedModule::parse(&source);
    let m = prepared.for_selector(SelectionTier::Baseline);
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-tables");
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
            let m = prepared.for_selector(selector);
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
                assert!(!text.contains(" = stack_addr"));
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline)
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
    // Bad shape and arity are verifier errors rather than emitter assumptions.
    for shape in VectorType::ALL {
        let accepted = N::Tbl1.accepts(Type::Vector(shape));
        assert_eq!(
            accepted,
            matches!(shape, VectorType::I8x8 | VectorType::I8x16)
        );
    }
}
