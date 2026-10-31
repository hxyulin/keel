use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::new();
    for (op, count) in [("msg1", 2), ("msg2", 2), ("rnds2", 3)] {
        for same in 0..2 {
            s += &format!(
                "func @sha_{op}_{same}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i64 16\n v3 = ptradd v0,v2\n v4 = ptradd v3,v2\n v5 = load.i32x4 align 1 v0\n v6 = load.i32x4 align 1 v3\n v7 = load.i32x4 align 1 v4\n v8 = intrinsic x86_sha256{op}(v5,{}{})\n store align 1 v8,v1\n v9 = ptradd v1,v2\n store align 1 v5,v9\n v10 = ptradd v9,v2\n store align 1 v6,v10\n v11 = ptradd v10,v2\n store align 1 v7,v11\n return\n}}\n",
                if same == 1 { "v5" } else { "v6" },
                if count == 3 { ",v7" } else { "" }
            );
        }
    }
    s
}
#[test]
fn sha256_types_features_aliases_and_spills() {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    assert!(keel_codegen::x86_64::compile(&m).is_err());
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    assert!(
        keel_ir::verify_module(
            &keel_ir::parse(&source().replace("load.i32x4", "load.i64x2")).unwrap()
        )
        .is_err()
    );
    let f = Features::all_known();
    let dir = std::env::temp_dir().join(format!("keel-sha256-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut harness =
        String::from("#include <immintrin.h>\n#include <string.h>\n#include <stdint.h>\n");
    for op in ["msg1", "msg2", "rnds2"] {
        for same in 0..2 {
            harness += &format!("extern void sha_{op}_{same}(void*,void*);\n");
        }
    }
    harness += "int main(void){uint32_t a[12],out[16];for(unsigned n=0;n<37;n++){for(unsigned i=0;i<12;i++)a[i]=(i+1)*0x9e3779b9u+n*0x1234567u;__m128i x=_mm_loadu_si128((void*)a),y=_mm_loadu_si128((void*)(a+4)),z=_mm_loadu_si128((void*)(a+8)),want;";
    for op in ["msg1", "msg2", "rnds2"] {
        for same in 0..2 {
            harness += &format!(
                "want=_mm_sha256{op}_epu32(x,{}{});sha_{op}_{same}(a,out);if(memcmp(out,&want,16)||memcmp(out+4,a,48))return 1;",
                if same == 1 { "x" } else { "y" },
                if op == "rnds2" { ",z" } else { "" }
            );
        }
    }
    harness += "}return 0;}";
    fs::write(dir.join("harness.c"), harness).unwrap();
    for tier in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let selector = FeatureSelector {
                    inner: &tier,
                    features: f,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &alloc,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
                fs::write(dir.join("sha.o"), object).unwrap();
                if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                    #[cfg(target_arch = "x86_64")]
                    if !std::is_x86_feature_detected!("sha") {
                        eprintln!("SKIP native SHA: hardware unavailable");
                        continue;
                    }
                    let output = Command::new("cc")
                        .args(["-msha", "-O2"])
                        .arg(dir.join("harness.c"))
                        .arg(dir.join("sha.o"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(
                        Command::new(dir.join("run")).status().unwrap().success(),
                        "{tier:?} {alloc:?}"
                    );
                }
            }
        }
    }
}

/// Pin only the unrelated wide SSA value, leaving every SHA operand spilled.
/// This makes the implicit XMM0 staging conflict independent of spill costs.
struct PinWideSimd0(AllocationTier);
impl keel_codegen::regalloc::RegisterAllocator for PinWideSimd0 {
    fn name(&self) -> &'static str {
        "sha-wide-simd0"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_codegen::regalloc::Loc;
        use keel_ir::{EntityRef, InstData};
        let spilled = keel_codegen::regalloc::AllocationInput {
            pools: &EMPTY,
            ..*input
        };
        let mut allocation = self.0.allocate(&spilled);
        let load = input
            .func
            .layout_insts()
            .find_map(|(_, inst)| match input.func.inst_data(inst) {
                InstData::Load { ty, .. } if ty.is_vector() && ty.bytes().unwrap() >= 32 => {
                    Some(inst)
                }
                _ => None,
            })
            .unwrap();
        let value = input.func.inst_results(load)[0];
        assert!(
            !allocation.locs[0]
                .iter()
                .any(|loc| *loc == Some(Loc::FReg(0)))
        );
        allocation.locs[0][value.index()] = Some(Loc::FReg(0));
        assert_eq!(allocation.loc(value), Some(Loc::FReg(0)));
        allocation
    }
}

