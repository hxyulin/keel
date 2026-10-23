//! SysV genuine MEMORY aggregates, including pointer-only function bodies.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::VectorType;
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source() -> String {
    let mut s = String::new();
    for t in VectorType::ALL_256 {
        s += &format!(
            "decl @check_{t}(ptr) -> i64\ndecl @c_read_{t}(ptr byval {{{t},i64}}) -> i64\ndecl @c_copy_{t}(ptr sret {{{t},i64}},ptr byval {{{t},i64}})\nfunc @read_{t}(ptr byval {{{t},i64}}) -> i64 {{\nblock0(v0:ptr):\n v1 = call @check_{t}(v0)\n return v1\n}}\nfunc @copy_{t}(ptr sret {{{t},i64}},ptr byval {{{t},i64}}) {{\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i64 64\n intrinsic memcpy(v0,v1,v2)\n return\n}}\nfunc @caller_{t}(ptr,ptr)->i64 {{\nblock0(v0:ptr,v1:ptr):\n v2 = call @c_read_{t}(v0)\n call @c_copy_{t}(v1,v0)\n return v2\n}}\n"
        );
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void){\n");
    for t in VectorType::ALL_256 {
        c += &format!(
            "typedef uint8_t V_{t} __attribute__((vector_size(32)));struct S_{t}{{V_{t} v;uint64_t tag;}};\nextern uint64_t read_{t}(struct S_{t});extern struct S_{t} copy_{t}(struct S_{t});extern uint64_t caller_{t}(const void*,void*);\nuint64_t check_{t}(const void*p){{const unsigned char*b=p;uint64_t r=0x12345678;for(unsigned i=0;i<40;i++)r=r*131+b[i];return r+((uintptr_t)p&31);}}\nuint64_t c_read_{t}(struct S_{t} v){{return check_{t}(&v);}}struct S_{t} c_copy_{t}(struct S_{t} v){{return v;}}\n"
        );
        main += &format!(
            "{{struct S_{t} a,b;for(unsigned k=0;k<256;k++){{memset(&a,k,sizeof(a));a.tag=0x123456789abcdef0ULL+k;uint64_t e=check_{t}(&a);if(read_{t}(a)!=e)return 10;b=copy_{t}(a);if(memcmp(&a,&b,40))return 11;memset(&b,0,sizeof(b));if(caller_{t}(&a,&b)!=e||memcmp(&a,&b,40))return 12;}}}}\n"
        );
    }
    c + &main + "return 0;}\n"
}
fn object(s: SelectionTier, a: AllocationTier, empty: bool) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let features = Features::for_level(CpuLevel::V3);
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
    let b = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (b, text)
}
#[test]
fn wide_memory_aggregates_emission_exports_and_alignment() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("aggregate.ir"), source()).unwrap();
        fs::write(d.join("aggregate.c"), harness()).unwrap();
    }
    let d = common::temp("wide-aggregate-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("aggregate.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx2",
                "-fsyntax-only",
            ])
            .arg(d.join("aggregate.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty);
                assert!(text.contains("and\trsp, -32"));
                if let Some(d) = &export {
                    let name = format!("aggregate-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_wide_memory_aggregate_interop() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("wide-aggregate-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("aggregate.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty);
                fs::write(d.join("aggregate.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx2"])
                        .arg(d.join("aggregate.c"))
                        .arg(d.join("aggregate.o"))
                        .arg("-o")
                        .arg(d.join("aggregate")),
                );
                common::command(&mut Command::new(d.join("aggregate")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
