#[path = "../src/run_balance_predicate.rs"]
mod run_balance_predicate;

use keel_ir::{BinaryOp as B, EntityRef, InstData as D, IntCC, Type};

const BALANCE: &str = r#"
func @rk_run_balance(ptr, ptr, ptr, i64) -> i64 cc(c) {
block0(v0: ptr, v1: ptr, v2: ptr, v3: i64):
    v4 = iconst.i64 0
    jump block1(v4, v4, v4)
block1(v7: i64, v17: i64, v30: i64):
    v9 = icmp.ugt v3, v7
    brif v9, block2, block4
block2:
    v35 = iconst.i64 2
    v12 = shl v7, v35
    v13 = ptradd v0, v12
    v14 = load.i32 v13
    v15 = iconst.i32 0
    v16 = icmp.sgt v14, v15
    brif v16, block5, block6
block4:
    return v30
block5:
    v22 = uext.i64 v14
    v23 = iadd v17, v22
    jump block7(v23)
block6:
    v24 = iconst.i64 17
    v25 = icmp.ugt v17, v24
    brif v25, block8, block9
block7(v31: i64):
    v32 = iadd v30, v31
    v33 = iconst.i64 1
    v34 = iadd v7, v33
    jump block1(v34, v31, v32)
block8:
    v41 = iconst.i64 17
    v27 = isub v17, v41
    jump block7(v27)
block9:
    v42 = iconst.i64 0
    jump block7(v42)
}
"#;

fn apply(src: &str, expected: bool) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(run_balance_predicate::run(&mut m.functions[0]), expected);
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    if !expected {
        assert_eq!(before, m.to_string());
    }
    let once = m.to_string();
    assert!(!run_balance_predicate::run(&mut m.functions[0]));
    assert_eq!(once, m.to_string());
    m
}

#[test]
fn converts_real_carried_balance_shape_with_one_load_and_two_selects() {
    let m = apply(BALANCE, true);
    let f = &m.functions[0];
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), D::Load { .. }))
            .count(),
        1
    );
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), D::Select { .. }))
            .count(),
        2
    );
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), D::Brif { .. }))
            .count(),
        1
    );
    assert_eq!(f.blocks().count(), 5);
}

#[test]
fn rejects_changed_predicates_widths_arithmetic_and_carried_values() {
    for src in [
        BALANCE.replace("icmp.sgt v14", "icmp.ugt v14"),
        BALANCE.replace("uext.i64 v14", "sext.i64 v14"),
        BALANCE.replace("icmp.ugt v17", "icmp.sgt v17"),
        BALANCE.replace("iconst.i64 17", "iconst.i64 18"),
        BALANCE.replace("v27 = isub", "v27 = iadd"),
        BALANCE.replace("v32 = iadd v30, v31", "v32 = isub v30, v31"),
        BALANCE.replace("jump block1(v34, v31, v32)", "jump block1(v34, v32, v31)"),
    ] {
        apply(&src, false);
    }
}

#[test]
fn refuses_effects_in_private_arms() {
    let store = BALANCE.replace(
        "v22 = uext.i64 v14",
        "store v14, v13\n    v22 = uext.i64 v14",
    );
    apply(&store, false);
    let call = BALANCE.replace(
        "v27 = isub v17, v41",
        "v51 = udiv v17, v41\n    v27 = isub v17, v41",
    );
    apply(&call, false);
}

#[test]
fn replacement_selects_retain_the_corresponding_branch_locations() {
    let mut m = keel_ir::parse(BALANCE).unwrap();
    m.files.push("balance.c".into());
    let f = &mut m.functions[0];
    let branches: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| match *f.inst_data(i) {
            D::Brif { cond, .. } => Some((i, cond)),
            _ => None,
        })
        .collect();
    let mut locations = std::collections::HashMap::new();
    for (i, cond) in branches {
        let loc = keel_ir::SourceLoc {
            file: keel_ir::SourceFile::new(0),
            line: 100 + i.index() as u32,
            col: 1,
            scope: None,
        };
        f.set_inst_loc(i, Some(loc));
        locations.insert(cond, loc);
    }
    assert!(run_balance_predicate::run(f));
    for (_, i) in f.layout_insts() {
        if let D::Select { args } = f.inst_data(i) {
            assert_eq!(f.inst_loc(i), locations.get(&args[0]).copied());
        }
    }
    keel_ir::verify_module(&m).unwrap();
}