#[test]
fn sha256_rounds_preserve_pinned_ymm0_zmm0_and_every_third_input_alias() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let mut source = String::new();
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for (kind, shape, bytes) in [("ymm", "i32x8", 32), ("zmm", "i32x16", 64)] {
        for alias in 0..4 {
            let (second, third) = match alias {
                0 => ("v8", "v9"),
                1 => ("v8", "v7"),
                2 => ("v8", "v8"),
                _ => ("v7", "v7"),
            };
            let name = format!("sha_pin_{kind}_{alias}");
            source += &format!(
                "func @{name}(ptr,ptr) target_features(\"avx512f,sha\") {{\nentry(v0:ptr,v1:ptr):\n v2=load.{shape} align 1 v0\n v3=iconst.i64 64\n v4=ptradd v0,v3\n v5=iconst.i64 16\n v6=ptradd v4,v5\n v7=load.i32x4 align 1 v4\n v8=load.i32x4 align 1 v6\n v10=ptradd v6,v5\n v9=load.i32x4 align 1 v10\n v11=intrinsic x86_sha256rnds2(v7,{second},{third})\n store align 1 v11,v1\n v12=ptradd v1,v5\n store align 1 v7,v12\n v13=ptradd v12,v5\n store align 1 v8,v13\n v14=ptradd v13,v5\n store align 1 v9,v14\n v15=ptradd v1,v3\n store align 1 v2,v15\n return\n}}\n"
            );
            harness += &format!("extern void {name}(const void*,void*);\n");
            let (second, third) = match alias {
                0 => ("b", "c"),
                1 => ("b", "a"),
                2 => ("b", "b"),
                _ => ("a", "a"),
            };
            checks += &format!(
                "{{unsigned char out[160];memset(out,0xa5,sizeof out);__m128i wanted=_mm_sha256rnds2_epu32(a,{second},{third});{name}(data,out);if(memcmp(out,&wanted,16)||memcmp(out+16,data+64,48)||memcmp(out+64,data,{bytes})){{fprintf(stderr,\"{name} mismatch\\n\");return 1;}}for(unsigned i=64+{bytes};i<sizeof out;i++)if(out[i]!=0xa5)return 2;}}\n"
            );
        }
    }
    harness += &format!(
        "int main(void){{unsigned char data[112],original[112];for(unsigned sample=0;sample<37;sample++){{for(unsigned i=0;i<sizeof data;i++)data[i]=(unsigned char)(sample*31+i*13+(i/16)*67);memcpy(original,data,sizeof data);__m128i a=_mm_loadu_si128((const void*)(data+64)),b=_mm_loadu_si128((const void*)(data+80)),c=_mm_loadu_si128((const void*)(data+96));{checks}if(memcmp(data,original,sizeof data))return 3;}}return 0;}}\n"
    );
    let original = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f,sha").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "keel-sha-pin-wide0-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native =
        native && std::is_x86_feature_detected!("sha") && std::is_x86_feature_detected!("avx512f");
    if native {
        let result = Command::new("cc")
            .args(["-O2", "-msha", "-mavx512f", "-c"])
            .arg(dir.join("oracle.c"))
            .arg("-o")
            .arg(dir.join("oracle.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let pools = Pools {
        float_caller_saved: &[0],
        ..EMPTY
    };
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for optimize in [false, true] {
            let mut module = original.clone();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let pin = PinWideSimd0(allocator);
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
                allocator: &pin,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, &pools, &pipeline).unwrap();
            let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                normalized.contains("movdqu xmm15, xmm0")
                    && normalized.contains("movdqu xmm0, xmm15"),
                "{text}"
            );
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, &pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(dir.join("sha.o"), object).unwrap();
            if native {
                let result = Command::new("cc")
                    .arg(dir.join("oracle.o"))
                    .arg(dir.join("sha.o"))
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
                    "{allocator:?} opt{optimize}: {} {}",
                    result.status,
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
}
