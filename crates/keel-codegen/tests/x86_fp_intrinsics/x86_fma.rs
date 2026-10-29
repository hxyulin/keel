//! Explicit fused rounding, native selection, and conservative feature fallback.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{Type, VectorType};
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn types() -> [Type; 6] {
    [
        Type::F32,
        Type::F64,
        Type::Vector(VectorType::F32x4),
        Type::Vector(VectorType::F64x2),
        Type::Vector(VectorType::F32x8),
        Type::Vector(VectorType::F64x4),
    ]
}
fn source() -> String {
    let mut s = String::new();
    for ty in types() {
        let (mul, add) = if ty.is_vector() {
            ("vfmul", "vfadd")
        } else {
            ("fmul", "fadd")
        };
        let bytes = ty.bytes().unwrap();
        s += &format!(
            "func @numeric_{ty}(ptr,ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.{ty} align 1 v0\n v5 = load.{ty} align 1 v1\n v6 = load.{ty} align 1 v2\n v7 = fma v4,v5,v6\n store align 1 v7,v3\n v8 = iconst.i64 {bytes}\n v9 = ptradd v3,v8\n v10 = {mul} v4,v5\n v11 = {add} v10,v6\n store align 1 v11,v9\n v12 = ptradd v9,v8\n v13 = fma v4,v5,v4\n store align 1 v13,v12\n return\n}}\n"
        );
    }
    s
}
fn harness() -> String {
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n#include <math.h>\n");
    let mut main = String::from("int main(void){\n");
    for ty in types() {
        let lane = ty.vector().map_or(ty, |s| s.lane_type());
        let (ct, fma, vals) = if lane == Type::F32 {
            (
                "float",
                "fmaf",
                "0x1.0008p0f,0x1.fffp-1f,-1.0f,0.0f,-0.0f,INFINITY,-INFINITY,NAN,0x1p-149f,0x1.fffffep127f",
            )
        } else {
            (
                "double",
                "fma",
                "0x1.0000002p0,0x1.ffffffcp-1,-1.0,0.0,-0.0,INFINITY,-INFINITY,NAN,0x1p-1074,0x1.fffffffffffffp1023",
            )
        };
        let lanes = ty.vector().map_or(1, |s| s.lanes());
        c += &format!("extern void numeric_{ty}(const void*,const void*,const void*,void*);\n");
        main += &format!(
            "{{ {ct} vals[]={{{vals}}},a[{lanes}],b[{lanes}],cc[{lanes}],out[3*{lanes}];for(unsigned k=0;k<100;k++){{for(unsigned i=0;i<{lanes};i++){{a[i]=vals[(k+i)%10];b[i]=vals[(k/10+i+1)%10];cc[i]=vals[(k+2*i+2)%10];}}numeric_{ty}(a,b,cc,out);for(unsigned i=0;i<{lanes};i++){{volatile {ct} product=a[i]*b[i];{ct} e[3]={{{fma}(a[i],b[i],cc[i]),product+cc[i],{fma}(a[i],b[i],a[i])}};for(unsigned j=0;j<3;j++){{{ct} r=out[j*{lanes}+i];if(!(isnan(r)&&isnan(e[j]))&&memcmp(&r,&e[j],sizeof(r)))return 10+j;}}}}}}}}\n"
        );
    }
    c + &main + "return 0;}\n"
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    features: Features,
) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if empty {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (b, text)
}
#[test]
fn explicit_fma_native_and_fallback_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("fma.ir"), source()).unwrap();
        fs::write(d.join("fma.c"), harness()).unwrap();
    }
    for native in [false, true] {
        let mut features = Features::for_level(CpuLevel::V3);
        features.fma = native;
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    let (b, text) = object(s, a, empty, features);
                    if native {
                        for suffix in ["ss", "sd", "ps", "pd"] {
                            assert!(text.contains(&format!("vfmadd213{suffix}")));
                        }
                        assert!(!text.contains("fmaf@PLT"));
                    } else {
                        assert!(!text.contains("vfmadd"));
                        assert!(text.contains("fmaf"));
                    }
                    if let Some(d) = &export {
                        let name = format!("fma-{s:?}-{a:?}-{empty}-{native}");
                        fs::write(d.join(format!("{name}.o")), b).unwrap();
                        fs::write(d.join(format!("{name}.s")), text).unwrap();
                    }
                }
            }
        }
    }
    let m = keel_ir::parse(source().split("func @numeric_f32x8").next().unwrap()).unwrap();
    let arm = keel_codegen::aarch64::compile(&m).unwrap();
    assert!(arm.contains("fmadd"));
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_explicit_fma_reference() {
    if !std::is_x86_feature_detected!("fma") || !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("fma-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("fma.c"), harness()).unwrap();
    for native in [false, true] {
        let mut features = Features::for_level(CpuLevel::V3);
        features.fma = native;
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    let (b, _) = object(s, a, empty, features);
                    fs::write(d.join("fma.o"), b).unwrap();
                    common::command(
                        Command::new("clang")
                            .args([
                                "-O2",
                                "-mavx2",
                                "-mfma",
                                "-fno-fast-math",
                                "-ffp-contract=off",
                            ])
                            .arg(d.join("fma.c"))
                            .arg(d.join("fma.o"))
                            .arg("-lm")
                            .arg("-o")
                            .arg(d.join("fma")),
                    );
                    common::command(&mut Command::new(d.join("fma")));
                }
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn fused_reference_harness_syntax_and_scalar_baseline() {
    let d = common::temp("fma-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("fma.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(d.join("fma.c")),
    );
    fs::remove_dir_all(d).unwrap();
    let baseline = source()
        .split("func @numeric_f32x8")
        .next()
        .unwrap()
        .to_string();
    let m = keel_ir::parse(&baseline).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&p, Features::NONE).unwrap();
    let bad = keel_ir::parse(
        "func @bad(i32x4)->i32x4 {\nblock0(v0:i32x4):\n v1 = fma v0,v0,v0\n return v1\n}",
    )
    .unwrap();
    assert!(keel_ir::verify_module(&bad).is_err());
}
