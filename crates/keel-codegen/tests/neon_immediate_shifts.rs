use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
#[path = "common/immediates.rs"]
mod immediates;
#[path = "common/matrix.rs"]
mod matrix;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn vector(ty: Type, signed: bool) -> String {
    let v = ty.vector().unwrap();
    format!(
        "{}{}x{}_t",
        if signed { "int" } else { "uint" },
        v.lane_type().bytes().unwrap() * 8,
        v.lanes()
    )
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <arm_neon.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,const void*,void*);void(*ref)(const void*,const void*,void*);const char *name;unsigned bytes,sourcebytes,resultbytes;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for &op in N::ALL
        .iter()
        .filter(|o| !o.is_conversion() && o.memory().is_none())
    {
        for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
            let ty = Type::Vector(shape);
            let Some(range) = op.immediate_range(ty) else {
                continue;
            };
            let inputs = op.operand_types(ty).unwrap();
            let vectors = &inputs[..inputs.len() - 1];
            let acc = vectors.len() == 2;
            let (stem, signed) = match op {
                N::Shrn => ("vshrn_n", false),
                N::Rshrn => ("vrshrn_n", false),
                N::Sqshrn => ("vqshrn_n", true),
                N::Uqshrn => ("vqshrn_n", false),
                N::Sqrshrn => ("vqrshrn_n", true),
                N::Uqrshrn => ("vqrshrn_n", false),
                N::Sqshrun => ("vqshrun_n", true),
                N::Sqrshrun => ("vqrshrun_n", true),
                N::SshrImm => ("vshrq_n", true),
                N::UshrImm => ("vshrq_n", false),
                N::SrshrImm => ("vrshrq_n", true),
                N::UrshrImm => ("vrshrq_n", false),
                N::ShlImm => ("vshlq_n", false),
                N::SqshlImm => ("vqshlq_n", true),
                N::UqshlImm => ("vqshlq_n", false),
                N::Sqshlu => ("vqshluq_n", true),
                N::Sli => ("vsliq_n", false),
                N::Sri => ("vsriq_n", false),
                N::Ssra => ("vsraq_n", true),
                N::Usra => ("vsraq_n", false),
                N::Srsra => ("vrsraq_n", true),
                N::Ursra => ("vrsraq_n", false),
                _ => unreachable!(),
            };
            let stem = if shape.bytes() == 8 {
                stem.replace("q_", "_")
            } else {
                stem.to_string()
            };
            let source = vectors[0].vector().unwrap();
            let width = source.lane_type().bytes().unwrap() * 8;
            let destwidth = shape.lane_type().bytes().unwrap() * 8;
            for n in immediates::boundaries(range) {
                let name = format!("{op}_{shape}_{n}");
                ir += &format!("func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n");
                for (i, t) in vectors.iter().enumerate() {
                    ir += &format!(" v{} = load.{t} align 1 v{i}\n", 3 + i);
                }
                ir += &format!(
                    " v5 = iconst.i32 {n}\n v6 = {op}.{ty}(v3,{}v5)\n store align 1 v6,v2\n return\n}}\n",
                    if acc { "v4," } else { "" }
                );
                c += &format!(
                    "extern void {name}(const void*,const void*,void*);\n__attribute__((noinline)) static void ref_{name}(const void*a,const void*b,void*out){{{} x,y;memcpy(&x,a,{1});memcpy(&y,b,{1});",
                    vector(vectors[0], signed),
                    vectors[0].bytes().unwrap()
                );
                let letter = if signed { "s" } else { "u" };
                let call = format!(
                    "{stem}_{letter}{width}({}{n})",
                    if acc { "x,y," } else { "x," }
                );
                let outsigned = signed && !matches!(op, N::Sqshrun | N::Sqrshrun | N::Sqshlu);
                let outletter = if outsigned { "s" } else { "u" };
                let expression = if op.changes_width() && shape.bytes() == 16 {
                    format!(
                        "vcombine_{outletter}{destwidth}({call},vdup_n_{outletter}{destwidth}(0))"
                    )
                } else {
                    call
                };
                c += &format!(
                    "{} r={expression};memcpy(out,&r,{});}}\n",
                    vector(ty, outsigned),
                    shape.bytes()
                );
                cases += &format!(
                    "{{{name},ref_{name},\"{name}\",{},{},{}}},\n",
                    width / 8,
                    vectors[0].bytes().unwrap(),
                    shape.bytes()
                );
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char* edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
int main(void){unsigned char *ae=edge(),*be=edge(),*oe=edge();unsigned char a[17],b[17],actual[18],expected[16];uint64_t seed=0x123456789abcdef;
 const uint64_t edge[]={0,1,127,128,255,256,32767,32768,65535,65536,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case *p=cases+ci;
  for(unsigned k=0;k<64;k++){
   for(unsigned i=0;i<16/p->bytes;i++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=k<16?edge[(k+i)%16]:seed,y=k<16?edge[(k*3+i)%16]:~seed;memcpy(a+1+i*p->bytes,&x,p->bytes);memcpy(b+1+i*p->bytes,&y,p->bytes);}
   for(unsigned qc=0;qc<2;qc++){
    uint64_t saved,s1,s2,wanted=(uint64_t)qc<<27;__asm__ volatile("mrs %0,fpsr":"=r"(saved));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->ref(a+1,b+1,expected);__asm__ volatile("mrs %0,fpsr":"=r"(s1));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");memset(actual,0xcd,sizeof(actual));p->f(a+1,b+1,actual+1);__asm__ volatile("mrs %0,fpsr":"=r"(s2));
    unsigned char *ga=ae-p->sourcebytes,*gb=be-p->sourcebytes,*go=oe-p->resultbytes;memcpy(ga,a+1,p->sourcebytes);memcpy(gb,b+1,p->sourcebytes);go[-1]=0xa5;
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->f(ga,gb,go);uint64_t guarded;__asm__ volatile("mrs %0,fpsr":"=r"(guarded));assert(!memcmp(go,expected,p->resultbytes)&&guarded==s1&&go[-1]==0xa5);
    unsigned char alias[16];memcpy(alias,a+1,p->sourcebytes);__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->f(alias,b+1,alias);__asm__ volatile("mrs %0,fpsr":"=r"(guarded));assert(!memcmp(alias,expected,p->resultbytes)&&guarded==s1);
    __asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
    if(memcmp(expected,actual+1,p->resultbytes)||s1!=s2||actual[0]!=0xcd||actual[1+p->resultbytes]!=0xcd){fprintf(stderr,"%s k=%u qc=%u fpsr=%llx/%llx\n",p->name,k,qc,(unsigned long long)s1,(unsigned long long)s2);return 1;}
   }
  }
 }return 0;}
"#;
    (ir, c)
}
#[test]
fn immediate_shift_boundaries_match_acle_and_qc() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let prepared = matrix::PreparedModule::parse(&source);
    let m = prepared.for_selector(SelectionTier::Baseline);
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-immediate-shifts");
    fs::write(dir.join("oracle.c"), c).unwrap();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    common::command(
        Command::new("clang")
            .args(["-O1", "-c"])
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
            for (spilled, pools) in [(false, &keel_codegen::aarch64::REGISTERS), (true, &SPILL)] {
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
