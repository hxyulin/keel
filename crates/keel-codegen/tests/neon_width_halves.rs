//! Every non-indexed widening/narrowing record, both halves and all narrow shifts.
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
fn vector(bytes: u32, width: u32, fp: bool) -> Type {
    Type::Vector(
        VectorType::ALL_64
            .into_iter()
            .chain(VectorType::ALL_128)
            .find(|v| {
                v.bytes() == bytes
                    && v.lane_type().bytes() == Some(width)
                    && v.lane_type().is_float() == fp
            })
            .unwrap(),
    )
}
fn arr(ty: Type) -> String {
    let lane = ty.vector().map_or(ty, |v| v.lane_type());
    let e = match lane.bytes().unwrap() {
        1 => 'b',
        2 => 'h',
        4 => 's',
        8 => 'd',
        _ => unreachable!(),
    };
    ty.vector()
        .map_or(e.to_string(), |v| format!("{}{e}", v.lanes()))
}
fn fixture() -> (String, String, usize, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned result,a,b,c,width,fp;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    for record in include_str!("../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let parts = record.split('_').collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let op_name = parts[0];
        let cls = parts[1];
        let tag = parts[2];
        let narrow = (cls == "asimddiff" && tag == "N")
            || (cls == "asimdshf" && tag == "N")
            || (cls == "asimdmisc"
                && tag == "N"
                && matches!(op_name, "SQXTN" | "UQXTN" | "SQXTUN" | "FCVTN" | "FCVTXN"));
        let long = (cls == "asimddiff" && tag != "N") || (cls == "asimdmisc" && op_name == "FCVTL");
        if !narrow && !long {
            continue;
        }
        records += 1;
        let fp = op_name.starts_with("FCVT");
        let widths = if op_name == "PMULL" {
            vec![1]
        } else if matches!(
            op_name,
            "SQDMULL" | "SQDMLAL" | "SQDMLSL" | "FCVTL" | "FCVTN"
        ) {
            vec![2, 4]
        } else if op_name == "FCVTXN" {
            vec![4]
        } else {
            vec![1, 2, 4]
        };
        for width in widths {
            let name = if op_name == "FCVTL" && width == 2 {
                "neon_fcvthl".to_string()
            } else if op_name == "FCVTN" && width == 2 {
                "neon_fcvthn".to_string()
            } else {
                format!("neon_{}", op_name.to_ascii_lowercase())
            };
            let op: N = name.parse().unwrap();
            let half_ty = if narrow {
                vector(8, width, fp && width == 4)
            } else {
                vector(16, width * 2, fp)
            };
            let result_low = half_ty;
            let inputs = op.operand_types(result_low).unwrap();
            let immediate = op.immediate_range(result_low);
            let counts = immediate.clone().map_or(vec![None], |r| {
                immediates::boundaries(r).into_iter().map(Some).collect()
            });
            for high in 0..2 {
                for &imm in &counts {
                    let ty = if narrow && high == 1 {
                        vector(16, width, fp && width == 4)
                    } else {
                        result_low
                    };
                    let label = format!("halves_{record}_{ty}_{width}_{high}_{}", imm.unwrap_or(0));
                    ir += &format!(
                        "func @{label}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
                    );
                    let mut next = 10;
                    let mut emit = |rhs: String| {
                        let n = next;
                        next += 1;
                        ir += &format!(" v{n} = {rhs}\n");
                        format!("v{n}")
                    };
                    let mut actual = Vec::new();
                    let old = if narrow && high == 1 {
                        actual.push(half_ty);
                        Some(emit(format!("load.{half_ty} align 1 v0")))
                    } else {
                        None
                    };
                    let offset = usize::from(old.is_some());
                    let mut args = Vec::new();
                    let mut values = Vec::new();
                    let value_types = &inputs[..inputs.len() - usize::from(imm.is_some())];
                    for (i, &expected) in value_types.iter().enumerate() {
                        let v = expected.vector().unwrap();
                        let source = if long
                            && v.lane_type().bytes().unwrap()
                                < ty.vector().unwrap().lane_type().bytes().unwrap()
                        {
                            vector(
                                if high == 0 { 8 } else { 16 },
                                v.lane_type().bytes().unwrap(),
                                v.lane_type().is_float(),
                            )
                        } else {
                            expected
                        };
                        actual.push(source);
                        let value = emit(format!("load.{source} align 1 v{}", i + offset));
                        values.push((value, source, expected));
                    }
                    for (value, source, expected) in values {
                        let mut value = value;
                        if long
                            && source.vector().unwrap().lane_type().bytes().unwrap()
                                < ty.vector().unwrap().lane_type().bytes().unwrap()
                        {
                            let shape = source.vector().unwrap();
                            if high == 1 {
                                let n = shape.lanes();
                                let lanes = (n / 2..n)
                                    .chain(n / 2..n)
                                    .map(|x| x.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                value = emit(format!("vshuffle {value},{value},[{lanes}]"));
                            } else {
                                let lane = shape.lane_type();
                                let zero = emit(if lane.is_float() {
                                    format!("fconst.{lane} 0.0")
                                } else {
                                    format!("iconst.{lane} 0")
                                });
                                let mut q = emit(format!("vsplat.{expected} {zero}"));
                                for i in 0..shape.lanes() {
                                    let v = emit(format!("vextract {value},{i}"));
                                    q = emit(format!("vinsert {q},{v},{i}"));
                                }
                                value = q;
                            }
                        }
                        args.push(value);
                    }
                    if let Some(imm) = imm {
                        args.push(emit(format!("iconst.i32 {imm}")));
                    }
                    let mut result = emit(format!("{op}.{result_low}({})", args.join(",")));
                    if let Some(old) = old {
                        let lane = half_ty.vector().unwrap().lane_type();
                        let zero = emit(if lane.is_float() {
                            format!("fconst.{lane} 0.0")
                        } else {
                            format!("iconst.{lane} 0")
                        });
                        let mut q = emit(format!("vsplat.{ty} {zero}"));
                        let n = half_ty.vector().unwrap().lanes();
                        for i in 0..n {
                            let v = emit(format!("vextract {old},{i}"));
                            q = emit(format!("vinsert {q},{v},{i}"));
                            let v = emit(format!("vextract {result},{i}"));
                            q = emit(format!("vinsert {q},{v},{}", n + i));
                        }
                        result = q;
                    }
                    drop(emit);
                    ir += &format!(" store align 1 {result},v3\n return\n}}\n");
                    let cl = |t: Type| if t.bytes() == Some(16) { 'q' } else { 'd' };
                    let accumulator = long && value_types.len() == 3;
                    let mut asm = String::new();
                    for (i, &source) in actual.iter().enumerate() {
                        let r = if offset == 1 || accumulator { i } else { i + 1 };
                        asm += &format!("ldr {}{r},[%x{i}]\\n", cl(source));
                    }
                    let left = if accumulator {
                        actual[1]
                    } else {
                        actual[offset]
                    };
                    asm += &format!(
                        "{}{} v0.{},v1.{}",
                        op_name.to_ascii_lowercase(),
                        if high == 1 { "2" } else { "" },
                        arr(ty),
                        arr(left)
                    );
                    if value_types.len() - usize::from(accumulator) == 2 {
                        asm += &format!(
                            ",v2.{}",
                            arr(actual[if accumulator { 2 } else { offset + 1 }])
                        );
                    }
                    if let Some(imm) = imm {
                        asm += &format!(",#{imm}");
                    }
                    asm += &format!("\\nstr {}0,[%x3]", cl(ty));
                    c += &format!(
                        "extern void {label}(const void*,const void*,const void*,void*);\nstatic void ref_{label}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"v0\",\"v1\",\"v2\");}}\n"
                    );
                    let source_width = value_types
                        .last()
                        .unwrap()
                        .vector()
                        .unwrap()
                        .lane_type()
                        .bytes()
                        .unwrap();
                    cases += &format!(
                        "{{{label},ref_{label},{},{},{},{},{source_width},{},\"{record}/{ty}/high{high}/imm{}\"}},\n",
                        ty.bytes().unwrap(),
                        actual[0].bytes().unwrap(),
                        actual.get(1).map_or(0, |t| t.bytes().unwrap()),
                        actual.get(2).map_or(0, |t| t.bytes().unwrap()),
                        u8::from(fp),
                        imm.unwrap_or(0)
                    );
                    combinations += 1;
                }
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char*edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
static void status(uint64_t r){__asm__ volatile("msr fpsr,%0"::"r"(r):"memory");}
static uint64_t flags(void){uint64_t r;__asm__ volatile("mrs %0,fpsr":"=r"(r));return r;}
int main(void){unsigned char*ae=edge(),*be=edge(),*ce=edge(),*oe=edge();uint64_t savedcr,old=flags(),seed=0x123456789abcdefull;__asm__ volatile("mrs %0,fpcr":"=r"(savedcr));
uint64_t edge64[]={0,1,127,128,255,32767,32768,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffffull,0x8000000000000000ull,~0ull,0x3ff0000000000000ull,0xbff0000000000000ull,0x7ff0000000000000ull,0xfff0000000000000ull,0x7ff0000000000001ull,0x7ff8000000000042ull,1,0x000fffffffffffffull};
uint16_t half[]={0,0x8000,0x3c00,0xbc00,0x7c00,0xfc00,0x7c01,0x7e01,1,0x0400,0x03ff};
uint32_t edge32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7f800001,0x7fc00042,1,0x007fffff,0x3f000000};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++){const struct C*p=cases+k;unsigned char*a=ae-p->a,*b=be-p->b,*c=ce-p->c,*out=oe-p->result;
for(unsigned rm=0;rm<4;rm++){uint64_t cr=(savedcr&~(3ull<<22))|((uint64_t)rm<<22);__asm__ volatile("msr fpcr,%0"::"r"(cr):"memory");
for(unsigned data=0;data<256;data++){unsigned char input[3][16],expected[16];unsigned sizes[]={p->a,p->b,p->c};for(unsigned j=0;j<3;j++)for(unsigned i=0;i<sizes[j];i+=p->width){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=data<42?edge64[(data*(2*j+1)+i)%21]:seed;if(p->fp&&data<42){if(p->width==4)x=edge32[(data*(2*j+1)+i)%11];else if(p->width==2)x=half[(data*(2*j+1)+i)%11];}unsigned n=sizes[j]-i<p->width?sizes[j]-i:p->width;memcpy(input[j]+i,&x,n);}
for(unsigned qc=0;qc<2;qc++){uint64_t initial=((uint64_t)qc<<27)|(p->fp?0:0x9full);memcpy(a,input[0],p->a);memcpy(b,input[1],p->b);memcpy(c,input[2],p->c);status(initial);p->ref(a,b,c,expected);uint64_t wanted=flags();status(initial);out[-1]=0x5a;p->f(a,b,c,out);uint64_t actual=flags();
if(memcmp(out,expected,p->result)||actual!=wanted||out[-1]!=0x5a){__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);fprintf(stderr,"%s data %u rm %u qc %u flags %llx/%llx\n",p->name,data,rm,qc,(unsigned long long)actual,(unsigned long long)wanted);return 1;}
for(unsigned alias=0;alias<3;alias++){unsigned char aa[16],bb[16],cc[16];memcpy(aa,input[0],p->a);memcpy(bb,input[1],p->b);memcpy(cc,input[2],p->c);unsigned char*d=alias==0?aa:alias==1?bb:cc;status(initial);p->f(aa,bb,cc,d);assert(!memcmp(d,expected,p->result)&&flags()==wanted);}
}
}}}
__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);return 0;}
"#;
    (ir, c, records, combinations)
}
#[test]
fn every_nonindexed_width_record_executes_both_halves_and_immediate_boundaries() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 40);
    eprintln!("width-changing records {records}, exact shape/high/lane signatures {combinations}");
    let prepared = matrix::PreparedModule::parse(&source);
    let dir = common::temp("neon-width-halves");
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
