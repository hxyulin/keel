//! The strict-v1 userspace join: scalar fallbacks, helper ABI and transport.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use object::{Object, ObjectSymbol};
use std::{collections::BTreeSet, fs, path::PathBuf};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn scalars() -> String {
    let mut source = String::new();
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in ["popcnt", "clz", "ctz", "bswap"] {
            if ty == "i8" && op == "bswap" {
                continue;
            }
            source += &format!(
                "func @{op}_{ty}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{ty} align 1 v0\n v3 = {op} v2\n store align 1 v3,v1\n return\n}}\n"
            );
        }
    }
    for ty in ["f32", "f64"] {
        for op in ["fnearest", "ffloor", "fceil", "ftrunc"] {
            // Both original and rounded values remain live: exercise scratch
            // ownership and spilled FP locations, not just isolated opcodes.
            source += &format!(
                "func @{op}_{ty}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{ty} align 1 v0\n v3 = {op} v2\n store align 1 v3,v1\n store align 1 v2,v0\n return\n}}\n"
            );
        }
    }
    source
}

fn compile(
    source: &str,
    level: CpuLevel,
    selection: SelectionTier,
    allocation: AllocationTier,
    spilled: bool,
) -> (Vec<u8>, String) {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if selection == SelectionTier::Full {
        keel_opt::optimize(&mut module).unwrap();
    }
    let features = Features::for_level(level);
    let selector = FeatureSelector {
        inner: &selection,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    let pools = if spilled {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    for item in &program.items {
        if let keel_asm_ir::Item::X86Instruction(i) = item {
            assert!(features.contains(i.required_features()), "{level:?}: {i}");
        }
    }
    let object = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (object, text)
}

fn undefined(object: &[u8]) -> BTreeSet<String> {
    object::File::parse(object)
        .unwrap()
        .symbols()
        .filter(|s| s.is_undefined() && !s.name().unwrap().is_empty())
        .map(|s| s.name().unwrap().to_owned())
        .collect()
}

#[test]
fn scalar_fallbacks_are_baseline_at_every_tier_and_keep_v2_v3_selection() {
    let export = std::env::var_os("KEEL_V1_EXPORT_DIR").map(PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("scalars.c"), harness()).unwrap();
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for spilled in [false, true] {
                    let (object, text) = compile(&scalars(), level, s, a, spilled);
                    assert!(
                        undefined(&object).is_empty(),
                        "scalar fallbacks must be self-contained"
                    );
                    assert_eq!(text.contains("\tpopcnt\t"), level != CpuLevel::V1);
                    assert_eq!(text.contains("\troundsd\t"), level != CpuLevel::V1);
                    if level == CpuLevel::V1
                        && let Some(dir) = &export
                    {
                        let name = format!("{s:?}-{a:?}-{spilled}");
                        fs::write(dir.join(format!("{name}.o")), &object).unwrap();
                    }
                    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                    if level != CpuLevel::V3
                        || (std::is_x86_feature_detected!("avx2")
                            && std::is_x86_feature_detected!("fma")
                            && std::is_x86_feature_detected!("bmi1"))
                    {
                        execute(
                            &format!("{level:?}-{s:?}-{a:?}-{spilled}"),
                            &object,
                            &harness(),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn helpers_are_explicit_link_requirements_and_fma_preserves_fusion() {
    let mut source = String::new();
    for ty in ["f32", "f64", "f80", "f32x4", "f64x2"] {
        source += &format!(
            "func @fused_{ty}({ty},{ty},{ty})->{ty} {{\nblock0(v0:{ty},v1:{ty},v2:{ty}):\n v3 = fma v0,v1,v2\n return v3\n}}\n"
        );
    }
    for op in ["udiv", "sdiv", "urem", "srem"] {
        source += &format!(
            "func @{op}(i128,i128)->i128 {{\nblock0(v0:i128,v1:i128):\n v2 = {op} v0,v1\n return v2\n}}\n"
        );
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for spilled in [false, true] {
                    let (object, _) = compile(&source, level, s, a, spilled);
                    let mut expected: BTreeSet<_> =
                        ["__udivti3", "__divti3", "__umodti3", "__modti3", "fmal"]
                            .map(str::to_owned)
                            .into();
                    if level != CpuLevel::V3 {
                        expected.extend(["fma".into(), "fmaf".into()]);
                    }
                    assert_eq!(undefined(&object), expected);
                    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                    if level != CpuLevel::V3
                        || (std::is_x86_feature_detected!("avx2")
                            && std::is_x86_feature_detected!("fma")
                            && std::is_x86_feature_detected!("bmi1"))
                    {
                        execute(
                            &format!("helpers-{level:?}-{s:?}-{a:?}-{spilled}"),
                            &object,
                            HELPER_HARNESS,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn scalar_sse2_and_ordinary_abi_corpus_obeys_explicit_levels() {
    for source in [
        include_str!("../programs/arith.keel"),
        include_str!("../programs/floats.keel"),
        include_str!("../programs/call_args.keel"),
        include_str!("../programs/abi_ext.keel"),
        include_str!("../programs/atomics.keel"),
    ] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
                    compile(source, level, s, a, true);
                }
            }
        }
    }
}

fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <math.h>\n#include <fenv.h>\n",
    );
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in ["popcnt", "clz", "ctz", "bswap"] {
            c += &format!("extern void {op}_{ty}(void*,void*);\n");
        }
    }
    for ty in ["f32", "f64"] {
        for op in ["fnearest", "ffloor", "fceil", "ftrunc"] {
            c += &format!("extern void {op}_{ty}(void*,void*);\n");
        }
    }
    c += "static uint64_t rng=1;static uint64_t next(void){rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;return rng;}\nint main(void){\n";
    for (ty, bits) in [
        ("i8", 8),
        ("i16", 16),
        ("i32", 32),
        ("i64", 64),
        ("i128", 128),
    ] {
        c += &format!(
            "for(unsigned n=0;n<2052;n++){{unsigned __int128 x=((unsigned __int128)next()<<64)|next();if(n<4)x=n==0?0:n==1?1:n==2?~(unsigned __int128)0:(unsigned __int128)1<<({bits}-1);unsigned char in[18]={{0}},out[18];memcpy(in+1,&x,{bits}/8);x=0;memcpy(&x,in+1,{bits}/8);\n"
        );
        for op in ["popcnt", "clz", "ctz", "bswap"] {
            if ty == "i8" && op == "bswap" {
                continue;
            }
            let reference = match op {
                "popcnt" => format!("for(unsigned k=0;k<{bits};k++)e+=(x>>k)&1;"),
                "clz" => format!("for(unsigned k={bits};k>0 && !((x>>(k-1))&1);k--)e++;"),
                "ctz" => format!("for(unsigned k=0;k<{bits} && !((x>>k)&1);k++)e++;"),
                _ => format!("for(unsigned k=0;k<{bits}/8;k++)e=(e<<8)|((x>>(k*8))&255);"),
            };
            c += &format!(
                "{{unsigned __int128 e=0,y=0;{reference}memset(out,0xa5,sizeof(out));{op}_{ty}(in+1,out+1);memcpy(&y,out+1,{bits}/8);if(y!=e||out[0]!=0xa5||out[1+{bits}/8]!=0xa5)return 10;}}\n"
            );
        }
        c += "}\n";
    }
    for (ty, ct, uint, suffix) in [
        ("f32", "float", "uint32_t", "f"),
        ("f64", "double", "uint64_t", ""),
    ] {
        c += &format!(
            "for(unsigned n=0;n<2058;n++){{{uint} u=next();{ct} x;memcpy(&x,&u,sizeof(x));switch(n){{case 0:x=0;break;case 1:x=-0.0;break;case 2:x=0.5;break;case 3:x=-0.5;break;case 4:x=1.5;break;case 5:x=-1.5;break;case 6:x=INFINITY;break;case 7:x=-INFINITY;break;case 8:x=NAN;break;case 9:x=nextafter{suffix}(0,1);break;}}\nfesetround(FE_TONEAREST);{ct} nearest=nearbyint{suffix}(x);int modes[]={{FE_TONEAREST,FE_DOWNWARD,FE_UPWARD,FE_TOWARDZERO}};for(unsigned m=0;m<4;m++){{fesetround(modes[m]);\n"
        );
        for (op, reference) in [
            ("fnearest", "nearest".into()),
            ("ffloor", format!("floor{suffix}(x)")),
            ("fceil", format!("ceil{suffix}(x)")),
            ("ftrunc", format!("trunc{suffix}(x)")),
        ] {
            c += &format!(
                "{{{ct} in=x,y,e={reference};{op}_{ty}(&in,&y);if(memcmp(&in,&x,sizeof(x)))return 20;if(isnan(e)){{if(!isnan(y))return 21;}}else if(memcmp(&y,&e,sizeof(y)))return 22;}}\n"
            );
        }
        c += "}}\n";
    }
    c + "return 0;}\n"
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute(name: &str, object: &[u8], harness: &str) {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("keel-v1-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("scalars.o"), object).unwrap();
    fs::write(dir.join("scalars.c"), harness).unwrap();
    let output = Command::new("cc")
        .args([
            "-O0",
            "-march=x86-64",
            "-fno-builtin",
            "scalars.c",
            "scalars.o",
            "-lm",
            "-o",
            "run",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        Command::new(dir.join("run")).status().unwrap().success(),
        "{name}"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const HELPER_HARNESS: &str = r#"
#include <stdint.h>
#include <math.h>
typedef float F4 __attribute__((vector_size(16)));
typedef double D2 __attribute__((vector_size(16)));
extern float fused_f32(float,float,float);
extern double fused_f64(double,double,double);
extern long double fused_f80(long double,long double,long double);
extern F4 fused_f32x4(F4,F4,F4);
extern D2 fused_f64x2(D2,D2,D2);
extern unsigned __int128 udiv(unsigned __int128,unsigned __int128);
extern unsigned __int128 urem(unsigned __int128,unsigned __int128);
extern __int128 sdiv(__int128,__int128);
extern __int128 srem(__int128,__int128);
int main(void) {
    float af=1+0x1p-13f,bf=1-0x1p-13f;
    double ad=1+0x1p-27,bd=1-0x1p-27;
    long double al=1+0x1p-33L,bl=1-0x1p-33L;
    if(fused_f32(af,bf,-1)!=-0x1p-26f) return 1;
    if(fused_f64(ad,bd,-1)!=-0x1p-54) return 2;
    if(fused_f80(al,bl,-1)!=-0x1p-66L) return 3;
    F4 f=fused_f32x4((F4){af,af,af,af},(F4){bf,bf,bf,bf},(F4){-1,-1,-1,-1});
    D2 d=fused_f64x2((D2){ad,ad},(D2){bd,bd},(D2){-1,-1});
    for(unsigned i=0;i<4;i++) if(f[i]!=-0x1p-26f) return 4;
    for(unsigned i=0;i<2;i++) if(d[i]!=-0x1p-54) return 5;
    unsigned __int128 q=((unsigned __int128)1<<92)+12345,x=q*17+11;
    if(udiv(x,17)!=q||urem(x,17)!=11) return 6;
    if(sdiv(-(__int128)x,17)!=-(__int128)q||srem(-(__int128)x,17)!=-11) return 7;
    return 0;
}
"#;
