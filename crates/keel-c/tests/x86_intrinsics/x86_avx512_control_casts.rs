//! Public controls normalize through pinned C casts before strict validation.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Case {
    name: &'static str,
    ty: &'static str,
    expression: &'static str,
    valid: u32,
    invalid: u32,
}
fn cases() -> Vec<Case> {
    let rows = [
        (
            "vl_roundscale",
            "__m128",
            "_mm_roundscale_ps(AS,@C@)",
            0,
            256,
        ),
        (
            "wide_roundscale",
            "__m512",
            "_mm512_mask_roundscale_ps(SP,0x5555,AP,@C@)",
            0x31,
            256,
        ),
        (
            "scalar_roundscale",
            "__m128d",
            "_mm_roundscale_sd(AD,BD,@C@)",
            0x21,
            256,
        ),
        (
            "vl_fixup",
            "__m256d",
            "_mm256_maskz_fixupimm_pd(5,AYD,BYD,TY,@C@)",
            0xa5,
            256,
        ),
        (
            "wide_fixup",
            "__m512",
            "_mm512_mask_fixupimm_ps(AP,0x5555,BP,TI,@C@)",
            0xa5,
            256,
        ),
        (
            "scalar_fixup",
            "__m128",
            "_mm_maskz_fixupimm_ss(1,AS,BS,TX,@C@)",
            0xa5,
            256,
        ),
        ("vl_getmant", "__m128", "_mm_getmant_ps(AS,@C@,0)", 3, 16),
        (
            "wide_getmant",
            "__m512d",
            "_mm512_mask_getmant_pd(SD,0x55,ADP,@C@,0)",
            3,
            16,
        ),
        (
            "scalar_getmant",
            "__m128d",
            "_mm_getmant_sd(AD,BD,@C@,0)",
            3,
            16,
        ),
        (
            "special_sae",
            "__m512",
            "_mm512_mask_getexp_round_ps(SP,0x5555,AP,@C@)",
            8,
            5,
        ),
        (
            "scalar_sae",
            "__m128d",
            "_mm_mask_getexp_round_sd(SXD,1,AD,BD,@C@)",
            8,
            5,
        ),
        (
            "packed_rc",
            "__m512",
            "_mm512_mask_add_round_ps(SP,0x5555,AP,BP,@C@)",
            8,
            5,
        ),
        (
            "scalar_rc",
            "__m128",
            "_mm_mask_div_round_ss(SS,1,AS,BS,@C@)",
            8,
            5,
        ),
        (
            "packed_fma",
            "__m512",
            "_mm512_mask_fmadd_round_ps(AP,0x5555,BP,SP,@C@)",
            8,
            5,
        ),
        (
            "scalar_fma",
            "__m128",
            "_mm_mask3_fmadd_round_ss(AS,BS,SS,1,@C@)",
            8,
            5,
        ),
        (
            "fp_predicate",
            "__mmask16",
            "_mm512_mask_cmp_ps_mask(0x5555,AP,BP,@C@)",
            3,
            32,
        ),
        (
            "scalar_predicate",
            "__mmask8",
            "_mm_mask_cmp_ss_mask(1,AS,BS,@C@)",
            3,
            32,
        ),
        (
            "compare_sae",
            "__mmask8",
            "_mm_cmp_round_sd_mask(AD,BD,3,@C@)",
            8,
            5,
        ),
        (
            "int_predicate",
            "__mmask8",
            "_mm512_mask_cmp_epi64_mask(0x55,AI,BI,@C@)",
            6,
            8,
        ),
        (
            "vl_predicate",
            "__mmask8",
            "_mm_mask_cmp_epu32_mask(5,AX,BX,@C@)",
            6,
            8,
        ),
        (
            "bw_predicate",
            "__mmask64",
            "_mm512_mask_cmp_epi8_mask(0x5555555555555555ULL,AI,BI,@C@)",
            6,
            8,
        ),
        (
            "gather_scale",
            "__m512i",
            "_mm512_mask_i32gather_epi32(AI,0x5555,_mm512_setzero_si512(),base,@C@)",
            4,
            3,
        ),
        (
            "qword_scale",
            "__m512d",
            "_mm512_mask_i64gather_pd(SD,0x55,_mm512_setzero_si512(),base,@C@)",
            8,
            3,
        ),
        (
            "extract_lane",
            "__m128i",
            "_mm512_mask_extracti32x4_epi32(SX,5,AI,@C@)",
            3,
            4,
        ),
        (
            "insert_lane",
            "__m512",
            "_mm512_mask_insertf32x4(SP,0x5555,AP,BS,@C@)",
            3,
            4,
        ),
        (
            "permutation",
            "__m512d",
            "_mm512_mask_permutex_pd(SD,0x55,ADP,@C@)",
            0x1b,
            256,
        ),
        (
            "ternary",
            "__m512i",
            "_mm512_mask_ternarylogic_epi64(AI,0x55,BI,TI,@C@)",
            0x96,
            256,
        ),
        (
            "vl_ternary",
            "__m128i",
            "_mm_maskz_ternarylogic_epi32(5,AX,BX,TX,@C@)",
            0x96,
            256,
        ),
        (
            "rotation",
            "__m512i",
            "_mm512_mask_rol_epi32(AI,0x5555,BI,@C@)",
            31,
            256,
        ),
        (
            "mask_shift",
            "__mmask16",
            "_kshiftli_mask16(0x1234,@C@)",
            5,
            256,
        ),
        (
            "conversion_rc",
            "__m512i",
            "_mm512_mask_cvt_roundps_epi32(AI,0x5555,AP,@C@)",
            8,
            5,
        ),
        (
            "scalar_conversion",
            "long long",
            "_mm_cvt_roundss_i64(AS,@C@)",
            8,
            5,
        ),
        (
            "half_conversion",
            "__m256i",
            "_mm512_mask_cvtps_ph(SY,0x5555,AP,@C@)",
            5,
            256,
        ),
        (
            "dq_range",
            "__m256d",
            "_mm256_mask_range_pd(SYD,5,AYD,BYD,@C@)",
            7,
            16,
        ),
        (
            "dq_reduce",
            "__m512",
            "_mm512_mask_reduce_ps(SP,0x5555,AP,@C@)",
            0x31,
            256,
        ),
        (
            "bw_shuffle",
            "__m512i",
            "_mm512_mask_shufflehi_epi16(AI,0x55555555,BI,@C@)",
            0x1b,
            256,
        ),
    ];
    rows.into_iter()
        .map(|(name, ty, expression, valid, invalid)| Case {
            name,
            ty,
            expression,
            valid,
            invalid,
        })
        .collect()
}
const PREAMBLE: &str = "#include <immintrin.h>\n#define SP (*(__m512*)s)\n#define SD (*(__m512d*)s)\n#define SX (*(__m128i*)s)\n#define SS (*(__m128*)s)\n#define SXD (*(__m128d*)s)\n#define SY (*(__m256i*)s)\n#define SYD (*(__m256d*)s)\n#define AP (*(__m512*)a)\n#define ADP (*(__m512d*)a)\n#define AI (*(__m512i*)a)\n#define AS (*(__m128*)a)\n#define AD (*(__m128d*)a)\n#define AX (*(__m128i*)a)\n#define AYD (*(__m256d*)a)\n#define BP (*(__m512*)b)\n#define BI (*(__m512i*)b)\n#define BS (*(__m128*)b)\n#define BD (*(__m128d*)b)\n#define BX (*(__m128i*)b)\n#define BYD (*(__m256d*)b)\n#define TI (*(__m512i*)t)\n#define TX (*(__m128i*)t)\n#define TY (*(__m256i*)t)\n";
fn function(case: &Case, name: &str, control: &str) -> String {
    let expression = case.expression.replace("@C@", control);
    format!(
        "__attribute__((target(\"avx512f,avx512vl,avx512dq,avx512bw,avx512cd\"))) void {name}(const void*s,const void*a,const void*b,const void*t,const void*base,void*out,int runtime){{\n{} result={expression};\n__builtin_memcpy(out,&result,sizeof result);\n}}\n",
        case.ty
    )
}
fn directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "keel-control-casts-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn public_roundscale_normalizes_wide_control_before_validation() {
    let source = "#include <immintrin.h>\n__attribute__((target(\"avx512f,avx512vl\"))) __m128 f(__m128 a){return _mm_roundscale_ps(a,0x100000000ULL);}";
    let module = parse(source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let raw = "#include <immintrin.h>\n__attribute__((target(\"avx512f,avx512vl\"))) __m128 f(__m128 a){return __builtin_keel_avx512vl_roundscaleps128(a,(signed char)-1,a,0x100000000ULL);}";
    assert!(parse(raw, Target::X86_64LinuxGnu).is_err());
}
#[test]
fn public_control_casts_keep_strict_post_conversion_bounds() {
    let cases = cases();
    let mut vendor = String::from(PREAMBLE);
    let mut expected = Vec::new();
    for case in &cases {
        for (label, control) in [
            ("negative", String::from("0xffffffffULL")),
            ("outside", format!("(0x100000000ULL+{})", case.invalid)),
            ("runtime", String::from("runtime")),
        ] {
            // The pinned ternary wrappers first truncate to unsigned char.
            // Every constant is then in range; only runtime controls are invalid.
            if matches!(case.name, "ternary" | "vl_ternary") && label != "runtime" {
                continue;
            }
            let name = format!("{}_{}", case.name, label);
            let body = function(case, &name, &control);
            let source = format!("{PREAMBLE}{body}");
            assert!(
                parse(&source, Target::X86_64LinuxGnu).is_err(),
                "accepted {name}: {control}"
            );
            vendor += &format!("#line 1 \"{name}.c\"\n{body}");
            expected.push(name);
        }
    }
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let dir = directory();
        fs::write(dir.join("invalid.c"), vendor).unwrap();
        let output = Command::new("clang")
            .args(["-fsyntax-only", "-ferror-limit=0"])
            .arg(dir.join("invalid.c"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        for name in expected {
            assert!(
                diagnostic.contains(&format!("{name}.c:2:")),
                "vendor accepted {name}: {diagnostic}"
            );
        }
    }
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let _ = (vendor, expected);
}
#[test]
fn public_control_casts_match_vendor_bits_mxcsr_and_allocation() {
    let cases = cases();
    let mut source = String::from(PREAMBLE);
    let mut oracle = String::from(PREAMBLE);
    let mut declarations = String::new();
    let mut table = String::new();
    for case in &cases {
        let mut controls = vec![
            ("wide", format!("(0x100000000ULL+{})", case.valid)),
            ("negative_wrap", format!("(-4294967296LL+{})", case.valid)),
        ];
        if matches!(case.name, "ternary" | "vl_ternary") {
            controls.extend([
                ("byte_max", String::from("0xffffffffULL")),
                ("byte_wrap", String::from("0x100000100ULL")),
            ]);
        }
        for (label, control) in controls {
            let name = format!("{}_{}", case.name, label);
            let reference = format!("oracle_{name}");
            source += &function(case, &name, &control);
            oracle += &function(case, &reference, &control);
            declarations += &format!(
                "extern void {name}(const void*,const void*,const void*,const void*,const void*,void*,int);extern void {reference}(const void*,const void*,const void*,const void*,const void*,void*,int);\n"
            );
            let sanitize_kind = match case.name {
                "wide_getmant" => 1,
                "dq_range" => 2,
                _ => 0,
            };
            table += &format!(
                "{{\"{name}\",{name},{reference},sizeof({}),{sanitize_kind}}},\n",
                case.ty
            );
        }
    }
    let mut module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir = directory();
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    let runner = format!(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n{declarations}typedef void(*Fn)(const void*,const void*,const void*,const void*,const void*,void*,int);struct Case{{const char*name;Fn actual,oracle;unsigned bytes,sanitize_kind;}};static struct Case cases[]={{{table}}};\n"
    );
    // Clang can lower constant-mask GETMANT/RANGE to unmasked arithmetic and a
    // selection. Keep original-input bit oracles, sanitize only inactive inputs
    // for their flag oracles, and independently require Keel to suppress isolated
    // inactive signaling NaNs without changing their merge lanes.
    let runner = runner
        + r#"int main(void){unsigned saved=_mm_getcsr();uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),t[8] __attribute__((aligned(64))),base[16] __attribute__((aligned(64)));unsigned char out[64],want[64];uint64_t edges[]={0,UINT64_C(0x8000000080000000),UINT64_C(0x3ff800003fc00000),UINT64_C(0xbff80000bfc00000),UINT64_C(0x7ff000017f800001),UINT64_C(0x7ff812347fc12345),UINT64_C(0x0000000100000001),UINT64_C(0x7fefffff7f7fffff)};
for(unsigned sample=0;sample<4;sample++){for(unsigned i=0;i<8;i++){s[i]=edges[(i+sample)%8];a[i]=edges[(i+sample*3+1)%8];b[i]=edges[(i+sample*3+3)%8];t[i]=UINT64_C(0x1111111111111111);}for(unsigned i=0;i<16;i++)base[i]=edges[i%8];for(unsigned c=0;c<sizeof cases/sizeof*cases;c++){struct Case tc=cases[c];for(unsigned rc=0;rc<4;rc++){memset(out,0xa5,64);memset(want,0xa5,64);unsigned csr=0x1f80|(rc<<13)|((sample&1)?0x8040:0);_mm_setcsr(csr);tc.oracle(s,a,b,t,base,want,0);unsigned flags=_mm_getcsr();if(tc.sanitize_kind){uint64_t safe_a[8] __attribute__((aligned(64))),safe_b[8] __attribute__((aligned(64)));unsigned char ignored[64];memcpy(safe_a,a,64);memcpy(safe_b,b,64);for(unsigned lane=0;lane<8;lane++)if(!(0x55u&(1u<<lane)))safe_a[lane]=safe_b[lane]=UINT64_C(0x3ff0000000000000);_mm_setcsr(csr);tc.oracle(s,safe_a,tc.sanitize_kind==2?safe_b:b,t,base,ignored,0);flags=_mm_getcsr();}_mm_setcsr(csr);tc.actual(s,a,b,t,base,out,0);unsigned actual=_mm_getcsr();if(memcmp(out,want,64)||actual!=flags){fprintf(stderr,"%s sample %u rounding %u flags %x/%x\n",tc.name,sample,rc,actual,flags);return 1;}for(unsigned i=tc.bytes;i<64;i++)if(out[i]!=0xa5)return 2;if(tc.sanitize_kind){uint64_t isolated[8] __attribute__((aligned(64)));for(unsigned lane=0;lane<8;lane++)isolated[lane]=UINT64_C(0x3ff0000000000000);isolated[3]=UINT64_C(0x7ff0000000000001);_mm_setcsr(0x1f80);tc.actual(s,isolated,tc.sanitize_kind==2?isolated:b,t,base,out,0);if(_mm_getcsr()!=0x1f80||memcmp(out+24,(unsigned char*)s+24,8))return 3;}}}}_mm_setcsr(saved);return 0;}
"#;
    fs::write(dir.join("runner.c"), runner).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native = std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512vl")
        && std::is_x86_feature_detected!("avx512dq")
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512cd");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        for stem in ["oracle", "runner"] {
            let output = Command::new("clang")
                .args(["-O2", "-c"])
                .arg(dir.join(format!("{stem}.c")))
                .arg("-o")
                .arg(dir.join(format!("{stem}.o")))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let encoding = module_features(&module, Features::NONE).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    for (label, pools) in [
        ("normal", &keel_codegen::x86_64::REGISTERS),
        ("spill", &EMPTY),
    ] {
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
        );
        let path = dir.join(format!("{label}.o"));
        fs::write(&path, object).unwrap();
        if native {
            let exe = dir.join(label);
            let output = Command::new("clang")
                .arg(dir.join("oracle.o"))
                .arg(dir.join("runner.o"))
                .arg(path)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(exe).output().unwrap();
            assert!(output.status.success(), "{label}: {output:?}");
        }
    }
    if native {
        eprintln!(
            "Executed public control vendor bit and MXCSR oracles with normal and spilled allocation"
        );
    }
}
