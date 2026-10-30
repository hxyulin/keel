//! GCC and Clang qualify ZMM aggregate calls, returns, tails and overflow arguments.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(policy: &str) -> String {
    let tag = format!("x86_vector_abi({policy})");
    let mut s = String::new();
    for (n, layout) in [
        ("single", "{i64x8}"),
        ("nested", "{{f32x16}}"),
        ("array", "{[1 x i64x8]}"),
        ("empty", "{{i64x8},[0 x i8]}"),
    ] {
        let sig = format!("(ptr sret {layout},ptr byval {layout})");
        s += &format!(
            "decl @external_{n}{sig} {tag}\nfunc @identity_{n}{sig} {tag} {{\nblock0(v0:ptr,v1:ptr):\n v2=iconst.i64 64\n intrinsic memcpy(v0,v1,v2)\n return\n}}\nfunc @forward_{n}{sig} {tag} {{\nblock0(v0:ptr,v1:ptr):\n call @external_{n}(v0,v1)\n return\n}}\nfunc @tail_{n}{sig} {tag} {{\nblock0(v0:ptr,v1:ptr):\n return_call @external_{n}(v0,v1)\n}}\n"
        );
        for tail in [false, true] {
            let prefix = if tail {
                "return_call_indirect"
            } else {
                "call_indirect"
            };
            let end = if tail { "" } else { " return\n" };
            s += &format!(
                "func @indirect_{n}_{tail}(ptr sret {layout},ptr byval {layout},ptr) {tag} {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n {prefix} v2(v0,v1) : {sig} {tag}\n{end}}}\n"
            );
        }
        let types = std::iter::repeat_n(format!("ptr byval {layout}"), 9)
            .collect::<Vec<_>>()
            .join(",");
        let params = (0..10)
            .map(|i| format!("v{i}:ptr"))
            .collect::<Vec<_>>()
            .join(",");
        s += &format!(
            "decl @external_many_{n}(ptr sret {layout},{types}) {tag}\nfunc @ninth_{n}(ptr sret {layout},{types}) {tag} {{\nblock0({params}):\n v10=iconst.i64 64\n intrinsic memcpy(v0,v9,v10)\n return\n}}\nfunc @send_many_{n}(ptr sret {layout},ptr) {tag} {{\nblock0(v0:ptr,v1:ptr):\n call @external_many_{n}(v0,v1,v1,v1,v1,v1,v1,v1,v1,v1)\n return\n}}\n"
        );
        s += &format!(
            "decl @external_var_{n}(ptr sret {layout},i32,...) {tag}\nfunc @send_var_{n}(ptr sret {layout},ptr) {tag} {{\nblock0(v0:ptr,v1:ptr):\n v2=iconst.i32 1\n call @external_var_{n}(v0,v2,v1 : ptr byval {layout})\n return\n}}\nfunc @read_var_{n}(ptr sret {layout},i32,...) {tag} {{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 64, align 64\nblock0(v0:ptr,v1:i32):\n v2=stack_addr ss0\n intrinsic va_start(v2)\n v3=va_arg_aggregate {layout},v2,ss1 {tag}\n v4=iconst.i64 64\n intrinsic memcpy(v0,v3,v4)\n return\n}}\n"
        );
    }
    s
}
fn harness() -> String {
    let mut s = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <stdarg.h>\ntypedef unsigned char V __attribute__((vector_size(64)));\ntypedef struct {V v;} single;typedef struct {struct{V v;}n;} nested;typedef struct {V v[1];} array;typedef struct {struct{V v;}n;char z[0];} empty;\n",
    );
    let mut cases = String::new();
    for n in ["single", "nested", "array", "empty"] {
        s += &format!(
            "extern {n} identity_{n}({n}),forward_{n}({n}),tail_{n}({n}),indirect_{n}_false({n},{n}(*fn)({n})),indirect_{n}_true({n},{n}(*fn)({n}));\n{n} external_{n}({n} x){{unsigned char*p=(void*)&x;for(unsigned i=0;i<64;i++)p[i]^=0xa5;return x;}}\n"
        );
        let types = std::iter::repeat_n(n, 9).collect::<Vec<_>>().join(",");
        let params = (0..9)
            .map(|i| format!("{n} a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        s += &format!(
            "extern {n} ninth_{n}({types}),send_many_{n}(void*),send_var_{n}(void*),read_var_{n}(int,...);\n{n} external_many_{n}({params}){{return a8;}}\n{n} external_var_{n}(int count,...){{va_list ap;va_start(ap,count);{n} r=va_arg(ap,{n});va_end(ap);return r;}}\n"
        );
        let args = (0..9)
            .map(|i| format!("a[{i}]"))
            .collect::<Vec<_>>()
            .join(",");
        cases += &format!(
            "{{{n} a[9],r,want;for(unsigned j=0;j<9;j++)for(unsigned i=0;i<64;i++)((unsigned char*)(a+j))[i]=i*17+j*29;want=external_{n}(a[0]);r=identity_{n}(a[0]);if(memcmp(&r,a,64))return 1;r=forward_{n}(a[0]);if(memcmp(&r,&want,64))return 2;r=tail_{n}(a[0]);if(memcmp(&r,&want,64))return 3;r=indirect_{n}_false(a[0],external_{n});if(memcmp(&r,&want,64))return 4;r=indirect_{n}_true(a[0],external_{n});if(memcmp(&r,&want,64))return 5;r=ninth_{n}({args});if(memcmp(&r,a+8,64))return 6;r=send_many_{n}(a);if(memcmp(&r,a,64))return 7;r=send_var_{n}(a);if(memcmp(&r,a,64))return 8;r=read_var_{n}(1,a[0]);if(memcmp(&r,a,64))return 9;}}\n"
        );
    }
    s + &format!("int main(void){{{cases}return 0;}}\n")
}
#[test]
fn zmm_aggregate_wrappers_match_gnu_and_clang() {
    let features = Features::NONE.with_features("avx512f").unwrap();
    let dir = std::env::temp_dir().join(format!("keel-zmm-wrappers-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness()).unwrap();
    for (policy, cc) in [("gnu", "gcc"), ("clang", "clang")] {
        let module = keel_ir::parse(&source(policy)).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let baseline = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE,
        };
        let pipeline = CodegenPipeline {
            selector: &baseline,
            allocator: &AllocationTier::Full,
        };
        assert!(keel_codegen::x86_64::compile_with_pipeline(&module, &EMPTY, &pipeline).is_err());
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let selector = FeatureSelector {
                    inner: &SelectionTier::Full,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &alloc,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                fs::write(dir.join("wrappers.o"), object).unwrap();
                if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                    #[cfg(target_arch = "x86_64")]
                    if !std::is_x86_feature_detected!("avx512f") {
                        eprintln!("SKIP native ZMM wrappers: AVX512F unavailable");
                        continue;
                    }
                    let output = Command::new(cc)
                        .args(["-O2", "-mavx512f"])
                        .arg(dir.join("harness.c"))
                        .arg(dir.join("wrappers.o"))
                        .arg("-o")
                        .arg(dir.join("run"))
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{policy}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let run = Command::new(dir.join("run")).status().unwrap();
                    assert!(run.success(), "{policy} {alloc:?}: {run}");
                }
            }
        }
    }
}
