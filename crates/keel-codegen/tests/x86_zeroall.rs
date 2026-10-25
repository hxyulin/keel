//! Whole SIMD-state clearing is distinct from killing live SSA values.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{Intrinsic, Type, VectorType};
use std::{fs, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const CUSTOM: Pools = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13, 14],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
fn types() -> Vec<Type> {
    [Type::F32, Type::F64]
        .into_iter()
        .chain(
            VectorType::ALL_128
                .into_iter()
                .chain(VectorType::ALL_256)
                .map(Type::Vector),
        )
        .collect()
}
fn source() -> String {
    let mut s =
        "func @zero_only() {\nblock0:\n intrinsic x86_avx_vzeroall()\n return\n}\n".to_string();
    for (i, t) in types().into_iter().enumerate() {
        s += &format!(
            "func @live{i}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{t} align 1 v0\n intrinsic x86_avx_vzeroall()\n store align 1 v2,v1\n return\n}}\n"
        );
    }
    s
}
fn compile(
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline)?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline)?;
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        obj,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    assert_eq!(
        text.lines()
            .filter(|l| l.trim().starts_with("vzeroall"))
            .count(),
        types().len() + 1
    );
    Ok((obj, text))
}
fn harness() -> String {
    let mut c = String::from(
        "typedef unsigned long long uint64_t;\nextern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int puts(const char*);\nextern void probe(void*);\n",
    );
    let mut asm = String::from(".text\n.globl probe\nprobe:\npushq %rbx\nmovq %rdi,%rbx\n");
    for i in 0..16 {
        asm +=
            &format!("vpcmpeqd %xmm{i},%xmm{i},%xmm{i}\nvinsertf128 $1,%xmm{i},%ymm{i},%ymm{i}\n");
    }
    asm += "call zero_only\n";
    for i in 0..16 {
        asm += &format!("vmovdqu %ymm{i},{}(%rbx)\n", i * 32);
    }
    asm += "vzeroupper\npopq %rbx\nret\n";
    c += &format!("__asm__({:?});\n", asm);
    for i in 0..types().len() {
        c += &format!("extern void live{i}(const void*,void*);\n");
    }
    c += "int main(void){unsigned char state[512];memset(state,0xa5,sizeof state);probe(state);for(unsigned j=0;j<512;j++)if(state[j])return 10;uint64_t rng=1;\n";
    for (i, t) in types().into_iter().enumerate() {
        let n = t.bytes().unwrap();
        c += &format!(
            "for(unsigned k=0;k<1024;k++)for(unsigned off=0;off<16;off++){{unsigned char a[96],o[96];for(unsigned j=0;j<96;j++){{rng=rng*6364136223846793005ULL+1;a[j]=rng>>32;o[j]=0xa5;}}live{i}(a+off,o+off);if(memcmp(a+off,o+off,{n}))return {};for(unsigned j=0;j<off;j++)if(o[j]!=0xa5)return 80;for(unsigned j=off+{n};j<96;j++)if(o[j]!=0xa5)return 81;}}\n",
            20 + i
        );
    }
    c += "puts(\"PASS VZEROALL state and live SSA values\");return 0;}\n";
    c
}
#[test]
fn zeroall_features_barrier_and_allocation() {
    assert!(!Intrinsic::X86AvxVzeroall.info().pure);
    assert!(
        compile(
            SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY,
            Features::NONE
        )
        .is_err()
    );
    let mut f = Features::NONE;
    f = f.with_features("avx").unwrap();
    compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).unwrap();
    assert!(
        keel_codegen::aarch64::compile(
            &keel_ir::parse("func @f(){\nblock0:\n intrinsic x86_avx_vzeroall()\n return\n}")
                .unwrap()
        )
        .is_err()
    );
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &CUSTOM] {
                compile(s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
            }
        }
    }
    let d = common::temp("zeroall-c-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("test.c")),
    );
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_zeroall_state_and_live_values() {
    assert!(std::is_x86_feature_detected!("avx"));
    let d = common::temp("zeroall-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &CUSTOM] {
                let (obj, _) = compile(s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("test.o"), obj).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx", "-mno-red-zone"])
                        .arg(d.join("test.c"))
                        .arg(d.join("test.o"))
                        .arg("-o")
                        .arg(d.join("test")),
                );
                common::command(&mut Command::new(d.join("test")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
