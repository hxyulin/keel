//! Standard scalar mask3 FMA preserves C upper lanes; mask/maskz preserve A.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
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
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#include <string.h>\n#ifdef CLANG_REFERENCE\n#define FN(n) clang_reference_##n\n#elif defined(REFERENCE)\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\n#ifdef CLANG_REFERENCE\n#define HOLD(v) __asm__ volatile(\"\" : \"+v\"(v))\n#else\n#define HOLD(v)\n#endif\nunsigned FN(fma_evaluations)[4];\nstatic __m128 take_ss(const void*p,unsigned i){__m128 a;FN(fma_evaluations)[i]++;__builtin_memcpy(&a,p,16);HOLD(a);return a;}\nstatic __m128d take_sd(const void*p,unsigned i){__m128d a;FN(fma_evaluations)[i]++;__builtin_memcpy(&a,p,16);HOLD(a);return a;}\nstatic __mmask8 take_mask(unsigned k){FN(fma_evaluations)[3]++;return k;}\n",
    );
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\nextern unsigned fma_evaluations[4],reference_fma_evaluations[4],clang_reference_fma_evaluations[4];\ntypedef void(*Fn)(const void*,const void*,const void*,void*,unsigned);\nstruct Case{Fn actual,reference,clang_reference,hardware;unsigned bits,family,kind,sae;const char*name;};\n",
    );
    let mut rows = String::new();
    let mut index = 0;
    for (family, name) in ["fmadd", "fmsub", "fnmadd", "fnmsub"]
        .into_iter()
        .enumerate()
    {
        for (suffix, bits) in [("ss", 32), ("sd", 64)] {
            for (kind, mask) in ["mask", "mask3", "maskz"].into_iter().enumerate() {
                for round in [None, Some(8), Some(9), Some(10), Some(11)] {
                    let api = format!(
                        "_mm_{mask}_{name}{}{suffix}",
                        if round.is_some() { "_round_" } else { "_" }
                    );
                    let a = format!("take_{suffix}(a,0)");
                    let b = format!("take_{suffix}(b,1)");
                    let c = format!("take_{suffix}(c,2)");
                    let args = match kind {
                        0 => format!("{a},take_mask(k),{b},{c}"),
                        1 => format!("{a},{b},{c},take_mask(k)"),
                        _ => format!("take_mask(k),{a},{b},{c}"),
                    };
                    let args = if let Some(control) = round {
                        format!("{args},{control}")
                    } else {
                        args
                    };
                    let ct = if bits == 32 { "__m128" } else { "__m128d" };
                    source += &format!(
                        "void FN(fma_case_{index})(const void*a,const void*b,const void*c,void*out,unsigned k){{{ct} r={api}({args});__builtin_memcpy(out,&r,16);__builtin_memcpy((char*)out+64,a,16);__builtin_memcpy((char*)out+80,b,16);__builtin_memcpy((char*)out+96,c,16);}}\n"
                    );
                    harness += &format!(
                        "extern void fma_case_{index}(const void*,const void*,const void*,void*,unsigned),reference_fma_case_{index}(const void*,const void*,const void*,void*,unsigned),clang_reference_fma_case_{index}(const void*,const void*,const void*,void*,unsigned);\n"
                    );
                    let (destination, left, right, arrangement) = if kind == 1 {
                        ("c", "a", "b", 231)
                    } else {
                        ("a", "b", "c", 213)
                    };
                    let rounding = match round {
                        Some(8) => "%{rn-sae%}, ",
                        Some(9) => "%{rd-sae%}, ",
                        Some(10) => "%{ru-sae%}, ",
                        Some(11) => "%{rz-sae%}, ",
                        _ => "",
                    };
                    let zero = if kind == 2 { "%{z%}" } else { "" };
                    harness += &format!(
                        "static void hardware_{index}(const void*a,const void*b,const void*c,void*out,unsigned k){{{ct} d,x,y;memcpy(&d,{destination},16);memcpy(&x,{left},16);memcpy(&y,{right},16);__asm__ volatile(\"v{name}{arrangement}{suffix} {rounding}%[y],%[x],%[d]%{{%[k]%}}{zero}\":[d]\"+&v\"(d):[x]\"v\"(x),[y]\"v\"(y),[k]\"Yk\"((__mmask8)k):\"memory\");memcpy(out,&d,16);memcpy((char*)out+64,a,16);memcpy((char*)out+80,b,16);memcpy((char*)out+96,c,16);}}\n"
                    );
                    rows += &format!(
                        "{{fma_case_{index},reference_fma_case_{index},clang_reference_fma_case_{index},hardware_{index},{bits},{family},{kind},{},\"{api}\"}},\n",
                        u8::from(round.is_some())
                    );
                    index += 1;
                }
            }
        }
    }
    assert_eq!(index, 120);
    harness += &format!("static const struct Case cases[]={{{rows}}};\n");
    harness += r#"
