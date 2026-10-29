//! Full legal-shape witnesses for portable baseline integer records.
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
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned result,a,b,c,width,fp;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    for record in include_str!("../../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let parts = record.split('_').collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let op = parts[0];
        let cls = parts[1];
        let binary = cls == "asimdsame"
            && matches!(
                op,
                "ADD"
                    | "SUB"
                    | "MUL"
                    | "AND"
                    | "ORR"
                    | "EOR"
                    | "CMEQ"
                    | "CMGE"
                    | "CMGT"
                    | "CMHI"
                    | "CMHS"
                    | "CMTST"
                    | "SABD"
                    | "UABD"
                    | "SHADD"
                    | "UHADD"
                    | "SRHADD"
                    | "URHADD"
                    | "SHSUB"
                    | "UHSUB"
                    | "SMIN"
                    | "SMAX"
                    | "UMIN"
                    | "UMAX"
                    | "SMINP"
                    | "SMAXP"
                    | "UMINP"
                    | "UMAXP"
                    | "ADDP"
            );
        let unary = cls == "asimdmisc"
            && matches!(
                op,
                "ABS"
                    | "NEG"
                    | "CLZ"
                    | "CNT"
                    | "NOT"
                    | "REV16"
                    | "REV32"
                    | "REV64"
                    | "SADDLP"
                    | "UADDLP"
                    | "XTN"
            );
        let reduce = cls == "asimdall"
            && matches!(
                op,
                "ADDV" | "SMINV" | "SMAXV" | "UMINV" | "UMAXV" | "SADDLV" | "UADDLV"
            );
        let long = cls == "asimdshf" && matches!(op, "SSHLL" | "USHLL");
        let pair = record == "ADDP_asisdpair_only";
        if !binary && !unary && !reduce && !long && !pair {
            continue;
        }
        records += 1;
        let bit = matches!(op, "AND" | "ORR" | "EOR" | "NOT");
        let narrow = op == "XTN";
        let pair_long = op == "SADDLP" || op == "UADDLP";
        let widths = if bit || op == "CNT" || op == "REV16" {
            vec![1]
        } else if op == "REV32" {
            vec![1, 2]
        } else if pair {
            vec![8]
        } else if matches!(
            op,
            "ADD"
                | "SUB"
                | "ADDP"
                | "ABS"
                | "NEG"
                | "CMEQ"
                | "CMGE"
                | "CMGT"
                | "CMHI"
                | "CMHS"
                | "CMTST"
        ) {
            vec![1, 2, 4, 8]
        } else {
            vec![1, 2, 4]
        };
        for width in widths {
            for bytes in if pair || long || narrow {
                vec![16]
            } else {
                vec![8, 16]
            } {
                if !pair && !long && !narrow && !pair_long && width == 8 && bytes == 8 {
                    continue;
                }
                if reduce && width == 4 && bytes == 8 {
                    continue;
                }
                for high in 0..if long || narrow { 2 } else { 1 } {
                    for shift in 0..if long { width * 8 } else { 1 } {
                        let ty = if pair {
                            Type::I64
                        } else if reduce {
                            scalar(
                                width
                                    * if op == "SADDLV" || op == "UADDLV" {
                                        2
                                    } else {
                                        1
                                    },
                                false,
                            )
                        } else if long || pair_long {
                            vector(bytes, width * 2, false)
                        } else if narrow {
                            vector(if high == 0 { 8 } else { 16 }, width, false)
                        } else {
                            vector(bytes, width, false)
                        };
                        let source = if pair {
                            vector(16, 8, false)
                        } else if long {
                            vector(if high == 0 { 8 } else { 16 }, width, false)
                        } else if narrow {
                            vector(16, width * 2, false)
                        } else {
                            vector(bytes, width, false)
                        };
                        let name = format!("portable_{record}_{ty}_bytes{bytes}_{high}_{shift}");
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
                        let mut ab = source.bytes().unwrap();
                        let mut bb = 0;
                        let cb = 0;
                        let accumulator = if narrow && high == 1 {
                            ab = 8;
                            Some(emit(format!("load.{} align 1 v0", vector(8, width, false))))
                        } else {
                            None
                        };
                        let a = emit(format!(
                            "load.{source} align 1 v{}",
                            if accumulator.is_some() { 1 } else { 0 }
                        ));
                        if accumulator.is_some() {
                            bb = 16;
                        }
                        let result = if pair {
                            let a0 = emit(format!("vextract {a},0"));
                            let a1 = emit(format!("vextract {a},1"));
                            emit(format!("iadd {a0},{a1}"))
                        } else if reduce && (op == "SADDLV" || op == "UADDLV") {
                            let n: N = format!("neon_{}", op.to_ascii_lowercase()).parse().unwrap();
                            let expected = n.operand_types(ty).unwrap()[0];
                            let mut value = a;
                            if source != expected {
                                let lane = source.vector().unwrap().lane_type();
                                let zero = emit(format!("iconst.{lane} 0"));
                                let mut q = emit(format!("vsplat.{expected} {zero}"));
                                for i in 0..source.vector().unwrap().lanes() {
                                    let v = emit(format!("vextract {value},{i}"));
                                    q = emit(format!("vinsert {q},{v},{i}"));
                                }
                                value = q;
                            }
                            emit(format!("{n}.{ty}({value})"))
                        } else if reduce {
                            let stem = match op {
                                "ADDV" => "vreduce_add",
                                "SMINV" => "vreduce_smin",
                                "SMAXV" => "vreduce_smax",
                                "UMINV" => "vreduce_umin",
                                "UMAXV" => "vreduce_umax",
                                _ => unreachable!(),
                            };
                            emit(format!("{stem} {a}"))
                        } else if long {
                            let wide = emit(format!(
                                "{}widen_{}.{ty} {a}",
                                if op == "SSHLL" { "s" } else { "u" },
                                if high == 0 { "low" } else { "high" }
                            ));
                            let count = emit(format!(
                                "iconst.{} {shift}",
                                ty.vector().unwrap().lane_type()
                            ));
                            let count = emit(format!("vsplat.{ty} {count}"));
                            emit(format!("vshl {wide},{count}"))
                        } else if pair_long {
                            emit(format!(
                                "{}pairwise_add_long.{ty} {a}",
                                if op == "SADDLP" { "s" } else { "u" }
                            ))
                        } else if narrow {
                            let half = vector(8, width, false);
                            let value = emit(format!("narrow_low.{half} {a}"));
                            if let Some(ref old) = accumulator {
                                let zero =
                                    emit(format!("iconst.{} 0", ty.vector().unwrap().lane_type()));
                                let mut q = emit(format!("vsplat.{ty} {zero}"));
                                let n = half.vector().unwrap().lanes();
                                for i in 0..n {
                                    let v = emit(format!("vextract {old},{i}"));
                                    q = emit(format!("vinsert {q},{v},{i}"));
                                    let v = emit(format!("vextract {value},{i}"));
                                    q = emit(format!("vinsert {q},{v},{}", n + i));
                                }
                                q
                            } else {
                                value
                            }
                        } else if binary {
                            bb = ab;
                            let b = emit(format!("load.{source} align 1 v1"));
                            let stem = match op {
                                "ADD" => "viadd",
                                "SUB" => "visub",
                                "MUL" => "vimul",
                                "AND" => "vand",
                                "ORR" => "vor",
                                "EOR" => "vxor",
                                "ADDP" => "vpadd",
                                "SABD" => "vsabd",
                                "UABD" => "vuabd",
                                "SHADD" => "vshadd",
                                "UHADD" => "vuhadd",
                                "SRHADD" => "vsrhadd",
                                "URHADD" => "vurhadd",
                                "SHSUB" => "vshsub",
                                "UHSUB" => "vuhsub",
                                "SMIN" => "vsmin",
                                "SMAX" => "vsmax",
                                "UMIN" => "vumin",
                                "UMAX" => "vumax",
                                "SMINP" => "vpsmin",
                                "SMAXP" => "vpsmax",
                                "UMINP" => "vpumin",
                                "UMAXP" => "vpumax",
                                "CMEQ" => "vicmp.eq",
                                "CMGE" => "vicmp.sge",
                                "CMGT" => "vicmp.sgt",
                                "CMHI" => "vicmp.ugt",
                                "CMHS" => "vicmp.uge",
                                "CMTST" => "cmtst",
                                _ => unreachable!(),
                            };
                            if op == "CMTST" {
                                let bits = emit(format!("vand {a},{b}"));
                                let zero =
                                    emit(format!("iconst.{} 0", ty.vector().unwrap().lane_type()));
                                let zero = emit(format!("vsplat.{ty} {zero}"));
                                emit(format!("vicmp.ne {bits},{zero}"))
                            } else {
                                emit(format!("{stem} {a},{b}"))
                            }
                        } else {
                            match op {
                                "ABS" => {
                                    let zero = emit(format!(
                                        "iconst.{} 0",
                                        ty.vector().unwrap().lane_type()
                                    ));
                                    let zero = emit(format!("vsplat.{ty} {zero}"));
                                    let neg = emit(format!("vineg {a}"));
                                    let sign = emit(format!("vicmp.slt {a},{zero}"));
                                    emit(format!("vselect {sign},{neg},{a}"))
                                }
                                "REV16" | "REV32" | "REV64" => {
                                    let group = match op {
                                        "REV16" => 2,
                                        "REV32" => 4,
                                        _ => 8,
                                    } / width;
                                    let n = source.vector().unwrap().lanes();
                                    let list = (0..n as u32)
                                        .map(|i| {
                                            (i / group * group + group - 1 - i % group).to_string()
                                        })
                                        .collect::<Vec<_>>()
                                        .join(",");
                                    emit(format!("vshuffle {a},{a},[{list}]"))
                                }
                                _ => emit(format!(
                                    "{} {a}",
                                    match op {
                                        "NEG" => "vineg",
                                        "CLZ" => "vclz",
                                        "CNT" => "vpopcnt",
                                        "NOT" => "vbnot",
                                        _ => unreachable!(),
                                    }
                                )),
                            }
                        };
                        drop(emit);
                        ir += &format!(" store align 1 {result},v3\n return\n}}\n");
                        let class = |ty: Type| {
                            if ty.is_vector() {
                                if ty.bytes() == Some(16) { 'q' } else { 'd' }
                            } else {
                                match ty.bytes().unwrap() {
                                    1 => 'b',
                                    2 => 'h',
                                    4 => 's',
                                    8 => 'd',
                                    _ => unreachable!(),
                                }
                            }
                        };
                        let mut asm = String::new();
                        if accumulator.is_some() {
                            asm += "ldr d0,[%x0]\\n";
                        }
                        asm += &format!(
                            "ldr {}1,[%x{}]\\n",
                            class(source),
                            if accumulator.is_some() { 1 } else { 0 }
                        );
                        if binary {
                            asm += &format!("ldr {}2,[%x1]\\n", class(source));
                        }
                        let dst = if ty.is_vector() {
                            format!("v0.{}", arr(ty))
                        } else {
                            format!("{}0", arr(ty))
                        };
                        let left = if pair {
                            "v1.2d".to_string()
                        } else {
                            format!("v1.{}", arr(source))
                        };
                        asm += &format!(
                            "{}{} {dst},{left}",
                            op.to_ascii_lowercase(),
                            if high == 1 { "2" } else { "" }
                        );
                        if binary {
                            asm += &format!(",v2.{}", arr(source));
                        }
                        if long {
                            asm += &format!(",#{shift}");
                        }
                        asm += &format!("\\nstr {}0,[%x3]", class(ty));
                        c += &format!(
                            "extern void {name}(const void*,const void*,const void*,void*);\nstatic void ref_{name}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"v0\",\"v1\",\"v2\");}}\n"
                        );
                        cases += &format!(
                            "{{{name},ref_{name},{},{ab},{bb},{cb},{},0,\"{record}/{ty}/high{high}/shift{shift}\"}},\n",
                            ty.bytes().unwrap(),
                            source.vector().unwrap().lane_type().bytes().unwrap()
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
fn portable_integer_records_have_all_width_high_shift_witnesses() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 50);
    eprintln!(
        "portable integer records {records}, exact shape/high/lane signatures {combinations}"
    );
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-portable-records");
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
