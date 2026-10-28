use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
#[path = "common/matrix.rs"]
mod matrix;
const SPILL: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn vector(ty: Type, sign: &str) -> String {
    let s = ty.vector().unwrap();
    let width = s.lane_type().bytes().unwrap() * 8;
    format!("{sign}{width}x{}_t", s.lanes())
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <arm_neon.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);const char *name;unsigned width,resultbytes;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for &op in N::ALL.iter().filter(|o| {
        o.changes_width()
            && !o.is_conversion()
            && o.immediate_range(Type::Vector(VectorType::I8x16)).is_none()
    }) {
        for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
            // One-lane long forms have scalar operands and are exhausted by neon_scalar.
            if shape.lanes() == 1 {
                continue;
            }
            let ty = Type::Vector(shape);
            let Some(inputs) = op.operand_types(ty) else {
                continue;
            };
            let name = format!("{op}_{shape}");
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
            );
            for (i, t) in inputs.iter().enumerate() {
                ir += &format!(" v{} = load.{t} align 1 v{i}\n", 4 + i);
            }
            let args = (0..inputs.len())
                .map(|i| format!("v{}", 4 + i))
                .collect::<Vec<_>>()
                .join(",");
            ir += &format!(" v8 = {op}.{ty}({args})\n store align 1 v8,v3\n return\n}}\n");
            c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
            let (stem, signed) = match op {
                N::Saddl => ("vaddl", true),
                N::Uaddl => ("vaddl", false),
                N::Ssubl => ("vsubl", true),
                N::Usubl => ("vsubl", false),
                N::Smull => ("vmull", true),
                N::Umull => ("vmull", false),
                N::Sabdl => ("vabdl", true),
                N::Uabdl => ("vabdl", false),
                N::Sqdmull => ("vqdmull", true),
                N::Pmull => ("vmull", false),
                N::Smlal => ("vmlal", true),
                N::Umlal => ("vmlal", false),
                N::Smlsl => ("vmlsl", true),
                N::Umlsl => ("vmlsl", false),
                N::Sabal => ("vabal", true),
                N::Uabal => ("vabal", false),
                N::Sqdmlal => ("vqdmlal", true),
                N::Sqdmlsl => ("vqdmlsl", true),
                N::Saddw => ("vaddw", true),
                N::Uaddw => ("vaddw", false),
                N::Ssubw => ("vsubw", true),
                N::Usubw => ("vsubw", false),
                N::Sqxtn => ("vqmovn", true),
                N::Uqxtn => ("vqmovn", false),
                N::Sqxtun => ("vqmovun", true),
                N::Addhn => ("vaddhn", false),
                N::Raddhn => ("vraddhn", false),
                N::Subhn => ("vsubhn", false),
                N::Rsubhn => ("vrsubhn", false),
                _ => unreachable!(),
            };
            let narrowing =
                inputs[0].vector().unwrap().lane_type().bytes() > shape.lane_type().bytes();
            let wide = matches!(op, N::Saddw | N::Uaddw | N::Ssubw | N::Usubw);
            let acc = inputs.len() == 3;
            let poly = op == N::Pmull;
            let source_letter = if poly {
                "p"
            } else if signed {
                "s"
            } else {
                "u"
            };
            let source_prefix = if poly {
                "poly"
            } else if signed {
                "int"
            } else {
                "uint"
            };
            let dest_unsigned = matches!(op, N::Sabdl | N::Sabal | N::Sqxtun) || !signed;
            let dest_prefix = if poly {
                "poly"
            } else if dest_unsigned {
                "uint"
            } else {
                "int"
            };
            let source_ty = if acc || wide { inputs[1] } else { inputs[0] };
            let source_width = source_ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
            let dest_width = shape.lane_type().bytes().unwrap() * 8;
            c += &format!(
                "__attribute__((noinline)) static void ref_{name}(const void*a,const void*b,const void*c,void*out){{"
            );
            for (i, t) in inputs.iter().enumerate() {
                let prefix = if i == 0 && (acc || wide) {
                    dest_prefix
                } else {
                    source_prefix
                };
                c += &format!(
                    "{} x{i};memcpy(&x{i},{},16);",
                    vector(*t, prefix),
                    ["a", "b", "c"][i]
                );
            }
            let operands = (0..inputs.len())
                .map(|i| {
                    if !narrowing && (!acc && !wide || i > 0) {
                        format!("vget_low_{source_letter}{source_width}(x{i})")
                    } else {
                        format!("x{i}")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let call = format!("{stem}_{source_letter}{source_width}({operands})");
            let expression = if narrowing && shape.bytes() == 16 {
                let letter = if dest_unsigned { "u" } else { "s" };
                format!("vcombine_{letter}{dest_width}({call},vdup_n_{letter}{dest_width}(0))")
            } else {
                call
            };
            c += &format!(
                "{} r={expression};memcpy(out,&r,{});}}\n",
                vector(ty, dest_prefix),
                shape.bytes()
            );
            cases += &format!(
                "{{{name},ref_{name},\"{name}\",{source_width},{}}},\n",
                shape.bytes()
            );
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
int main(void){unsigned char a[17],b[17],c[17],actual[18],expected[16];uint64_t seed=0x123456789abcdef;
 const uint64_t edge[]={0,1,127,128,255,256,32767,32768,65535,65536,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case *p=cases+ci;unsigned bytes=p->width/8;
  for(unsigned k=0;k<512;k++){
   for(unsigned i=0;i<16;i++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;a[1+i]=seed;b[1+i]=seed>>8;c[1+i]=seed>>16;}
   if(k<256)for(unsigned i=0;i<16/bytes;i++){uint64_t x=edge[(k/16+i)%16],y=edge[(k+i)%16],z=edge[(k*3+i)%16];memcpy(a+1+i*bytes,&x,bytes);memcpy(b+1+i*bytes,&y,bytes);memcpy(c+1+i*bytes,&z,bytes);}
   for(unsigned qc=0;qc<2;qc++){
    uint64_t saved,s1,s2,wanted=(uint64_t)qc<<27;__asm__ volatile("mrs %0,fpsr":"=r"(saved));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->ref(a+1,b+1,c+1,expected);__asm__ volatile("mrs %0,fpsr":"=r"(s1));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");memset(actual,0xcd,sizeof(actual));p->f(a+1,b+1,c+1,actual+1);__asm__ volatile("mrs %0,fpsr":"=r"(s2));
    __asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
    if(memcmp(expected,actual+1,p->resultbytes)||s1!=s2||actual[0]!=0xcd||actual[1+p->resultbytes]!=0xcd){fprintf(stderr,"%s k=%u qc=%u fpsr=%llx/%llx\n",p->name,k,qc,(unsigned long long)s1,(unsigned long long)s2);return 1;}
   }
  }
  size_t page=sysconf(_SC_PAGESIZE);unsigned char *m=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(m==MAP_FAILED||mprotect(m+page,page,PROT_NONE))return 2;memcpy(m+page-16,a+1,16);p->f(m+page-16,b+1,c+1,actual+1);p->f(a+1,m+page-16,c+1,actual+1);p->f(a+1,b+1,m+page-16,actual+1);p->f(a+1,b+1,c+1,m+page-p->resultbytes);if(memcmp(expected,m+page-p->resultbytes,p->resultbytes))return 3;unsigned char alias[16];memcpy(alias,a+1,16);p->f(alias,b+1,c+1,alias);if(memcmp(expected,alias,p->resultbytes))return 4;munmap(m,2*page);
 }
 return 0;}
"#;
    (ir, c)
}
#[test]
fn long_and_narrow_intrinsics_match_acle_and_qc() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let prepared = matrix::PreparedModule::parse(&source);
    let m = prepared.for_selector(SelectionTier::Baseline);
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-long-narrow");
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
}