int main(void){unsigned saved=_mm_getcsr();
 for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){
  const struct Case*t=cases+ci;unsigned bytes=t->bits/8;
  for(unsigned sample=0;sample<8;sample++){
   uint32_t a32[4]={0x40000000,0x7f812301,0x80000000,0xdeadbeef},b32[4]={0x40400000,0xff812302,1,0x11223344},c32[4]={0x40800000,0x7f812303,0,0x55667788};
   uint64_t a64[2]={UINT64_C(0x4000000000000000),UINT64_C(0x7ff0123456789001)},b64[2]={UINT64_C(0x4008000000000000),UINT64_C(0xfff0123456789002)},c64[2]={UINT64_C(0x4010000000000000),UINT64_C(0x7ff0123456789003)};
   if(sample==1){a32[0]^=0x80000000;c32[0]^=0x80000000;a64[0]^=UINT64_C(0x8000000000000000);c64[0]^=UINT64_C(0x8000000000000000);}
   if(sample>=2&&sample<=5){if(sample==2||sample==3){a32[0]=0x7f812311;a64[0]=UINT64_C(0x7ff0123456789111);}if(sample==2||sample==4){b32[0]=0x7f812322;b64[0]=UINT64_C(0x7ff0123456789222);}if(sample==2||sample==5){c32[0]=0x7f812333;c64[0]=UINT64_C(0x7ff0123456789333);}}
   if(sample==7){a32[0]=0x80000000;b32[0]=0x40000000;c32[0]=0;a64[0]=UINT64_C(0x8000000000000000);b64[0]=UINT64_C(0x4000000000000000);c64[0]=0;}
   const void*a=t->bits==32?(void*)a32:(void*)a64,*b=t->bits==32?(void*)b32:(void*)b64,*c=t->bits==32?(void*)c32:(void*)c64;
   const unsigned masks[]={0,1,0x80,0xfe,0xff};
   for(unsigned rounding=0;rounding<4;rounding++)for(unsigned denormal=0;denormal<4;denormal++)for(unsigned mi=0;mi<sizeof masks/sizeof(*masks);mi++){
    unsigned k=masks[mi],csr=0x1f80|0x20|(rounding<<13)|((denormal&1)<<6)|((denormal>>1)<<15);unsigned char out[160],reference[160],clang_reference[160],hardware[160];memset(out,0xa5,sizeof out);memset(reference,0xa5,sizeof reference);memset(hardware,0xa5,sizeof hardware);memset(clang_reference,0xa5,sizeof clang_reference);memset(fma_evaluations,0,16);memset(reference_fma_evaluations,0,16);memset(clang_reference_fma_evaluations,0,16);
    _mm_setcsr(csr);t->hardware(a,b,c,hardware,k);unsigned expected_flags=_mm_getcsr();_mm_setcsr(csr);t->reference(a,b,c,reference,k);unsigned vendor_flags=_mm_getcsr();_mm_setcsr(csr);t->clang_reference(a,b,c,clang_reference,k);unsigned clang_flags=_mm_getcsr();_mm_setcsr(csr);t->actual(a,b,c,out,k);unsigned flags=_mm_getcsr();
    unsigned active_nan=(k&1)&&sample>=2&&sample<=5;
    if(flags!=expected_flags||vendor_flags!=expected_flags||clang_flags!=expected_flags||memcmp(out,hardware,sizeof out)||memcmp(out+(active_nan?bytes:0),reference+(active_nan?bytes:0),sizeof out-(active_nan?bytes:0))||memcmp(out+(active_nan?bytes:0),clang_reference+(active_nan?bytes:0),sizeof out-(active_nan?bytes:0))){fprintf(stderr,"%s sample%u mask%x round%u denormal%u flags%x/%x\n",t->name,sample,k,rounding,denormal,flags,expected_flags);return 1;}
    for(unsigned i=0;i<4;i++)if(fma_evaluations[i]!=1||reference_fma_evaluations[i]!=1||clang_reference_fma_evaluations[i]!=1)return 2;
    if(active_nan){uint64_t vendor=0,clang=0;memcpy(&vendor,reference,bytes);memcpy(&clang,clang_reference,bytes);uint64_t exponent=t->bits==32?UINT64_C(0x7f800000):UINT64_C(0x7ff0000000000000),quiet=t->bits==32?UINT64_C(0x400000):UINT64_C(0x8000000000000);if((vendor&exponent)!=exponent||!(vendor&quiet)||(clang&exponent)!=exponent||!(clang&quiet))return 7;}
    const unsigned char*upper=t->kind==1?c:a;if(memcmp(out+bytes,upper+bytes,16-bytes))return 3;
    if(!(k&1)){uint64_t inactive=0;if(t->kind!=2)memcpy(&inactive,t->kind==1?c:a,bytes);if(memcmp(out,&inactive,bytes)||flags!=csr)return 4;}
    else if(t->sae&&flags!=csr)return 5;
    if((k&1)&&(sample<2||sample==6)){
     unsigned negproduct=t->family>=2,negaddend=t->family==1||t->family==3;
     if(t->bits==32){float x,y,z;memcpy(&x,a,4);memcpy(&y,b,4);memcpy(&z,c,4);float wanted=(negproduct?-x:x)*y+(negaddend?-z:z);if(memcmp(out,&wanted,4))return 6;}
     else{double x,y,z;memcpy(&x,a,8);memcpy(&y,b,8);memcpy(&z,c,8);double wanted=(negproduct?-x:x)*y+(negaddend?-z:z);if(memcmp(out,&wanted,8))return 6;}
    }
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
fn scalar_fma_mask3_upper_c_mask_and_maskz_upper_a_vendor_bits_flags_and_once() {
    let (source, harness) = sources();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.fma && !features.avx512dq);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "keel-scalar-fma-c-upper-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("runner.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512f");
    if native {
        // Clang21's unrounded maskz memory fold can emit A*C+B: 2*3+4
        // becomes 2*4+3. An opaque register barrier keeps the standard Clang
        // API oracle while preventing that fold. GCC is checked independently.
        command(
            Command::new("clang")
                .args(["-O1", "-mavx512f", "-DCLANG_REFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("clang-reference.o")),
        );
        command(
            Command::new("gcc")
                .args(["-O1", "-mavx512f", "-DREFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-mavx512f", "-ffp-contract=off", "-c"])
                .arg(dir.join("runner.c"))
                .arg("-o")
                .arg(dir.join("runner.o")),
        );
    }
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
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
                            .arg(dir.join("clang-reference.o"))
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
