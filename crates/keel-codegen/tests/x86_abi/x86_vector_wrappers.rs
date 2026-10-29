//! Single-vector aggregates are SSE+SSEUP: one XMM/YMM physical register.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::VectorType;
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn layouts(t: VectorType) -> [String; 4] {
    [
        format!("{{{t}}}"),
        format!("{{{{{t}}}}}"),
        format!("{{[1 x {t}]}}"),
        format!("{{[0 x i8],{{[1 x {t}]}}}}"),
    ]
}
fn source(wide: bool) -> String {
    let mut s = String::new();
    for t in VectorType::ALL_128
        .into_iter()
        .chain(VectorType::ALL_256)
        .filter(|t| wide || t.bytes() == 16)
    {
        for (n, l) in layouts(t).into_iter().enumerate() {
            if t.bytes() == 32 && n == 3 {
                continue;
            }
            let name = format!("{t}_{n}");
            let bytes = t.bytes();
            s += &format!(
                "func @tail_{name}(ptr sret {l},ptr byval {l}) {{\nblock0(v0:ptr,v1:ptr):\n return_call @c_id_{name}(v0,v1)\n}}\nfunc @tail_ind_{name}(ptr sret {l},ptr byval {l},ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n return_call_indirect v2(v0,v1) : (ptr sret {l},ptr byval {l})\n}}\n"
            );

            s += &format!(
                "decl @c_id_{name}(ptr sret {l},ptr byval {l})\ndecl @hash_{name}(ptr)->i64\nfunc @id_{name}(ptr sret {l},ptr byval {l}) {{\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i64 {bytes}\n intrinsic memcpy(v0,v1,v2)\n return\n}}\nfunc @caller_{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n call @c_id_{name}(v1,v0)\n return\n}}\n"
            );
            let types = std::iter::repeat_n(format!("ptr byval {l}"), 10)
                .collect::<Vec<_>>()
                .join(",");
            let params = (0..10)
                .map(|i| format!("v{i}:ptr"))
                .collect::<Vec<_>>()
                .join(",");
            s += &format!(
                "decl @c_many_{name}({types})->i64\nfunc @many_{name}({types})->i64 {{\nblock0({params}):\n"
            );
            let mut value = 10;
            for i in 0..10 {
                s += &format!(" v{} = call @hash_{name}(v{i})\n", 10 + i);
                if i != 0 {
                    s += &format!(" v{} = iadd v{},v{}\n", 20 + i, value, 10 + i);
                    value = 20 + i;
                }
            }
            s += &format!(
                " return v{value}\n}}\nfunc @many_call_{name}(ptr)->i64 {{\nblock0(v0:ptr):\n v1 = call @c_many_{name}({})\n return v1\n}}\n",
                std::iter::repeat_n("v0", 10).collect::<Vec<_>>().join(",")
            );
            for count in [7, 8] {
                let floats = std::iter::repeat_n("f64", count)
                    .collect::<Vec<_>>()
                    .join(",");
                let fparams = (0..count)
                    .map(|i| format!("v{i}:f64"))
                    .collect::<Vec<_>>()
                    .join(",");
                let fargs = (0..count)
                    .map(|i| format!("v{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                s += &format!(
                    "decl @checkmix_{name}_{count}(ptr,{floats},f64,i64)->i64\nfunc @mix_{name}_{count}({floats},ptr byval {l},f64,i64)->i64 {{\nblock0({fparams},v{count}:ptr,v{}:f64,v{}:i64):\n v{} = call @checkmix_{name}_{count}(v{count},{fargs},v{},v{})\n return v{}\n}}\n",
                    count + 1,
                    count + 2,
                    count + 3,
                    count + 1,
                    count + 2,
                    count + 3
                );
            }
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void){\n");
    for t in VectorType::ALL_128.into_iter().chain(VectorType::ALL_256) {
        let bytes = t.bytes();
        for n in 0..4 {
            if t.bytes() == 32 && n == 3 {
                continue;
            }
            let name = format!("{t}_{n}");
            c += &format!("typedef uint8_t V_{name} __attribute__((vector_size({bytes})));\n");
            let def = match n {
                0 => format!("V_{name} v;"),
                1 => format!("struct{{V_{name} v;}}n;"),
                2 => format!("V_{name} v[1];"),
                _ => format!("uint8_t zero[0];struct{{V_{name} v[1];}}n;"),
            };
            c += &format!(
                "typedef struct{{{def}}} W_{name};\nextern W_{name} id_{name}(W_{name});extern void caller_{name}(const void*,void*);\nW_{name} c_id_{name}(W_{name} a){{return a;}}uint64_t hash_{name}(const void*p){{const unsigned char*b=p;uint64_t r=17;for(unsigned i=0;i<{bytes};i++)r=r*131+b[i];return r+((uintptr_t)p%{bytes});}}\n"
            );
            c += &format!(
                "extern W_{name} tail_{name}(W_{name});extern W_{name} tail_ind_{name}(W_{name},W_{name}(*)(W_{name}));\n"
            );
            let types = std::iter::repeat_n(format!("W_{name}"), 10)
                .collect::<Vec<_>>()
                .join(",");
            let params = (0..10)
                .map(|i| format!("W_{name} a{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let terms = (0..10)
                .map(|i| format!("hash_{name}(&a{i})"))
                .collect::<Vec<_>>()
                .join("+");
            c += &format!(
                "extern uint64_t many_{name}({types});extern uint64_t many_call_{name}(const void*);uint64_t c_many_{name}({params}){{return {terms};}}\n"
            );
            main += &format!(
                "{{W_{name} a[10],r;for(unsigned k=0;k<256;k++){{uint64_t e=0;for(unsigned j=0;j<10;j++){{unsigned char*p=(unsigned char*)&a[j];for(unsigned i=0;i<{bytes};i++)p[i]=k+31*i+7*j;e+=hash_{name}(&a[j]);}}r=tail_{name}(a[0]);if(memcmp(&r,&a[0],{bytes}))return 14;r=tail_ind_{name}(a[0],c_id_{name});if(memcmp(&r,&a[0],{bytes}))return 15;r=id_{name}(a[0]);if(memcmp(&r,&a[0],{bytes}))return 10;caller_{name}(&a[0],&r);if(memcmp(&r,&a[0],{bytes}))return 11;if(many_{name}({})!=e)return 12;if(many_call_{name}(&a[0])!=10*hash_{name}(&a[0]))return 13;}}}}\n",
                (0..10)
                    .map(|i| format!("a[{i}]"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            for count in [7, 8] {
                let fp = std::iter::repeat_n("double", count)
                    .collect::<Vec<_>>()
                    .join(",");
                let named = (0..count)
                    .map(|i| format!("double a{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sum = (0..count)
                    .map(|i| format!("(uint64_t)a{i}"))
                    .collect::<Vec<_>>()
                    .join("+");
                let vals = (1..=count)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                c += &format!(
                    "extern uint64_t mix_{name}_{count}({fp},W_{name},double,uint64_t);uint64_t checkmix_{name}_{count}(const void*p,{named},double last,uint64_t tag){{return hash_{name}(p)+{sum}+(uint64_t)last+tag;}}\n"
                );
                main += &format!(
                    "{{W_{name} a;for(unsigned i=0;i<{bytes};i++)((unsigned char*)&a)[i]=i*7;uint64_t e=hash_{name}(&a)+{}+99+123;if(mix_{name}_{count}({vals},a,99,123)!=e)return 20;}}\n",
                    count * (count + 1) / 2
                );
            }
        }
    }
    c + &main + "return 0;}\n"
}
fn object(s: SelectionTier, a: AllocationTier, empty: bool, wide: bool) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(&source(wide)).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let features = if wide {
        Features::for_level(CpuLevel::V3)
    } else {
        Features::NONE
    };
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
fn vector_wrapper_sseup_emission_exports_and_baseline() {
    object(SelectionTier::Full, AllocationTier::Full, true, false);
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("wrappers.ir"), source(true)).unwrap();
        fs::write(d.join("wrappers.c"), harness()).unwrap();
    }
    let d = common::temp("wrapper-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("wrappers.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-mavx2",
                "-fsyntax-only",
            ])
            .arg(d.join("wrappers.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, true);
                if let Some(d) = &export {
                    let name = format!("wrappers-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_vector_wrapper_sseup_interop() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("wrapper-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("wrappers.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, true);
                fs::write(d.join("wrappers.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx2"])
                        .arg(d.join("wrappers.c"))
                        .arg(d.join("wrappers.o"))
                        .arg("-o")
                        .arg(d.join("wrappers")),
                );
                common::command(&mut Command::new(d.join("wrappers")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn ambiguous_gnu_zero_member_wide_wrappers_fail_closed() {
    for t in VectorType::ALL_256 {
        for layout in [
            layouts(t)[3].clone(),
            format!("{{{{{t}}},[0 x i8]}}"),
            format!("{{{{[0 x i8],{t}}}}}"),
        ] {
            for purpose in ["byval", "sret"] {
                let source = format!("decl @ambiguous(ptr {purpose} {layout})");
                let module = keel_ir::parse(&source).unwrap();
                keel_ir::verify_module(&module).unwrap();
                let selector = FeatureSelector {
                    inner: &SelectionTier::Baseline,
                    features: Features::for_level(CpuLevel::V3),
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                };
                let e = keel_codegen::x86_64::compile_with_pipeline(&module, &EMPTY, &pipeline)
                    .unwrap_err();
                assert!(e.message.contains("compiler-dependent SysV ABI"), "{e}");
                let e =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                        .unwrap_err();
                assert!(e.message.contains("compiler-dependent SysV ABI"), "{e}");
            }
        }
    }
}
