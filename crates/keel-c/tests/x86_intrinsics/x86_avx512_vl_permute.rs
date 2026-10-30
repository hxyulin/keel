//! Standard VL permutation APIs retain C controls and raw mask2 index payloads.
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
// Public API shapes are explicit and independent of intrinsic metadata.
const APIS: [(&str, u32, u32, bool, &str, u32, u32, bool); 58] = [
    (
        "permutex2var_epi32",
        128,
        32,
        false,
        "TwoIndex",
        255,
        1,
        true,
    ),
    (
        "permutex2var_epi64",
        128,
        64,
        false,
        "TwoIndex",
        255,
        1,
        true,
    ),
    ("permutex2var_ps", 128, 32, true, "TwoIndex", 255, 1, true),
    ("permutex2var_pd", 128, 64, true, "TwoIndex", 255, 1, true),
    ("permutevar_ps", 128, 32, true, "Local", 255, 2, false),
    ("permutevar_pd", 128, 64, true, "Local", 255, 3, false),
    ("permute_ps", 128, 32, true, "Immediate", 255, 4, false),
    ("permute_pd", 128, 64, true, "Immediate", 3, 5, false),
    ("shuffle_ps", 128, 32, true, "PairImmediate", 255, 7, false),
    ("shuffle_pd", 128, 64, true, "PairImmediate", 3, 8, false),
    ("shuffle_epi32", 128, 32, false, "Immediate", 255, 9, false),
    (
        "alignr_epi32",
        128,
        32,
        false,
        "PairImmediate",
        255,
        11,
        true,
    ),
    (
        "alignr_epi64",
        128,
        64,
        false,
        "PairImmediate",
        255,
        11,
        true,
    ),
    ("unpacklo_epi32", 128, 32, false, "Pair", 255, 12, false),
    ("unpacklo_epi64", 128, 64, false, "Pair", 255, 12, false),
    ("unpacklo_ps", 128, 32, true, "Pair", 255, 12, false),
    ("unpacklo_pd", 128, 64, true, "Pair", 255, 12, false),
    ("unpackhi_epi32", 128, 32, false, "Pair", 255, 13, false),
    ("unpackhi_epi64", 128, 64, false, "Pair", 255, 13, false),
    ("unpackhi_ps", 128, 32, true, "Pair", 255, 13, false),
    ("unpackhi_pd", 128, 64, true, "Pair", 255, 13, false),
    ("movehdup_ps", 128, 32, true, "Unary", 255, 14, false),
    ("moveldup_ps", 128, 32, true, "Unary", 255, 15, false),
    ("movedup_pd", 128, 64, true, "Unary", 255, 16, false),
    ("permutexvar_epi32", 256, 32, false, "Full", 255, 0, true),
    ("permutexvar_epi64", 256, 64, false, "Full", 255, 0, true),
    ("permutexvar_ps", 256, 32, true, "Full", 255, 0, true),
    ("permutexvar_pd", 256, 64, true, "Full", 255, 0, true),
    (
        "permutex2var_epi32",
        256,
        32,
        false,
        "TwoIndex",
        255,
        1,
        true,
    ),
    (
        "permutex2var_epi64",
        256,
        64,
        false,
        "TwoIndex",
        255,
        1,
        true,
    ),
    ("permutex2var_ps", 256, 32, true, "TwoIndex", 255, 1, true),
    ("permutex2var_pd", 256, 64, true, "TwoIndex", 255, 1, true),
    ("permutevar_ps", 256, 32, true, "Local", 255, 2, false),
    ("permutevar_pd", 256, 64, true, "Local", 255, 3, false),
    ("permute_ps", 256, 32, true, "Immediate", 255, 4, false),
    ("permute_pd", 256, 64, true, "Immediate", 15, 5, false),
    ("permutex_epi64", 256, 64, false, "Immediate", 255, 6, true),
    ("permutex_pd", 256, 64, true, "Immediate", 255, 6, true),
    ("shuffle_ps", 256, 32, true, "PairImmediate", 255, 7, false),
    ("shuffle_pd", 256, 64, true, "PairImmediate", 15, 8, false),
    ("shuffle_epi32", 256, 32, false, "Immediate", 255, 9, false),
    (
        "shuffle_i32x4",
        256,
        32,
        false,
        "PairImmediate",
        3,
        10,
        true,
    ),
    (
        "shuffle_i64x2",
        256,
        64,
        false,
        "PairImmediate",
        3,
        10,
        true,
    ),
    ("shuffle_f32x4", 256, 32, true, "PairImmediate", 3, 10, true),
    ("shuffle_f64x2", 256, 64, true, "PairImmediate", 3, 10, true),
    (
        "alignr_epi32",
        256,
        32,
        false,
        "PairImmediate",
        255,
        11,
        true,
    ),
    (
        "alignr_epi64",
        256,
        64,
        false,
        "PairImmediate",
        255,
        11,
        true,
    ),
    ("unpacklo_epi32", 256, 32, false, "Pair", 255, 12, false),
    ("unpacklo_epi64", 256, 64, false, "Pair", 255, 12, false),
    ("unpacklo_ps", 256, 32, true, "Pair", 255, 12, false),
    ("unpacklo_pd", 256, 64, true, "Pair", 255, 12, false),
    ("unpackhi_epi32", 256, 32, false, "Pair", 255, 13, false),
    ("unpackhi_epi64", 256, 64, false, "Pair", 255, 13, false),
    ("unpackhi_ps", 256, 32, true, "Pair", 255, 13, false),
    ("unpackhi_pd", 256, 64, true, "Pair", 255, 13, false),
    ("movehdup_ps", 256, 32, true, "Unary", 255, 14, false),
    ("moveldup_ps", 256, 32, true, "Unary", 255, 15, false),
    ("movedup_pd", 256, 64, true, "Unary", 255, 16, false),
];
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\ntypedef void(*function)(void*,void*,void*,void*,void*,uint8_t);\nstruct testcase{function run,reference;unsigned width,bits,operation,control,form;const char*name;};\n",
    );
    let mut cases = String::new();
    for (api, width, bits, floating, recipe, maximum, operation, plain) in APIS {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let vector = if !floating {
            format!("__m{width}i")
        } else if bits == 64 {
            format!("__m{width}d")
        } else {
            format!("__m{width}")
        };
        let immediate = matches!(recipe, "Immediate" | "PairImmediate");
        let mut forms = vec!["mask", "maskz"];
        if plain {
            forms.push("plain");
        }
        if recipe == "TwoIndex" {
            forms.push("mask2");
        }
        let controls = if !immediate {
            vec![0]
        } else if maximum == 3 {
            vec![0, 1, 2, 3]
        } else if maximum == 15 {
            vec![0, 1, 5, 10, 15]
        } else {
            vec![0, 1, 0x55, 0xaa, 0xff, 0x1b, 0x4e, 0xb1, 0xe4]
        };
        for form in forms {
            for control in &controls {
                for high in [false, true].into_iter().filter(|&h| !h || immediate) {
                    let name =
                        format!("consumer_{api}_{width}_{form}_{control}_{}", u8::from(high));
                    let imm = if high {
                        format!("(UINT64_C(0x100000000)+{control})")
                    } else {
                        control.to_string()
                    };
                    let index = if form == "mask2" {
                        "({evaluations++;i;})"
                    } else {
                        "i"
                    };
                    let arguments = match recipe {
                        "Full" => format!("{index},a"),
                        "Local" => format!("a,{index}"),
                        "TwoIndex" => format!("a,{index},b"),
                        "Immediate" => format!("a,{imm}"),
                        "PairImmediate" => format!("a,b,{imm}"),
                        "Pair" => "a,b".to_string(),
                        _ => "a".to_string(),
                    };
                    let expression = match form {
                        "plain" => format!("{prefix}_{api}({arguments})"),
                        "mask" if recipe == "TwoIndex" => format!("{prefix}_mask_{api}(a,k,i,b)"),
                        "mask2" => format!("{prefix}_mask2_{api}(a,{index},k,b)"),
                        "mask" => format!("{prefix}_mask_{api}(m,k,{arguments})"),
                        _ => format!("{prefix}_maskz_{api}(k,{arguments})"),
                    };
                    let suffix = if !floating {
                        format!("si{width}")
                    } else if bits == 64 {
                        "pd".to_string()
                    } else {
                        "ps".to_string()
                    };
                    let body = format!(
                        "{vector} m=*(const {vector}*)mp,a=*(const {vector}*)ap,b=*(const {vector}*)bp;__m{width}i i=*(const __m{width}i*)ip;unsigned evaluations=0;{prefix}_storeu_{suffix}(out,{expression});*(unsigned*)((char*)out+64)=evaluations;"
                    );
                    // ULL literals are written without stdint dependencies in the compiler input.
                    let consumer_body = body.replace("UINT64_C(0x100000000)", "0x100000000ULL");
                    source += &format!(
                        "__attribute__((target(\"avx512vl\"))) void {name}(void*mp,void*ap,void*ip,void*bp,void*out,__mmask8 k){{{consumer_body}}}\n"
                    );
                    c += &format!(
                        "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*mp,void*ap,void*ip,void*bp,void*out,__mmask8 k){{{body}}}\n"
                    );
                    let formcode = match form {
                        "mask2" => 3,
                        "plain" => 2,
                        "maskz" => 0,
                        _ => 1,
                    };
                    cases += &format!(
                        "{{{name},reference_{name},{width},{bits},{operation},{control},{formcode},\"{name}\"}},\n"
                    );
                }
            }
        }
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
static uint64_t read_lane(void*p,unsigned bits,unsigned lane){uint64_t x=0;memcpy(&x,(char*)p+lane*(bits/8),bits/8);return x;}
static uint64_t scalar(struct testcase t,void*a,void*i,void*b,unsigned lane){unsigned n=t.width/t.bits,local=128/t.bits,group=(lane/local)*local,j=lane%local,ix=0;uint64_t index=read_lane(i,t.bits,lane);void*source=a;switch(t.operation){case 0:ix=index&(n-1);break;case 1:ix=index&(2*n-1);if(ix>=n){source=b;ix-=n;}break;case 2:ix=group+(index&3);break;case 3:ix=group+((index>>1)&1);break;case 4:case 9:ix=group+((t.control>>(2*j))&3);break;case 5:ix=group+((t.control>>lane)&1);break;case 6:ix=(t.control>>(2*lane))&3;break;case 7:source=j<2?a:b;ix=group+((t.control>>(2*j))&3);break;case 8:source=j?b:a;ix=group+((t.control>>lane)&1);break;case 10:{unsigned block=lane/local;source=block?b:a;ix=((t.control>>block)&1)*local+j;break;}case 11:ix=(t.control&(n-1))+lane;if(ix<n)source=b;else ix-=n;break;case 12:case 13:source=j&1?b:a;ix=group+(t.operation==13?local/2:0)+j/2;break;case 14:ix=lane|1;break;case 15:case 16:ix=lane&~1u;break;default:return 0;}return read_lane(source,t.bits,ix);}
int main(void){unsigned saved=_mm_getcsr();uint64_t values[]={0,UINT64_MAX,UINT64_C(0x800000007f812345),UINT64_C(0x7ff012347fc54321),UINT64_C(0xfff05432ffc12345),UINT64_C(0x3ff00000bf800000),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x8000000000000000)};for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase t=cases[ci];unsigned bytes=t.width/8,n=t.width/t.bits;uint64_t m[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),index[8] __attribute__((aligned(64)));unsigned char out[96],want[96];uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<n)-1)};for(unsigned sample=0;sample<2*n+2;sample++){for(unsigned j=0;j<8;j++){m[j]=values[(j+sample+3)%8];a[j]=values[(j+sample)%8];b[j]=values[(j+sample+5)%8];}for(unsigned lane=0;lane<n;lane++){uint64_t ix=(lane+sample)%(2*n);if(sample&1)ix|=t.bits==32?UINT64_C(0x7f810000):UINT64_C(0x7ff0123400000000);memcpy((char*)index+lane*(t.bits/8),&ix,t.bits/8);}for(unsigned mi=0;mi<7;mi++){unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);t.reference(m,a,index,b,want,masks[mi]);t.run(m,a,index,b,out,masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,sizeof out)){fprintf(stderr,"%s sample=%u mask=%x system mismatch\n",t.name,sample,masks[mi]);return 1;}for(unsigned lane=0;lane<n;lane++){uint64_t merge=t.form==0?0:read_lane(t.form==3?(void*)index:t.operation==1?(void*)a:(void*)m,t.bits,lane);uint64_t expected=(t.form==2||(masks[mi]&(1u<<lane)))?scalar(t,a,index,b,lane):merge;if(read_lane(out,t.bits,lane)!=expected){fprintf(stderr,"%s sample=%u lane=%u scalar mismatch\n",t.name,sample,lane);return 2;}}unsigned evaluations;memcpy(&evaluations,out+64,4);if(evaluations!=(t.form==3))return 3;}}}_mm_setcsr(saved);return 0;}
"#;
    (source, c)
}
#[test]
fn vl_permutation_wrappers_match_scalar_system_controls_and_mask2_bits() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.avx2 && !features.avx512bw && !features.avx512dq);
    let missing = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&missing, &EMPTY, &pipeline).is_err()
    );
    let directory = std::env::temp_dir().join(format!("keel-c-vl-permute-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("permute.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native C VL permutations: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-fno-tree-vectorize", "-mavx512f", "-mavx512vl"])
                .arg(directory.join("oracle.c"))
                .arg(directory.join("permute.o"))
                .arg("-o")
                .arg(directory.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}
#[test]
fn vl_public_permutation_controls_preserve_narrow_bounds() {
    for (api, width, bits, floating, recipe, maximum, _, _) in APIS {
        if !matches!(recipe, "Immediate" | "PairImmediate") {
            continue;
        }
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let vector = if !floating {
            format!("__m{width}i")
        } else if bits == 64 {
            format!("__m{width}d")
        } else {
            format!("__m{width}")
        };
        for control in ["-1".to_string(), (maximum + 1).to_string(), "n".to_string()] {
            let args = if recipe == "PairImmediate" {
                format!("a,-1,a,b,{control}")
            } else {
                format!("a,-1,a,{control}")
            };
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512vl\"))) {vector} f({vector} a,{vector} b,int n){{return {prefix}_mask_{api}({args});}}\n"
            );
            match parse(&source, Target::X86_64LinuxGnu) {
                Err(_) => {}
                Ok(module) => assert!(
                    keel_ir::verify_module(&module).is_err(),
                    "{api} {width} {control}"
                ),
            }
        }
    }
}
