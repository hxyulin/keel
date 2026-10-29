//! Independent clang scalar instruction oracle: bits, QC/FP flags and spills.
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
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\ntypedef uint8_t V __attribute__((vector_size(16)));\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*r)(const void*,const void*,const void*,void*);unsigned bytes;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    for &op in N::ALL {
        for ty in [
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::F32,
            Type::F64,
            Type::Vector(VectorType::I64x1),
            Type::Vector(VectorType::F64x1),
        ] {
            let scalar = ty.vector().map_or(ty, |v| v.lane_type());
            if !op.accepts_scalar(scalar) {
                continue;
            }
            let range = op.immediate_range(ty);
            let counts = if let Some(r) = &range {
                immediates::boundaries(r.clone())
            } else {
                vec![0]
            };
            for imm in counts {
                let name = format!("{op}_{ty}_{imm}");
                let inputs = op.operand_types(ty).unwrap();
                let count = inputs.len() - usize::from(range.is_some());
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
                );
                for (i, t) in inputs[..count].iter().enumerate() {
                    ir += &format!(" v{} = load.{t} align 1 v{i}\n", 4 + i);
                }
                let mut args = (0..count)
                    .map(|i| format!("v{}", 4 + i))
                    .collect::<Vec<_>>();
                if range.is_some() {
                    ir += &format!(" v7 = iconst.i32 {imm}\n");
                    args.push("v7".into());
                }
                ir += &format!(
                    " v8 = {op}.{ty}({})\n store align 1 v8,v3\n return\n}}\n",
                    args.join(",")
                );
                c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
                let acc = matches!(
                    op,
                    N::Fmla
                        | N::Fmls
                        | N::Suqadd
                        | N::Usqadd
                        | N::Sqdmlal
                        | N::Sqdmlsl
                        | N::Ssra
                        | N::Usra
                        | N::Srsra
                        | N::Ursra
                        | N::Sli
                        | N::Sri
                );
                let pair = matches!(op, N::Faddp | N::Fminp | N::Fmaxp | N::Fminnmp | N::Fmaxnmp);
                let class = |bytes| match bytes {
                    1 => 'b',
                    2 => 'h',
                    4 => 's',
                    8 => 'd',
                    _ => panic!(),
                };
                let bytes = ty.bytes().unwrap();
                let dc = class(bytes);
                let mnemonic = op
                    .name()
                    .trim_start_matches("neon_")
                    .trim_end_matches("_imm")
                    .trim_end_matches("_fixed");
                let mut asm = format!("{mnemonic} %{dc}0");
                for (j, t) in inputs[..count].iter().enumerate().skip(usize::from(acc)) {
                    if pair {
                        asm += &format!(", %{}.{}", j + 1, if bytes == 4 { "2s" } else { "2d" });
                    } else {
                        let sc = class(t.vector().map_or(*t, |v| v.lane_type()).bytes().unwrap());
                        if matches!(op, N::Fmla | N::Fmls) && j == 2 {
                            asm += &format!(", %{}.{sc}[0]", j + 1);
                        } else {
                            asm += &format!(", %{sc}{}", j + 1);
                        }
                    }
                }
                if range.is_some() {
                    asm += &format!(", #{imm}");
                }
                c += &format!(
                    "__attribute__((noinline)) static void ref_{name}(const void*a,const void*b,const void*c,void*out){{uint64_t r=0;"
                );
                for (i, t) in inputs[..count].iter().enumerate() {
                    let var = ["x", "y", "z"][i];
                    c += &format!(
                        "{} {var}={{0}};memcpy(&{var},{},{});",
                        if pair { "V" } else { "uint64_t" },
                        ["a", "b", "c"][i],
                        t.bytes().unwrap()
                    );
                }
                if acc {
                    c += "r=x;";
                }
                c += &format!(
                    "__asm__ volatile(\"{asm}\":\"{}w\"(r):{});memcpy(out,&r,{bytes});}}\n",
                    if acc { "+" } else { "=" },
                    (0..count)
                        .map(|i| format!("\"w\"({})", ["x", "y", "z"][i]))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                cases += &format!("{{{name},ref_{name},{bytes},\"{name}\"}},\n");
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
int main(void){uint64_t savedcr,savedstatus;__asm__ volatile("mrs %0,fpcr":"=r"(savedcr));__asm__ volatile("mrs %0,fpsr":"=r"(savedstatus));
uint64_t edges[]={0,1,0xff,0x7f,0x80,0xffff,0x7fff,0x8000,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX,0x7f800000,0x7fc12345,0x7f812345,0x3f800000,0x7ff0000000000000,0x7ff8123456789abc,0x7ff0123456789abc,0x3ff0000000000000};
uint64_t seed=0x123456789abcdef;
for(unsigned mode=0;mode<4;mode++){uint64_t cr=(savedcr&~(3ull<<22))|((uint64_t)mode<<22);__asm__ volatile("msr fpcr,%0"::"r"(cr):"memory");
for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct C*p=cases+ci;
for(unsigned k=0;k<256;k++){uint64_t a[2],b[2],c[2];for(unsigned j=0;j<2;j++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;a[j]=k<22?edges[(k+j)%22]:seed;b[j]=k<22?edges[(k*3+j)%22]:~seed;c[j]=k<22?edges[(k*7+j)%22]:seed^0x5a5a5a5a5a5a5a5a;}
for(unsigned qc=0;qc<2;qc++){uint64_t wanted=(uint64_t)qc<<27,s1,s2;unsigned char expected[8],out[10];
__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->r(a,b,c,expected);__asm__ volatile("mrs %0,fpsr":"=r"(s1));
memset(out,0xcd,10);__asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->f(a,b,c,out+1);__asm__ volatile("mrs %0,fpsr":"=r"(s2));
if(memcmp(expected,out+1,p->bytes)||s1!=s2||out[0]!=0xcd||out[1+p->bytes]!=0xcd){fprintf(stderr,"%s k=%u mode=%u status=%llx/%llx\n",p->name,k,mode,(unsigned long long)s1,(unsigned long long)s2);return 1;}
}}}}
__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");__asm__ volatile("msr fpsr,%0"::"r"(savedstatus):"memory");return 0;}
"#;
    (ir, c)
}
#[test]
fn scalar_neon_bits_state_spills_and_clang_encodings() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let prepared = matrix::PreparedModule::parse(&source);
    let dir = common::temp("neon-scalar");
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
                let m = prepared.for_selector(selector);
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &p).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &p).unwrap();
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
