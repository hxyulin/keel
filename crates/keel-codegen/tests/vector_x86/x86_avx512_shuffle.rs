//! Concatenated-source permutations and exact lane transfers across ZMM quarters.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};

#[test]
fn avx512_shuffle_and_lanes_preserve_bits_under_pressure() {
    let mut source = String::new();
    let mut harness = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut cases = String::new();
    for (shape, ctype, count) in [
        ("i32x16", "uint32_t", 16usize),
        ("i64x8", "uint64_t", 8),
        ("f32x16", "uint32_t", 16),
        ("f64x8", "uint64_t", 8),
    ] {
        let setup = format!(
            "{ctype} a[{count}],b[{count}],out[{count}*4];for(unsigned i=0;i<{count};i++){{a[i]=({ctype})0xfff812347fc00001ULL+i*0x12345;b[i]=({ctype})0x7ff0012380000000ULL-i*0x713;}}"
        );
        let patterns: [Vec<usize>; 3] = [
            (0..count)
                .map(|i| if i % 2 == 0 { count - 1 - i } else { count + i })
                .collect(),
            (0..count)
                .map(|i| [0, count - 1, count, count * 2 - 1][i % 4])
                .collect(),
            (0..count).map(|i| (i * 7 + 3) % (count * 2)).collect(),
        ];
        for (pattern, indices) in patterns.iter().enumerate() {
            for same in [false, true] {
                let name = format!("shuffle_{shape}_{pattern}_{same}");
                let lanes = indices
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                source += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vshuffle v3,{},[{lanes}]\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n",
                    if same { "v3" } else { "v4" }
                );
                harness += &format!("extern void {name}(void*,void*,void*);\n");
                cases += &format!(
                    "{{{setup}unsigned indices[]={{{lanes}}};{name}(a,b,out);for(unsigned i=0;i<{count};i++){{unsigned j=indices[i];{ctype} expected=j<{count}?a[j]:{}[j-{count}];if(out[i]!=expected||out[i+{count}]!=a[i]||out[i+2*{count}]!=b[i])return 1;}}}}\n",
                    if same { "a" } else { "b" }
                );
            }
        }
        let boundaries: Vec<usize> = (0..count)
            .filter(|i| i % (count / 4) == 0 || i % (count / 4) == count / 4 - 1)
            .collect();
        for lane in boundaries {
            let name = format!("lanes_{shape}_{lane}");
            source += &format!(
                "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vextract v3,{lane}\n v6 = vextract v4,{}\n v7 = vinsert v3,v6,{lane}\n store align 1 v7,v2\n v8 = iconst.i64 64\n v9 = ptradd v2,v8\n store align 1 v3,v9\n v10 = ptradd v9,v8\n store align 1 v4,v10\n v11 = ptradd v10,v8\n store align 1 v5,v11\n return\n}}\n",
                count - lane - 1
            );
            harness += &format!("extern void {name}(void*,void*,void*);\n");
            cases += &format!(
                "{{{setup}{name}(a,b,out);for(unsigned i=0;i<{count};i++)if(out[i]!=(i=={lane}?b[{}]:a[i])||out[i+{count}]!=a[i]||out[i+2*{count}]!=b[i])return 2;if(out[3*{count}]!=a[{lane}])return 3;}}\n",
                count - lane - 1
            );
        }
    }
    harness += &format!("int main(void){{{cases}return 0;}}\n");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.sse41 && !features.avx512vl);
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
        std::env::temp_dir().join(format!("keel-avx512-shuffle-{}", std::process::id()));
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
            for opcode in ["vpermi2d", "vpermi2q", "vextracti32x4", "vinserti32x4"] {
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
