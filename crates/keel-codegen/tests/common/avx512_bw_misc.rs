//! Independent scalar and system-intrinsic references for BW miscellaneous ops.
use keel_ir::{Intrinsic, Type, VectorType};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy)]
pub struct Case {
    pub id: Intrinsic,
    pub width: u32,
    pub op: u8,
    pub alias: u8,
    pub control: u32,
    pub input: VectorType,
    pub result: VectorType,
    pub mask: Type,
}
impl Case {
    pub fn name(self) -> String {
        format!("{}_{}_{}", self.id.info().name, self.alias, self.control)
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (width, byte, word, quad) in [
        (128, VectorType::I8x16, VectorType::I16x8, VectorType::I64x2),
        (
            256,
            VectorType::I8x32,
            VectorType::I16x16,
            VectorType::I64x4,
        ),
        (
            512,
            VectorType::I8x64,
            VectorType::I16x32,
            VectorType::I64x8,
        ),
    ] {
        for (op, stem) in [
            "pabsb",
            "pabsw",
            "broadcastb",
            "broadcastw",
            "blendb",
            "blendw",
            "psadbw",
            "dbsad",
        ]
        .into_iter()
        .enumerate()
        {
            let input = if [1, 3, 5].contains(&op) { word } else { byte };
            let result = if op == 6 {
                quad
            } else if op == 7 {
                word
            } else {
                input
            };
            let mask = match result.lanes() {
                2 | 4 | 8 => Type::I8,
                16 => Type::I16,
                32 => Type::I32,
                64 => Type::I64,
                _ => unreachable!(),
            };
            let id = Intrinsic::from_name(&format!("x86_avx512bw_{stem}{width}")).unwrap();
            for alias in [0, 1, 2] {
                for &control in if op == 7 {
                    &[0, 255, 0x1b, 0xe4, 0x72][..]
                } else {
                    &[0][..]
                } {
                    cases.push(Case {
                        id,
                        width,
                        op: op as u8,
                        alias,
                        control,
                        input,
                        result,
                        mask,
                    });
                }
            }
        }
    }
    cases
}
pub fn directory(label: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "keel-bw-misc-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
pub fn sources(cases: &[Case], c_api: bool) -> (String, String) {
    let mut source = if c_api {
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n".into()
    } else {
        String::new()
    };
    let mut declarations = String::new();
    let mut refs = String::new();
    let mut table = String::new();
    for &case in cases {
        let name = case.name();
        let input = case.input;
        let result = case.result;
        let width = case.width;
        let size = width / 8;
        let ctl = case.control;
        let alias = case.alias;
        let op = case.op;
        let prefix = if width == 128 {
            "_mm"
        } else if width == 256 {
            "_mm256"
        } else {
            "_mm512"
        };
        let ct = format!("__m{width}i");
        let scalar = if op == 3 { "short" } else { "signed char" };
        let lane = if [1, 3, 5].contains(&op) { 16 } else { 8 };
        let feature = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        let a = "a";
        let b = if alias == 1 { "a" } else { "b" };
        let merge = if alias == 2 { "a" } else { "s" };
        let invoke = |merge: &str, k: &str, tracked: bool| {
            let av = if tracked { "(na++,a)" } else { a };
            let bv = if tracked {
                format!("(nb++,{b})")
            } else {
                b.into()
            };
            let mv = if tracked {
                format!("(ns++,{merge})")
            } else {
                merge.into()
            };
            let kv = if tracked {
                format!("(nk++,{k})")
            } else {
                k.into()
            };
            match op {
                0 | 1 => format!("{prefix}_mask_abs_epi{lane}({mv},{kv},{av})"),
                2 | 3 => {
                    if alias == 1 {
                        let xmm = if tracked {
                            "(na++,*(const __m128i*)pa)"
                        } else {
                            "*(const __m128i*)pa"
                        };
                        format!(
                            "{prefix}_mask_broadcast{}_epi{lane}({mv},{kv},{xmm})",
                            if op == 2 { "b" } else { "w" }
                        )
                    } else {
                        let value = if tracked {
                            format!(
                                "(na++,((unsigned long long)*(const {scalar}*)pa|0x12340000ULL))"
                            )
                        } else {
                            format!("*(const {scalar}*)pa")
                        };
                        format!("{prefix}_mask_set1_epi{lane}({mv},{kv},{value})")
                    }
                }
                4 | 5 => format!("{prefix}_mask_blend_epi{lane}({kv},{av},{bv})"),
                // The standard narrow SAD interfaces retain their legacy
                // SSE2/AVX2 contracts. Exercise the new typed route explicitly.
                6 if tracked && width < 512 => format!(
                    "(({ct})__builtin_keel_avx512bw_psadbw{width}((__keel_i8_{width}){av},(__keel_i8_{width}){bv}))"
                ),
                6 => format!("{prefix}_sad_epu8({av},{bv})"),
                7 => format!("{prefix}_mask_dbsad_epu8({mv},{kv},{av},{bv},{ctl})"),
                _ => unreachable!(),
            }
        };
        let mut extras = String::new();
        if c_api && (op < 4 || op == 7) {
            let expression = match op {
                0 | 1 => format!("{prefix}_maskz_abs_epi{lane}(k,a)"),
                2 | 3 => {
                    if alias == 1 {
                        format!(
                            "{prefix}_maskz_broadcast{}_epi{lane}(k,*(const __m128i*)pa)",
                            if op == 2 { "b" } else { "w" }
                        )
                    } else {
                        format!("{prefix}_maskz_set1_epi{lane}(k,*(const {scalar}*)pa)")
                    }
                }
                7 => format!("{prefix}_maskz_dbsad_epu8(k,a,{b},{ctl})"),
                _ => unreachable!(),
            };
            extras += &format!("{{ {ct} z={expression};memcpy((char*)out+320,&z,{size}); }}");
            if width == 512 || op == 7 {
                let plain = expression.replace("_maskz_", "_").replacen("(k,", "(", 1);
                extras += &format!("{{ {ct} z={plain};memcpy((char*)out+384,&z,{size}); }}");
            }
        }
        let expression = invoke(merge, "k", false);
        refs += &format!(
            "static void ref_{name}(const void*ps,const void*pa,const void*pb,void*out,uint64_t k){{{ct} s,a,b,r;memcpy(&s,ps,{size});memcpy(&a,pa,{size});memcpy(&b,pb,{size});unsigned na=0;r={expression};memcpy(out,&r,{size});memcpy((char*)out+64,&s,{size});memcpy((char*)out+128,&a,{size});memcpy((char*)out+192,&b,{size});memcpy((char*)out+256,&k,8);{extras}}}\n"
        );
        if c_api {
            let ns = u8::from(op < 4 || op == 7);
            let nk = u8::from(op != 6);
            let nb = u8::from(op >= 4);
            let na = 1;
            let expression = invoke(merge, "k", true);
            source += &format!(
                "__attribute__((target(\"{feature}\"))) void {name}(const void*ps,const void*pa,const void*pb,void*out,uint64_t k){{{ct} s=*({ct}*)ps,a=*({ct}*)pa,b=*({ct}*)pb;unsigned ns=0,nk=0,na=0,nb=0;{ct} r={expression};if(ns!={ns}||nk!={nk}||na!={na}||nb!={nb})return;*({ct}*)out=r;*({ct}*)((char*)out+64)=s;*({ct}*)((char*)out+128)=a;*({ct}*)((char*)out+192)=b;*(uint64_t*)((char*)out+256)=k;{extras}}}\n"
            );
        } else {
            let mt = case.mask;
            let mask = if mt == Type::I64 { "v4" } else { "v5" };
            source += &format!(
                "func @{name}(ptr,ptr,ptr,ptr,i64) target_features(\"{feature}\") {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i64):\n"
            );
            if mt != Type::I64 {
                source += &format!(" v5=trunc.{mt} v4\n");
            }
            source += &format!(
                " v6=load.{result} align 1 v0\n v7=load.{input} align 1 v1\n v8=load.{input} align 1 v2\n"
            );
            let merge = if alias == 2 {
                if result != input {
                    source += &format!(" v9=bitcast.{result} v7\n");
                    "v9"
                } else {
                    "v7"
                }
            } else {
                "v6"
            };
            let b = if alias == 1 { "v7" } else { "v8" };
            let args = match op {
                0 | 1 => format!("{merge},{mask},v7"),
                2 | 3 => {
                    let st = if op == 3 { "i16" } else { "i8" };
                    source += &format!(" v11=load.{st} align 1 v1\n");
                    format!("{merge},{mask},v11")
                }
                4 | 5 => format!("{mask},v7,{b}"),
                6 => format!("v7,{b}"),
                7 => {
                    source += &format!(" v12=iconst.i32 {ctl}\n");
                    format!("{merge},{mask},v7,{b},v12")
                }
                _ => unreachable!(),
            };
            source += &format!(
                " v10=intrinsic {}({args})\n store align 1 v10,v3\n",
                case.id.info().name
            );
            for (offset, value) in [(64, "v6"), (128, "v7"), (192, "v8"), (256, "v4")] {
                source += &format!(
                    " n{offset}=iconst.i64 {offset}\n p{offset}=ptradd v3,n{offset}\n store align 1 {value},p{offset}\n"
                );
            }
            source += " return\n}\n";
        }
        declarations +=
            &format!("extern void {name}(const void*,const void*,const void*,void*,uint64_t);\n");
        table += &format!("{{\"{name}\",{name},ref_{name},{size},{op},{alias},{ctl}}},\n");
    }
    let harness = include_str!("../vector_x86/x86_avx512_bw_misc.c")
        .replace("@DECLS@", &declarations)
        .replace("@REFS@", &refs)
        .replace("@TABLE@", &table);
    (source, harness)
}

pub fn run(module: &keel_ir::Module, harness: &str, label: &str) {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::{AllocationTier, Pools},
        x86_features::{FeatureSelector, Features, module_features},
    };
    use std::{fs, process::Command};
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        float_caller_saved: &[12],
        ..keel_codegen::x86_64::REGISTERS
    };
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(module, base).unwrap();
    assert!(features.avx512bw && !features.avx512dq && !features.avx2);
    let dir = directory(label);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512bw");
    if native {
        let result = Command::new("cc")
            .args(["-O1", "-mavx512bw", "-mavx512vl", "-c"])
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &empty, &one] {
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
                    features: base,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                assert!(text.contains("kmovd") && text.contains("kmovq"));
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                );
                fs::write(dir.join("mask.o"), object).unwrap();
                if native {
                    let result = Command::new("cc")
                        .arg(dir.join("runner.o"))
                        .arg(dir.join("mask.o"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let result = Command::new(dir.join("run")).output().unwrap();
                    assert!(
                        result.status.success(),
                        "{label} {allocator:?} opt{optimize}: {} {}",
                        result.status,
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
            }
        }
    }
}
