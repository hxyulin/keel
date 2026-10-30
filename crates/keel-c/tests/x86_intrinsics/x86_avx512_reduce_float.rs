//! Floating reductions have a fixed lower-first tree and live-prefix operations.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, Target, ValueDef};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(vector_count),FN(mask_count);\nstatic __inline__ __attribute__((always_inline)) __m512 take_ps(const void*p){__m512 value;FN(vector_count)++;__builtin_memcpy(&value,p,64);return value;}\nstatic __inline__ __attribute__((always_inline)) __m512d take_pd(const void*p){__m512d value;FN(vector_count)++;__builtin_memcpy(&value,p,64);return value;}\nstatic __inline__ __attribute__((always_inline)) unsigned take_mask(unsigned k){FN(mask_count)++;return k;}\n",
    );
    let mut harness = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
extern int vector_count,mask_count,reference_vector_count,reference_mask_count;
typedef void(*Fn)(const void*,void*,unsigned);
struct Case{Fn actual,vendor;unsigned operation,width,masked;};
/* Each scalar instruction is sequenced independently of the packed shuffle
   recipe. No C arithmetic may reassociate these operations or quiet NaNs. */
static __attribute__((noinline)) float step_ps(unsigned op,float a,float b){switch(op){case 0:__asm__ volatile("addss %1,%0":"+x"(a):"xm"(b));break;case 1:__asm__ volatile("mulss %1,%0":"+x"(a):"xm"(b));break;case 2:__asm__ volatile("minss %1,%0":"+x"(a):"xm"(b));break;default:__asm__ volatile("maxss %1,%0":"+x"(a):"xm"(b));break;}return a;}
static __attribute__((noinline)) double step_pd(unsigned op,double a,double b){switch(op){case 0:__asm__ volatile("addsd %1,%0":"+x"(a):"xm"(b));break;case 1:__asm__ volatile("mulsd %1,%0":"+x"(a):"xm"(b));break;case 2:__asm__ volatile("minsd %1,%0":"+x"(a):"xm"(b));break;default:__asm__ volatile("maxsd %1,%0":"+x"(a):"xm"(b));break;}return a;}
static void oracle(const struct Case*c,const void*input,unsigned k,void*out){
 if(c->width==4){float values[16];uint32_t identity[]={0,0x3f800000,0x7f800000,0xff800000};for(unsigned i=0;i<16;i++)memcpy(&values[i],(k>>i)&1?(const char*)input+i*4:(const void*)&identity[c->operation],4);for(unsigned stride=8;stride;stride/=2)for(unsigned i=0;i<stride;i++)values[i]=step_ps(c->operation,values[i],values[i+stride]);memcpy(out,&values[0],4);}
 else{double values[8];uint64_t identity[]={0,UINT64_C(0x3ff0000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000)};for(unsigned i=0;i<8;i++)memcpy(&values[i],(k>>i)&1?(const char*)input+i*8:(const void*)&identity[c->operation],8);for(unsigned stride=4;stride;stride/=2)for(unsigned i=0;i<stride;i++)values[i]=step_pd(c->operation,values[i],values[i+stride]);memcpy(out,&values[0],8);}
}
static void initialize(void*input,unsigned width,unsigned sample){
 unsigned lanes=64/width;uint64_t posinf=width==4?UINT64_C(0x7f800000):UINT64_C(0x7ff0000000000000),neginf=width==4?UINT64_C(0xff800000):UINT64_C(0xfff0000000000000),negativezero=width==4?UINT64_C(0x80000000):UINT64_C(0x8000000000000000),qnan=width==4?UINT64_C(0x7fc12345):UINT64_C(0x7ff812345678abcd),snan=width==4?UINT64_C(0x7f812345):UINT64_C(0x7ff012345678abcd);
 for(unsigned i=0;i<lanes;i++){
  if(width==4){float value=sample==1?(i%3?1.0f:2.0f):(float)(sample==0?i%3+1:i+1);memcpy((char*)input+i*width,&value,width);}
  else{double value=sample==1?(i%3?1.0:2.0):(double)(sample==0?i%3+1:i+1);memcpy((char*)input+i*width,&value,width);}
 }
 if(sample<2)return;
 if(sample==2){memset(input,0,64);if(width==4){float a[]={0x1p100f,-0x1p100f,1,1};memcpy(input,a,sizeof a);}else{double a[]={0x1p900,-0x1p900,1,1};memcpy(input,a,sizeof a);}return;}
 if(sample==3){for(unsigned i=0;i<lanes;i++){if(width==4){float a=i<2?0x1p100f:i<4?0x1p-100f:1;memcpy((char*)input+i*width,&a,width);}else{double a=i<2?0x1p900:i<4?0x1p-900:1;memcpy((char*)input+i*width,&a,width);}}return;}
 if(sample==4){memset(input,0,64);for(unsigned i=0;i<lanes;i++)memcpy((char*)input+i*width,&negativezero,width);return;}
 if(sample==5||sample==6){memset(input,0,64);memcpy((char*)input+(sample==5?3:lanes-1)*width,&negativezero,width);return;}
 if(sample==7||sample==8||sample==9||sample==10){uint64_t value=sample<9?qnan:snan;unsigned lane=sample%2?3:lanes-1;memcpy((char*)input+lane*width,&value,width);return;}
 if(sample==11){memcpy(input,&posinf,width);memcpy((char*)input+width,&neginf,width);return;}
 if(sample==12){for(unsigned i=0;i<lanes;i++){uint64_t value=i&1?negativezero|1:1;memcpy((char*)input+i*width,&value,width);}return;}
 if(sample==13){for(unsigned i=0;i<lanes;i++){uint64_t value=i&1?negativezero|((width==4)?UINT64_C(0x00800000):UINT64_C(0x0010000000000000)):((width==4)?UINT64_C(0x00800000):UINT64_C(0x0010000000000000));memcpy((char*)input+i*width,&value,width);}return;}
 if(sample==14){memcpy(input,&snan,width);memcpy((char*)input+(lanes-1)*width,&qnan,width);return;}
 if(sample==15){for(unsigned i=0;i<lanes;i++)memcpy((char*)input+i*width,&snan,width);return;}
 if(sample==16){uint64_t value=width==4?UINT64_C(0x7f7fffff):UINT64_C(0x7fefffffffffffff);for(unsigned i=0;i<lanes;i++)memcpy((char*)input+i*width,&value,width);return;}
}
"#,
    );
    let mut entries = String::new();
    let mut number = 0;
    for (suffix, scalar, width) in [("ps", "float", 4), ("pd", "double", 8)] {
        for (operation, op) in ["add", "mul", "min", "max"].into_iter().enumerate() {
            for masked in [false, true] {
                let call = if masked {
                    format!("_mm512_mask_reduce_{op}_{suffix}(take_mask(k),take_{suffix}(input))")
                } else {
                    format!("_mm512_reduce_{op}_{suffix}(take_{suffix}(input))")
                };
                source += &format!(
                    "void FN(case_{number})(const void*input,void*out,unsigned k){{{scalar} result={call};__builtin_memcpy(out,&result,{width});}}\n"
                );
                harness += &format!(
                    "extern void case_{number}(const void*,void*,unsigned),reference_case_{number}(const void*,void*,unsigned);\n"
                );
                entries += &format!(
                    "{{case_{number},reference_case_{number},{operation},{width},{}}},\n",
                    usize::from(masked)
                );
                number += 1;
            }
        }
    }
    harness += &format!(
        r#"
static const struct Case cases[]={{{entries}}};
int main(void){{
 unsigned saved_mxcsr=_mm_getcsr();long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard,page,PROT_READ|PROT_WRITE))return 1;void*input=guard+page-64;unsigned char actual[16],expected[16],vendor[16];
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){{const struct Case*c=&cases[ci];unsigned lanes=64/c->width,all=(1u<<lanes)-1,masks[]={{0,all,0xa55a&all,1u<<(lanes-1),1}};
 for(unsigned sample=0;sample<17;sample++){{_mm_setcsr(0x1f80);initialize(input,c->width,sample);
 for(unsigned mode=0;mode<16;mode++){{unsigned controls=0x1f80|((mode&3)<<13)|((mode&4)?0x8000:0)|((mode&8)?0x40:0);
 for(unsigned pattern=0;pattern<5;pattern++){{unsigned k=masks[pattern],effective=c->masked?k:all;memset(actual,0xa5,sizeof actual);memset(expected,0xa5,sizeof expected);vector_count=mask_count=reference_vector_count=reference_mask_count=0;
 _mm_setcsr(controls);c->actual(input,actual,k);unsigned actual_mxcsr=_mm_getcsr();
 _mm_setcsr(controls);oracle(c,input,effective,expected);unsigned expected_mxcsr=_mm_getcsr();
 if(memcmp(actual,expected,c->width)||actual_mxcsr!=expected_mxcsr||vector_count!=1||mask_count!=c->masked){{uint64_t a=0,e=0;memcpy(&a,actual,c->width);memcpy(&e,expected,c->width);fprintf(stderr,"FP reduce case%u sample%u mode%u mask%u bits%llx/%llx mxcsr%x/%x eval%d/%d\n",ci,sample,mode,pattern,(unsigned long long)a,(unsigned long long)e,actual_mxcsr,expected_mxcsr,vector_count,mask_count);return 2;}}
 for(unsigned i=c->width;i<sizeof actual;i++)if(actual[i]!=0xa5)return 3;
 /* Small positive integers/powers of two are exactly representable for both
    vendor association choices. Exceptional inputs use the exact tree only. */
 if(sample<2&&mode==0){{_mm_setcsr(controls);c->vendor(input,vendor,k);unsigned vendor_mxcsr=_mm_getcsr();if(memcmp(actual,vendor,c->width)||vendor_mxcsr!=actual_mxcsr||reference_vector_count!=1||reference_mask_count!=c->masked)return 4;}}
 }}}}}}
 }}_mm_setcsr(saved_mxcsr);return 0;
}}
"#
    );
    (source, harness)
}
fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_float_reduction_tree_masks_and_feature_legality() {
    let (source, _) = sources();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let mut checked = 0;
    for function in module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("case_"))
    {
        let masks: Vec<_> = function
            .layout_insts()
            .filter_map(|(_, inst)| match function.inst_data(inst) {
                InstData::Intrinsic { id, args }
                    if matches!(
                        id,
                        Intrinsic::X86Avx512MaskAddps
                            | Intrinsic::X86Avx512MaskAddpd
                            | Intrinsic::X86Avx512MaskMulps
                            | Intrinsic::X86Avx512MaskMulpd
                            | Intrinsic::X86Avx512MaskMinps
                            | Intrinsic::X86Avx512MaskMinpd
                            | Intrinsic::X86Avx512MaskMaxps
                            | Intrinsic::X86Avx512MaskMaxpd
                    ) =>
                {
                    let value = function.resolve(args[1]);
                    let ValueDef::Result(def, _) = function.value_def(value) else {
                        panic!("tree mask must be constant")
                    };
                    let InstData::Iconst { imm, .. } = function.inst_data(def) else {
                        panic!("tree mask must be iconst")
                    };
                    Some(*imm)
                }
                _ => None,
            })
            .collect();
        let index = function
            .name
            .trim_start_matches("case_")
            .parse::<usize>()
            .unwrap();
        assert_eq!(
            masks,
            if index < 8 {
                vec![255, 15, 3, 1]
            } else {
                vec![15, 3, 1]
            },
            "{}",
            function.name
        );
        checked += 1;
    }
    assert_eq!(checked, 16);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .is_err()
    );
}
#[test]
fn avx512_float_reduction_native_tree_flags_and_vendor_exact_cases() {
    let (source, harness) = sources();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let directory =
        std::env::temp_dir().join(format!("keel-c-avx512-reduce-float-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.c"), source).unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    let native = {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    };
    if native {
        command(
            Command::new("cc")
                .args([
                    "-mavx512f",
                    "-mno-avx512dq",
                    "-mno-avx512bw",
                    "-O1",
                    "-DREFERENCE",
                    "-c",
                ])
                .arg(directory.join("source.c"))
                .arg("-o")
                .arg(directory.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args([
                    "-mavx512f",
                    "-O1",
                    "-fno-associative-math",
                    "-ffp-contract=off",
                    "-c",
                ])
                .arg(directory.join("harness.c"))
                .arg("-o")
                .arg(directory.join("harness.o")),
        );
    }
    for (tier, allocator, pools, optimize) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
    ] {
        let mut module = original.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &allocator,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(directory.join("actual.o"))
                    .arg(directory.join("reference.o"))
                    .arg(directory.join("harness.o"))
                    .arg("-o")
                    .arg(directory.join("run")),
            );
            command(&mut Command::new(directory.join("run")));
        }
    }
}
