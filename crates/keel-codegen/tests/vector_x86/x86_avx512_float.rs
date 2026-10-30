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
            "0,0x80000000u,0x3f800000u,0xc0000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x00800000u,0x3f000000u,0xbf000000u,0x3fc00000u,0xbfc00000u,0x40200000u,0xc0200000u,0x7f812345u,0xffc54321u",
            "sqrtf",
        ),
        (
            "f64x8",
            "double",
            "uint64_t",
            8,
            "0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xc000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x0010000000000000ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x4004000000000000ULL,0xc004000000000000ULL,0x7ff0123456789abcULL,0xfff8543212345678ULL",
            "sqrt",
        ),
    ] {
        for (operation, symbol) in [
            ("vfadd", "+"),
            ("vfsub", "-"),
            ("vfmul", "*"),
            ("vfdiv", "/"),
            ("vfsqrt", ""),
            ("vfneg", ""),
            ("vfabs", ""),
            ("vfcopysign", ""),
            ("vfceil", ""),
            ("vffloor", ""),
            ("vftrunc", ""),
            ("vfnearest", ""),
        ] {
            let name = format!("{shape}_{operation}");
            let operands = if symbol.is_empty() && operation != "vfcopysign" {
                "v3"
            } else {
                "v3,v4"
            };
            source += &format!(
                "func @{name}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = {operation} {operands}\n store align 1 v5,v2\n v6 = iconst.i64 64\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = ptradd v7,v6\n store align 1 v4,v8\n return\n}}\n"
            );
            harness += &format!("extern void {name}(void*,void*,void*);\n");
            let scalar = if ctype == "float" { "f" } else { "" };
            let expected = match operation {
                "vfsqrt" => format!("{sqrt}(a[i])"),
                "vfneg" => "-a[i]".into(),
                "vfabs" => format!("fabs{scalar}(a[i])"),
                "vfcopysign" => format!("copysign{scalar}(a[i],b[i])"),
                "vfceil" => format!("ceil{scalar}(a[i])"),
                "vffloor" => format!("floor{scalar}(a[i])"),
                "vftrunc" => format!("trunc{scalar}(a[i])"),
                "vfnearest" => format!("nearbyint{scalar}(a[i])"),
                _ => format!("a[i]{symbol}b[i]"),
            };
            let nan = if matches!(operation, "vfneg" | "vfabs" | "vfcopysign") {
                "if(memcmp(&want,out+i,sizeof(want)))return 4;"
            } else {
                "if(!isnan(out[i]))return 2;"
            };
            cases += &format!(
                "{{{uint} bits[16]={{{bits}}};{ctype} a[{lanes}],b[{lanes}],out[3*{lanes}];for(unsigned shift=0;shift<16;shift++){{for(unsigned i=0;i<{lanes};i++){{memcpy(a+i,bits+((i+shift)%16),sizeof(*a));memcpy(b+i,bits+((i+shift*3)%16),sizeof(*b));}}{name}(a,b,out);if(memcmp(out+{lanes},a,64)||memcmp(out+2*{lanes},b,64))return 1;for(unsigned i=0;i<{lanes};i++){{{ctype} want={expected};if(isnan(want)){{{nan}}}else if(memcmp(&want,out+i,sizeof(want)))return 3;}}}}}}\n"
            );
        }
    }
    for (shape, ctype, lanes, scalar, a, b) in [
        (
            "f32x16",
            "float",
            16,
            "fmaf",
            "0x1.000002p0f",
            "0x1.fffffcp-1f",
        ),
        (
            "f64x8",
            "double",
            8,
            "fma",
            "0x1.0000000000001p0",
            "0x1.ffffffffffffep-1",
        ),
    ] {
        source += &format!(
            "func @fused_{shape}(ptr,ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.{shape} align 1 v0\n v5 = load.{shape} align 1 v1\n v6 = load.{shape} align 1 v2\n v7 = fma v4,v5,v6\n store align 1 v7,v3\n v8 = iconst.i64 64\n v9 = ptradd v3,v8\n store align 1 v4,v9\n v10 = ptradd v9,v8\n store align 1 v5,v10\n v11 = ptradd v10,v8\n store align 1 v6,v11\n return\n}}\n"
        );
        harness += &format!("extern void fused_{shape}(void*,void*,void*,void*);\n");
        cases += &format!(
            "{{ {ctype} a[{lanes}],b[{lanes}],c[{lanes}],out[4*{lanes}];for(unsigned i=0;i<{lanes};i++){{a[i]={a};b[i]={b};c[i]=-1;}}fused_{shape}(a,b,c,out);for(unsigned i=0;i<{lanes};i++){{{ctype} want={scalar}(a[i],b[i],c[i]);volatile {ctype} product=a[i]*b[i];if(want==product+c[i])return 5;if(memcmp(&want,out+i,sizeof(want)))return 6;}}if(memcmp(out+{lanes},a,64)||memcmp(out+2*{lanes},b,64)||memcmp(out+3*{lanes},c,64))return 7;}}\n"
        );
    }
    harness += &format!("int main(void){{{cases}return 0;}}\n");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.fma);
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
                "vaddps",
                "vsubps",
                "vmulps",
                "vdivps",
                "vsqrtps",
                "vaddpd",
                "vsubpd",
                "vmulpd",
                "vdivpd",
                "vsqrtpd",
                "vrndscaleps",
                "vrndscalepd",
                "vpandnd",
                "vpxord",
            ] {
                assert!(text.contains(op), "missing {op}: {text}");
            }
            assert_eq!(
                text.matches("vfmadd213").count(),
                2,
                "ordinary arithmetic must remain unfused: {text}"
            );
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
