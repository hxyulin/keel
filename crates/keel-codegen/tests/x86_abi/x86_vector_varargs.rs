//! SysV Q varargs: sixteen-byte register-save slots and aligned overflow slots.
use crate::common;
use keel_asm_ir::x86::Features;
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
fn source() -> String {
    let mut s = String::new();
    for t in VectorType::ALL_128 {
        for kind in [0, 6, 8] {
            let name = format!("{t}_{kind}");
            let mask = t.mask();
            let fp = if kind == 8 {
                "f64,f64,f64,f64,f64,f64,f64,f64,"
            } else {
                ""
            };
            let params = if kind == 8 {
                (0..8)
                    .map(|i| format!("v{}:f64,", 100 + i))
                    .collect::<String>()
            } else {
                String::new()
            };
            s += &format!(
                "decl @c_var_{name}({fp}i64,...)->{t}\nfunc @var_{name}({fp}i64,...)->{t} {{\n ss0 = stack_slot 24, align 8\nblock0({params}v0:i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n"
            );
            if kind == 6 {
                for i in 0..6 {
                    s += &format!(" v{} = va_arg.i64 v1\n", 200 + i);
                }
            }
            let mut value = 0;
            for i in 0..10 {
                s += &format!(
                    " v{} = va_arg.{t} v1\n v{} = bitcast.{mask} v{}\n",
                    2 + i,
                    20 + i,
                    2 + i
                );
                if i == 0 {
                    value = 20;
                } else {
                    s += &format!(" v{} = vxor v{value},v{}\n", 40 + i, 20 + i);
                    value = 40 + i;
                }
            }
            s += &format!(
                " v60 = bitcast.{t} v{value}\n return v60\n}}\nfunc @call_var_{name}(ptr,ptr){{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{t} align 1 v0\n v3 = iconst.i64 10\n"
            );
            let mut args = Vec::new();
            if kind == 8 {
                for i in 0..8 {
                    s += &format!(" v{} = fconst.f64 {}.0\n", 100 + i, i + 1);
                    args.push(format!("v{}", 100 + i));
                }
            }
            args.push("v3".into());
            if kind == 6 {
                args.extend(std::iter::repeat_n("v3".to_string(), 6));
            }
            args.extend(std::iter::repeat_n("v2".to_string(), 10));
            s += &format!(
                " v4 = call @c_var_{name}({})\n store align 1 v4,v1\n return\n}}\n",
                args.join(",")
            );
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdarg.h>\nextern int memcmp(const void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void){\n");
    for t in VectorType::ALL_128 {
        c += &format!("typedef unsigned char V_{t} __attribute__((vector_size(16)));\n");
        for kind in [0, 6, 8] {
            let name = format!("{t}_{kind}");
            let fp = if kind == 8 {
                "double a0,double a1,double a2,double a3,double a4,double a5,double a6,double a7,"
            } else {
                ""
            };
            c += &format!(
                "extern V_{t} var_{name}({fp}uint64_t,...);extern void call_var_{name}(const void*,void*);V_{t} c_var_{name}({fp}uint64_t n,...){{va_list ap;va_start(ap,n);"
            );
            if kind == 6 {
                c += "for(unsigned i=0;i<6;i++)(void)va_arg(ap,uint64_t);";
            }
            c += &format!(
                "V_{t} r={{0}};for(unsigned i=0;i<10;i++)r^=va_arg(ap,V_{t});va_end(ap);return r;}}\n"
            );
            let mut args = Vec::new();
            if kind == 8 {
                args.extend((1..=8).map(|i| format!("{i}.0")));
            }
            args.push("10ULL".into());
            if kind == 6 {
                args.extend(std::iter::repeat_n("9ULL".into(), 6));
            }
            args.extend((0..10).map(|i| format!("a[{i}]")));
            main += &format!(
                "{{V_{t} a[10],r,e;for(unsigned k=0;k<256;k++){{e=(V_{t}){{0}};for(unsigned j=0;j<10;j++){{for(unsigned i=0;i<16;i++)a[j][i]=k+31*i+7*j;e^=a[j];}}r=var_{name}({});if(memcmp(&r,&e,16))return 10;call_var_{name}(a,&r);e=(V_{t}){{0}};if(memcmp(&r,&e,16))return 11;}}}}\n",
                args.join(",")
            );
        }
    }
    c + &main + "return 0;}\n"
}
fn object(s: SelectionTier, a: AllocationTier, empty: bool) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features: Features::NONE,
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
    let b = keel_asm::x86::assemble_program_object_with_features(&p, Features::NONE).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, Features::NONE).unwrap()
    );
    (b, text)
}
#[test]
fn q_varargs_register_save_and_overflow_export() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("varargs128.ir"), source()).unwrap();
        fs::write(d.join("varargs128.c"), harness()).unwrap();
    }
    let d = common::temp("q-vararg-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("varargs.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(d.join("varargs.c")),
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
                if let Some(d) = &export {
                    let name = format!("varargs128-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_q_varargs_both_directions_and_stack_alignment() {
    let d = common::temp("q-vararg-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("varargs.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty);
                fs::write(d.join("varargs.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-msse2"])
                        .arg(d.join("varargs.c"))
                        .arg(d.join("varargs.o"))
                        .arg("-o")
                        .arg(d.join("varargs")),
                );
                common::command(&mut Command::new(d.join("varargs")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
