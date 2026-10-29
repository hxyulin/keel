//! Width-aware comparisons, possible-bit masks and shifts, constant offsets,
//! and selects: each rule is checked against an independent evaluator after
//! instcombine, the Fast tier and the Full tier.

use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InstData, IntCC, Module, Type, UnaryOp, Value,
};
use keel_opt::OptimizationTier;

fn mask(ty: Type) -> u128 {
    u128::MAX >> (128 - 8 * ty.bytes().unwrap_or(8))
}

fn signed(value: u128, ty: Type) -> i128 {
    let shift = 128 - 8 * ty.bytes().unwrap_or(8);
    ((value << shift) as i128) >> shift
}

/// Straight-line evaluator with u128 storage, independent of optimizer helpers.
/// Pointers are 64-bit addresses. Division by zero panics as a trap would.
fn execute(func: &Function, inputs: &[u128]) -> u128 {
    let mut values = vec![0u128; func.num_values()];
    let block = func.entry_block().unwrap();
    for (&param, &input) in func.block_params(block).iter().zip(inputs) {
        values[param.index()] = input & mask(func.value_type(param));
    }
    for &inst in func.block_insts(block) {
        let read = |v: Value| values[func.resolve(v).index()];
        let ty_of = |v: Value| func.value_type(func.resolve(v));
        let result = match func.inst_data(inst) {
            InstData::Iconst { imm, .. } => *imm as i128 as u128,
            InstData::Unary { op, arg } => {
                let (x, ty) = (read(*arg), ty_of(*arg));
                let width = 8 * ty.bytes().unwrap();
                match op {
                    UnaryOp::Ineg => x.wrapping_neg(),
                    UnaryOp::Bnot => !x,
                    UnaryOp::Popcnt => u128::from(x.count_ones()),
                    UnaryOp::Clz => u128::from(x.leading_zeros() - (128 - width)),
                    UnaryOp::Ctz => u128::from(x.trailing_zeros().min(width)),
                    _ => panic!("unsupported {op:?}"),
                }
            }
            InstData::Binary { op, args } => {
                let (a, b) = (read(args[0]), read(args[1]));
                let ty = ty_of(args[0]);
                let width = 8 * ty.bytes().unwrap_or(8);
                let n = (b % u128::from(width)) as u32;
                match op {
                    BinaryOp::Iadd | BinaryOp::Ptradd => a.wrapping_add(b),
                    BinaryOp::Isub => a.wrapping_sub(b),
                    BinaryOp::Imul => a.wrapping_mul(b),
                    BinaryOp::And => a & b,
                    BinaryOp::Or => a | b,
                    BinaryOp::Xor => a ^ b,
                    BinaryOp::Shl => a << n,
                    BinaryOp::Ushr => a >> n,
                    BinaryOp::Sshr => (signed(a, ty) >> n) as u128,
                    BinaryOp::Udiv => a / b,
                    BinaryOp::Urem => a % b,
                    _ => panic!("unsupported {op:?}"),
                }
            }
            InstData::Convert { op, arg, .. } => match op {
                ConvertOp::Uext | ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                    read(*arg)
                }
                ConvertOp::Sext => signed(read(*arg), ty_of(*arg)) as u128,
                _ => panic!("unsupported {op:?}"),
            },
            InstData::Icmp { cond, args } => {
                let (a, b) = (read(args[0]), read(args[1]));
                let ty = ty_of(args[0]);
                let (sa, sb) = (signed(a, ty), signed(b, ty));
                u128::from(match cond {
                    IntCC::Eq => a == b,
                    IntCC::Ne => a != b,
                    IntCC::Ult => a < b,
                    IntCC::Ule => a <= b,
                    IntCC::Ugt => a > b,
                    IntCC::Uge => a >= b,
                    IntCC::Slt => sa < sb,
                    IntCC::Sle => sa <= sb,
                    IntCC::Sgt => sa > sb,
                    IntCC::Sge => sa >= sb,
                })
            }
            InstData::Select { args } => read(args[if read(args[0]) != 0 { 1 } else { 2 }]),
            InstData::Return { args } => return read(args[0]),
            data => panic!("unsupported {data:?}"),
        };
        let value = func.first_result(inst);
        values[value.index()] = result & mask(func.value_type(value));
    }
    panic!("missing return")
}

