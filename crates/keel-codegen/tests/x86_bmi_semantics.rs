//! BMI values have explicit typed access; no implicit architectural flags result.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn operations() -> [(&'static str, u8); 3] {
    [("bextr", 1), ("pdep", 2), ("pext", 2)]
}
fn source() -> String {
    let mut s = String::new();
    for (op, feature) in operations() {
        for bits in [32, 64] {
            let name = format!("{op}{bits}");
            let intrinsic = format!("x86_bmi{feature}_{name}");
            s += &format!(
                "func @{name}(i{bits},i{bits})->i{bits} {{\nblock0(v0:i{bits},v1:i{bits}):\n v2 = intrinsic {intrinsic}(v0,v1)\n return v2\n}}\nfunc @{name}_alias(i{bits})->i{bits} {{\nblock0(v0:i{bits}):\n v1 = intrinsic {intrinsic}(v0,v0)\n return v1\n}}\nfunc @{name}_live(i{bits},i{bits})->i{bits} {{\nblock0(v0:i{bits},v1:i{bits}):\n v2 = intrinsic {intrinsic}(v0,v1)\n v3 = xor v2,v0\n v4 = xor v3,v1\n return v4\n}}\n"
            );
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern int printf(const char*,...);\nstatic uint64_t reference(uint64_t x,uint64_t mask,unsigned bits,unsigned op){uint64_t all=bits==64?~0ULL:0xffffffffULL;x&=all;mask&=all;if(op==0){unsigned start=mask&255,n=(mask>>8)&255;if(start>=bits)return 0;if(n>bits-start)n=bits-start;return(x>>start)&(n==64?~0ULL:((1ULL<<n)-1));}uint64_t r=0,bit=1;while(mask){uint64_t low=mask&-mask;if(op==1){if(x&bit)r|=low;}else{if(x&low)r|=bit;}bit<<=1;mask&=mask-1;}return r;}\n",
    );
    let mut main = String::from("int main(void){uint64_t rng=0x123456789abcdef0ULL;\n");
    for (index, (op, _)) in operations().into_iter().enumerate() {
        for bits in [32, 64] {
            let name = format!("{op}{bits}");
            c += &format!(
                "extern uint{bits}_t {name}(uint{bits}_t,uint{bits}_t);extern uint{bits}_t {name}_alias(uint{bits}_t);extern uint{bits}_t {name}_live(uint{bits}_t,uint{bits}_t);\n"
            );
            let n = if index == 0 { 65536 } else { 20000 };
            main += &format!(
                "for(unsigned k=0;k<{n};k++){{rng=rng*6364136223846793005ULL+1442695040888963407ULL;uint{bits}_t x=(uint{bits}_t)rng;uint{bits}_t m=(uint{bits}_t){};uint{bits}_t e=(uint{bits}_t)reference(x,m,{bits},{index});if({name}(x,m)!=e){{printf(\"{name} k%u\\n\",k);return 10;}}if({name}_alias(x)!=(uint{bits}_t)reference(x,x,{bits},{index}))return 11;if({name}_live(x,m)!=(uint{bits}_t)(e^x^m))return 12;}}\n",
                if index == 0 {
                    "((rng&~65535ULL)|k)"
                } else {
                    "((rng>>19)^(rng<<17))"
                }
            );
            main += &format!(
                "for(unsigned i=0;i<{bits};i++)for(unsigned j=0;j<{bits};j++){{uint{bits}_t x=(uint{bits}_t)(1ULL<<i),m=(uint{bits}_t)(1ULL<<j);if({name}(x,m)!=(uint{bits}_t)reference(x,m,{bits},{index}))return 13;}}\n"
            );
        }
    }
    c + &main + "return 0;}\n"
}
fn features() -> Features {
    Features {
        bmi1: true,
        bmi2: true,
        ..Features::NONE
    }
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features: f,
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
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((b, text))
}
#[test]
fn bmi_typed_semantics_features_aliases_and_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("bmi.ir"), source()).unwrap();
        fs::write(d.join("bmi.c"), harness()).unwrap();
    }
    let d = common::temp("bmi-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("bmi.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(d.join("bmi.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, features()).unwrap();
                for op in ["bextr", "pdep", "pext"] {
                    assert!(text.contains(op));
                }
                if let Some(d) = &export {
                    let name = format!("bmi-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
    for f in [
        Features::NONE,
        Features {
            bmi1: true,
            ..Features::NONE
        },
        Features {
            bmi2: true,
            ..Features::NONE
        },
    ] {
        assert!(object(SelectionTier::Baseline, AllocationTier::Full, false, f).is_err());
    }
    let bad = source().replace("x86_bmi1_bextr32(v0,v1)", "x86_bmi1_bextr64(v0,v1)");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let m = keel_ir::parse(&source()).unwrap();
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_bmi_bit_extraction_deposit_reference() {
    if !std::is_x86_feature_detected!("bmi1") || !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    let d = common::temp("bmi-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("bmi.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, features()).unwrap();
                fs::write(d.join("bmi.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("bmi.c"))
                        .arg(d.join("bmi.o"))
                        .arg("-o")
                        .arg(d.join("bmi")),
                );
                common::command(&mut Command::new(d.join("bmi")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
