//! Exact native witnesses for zero comparisons, scalar integer and bit recipes.
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
        let op = parts[0];
        let cls = parts[1];
        let tag = parts[2];
        let zero = matches!(cls, "asimdmisc" | "asisdmisc") && matches!(tag, "Z" | "FZ");
        let fp = tag == "FZ";
        let is_scalar = cls.starts_with("asisd");
        let scalar_int = is_scalar
            && matches!(
                op,
                "ABS"
                    | "ADD"
                    | "NEG"
                    | "SUB"
                    | "CMEQ"
                    | "CMGE"
                    | "CMGT"
                    | "CMHI"
                    | "CMHS"
                    | "CMTST"
            )
            && !zero;
        let bits = cls == "asimdsame" && matches!(op, "BIC" | "BIF" | "BIT" | "ORN" | "BSL");
        let shll = op == "SHLL";
        if !zero && !scalar_int && !bits && !shll {
            continue;
        }
        records += 1;
        let widths = if bits {
            vec![1]
        } else if scalar_int || (zero && is_scalar && !fp) {
            vec![8]
        } else if fp {
            vec![4, 8]
        } else if shll {
            vec![1, 2, 4]
        } else {
            vec![1, 2, 4, 8]
        };
        for width in widths {
            for bytes in if is_scalar {
                vec![width]
            } else if shll {
                vec![16]
            } else {
                vec![8, 16]
            } {
                if !is_scalar && !shll && width == 8 && bytes == 8 {
                    continue;
                }
                let variants = if shll || (zero && (!fp || op == "FCMEQ")) {
                    2
                } else {
                    1
                };
                for variant in 0..variants * if is_scalar && fp && width == 8 { 2 } else { 1 } {
                    let high = if shll { variant } else { 0 };
                    let reverse = zero && variant % variants == 1;
                    let wrapper = is_scalar && fp && width == 8 && variant >= variants;
                    let ty = if wrapper {
                        vector(8, 8, false)
                    } else if is_scalar {
                        scalar(width, false)
                    } else {
                        vector(bytes, if shll { width * 2 } else { width }, false)
                    };
                    let source = if fp {
                        if wrapper {
                            vector(8, 8, true)
                        } else if is_scalar {
                            scalar(width, true)
                        } else {
                            vector(bytes, width, true)
                        }
                    } else if shll {
                        vector(if high == 0 { 8 } else { 16 }, width, false)
                    } else {
                        ty
                    };
                    let name = format!("recipe_{record}_{ty}_{variant}");
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
                    );
                    let mut next = 10;
                    let mut emit = |rhs: String| {
                        let n = next;
                        next += 1;
                        ir += &format!(" v{n} = {rhs}\n");
                        format!("v{n}")
                    };
                    let a = emit(format!("load.{source} align 1 v0"));
                    let ab = source.bytes().unwrap();
                    let mut bb = 0;
                    let mut cb = 0;
                    let result = if zero {
                        let z = if fp {
                            emit(format!(
                                "fconst.{} 0.0",
                                source.vector().map_or(source, |v| v.lane_type())
                            ))
                        } else {
                            emit(format!(
                                "iconst.{} 0",
                                source.vector().map_or(source, |v| v.lane_type())
                            ))
                        };
                        let z = if source.is_vector() {
                            emit(format!("vsplat.{source} {z}"))
                        } else {
                            z
                        };
                        if fp {
                            let (stem, args) = match op {
                                "FCMEQ" => (
                                    "neon_fcmeq",
                                    if reverse {
                                        format!("{z},{a}")
                                    } else {
                                        format!("{a},{z}")
                                    },
                                ),
                                "FCMGE" => ("neon_fcmge", format!("{a},{z}")),
                                "FCMGT" => ("neon_fcmgt", format!("{a},{z}")),
                                "FCMLE" => ("neon_fcmge", format!("{z},{a}")),
                                "FCMLT" => ("neon_fcmgt", format!("{z},{a}")),
                                _ => unreachable!(),
                            };
                            emit(format!("{stem}.{ty}({args})"))
                        } else {
                            let cond = match op {
                                "CMEQ" => "eq",
                                "CMGE" => "sge",
                                "CMGT" => "sgt",
                                "CMLE" => "sle",
                                "CMLT" => "slt",
                                _ => unreachable!(),
                            };
                            let cond = if reverse {
                                match cond {
                                    "sge" => "sle",
                                    "sgt" => "slt",
                                    "sle" => "sge",
                                    "slt" => "sgt",
                                    _ => cond,
                                }
                            } else {
                                cond
                            };
                            let operands = if reverse {
                                format!("{z},{a}")
                            } else {
                                format!("{a},{z}")
                            };
                            let p = emit(format!(
                                "{}.{cond} {operands}",
                                if is_scalar { "icmp" } else { "vicmp" }
                            ));
                            if is_scalar {
                                {
                                    let p = emit(format!("uext.{ty} {p}"));
                                    emit(format!("ineg {p}"))
                                }
                            } else {
                                p
                            }
                        }
                    } else if scalar_int {
                        if op == "NEG" {
                            emit(format!("ineg {a}"))
                        } else if op == "ABS" {
                            let z = emit("iconst.i64 0".into());
                            let neg = emit(format!("ineg {a}"));
                            let cond = emit(format!("icmp.slt {a},{z}"));
                            emit(format!("select {cond},{neg},{a}"))
                        } else {
                            bb = ab;
                            let b = emit(format!("load.{source} align 1 v1"));
                            if op == "ADD" || op == "SUB" {
                                emit(format!(
                                    "{} {a},{b}",
                                    if op == "ADD" { "iadd" } else { "isub" }
                                ))
                            } else {
                                let cond = match op {
                                    "CMEQ" => "eq",
                                    "CMGE" => "sge",
                                    "CMGT" => "sgt",
                                    "CMHI" => "ugt",
                                    "CMHS" => "uge",
                                    "CMTST" => "ne",
                                    _ => unreachable!(),
                                };
                                let (a, b) = if op == "CMTST" {
                                    (emit(format!("and {a},{b}")), emit("iconst.i64 0".into()))
                                } else {
                                    (a, b)
                                };
                                let p = emit(format!("icmp.{cond} {a},{b}"));
                                {
                                    let p = emit(format!("uext.i64 {p}"));
                                    emit(format!("ineg {p}"))
                                }
                            }
                        }
                    } else if bits {
                        bb = ab;
                        let b = emit(format!("load.{source} align 1 v1"));
                        if op == "BIC" || op == "ORN" {
                            let b = emit(format!("vbnot {b}"));
                            emit(format!(
                                "{} {a},{b}",
                                if op == "BIC" { "vand" } else { "vor" }
                            ))
                        } else {
                            cb = ab;
                            let c = emit(format!("load.{source} align 1 v2"));
                            let (mask, left, right) = if op == "BSL" {
                                (a, b, c)
                            } else if op == "BIT" {
                                (c, b, a)
                            } else {
                                (c, a, b)
                            };
                            let not = emit(format!("vbnot {mask}"));
                            let yes = emit(format!("vand {mask},{left}"));
                            let no = emit(format!("vand {not},{right}"));
                            emit(format!("vor {yes},{no}"))
                        }
                    } else {
                        let wide = emit(format!(
                            "uwiden_{}.{ty} {a}",
                            if high == 0 { "low" } else { "high" }
                        ));
                        let count = emit(format!(
                            "iconst.{} {}",
                            ty.vector().unwrap().lane_type(),
                            width * 8
                        ));
                        let count = emit(format!("vsplat.{ty} {count}"));
                        emit(format!("vshl {wide},{count}"))
                    };
                    drop(emit);
                    ir += &format!(" store align 1 {result},v3\n return\n}}\n");
                    let class = |ty: Type| {
                        if ty.is_vector() {
                            if ty.bytes() == Some(16) { 'q' } else { 'd' }
                        } else {
                            match ty.bytes().unwrap() {
                                4 => 's',
                                8 => 'd',
                                _ => unreachable!(),
                            }
                        }
                    };
                    let mut asm = String::new();
                    let acc = bits && matches!(op, "BIT" | "BIF" | "BSL");
                    let ar = if acc { 0 } else { 1 };
                    asm += &format!("ldr {}{ar},[%x0]\\n", class(source));
                    if bb > 0 {
                        asm +=
                            &format!("ldr {}{},[%x1]\\n", class(source), if acc { 1 } else { 2 });
                    }
                    if cb > 0 {
                        asm += &format!("ldr {}2,[%x2]\\n", class(source));
                    }
                    let dst = if is_scalar {
                        format!("{}0", arr(scalar(width, false)))
                    } else if ty.is_vector() {
                        format!("v0.{}", arr(ty))
                    } else {
                        format!("{}0", arr(ty))
                    };
                    let left = if is_scalar {
                        format!("{}{ar}", arr(scalar(width, fp)))
                    } else if source.is_vector() {
                        format!("v{ar}.{}", arr(source))
                    } else {
                        format!("{}{ar}", arr(source))
                    };
                    asm += &format!(
                        "{}{} {dst},{left}",
                        op.to_ascii_lowercase(),
                        if high == 1 { "2" } else { "" }
                    );
                    if zero {
                        asm += if fp { ",#0.0" } else { ",#0" };
                    } else if shll {
                        asm += &format!(",#{}", width * 8);
                    } else if bb > 0 {
                        let r = 2;
                        asm += &if source.is_vector() {
                            format!(",v{r}.{}", arr(source))
                        } else {
                            format!(",{}{r}", arr(source))
                        };
                    }
                    // Accumulator encodings use destination as operand zero, then input1/input2.
                    if acc {
                        asm = asm.replace(
                            &format!("{dst},{left}"),
                            &format!("{dst},v1.{}", arr(source)),
                        );
                    }
                    asm += &format!("\\nstr {}0,[%x3]", class(ty));
                    c += &format!(
                        "extern void {name}(const void*,const void*,const void*,void*);\nstatic void ref_{name}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"v0\",\"v1\",\"v2\");}}\n"
                    );
                    cases += &format!(
                        "{{{name},ref_{name},{},{ab},{bb},{cb},{width},{},\"{record}/{ty}/high{high}\"}},\n",
                        ty.bytes().unwrap(),
                        u8::from(fp)
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
for(unsigned data=0;data<256;data++){unsigned char input[3][16],expected[16];unsigned sizes[]={p->a,p->b,p->c};for(unsigned j=0;j<3;j++)for(unsigned i=0;i<sizes[j];i+=p->width){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=data<42?edge64[(data*(2*j+1)+i)%21]:seed;if(p->fp&&p->width==4&&data<42)x=edge32[(data*(2*j+1)+i)%11];unsigned n=sizes[j]-i<p->width?sizes[j]-i:p->width;memcpy(input[j]+i,&x,n);}
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
fn pure_and_zero_form_records_have_native_execution_witnesses() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 36);
    eprintln!("recipe records {records}, exact shape/high/lane signatures {combinations}");
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-recipes");
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
                for record in include_str!("../../keel-asm/isa/neon.txt")
                    .lines()
                    .filter(|s| {
                        s.contains("_asimdmisc_Z")
                            || s.contains("_asimdmisc_FZ")
                            || s.contains("_asisdmisc_FZ")
                    })
                {
                    assert!(program.items.iter().any(|i|matches!(i,keel_asm_ir::Item::Instruction(i) if i.opcode.name()==record)), "zero selection missing {record}");
                }
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
    fs::remove_dir_all(dir).unwrap();
}
