//! Exact SSA load/store + ptradd witnesses for every pinned structured
//! post-index record. No new IR operation or post-index selection is implied.
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
fn fixture() -> (String, String, usize, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\n#include <signal.h>\n#include <setjmp.h>\ntypedef void(*F)(void*,const void*,void*,uintptr_t*,int64_t);\nstruct C{F f,ref;unsigned count,bytes,foot,store,regpost;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    for record in include_str!("../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|l| l.contains("_asisdlsep_") || l.contains("_asisdlsop_"))
    {
        records += 1;
        let pieces = record.split('_').collect::<Vec<_>>();
        let mnemonic = pieces[0].to_ascii_lowercase();
        let class = pieces[1];
        let tag = pieces[2];
        let store = mnemonic.starts_with("st");
        let replicate = mnemonic.ends_with('r');
        let full = class == "asisdlsep";
        let count = tag.chars().last().unwrap().to_digit(10).unwrap() as u8;
        let lane = !full && !replicate;
        let register = pieces[3].starts_with('r');
        let width = if lane {
            match tag.as_bytes()[0] {
                b'B' => 1,
                b'H' => 2,
                b'S' => 4,
                b'D' => 8,
                _ => unreachable!(),
            }
        } else {
            0
        };
        let op_name = if replicate {
            format!("neon_{mnemonic}")
        } else if lane {
            format!("neon_{mnemonic}_lane")
        } else if mnemonic.ends_with('1') && count > 1 {
            format!("neon_{mnemonic}x{count}")
        } else {
            format!("neon_{mnemonic}")
        };
        let op: N = op_name.parse().unwrap();
        for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
            let bytes = shape.bytes();
            let w = shape.lane_type().bytes().unwrap();
            if lane && w != width {
                continue;
            }
            // Interleaved .1d has no legal hardware encoding; that IR signature uses
            // LD1/ST1 lists and is covered by the base structured oracle instead.
            if full && shape.lanes() == 1 && !mnemonic.ends_with('1') {
                continue;
            }
            for index in 0..if lane { shape.lanes() } else { 1 } {
                let name = format!("post_{record}_{shape}_{index}");
                let footprint = u32::from(count) * if lane || replicate { w } else { bytes };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr,ptr,i64) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr,v4: i64):\n"
                );
                let mut args = vec!["v0".to_string()];
                if store || lane {
                    for i in 0..count {
                        let n = 10 + 3 * u32::from(i);
                        ir += &format!(
                            " v{n} = iconst.i64 {}\n v{} = ptradd v1,v{n}\n v{} = load.{shape} align 1 v{}\n",
                            u32::from(i) * bytes,
                            n + 1,
                            n + 2,
                            n + 1
                        );
                        args.push(format!("v{}", n + 2));
                    }
                }
                if lane {
                    ir += &format!(" v25 = iconst.i32 {index}\n");
                    args.push("v25".into());
                }
                let results = (0..if store { 0 } else { count })
                    .map(|i| format!("v{}", 30 + i))
                    .collect::<Vec<_>>();
                ir += &format!(
                    " {}{op}.{shape}({})\n",
                    if store {
                        String::new()
                    } else {
                        format!("{} = ", results.join(","))
                    },
                    args.join(",")
                );
                if !store {
                    for (i, result) in results.iter().enumerate() {
                        let n = 40 + 2 * i;
                        ir += &format!(
                            " v{n} = iconst.i64 {}\n v{} = ptradd v2,v{n}\n store align 1 {result},v{}\n",
                            i as u32 * bytes,
                            n + 1,
                            n + 1
                        );
                    }
                }
                if register {
                    ir += " v50 = ptradd v0,v4\n";
                } else {
                    ir += &format!(" v49 = iconst.i64 {footprint}\n v50 = ptradd v0,v49\n");
                }
                ir += " store v50,v3\n return\n}\n";
                let element = match w {
                    1 => 'b',
                    2 => 'h',
                    4 => 's',
                    8 => 'd',
                    _ => unreachable!(),
                };
                let arrangement = if lane {
                    element.to_string()
                } else {
                    format!("{}{element}", shape.lanes())
                };
                let list = (0..count)
                    .map(|r| format!("v{r}.{arrangement}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut asm = String::new();
                let reg = if bytes == 8 { 'd' } else { 'q' };
                if store || lane {
                    for r in 0..count {
                        asm += &format!("ldr {reg}{r},[%x1,#{}]\\n", u32::from(r) * bytes);
                    }
                }
                asm += &format!(
                    "{mnemonic} {{{list}}}{} ,[%x0],{}\\n",
                    if lane {
                        format!("[{index}]")
                    } else {
                        String::new()
                    },
                    if register {
                        "%x3".to_string()
                    } else {
                        format!("#{footprint}")
                    }
                );
                if !store {
                    for r in 0..count {
                        asm += &format!("str {reg}{r},[%x2,#{}]\\n", u32::from(r) * bytes);
                    }
                }
                c += &format!(
                    "extern void {name}(void*,const void*,void*,uintptr_t*,int64_t);\nstatic void ref_{name}(void*base,const void*input,void*out,uintptr_t*wb,int64_t delta){{__asm__ volatile(\"{asm}\":\"+&r\"(base):\"r\"(input),\"r\"(out),\"r\"(delta):\"memory\",\"v0\",\"v1\",\"v2\",\"v3\");*wb=(uintptr_t)base;}}\n"
                );
                cases += &format!(
                    "{{{name},ref_{name},{count},{bytes},{footprint},{},{},\"{record}/{shape}/{index}\"}},\n",
                    u8::from(store),
                    u8::from(register)
                );
                combinations += 1;
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char*edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
static sigjmp_buf fault_jump;static void fault_handler(int sig){(void)sig;siglongjmp(fault_jump,1);}
static void expect_fault(F f,void*base,const void*input,void*out,int64_t delta){volatile uintptr_t wb=0x12345678;if(!sigsetjmp(fault_jump,1)){f(base,input,out,(uintptr_t*)&wb,delta);assert(!"expected structured memory fault");}assert(wb==0x12345678);}
int main(void){struct sigaction action={0};action.sa_handler=fault_handler;sigemptyset(&action.sa_mask);assert(!sigaction(SIGBUS,&action,0));assert(!sigaction(SIGSEGV,&action,0));
unsigned char*ae=edge(),*re=edge(),*oe=edge(),*ee=edge(),*fe=edge();int64_t deltas[]={-128,-1,0,1,7,16,63,128};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++){const struct C*p=cases+k;unsigned full=p->count*p->bytes;unsigned char*base=ae-p->foot,*reference=re-p->foot,*out=oe-full,*expected=ee-full,*input=fe-full;
for(unsigned seed=0;seed<8;seed++)for(unsigned d=0;d<(p->regpost?8:1);d++){int64_t delta=p->regpost?deltas[d]:p->foot;for(unsigned i=0;i<p->foot;i++)base[i]=reference[i]=(i*17+seed*41)^0xa5;for(unsigned i=0;i<full;i++)input[i]=(i*13+seed*19)^0x5a;
memset(out,0xcd,full);memset(expected,0xcd,full);uintptr_t actualwb=0,expectedwb=0;out[-1]=0x5a;
p->ref(reference,input,expected,&expectedwb,delta);p->f(base,input,out,&actualwb,delta);
if(actualwb!=(uintptr_t)base+delta||expectedwb!=(uintptr_t)reference+delta||memcmp(base,reference,p->foot)||(!p->store&&memcmp(out,expected,full))||out[-1]!=0x5a){fprintf(stderr,"%s seed %u delta %lld\n",p->name,seed,(long long)delta);return 1;}
}
// Both immediate-first faults and late faults after a valid prefix must leave
// the observable pointer output unchanged. No fault-freedom assumption is used.
expect_fault(p->ref,re,input,expected,7);expect_fault(p->f,ae,input,out,7);
if(p->foot>1){expect_fault(p->ref,re-(p->foot-1),input,expected,7);expect_fault(p->f,ae-(p->foot-1),input,out,7);}
}
return 0;}
"#;
    (ir, c, records, combinations)
}
#[test]
fn every_postindex_record_has_ordered_ssa_pointer_and_fault_witness() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 100);
    eprintln!("post-index records {records}, shape/lane signatures {combinations}");
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-postindex");
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
                let mut m = m.clone();
                if selector == SelectionTier::Full {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &p).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &p).unwrap();
                let bytes =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                assert_eq!(bytes, keel_asm::assemble_object(&text).unwrap());
                if common::clang_parity(selector, allocator, spilled) {
                    fs::write(dir.join("test.s"), text).unwrap();
                    common::command(
                        Command::new("clang")
                            .args(["-target", "arm64-apple-macos11", "-c"])
                            .arg(dir.join("test.s"))
                            .arg("-o")
                            .arg(dir.join("clang.o")),
                    );
                    let cb = fs::read(dir.join("clang.o")).unwrap();
                    let cf = object::File::parse(&*cb).unwrap();
                    assert_eq!(
                        keel_asm::assemble_program(&program, Default::default())
                            .unwrap()
                            .text,
                        cf.section_by_name("__text").unwrap().data().unwrap()
                    );
                }
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
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
