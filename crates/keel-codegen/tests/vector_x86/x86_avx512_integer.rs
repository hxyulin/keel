//! AVX512F primitives need no AVX2 permission and preserve overlapping inputs.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};

#[test]
fn unmasked_integer512_native_and_spilled_oracle() {
    let mut source = String::new();
    let mut harness = String::from("#include <stdint.h>\n#include <string.h>\n");
    let operations = [
        ("viadd", "+"),
        ("visub", "-"),
        ("vand", "&"),
        ("vor", "|"),
        ("vxor", "^"),
        // AVX512F has no qword multiply; preserve the lane fallback.
        ("vimul", "*"),
    ];
    let mut cases = String::new();
    for (shape, scalar, ctype, lanes) in [
        ("i32x16", "i32", "uint32_t", 16),
        ("i64x8", "i64", "uint64_t", 8),
    ] {
        for (operation, symbol) in operations {
            for same in [false, true] {
                let name = format!("{shape}_{operation}_{same}");
                let b = if same { "v3" } else { "v4" };
                source += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = {operation} v3,{b}\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n"
                );
                harness += &format!("extern void {name}(void*,void*,void*);\n");
                let rhs = if same { "a[i]" } else { "b[i]" };
                cases += &format!(
                    "{{ {ctype} a[{lanes}],b[{lanes}],out[{lanes}*3];for(unsigned i=0;i<{lanes};i++){{a[i]=({ctype})0xdeadbeef92345678ULL+i*17;b[i]=({ctype})0xa5a5123456789abcULL-i*31;}} {name}(a,b,out);for(unsigned i=0;i<{lanes};i++)if(out[i]!=({ctype})(a[i]{symbol}{rhs})||out[i+{lanes}]!=a[i]||out[i+2*{lanes}]!=b[i])return 1; }}\n"
                );
            }
        }
        let name = format!("splat_{shape}");
        let other = if shape == "i32x16" { "i64x8" } else { "i32x16" };
        source += &format!(
            "func @{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{scalar} align 1 v0\n v3 = vsplat.{shape} v2\n v4 = bitcast.{other} v3\n store align 1 v4,v1\n return\n}}\n"
        );
        harness += &format!("extern void {name}(void*,void*);\n");
        cases += &format!(
            "{{ {ctype} a=({ctype})0x81abcdef12345678ULL,out[{lanes}];{name}(&a,out);for(unsigned i=0;i<{lanes};i++)if(out[i]!=a)return 2; }}\n"
        );
    }
    harness += &format!("int main(void){{\n{cases}return 0;}}\n");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2);
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
        std::env::temp_dir().join(format!("keel-avx512-integer-{}", std::process::id()));
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
            for opcode in [
                "vpaddd",
                "vpaddq",
                "vpsubd",
                "vpsubq",
                "vpandd",
                "vpord",
                "vpxord",
                "vpbroadcastd",
                "vpbroadcastq",
            ] {
                assert!(text.contains(opcode), "missing {opcode}: {text}");
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
