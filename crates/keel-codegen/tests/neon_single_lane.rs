//! All accepted one-lane architectural signatures, with independent scalar
//! instruction references. Immediate ranges are enumerated in full.
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
fn fixture() -> (String, String, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(void*,void*,void*);uint64_t(*ref)(uint64_t,uint64_t);const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut count = 0;
    for &op in N::ALL {
        for shape in [VectorType::I64x1, VectorType::F64x1] {
            let ty = Type::Vector(shape);
            if op.memory().is_some() || matches!(op, N::Fmla | N::Fmls) || !op.accepts(ty) {
                continue;
            }
            let inputs = op.operand_types(ty).unwrap();
            if inputs
                .iter()
                .any(|t| t.vector().is_some_and(|v| v.lanes() != 1))
            {
                continue;
            }
            let immediates: Vec<_> = op
                .immediate_range(ty)
                .map_or(vec![None], |r| r.map(Some).collect());
            for imm in immediates {
                let name = format!("{op}_{shape}_{}", imm.unwrap_or(0));
                ir += &format!("func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n");
                let mut args = Vec::new();
                for (i, input) in inputs.iter().enumerate() {
                    let n = i + 3;
                    if *input == Type::I32 {
                        ir += &format!(" v{n} = iconst.i32 {}\n", imm.unwrap());
                    } else {
                        ir += &format!(" v{n} = load.{input} v{}\n", i.min(1));
                    }
                    args.push(format!("v{n}"));
                }
                ir += &format!(
                    " v10 = {op}.{shape}({})\n store v10,v2\n return\n}}\n",
                    args.join(",")
                );
                let mnemonic = op
                    .name()
                    .strip_prefix("neon_")
                    .unwrap()
                    .trim_end_matches("_imm")
                    .trim_end_matches("_fixed");
                // The reference directly names the architectural scalar
                // instruction independently of codegen's opcode selection.
                let second = if imm.is_some() && inputs.len() == 2 {
                    "d0"
                } else {
                    "d1"
                };
                let instruction = if let Some(imm) = imm {
                    format!("{mnemonic} d0,{second},#{imm}")
                } else if matches!(op, N::Suqadd | N::Usqadd) {
                    format!("{mnemonic} d0,d1")
                } else if inputs.len() == 2 {
                    format!("{mnemonic} d0,d0,d1")
                } else {
                    format!("{mnemonic} d0,d0")
                };
                c += &format!(
                    "extern void {name}(void*,void*,void*);\nstatic uint64_t ref_{name}(uint64_t a,uint64_t b){{uint64_t r;__asm__ volatile(\"fmov d0,%x1\\nfmov d1,%x2\\n{instruction}\\nfmov %x0,d0\":\"=r\"(r):\"r\"(a),\"r\"(b):\"v0\",\"v1\");return r;}}\n"
                );
                cases += &format!("{{{name},ref_{name},\"{name}\"}},\n");
                count += 1;
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char* edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
static uint64_t fpsr(void){uint64_t r;__asm__ volatile("mrs %0,fpsr":"=r"(r));return r;}
static void status(uint64_t r){__asm__ volatile("msr fpsr,%0"::"r"(r):"memory");}
static void control(uint64_t r){__asm__ volatile("msr fpcr,%0"::"r"(r):"memory");}
int main(void){uint64_t saved,old=fpsr();__asm__ volatile("mrs %0,fpcr":"=r"(saved));unsigned char*a=edge()-8,*b=edge()-8,*out=edge()-8;
uint64_t patterns[]={0,1,2,63,64,127,128,255,256,0x7fffffffffffffffull,0x8000000000000000ull,~0ull,0x0123456789abcdefull,0x3ff0000000000000ull,0xbff0000000000000ull,0x7ff0000000000000ull,0xfff0000000000000ull,0x7ff0000000000001ull,0x7ff8000000000042ull,0x000fffffffffffffull,0x3fe0000000000000ull};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++)for(unsigned rm=0;rm<4;rm++)for(unsigned qc=0;qc<2;qc++)for(unsigned seed=0;seed<64;seed++){
uint64_t x=patterns[seed%21],y=patterns[(seed*7+3)%21];if(seed>=42){x=(x*0x9e3779b97f4a7c15ull)^seed;y^=x>>7;}
memcpy(a,&x,8);memcpy(b,&y,8);control((saved&~(3ull<<22))|((uint64_t)rm<<22));status((uint64_t)qc<<27);
uint64_t expected=cases[k].ref(x,y),flags=fpsr();status((uint64_t)qc<<27);out[-1]=0x5a;cases[k].f(a,b,out);uint64_t actual;memcpy(&actual,out,8);uint64_t got=fpsr();
if(actual!=expected||flags!=got||out[-1]!=0x5a){control(saved);status(old);fprintf(stderr,"%s seed %u rm %u qc %u: %llx/%llx flags %llx/%llx\n",cases[k].name,seed,rm,qc,(unsigned long long)actual,(unsigned long long)expected,(unsigned long long)got,(unsigned long long)flags);return 1;}
// In-place output verifies the source is consumed before any store.
status((uint64_t)qc<<27);cases[k].f(a,b,a);memcpy(&actual,a,8);assert(actual==expected&&fpsr()==flags);
}
control(saved);status(old);return 0;}
"#;
    (ir, c, count)
}
#[test]
fn every_single_lane64_signature_and_immediate_native_oracle() {
    let (source, c, count) = fixture();
    eprintln!("single-lane64 operation/shape/immediate combinations: {count}");
    assert!(
        count > 1000,
        "enumerate full immediate ranges, not just boundaries: {count}"
    );
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-single-lane");
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
                    fs::write(dir.join("test.s"), text).unwrap();
                    common::command(
                        Command::new("clang")
                            .arg("-c")
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

#[test]
fn single_lane64_rejects_nonarchitectural_shapes_and_immediates() {
    for body in [
        "v3 = neon_sqdmulh.i64x1(v1,v2)",
        "v3 = neon_fadd.i64x1(v1,v2)",
        "v3 = neon_shl_imm.i64x1(v1,v4)",
        "v3 = neon_sshr_imm.i64x1(v1,v5)",
    ] {
        let source = format!(
            "func @bad(i64x1,i64x1) {{\nblock0(v1: i64x1,v2: i64x1):\n v4 = iconst.i32 64\n v5 = iconst.i32 0\n {body}\n return\n}}"
        );
        let module = keel_ir::parse(&source).unwrap();
        assert!(keel_ir::verify_module(&module).is_err(), "{body}");
    }
}
