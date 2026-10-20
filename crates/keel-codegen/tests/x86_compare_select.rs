//! Compare/consumer fusion must preserve NaNs, operand lifetimes and spills.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[7],
    ..SPILLED
};
fn compile(
    src: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let p = CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &p).unwrap();
    let prog = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &p).unwrap();
    let object = keel_asm::x86::assemble_program_object(&prog).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object(&text).unwrap(),
        "{text}"
    );
    (text, object)
}
fn select(name: &str, ty: &str, cmp: &str, shared: bool) -> String {
    format!(
        "func @{name}({ty}, {ty}, i64, i64) -> i64 cc(c) {{
block0(v0: {ty}, v1: {ty}, v2: i64, v3: i64):
 v4 = {cmp} v0, v1
 v5 = select v4, v2, v3
 {}
}}",
        if shared {
            "v6 = uext.i64 v4\n v7 = xor v5, v6\n return v7"
        } else {
            "return v5"
        }
    )
}
#[test]
fn adjacent_single_use_compare_selects_keep_flags_without_boolean() {
    for ty in ["i8", "i16", "i32", "i64", "f32", "f64"] {
        let cmp = if ty.starts_with('f') {
            "fcmp.ne"
        } else {
            "icmp.slt"
        };
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &ONE, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) = compile(&select("f", ty, cmp, false), sel, alloc, pools);
                    assert!(text.contains("\tcmov"), "{text}");
                    if ty.starts_with('i') {
                        assert!(!text.contains("\tset"), "{text}");
                    }
                    assert!(!text.contains("\ttest\t"), "{text}");
                }
            }
        }
    }
}
#[test]
fn shared_and_nonadjacent_conditions_remain_materialized() {
    for shared in [false, true] {
        let src = select("f", "i64", "icmp.slt", shared);
        let src = if shared {
            src
        } else {
            src.replace(" v5 = select", " v9 = iconst.i64 3\n v5 = select")
        };
        let (text, _) = compile(&src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
        assert!(text.contains("\tsetl\t"), "{text}");
    }
}
#[test]
fn self_fp_comparisons_use_parity_and_general_eq_ne_avoid_renormalization() {
    for ty in ["f32", "f64"] {
        for cc in ["eq", "ne"] {
            for same in [false, true] {
                let src = format!(
                    "func @f({ty}, {ty}) -> i64 cc(c) {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = fcmp.{cc} v0, {}\n v3 = uext.i64 v2\n return v3\n}}",
                    if same { "v0" } else { "v1" }
                );
                let (text, _) = compile(&src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
                if same {
                    let expected = if cc == "eq" { "setnp" } else { "setp" };
                    assert!(text.contains(&format!("\t{expected}\t")), "{text}");
                    assert!(!text.contains("\ttest\t"), "{text}");
                } else {
                    assert_eq!(
                        text.matches("\tsetne\t").count(),
                        usize::from(cc == "ne"),
                        "{text}"
                    );
                }
            }
        }
    }
}
#[test]
fn memory_comparison_does_not_overwrite_staged_select_arm() {
    let src = "func @f(ptr, i64, i64, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64, v2: i64, v3: i64):
 v4 = load.i64 v0
 v5 = icmp.slt v1, v4
 v6 = select v5, v2, v3
 return v6
}";
    let (text, _) = compile(src, SelectionTier::Full, AllocationTier::Full, &SPILLED);
    assert!(text.contains("\tsetl\t"), "{text}");
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_comparisons_match_c_with_nan_signed_zero_spills_and_shared_conditions() {
    use std::{fs, process::Command};
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <math.h>\n#include <stdio.h>\n");
    let mut checks = String::new();
    let mut n = 0;
    for ty in ["i8", "i16", "i32", "i64", "f32", "f64"] {
        let width = ty[1..].parse::<u32>().unwrap();
        let float = ty.starts_with('f');
        let comparisons: &[(&str, &str)] = if float {
            &[
                ("eq", "a==b"),
                ("ne", "a!=b"),
                ("lt", "a<b"),
                ("le", "a<=b"),
                ("gt", "a>b"),
                ("ge", "a>=b"),
                ("ord", "!isunordered(a,b)"),
                ("uno", "isunordered(a,b)"),
            ]
        } else {
            &[
                ("eq", "a==b"),
                ("ne", "a!=b"),
                ("slt", "a<b"),
                ("sle", "a<=b"),
                ("sgt", "a>b"),
                ("sge", "a>=b"),
                ("ult", "ua<ub"),
                ("ule", "ua<=ub"),
                ("ugt", "ua>ub"),
                ("uge", "ua>=ub"),
            ]
        };
        let ct = if float {
            if width == 32 {
                "float".into()
            } else {
                "double".into()
            }
        } else {
            format!("int{width}_t")
        };
        for &(cc, expression) in comparisons {
            for shared in [false, true] {
                let name = format!("f{n}");
                n += 1;
                ir += &select(
                    &name,
                    ty,
                    &format!("{}.{cc}", if float { "fcmp" } else { "icmp" }),
                    shared,
                );
                c += &format!("extern uint64_t {name}({ct}, {ct}, uint64_t, uint64_t);\n");
                let setup = if float {
                    format!(
                        "const {ct} vals[]={{NAN,INFINITY,-INFINITY,0.0,-0.0,1.0,-1.0,1e-20,-1e20}}; for(unsigned i=0;i<9;i++) for(unsigned j=0;j<9;j++) {{ {ct} a=vals[i],b=vals[j];"
                    )
                } else {
                    format!(
                        "for(unsigned i=0;i<40;i++) for(unsigned j=0;j<40;j++) {{ {ct} a=({ct})(0x9e3779b97f4a7c15ULL*i),b=({ct})(0xa0761d6478bd642fULL*j); uint{width}_t ua=(uint{width}_t)a,ub=(uint{width}_t)b;"
                    )
                };
                checks += &format!(
                    "{{ {setup} uint64_t x=0x123456789abcdef0ULL+i,y=0xfedcba9876543210ULL-j; uint64_t e=({expression})?x:y; {} if({name}(a,b,x,y)!=e) {{ puts(\"{name}\"); return 1; }} }} }}\n",
                    if shared {
                        format!("e ^= !!({expression});")
                    } else {
                        String::new()
                    }
                );
            }
        }
    }
    for ty in ["f32", "f64"] {
        let ct = if ty == "f32" { "float" } else { "double" };
        for cc in ["eq", "ne"] {
            let name = format!("self_{ty}_{cc}");
            ir += &format!(
                "func @{name}({ty}) -> i64 cc(c) {{\nblock0(v0: {ty}):\n v1 = fcmp.{cc} v0, v0\n v2 = uext.i64 v1\n return v2\n}}\n"
            );
            c += &format!("extern uint64_t {name}({ct});\n");
            checks += &format!(
                "{{ {ct} vals[]={{NAN,INFINITY,-INFINITY,0.0,-0.0,1,-1}}; for(unsigned i=0;i<7;i++) if({name}(vals[i]) != (uint64_t)({}isnan(vals[i]))) return 2; }}\n",
                if cc == "eq" { "!" } else { "!!" }
            );
        }
    }
    c += &format!("int main(void) {{ {checks} return 0; }}");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("compare-select-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    let out = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&SPILLED, &ONE, &keel_codegen::x86_64::REGISTERS] {
                let (_, obj) = compile(&ir, sel, alloc, pools);
                fs::write(dir.join("test.o"), obj).unwrap();
                let out = Command::new("clang")
                    .arg(dir.join("oracle.o"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{out:?}");
                let out = Command::new(dir.join("test")).output().unwrap();
                assert!(out.status.success(), "{sel:?} {alloc:?}: {out:?}");
            }
        }
    }
}
