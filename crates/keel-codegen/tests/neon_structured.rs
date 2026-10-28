use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,void*);unsigned count,bytes,width,store,lane,replicate,interleaved,index;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    for &op in N::ALL {
        let Some(m) = op.memory() else {
            continue;
        };
        for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
            for index in 0..if m.lane { shape.lanes() } else { 1 } {
                let name = format!("{op}_{shape}_{index}");
                ir += &format!("func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n");
                let mut args = vec![if m.store {
                    "v2".to_string()
                } else {
                    "v0".to_string()
                }];
                if m.store || m.lane {
                    for j in 0..m.count {
                        let n = 3 + u32::from(j) * 3;
                        ir += &format!(
                            " v{n} = iconst.i64 {}\n v{} = ptradd v{},v{n}\n v{} = load.{shape} v{}\n",
                            u32::from(j) * shape.bytes(),
                            n + 1,
                            if m.store { 0 } else { 1 },
                            n + 2,
                            n + 1
                        );
                        args.push(format!("v{}", n + 2));
                    }
                }
                if m.lane {
                    ir += &format!(" v20 = iconst.i32 {index}\n");
                    args.push("v20".into());
                }
                let results = (0..m.count)
                    .map(|j| format!("v{}", 30 + j))
                    .collect::<Vec<_>>()
                    .join(",");
                ir += &format!(
                    " {}{op}.{shape}({})\n",
                    if m.store {
                        String::new()
                    } else {
                        format!("{results} = ")
                    },
                    args.join(",")
                );
                if !m.store {
                    for j in 0..m.count {
                        let n = 40 + u32::from(j) * 2;
                        ir += &format!(
                            " v{n} = iconst.i64 {}\n v{} = ptradd v2,v{n}\n store v{},v{}\n",
                            u32::from(j) * shape.bytes(),
                            n + 1,
                            30 + j,
                            n + 1
                        );
                    }
                }
                ir += " return\n}\n";
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
                cases += &format!(
                    "{{{name},{},{},{},{},{},{},{},{index},\"{name}\"}},\n",
                    m.count,
                    shape.bytes(),
                    shape.lane_type().bytes().unwrap(),
                    u8::from(m.store),
                    u8::from(m.lane),
                    u8::from(m.replicate),
                    u8::from(m.interleaved)
                );
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char* edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(mprotect(p+n,n,PROT_NONE)==0);return p+n;}
static void invoke(const struct C*p,const void*a,const void*b,void*out){uint64_t before,after;__asm__ volatile("mrs %0,fpsr":"=r"(before));p->f(a,b,out);__asm__ volatile("mrs %0,fpsr":"=r"(after));assert(before==after);}
int main(void){uint64_t saved;__asm__ volatile("mrs %0,fpsr":"=r"(saved));unsigned char *ae=edge(),*be=edge(),*oe=edge();unsigned char expected[64],input[64],fallback[64];
for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++){const struct C*p=cases+k;unsigned lanes=p->bytes/p->width,full=p->count*p->bytes,foot=p->lane||p->replicate?p->count*p->width:full;
unsigned insize=p->store?full:foot,outsize=p->store?foot:full;unsigned char*a=ae-insize,*b=be-full,*out=oe-outsize;
for(unsigned seed=0;seed<8;seed++)for(unsigned qc=0;qc<2;qc++){uint64_t wanted=((uint64_t)qc<<27)|0x9full;__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");for(unsigned i=0;i<insize;i++)input[i]=(i*17+seed*41)^0xa5;for(unsigned i=0;i<full;i++)fallback[i]=(i*13+seed*19)^0x5a;
memcpy(a,input,insize);memcpy(b,fallback,full);memcpy(expected,fallback,full);
for(unsigned component=0;component<p->count;component++)for(unsigned lane=0;lane<(p->lane?1:lanes);lane++){
 unsigned vi=p->lane?p->index:lane;unsigned mi=p->lane||p->replicate?component:p->interleaved?lane*p->count+component:component*lanes+lane;
 if(p->store)memcpy(expected+mi*p->width,input+(component*lanes+vi)*p->width,p->width);
 else memcpy(expected+(component*lanes+vi)*p->width,input+mi*p->width,p->width);
}
memset(out,0xcc,outsize);out[-1]=0xdd;invoke(p,a,b,out);if(memcmp(out,expected,outsize)||out[-1]!=0xdd){fprintf(stderr,"%s seed %u\n",p->name,seed);return 1;}
// Output overlaps the complete input region. Every SSA input is captured first.
unsigned char alias[128];memcpy(alias,input,insize);invoke(p,alias,b,alias);if(memcmp(alias,expected,outsize)){fprintf(stderr,"alias %s\n",p->name);return 2;}
}}
__asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");return 0;}
"#;
    (ir, c)
}
#[test]
fn structured_d_q_all_counts_shapes_lanes_exact_footprints() {
    let (source, c) = fixture();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-structured");
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
            let mut m = m.clone();
            if selector == SelectionTier::Full {
                keel_opt::optimize(&mut m).unwrap();
            }
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let bytes =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                assert_eq!(bytes, keel_asm::assemble_object(&text).unwrap());
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    use object::{Object, ObjectSection};
                    fs::write(dir.join("test.s"), &text).unwrap();
                    common::command(
                        Command::new("clang")
                            .arg("-c")
                            .arg(dir.join("test.s"))
                            .arg("-o")
                            .arg(dir.join("clang.o")),
                    );
                    let clang = fs::read(dir.join("clang.o")).unwrap();
                    let clang = object::File::parse(&*clang).unwrap();
                    assert_eq!(
                        keel_asm::assemble_program(&program, Default::default())
                            .unwrap()
                            .text,
                        clang.section_by_name("__text").unwrap().data().unwrap()
                    );
                    fs::write(dir.join("test.o"), bytes).unwrap();
                    common::command(
                        Command::new("clang")
                            .arg(dir.join("oracle.o"))
                            .arg(dir.join("test.o"))
                            .arg("-o")
                            .arg(dir.join("test")),
                    );
                    common::command(&mut Command::new(dir.join("test")));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
