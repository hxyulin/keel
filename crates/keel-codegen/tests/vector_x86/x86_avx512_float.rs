//! Scalar floating oracles include signed zeros, infinities and NaNs.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[test]
fn floating512_arithmetic_and_sqrt_preserve_live_sources() {
    let mut source = String::new();
    let mut harness = String::from("#include <stdint.h>\n#include <string.h>\n#include <math.h>\n");
    let mut cases = String::new();
    for (shape, ctype, uint, lanes, bits, sqrt) in [
        (
            "f32x16",
            "float",
            "uint32_t",
            16,
            "0,0x80000000u,0x3f800000u,0xc0000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x00800000u",
            "sqrtf",
        ),
        (
            "f64x8",
            "double",
            "uint64_t",
            8,
            "0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xc000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x0010000000000000ULL",
            "sqrt",
        ),
    ] {
        for (operation, symbol) in [
            ("vfadd", "+"),
            ("vfsub", "-"),
            ("vfmul", "*"),
            ("vfdiv", "/"),
            ("vfsqrt", ""),
        ] {
            let name = format!("{shape}_{operation}");
            let operands = if operation == "vfsqrt" { "v3" } else { "v3,v4" };
            source += &format!(
                "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = {operation} {operands}\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n"
            );
            harness += &format!("extern void {name}(void*,void*,void*);\n");
            let expected = if operation == "vfsqrt" {
                format!("{sqrt}(a[i])")
            } else {
                format!("a[i]{symbol}b[i]")
            };
            cases += &format!(
                "{{{uint} bits[8]={{{bits}}};{ctype} a[{lanes}],b[{lanes}],out[3*{lanes}];for(unsigned shift=0;shift<8;shift++){{for(unsigned i=0;i<{lanes};i++){{memcpy(a+i,bits+(i%8),sizeof(*a));memcpy(b+i,bits+((i+shift)%8),sizeof(*b));}}{name}(a,b,out);if(memcmp(out+{lanes},a,64)||memcmp(out+2*{lanes},b,64))return 1;for(unsigned i=0;i<{lanes};i++){{{ctype} want={expected};if(isnan(want)){{if(!isnan(out[i]))return 2;}}else if(memcmp(&want,out+i,sizeof(want)))return 3;}}}}}}\n"
            );
        }
    }
    harness += &format!("int main(void){{{cases}return 0;}}\n");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.fma);
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
    let dir = std::env::temp_dir().join(format!("keel-avx512-float-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &empty, &one] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &alloc,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            for op in [
                "vaddps", "vsubps", "vmulps", "vdivps", "vsqrtps", "vaddpd", "vsubpd", "vmulpd",
                "vdivpd", "vsqrtpd",
            ] {
                assert!(text.contains(op), "missing {op}: {text}");
            }
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            fs::write(dir.join("float.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native AVX512F");
                    continue;
                }
                let output = Command::new("cc")
                    .args(["-O2", "-fno-fast-math", "-ffp-contract=off"])
                    .arg(dir.join("harness.c"))
                    .arg(dir.join("float.o"))
                    .args(["-lm", "-o"])
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
                    "{alloc:?}"
                );
            }
        }
    }
}