#[test]
fn collatz_style_parity_branch_is_a_negative_control() {
    apply(include_str!("run_balance_predicate/collatz.keel"), false);
}

fn execute(f: &keel_ir::Function, input: &[i32], initial_balance: u64, initial_sum: u64) -> u64 {
    let mut values = vec![0u64; f.num_values()];
    let mut block = f.entry_block().unwrap();
    let entry = f.block_params(block);
    for (&param, value) in
        entry
            .iter()
            .zip([0, 0, 0, input.len() as u64, initial_balance, initial_sum])
    {
        values[param.index()] = value;
    }
    for _ in 0..10_000 {
        let mut next = None;
        for &inst in f.block_insts(block) {
            let read = |v| values[f.resolve(v).index()];
            let result = match f.inst_data(inst) {
                D::Iconst { imm, .. } => Some(*imm as u64),
                D::Binary { op, args } => Some(match op {
                    B::Iadd | B::Ptradd => read(args[0]).wrapping_add(read(args[1])),
                    B::Isub => read(args[0]).wrapping_sub(read(args[1])),
                    B::Shl => read(args[0]) << read(args[1]),
                    _ => panic!("unsupported binary {op:?}"),
                }),
                D::Icmp { cond, args } => Some(u64::from(match cond {
                    IntCC::Ugt => read(args[0]) > read(args[1]),
                    IntCC::Sgt => (read(args[0]) as u32 as i32) > (read(args[1]) as u32 as i32),
                    _ => panic!("unsupported comparison {cond:?}"),
                })),
                D::Load {
                    ty: Type::I32,
                    addr,
                    ..
                } => Some(input[(read(*addr) / 4) as usize] as u32 as u64),
                D::Convert { arg, .. } => Some(read(*arg)),
                D::Select { args } => Some(read(args[if read(args[0]) != 0 { 1 } else { 2 }])),
                D::Return { args } => return read(args[0]),
                D::Jump { dest } => {
                    next = Some(dest);
                    None
                }
                D::Brif { cond, dests } => {
                    next = Some(&dests[usize::from(read(*cond) == 0)]);
                    None
                }
                data => panic!("unsupported instruction {data:?}"),
            };
            if let Some(mut result) = result {
                let value = f.first_result(inst);
                if f.value_type(value) == Type::I32 {
                    result &= u32::MAX as u64;
                }
                values[value.index()] = result;
            }
        }
        let edge = next.expect("terminator");
        let args: Vec<_> = edge
            .args
            .iter()
            .map(|&v| values[f.resolve(v).index()])
            .collect();
        block = edge.block;
        for (&param, arg) in f.block_params(block).iter().zip(args) {
            values[param.index()] = arg;
        }
    }
    panic!("execution exceeded step limit")
}

#[test]
fn zero_odd_extreme_and_wraparound_recurrences_match() {
    let src = BALANCE
        .replace("ptr, ptr, ptr, i64) ->", "ptr, ptr, ptr, i64, i64, i64) ->")
        .replace("v3: i64):", "v3: i64, v50: i64, v51: i64):")
        .replace("jump block1(v4, v4, v4)", "jump block1(v4, v50, v51)");
    let before = keel_ir::parse(&src).unwrap();
    let after = apply(&src, true);
    for input in [
        vec![],
        vec![1],
        vec![0, -1, 17],
        vec![i32::MAX, i32::MIN, 18, -7, 1],
        vec![1, 0, 0, 1, -1, 5, 0],
    ] {
        for (initial_balance, initial_sum) in [(0, 0), (17, 7), (u64::MAX - 2, u64::MAX - 5)] {
            let mut balance = initial_balance;
            let mut sum = initial_sum;
            for &x in &input {
                balance = if x > 0 {
                    balance.wrapping_add(x as u32 as u64)
                } else {
                    balance.saturating_sub(17)
                };
                sum = sum.wrapping_add(balance);
            }
            assert_eq!(
                execute(&before.functions[0], &input, initial_balance, initial_sum),
                sum
            );
            assert_eq!(
                execute(&after.functions[0], &input, initial_balance, initial_sum),
                sum
            );
        }
    }
}
