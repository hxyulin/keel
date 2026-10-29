//! Architectural intrinsics checked against independently compiled ACLE calls.
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
fn acle(op: N, width: u32, bits: u32) -> (String, String, String) {
    let suffix = if matches!(
        op,
        N::Fadd
            | N::Fsub
            | N::Fmul
            | N::Fdiv
            | N::Fabs
            | N::Fneg
            | N::Fsqrt
            | N::Frintn
            | N::Frintp
            | N::Frintm
            | N::Frintz
            | N::Fmin
            | N::Fmax
            | N::Faddp
            | N::Fminp
            | N::Fmaxp
            | N::Fminnmp
            | N::Fmaxnmp
            | N::Fcmeq
            | N::Fcmge
            | N::Fcmgt
            | N::Facge
            | N::Facgt
            | N::Fmla
            | N::Fmls
            | N::Fmulx
            | N::Fabd
            | N::Frecpe
            | N::Frsqrte
            | N::Frecps
            | N::Frsqrts
            | N::Frinta
            | N::Frintx
            | N::Frinti
            | N::Fminnm
            | N::Fmaxnm
    ) {
        format!("f{width}")
    } else if op == N::Pmul {
        "p8".into()
    } else if matches!(
        op,
        N::Uqadd
            | N::Uqsub
            | N::Usqadd
            | N::Ushl
            | N::Uqshl
            | N::Uqrshl
            | N::Urshl
            | N::Urecpe
            | N::Ursqrte
    ) {
        format!("u{width}")
    } else {
        format!("s{width}")
    };
    let stem = match op {
        N::Fadd => "vaddq",
        N::Fsub => "vsubq",
        N::Fmul => "vmulq",
        N::Fdiv => "vdivq",
        N::Fabs => "vabsq",
        N::Fneg => "vnegq",
        N::Fsqrt => "vsqrtq",
        N::Frintn => "vrndnq",
        N::Frintp => "vrndpq",
        N::Frintm => "vrndmq",
        N::Frintz => "vrndq",
        N::Fmin => "vminq",
        N::Fmax => "vmaxq",
        N::Faddp => "vpaddq",
        N::Fminp => "vpminq",
        N::Fmaxp => "vpmaxq",
        N::Fminnmp => "vpminnmq",
        N::Fmaxnmp => "vpmaxnmq",
        N::Fcmeq => "vceqq",
        N::Fcmge => "vcgeq",
        N::Fcmgt => "vcgtq",
        N::Facge => "vcageq",
        N::Facgt => "vcagtq",
        N::Sqadd | N::Uqadd => "vqaddq",
        N::Sqsub | N::Uqsub => "vqsubq",
        N::Suqadd => "vuqaddq",
        N::Usqadd => "vsqaddq",
        N::Sqabs => "vqabsq",
        N::Sqneg => "vqnegq",
        N::Sqshl | N::Uqshl => "vqshlq",
        N::Sshl | N::Ushl => "vshlq",
        N::Sqrshl | N::Uqrshl => "vqrshlq",
        N::Srshl | N::Urshl => "vrshlq",
        N::Sqdmulh => "vqdmulhq",
        N::Sqrdmulh => "vqrdmulhq",
        N::Fmla => "vfmaq",
        N::Fmls => "vfmsq",
        N::Fmulx => "vmulxq",
        N::Fabd => "vabdq",
        N::Frecpe | N::Urecpe => "vrecpeq",
        N::Frsqrte | N::Ursqrte => "vrsqrteq",
        N::Frecps => "vrecpsq",
        N::Frsqrts => "vrsqrtsq",
        N::Frinta => "vrndaq",
        N::Frintx => "vrndxq",
        N::Frinti => "vrndiq",
        N::Fminnm => "vminnmq",
        N::Fmaxnm => "vmaxnmq",
        N::Pmul => "vmulq",
        _ => unreachable!(),
    };
    let vector = |s: &str| {
        format!(
            "{}{}x{}_t",
            match s.as_bytes()[0] {
                b'f' => "float",
                b'u' => "uint",
                b'p' => "poly",
                _ => "int",
            },
            width,
            bits / width
        )
    };
    let first = vector(&suffix);
    let second = if matches!(op, N::Ushl | N::Uqshl | N::Uqrshl | N::Urshl | N::Usqadd) {
        format!("int{width}x{}_t", bits / width)
    } else if op == N::Suqadd {
        format!("uint{width}x{}_t", bits / width)
    } else {
        first.clone()
    };
    (
        format!(
            "{}_{suffix}",
            if bits == 64 {
                stem.strip_suffix('q').unwrap()
            } else {
                stem
            }
        ),
        first,
        second,
    )
}
fn fixture(bits: u32) -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <arm_neon.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nstruct Case{void(*f)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);const char *name;unsigned width,isfloat;};\n",
    );
    let mut cases = String::from("static const struct Case cases[]={\n");
    for &op in N::ALL.iter().filter(|o| {
        !o.is_integer_misc()
            && !o.is_status()
            && o.memory().is_none()
            && !o.is_table()
            && !o.is_conversion()
            && !o.is_reduction()
            && !o.changes_width()
            && o.immediate_range(Type::Vector(VectorType::I32x4)).is_none()
    }) {
        for shape in if bits == 64 {
            VectorType::ALL_64
        } else {
            VectorType::ALL_128
        } {
            if shape.lanes() == 1 || !op.accepts(Type::Vector(shape)) {
                continue;
            }
            let name = format!("{op}_{shape}");
            let width = shape.lane_type().bytes().unwrap() * 8;
            let inputs = op.operand_types(Type::Vector(shape)).unwrap();
            let args = (0..op.arity())
                .map(|i| format!("v{}", 4 + i))
                .collect::<Vec<_>>()
                .join(",");
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr):\n"
            );
            for (i, input) in inputs[..op.arity()].iter().enumerate() {
                ir += &format!(" v{} = load.{input} align 1 v{i}\n", 4 + i);
            }
            ir += &format!(" v8 = {op}.{shape}({args})\n store align 1 v8,v3\n return\n}}\n");
            c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
            let (acle, first, second) = acle(op, width, bits);
            let operands = match op.arity() {
                1 => "x",
                2 => "x,y",
                3 => "x,y,z",
                _ => unreachable!(),
            };
            let expression = if op == N::Fmls {
                // ACLE permits Clang to commute FMLS multiplicands, changing NaN sign
                // and payload preference. This intrinsic specifies written ARM order.
                format!(
                    "__asm__ volatile(\"fmls %0.{}, %1.{}, %2.{}\":\"+w\"(x):\"w\"(y),\"w\"(z));r=x;",
                    if bits == 64 {
                        "2s"
                    } else if width == 32 {
                        "4s"
                    } else {
                        "2d"
                    },
                    if bits == 64 {
                        "2s"
                    } else if width == 32 {
                        "4s"
                    } else {
                        "2d"
                    },
                    if bits == 64 {
                        "2s"
                    } else if width == 32 {
                        "4s"
                    } else {
                        "2d"
                    }
                )
            } else {
                format!("r={acle}({operands});")
            };
            let returned = if op.is_float_comparison() {
                format!("uint{width}x{}_t", bits / width)
            } else {
                first.clone()
            };
            c += &format!(
                "__attribute__((noinline)) static void ref_{name}(const void*a,const void*b,const void*c,void*out){{{first} x,z;{returned} r;{second} y;memcpy(&x,a,16);memcpy(&y,b,16);memcpy(&z,c,16);{expression}memcpy(out,&r,16);}}\n"
            );
            cases += &format!(
                "{{{name},ref_{name},\"{name}\",{width},{}}},\n",
                u8::from(inputs[0].vector().unwrap().lane_type().is_float())
            );
        }
    }
    // The arithmetic value is intentionally dead; its QC update must survive DCE.
    ir += "func @qc_dead(i64,i8x16,i8x16) -> i64 {\nblock0(v0: i64,v1: i8x16,v2: i8x16):\n v3 = neon_swap_fpsr.i64(v0)\n v4 = neon_sqadd.i8x16(v1,v2)\n v5 = neon_read_fpsr.i64()\n v6 = neon_swap_fpsr.i64(v3)\n return v5\n}\nfunc @fpcr_swap(i64) -> i64 {\nblock0(v0: i64):\n v1 = neon_swap_fpcr.i64(v0)\n v2 = neon_read_fpcr.i64()\n v3 = neon_swap_fpcr.i64(v1)\n return v2\n}\n";
    c += "extern uint64_t qc_dead(uint64_t,int8x16_t,int8x16_t);\nextern uint64_t fpcr_swap(uint64_t);\n";
    c += &cases;
    c += "};\n";
    c += r#"
