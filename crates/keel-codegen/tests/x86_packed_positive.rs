//! Exact positive32 cover and portable Linux oracle export.
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::EntityRef;
use std::{fs, path::PathBuf};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
const SOURCE: &str = "func @positive(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i32 0\n v3 = vsplat.i32x4 v2\n v4 = load.i64 align 1 v0\n v5 = vsplat.i64x2 v4\n v6 = bitcast.i32x4 v5\n v7 = vicmp.sgt v6,v3\n v8 = vand v7,v6\n v9 = uwiden_low.i64x2 v8\n store align 1 v9,v1\n return\n}\n";
fn compile(
    src: &str,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    compile_module(&m, &sel, alloc, pools)
}
fn compile_module(
    m: &keel_ir::Module,
    sel: &dyn keel_codegen::InstructionSelector,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector: sel,
        allocator: &alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = assemble_program_object(&program).unwrap();
    assert_eq!(assemble_object(&text).unwrap(), object);
    (text, object)
}

fn loads(text: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with("movq\txmm") && l.contains("qword ptr ["))
        .count()
}
#[test]
fn exact_commuted_and_rejections_all_configurations() {
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &TINY, &EMPTY] {
                for src in [
                    SOURCE.to_string(),
                    SOURCE.replace("vand v7,v6", "vand v6,v7"),
                ] {
                    let text = compile(&src, sel, alloc, pools).0;
                    assert_eq!(loads(&text), 1, "{text}");
                    assert!(!text.contains("punpcklqdq\t"), "{text}");
                    assert_eq!(text.matches("pcmpgtd\t").count(), 1);
                    assert_eq!(text.matches("punpckldq\t").count(), 1);
                }
                for src in rejections() {
                    let text = compile(&src, sel, alloc, pools).0;
                    assert_eq!(loads(&text), 0, "{src}\n{text}");
                }
            }
        }
    }
}
fn rejections() -> Vec<String> {
    let mut cases = vec![
        SOURCE.replace("load.i64 align", "load.i64 volatile align"),
        SOURCE.replace("iconst.i32 0", "iconst.i32 1"),
        SOURCE
            .replace(" v3 =", " v21 = iadd v2,v2\n v3 =")
            .replace("vsplat.i32x4 v2", "vsplat.i32x4 v21"),
        SOURCE.replace("vicmp.sgt", "vicmp.ugt"),
        SOURCE.replace("vicmp.sgt", "vicmp.sge"),
        SOURCE.replace("vicmp.sgt v6,v3", "vicmp.sgt v3,v6"),
        SOURCE.replace("vand v7,v6", "vand v7,v7"),
        SOURCE.replace("uwiden_low", "swiden_low"),
        SOURCE.replace(" v7 =", " fence seq_cst\n v7 ="),
        SOURCE.replace(" v5 =", " store v2,v1\n v5 ="),
        SOURCE.replace(" v8 =", " v20 = load.i32 v1\n v8 ="),
        SOURCE.replace(" v9 =", " v20 = iconst.i32 0\n v9 ="),
    ];
    for v in [4, 5, 6, 7, 8] {
        cases.push(SOURCE.replace(" return", &format!(" store v{v},v1\n return")));
    }
    cases
}
#[test]
fn constraints_and_source_boundaries() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{Function, InstData, Value};
    struct Constrained(usize);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "positive-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            let at = f.layout_insts().nth(2 + self.0).unwrap().1;
            s.set_constraint(
                at,
                ValuePart::low(f.first_result(at)),
                OperandKind::Def,
                Constraint::Register,
            );
            s
        }
    }
    for at in 0..6 {
        let m = keel_ir::parse(SOURCE).unwrap();
        let selector = Constrained(at);
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let text =
            keel_codegen::x86_64::compile_with_pipeline(&m, &keel_codegen::x86_64::REGISTERS, &p)
                .unwrap();
        assert_eq!(loads(&text), 0);
        let mut m = m;
        m.files.push("positive.c".into());
        let f = &mut m.functions[0];
        let inst = f.layout_insts().nth(2 + at).unwrap().1;
        f.set_inst_loc(
            inst,
            Some(keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(0),
                line: 2,
                col: 1,
                scope: None,
            }),
        );
        assert_eq!(loads(&keel_codegen::x86_64::compile(&m).unwrap()), 0);
    }
}

fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = include_str!("x86_positive/oracle.c").to_string();
    let mut calls = String::new();
    for reverse in [false, true] {
        for mode in ["direct", "offset", "indexed", "live"] {
            let name = format!("positive_{mode}_{}", u8::from(reverse));
            let mut src = SOURCE
                .replace("@positive(ptr,ptr)", &format!("@{name}(ptr,ptr,i64)"))
                .replace("v1:ptr):", "v1:ptr,v10:i64):");
            let address = match mode {
                "offset" => " v11 = iconst.i64 8\n v12 = ptradd v0,v11\n",
                "indexed" => " v11 = iconst.i64 3\n v13 = shl v10,v11\n v12 = ptradd v0,v13\n",
                _ => "",
            };
            src = src.replace(" v2 =", &format!("{address} v2 ="));
            if matches!(mode, "offset" | "indexed") {
                src = src.replace("align 1 v0", "align 1 v12");
            }
            if mode == "live" {
                src = src.replace(" v4 =", " v14 = load.i64x2 align 1 v1\n v4 =");
                src = src.replace(
                    " return",
                    " v15 = iconst.i64 16\n v16 = ptradd v1,v15\n store align 1 v14,v16\n return",
                );
            }
            if reverse {
                src = src.replace("vand v7,v6", "vand v6,v7");
            }
            ir += &src;
            c += &format!("extern void {name}(const void*,void*,uint64_t);\n");
            calls += &format!(
                " if(check({name},{},{}))return 2;\n",
                if matches!(mode, "offset" | "indexed") {
                    8
                } else {
                    0
                },
                u8::from(mode == "live")
            );
        }
    }
    // Include a live vector accumulator, with wrapping additions after widening.
    let src = SOURCE
        .replace("@positive", "@positive_sum")
        .replace(" v4 =", " v14 = load.i64x2 align 1 v1\n v4 =")
        .replace(
            " store align 1 v9,v1",
            " v15 = viadd v14,v9\n store align 1 v15,v1",
        );
    ir += &src;
    c += "extern void positive_sum(const void*,void*);\n";
    c += &format!(
        "int main(void){{ if(setup())return 1; {calls} if(check_sum())return 3; munmap(region,2*page); puts(\"positive32 guard/unaligned/alias/live/extrema/random/wrap/fault-order passed\");return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn export_linux_oracles() {
    let evidence = std::env::var_os("KEEL_POSITIVE32_EVIDENCE").map(PathBuf::from);
    let (ir, c) = fixture();
    if let Some(dir) = &evidence {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.keel"), &ir).unwrap();
        fs::write(
            dir.join("run_native.py"),
            include_str!("x86_positive/run_native.py"),
        )
        .unwrap();
    }
    if let (Some(dir), Some(input)) = (&evidence, std::env::var_os("KEEL_POSITIVE32_KERNEL_IR")) {
        let src = fs::read_to_string(input).unwrap();
        let (text, obj) = compile(
            &src,
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
        );
        fs::write(dir.join("kernel-direct.s"), text).unwrap();
        fs::write(dir.join("kernel-direct.o"), obj).unwrap();
        fs::write(
            dir.join("kernel-direct.c"),
            include_str!("x86_positive/kernel.c"),
        )
        .unwrap();
    }
    for sel in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &EMPTY),
            ] {
                for control in [false, true] {
                    let source = if control {
                        ir.replace("load.i64 align", "load.i64 volatile align")
                    } else {
                        ir.clone()
                    };
                    let (text, object) = compile(&source, sel, alloc, pools);
                    assert_eq!(
                        loads(&text),
                        if control { 0 } else { 9 },
                        "{sel:?}/{alloc:?}/{pool}/{control}"
                    );
                    if let Some(dir) = &evidence {
                        let name = format!("{sel:?}-{alloc:?}-{pool}-control{control}");
                        fs::write(dir.join(format!("{name}.s")), text).unwrap();
                        fs::write(dir.join(format!("{name}.o")), object).unwrap();
                        fs::write(dir.join(format!("{name}.c")), &c).unwrap();
                    }
                }
            }
        }
    }
}

struct ConstrainedLoads;
impl keel_codegen::InstructionSelector for ConstrainedLoads {
    fn name(&self) -> &'static str {
        "positive-load-constraints"
    }
    fn select(
        &self,
        f: &keel_ir::Function,
        t: keel_codegen::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        use keel_codegen::regalloc::{Constraint, OperandKind, ValuePart};
        let mut s = SelectionTier::Baseline.select(f, t, folds);
        for (_, i) in f.layout_insts() {
            if matches!(
                f.inst_data(i),
                keel_ir::InstData::Load {
                    ty: keel_ir::Type::I64,
                    ..
                }
            ) {
                s.set_constraint(
                    i,
                    ValuePart::low(f.first_result(i)),
                    OperandKind::Def,
                    Constraint::Any,
                );
            }
        }
        s
    }
}
#[test]
fn export_rejection_oracles() {
    let evidence = std::env::var_os("KEEL_POSITIVE32_EVIDENCE").map(PathBuf::from);
    let (ir, c) = fixture();
    for boundary in ["debug", "constraints", "external"] {
        let mut module = keel_ir::parse(&ir).unwrap();
        let mut oracle = c.clone();
        if boundary == "debug" {
            module.files.push("positive.c".into());
            for f in &mut module.functions {
                let loads: Vec<_> = f
                    .layout_insts()
                    .filter_map(|(_, i)| {
                        matches!(
                            f.inst_data(i),
                            keel_ir::InstData::Load {
                                ty: keel_ir::Type::I64,
                                ..
                            }
                        )
                        .then_some(i)
                    })
                    .collect();
                for i in loads {
                    f.set_inst_loc(
                        i,
                        Some(keel_ir::SourceLoc {
                            file: keel_ir::SourceFile::new(0),
                            line: 2,
                            col: 1,
                            scope: None,
                        }),
                    );
                }
            }
        } else if boundary == "external" {
            let (prefix, sum) = ir.split_once("func @positive_sum").unwrap();
            let prefix = prefix.replace(
                " return",
                " v17 = iconst.i64 32\n v18 = ptradd v1,v17\n store align 1 v6,v18\n return",
            );
            module = keel_ir::parse(&format!("{prefix}func @positive_sum{sum}")).unwrap();
            oracle = format!("#define EXTERNAL 1\n{oracle}");
        }
        keel_ir::verify_module(&module).unwrap();
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("empty", &EMPTY),
            ] {
                let selector: &dyn keel_codegen::InstructionSelector = if boundary == "constraints"
                {
                    &ConstrainedLoads
                } else {
                    &SelectionTier::Full
                };
                let (text, obj) = compile_module(&module, selector, alloc, pools);
                assert_eq!(loads(&text), usize::from(boundary == "external"));
                if let Some(dir) = &evidence {
                    let name = format!("rejection-{boundary}-{alloc:?}-{pool}");
                    fs::write(dir.join(format!("{name}.s")), text).unwrap();
                    fs::write(dir.join(format!("{name}.o")), obj).unwrap();
                    fs::write(dir.join(format!("{name}.c")), &oracle).unwrap();
                }
            }
        }
    }
}
