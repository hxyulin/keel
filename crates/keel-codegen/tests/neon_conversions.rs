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
fn ctype(ty: Type, signed: bool) -> String {
    if let Some(v) = ty.vector() {
        let lane = v.lane_type();
        format!(
            "{}{}x{}_t",
            if lane.is_float() {
                "float"
            } else if signed {
                "int"
            } else {
                "uint"
            },
            lane.bytes().unwrap() * 8,
            v.lanes()
        )
    } else if ty == Type::F32 {
        "float".into()
    } else {
        format!(
            "{}{}_t",
            if signed { "int" } else { "uint" },
            ty.bytes().unwrap() * 8
        )
    }
}
fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <arm_neon.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <assert.h>\n#include <sys/mman.h>\n#include <unistd.h>\nstruct Case{void(*f)(const void*,void*);void(*ref)(const void*,void*);const char *name;unsigned srcwidth,kind,resultbytes,sourcebytes;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for &op in N::ALL
        .iter()
        .filter(|o| o.is_conversion() || o.is_reduction())
    {
        for ty in VectorType::ALL_64
            .into_iter()
            .chain(VectorType::ALL_128)
            .map(Type::Vector)
            .chain([Type::I16, Type::I32, Type::I64, Type::F32])
        {
            if !ty.is_vector() && op.is_conversion() {
                continue;
            }
            let Some(inputs) = op.operand_types(ty) else {
                continue;
            };
            let source = inputs[0].vector().unwrap();
            let width = source.lane_type().bytes().unwrap() * 8;
            let signed = !matches!(
                op,
                N::Ucvtf
                    | N::UcvtfFixed
                    | N::Fcvtau
                    | N::Fcvtmu
                    | N::Fcvtnu
                    | N::Fcvtpu
                    | N::Fcvtzu
                    | N::FcvtzuFixed
                    | N::Fcvthl
                    | N::Fcvthn
                    | N::Uaddlv
            );
            let shifts: Vec<_> = op
                .immediate_range(ty)
                .map(|r| r.collect())
                .unwrap_or(vec![0]);
            for n in shifts {
                let name = format!("{op}_{ty}_{n}");
                ir += &format!(
                    "func @{name}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{} align 1 v0\n",
                    inputs[0]
                );
                if inputs.len() == 2 {
                    ir += &format!(" v3 = iconst.i32 {n}\n");
                }
                ir += &format!(
                    " v4 = {op}.{ty}(v2{})\n store align 1 v4,v1\n return\n}}\n",
                    if inputs.len() == 2 { ",v3" } else { "" }
                );
                c += &format!(
                    "extern void {name}(const void*,void*);\n__attribute__((noinline)) static void ref_{name}(const void*a,void*out){{{} x;memcpy(&x,a,{});",
                    ctype(inputs[0], signed),
                    inputs[0].bytes().unwrap()
                );
                let resultwidth = ty.vector().map_or(ty, |s| s.lane_type()).bytes().unwrap() * 8;
                let call = match op {
                    N::Scvtf | N::Ucvtf => format!(
                        "vcvtq_f{resultwidth}_{}{width}(x)",
                        if signed { "s" } else { "u" }
                    ),
                    N::ScvtfFixed | N::UcvtfFixed => format!(
                        "vcvtq_n_f{resultwidth}_{}{width}(x,{n})",
                        if signed { "s" } else { "u" }
                    ),
                    N::Fcvtas
                    | N::Fcvtau
                    | N::Fcvtms
                    | N::Fcvtmu
                    | N::Fcvtns
                    | N::Fcvtnu
                    | N::Fcvtps
                    | N::Fcvtpu
                    | N::Fcvtzs
                    | N::Fcvtzu => {
                        let stem = match op {
                            N::Fcvtas | N::Fcvtau => "vcvtaq",
                            N::Fcvtms | N::Fcvtmu => "vcvtmq",
                            N::Fcvtns | N::Fcvtnu => "vcvtnq",
                            N::Fcvtps | N::Fcvtpu => "vcvtpq",
                            _ => "vcvtq",
                        };
                        format!(
                            "{stem}_{}{resultwidth}_f{width}(x)",
                            if signed { "s" } else { "u" }
                        )
                    }
                    N::FcvtzsFixed | N::FcvtzuFixed => format!(
                        "vcvtq_n_{}{resultwidth}_f{width}(x,{n})",
                        if signed { "s" } else { "u" }
                    ),
                    N::Fcvtl => "vcvt_f64_f32(vget_low_f32(x))".into(),
                    N::Fcvtn => "vcombine_f32(vcvt_f32_f64(x),vdup_n_f32(0))".into(),
                    N::Fcvtxn => "vcombine_f32(vcvtx_f32_f64(x),vdup_n_f32(0))".into(),
                    N::Fcvthl => "vcvt_f32_f16(vget_low_f16(vreinterpretq_f16_u16(x)))".into(),
                    N::Fcvthn => {
                        "vreinterpretq_u16_f16(vcombine_f16(vcvt_f16_f32(x),vdup_n_f16(0)))".into()
                    }
                    N::Fminv => "vminvq_f32(x)".into(),
                    N::Fmaxv => "vmaxvq_f32(x)".into(),
                    N::Fminnmv => "vminnmvq_f32(x)".into(),
                    N::Fmaxnmv => "vmaxnmvq_f32(x)".into(),
                    N::Saddlv | N::Uaddlv => {
                        format!("vaddlvq_{}{width}(x)", if signed { "s" } else { "u" })
                    }
                    _ => unreachable!(),
                };
                let call = if ty.vector().is_some_and(|v| v.bytes() == 8) {
                    match op {
                        N::Fcvtn => "vcvt_f32_f64(x)".to_string(),
                        N::Fcvtxn => "vcvtx_f32_f64(x)".to_string(),
                        N::Fcvthn => "vreinterpret_u16_f16(vcvt_f16_f32(x))".to_string(),
                        _ => call.replacen("q_", "_", 1),
                    }
                } else {
                    call
                };
                let bytes = ty.bytes().unwrap();
                c += &format!("{} r={call};memcpy(out,&r,{bytes});}}\n", ctype(ty, signed));
                let kind = if op == N::Fcvthl {
                    2
                } else if source.lane_type().is_float() {
                    1
                } else {
                    0
                };
                cases += &format!(
                    "{{{name},ref_{name},\"{name}\",{width},{kind},{bytes},{}}},\n",
                    inputs[0].bytes().unwrap()
                );
            }
        }
    }
    c += &cases;
    c += "};\n";
    c += r#"
