//! Widening selects the requested source lanes; narrowing zeros unused lanes.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};

#[test]
fn avx512_integer_conversions_preserve_lanes_and_zero_unused_bits() {
    let mut source = String::new();
    let mut harness = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut cases = String::new();
    for (shape, lanes) in [("i32x8", 8), ("i32x16", 16)] {
        for (operation, signed, high) in [
            ("swiden_low", true, false),
            ("uwiden_low", false, false),
            ("swiden_high", true, true),
            ("uwiden_high", false, true),
        ] {
            let name = format!("{operation}_{shape}");
            source += &format!(
                "func @{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n v3 = {operation}.i64x8 v2\n store align 1 v3,v1\n v4 = iconst.i64 64\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\n"
            );
            harness += &format!("extern void {name}(void*,void*);\n");
            let offset = if high { lanes - 8 } else { 0 };
            let extension = if signed {
                "((x&0x80000000u)?UINT64_C(0xffffffff00000000):0)"
            } else {
                "0"
            };
            cases += &format!(
                "{{ uint32_t a[{lanes}];unsigned char out[128];uint32_t boundary[]={{0,1,0x7fffffffu,0x80000000u,0xffffffffu}};for(unsigned i=0;i<{lanes};i++)a[i]=boundary[i%5]^(i>=8?0x5a123456u:0);memset(out,0xa5,sizeof out);{name}(a,out);for(unsigned i=0;i<8;i++){{uint32_t x=a[i+{offset}];uint64_t expected=(uint64_t)x|{extension},got;memcpy(&got,out+i*8,8);if(got!=expected)return 1;}}if(memcmp(out+64,a,sizeof a))return 2;for(unsigned i=64+sizeof a;i<sizeof out;i++)if(out[i]!=0xa5)return 3; }}\n"
            );
        }
    }
    for (shape, lanes) in [("i32x8", 8), ("i32x16", 16)] {
        let name = format!("narrow_{shape}");
        source += &format!(
            "func @{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.i64x8 align 1 v0\n v3 = narrow_low.{shape} v2\n store align 1 v3,v1\n v4 = iconst.i64 64\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\n"
        );
        harness += &format!("extern void {name}(void*,void*);\n");
        cases += &format!(
            "{{ uint64_t a[8];unsigned char out[128];for(unsigned i=0;i<8;i++)a[i]=UINT64_C(0xdeadbeef81abcdef)+i*UINT64_C(0x100001234);memset(out,0xa5,sizeof out);{name}(a,out);for(unsigned i=0;i<{lanes};i++){{uint32_t expected=i<8?(uint32_t)a[i]:0,got;memcpy(&got,out+i*4,4);if(got!=expected)return 4;}}if(memcmp(out+64,a,64))return 5;for(unsigned i={lanes}*4;i<64;i++)if(out[i]!=0xa5)return 6; }}\n"
        );
    }
    source += "func @convert_chain(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n v2 = load.i32x16 align 1 v0\n v3 = swiden_high.i64x8 v2\n v4 = narrow_low.i32x16 v3\n store align 1 v4,v1\n return\n}\n";
    harness += "extern void convert_chain(void*,void*);\n";
    cases += "{uint32_t a[16],out[16];for(unsigned i=0;i<16;i++)a[i]=0x81234567u+i*0x1234;convert_chain(a,out);for(unsigned i=0;i<16;i++)if(out[i]!=(i<8?a[i+8]:0))return 7;}\n";
    harness += &format!("int main(void){{{cases}return 0;}}\n");
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[0],
        float_callee_saved: &[],
    };
    let directory =
        std::env::temp_dir().join(format!("keel-avx512-convert-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness).unwrap();
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool_name, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("empty", &empty),
            ("one", &one),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            for opcode in ["vpmovsxdq", "vpmovzxdq", "vpmovqd", "vextracti64x4"] {
                assert!(text.contains(opcode), "missing {opcode}");
            }
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            let name = format!("{allocator:?}-{pool_name}");
            let path = directory.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native: AVX512F unavailable");
                    continue;
                }
                let executable = directory.join(name);
                let output = Command::new("cc")
                    .arg(directory.join("harness.c"))
                    .arg(path)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(Command::new(executable).status().unwrap().success());
            }
        }
    }
}
