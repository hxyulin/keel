use crate::support;

use keel_ir::{BinaryOp, EntityRef, InstData, SourceLoc};
use keel_opt::OptimizationTier;

fn optimized(source: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Fast).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn narrow_integer_identities_and_power_of_two_arithmetic_are_equivalent() {
    for ty in ["i8", "i16", "i32", "i64"] {
        for op in ["iadd", "isub", "imul", "and", "or", "xor", "udiv", "urem"] {
            for literal in [0, 1, 2, 4, 8, 255, 256, -1] {
                let source = format!(
                    "func @f({ty}) -> {ty} {{
block0(v0: {ty}):
 v1 = iconst.{ty} {literal}
 v2 = {op} v0, v1
 return v2
}}"
                );
                let original = keel_ir::parse(&source).unwrap();
                let result = optimized(&source);
                let mask = u64::MAX
                    >> (64
                        - 8 * original.functions[0]
                            .value_type(keel_ir::Value::new(0))
                            .bytes()
                            .unwrap());
                if matches!(op, "udiv" | "urem") && literal as u64 & mask == 0 {
                    assert!(result.to_string().contains(op));
                    continue;
                }
                for input in (0..=255).chain([256, 65535, 1 << 31, 1 << 63, u64::MAX]) {
                    assert_eq!(
                        support::execute(&result.functions[0], &[input]),
                        support::execute(&original.functions[0], &[input]),
                        "{ty} {op} {literal}, input {input}"
                    );
                }
            }
        }
    }
}

#[test]
fn dead_results_keep_traps_loads_stores_and_calls() {
    let source = "decl @effect() -> i64
func @f(i64, ptr) -> i64 {
block0(v0: i64, v1: ptr):
 v2 = iconst.i64 0
 v3 = sdiv v0, v2
 v4 = srem v0, v2
 v5 = udiv v0, v2
 v6 = urem v0, v2
 v7 = load.i64 v1
 v8 = load.i64 volatile v1
 store v0, v1
 v9 = call @effect()
 return v0
}";
    let result = optimized(source).to_string();
    for instruction in ["sdiv", "udiv", "load", "volatile", "store", "call"] {
        assert!(
            result.contains(instruction),
            "missing {instruction}: {result}"
        );
    }
    // A preceding division can guard a reused remainder; a standalone
    // remainder by zero must retain its own trap even when unused.
    for op in ["srem", "urem"] {
        let result = optimized(&format!(
            "func @f(i64) -> i64 {{
block0(v0: i64):
 v1 = iconst.i64 0
 v2 = {op} v0, v1
 return v0
}}"
        ));
        assert!(result.to_string().contains(op));
    }
    for ty in ["i8", "i64", "i128"] {
        let result = optimized(&format!(
            "func @f({ty}) -> {ty} {{
block0(v0: {ty}):
 v1 = iconst.{ty} -1
 v2 = sdiv v0, v1
 return v2
}}"
        ));
        assert!(result.to_string().contains("sdiv"));
    }
}

#[test]
fn same_block_reuse_does_not_merge_loads_or_sibling_values() {
    let source = "func @f(i64, i8, ptr) -> i64 {
block0(v0: i64, v1: i8, v2: ptr):
 brif v1, block1, block2
block1:
 v3 = iadd v0, v0
 jump block3(v3)
block2:
 v4 = iadd v0, v0
 jump block3(v4)
block3(v5: i64):
 v6 = load.i64 v2
 store v0, v2
 v7 = load.i64 v2
 v8 = iadd v6, v7
 v9 = iadd v0, v5
 v10 = iadd v5, v0
 v11 = iadd v9, v10
 v12 = iadd v11, v8
 return v12
}";
    let result = optimized(source);
    let f = &result.functions[0];
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count(),
        2
    );
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(
                f.inst_data(*i),
                InstData::Binary {
                    op: BinaryOp::Iadd,
                    ..
                }
            ))
            .count(),
        6
    );
}

#[test]
fn cfg_preserves_argument_permutations_and_external_parameter_uses() {
    for source in [
        "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
 brif v2, block1(v0, v1), block2(v0, v1)
block1(v3: i64, v4: i64):
 jump block3(v4, v3)
block2(v5: i64, v6: i64):
 jump block3(v5, v5)
block3(v7: i64, v8: i64):
 v9 = isub v7, v8
 return v9
}",
        "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
 brif v2, block1(v0), block1(v1)
block1(v3: i64):
 jump block2
block2:
 return v3
}",
    ] {
        let original = keel_ir::parse(source).unwrap();
        let result = optimized(source);
        for a in [0, 1, 7, u64::MAX] {
            for b in [0, 99, u64::MAX] {
                for cond in [0, 1, 255] {
                    assert_eq!(
                        support::execute(&original.functions[0], &[a, b, cond]),
                        support::execute(&result.functions[0], &[a, b, cond])
                    );
                }
            }
        }
    }
}

#[test]
fn forwarding_cycle_and_long_chains_terminate() {
    optimized(
        "func @cycle(i64) {
block0(v0: i64):
 jump block1(v0)
block1(v1: i64):
 jump block2(v1)
block2(v2: i64):
 jump block1(v2)
}",
    );
    let mut source = String::from("func @f(i64) -> i64 {\nblock0(v0: i64):\n jump block1000(v0)\n");
    for block in 1..=1000 {
        let p = 3 * block;
        source.push_str(&format!(
            "block{block}(v{p}: i64):\n v{} = iconst.i64 1\n v{} = iadd v{p}, v{}\n",
            p + 1,
            p + 2,
            p + 1
        ));
        if block == 1 {
            source.push_str(&format!(" return v{}\n", p + 2));
        } else {
            source.push_str(&format!(" jump block{}(v{})\n", block - 1, p + 2));
        }
    }
    source.push('}');
    let result = optimized(&source);
    assert_eq!(support::execute(&result.functions[0], &[42]), 1042);
}

#[test]
fn folded_instruction_retains_its_source_location() {
    let mut module = keel_ir::parse(
        "func @f() -> i64 {
block0:
 v0 = iconst.i64 20
 v1 = iconst.i64 22
 v2 = iadd v0, v1
 return v2
}",
    )
    .unwrap();
    let file = module.add_file("fast.c");
    let f = &mut module.functions[0];
    let inst = f.layout_insts().nth(2).unwrap().1;
    let loc = SourceLoc {
        file,
        line: 42,
        col: 7,
        scope: None,
    };
    f.set_inst_loc(inst, Some(loc));
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Fast).unwrap();
    assert_eq!(module.functions[0].inst_loc(inst), Some(loc));
    assert!(matches!(
        module.functions[0].inst_data(inst),
        InstData::Iconst { imm: 42, .. }
    ));
    keel_ir::verify_module(&module).unwrap();
}