static unsigned char* edge(void){long n=sysconf(_SC_PAGESIZE);unsigned char*p=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);assert(p!=MAP_FAILED);assert(!mprotect(p+n,n,PROT_NONE));return p+n;}
int main(void){unsigned char *ae=edge(),*oe=edge();unsigned char a[17],actual[18],expected[16];uint64_t seed=0x123456789abcdef,savedfpcr;__asm__ volatile("mrs %0,fpcr":"=r"(savedfpcr));
 const uint64_t edge[]={0,1,127,128,255,256,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 const uint16_t half[]={0,0x8000,0x3c00,0xbc00,0x3e00,0xbe00,0x7c00,0xfc00,0x7e01,0x7c01,1,0x8001,0x7bff,0x0400,0x3800};
 const uint32_t f32[]={0,0x80000000,0x3f800000,0xbf800000,0x3fc00000,0xbfc00000,0x7f800000,0xff800000,0x7fc12345,0x7f812345,1,0x80000001,0x7f7fffff,0x00800000,0x33800000};
 const uint64_t f64[]={0,0x8000000000000000,0x3ff0000000000000,0xbff0000000000000,0x3ff8000000000000,0xbff8000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc,0x7ff0123456789abc,1,0x8000000000000001,0x7fefffffffffffff,0x0010000000000000,0x3ca0000000000000};
 for(unsigned mode=0;mode<4;mode++){uint64_t fpcr=(savedfpcr&~(3ull<<22))|((uint64_t)mode<<22);__asm__ volatile("msr fpcr,%0"::"r"(fpcr):"memory");
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case *p=cases+ci;unsigned bytes=p->srcwidth/8;
  for(unsigned k=0;k<64;k++){
   for(unsigned i=0;i<16/bytes;i++){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=k<15?edge[(k+i)%15]:seed;if(k<15&&p->kind)x=p->kind==2?half[(k+i)%15]:bytes==4?f32[(k+i)%15]:f64[(k+i)%15];memcpy(a+1+i*bytes,&x,bytes);}
   for(unsigned qc=0;qc<2;qc++){uint64_t saved,s1,s2,wanted=(uint64_t)qc<<27;__asm__ volatile("mrs %0,fpsr":"=r"(saved));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->ref(a+1,expected);__asm__ volatile("mrs %0,fpsr":"=r"(s1));
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");memset(actual,0xcd,sizeof(actual));p->f(a+1,actual+1);__asm__ volatile("mrs %0,fpsr":"=r"(s2));
    unsigned char *ga=ae-p->sourcebytes,*go=oe-p->resultbytes;memcpy(ga,a+1,p->sourcebytes);go[-1]=0xa5;
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->f(ga,go);uint64_t guarded;__asm__ volatile("mrs %0,fpsr":"=r"(guarded));assert(!memcmp(go,expected,p->resultbytes)&&guarded==s1&&go[-1]==0xa5);
    __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");unsigned char alias[16];memcpy(alias,a+1,p->sourcebytes);p->f(alias,alias);__asm__ volatile("mrs %0,fpsr":"=r"(guarded));assert(!memcmp(alias,expected,p->resultbytes)&&guarded==s1);
    __asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
    if(memcmp(expected,actual+1,p->resultbytes)||s1!=s2||actual[0]!=0xcd||actual[1+p->resultbytes]!=0xcd){fprintf(stderr,"%s k=%u mode=%u qc=%u fpsr=%llx/%llx\n",p->name,k,mode,qc,(unsigned long long)s1,(unsigned long long)s2);return 1;}
   }
  }
 }}__asm__ volatile("msr fpcr,%0"::"r"(savedfpcr):"memory");return 0;}
"#;
    (ir, c)
}
#[test]
fn conversions_and_reductions_match_acle_all_modes_and_fractional_bits() {
    use object::{Object, ObjectSection};
    let (source, c) = fixture();
    let prepared = matrix::PreparedModule::parse(&source);
    let m = prepared.for_selector(SelectionTier::Baseline);
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    let dir = common::temp("neon-conversions");
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
