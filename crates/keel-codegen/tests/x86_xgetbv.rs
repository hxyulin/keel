//! Bounded user-mode XCR0 query; no XCR write or xstate-buffer contract.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const SOURCE: &str = r#"
func @read_xcr0()->i64 {
block0:
 v0 = intrinsic x86_xgetbv0()
 return v0
}
func @unused_xcr0() {
block0:
 v0 = intrinsic x86_xgetbv0()
 return
}
func @live_xcr0(ptr,ptr,i64)->i64 {
block0(v0:ptr,v1:ptr,v2:i64):
 v3 = load.f64 align 1 v0
 v4 = intrinsic x86_xgetbv0()
 store align 1 v3,v1
 v5 = iconst.i64 8
 v6 = ptradd v1,v5
 store align 1 v2,v6
 return v4
}
"#;
fn compile(
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(SOURCE).unwrap();
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
    let bytes = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    assert_eq!(text.lines().filter(|l| l.trim() == "xgetbv").count(), 3);
    Ok((bytes, text))
}
const C: &str = r#"
typedef unsigned int u32;typedef unsigned long long u64;
extern u64 read_xcr0(void);extern void unused_xcr0(void);extern u64 live_xcr0(const void*,void*,u64);extern int puts(const char*);
static u64 reference(void){u32 a,d;__asm__ volatile("xgetbv":"=a"(a),"=d"(d):"c"(0):"memory");return (u64)a|((u64)d<<32);}
int main(void){u32 a,b,c,d;__asm__ volatile("cpuid":"=a"(a),"=b"(b),"=c"(c),"=d"(d):"0"(1),"2"(0):"memory");if(!(c&(1u<<27)))return 80;u64 rng=1;for(unsigned k=0;k<4096;k++){u64 expected=reference(),in=rng,out[2]={0,0};rng=rng*6364136223846793005ULL+1;if(read_xcr0()!=expected)return 10;if(live_xcr0(&in,out,rng)!=expected||out[0]!=in||out[1]!=rng)return 11;unused_xcr0();if(reference()!=expected)return 12;}puts("PASS XGETBV0 exact results and live inputs");return 0;}
"#;
#[test]
fn xgetbv_features_unused_and_foreign() {
    let bad =
        "func @bad(i32)->i64 {\nblock0(v0:i32):\n v1 = intrinsic x86_xgetbv0(v0)\n return v1\n}";
    assert!(keel_ir::parse(bad).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    let m = keel_ir::parse(SOURCE).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    for level in [CpuLevel::V1, CpuLevel::V2] {
        assert!(
            compile(
                SelectionTier::Full,
                AllocationTier::Full,
                &EMPTY,
                Features::for_level(level)
            )
            .is_err()
        );
    }
    let mut f = Features::NONE;
    assert!(compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).is_err());
    f.xsave = true;
    assert!(compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).is_err());
    f.osxsave = true;
    compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                compile(s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
            }
        }
    }
    let d = common::temp("xgetbv-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), C).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-fsyntax-only"])
            .arg(d.join("test.c")),
    );
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_xgetbv0_exact_bits_and_live_inputs() {
    let d = common::temp("xgetbv-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), C).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for p in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let (obj, _) = compile(s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("test.o"), obj).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-march=x86-64"])
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
