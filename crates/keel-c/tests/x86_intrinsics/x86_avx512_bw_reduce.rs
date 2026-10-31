//! Portable narrow reductions preserve lane-width arithmetic and scalar ABI types.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{Target, Type};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[7],
    float_caller_saved: &[12],
    ..keel_codegen::x86_64::REGISTERS
};
const OPS: [(&str, &str, u8); 8] = [
    ("add", "epi", 0),
    ("mul", "epi", 1),
    ("and", "epi", 2),
    ("or", "epi", 3),
    ("min", "epi", 4),
    ("max", "epi", 5),
    ("min", "epu", 6),
    ("max", "epu", 7),
];
fn sources(widths: &[u32]) -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(evaluations);\nstatic __inline__ __attribute__((always_inline)) __m128i take128(const void*p){__m128i a;FN(evaluations)++;__builtin_memcpy(&a,p,16);return a;}\nstatic __inline__ __attribute__((always_inline)) __m256i take256(const void*p){__m256i a;FN(evaluations)++;__builtin_memcpy(&a,p,32);return a;}\nstatic __inline__ __attribute__((always_inline)) unsigned take_mask(unsigned k){FN(evaluations)++;return k;}\n",
    );
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\nextern int evaluations,reference_evaluations;\ntypedef void(*Fn)(const void*,void*,unsigned);\nstruct Case{Fn actual,reference;unsigned width,bits,op,masked,maskbits;const char*name;};\n",
    );
    let mut rows = String::new();
    let mut index = 0;
    for &width in widths {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        for bits in [8, 16] {
            for (op, sign, code) in OPS {
                for masked in [false, true] {
                    let api = format!(
                        "{prefix}_{}reduce_{op}_{sign}{bits}",
                        if masked { "mask_" } else { "" }
                    );
                    let result = match (bits, sign) {
                        (8, "epu") => "unsigned char",
                        (8, _) => "signed char",
                        (16, "epu") => "unsigned short",
                        _ => "short",
                    };
                    let maskbits = if width == 128 && bits == 16 && (op == "min" || op == "max") {
                        16
                    } else {
                        (width / bits).max(8)
                    };
                    let args = if masked {
                        format!("take_mask(k),take{width}(p)")
                    } else {
                        format!("take{width}(p)")
                    };
                    source += &format!(
                        "{result} FN(case_{index})(const void*p,unsigned k){{return {api}({args});}}\nint FN(promote_{index})(const void*p,unsigned k){{return {api}({args});}}\n"
                    );
                    harness += &format!(
                        "extern {result} case_{index}(const void*,unsigned),reference_case_{index}(const void*,unsigned);extern int promote_{index}(const void*,unsigned),reference_promote_{index}(const void*,unsigned);\nstatic void invoke_{index}(const void*p,void*out,unsigned k){{{result} value=case_{index}(p,k);int promoted=promote_{index}(p,k);memcpy(out,&value,{});memcpy((char*)out+4,&promoted,4);}}\nstatic void reference_invoke_{index}(const void*p,void*out,unsigned k){{{result} value=reference_case_{index}(p,k);int promoted=reference_promote_{index}(p,k);memcpy(out,&value,{});memcpy((char*)out+4,&promoted,4);}}\n",
                        bits / 8,
                        bits / 8
                    );
                    rows += &format!(
                        "{{invoke_{index},reference_invoke_{index},{width},{bits},{code},{},{maskbits},\"{api}\"}},\n",
                        u8::from(masked)
                    );
                    index += 1;
                }
            }
        }
    }
    harness += &format!("static const struct Case cases[]={{{rows}}};\n");
    harness += r#"
