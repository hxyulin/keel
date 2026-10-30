//! Public permutation wrappers retain argument order and evaluate operands once.
use super::pclmul::parse;
use keel_codegen::x86_features::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const APIS: &[(&str, &str, &str)] = &[
    ("permutexvar_epi32", "I32x16", "Full"),
    ("permutexvar_epi64", "I64x8", "Full"),
    ("permutexvar_ps", "F32x16", "Full"),
    ("permutexvar_pd", "F64x8", "Full"),
    ("permutevar_ps", "F32x16", "Local"),
    ("permutevar_pd", "F64x8", "Local"),
    ("permute_ps", "F32x16", "Immediate"),
    ("permute_pd", "F64x8", "Immediate"),
    ("permutex_epi64", "I64x8", "Immediate"),
    ("permutex_pd", "F64x8", "Immediate"),
    ("shuffle_ps", "F32x16", "PairImmediate"),
    ("shuffle_pd", "F64x8", "PairImmediate"),
    ("shuffle_epi32", "I32x16", "Immediate"),
    ("shuffle_i32x4", "I32x16", "PairImmediate"),
    ("shuffle_i64x2", "I64x8", "PairImmediate"),
    ("shuffle_f32x4", "F32x16", "PairImmediate"),
    ("shuffle_f64x2", "F64x8", "PairImmediate"),
    ("alignr_epi32", "I32x16", "PairImmediate"),
    ("alignr_epi64", "I64x8", "PairImmediate"),
    ("unpacklo_epi32", "I32x16", "Pair"),
    ("unpacklo_epi64", "I64x8", "Pair"),
    ("unpacklo_ps", "F32x16", "Pair"),
    ("unpacklo_pd", "F64x8", "Pair"),
    ("unpackhi_epi32", "I32x16", "Pair"),
    ("unpackhi_epi64", "I64x8", "Pair"),
    ("unpackhi_ps", "F32x16", "Pair"),
    ("unpackhi_pd", "F64x8", "Pair"),
    ("movehdup_ps", "F32x16", "Unary"),
    ("moveldup_ps", "F32x16", "Unary"),
    ("movedup_pd", "F64x8", "Unary"),
    ("permutex2var_epi32", "I32x16", "Two"),
    ("permutex2var_epi64", "I64x8", "Two"),
    ("permutex2var_ps", "F32x16", "Two"),
    ("permutex2var_pd", "F64x8", "Two"),
];
fn public_type(shape: &str) -> &'static str {
    match shape {
        "F32x16" => "__m512",
        "F64x8" => "__m512d",
        _ => "__m512i",
    }
}
fn reader(shape: &str) -> &'static str {
    match shape {
        "F32x16" => "take_ps",
        "F64x8" => "take_pd",
        _ => "take_i",
    }
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(vector_count),FN(mask_count);\nstatic __inline__ __attribute__((always_inline)) __m512i take_i(const void*p){__m512i v;FN(vector_count)++;__builtin_memcpy(&v,p,64);return v;}\nstatic __inline__ __attribute__((always_inline)) __m512 take_ps(const void*p){__m512 v;FN(vector_count)++;__builtin_memcpy(&v,p,64);return v;}\nstatic __inline__ __attribute__((always_inline)) __m512d take_pd(const void*p){__m512d v;FN(vector_count)++;__builtin_memcpy(&v,p,64);return v;}\nstatic __inline__ __attribute__((always_inline)) unsigned short take_mask(unsigned k){FN(mask_count)++;return k;}\n",
    );
    let mut harness = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nextern int vector_count,mask_count,reference_vector_count,reference_mask_count;\ntypedef void(*Fn)(const void*,const void*,const void*,const void*,void*,unsigned);\nstruct Case{Fn actual,reference;unsigned vectors,masks;};\n",
    );
    let mut cases = String::new();
    let mut number = 0;
    for &(api, shape, kind) in APIS {
        let r = reader(shape);
        let ty = public_type(shape);
        let modes: &[&str] = if kind == "Two" {
            &["", "mask_", "mask2_", "maskz_"]
        } else {
            &["", "mask_", "maskz_"]
        };
        let controls: &[i32] = if matches!(kind, "Immediate" | "PairImmediate") {
            &[0, 78, 255]
        } else {
            &[0]
        };
        for &control in controls {
            for &mode in modes {
                let a = format!("{r}(a)");
                let b = format!("{r}(b)");
                let idx = "take_i(i)".to_owned();
                let merge = format!("{r}(m)");
                let mut args: Vec<String> = match kind {
                    "Full" => vec![idx.clone(), a.clone()],
                    "Local" => vec![a.clone(), idx.clone()],
                    "Immediate" => vec![a.clone(), control.to_string()],
                    "PairImmediate" => vec![a.clone(), b.clone(), control.to_string()],
                    "Pair" => vec![a.clone(), b.clone()],
                    "Unary" => vec![a.clone()],
                    "Two" => vec![a.clone(), idx.clone(), b.clone()],
                    _ => unreachable!(),
                };
                if kind == "Two" {
                    match mode {
                        "mask_" => args.insert(1, "take_mask(k)".into()),
                        "mask2_" => args.insert(2, "take_mask(k)".into()),
                        "maskz_" => args.insert(0, "take_mask(k)".into()),
                        _ => {}
                    }
                } else {
                    match mode {
                        "mask_" => {
                            args.insert(0, "take_mask(k)".into());
                            args.insert(0, merge);
                        }
                        "maskz_" => args.insert(0, "take_mask(k)".into()),
                        _ => {}
                    }
                }
                let expression = format!("_mm512_{mode}{api}({})", args.join(","));
                let vectors = args
                    .iter()
                    .filter(|a| a.starts_with("take_") && !a.starts_with("take_mask"))
                    .count();
                let masks = usize::from(!mode.is_empty());
                source += &format!(
                    "void FN(case_{number})(const void*m,const void*a,const void*i,const void*b,void*o,unsigned k){{{ty} result={expression};__builtin_memcpy(o,&result,64);}}\n"
                );
                harness += &format!(
                    "extern void case_{number}(const void*,const void*,const void*,const void*,void*,unsigned);\nextern void reference_case_{number}(const void*,const void*,const void*,const void*,void*,unsigned);\n"
                );
                cases += &format!("{{case_{number},reference_case_{number},{vectors},{masks}}},\n");
                number += 1;
            }
        }
    }
    harness += &format!(
        "static const struct Case cases[]={{{cases}}};\nint main(void){{uint32_t a[16],b[16],idx[16],m[16];unsigned char out[64],expected[64];for(unsigned sample=0;sample<5;sample++){{for(unsigned i=0;i<16;i++){{a[i]=0x7f800001u+i*17;b[i]=0xffc00011u+i*31;idx[i]=sample%2?~(i*7u):i+sample*16;m[i]=0xa5a51234u+i;}}for(unsigned pattern=0;pattern<4;pattern++){{unsigned k=pattern==0?0:pattern==1?0xffff:pattern==2?0xaaaa:0x80;for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){{const struct Case*c=&cases[ci];vector_count=mask_count=reference_vector_count=reference_mask_count=0;c->actual(m,a,idx,b,out,k);c->reference(m,a,idx,b,expected,k);if(memcmp(out,expected,64)||vector_count!=c->vectors||mask_count!=c->masks||reference_vector_count!=c->vectors||reference_mask_count!=c->masks){{fprintf(stderr,\"case%u sample%u mask%u vectors%d/%d expected%u masks%d/%d expected%u\\n\",ci,sample,pattern,vector_count,reference_vector_count,c->vectors,mask_count,reference_mask_count,c->masks);return 1;}}}}}}}}return 0;}}\n"
    );
    (source, harness)
}
fn command(cmd: &mut Command) {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_c_permute_controls_and_recognition() {
    for &(api, shape, kind) in APIS
        .iter()
        .filter(|(_, _, kind)| matches!(*kind, "Immediate" | "PairImmediate"))
    {
        let ty = public_type(shape);
        for control in ["-1", "256", "n"] {
            let args = if kind == "PairImmediate" {
                format!("a,b,{control}")
            } else {
                format!("a,{control}")
            };
            let text = format!(
                "#include <immintrin.h>\n{ty} f({ty} a,{ty} b,int n){{return _mm512_{api}({args});}}\n"
            );
            assert!(
                parse(&text, Target::X86_64LinuxGnu).is_err(),
                "{api} {control}"
            );
        }
    }
    let (source, _) = sources();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let covered: std::collections::HashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| match f.inst_data(i) {
                keel_ir::InstData::Intrinsic { id, .. } if id.x86_avx512_permute().is_some() => {
                    Some(*id)
                }
                _ => None,
            })
        })
        .collect();
    assert_eq!(
        covered.len(),
        38,
        "all typed permutation routes need a public wrapper"
    );
}
#[test]
fn avx512_c_permute_native_wrappers_and_single_evaluation() {
    let (source, harness) = sources();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-permute-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
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
                .args(["-O1", "-mavx512f", "-DREFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-c"])
                .arg(dir.join("harness.c"))
                .arg("-o")
                .arg(dir.join("harness.o")),
        );
    }
    for (tier, alloc, pools, opt) in [
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
        let mut m = original.clone();
        if opt {
            keel_opt::optimize(&mut m).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(dir.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(dir.join("actual.o"))
                    .arg(dir.join("reference.o"))
                    .arg(dir.join("harness.o"))
                    .arg("-o")
                    .arg(dir.join("run")),
            );
            command(&mut Command::new(dir.join("run")));
        }
    }
}
