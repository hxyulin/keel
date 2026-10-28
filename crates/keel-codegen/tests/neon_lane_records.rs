//! All lane/transport/permutation records and every valid lane pairing.
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
enum Kind {
    Shuffle(Vec<u8>),
    SplatLane(u8),
    SplatGp,
    InsertGp(u8),
    InsertLane(u8, u8),
    Extract(u8, bool),
    ScalarDup(u8),
}
fn fixture() -> (String, String, usize, usize) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct C{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned result,a,b,c,width,fp;const char*name;};\n",
    );
    let mut cases = String::from("static const struct C cases[]={\n");
    let mut records = 0;
    let mut combinations = 0;
    let mut add = |record: &str,
                   ty: Type,
                   inputs: Vec<Type>,
                   kind: Kind,
                   raw: String,
                   width: u32| {
        let name = format!("lane_case_{combinations}");
        combinations += 1;
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
        let mut args = Vec::new();
        for (i, t) in inputs.iter().enumerate() {
            args.push(emit(format!("load.{t} align 1 v{i}")));
        }
        let result = match &kind {
            Kind::Shuffle(lanes) => {
                let indices = lanes
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                emit(format!("vshuffle {},{},[{indices}]", args[0], args[1]))
            }
            Kind::SplatLane(lane) => {
                let v = emit(format!("vextract {},{lane}", args[0]));
                emit(format!("vsplat.{ty} {v}"))
            }
            Kind::SplatGp => {
                let lane = ty.vector().unwrap().lane_type();
                let v = if lane != inputs[0] {
                    emit(format!("trunc.{lane} {}", args[0]))
                } else {
                    args[0].clone()
                };
                emit(format!("vsplat.{ty} {v}"))
            }
            Kind::InsertGp(lane) => {
                let scalar = ty.vector().unwrap().lane_type();
                let v = if scalar != inputs[1] {
                    emit(format!("trunc.{scalar} {}", args[1]))
                } else {
                    args[1].clone()
                };
                emit(format!("vinsert {},{v},{lane}", args[0]))
            }
            Kind::InsertLane(dst, src) => {
                let v = emit(format!("vextract {},{src}", args[1]));
                emit(format!("vinsert {},{v},{dst}", args[0]))
            }
            Kind::Extract(lane, signed) => {
                let v = emit(format!("vextract {},{lane}", args[0]));
                if inputs[0].vector().unwrap().lane_type() != ty {
                    emit(format!(
                        "{}.{ty} {v}",
                        if *signed { "sext" } else { "uext" }
                    ))
                } else {
                    v
                }
            }
            Kind::ScalarDup(lane) => emit(format!("vextract {},{lane}", args[0])),
        };
        drop(emit);
        ir += &format!(" store align 1 {result},v3\n return\n}}\n");
        let cl = |t: Type| {
            if t.is_vector() {
                if t.bytes() == Some(16) { 'q' } else { 'd' }
            } else {
                match t.bytes().unwrap() {
                    1 => 'b',
                    2 => 'h',
                    4 => 's',
                    8 => 'd',
                    _ => unreachable!(),
                }
            }
        };
        let gp = |t: Type| if t == Type::I64 { "x9" } else { "w9" };
        let mut asm = String::new();
        match &kind {
            Kind::Shuffle(_) => {
                asm += &format!(
                    "ldr {}1,[%x0]\\nldr {}2,[%x1]\\n",
                    cl(inputs[0]),
                    cl(inputs[1])
                )
            }
            Kind::SplatLane(_) | Kind::Extract(_, _) | Kind::ScalarDup(_) => {
                asm += "ldr q1,[%x0]\\n"
            }
            Kind::SplatGp => asm += &format!("ldr {},[%x0]\\n", gp(inputs[0])),
            Kind::InsertGp(_) => asm += &format!("ldr q0,[%x0]\\nldr {},[%x1]\\n", gp(inputs[1])),
            Kind::InsertLane(_, _) => asm += "ldr q0,[%x0]\\nldr q1,[%x1]\\n",
        }
        asm += &raw;
        asm += "\\n";
        if matches!(&kind, Kind::Extract(_, _)) {
            asm += &format!("str {},[%x3]", gp(ty));
        } else {
            asm += &format!("str {}0,[%x3]", cl(ty));
        }
        c += &format!(
            "extern void {name}(const void*,const void*,const void*,void*);\nstatic void ref_{name}(const void*a,const void*b,const void*c,void*out){{__asm__ volatile(\"{asm}\"::\"r\"(a),\"r\"(b),\"r\"(c),\"r\"(out):\"memory\",\"x9\",\"v0\",\"v1\",\"v2\");}}\n"
        );
        cases += &format!(
            "{{{name},ref_{name},{},{},{},0,{width},0,\"{record}/{ty}/case{combinations}\"}},\n",
            ty.bytes().unwrap(),
            inputs[0].bytes().unwrap(),
            inputs.get(1).map_or(0, |t| t.bytes().unwrap())
        );
    };
    for record in include_str!("../../keel-asm/isa/neon.txt")
        .lines()
        .filter(|s| {
            s.contains("_asimdperm_")
                || s.contains("_asimdext_")
                || s.contains("_asimdins_")
                || s.contains("_asisdone_")
        })
    {
        records += 1;
        let parts = record.split('_').collect::<Vec<_>>();
        let op = parts[0].to_ascii_lowercase();
        let cls = parts[1];
        let tag = parts[2..].join("_");
        if cls == "asimdperm" {
            for bytes in [8, 16] {
                for width in [1, 2, 4, 8] {
                    if bytes == 8 && width == 8 {
                        continue;
                    }
                    let ty = vector(bytes, width, false);
                    let n = ty.vector().unwrap().lanes();
                    let kind = match op.as_str() {
                        "zip1" => 0,
                        "zip2" => 1,
                        "uzp1" => 2,
                        "uzp2" => 3,
                        "trn1" => 4,
                        _ => 5,
                    };
                    let lanes = (0..n)
                        .map(|i| match kind {
                            0 | 1 => (kind as u8) * (n / 2) + i / 2 + (i % 2) * n,
                            2 | 3 => (i / (n / 2)) * n + 2 * (i % (n / 2)) + (kind as u8 - 2),
                            _ => (i / 2) * 2 + (kind as u8 - 4) + (i % 2) * n,
                        })
                        .collect();
                    add(
                        record,
                        ty,
                        vec![ty, ty],
                        Kind::Shuffle(lanes),
                        format!("{op} v0.{0},v1.{0},v2.{0}", arr(ty)),
                        width,
                    );
                }
            }
        } else if cls == "asimdext" {
            for bytes in [8, 16] {
                let ty = vector(bytes, 1, false);
                for offset in 0..bytes {
                    let lanes = (offset..offset + bytes).map(|x| x as u8).collect();
                    add(
                        record,
                        ty,
                        vec![ty, ty],
                        Kind::Shuffle(lanes),
                        format!("ext v0.{0},v1.{0},v2.{0},#{offset}", arr(ty)),
                        1,
                    );
                }
            }
        } else {
            for width in [1, 2, 4, 8] {
                let source = vector(16, width, false);
                let elem = arr(scalar(width, false));
                let n = 16 / width;
                match (cls, op.as_str(), tag.as_str()) {
                    ("asisdone", "dup", _) => {
                        for lane in 0..n {
                            let ty = scalar(width, false);
                            add(
                                record,
                                ty,
                                vec![source],
                                Kind::ScalarDup(lane as u8),
                                format!("dup {elem}0,v1.{elem}[{lane}]"),
                                width,
                            );
                        }
                    }
                    (_, "dup", "DV_v") => {
                        for bytes in [8, 16] {
                            if bytes == 8 && width == 8 {
                                continue;
                            }
                            let ty = vector(bytes, width, false);
                            for lane in 0..n {
                                add(
                                    record,
                                    ty,
                                    vec![source],
                                    Kind::SplatLane(lane as u8),
                                    format!("dup v0.{},v1.{elem}[{lane}]", arr(ty)),
                                    width,
                                );
                            }
                        }
                    }
                    (_, "dup", "DR_r") => {
                        for bytes in [8, 16] {
                            if bytes == 8 && width == 8 {
                                continue;
                            }
                            let ty = vector(bytes, width, false);
                            let gp = if width == 8 { Type::I64 } else { Type::I32 };
                            add(
                                record,
                                ty,
                                vec![gp],
                                Kind::SplatGp,
                                format!(
                                    "dup v0.{},{}9",
                                    arr(ty),
                                    if width == 8 { 'x' } else { 'w' }
                                ),
                                width,
                            );
                        }
                    }
                    (_, "ins", "IR_r") => {
                        let gp = if width == 8 { Type::I64 } else { Type::I32 };
                        for lane in 0..n {
                            add(
                                record,
                                source,
                                vec![source, gp],
                                Kind::InsertGp(lane as u8),
                                format!(
                                    "ins v0.{elem}[{lane}],{}9",
                                    if width == 8 { 'x' } else { 'w' }
                                ),
                                width,
                            );
                        }
                    }
                    (_, "ins", "IV_v") => {
                        for dst in 0..n {
                            for src in 0..n {
                                add(
                                    record,
                                    source,
                                    vec![source, source],
                                    Kind::InsertLane(dst as u8, src as u8),
                                    format!("ins v0.{elem}[{dst}],v1.{elem}[{src}]"),
                                    width,
                                );
                            }
                        }
                    }
                    (_, "smov", "W_w") if width <= 2 => {
                        for lane in 0..n {
                            add(
                                record,
                                Type::I32,
                                vec![source],
                                Kind::Extract(lane as u8, true),
                                format!("smov w9,v1.{elem}[{lane}]"),
                                width,
                            );
                        }
                    }
                    (_, "smov", "X_x") if width <= 4 => {
                        for lane in 0..n {
                            add(
                                record,
                                Type::I64,
                                vec![source],
                                Kind::Extract(lane as u8, true),
                                format!("smov x9,v1.{elem}[{lane}]"),
                                width,
                            );
                        }
                    }
                    (_, "umov", "W_w") if width <= 4 => {
                        for lane in 0..n {
                            add(
                                record,
                                Type::I32,
                                vec![source],
                                Kind::Extract(lane as u8, false),
                                format!("umov w9,v1.{elem}[{lane}]"),
                                width,
                            );
                        }
                    }
                    (_, "umov", "X_x") if width == 8 => {
                        for lane in 0..n {
                            add(
                                record,
                                Type::I64,
                                vec![source],
                                Kind::Extract(lane as u8, false),
                                format!("umov x9,v1.{elem}[{lane}]"),
                                width,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    drop(add);
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
fn every_lane_permutation_record_and_legal_lane_pair_executes() {
    use object::{Object, ObjectSection};
    let (source, c, records, combinations) = fixture();
    assert_eq!(records, 16);
    eprintln!(
        "lane permutation records {records}, exact shape/high/lane signatures {combinations}"
    );
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let dir = common::temp("neon-lane-records");
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
                for opcode in [
                    "ZIP1_asimdperm_only",
                    "ZIP2_asimdperm_only",
                    "UZP1_asimdperm_only",
                    "UZP2_asimdperm_only",
                    "TRN1_asimdperm_only",
                    "TRN2_asimdperm_only",
                ] {
                    assert!(program.items.iter().any(|i|matches!(i,keel_asm_ir::Item::Instruction(i) if i.opcode.name()==opcode)),"missing direct permutation {opcode}");
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
