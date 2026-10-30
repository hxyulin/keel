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
