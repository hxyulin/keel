//! Every modified-immediate encoding pattern has an executed IR bit-expansion witness.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::{Type, VectorType};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod common;
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
fn fixture(immediates: &[u64]) -> (String, String, usize, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned result,a,b,c,width,fp;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    for record in include_str!("../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|s| s.contains("_asimdimm_"))
    {
        records += 1;
        let parts = record.split('_').collect::<Vec<_>>();
        let op = parts[0];
        let tag = parts[2..].join("_");
        let fp = op == "FMOV";
        let destructive = op == "BIC" || op == "ORR";
        let (width, shifts, qs, is_scalar) = match tag.as_str() {
            "L_hl" => (2, vec![0, 8], vec![0, 1], false),
            "L_sl" => (4, vec![0, 8, 16, 24], vec![0, 1], false),
            "M_sm" => (4, vec![8, 16], vec![0, 1], false),
            "N_b" => (1, vec![0], vec![0, 1], false),
            "D_ds" => (8, vec![0], vec![0], true),
            "D2_d" => (8, vec![0], vec![1], false),
            "S_s" => (4, vec![0], vec![0, 1], false),
            _ => unreachable!("{record}"),
        };
        for q in qs {
            let bytes = 8 << q;
            let int_ty = if is_scalar {
                Type::I64
            } else {
                vector(bytes, width, false)
            };
            let ty = if fp {
                vector(bytes, width, true)
            } else {
                int_ty
            };
            for &shift in &shifts {
                for &imm in immediates {
                    let raw = if fp {
                        let b6 = (imm >> 6) & 1;
                        if width == 4 {
                            ((imm >> 7) << 31)
                                | ((1 - b6) << 30)
                                | (if b6 == 1 { 0x1f << 25 } else { 0 })
                                | ((imm & 63) << 19)
                        } else {
                            ((imm >> 7) << 63)
                                | ((1 - b6) << 62)
                                | (if b6 == 1 { 0xffu64 << 54 } else { 0 })
                                | ((imm & 63) << 48)
                        }
                    } else if width == 8 {
                        let mut bits = 0;
                        for i in 0..8 {
                            if (imm >> i) & 1 == 1 {
                                bits |= 255u64 << (i * 8);
                            }
                        }
                        bits
                    } else {
                        let pattern = (imm << shift)
                            | if tag == "M_sm" {
                                (1u64 << shift) - 1
                            } else {
                                0
                            };
                        let mask = (1u64 << (8 * width)) - 1;
                        if op == "MVNI" {
                            !pattern & mask
                        } else {
                            pattern
                        }
                    };
                    let name = format!("imm_{record}_{q}_{shift}_{imm}");
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
                    );
                    let lane = if is_scalar {
                        Type::I64
                    } else {
                        int_ty.vector().unwrap().lane_type()
                    };
                    let signed = match width {
                        1 => raw as i8 as i64,
                        2 => raw as i16 as i64,
                        4 => raw as i32 as i64,
                        8 => raw as i64,
                        _ => unreachable!(),
                    };
                    ir += &format!(" v10 = iconst.{lane} {signed}\n");
                    let mut result = if is_scalar {
                        "v10".to_string()
                    } else {
                        ir += &format!(" v11 = vsplat.{int_ty} v10\n");
                        "v11".to_string()
                    };
                    if destructive {
                        ir += &format!(" v12 = load.{int_ty} align 1 v0\n");
                        if op == "BIC" {
                            ir += " v13 = vbnot v11\n v14 = vand v12,v13\n";
                        } else {
                            ir += " v14 = vor v12,v11\n";
                        }
                        result = "v14".into();
                    }
                    if fp {
                        ir += &format!(" v15 = bitcast.{ty} {result}\n");
                        result = "v15".into();
                    }
                    ir += &format!(" store align 1 {result},v3\n return\n}}\n");
                    let reg = if bytes == 8 { 'd' } else { 'q' };
                    let dst = if is_scalar {
                        "d0".to_string()
                    } else {
                        format!("v0.{}", arr(ty))
                    };
                    let immediate = if fp {
                        if width == 4 {
                            format!("{:?}", f32::from_bits(raw as u32))
                        } else {
                            format!("{:?}", f64::from_bits(raw))
                        }
                    } else if width == 8 {
                        format!("0x{raw:x}")
                    } else {
                        imm.to_string()
                    };
                    let mut asm = if destructive {
                        format!("ldr {reg}0,[%x0]\\n")
                    } else {
                        String::new()
                    };
                    asm += &format!("{} {dst},#{immediate}", op.to_ascii_lowercase());
                    if matches!(tag.as_str(), "L_hl" | "L_sl" | "M_sm") {
                        asm += &format!(",{} #{shift}", if tag == "M_sm" { "msl" } else { "lsl" });
                    }
                    asm += &format!("\\nstr {reg}0,[%x3]");
                    c += &format!(
                        "extern void {name}(const void*,const void*,const void*,void*);\nstatic void ref_{name}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"v0\");}}\n"
                    );
                    cases += &format!(
                        "{{{name},ref_{name},{},{},0,0,{width},0,\"{record}/q{q}/shift{shift}/imm{imm}\"}},\n",
                        ty.bytes().unwrap(),
                        if destructive { bytes } else { 0 }
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
uint32_t edge32[]={0,0x80000000,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7f800001,0x7fc00042,1,0x007fffff,0x3f000000};
for(unsigned k=0;k<sizeof(cases)/sizeof(*cases);k++){const struct C*p=cases+k;unsigned char*a=ae-p->a,*b=be-p->b,*c=ce-p->c,*out=oe-p->result;
for(unsigned rm=0;rm<4;rm++){uint64_t cr=(savedcr&~(3ull<<22))|((uint64_t)rm<<22);__asm__ volatile("msr fpcr,%0"::"r"(cr):"memory");
for(unsigned data=0;data<8;data++){unsigned char input[3][16],expected[16];unsigned sizes[]={p->a,p->b,p->c};for(unsigned j=0;j<3;j++)for(unsigned i=0;i<sizes[j];i+=p->width){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=data<42?edge64[(data*(2*j+1)+i)%21]:seed;if(p->fp&&p->width==4&&data<42)x=edge32[(data*(2*j+1)+i)%11];unsigned n=sizes[j]-i<p->width?sizes[j]-i:p->width;memcpy(input[j]+i,&x,n);}
for(unsigned qc=0;qc<2;qc++){uint64_t initial=((uint64_t)qc<<27)|(p->fp?0:0x9full);memcpy(a,input[0],p->a);memcpy(b,input[1],p->b);memcpy(c,input[2],p->c);status(initial);p->ref(a,b,c,expected);uint64_t wanted=flags();status(initial);out[-1]=0x5a;p->f(a,b,c,out);uint64_t actual=flags();
if(memcmp(out,expected,p->result)||actual!=wanted||out[-1]!=0x5a){__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);fprintf(stderr,"%s data %u rm %u qc %u flags %llx/%llx\n",p->name,data,rm,qc,(unsigned long long)actual,(unsigned long long)wanted);return 1;}
for(unsigned alias=0;alias<3;alias++){unsigned char aa[16],bb[16],cc[16];memcpy(aa,input[0],p->a);memcpy(bb,input[1],p->b);memcpy(cc,input[2],p->c);unsigned char*d=alias==0?aa:alias==1?bb:cc;status(initial);p->f(aa,bb,cc,d);assert(!memcmp(d,expected,p->result)&&flags()==wanted);}
}
}}}
__asm__ volatile("msr fpcr,%0"::"r"(savedcr):"memory");status(old);return 0;}
"#;
    (ir, c, records, combinations)
}
#[path = "common/matrix.rs"]
mod matrix;

#[test]
fn exhaustive_all_modified_immediate_patterns_have_native_expansion_witnesses() {
    check_modified_immediates(true);
}

#[test]
fn modified_immediate_boundaries_have_native_expansion_witnesses() {
    check_modified_immediates(false);
}

fn check_modified_immediates(exhaustive: bool) {
    use object::{Object, ObjectSection};
    let immediates = if exhaustive {
        (0..256).collect::<Vec<_>>()
    } else {
        vec![0, 1, 127, 128, 255]
    };
    let (source, c, records, combinations) = fixture(&immediates);
    assert_eq!(records, 15);
    eprintln!(
        "modified-immediate records {records}, exact shape/high/lane signatures {combinations}"
    );
    let prepared = matrix::PreparedModule::parse(&source);
    let dir = common::temp(if exhaustive {
        "neon-modified-immediates"
    } else {
        "neon-immediate-smoke"
    });
    fs::write(dir.join("oracle.c"), c).unwrap();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    common::command(
        Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o")),
    );
    let mut configurations = 0;
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let m = prepared.for_selector(selector);
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for (spilled, pools) in [(false, &keel_codegen::aarch64::REGISTERS), (true, &SPILL)] {
                // Sweep every value on both raw/register and optimized/spill paths.
                // The smaller boundary fixture covers every backend combination.
                if exhaustive
                    && !matches!(
                        (selector, allocator, spilled),
                        (SelectionTier::Baseline, AllocationTier::Fast, false)
                            | (SelectionTier::Full, AllocationTier::Full, true)
                    )
                {
                    continue;
                }
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &p).unwrap();
                configurations += 1;
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &p).unwrap();
                let bytes =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                assert_eq!(bytes, keel_asm::assemble_object(&text).unwrap());
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
    assert_eq!(configurations, if exhaustive { 2 } else { 12 });
    fs::remove_dir_all(dir).unwrap();
}