#[derive(Clone, Copy, Debug)]
enum Pipeline {
    Instcombine,
    Fast,
    Full,
}

fn optimize(source: &str, pipeline: Pipeline) -> Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    match pipeline {
        Pipeline::Instcombine => {
            keel_opt::run_passes(&mut module, &["instcombine", "dce"]).unwrap();
            let snapshot = module.to_string();
            // The worklist reaches a fixed point: a second run changes nothing.
            assert!(
                !keel_opt::find_pass("instcombine")
                    .unwrap()
                    .run(&mut module.functions[0]),
                "{source}\n{snapshot}"
            );
            assert_eq!(snapshot, module.to_string());
        }
        Pipeline::Fast => {
            keel_opt::optimize_with_tier(&mut module, OptimizationTier::Fast).unwrap()
        }
        Pipeline::Full => {
            keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap()
        }
    }
    keel_ir::verify_module(&module).unwrap();
    module
}

/// Optimizes with every pipeline, compares every input, and returns the
/// instcombine, Fast and Full results for structural checks.
fn check(source: &str, inputs: &[Vec<u128>]) -> [String; 3] {
    let original = keel_ir::parse(source).unwrap();
    let mut texts = Vec::new();
    for pipeline in [Pipeline::Instcombine, Pipeline::Fast, Pipeline::Full] {
        let module = optimize(source, pipeline);
        for input in inputs {
            assert_eq!(
                execute(&module.functions[0], input),
                execute(&original.functions[0], input),
                "{pipeline:?} {input:?}\n{source}\n{module}"
            );
        }
        texts.push(module.to_string());
    }
    texts.try_into().unwrap()
}

fn singles(values: impl IntoIterator<Item = u128>) -> Vec<Vec<u128>> {
    values.into_iter().map(|v| vec![v]).collect()
}

fn boundaries(ty: Type) -> Vec<u128> {
    let full = mask(ty);
    let sign = full ^ (full >> 1);
    let mut values = vec![0, 1, 2, 3, 15, 16, 127, 128, 255, 256, 0x7fff, 0x8000];
    values.extend([sign - 1, sign, sign + 1, full - 2, full - 1, full]);
    values.into_iter().map(|v| v & full).collect()
}

const WIDTHS: [(&str, Type); 5] = [
    ("i8", Type::I8),
    ("i16", Type::I16),
    ("i32", Type::I32),
    ("i64", Type::I64),
    ("i128", Type::I128),
];
const PREDICATES: [&str; 10] = [
    "eq", "ne", "ult", "ule", "ugt", "uge", "slt", "sle", "sgt", "sge",
];

fn literal(value: u128, ty: Type) -> i128 {
    signed(value, ty)
}

