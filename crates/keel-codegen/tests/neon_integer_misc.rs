//! Independent integer formulas for baseline bit and accumulator families.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
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
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);unsigned op,bytes,width;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    for (kind, op) in [
        N::Cls,
        N::Rbit,
        N::Mla,
        N::Mls,
        N::Saba,
        N::Uaba,
        N::Sadalp,
        N::Uadalp,
    ]
    .into_iter()
    .enumerate()
    {
        for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
            let ty = Type::Vector(shape);
            let Some(inputs) = op.operand_types(ty) else {
                continue;
            };
            let name = format!("{op}_{shape}");
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
            );
            let mut args = Vec::new();
            for (i, input) in inputs.iter().enumerate() {
                ir += &format!(" v{} = load.{input} align 1 v{i}\n", i + 4);
                args.push(format!("v{}", i + 4));
            }
            ir += &format!(
                " v8 = {op}.{shape}({})\n store align 1 v8,v3\n return\n}}\n",
                args.join(",")
            );
            c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
            cases += &format!(
                "{{{name},{kind},{},{},\"{name}\"}},\n",
                shape.bytes(),
                shape.lane_type().bytes().unwrap()
            );
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char*edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
static uint64_t get(const unsigned char*p,unsigned n){uint64_t x=0;memcpy(&x,p,n);return x;}
static __int128 sign(uint64_t x,unsigned bits){return x&(1ull<<(bits-1))?(__int128)x-((__int128)1<<bits):x;}
static uint64_t status(void){uint64_t s;__asm__ volatile("mrs %0,fpsr":"=r"(s));return s;}
static void setstatus(uint64_t s){__asm__ volatile("msr fpsr,%0"::"r"(s):"memory");}
int main(void){unsigned char*ae=edge(),*be=edge(),*ce=edge(),*oe=edge();uint64_t saved=status(),seed=0x123456789abcdefull;
uint64_t edges[]={0,1,2,7,8,15,16,31,32,63,64,127,128,129,255,256,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000ull,~0ull};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++){const struct C*p=cases+k;unsigned w=p->width,bits=8*w,n=p->bytes;unsigned char*a=ae-n,*b=be-n,*c=ce-n,*out=oe-n;
for(unsigned data=0;data<512;data++){unsigned char input[3][16],expected[16];for(unsigned j=0;j<3;j++){unsigned iw=j==1&&p->op>=6?w/2:w;for(unsigned i=0;i<n/iw;i++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=data<25?edges[(data*(2*j+1)+i)%25]:seed;memcpy(input[j]+i*iw,&x,iw);}}
for(unsigned lane=0;lane<n/w;lane++){uint64_t x=get(input[0]+lane*w,w),y=get(input[1]+lane*w,w),z=get(input[2]+lane*w,w),r=0;
if(p->op==0){unsigned signbit=x>>(bits-1);for(int bit=(int)bits-2;bit>=0&&((x>>bit)&1)==signbit;bit--)r++;}
else if(p->op==1){for(unsigned i=0;i<8;i++)r|=((x>>i)&1)<<(7-i);}
else if(p->op==2)r=x+(uint64_t)((unsigned __int128)y*z);
else if(p->op==3)r=x-(uint64_t)((unsigned __int128)y*z);
else if(p->op==4){__int128 d=sign(y,bits)-sign(z,bits);r=x+(uint64_t)(d<0?-d:d);}
else if(p->op==5)r=x+(y<z?z-y:y-z);
else{unsigned sw=w/2,sbits=bits/2;uint64_t a=get(input[1]+lane*w,sw),b=get(input[1]+lane*w+sw,sw);r=p->op==6?x+(uint64_t)(sign(a,sbits)+sign(b,sbits)):x+a+b;}
memcpy(expected+lane*w,&r,w);}
for(unsigned qc=0;qc<2;qc++){uint64_t wanted=((uint64_t)qc<<27)|0x9full;memcpy(a,input[0],n);memcpy(b,input[1],n);memcpy(c,input[2],n);out[-1]=0xa5;setstatus(wanted);p->f(a,b,c,out);uint64_t got=status();
if(memcmp(out,expected,n)||out[-1]!=0xa5||got!=wanted){setstatus(saved);fprintf(stderr,"%s data %u qc %u flags %llx/%llx\n",p->name,data,qc,(unsigned long long)got,(unsigned long long)wanted);return 1;}
for(unsigned alias=0;alias<3;alias++){memcpy(a,input[0],n);memcpy(b,input[1],n);memcpy(c,input[2],n);unsigned char*d=alias==0?a:alias==1?b:c;setstatus(wanted);p->f(a,b,c,d);assert(!memcmp(d,expected,n)&&status()==wanted);}
}
}}
setstatus(saved);return 0;}
"#;
    (ir, c)
}
#[test]
fn integer_bits_and_accumulators_match_formulas_state_aliases_and_guardpages() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-integer-misc");
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
            for pools in [&keel_codegen::aarch64::REGISTERS, &SPILL] {
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
                let direct =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                let parsed = keel_asm::assemble_object(&text).unwrap();
                assert_eq!(direct, parsed);
                assert_eq!(program.stats().text_bytes, 0);
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
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                for bytes in [direct, parsed] {
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