static unsigned model(const unsigned char*p,unsigned width,unsigned bits,unsigned op,unsigned mask){
 unsigned maximum=(1u<<bits)-1,sign=1u<<(bits-1),result=op==1?1:op==2||op==6?maximum:op==4?sign-1:op==5?sign:0;
 for(unsigned i=0;i<width/bits;i++)if((mask>>i)&1){unsigned value=0;memcpy(&value,p+i*(bits/8),bits/8);switch(op){
 case 0:result=(result+value)&maximum;break;case 1:result=(result*value)&maximum;break;case 2:result&=value;break;case 3:result|=value;break;
 case 4:if((value^sign)<(result^sign))result=value;break;case 5:if((value^sign)>(result^sign))result=value;break;
 case 6:if(value<result)result=value;break;case 7:if(value>result)result=value;break;}}
 return result;
}
int main(void){unsigned saved=_mm_getcsr();
 for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){
  const struct Case*c=cases+ci;unsigned lanes=c->width/c->bits,bytes=c->bits/8,maximum=(1u<<c->bits)-1,sign=1u<<(c->bits-1),active=lanes==32?UINT32_MAX:(1u<<lanes)-1;
  unsigned ignored=~active;unsigned masks[]={0,1,1u<<(lanes-1),active,0xaaaaaaaa,0x55555555,ignored,ignored|1,ignored|(1u<<(lanes-1)),0xffffffff};
  for(unsigned sample=0;sample<lanes+6;sample++){
   unsigned char input[64],original[64];memset(input,0xa5,sizeof input);
   for(unsigned i=0;i<lanes;i++){unsigned value=sample==0?0:sample==1?maximum:sample==2?(i+1)&maximum:sample==3?(i%2?sign:sign-1):sample==4?((0x9e37u*(i+1))&maximum):sample==5?(maximum-i*2)&maximum:i==sample-6?sign:1;memcpy(input+1+i*bytes,&value,bytes);}memcpy(original,input,sizeof input);
   for(unsigned mi=0;mi<(c->masked?sizeof masks/sizeof(*masks):1);mi++){
    unsigned k=c->masked?masks[mi]:active;unsigned effective=c->masked&&c->maskbits<32?k&((1u<<c->maskbits)-1):k;
    unsigned expected=model(input+1,c->width,c->bits,c->op,effective);int promoted=(c->op<6&&(expected&sign))?(int)expected-(1<<c->bits):(int)expected;
    unsigned char out[16],reference[16],wanted[16];memset(out,0xa5,sizeof out);memset(reference,0xa5,sizeof reference);memset(wanted,0xa5,sizeof wanted);memcpy(wanted,&expected,bytes);memcpy(wanted+4,&promoted,4);
    evaluations=reference_evaluations=0;unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);c->actual(input+1,out,k);if(_mm_getcsr()!=csr)return 1;c->reference(input+1,reference,k);
    int actual_promoted;memcpy(&actual_promoted,out+4,4);
    if(memcmp(out,wanted,16)||memcmp(out,reference,16)||memcmp(input,original,sizeof input)||evaluations!=2*(1+(int)c->masked)||reference_evaluations!=evaluations){fprintf(stderr,"%s sample%u mask%x got%02x%02x promoted%d expected%x/%d evaluations%d/%d\n",c->name,sample,k,out[1],out[0],actual_promoted,expected,promoted,evaluations,reference_evaluations);return 2;}
   }
  }
 }
 _mm_setcsr(saved);return 0;
}
"#;
    (source, harness)
}
fn command(cmd: &mut Command) {
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "{cmd:?}: {} {}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn bw_reduce_portable_permissions_and_exact_scalar_returns() {
    for widths in [&[128][..], &[256][..]] {
        let (source, _) = sources(widths);
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let cases: Vec<_> = module
            .functions
            .iter()
            .filter(|f| f.name.starts_with("case_"))
            .collect();
        assert_eq!(cases.len(), 32);
        for func in cases {
            let index: usize = func.name.strip_prefix("case_").unwrap().parse().unwrap();
            assert_eq!(
                func.sig.returns.iter().map(|p| p.ty).collect::<Vec<_>>(),
                vec![if (index / 16) % 2 == 0 {
                    Type::I8
                } else {
                    Type::I16
                }]
            );
        }
        for optimize in [false, true] {
            let mut module = module.clone();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let base = Features::for_level(CpuLevel::V1);
            for features in [base, base.with_features("avx").unwrap()] {
                let selector = FeatureSelector {
                    inner: &SelectionTier::Full,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                };
                let result =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline);
                assert_eq!(
                    result.is_ok(),
                    widths[0] == 128 || features.avx,
                    "width{} opt{optimize}: {result:?}",
                    widths[0]
                );
            }
        }
    }
}
#[test]
fn bw_reduce_native_vendor_scalar_identities_wrap_masks_and_spills() {
    let (source, harness) = sources(&[128, 256]);
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "keel-c-bw-reduce-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("runner.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512vl");
    if native {
        command(
            Command::new("clang")
                .args(["-O1", "-mavx512bw", "-mavx512vl", "-DREFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-c"])
                .arg(dir.join("runner.c"))
                .arg("-o")
                .arg(dir.join("runner.o")),
        );
    }
    let features = Features::for_level(CpuLevel::V1)
        .with_features("avx")
        .unwrap();
    assert!(!features.avx2 && !features.avx512f && !features.avx512bw && !features.avx512vl);
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE] {
            for optimize in [false, true] {
                let mut m = module.clone();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                let tier = if optimize {
                    SelectionTier::Full
                } else {
                    SelectionTier::Baseline
                };
                let selector = FeatureSelector {
                    inner: &tier,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                );
                fs::write(dir.join("actual.o"), object).unwrap();
                if native {
                    command(
                        Command::new("cc")
                            .arg(dir.join("actual.o"))
                            .arg(dir.join("reference.o"))
                            .arg(dir.join("runner.o"))
                            .arg("-o")
                            .arg(dir.join("run")),
                    );
                    command(&mut Command::new(dir.join("run")));
                }
            }
        }
    }
}