int main(void){
 unsigned char a[17],b[17],c[17],actual[18],expected[16];uint64_t seed=0x123456789abcdef;
 const uint64_t edge[]={0,1,7,8,15,16,31,32,63,64,127,128,129,255,256,32767,32768,65535,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffff,0x8000000000000000,UINT64_MAX};
 const uint32_t f32[]={0,0x80000000,0x3f800000,0xbf800000,0x3fc00000,0xbfc00000,0x7f800000,0xff800000,0x7fc12345,0x7f812345,1,0x80000001,0x7f7fffff,0x00800000,0x33800000};
 const uint64_t f64[]={0,0x8000000000000000,0x3ff0000000000000,0xbff0000000000000,0x3ff8000000000000,0xbff8000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc,0x7ff0123456789abc,1,0x8000000000000001,0x7fefffffffffffff,0x0010000000000000,0x3ca0000000000000};
 uint64_t saved_fpcr;__asm__ volatile("mrs %0,fpcr":"=r"(saved_fpcr));
 for(unsigned mode=0;mode<4;mode++){
  uint64_t fpcr=(saved_fpcr&~(3ull<<22))|((uint64_t)mode<<22);__asm__ volatile("msr fpcr,%0"::"r"(fpcr):"memory");
  if(fpcr_swap(fpcr)!=fpcr)return 1;
  for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){
   const struct Case *p=cases+ci;unsigned bytes=p->width/8;
   for(unsigned k=0;k<256;k++){
    for(unsigned i=0;i<16/bytes;i++){
     seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t x=k<24?edge[(k+i)%24]:seed;
     seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t y=k<24?edge[(k*3+i)%24]:seed;
     seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;uint64_t z=k<24?edge[(k*7+i)%24]:seed;
     if(p->isfloat&&k<225){x=bytes==4?f32[(k/15+i)%15]:f64[(k/15+i)%15];y=bytes==4?f32[(k+i)%15]:f64[(k+i)%15];z=bytes==4?f32[(k*3+i+2)%15]:f64[(k*3+i+2)%15];}
     memcpy(a+1+i*bytes,&x,bytes);memcpy(b+1+i*bytes,&y,bytes);memcpy(c+1+i*bytes,&z,bytes);
    }
    for(unsigned qc=0;qc<2;qc++){
     uint64_t saved,wanted=(uint64_t)qc<<27,s1,s2;__asm__ volatile("mrs %0,fpsr":"=r"(saved));
     __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");p->ref(a+1,b+1,c+1,expected);__asm__ volatile("mrs %0,fpsr":"=r"(s1));
     __asm__ volatile("msr fpsr,%0"::"r"(wanted):"memory");memset(actual,0xcd,sizeof(actual));p->f(a+1,b+1,c+1,actual+1);__asm__ volatile("mrs %0,fpsr":"=r"(s2));
     __asm__ volatile("msr fpsr,%0"::"r"(saved):"memory");
     if(memcmp(expected,actual+1,16)||s1!=s2||actual[0]!=0xcd||actual[17]!=0xcd){fprintf(stderr,"%s k=%u mode=%u qc=%u fpsr=%llx/%llx\n",p->name,k,mode,qc,(unsigned long long)s1,(unsigned long long)s2);return 2;}
    }
   }
  }
 }
 __asm__ volatile("msr fpcr,%0"::"r"(saved_fpcr):"memory");
 int8x16_t max=vdupq_n_s8(127),one=vdupq_n_s8(1);if(!(qc_dead(0,max,one)&(1ull<<27)))return 3;
 return 0;
}
"#;
    if bits == 64 {
        c = c
            .replace("memcpy(&x,a,16)", "memcpy(&x,a,8)")
            .replace("memcpy(&y,b,16)", "memcpy(&y,b,8)")
            .replace("memcpy(&z,c,16)", "memcpy(&z,c,8)")
            .replace("memcpy(out,&r,16)", "memcpy(out,&r,8)")
            .replace("i<16/bytes", "i<8/bytes")
            .replace(
                "memcmp(expected,actual+1,16)",
                "memcmp(expected,actual+1,8)",
            )
            .replace("actual[17]!=0xcd", "actual[9]!=0xcd");
    }
    (ir, c)
}
#[test]
fn architectural_neon_matches_acle_values_status_and_rounding() {
    use object::{Object, ObjectSection};
    for bits in [64, 128] {
        let (source, c) = fixture(bits);
        let prepared = matrix::PreparedModule::parse(&source);
        let m = prepared.for_selector(SelectionTier::Baseline);
        assert_eq!(
            keel_ir::parse(&m.to_string()).unwrap().to_string(),
            m.to_string()
        );
        assert!(keel_codegen::x86_64::compile(m).is_err());
        let dir = common::temp("neon-architectural");
        fs::write(dir.join("oracle.c"), c).unwrap();
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        common::command(
            Command::new("clang")
                .args(["-O2", "-ffp-contract=off", "-c"])
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
                for (spilled, pools) in [(false, &keel_codegen::aarch64::REGISTERS), (true, &SPILL)]
                {
                    let text =
                        keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
                    assert!(!text.contains(" = stack_addr"));
                    assert!(
                        text.contains("sqadd\t")
                            && text.contains("fmla\t")
                            && text.contains("mrs\t")
                            && text.contains("msr\t")
                    );
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
}