#[test]
fn every_i8_comparison_constant_and_operand_range_is_exact() {
    // Shapes with full, masked, shifted, extended and counted ranges.
    let shapes = [
        ("v1 = iadd v0, v0", "v1"),
        ("v1 = iconst.i8 13\n v9 = and v0, v1", "v9"),
        ("v1 = iconst.i8 2\n v9 = ushr v0, v1", "v9"),
        ("v1 = iconst.i8 3\n v9 = shl v0, v1", "v9"),
        ("v9 = popcnt v0", "v9"),
        ("v1 = iconst.i8 -4\n v9 = or v0, v1", "v9"),
    ];
    // Fast runs the same instcombine rules, so it only needs the range edges
    // of the shapes above and the sign boundary.
    let fast_constants = [
        0, 1, 2, 3, 7, 8, 9, 12, 13, 14, 15, 16, 63, 64, 126, 127, 128, 129, 251, 252, 253, 254,
        255,
    ];
    let inputs = singles(0..256);
    for (body, operand) in shapes {
        for cc in PREDICATES {
            for k in 0..256u128 {
                let source = format!(
                    "func @f(i8) -> i8 {{\nblock0(v0: i8):\n {body}\n v20 = iconst.i8 {}\n v21 = icmp.{cc} {operand}, v20\n return v21\n}}",
                    literal(k, Type::I8)
                );
                let original = keel_ir::parse(&source).unwrap();
                let pipelines: &[Pipeline] = if fast_constants.contains(&k) {
                    &[Pipeline::Instcombine, Pipeline::Fast]
                } else {
                    &[Pipeline::Instcombine]
                };
                for &pipeline in pipelines {
                    let module = optimize(&source, pipeline);
                    for input in &inputs {
                        assert_eq!(
                            execute(&module.functions[0], input),
                            execute(&original.functions[0], input),
                            "{pipeline:?} {input:?}\n{source}\n{module}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn wide_comparison_boundaries_fold_or_become_equality() {
    for (name, ty) in WIDTHS {
        let ones = mask(ty);
        let sign = ones ^ (ones >> 1);
        for cc in PREDICATES {
            // i128 literals are sign-extended i64 immediates.
            for k in boundaries(ty)
                .into_iter()
                .filter(|&k| i64::try_from(literal(k, ty)).is_ok())
            {
                let source = format!(
                    "func @f({name}) -> i8 {{\nblock0(v0: {name}):\n v1 = iconst.{name} {}\n v2 = icmp.{cc} v0, v1\n return v2\n}}",
                    literal(k, ty)
                );
                let [combined, _, full] = check(&source, &singles(boundaries(ty)));
                // Over the whole type, how many values satisfy the predicate,
                // and how many do not? Signed order is unsigned with the sign
                // bit flipped.
                let key = if cc.starts_with('s') { k ^ sign } else { k };
                let below = key; // values whose key is below k's
                let above = ones - key;
                // An i128 count of 2^128 saturates; only 0 and 1 matter.
                let plus = |n: u128| n.saturating_add(1);
                let (true_count, false_count) = match &cc[1..] {
                    "lt" => (below, plus(above)),
                    "le" => (plus(below), above),
                    "gt" => (above, plus(below)),
                    "ge" => (plus(above), below),
                    _ => continue,
                };
                if true_count == 0 || false_count == 0 {
                    assert!(!combined.contains("icmp"), "{source}\n{combined}");
                    assert!(!full.contains("icmp"), "{source}\n{full}");
                } else if (true_count == 1 || false_count == 1) && ty != Type::I128 {
                    for text in [&combined, &full] {
                        assert!(
                            text.contains("icmp.eq") || text.contains("icmp.ne"),
                            "{source}\n{text}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn extension_ranges_decide_comparisons_and_unsigned_tests_reach_zero() {
    for (name, ty) in WIDTHS.into_iter().skip(1) {
        for cc in PREDICATES {
            for k in [0i128, 1, 127, 128, 255, 256, -1, -128] {
                let source = format!(
                    "func @f(i8) -> i8 {{\nblock0(v0: i8):\n v1 = uext.{name} v0\n v2 = iconst.{name} {k}\n v3 = icmp.{cc} v1, v2\n return v3\n}}"
                );
                let [combined, _, full] = check(&source, &singles(0..256));
                let k = k as u128 & mask(ty);
                let out_of_range = k > 255;
                if out_of_range || (k == 0 && matches!(cc, "ult" | "uge" | "slt" | "sge")) {
                    assert!(!combined.contains("icmp"), "{source}\n{combined}");
                    assert!(!full.contains("icmp"), "{source}\n{full}");
                }
                // `uext x < 1` is `x == 0`, which then drops the extension.
                if k == 1 && matches!(cc, "ult" | "slt") {
                    assert!(!combined.contains("uext"), "{source}\n{combined}");
                    assert!(!full.contains("uext"), "{source}\n{full}");
                }
            }
        }
    }
}

#[test]
fn equality_looks_through_invertible_operations() {
    for op in ["iadd", "isub", "xor"] {
        for c1 in [0i128, 1, 5, 127, -128, -1] {
            for c2 in [0i128, 1, 6, 127, -128, -1] {
                for cc in ["eq", "ne"] {
                    let source = format!(
                        "func @f(i8) -> i8 {{\nblock0(v0: i8):\n v1 = iconst.i8 {c1}\n v2 = {op} v0, v1\n v3 = iconst.i8 {c2}\n v4 = icmp.{cc} v2, v3\n return v4\n}}"
                    );
                    let [combined, _, full] = check(&source, &singles(0..256));
                    for text in [combined, full] {
                        assert!(!text.contains(&format!(" = {op}")), "{source}\n{text}");
                    }
                }
            }
        }
        for cc in ["eq", "ne"] {
            let source = format!(
                "func @f(i8, i8) -> i8 {{\nblock0(v0: i8, v1: i8):\n v2 = {op} v0, v1\n v3 = iconst.i8 0\n v4 = icmp.{cc} v2, v3\n return v4\n}}"
            );
            let inputs: Vec<_> = (0..256)
                .flat_map(|x| (0..256).map(move |y| vec![x, y]))
                .collect();
            let [combined, _, full] = check(&source, &inputs);
            for text in [combined, full] {
                // Addition is not invertible this way: x + y == 0 needs a negation.
                assert_eq!(text.contains(&format!(" = {op}")), op == "iadd", "{text}");
            }
        }
    }
    // An unrepresentable i128 target (1 + 2^63) keeps the original form.
    let source = "func @f(i128) -> i8 {\nblock0(v0: i128):\n v1 = iconst.i128 -9223372036854775808\n v2 = iadd v0, v1\n v3 = iconst.i128 1\n v4 = icmp.eq v2, v3\n return v4\n}";
    let [combined, _, _] = check(source, &singles(boundaries(Type::I128)));
    assert!(combined.contains(" = iadd"), "{combined}");
}

#[test]
fn masks_that_keep_every_possible_bit_are_removed() {
    let cases = [
        // (parameter, body, result, removed instruction)
        (
            "i8",
            "v1 = uext.i32 v0\n v2 = iconst.i32 255\n v3 = and v1, v2",
            "i32",
            " = and",
        ),
        (
            "i32",
            "v1 = iconst.i32 24\n v2 = ushr v0, v1\n v4 = iconst.i32 255\n v3 = and v2, v4",
            "i32",
            " = and",
        ),
        (
            "i32",
            "v1 = iconst.i32 28\n v2 = shl v0, v1\n v4 = iconst.i32 -268435456\n v3 = and v2, v4",
            "i32",
            " = and",
        ),
        (
            "i32",
            "v1 = iconst.i32 7\n v2 = icmp.ult v0, v1\n v5 = uext.i32 v2\n v4 = iconst.i32 1\n v3 = and v5, v4",
            "i32",
            " = and",
        ),
        (
            "i32",
            "v1 = iconst.i32 15\n v2 = and v0, v1\n v4 = iconst.i32 16\n v3 = and v2, v4",
            "i32",
            " = and v2",
        ),
        (
            "i32",
            "v1 = iconst.i32 3\n v2 = and v0, v1\n v4 = iconst.i32 3\n v3 = or v2, v4",
            "i32",
            " = or",
        ),
        (
            "i8",
            "v1 = uext.i64 v0\n v2 = iconst.i64 8\n v3 = ushr v1, v2",
            "i64",
            " = ushr",
        ),
        (
            "i32",
            "v1 = clz v0\n v2 = iconst.i32 6\n v3 = ushr v1, v2",
            "i32",
            " = ushr",
        ),
        (
            "i64",
            "v1 = iconst.i64 -1\n v2 = and v0, v1\n v4 = iconst.i64 255\n v5 = and v2, v4\n v3 = trunc.i8 v5",
            "i8",
            " = and",
        ),
        (
            "i64",
            "v1 = iconst.i64 256\n v2 = or v0, v1\n v3 = trunc.i8 v2",
            "i8",
            " = or",
        ),
        (
            "i64",
            "v1 = iconst.i64 -256\n v2 = xor v0, v1\n v3 = trunc.i8 v2",
            "i8",
            " = xor",
        ),
    ];
    for (param, body, result, removed) in cases {
        let source = format!(
            "func @f({param}) -> {result} {{\nblock0(v0: {param}):\n {body}\n return v3\n}}"
        );
        let ty = match param {
            "i8" => Type::I8,
            "i32" => Type::I32,
            _ => Type::I64,
        };
        let mut inputs: Vec<u128> = (0..256).collect();
        inputs.extend(boundaries(ty));
        let [combined, _, full] = check(&source, &singles(inputs));
        assert!(!combined.contains(removed), "{source}\n{combined}");
        assert!(!full.contains(removed), "{source}\n{full}");
    }
    // A mask that clears a possible bit stays, as does truncating a mask that
    // clears a low bit.
    for body in [
        "v1 = uext.i32 v0\n v2 = iconst.i32 127\n v3 = and v1, v2",
        "v1 = uext.i32 v0\n v2 = iconst.i32 -2\n v4 = and v1, v2\n v5 = trunc.i16 v4\n v3 = uext.i32 v5",
    ] {
        let source = format!("func @f(i8) -> i32 {{\nblock0(v0: i8):\n {body}\n return v3\n}}");
        let [combined, _, full] = check(&source, &singles(0..256));
        assert!(combined.contains(" = and") && full.contains(" = and"));
    }
}

#[test]
fn constant_shift_pairs_cover_every_i8_count_pair_including_reduced_counts() {
    // Counts reduce modulo 8. These cover zero, small, equal and overflowing
    // sums, and counts of 8 and more.
    let counts = [0, 1, 2, 7, 8, 9, 15, 16];
    let inputs = singles(0..256);
    for inner in ["shl", "ushr", "sshr"] {
        for outer in ["shl", "ushr", "sshr"] {
            for m in counts {
                for n in counts {
                    let source = format!(
                        "func @f(i8) -> i8 {{\nblock0(v0: i8):\n v1 = iconst.i8 {m}\n v2 = {inner} v0, v1\n v3 = iconst.i64 {n}\n v4 = {outer} v2, v3\n return v4\n}}"
                    );
                    let [combined, _, full] = check(&source, &inputs);
                    let (m, n) = (m % 8, n % 8);
                    let merged = m != 0
                        && n != 0
                        && (inner == outer
                            || (m == n
                                && matches!(
                                    (inner, outer),
                                    ("shl", "ushr") | ("ushr" | "sshr", "shl")
                                )));
                    if merged {
                        for text in [&combined, &full] {
                            let shifts = [" = shl", " = ushr", " = sshr"]
                                .iter()
                                .map(|s| text.matches(s).count())
                                .sum::<usize>();
                            assert!(shifts <= 1, "{source}\n{text}");
                        }
                    }
                }
            }
        }
    }
    // An i128 low mask beyond the immediate range keeps both shifts.
    let source = "func @f(i128) -> i128 {\nblock0(v0: i128):\n v1 = iconst.i64 3\n v2 = shl v0, v1\n v3 = ushr v2, v1\n return v3\n}";
    let [combined, _, _] = check(source, &singles(boundaries(Type::I128)));
    assert!(combined.contains(" = ushr"), "{combined}");
    // Wider shift pairs at their boundaries.
    for (name, ty) in WIDTHS {
        let width = 8 * ty.bytes().unwrap();
        for (m, n) in [
            (1, 1),
            (width / 2, width / 2),
            (width - 1, 1),
            (width - 1, width - 1),
        ] {
            for (inner, outer) in [
                ("shl", "ushr"),
                ("ushr", "shl"),
                ("sshr", "sshr"),
                ("shl", "shl"),
            ] {
                let source = format!(
                    "func @f({name}) -> {name} {{\nblock0(v0: {name}):\n v1 = iconst.i32 {m}\n v2 = {inner} v0, v1\n v3 = iconst.i32 {n}\n v4 = {outer} v2, v3\n return v4\n}}"
                );
                check(&source, &singles(boundaries(ty)));
            }
        }
    }
}

#[test]
fn constant_offsets_group_once_unshared_and_wrap_at_their_width() {
    let inputs: Vec<_> = singles((0..256).chain(boundaries(Type::I64)));
    for (name, ty) in WIDTHS {
        for inner in ["iadd", "isub"] {
            for outer in ["iadd", "isub"] {
                for (c1, c2) in [(1i128, 1i128), (3, -3), (127, 1), (-128, -1), (5, 7)] {
                    let source = format!(
                        "func @f({name}) -> {name} {{\nblock0(v0: {name}):\n v1 = iconst.{name} {c1}\n v2 = {inner} v0, v1\n v3 = iconst.{name} {c2}\n v4 = {outer} v2, v3\n return v4\n}}"
                    );
                    let inputs: Vec<_> = inputs.iter().map(|v| vec![v[0] & mask(ty)]).collect();
                    let [combined, _, _] = check(&source, &inputs);
                    let ops =
                        combined.matches(" = iadd").count() + combined.matches(" = isub").count();
                    assert!(ops <= 1, "{source}\n{combined}");
                }
            }
        }
    }
    // Pointer offsets group exactly; a shared inner offset stays.
    let source = "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = inttoptr.ptr v0\n v2 = iconst.i64 24\n v3 = ptradd v1, v2\n v4 = iconst.i64 -8\n v5 = ptradd v3, v4\n v6 = ptrtoint.i64 v5\n return v6\n}";
    let [combined, _, _] = check(source, &inputs);
    assert_eq!(combined.matches(" = ptradd").count(), 1, "{combined}");
    let source = "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = inttoptr.ptr v0\n v2 = iconst.i64 24\n v3 = ptradd v1, v2\n v4 = iconst.i64 -24\n v5 = ptradd v3, v4\n v6 = ptrtoint.i64 v5\n return v6\n}";
    let [combined, _, _] = check(source, &inputs);
    assert!(!combined.contains("ptradd"), "{combined}");
    let source = "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = inttoptr.ptr v0\n v2 = iconst.i64 24\n v3 = ptradd v1, v2\n v4 = iconst.i64 8\n v5 = ptradd v3, v4\n v6 = ptrtoint.i64 v5\n v7 = ptrtoint.i64 v3\n v8 = xor v6, v7\n return v8\n}";
    let [combined, _, full] = check(source, &inputs);
    assert_eq!(combined.matches(" = ptradd").count(), 2, "{combined}");
    assert_eq!(full.matches(" = ptradd").count(), 2, "{full}");
    // Fast has no use information, so it never regroups.
    let source = "func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = iconst.i64 3\n v2 = iadd v0, v1\n v3 = iconst.i64 4\n v4 = iadd v2, v3\n return v4\n}";
    let [_, fast, _] = check(source, &inputs);
    assert_eq!(fast.matches(" = iadd").count(), 2, "{fast}");
}

#[test]
fn selects_use_tested_values_and_boolean_conditions_directly() {
    let inputs: Vec<_> = (0..256)
        .flat_map(|x| [0u128, 1, 200].map(move |y| vec![x, y, 255 - y]))
        .collect();
    for cc in ["eq", "ne"] {
        let source = format!(
            "func @f(i8, i8, i8) -> i8 {{\nblock0(v0: i8, v1: i8, v2: i8):\n v3 = iconst.i8 0\n v4 = icmp.{cc} v0, v3\n v5 = select v4, v1, v2\n return v5\n}}"
        );
        let [combined, _, full] = check(&source, &inputs);
        for text in [combined, full] {
            assert!(!text.contains("icmp"), "{text}");
            assert!(text.contains("select v0,"), "{text}");
        }
    }
    for (name, ty) in WIDTHS {
        let source = format!(
            "func @f(i8, i8) -> {name} {{\nblock0(v0: i8, v1: i8):\n v2 = icmp.slt v0, v1\n v3 = iconst.{name} 1\n v4 = iconst.{name} 0\n v5 = select v2, v3, v4\n return v5\n}}"
        );
        let inputs: Vec<_> = (0..256)
            .flat_map(|x| [0u128, 1, 127, 128, 255].map(move |y| vec![x, y]))
            .collect();
        let [combined, _, full] = check(&source, &inputs);
        for text in [combined, full] {
            assert!(!text.contains("select"), "{text}");
            assert_eq!(text.contains("uext"), ty != Type::I8, "{text}");
        }
        // A 0/1 condition wider than the result is truncated.
        let source = format!(
            "func @f({name}) -> i8 {{\nblock0(v0: {name}):\n v1 = iconst.{name} 1\n v2 = and v0, v1\n v3 = iconst.i8 1\n v4 = iconst.i8 0\n v5 = select v2, v3, v4\n return v5\n}}"
        );
        check(&source, &singles(boundaries(ty)));
    }
    // Neither a non-0/1 condition nor reversed arms are rewritten here, and an
    // i128 condition keeps its comparison.
    let [combined, _, _] = check(
        "func @f(i8) -> i8 {\nblock0(v0: i8):\n v1 = iconst.i8 1\n v2 = iconst.i8 0\n v3 = select v0, v1, v2\n return v3\n}",
        &singles(0..256),
    );
    assert!(combined.contains("select"));
    let [combined, _, _] = check(
        "func @f(i128, i8, i8) -> i8 {\nblock0(v0: i128, v1: i8, v2: i8):\n v3 = iconst.i128 0\n v4 = icmp.ne v0, v3\n v5 = select v4, v1, v2\n return v5\n}",
        &[vec![0, 1, 2], vec![1 << 100, 1, 2], vec![u128::MAX, 3, 4]],
    );
    assert!(combined.contains("icmp.ne"), "{combined}");
}

#[test]
fn rewrites_keep_trapping_producers_and_strict_floats() {
    for body in [
        "v2 = udiv v0, v1\n v3 = iconst.i64 0\n v4 = and v2, v3",
        "v2 = urem v0, v1\n v3 = iconst.i64 0\n v5 = icmp.ult v2, v3\n v4 = uext.i64 v5",
        "v2 = udiv v0, v1\n v3 = iconst.i64 5\n v5 = iadd v2, v3\n v6 = iconst.i64 9\n v7 = icmp.eq v5, v6\n v4 = uext.i64 v7",
    ] {
        let source = format!(
            "func @f(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n {body}\n return v4\n}}"
        );
        for pipeline in [Pipeline::Instcombine, Pipeline::Fast, Pipeline::Full] {
            let text = optimize(&source, pipeline).to_string();
            assert!(
                text.contains("udiv") || text.contains("urem"),
                "{pipeline:?}\n{text}"
            );
        }
    }
    let source = "func @f(f64, f64) -> f64 {\nblock0(v0: f64, v1: f64):\n v2 = fadd v0, v1\n v3 = fsub v2, v1\n return v3\n}";
    for pipeline in [Pipeline::Instcombine, Pipeline::Fast, Pipeline::Full] {
        let text = optimize(source, pipeline).to_string();
        assert!(text.contains("fadd") && text.contains("fsub"), "{text}");
    }
}

#[test]
fn fast_keeps_its_original_local_rules() {
    // Width-aware rules are Full/Basic only: measured Fast cost exceeded the
    // size benefit. Fast still evaluates the same inputs identically.
    for source in [
        "func @f(i32) -> i8 {\nblock0(v0: i32):\n v1 = iconst.i32 1\n v2 = icmp.ult v0, v1\n return v2\n}",
        "func @f(i32) -> i32 {\nblock0(v0: i32):\n v1 = iconst.i32 24\n v2 = ushr v0, v1\n v3 = iconst.i32 255\n v4 = and v2, v3\n return v4\n}",
    ] {
        let [combined, fast, full] = check(source, &singles(boundaries(Type::I32)));
        assert_ne!(combined, fast, "{fast}");
        assert_ne!(full, fast, "{fast}");
        assert!(
            fast.contains("icmp.ult") || fast.contains(" = and"),
            "{fast}"
        );
    }
}
