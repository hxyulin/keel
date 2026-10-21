//! Bounded integer compare/select covers publish all leaves before allocation.
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier};
use keel_ir::EntityRef;

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[9],
    ..EMPTY
};
const CCS: [&str; 10] = [
    "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
];
fn source(ty: &str, result: &str, cc: &str, gap: &str) -> String {
    format!(
        "func @f({ty},{ty},{result},{result},ptr)->{result} cc(c) {{
block0(v0:{ty},v1:{ty},v2:{result},v3:{result},v4:ptr):
 v5 = icmp.{cc} v0,v1
 {gap}
 v6 = select v5,v2,v3
 store v0,v4
 return v6
}}"
    )
}
fn compile_module(
    m: &keel_ir::Module,
    sel: &dyn InstructionSelector,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>, Vec<u8>) {
    keel_ir::verify_module(m).unwrap();
    let pipeline = CodegenPipeline {
        selector: sel,
        allocator: &alloc,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let prog = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let typed = assemble_program_object(&prog, TargetFeatures::default()).unwrap();
    let parsed = assemble_object(&text).unwrap();
    assert_eq!(typed, parsed, "{text}");
    (text, typed, parsed)
}
fn compile(
    src: &str,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>, Vec<u8>) {
    compile_module(&keel_ir::parse(src).unwrap(), &sel, alloc, pools)
}
fn fused(text: &str) {
    assert!(text.contains("\tcsel\t"), "{text}");
    assert!(!text.contains("\tcset\t"), "{text}");
    assert!(!text.contains("\ttst\t"), "{text}");
}
#[test]
fn all_predicates_widths_results_and_spills_use_original_flags() {
    for ty in ["i32", "i64"] {
        for result in ["i32", "i64", "ptr"] {
            for cc in CCS {
                for sel in [SelectionTier::Baseline, SelectionTier::Full] {
                    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                        for pools in [&EMPTY, &ONE, &keel_codegen::aarch64::REGISTERS] {
                            fused(&compile(&source(ty, result, cc, ""), sel, alloc, pools).0);
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn balance_window_constants_and_aliases_publish_original_operands() {
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &ONE, &keel_codegen::aarch64::REGISTERS] {
                for ty in ["i32", "i64"] {
                    // Bound, compare, 17, subtract, zero, select: the production
                    // balance predicate needs a three-definition gap.
                    let src = format!(
                        "func @f({ty},ptr)->{ty} {{
block0(v0:{ty},v1:ptr):
 v2 = iconst.{ty} 4096
 v3 = icmp.ult v0,v2
 v4 = iconst.{ty} 17
 v5 = isub v0,v4
 v6 = iconst.{ty} 0
 v7 = select v3,v5,v6
 store v0,v1
 return v7
}}"
                    );
                    fused(&compile(&src, sel, alloc, pools).0);
                    // Same folded immediate is also a selected leaf: retain
                    // its allocation occurrence when removing the CMP fold.
                    let src = src.replace("select v3,v5,v6", "select v3,v2,v5");
                    fused(&compile(&src, sel, alloc, pools).0);
                    let src = source(ty, ty, "sge", &format!("v8 = iconst.{ty} 99"));
                    let mut m = keel_ir::parse(&src).unwrap();
                    let f = &mut m.functions[0];
                    let values: Vec<_> = f
                        .layout_insts()
                        .filter_map(|(_, i)| f.inst_results(i).first().copied())
                        .collect();
                    f.replace_all_uses(values[1], keel_ir::Value::new(0));
                    fused(&compile_module(&m, &sel, alloc, pools).0);
                }
                // Null and a large stack address rematerialize after CMP;
                // ADD and MOVZ/MOVK must preserve flags and the other scratch.
                let src = "func @f(i64,i64)->ptr {
ss0 = stack_slot 8192, align 16
block0(v0:i64,v1:i64):
 v2 = stack_addr ss0
 v8 = iconst.i64 0
 v3 = inttoptr.ptr v8
 v4 = icmp.slt v0,v1
 v5 = select v4,v2,v3
 return v5
}";
                fused(&compile(src, sel, alloc, pools).0);
            }
        }
    }
}
#[test]
fn barriers_shared_conditions_narrow_types_and_window_bound_stay_materialized() {
    let base = source("i64", "i64", "slt", "GAP");
    let cross_block = base
        .replace("GAP", "")
        .replace("v6 = select", "jump block1\nblock1:\n v6 = select");
    assert!(
        compile(
            &cross_block,
            SelectionTier::Full,
            AllocationTier::Full,
            &EMPTY
        )
        .0
        .contains("\tcset\t")
    );
    for gap in [
        "store v0,v4",
        "v8 = load.i64 v4",
        "v8 = sdiv v0,v1",
        "v8 = call @poll()",
        "v8 = iconst.i64 1\n v9 = iconst.i64 2\n v10 = iconst.i64 3\n v11 = iconst.i64 4\n v12 = iconst.i64 5",
        "v8 = uext.i64 v5",
    ] {
        let mut src = base.replace("GAP", gap);
        if gap.contains("v12 =") {
            src = src.replace("store v0,v4", "store v0,v4\n store v8,v4\n store v9,v4\n store v10,v4\n store v11,v4\n store v12,v4");
        }
        let src = format!("decl @poll()->i64\n{src}");
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            let text = compile(&src, sel, AllocationTier::Full, &EMPTY).0;
            assert!(text.contains("\tcset\t"), "{gap}: {text}");
        }
    }
    for src in [
        source("i8", "i64", "slt", ""),
        source("i16", "i64", "slt", ""),
        source("i64", "i16", "slt", "").replace("->i16 cc(c)", "->i16 uext cc(c)"),
    ] {
        assert!(
            compile(&src, SelectionTier::Full, AllocationTier::Full, &EMPTY)
                .0
                .contains("\tcset\t")
        );
    }
    let gap = "v8 = iconst.i64 1\n v9 = iconst.i64 2\n v10 = iconst.i64 3\n v11 = iconst.i64 4";
    let src = base.replace("GAP", gap).replace(
        "store v0,v4",
        "store v0,v4\n store v8,v4\n store v9,v4\n store v10,v4\n store v11,v4",
    );
    fused(&compile(&src, SelectionTier::Full, AllocationTier::Full, &EMPTY).0);
}
#[test]
fn constraints_matches_covered_patterns_and_debug_lines_preserve_boundaries() {
    use keel_codegen::{
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Boundary(usize, bool);
    impl InstructionSelector for Boundary {
        fn name(&self) -> &'static str {
            "compare-select-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut plan = SelectionTier::Baseline.select(f, t, folds);
            let i = f.layout_insts().nth(self.0).unwrap().1;
            if self.1 {
                plan.cover(i);
            } else {
                let value = if self.0 == 1 {
                    f.first_result(i)
                } else {
                    let mut v = Vec::new();
                    f.inst_data(i).for_each_operand(|a| v.push(a));
                    v[0]
                };
                plan.set_constraint(
                    i,
                    ValuePart::low(value),
                    if self.0 == 1 {
                        OperandKind::Def
                    } else {
                        OperandKind::Use
                    },
                    Constraint::Register,
                );
            }
            plan
        }
    }
    let src = source("i64", "i64", "slt", "v8 = iconst.i64 17");
    for at in 0..3 {
        let m = keel_ir::parse(&src).unwrap();
        // Explicit operand constraints preserve each original boundary.
        let selector = Boundary(at, false);
        assert!(
            compile_module(
                &m,
                &selector,
                AllocationTier::Full,
                &keel_codegen::aarch64::REGISTERS
            )
            .0
            .contains("\tcset\t")
        );
        if at == 1 {
            fused(&compile_module(&m, &Boundary(at, true), AllocationTier::Full, &EMPTY).0);
        }
        let mut m = keel_ir::parse(&src).unwrap();
        m.files.push("select.c".into());
        let f = &mut m.functions[0];
        let insts: Vec<_> = f.layout_insts().take(3).map(|(_, i)| i).collect();
        for (n, i) in insts.into_iter().enumerate() {
            f.set_inst_loc(
                i,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: if n == at { 11 } else { 10 },
                    col: 1,
                    scope: None,
                }),
            );
        }
        assert!(
            compile_module(&m, &SelectionTier::Full, AllocationTier::Full, &EMPTY)
                .0
                .contains("\tcset\t")
        );
    }
    // Outer Boolean-test condition patterns remain on their existing path.
    let src = src.replace(
        "v6 = select v5",
        "v9 = uext.i64 v5\n v10 = iconst.i64 0\n v11 = icmp.eq v9,v10\n v6 = select v11",
    );
    assert!(
        compile(&src, SelectionTier::Full, AllocationTier::Full, &EMPTY)
            .0
            .contains("\tcset\t")
    );
}
#[test]
fn inline_assembly_in_successor_and_abs_priority_keep_old_contracts() {
    for clobber in ["", ",~{cc}"] {
        let src = source("i64", "i64", "slt", "").replace("return v6", &format!("jump block1\nblock1:\n v8 = asm \"cset %w0, eq\", \"=r{clobber}\"() -> i32\n return v6"));
        let text = compile(&src, SelectionTier::Full, AllocationTier::Full, &EMPTY).0;
        assert!(text.contains("\tcset\t"), "{text}");
        assert!(text.contains("\ttst\t"), "{text}");
    }
    let src = "func @f(i64)->i64 {\nblock0(v0:i64):\n v1 = iconst.i64 0\n v2 = icmp.slt v0,v1\n v3 = ineg v0\n v4 = select v2,v3,v0\n return v4\n}";
    assert!(
        compile(src, SelectionTier::Full, AllocationTier::Full, &EMPTY)
            .0
            .contains("\tcsneg\t")
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_predicates_extrema_aliases_live_values_barriers_and_both_object_routes() {
    use keel_ir::{InstData, Type};
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("arm-compare-select-native");
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::from("decl @clobber()\n");
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdio.h>\nvoid clobber(void) { __asm__ volatile(\"cmp xzr, xzr\" ::: \"cc\"); }\n",
    );
    let mut checks = String::new();
    let expressions = [
        "a==b", "a!=b", "sa<sb", "sa<=sb", "sa>sb", "sa>=sb", "a<b", "a<=b", "a>b", "a>=b",
    ];
    for bits in [32, 64] {
        let ty = format!("i{bits}");
        for (cc, expression) in CCS.into_iter().zip(expressions) {
            for variant in 0..12 {
                let name = format!("f{bits}_{cc}_{variant}");
                let mut src = source(&ty, &ty, cc, "").replace("@f(", &format!("@{name}("));
                let mut expression = expression.to_string();
                let mut yes = "x".to_string();
                let mut no = "y".to_string();
                let mut extra = String::new();
                match variant {
                    1 => {
                        // Production balance shape, with a folded shifted bound.
                        src = src.replace(&format!("v5 = icmp.{cc} v0,v1"), &format!("v8 = iconst.{ty} 4096\n v5 = icmp.{cc} v0,v8\n v9 = iconst.{ty} 17\n v10 = isub v0,v9\n v11 = iconst.{ty} 0"));
                        src = src.replace("select v5,v2,v3", "select v5,v10,v11");
                        expression = expression.replace("sb", "4096").replace('b', "4096");
                        yes = format!("(uint{bits}_t)(a-17)");
                        no = "0".into();
                    }
                    2 => {
                        // Neither the CMP constant nor wide arms are small immediates.
                        src = src.replace(&format!("v5 = icmp.{cc} v0,v1"), &format!("v8 = iconst.{ty} -1234567\n v9 = iconst.{ty} 305419896\n v5 = icmp.{cc} v0,v8"));
                        src = src.replace("select v5,v2,v3", "select v5,v8,v9");
                        expression = expression
                            .replace("sb", &format!("((int{bits}_t)-1234567)"))
                            .replace("b", &format!("((uint{bits}_t)-1234567)"));
                        yes = format!("(uint{bits}_t)-1234567");
                        no = "305419896".into();
                    }
                    3 => {
                        src = src.replace(
                            "return v6",
                            &format!("v8 = uext.{ty} v5\n v9 = xor v6,v8\n return v9"),
                        );
                        extra = format!("e ^= !!({expression});");
                    }
                    4 => {
                        src =
                            src.replace("v6 = select", &format!("v8 = load.{ty} v4\n v6 = select"));
                    }
                    5 => {
                        src = src.replace("v6 = select", "store v0,v4\n v6 = select");
                    }
                    7 => {
                        // An alias of the original compare input is a selected arm.
                        yes = "a".into();
                    }
                    8 => {
                        // Both arms refer to the same live comparison input.
                        src = src.replace("select v5,v2,v3", "select v5,v0,v0");
                        yes = "a".into();
                        no = "a".into();
                    }
                    9 | 10 => {
                        // NZCV is observed after the select.
                        let clobber = if variant == 10 { ",~{cc}" } else { "" };
                        src = src.replace("return v6", &format!("v8 = asm \"cset %w0, eq\", \"=r{clobber}\"() -> i32\n v9 = uext.{ty} v8\n v10 = xor v6,v9\n return v10"));
                        if bits == 32 {
                            src = src
                                .replace("v9 = uext.i32 v8\n v10 = xor v6,v9", "v10 = xor v6,v8");
                        }
                        extra = format!("e ^= !({expression});");
                    }
                    11 => {
                        src = src.replace("v6 = select", "call @clobber()\n v6 = select");
                    }
                    _ => {}
                }
                ir += &src;
                c += &format!(
                    "extern uint{bits}_t {name}(uint{bits}_t,uint{bits}_t,uint{bits}_t,uint{bits}_t,void*);\n"
                );
                checks += &format!(
                    "{{ uint{bits}_t a=(uint{bits}_t)vals[i],b=(uint{bits}_t)vals[j],x=(uint{bits}_t)(vals[j]^0x123456789abcdef0ULL),y=(uint{bits}_t)(vals[i]^0xfedcba9876543210ULL),live=0;int{bits}_t sa=(int{bits}_t)a,sb=(int{bits}_t)b;uint{bits}_t e=({expression})?({yes}):({no});{extra}uint{bits}_t got={name}(a,b,x,y,&live);if(got!=e||live!=a){{puts(\"{name}\");return 1;}}}}\n"
                );
            }
        }
    }
    // Pointer arms use identical GP staging, with no dereference of test values.
    for cc in CCS {
        let name = format!("ptr_{cc}");
        ir += &source("i64", "ptr", cc, "").replace("@f(", &format!("@{name}("));
        c += &format!("extern void* {name}(uint64_t,uint64_t,void*,void*,void*);\n");
        let expression = expressions[CCS.iter().position(|&x| x == cc).unwrap()];
        checks += &format!(
            "{{uint64_t a=vals[i],b=vals[j],live=0;int64_t sa=(int64_t)a,sb=(int64_t)b;void*x=(void*)(uintptr_t)vals[i],*y=(void*)(uintptr_t)vals[j];if({name}(a,b,x,y,&live)!=(({expression})?x:y)||live!=a)return 2;}}\n"
        );
    }
    c += &format!(
        "int main(void){{const uint64_t vals[]={{0,1,2,16,17,4095,4096,4097,0x7fffffffULL,0x80000000ULL,0xffffffffULL,0x7fffffffffffffffULL,0x8000000000000000ULL,0xffffffffffffffffULL,0xabcdef0123456789ULL}};for(unsigned i=0;i<sizeof(vals)/sizeof(vals[0]);i++)for(unsigned j=0;j<sizeof(vals)/sizeof(vals[0]);j++){{{checks}}}return 0;}}"
    );
    let mut m = keel_ir::parse(&ir).unwrap();
    m.files.push("compare-select.c".into());
    for f in &mut m.functions {
        let name = &f.name;
        if name.ends_with("_6") {
            let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
            for (n, i) in insts.into_iter().enumerate() {
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line: 10 + n as u32,
                        col: 1,
                        scope: None,
                    }),
                );
            }
        } else if name.ends_with("_7") {
            let ty = f.value_type(keel_ir::Value::new(0));
            assert!(matches!(ty, Type::I32 | Type::I64));
            f.replace_all_uses(keel_ir::Value::new(2), keel_ir::Value::new(0));
        }
    }
    fs::write(dir.join("oracle.c"), c).unwrap();
    fs::write(dir.join("input.keel"), m.to_string()).unwrap();
    let out = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let mut executions = 0;
    for sel in [SelectionTier::Baseline, SelectionTier::Full] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&EMPTY, &ONE, &keel_codegen::aarch64::REGISTERS] {
                let (text, typed, parsed) = compile_module(&m, &sel, alloc, pools);
                fs::write(dir.join("generated.s"), text).unwrap();
                for object in [typed, parsed] {
                    fs::write(dir.join("test.o"), object).unwrap();
                    let out = Command::new("clang")
                        .arg(dir.join("oracle.o"))
                        .arg(dir.join("test.o"))
                        .arg("-o")
                        .arg(dir.join("test"))
                        .output()
                        .unwrap();
                    assert!(out.status.success(), "{out:?}");
                    let out = Command::new(dir.join("test")).output().unwrap();
                    assert!(out.status.success(), "{sel:?}/{alloc:?}: {out:?}");
                    executions += 1;
                }
            }
        }
    }
    // Independently execute explicit allocation constraints with allocatable
    // registers, preserving both original compare and select boundaries.
    struct Constrained;
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "native-compare-select-constraints"
        }
        fn select(
            &self,
            f: &keel_ir::Function,
            t: keel_codegen::select::Target,
            folds: &dyn Fn(&InstData) -> Vec<keel_ir::Value>,
        ) -> keel_codegen::select::Selection {
            use keel_codegen::regalloc::{Constraint, OperandKind, ValuePart};
            let mut p = SelectionTier::Baseline.select(f, t, folds);
            for (_, i) in f.layout_insts() {
                if let InstData::Icmp { args, .. } = *f.inst_data(i) {
                    p.set_constraint(
                        i,
                        ValuePart::low(f.resolve(args[0])),
                        OperandKind::Use,
                        Constraint::Register,
                    );
                }
            }
            p
        }
    }
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        let (_, typed, parsed) =
            compile_module(&m, &Constrained, alloc, &keel_codegen::aarch64::REGISTERS);
        for object in [typed, parsed] {
            fs::write(dir.join("test.o"), object).unwrap();
            let out = Command::new("clang")
                .arg(dir.join("oracle.o"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
            let out = Command::new(dir.join("test")).output().unwrap();
            assert!(out.status.success(), "constraints/{alloc:?}: {out:?}");
            executions += 1;
        }
    }
    fs::write(dir.join("evidence.json"),format!("{{\"functions\":250,\"cases_per_function\":225,\"executions\":{executions},\"status\":\"passed\",\"runtime_claim\":false}}\n")).unwrap();
}
