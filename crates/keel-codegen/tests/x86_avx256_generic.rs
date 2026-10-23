//! Full generic-operation oracle. The same scalar C reference exercises every
//! supported wide shape, all lane widths, modulo shift counts, NaNs and zeros.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use std::{fs, path::PathBuf, process::Command};
#[path = "common/vector.rs"]
mod common;
#[path = "common/userspace.rs"]
mod userspace;
fn source() -> String {
    common::fixture_for(&keel_ir::VectorType::ALL_256)
}
fn harness() -> String {
    common::harness_for(&keel_ir::VectorType::ALL_256)
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    features: Features,
) -> (Vec<u8>, String) {
    object_for(&source(), s, a, empty, features)
}
fn object_for(
    source: &str,
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    features: Features,
) -> (Vec<u8>, String) {
    assert_eq!(features, Features::for_level(CpuLevel::V3));
    userspace::compile(source, CpuLevel::V3, s, a, empty)
}
#[test]
fn packed_wide_corpus_emits_typed_objects_and_native_oracle_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("generic.ir"), source()).unwrap();
        fs::write(d.join("generic.c"), harness()).unwrap();
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V3));
                for op in [
                    "vpaddb",
                    "vpmulld",
                    "vpmuludq",
                    "vpsllvd",
                    "vpsrlvq",
                    "vpshufb",
                    "vperm2i128",
                    "vpcmpeqq",
                    "vpcmpgtq",
                    "vroundps",
                    "vcmppd",
                ] {
                    assert!(text.contains(op), "missing{op}");
                }
                if let Some(d) = &export {
                    let n = format!("generic-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{n}.o")), b).unwrap();
                    fs::write(d.join(format!("{n}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn wide_reference_harness_has_strict_c_syntax() {
    let d = common::temp("wide-syntax");
    fs::create_dir_all(&d).unwrap();
    let c = d.join("generic.c");
    fs::write(&c, harness()).unwrap();
    common::command(
        Command::new("clang")
            .args(["-std=gnu11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(&c),
    );
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_full_generic_256_reference() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("wide-native");
    fs::create_dir_all(&d).unwrap();
    let c = d.join("generic.c");
    fs::write(&c, harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V3));
                let o = d.join("wide.o");
                let exe = d.join("wide");
                fs::write(&o, b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx2", "-fno-fast-math", "-ffp-contract=off"])
                        .arg(&c)
                        .arg(&o)
                        .arg("-lm")
                        .arg("-o")
                        .arg(&exe),
                );
                common::command(&mut Command::new(exe));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

fn abi_source() -> (String, String) {
    let (extra, mut c) = common::abi_fixture_for(false, &keel_ir::VectorType::ALL_256);
    let mut ir = source() + &extra;
    let mut checks = String::new();
    let mut decls = String::new();
    for shape in keel_ir::VectorType::ALL_256 {
        let op = if shape.lane_type().is_float() {
            "vfadd"
        } else {
            "viadd"
        };
        ir += &format!(
            "func @read_var_{shape}(i64, ...) -> {shape} {{\n ss0 = stack_slot 24, align 8\nblock0(v0: i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.{shape} v1\n v3 = va_arg.{shape} v1\n v4 = {op} v2, v3\n return v4\n}}\n"
        );
        // A stack-to-stack tail call must preserve the incoming 32-byte area.
        let types = std::iter::repeat_n(shape.to_string(), 10)
            .collect::<Vec<_>>()
            .join(", ");
        let params = (0..10)
            .map(|i| format!("v{i}: {shape}"))
            .collect::<Vec<_>>()
            .join(", ");
        let args = (0..10)
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        ir += &format!(
            "decl @tail_target_{shape}({types}) -> {shape}\nfunc @tail_stack_{shape}({types}) -> {shape} {{\nblock0({params}):\n return_call @tail_target_{shape}({args})\n}}\n"
        );
        decls += &format!(
            "extern V_{shape} read_var_{shape}(uint64_t,...);\nextern V_{shape} tail_stack_{shape}({});\nV_{shape} tail_target_{shape}({}) {{return a0+a9;}}\n",
            std::iter::repeat_n(format!("V_{shape}"), 10)
                .collect::<Vec<_>>()
                .join(","),
            (0..10)
                .map(|i| format!("V_{shape} a{i}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        checks += &format!(
            "{{V_{shape} a,b,r,e; for(unsigned i=0;i<sizeof(a)/sizeof(a[0]);i++){{a[i]=i+1;b[i]=i+2;}} e=a+b;r=read_var_{shape}(2,a,b);if(memcmp(&r,&e,32))return 30;r=tail_stack_{shape}(a,a,a,a,a,a,a,a,a,b);if(memcmp(&r,&e,32))return 31;}}\n"
        );
    }
    c = c.replace("int main(void){", &(decls + "int main(void){"));
    c = c.replace("return 0;}", &(checks + "return 0;}"));
    (ir, c)
}
#[test]
fn wide_sysv_abi_emission_and_exports() {
    let (ir, c) = abi_source();
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    let d = common::temp("wide-abi-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("abi.c"), &c).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-mavx2",
                "-std=gnu11",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(d.join("abi.c")),
    );
    fs::remove_dir_all(d).unwrap();
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("abi.ir"), &ir).unwrap();
        fs::write(d.join("abi.c"), &c).unwrap();
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object_for(&ir, s, a, empty, Features::for_level(CpuLevel::V3));
                assert!(text.contains("vzeroupper"));
                if let Some(d) = &export {
                    let n = format!("abi-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{n}.o")), b).unwrap();
                    fs::write(d.join(format!("{n}.s")), text).unwrap();
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_wide_sysv_abi_reference() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let (ir, c) = abi_source();
    let d = common::temp("wide-abi-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("abi.c"), c).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object_for(&ir, s, a, empty, Features::for_level(CpuLevel::V3));
                fs::write(d.join("abi.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx2", "-fno-fast-math", "-ffp-contract=off"])
                        .arg(d.join("abi.c"))
                        .arg(d.join("abi.o"))
                        .arg("-lm")
                        .arg("-o")
                        .arg(d.join("abi")),
                );
                common::command(&mut Command::new(d.join("abi")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
