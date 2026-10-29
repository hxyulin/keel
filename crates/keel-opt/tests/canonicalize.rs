#[path = "canonicalize/rotate.rs"]
mod rotate;

mod support;
use keel_ir::{EntityRef, Function, InstData, Value};
use keel_opt::Pass;

fn canonical(source: &str) -> (keel_ir::Module, keel_ir::Module) {
    let original = keel_ir::parse(source).unwrap();
    let mut module = original.clone();
    let count = module.functions[0].layout_insts().count();
    keel_opt::run_passes(&mut module, &["canonicalize"]).unwrap();
    assert_eq!(count, module.functions[0].layout_insts().count());
    assert!(
        !keel_opt::find_pass("canonicalize")
            .unwrap()
            .run(&mut module.functions[0])
    );
    (original, module)
}

#[test]
fn comparisons_reverse_every_predicate_at_every_width() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for (cond, reverse) in [
            ("eq", "eq"),
            ("ne", "ne"),
            ("slt", "sgt"),
            ("sle", "sge"),
            ("sgt", "slt"),
            ("sge", "sle"),
            ("ult", "ugt"),
            ("ule", "uge"),
            ("ugt", "ult"),
            ("uge", "ule"),
        ] {
            for imm in [0, 37, -1] {
                let source = format!(
                    "func @f({ty}) -> i8 {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} {imm}\n v2 = icmp.{cond} v1, v0\n return v2\n}}"
                );
                let (original, module) = canonical(&source);
                assert!(
                    module
                        .to_string()
                        .contains(&format!("icmp.{reverse} v0, v1")),
                    "{module}"
                );
                if ty != "i128" {
                    for x in (0..256).chain([u64::MAX, 1 << 63]) {
                        assert_eq!(
                            support::execute(&original.functions[0], &[x]),
                            support::execute(&module.functions[0], &[x]),
                            "{source} {x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn integer_pairs_are_deterministic_including_both_overflow_results() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in [
            "iadd",
            "imul",
            "and",
            "or",
            "xor",
            "umulhi",
            "smulhi",
            "uadd_overflow",
            "sadd_overflow",
            "umul_overflow",
            "smul_overflow",
        ] {
            let results = if op.ends_with("overflow") {
                "v2, v3"
            } else {
                "v2"
            };
            let source = format!(
                "func @f({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n {results} = {op} v1, v0\n return v2\n}}"
            );
            let (_, module) = canonical(&source);
            assert!(
                module.to_string().contains(&format!("{op} v0, v1")),
                "{module}"
            );
            if op.ends_with("overflow") {
                let f = &module.functions[0];
                let inst = f.layout_insts().next().unwrap().1;
                assert_eq!(f.inst_results(inst), &[Value::new(2), Value::new(3)]);
            }
        }
    }
}

#[test]
fn normalization_preserves_float_noncommutative_and_memory_order() {
    let source = "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 37
 v3 = isub v2, v1
 store v3, v0
 v4 = load.i64 v0
 return v4
}";
    let (original, module) = canonical(source);
    assert_eq!(original.to_string(), module.to_string());
    for op in ["fadd", "fmul", "fsub", "fdiv"] {
        let source = format!(
            "func @f(f64, f64) -> f64 {{\nblock0(v0: f64, v1: f64):\n v2 = {op} v1, v0\n return v2\n}}"
        );
        let (original, module) = canonical(&source);
        assert_eq!(original.to_string(), module.to_string());
    }
}

struct ExposeLiteral;
impl Pass for ExposeLiteral {
    fn name(&self) -> &'static str {
        "expose-literal"
    }
    fn description(&self) -> &'static str {
        "alias an operand to a literal"
    }
    fn run(&self, f: &mut Function) -> bool {
        f.replace_all_uses(Value::new(0), Value::new(2));
        true
    }
}
struct Consumer;
impl Pass for Consumer {
    fn name(&self) -> &'static str {
        "consumer"
    }
    fn description(&self) -> &'static str {
        "require the canonical entry contract"
    }
    fn requires_canonical_form(&self) -> bool {
        true
    }
    fn run(&self, f: &mut Function) -> bool {
        assert!(!keel_opt::find_pass("canonicalize").unwrap().run(f));
        let inst = f
            .layout_insts()
            .find(|(_, i)| matches!(f.inst_data(*i), InstData::Binary { .. }))
            .unwrap()
            .1;
        let InstData::Binary { args, .. } = f.inst_data(inst) else {
            unreachable!()
        };
        assert_eq!(*args, [Value::new(1), Value::new(2)]);
        false
    }
}

#[test]
fn manager_restores_form_after_aliases_even_in_explicit_custom_pipelines() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iconst.i64 37
 v3 = iadd v0, v1
 return v3
}";
    for upfront in [false, true] {
        let mut module = keel_ir::parse(source).unwrap();
        let mut passes: Vec<&dyn Pass> = Vec::new();
        if upfront {
            passes.push(keel_opt::find_pass("canonicalize").unwrap());
        }
        passes.extend([&ExposeLiteral as &dyn Pass, &Consumer]);
        keel_opt::run_pipeline(&mut module, &passes).unwrap();
    }
    assert_eq!(
        keel_opt::OptimizationTier::Basic.pipeline()[1],
        "canonicalize"
    );
    let full = keel_opt::DEFAULT_PIPELINE;
    let inline = full
        .iter()
        .position(|p| *p == "inline-cheap-reserve")
        .unwrap();
    assert_eq!(full[inline + 2], "canonicalize");
    assert_eq!(
        keel_opt::OptimizationTier::Basic.pipeline()[0],
        "simplify-cfg"
    );
    assert_eq!(full[inline + 1], "simplify-cfg");
    assert!(keel_opt::OptimizationTier::None.pipeline().is_empty());
}

#[test]
fn literals_sort_last_even_when_their_value_ids_are_smaller() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for op in [
            "iadd",
            "imul",
            "and",
            "or",
            "xor",
            "umulhi",
            "smulhi",
            "uadd_overflow",
            "sadd_overflow",
            "umul_overflow",
            "smul_overflow",
        ] {
            let results = if op.ends_with("overflow") {
                "v2, v3"
            } else {
                "v2"
            };
            let source = format!(
                "func @f() -> {ty} {{\nblock0:\n v0 = iconst.{ty} 37\n jump block1(v0)\nblock1(v1: {ty}):\n {results} = {op} v0, v1\n return v2\n}}"
            );
            let (_, module) = canonical(&source);
            assert!(
                module.to_string().contains(&format!("{op} v1, v0")),
                "{module}"
            );
        }
    }
}
