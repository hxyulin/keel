//! All indexed records via lane extraction/broadcast and pinned intrinsics.
use crate::common;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
use std::{fs, process::Command};
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
fn scalar(width: u32, fp: bool) -> Type {
    vector(8, width, fp).vector().unwrap().lane_type()
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
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned result,source,width,fp;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    for record in include_str!("../../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|s| s.contains("_asimdelem_") || s.contains("_asisdelem_"))
    {
        records += 1;
        let parts = record.split('_').collect::<Vec<_>>();
        let mnemonic = parts[0].to_ascii_lowercase();
        let fp = mnemonic.starts_with('f');
        let is_scalar = parts[1] == "asisdelem";
        let long = parts[2] == "L";
        let op: Option<N> = format!("neon_{mnemonic}").parse().ok();
        assert!(op.is_some() || mnemonic == "mul");
        let acc = op.is_some_and(|op| op.arity() == 3);
        for width in if fp { [4, 8] } else { [2, 4] } {
            for bytes in if is_scalar || long {
                vec![if is_scalar {
                    width * if long { 2 } else { 1 }
                } else {
                    16
                }]
            } else {
                vec![8, 16]
            } {
                if !is_scalar && fp && width == 8 && bytes == 8 {
                    continue;
                }
                let ty = if is_scalar {
                    scalar(bytes, fp)
                } else {
                    vector(bytes, width * if long { 2 } else { 1 }, fp)
                };
                let inputs = op.map_or_else(|| vec![ty; 2], |op| op.operand_types(ty).unwrap());
                for high in 0..if long && !is_scalar { 2 } else { 1 } {
                    let source = if is_scalar {
                        scalar(width, fp)
                    } else if long {
                        vector(if high == 1 { 16 } else { 8 }, width, fp)
                    } else {
                        ty
                    };
                    let indexed = vector(16, width, fp);
                    for lane in 0..16 / width {
                        let name = format!("idx_{record}_{}_{}_{}", ty, high, lane);
                        let mut next = 10;
                        ir += &format!(
                            "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
                        );
                        let mut emit = |rhs: String| {
                            let n = next;
                            next += 1;
                            ir += &format!(" v{n} = {rhs}\n");
                            format!("v{n}")
                        };
                        let accumulator = if acc {
                            Some(emit(format!("load.{ty} align 1 v0")))
                        } else {
                            None
                        };
                        let mut a = emit(format!("load.{source} align 1 v1"));
                        let table = emit(format!("load.{indexed} align 1 v2"));
                        let b = emit(format!("vextract {table}, {lane}"));
                        let source_expected = inputs[usize::from(acc)];
                        if source != source_expected {
                            let shape = source.vector().unwrap();
                            if high == 1 {
                                let n = shape.lanes();
                                let lanes = (n / 2..n)
                                    .chain(n / 2..n)
                                    .map(|x| x.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                a = emit(format!("vshuffle {a},{a},[{lanes}]"));
                            } else {
                                let zero = emit(format!("iconst.{} 0", shape.lane_type()));
                                let mut q = emit(format!("vsplat.{source_expected} {zero}"));
                                for i in 0..shape.lanes() {
                                    let v = emit(format!("vextract {a},{i}"));
                                    q = emit(format!("vinsert {q},{v},{i}"));
                                }
                                a = q;
                            }
                        } else if high == 1 {
                            let n = source.vector().unwrap().lanes();
                            let lanes = (n / 2..n)
                                .chain(n / 2..n)
                                .map(|x| x.to_string())
                                .collect::<Vec<_>>()
                                .join(",");
                            a = emit(format!("vshuffle {a},{a},[{lanes}]"));
                        }
                        let b = if inputs.last().unwrap().is_vector() {
                            emit(format!("vsplat.{} {b}", inputs.last().unwrap()))
                        } else {
                            b
                        };
                        let mut args = Vec::new();
                        if let Some(a) = accumulator {
                            args.push(a);
                        }
                        args.push(a);
                        args.push(b);
                        let result = emit(if let Some(op) = op {
                            format!("{op}.{ty}({})", args.join(","))
                        } else {
                            format!("vimul {},{}", args[0], args[1])
                        });
                        drop(emit);
                        ir += &format!(" store align 1 {result},v3\n return\n}}\n");
                        let class = |ty: Type| {
                            if ty.is_vector() {
                                if ty.bytes() == Some(16) { 'q' } else { 'd' }
                            } else {
                                match ty.bytes().unwrap() {
                                    2 => 'h',
                                    4 => 's',
                                    8 => 'd',
                                    _ => unreachable!(),
                                }
                            }
                        };
                        let mut asm = String::new();
                        if acc {
                            asm += &format!("ldr {}0,[%x0]\\n", class(ty));
                        }
                        asm += &format!("ldr {}1,[%x1]\\nldr q2,[%x2]\\n", class(source));
                        let destination = if is_scalar {
                            format!("{}0", arr(ty))
                        } else {
                            format!("v0.{}", arr(ty))
                        };
                        let left = if is_scalar {
                            format!("{}1", arr(source))
                        } else {
                            format!("v1.{}", arr(source))
                        };
                        let element = arr(scalar(width, fp));
                        asm += &format!(
                            "{mnemonic}{} {destination},{left},v2.{element}[{lane}]\\nstr {}0,[%x3]",
                            if high == 1 { "2" } else { "" },
                            class(ty)
                        );
                        c += &format!(
                            "extern void {name}(const void*,const void*,const void*,void*);\nstatic void ref_{name}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"v0\",\"v1\",\"v2\");}}\n"
                        );
                        cases += &format!(
                            "{{{name},ref_{name},{},{},{width},{},\"{record}/{ty}/high{high}/lane{lane}\"}},\n",
                            ty.bytes().unwrap(),
                            source.bytes().unwrap(),
                            u8::from(fp)
                        );
                        combinations += 1;
                    }
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
uint32_t edge32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7f800001,0x7fc00042,1,0x007fffff,0x3f000000};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++){const struct C*p=cases+k;unsigned char*a=ae-p->result,*b=be-p->source,*c=ce-16,*out=oe-p->result;
for(unsigned rm=0;rm<4;rm++){uint64_t cr=(savedcr&~(3ull<<22))|((uint64_t)rm<<22);__asm__ volatile("msr fpcr,%0"::"r"(cr):"memory");
for(unsigned data=0;data<256;data++){unsigned char input[3][16],expected[16];unsigned sizes[]={p->result,p->source,16};for(unsigned j=0;j<3;j++)for(unsigned i=0;i<sizes[j];i+=p->width){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=data<42?edge64[(data*(2*j+1)+i)%21]:seed;if(p->fp&&p->width==4&&data<42)x=edge32[(data*(2*j+1)+i)%11];unsigned n=sizes[j]-i<p->width?sizes[j]-i:p->width;memcpy(input[j]+i,&x,n);}
for(unsigned qc=0;qc<2;qc++){uint64_t initial=(uint64_t)qc<<27;memcpy(a,input[0],p->result);memcpy(b,input[1],p->source);memcpy(c,input[2],16);status(initial);p->ref(a,b,c,expected);uint64_t wanted=flags();status(initial);out[-1]=0x5a;p->f(a,b,c,out);uint64_t actual=flags();
if(memcmp(out,expected,p->result)||actual!=wanted||out[-1]!=0x5a){__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);fprintf(stderr,"%s data %u rm %u qc %u flags %llx/%llx\n",p->name,data,rm,qc,(unsigned long long)actual,(unsigned long long)wanted);return 1;}
for(unsigned alias=0;alias<3;alias++){unsigned char aa[16],bb[16],cc[16];memcpy(aa,input[0],p->result);memcpy(bb,input[1],p->source);memcpy(cc,input[2],16);unsigned char*d=alias==0?aa:alias==1?bb:cc;status(initial);p->f(aa,bb,cc,d);assert(!memcmp(d,expected,p->result)&&flags()==wanted);}
}
}}}
__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);return 0;}
"#;
    (ir, c, records, combinations)
}
#[test]
fn all_indexed_records_shapes_high_halves_and_lanes_have_native_witnesses() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 27);
    eprintln!("indexed records {records}, exact shape/high/lane signatures {combinations}");
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-indexed");
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
