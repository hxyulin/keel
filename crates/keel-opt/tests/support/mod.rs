use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InstData, IntCC, OverflowOp, Type, UnaryOp, Value,
};

/// Independent execution model for the integer/control-flow subset below.
/// In particular, block arguments are copied simultaneously on each edge.
pub fn execute(func: &Function, inputs: &[u64]) -> u64 {
    fn mask(ty: Type) -> u64 {
        u64::MAX >> (64 - ty.bytes().unwrap() * 8)
    }
    let mut values = vec![0u64; func.num_values()];
    let mut block = func.entry_block().unwrap();
    for (&param, &input) in func.block_params(block).iter().zip(inputs) {
        values[func.resolve(param).index()] = input & mask(func.value_type(param));
    }
    for _ in 0..10_000 {
        let mut next = None;
        for &inst in func.block_insts(block) {
            let read = |v: Value| values[func.resolve(v).index()];
            if let InstData::Overflow {
                op: OverflowOp::Sadd,
                args,
            } = func.inst_data(inst)
            {
                let width = func.value_type(args[0]).bytes().unwrap() * 8;
                let shift = 64 - width;
                let a = (read(args[0]) << shift) as i64 >> shift;
                let b = (read(args[1]) << shift) as i64 >> shift;
                let exact = i128::from(a) + i128::from(b);
                let low = -(1i128 << (width - 1));
                let high = (1i128 << (width - 1)) - 1;
                let results = func.inst_results(inst);
                values[results[0].index()] = exact as u64 & mask(func.value_type(results[0]));
                values[results[1].index()] = u64::from(exact < low || exact > high);
                continue;
            }
            let result = match func.inst_data(inst) {
                InstData::Iconst { imm, .. } => Some(*imm as u64),
                InstData::Binary { op, args } => Some(match op {
                    BinaryOp::Iadd => read(args[0]).wrapping_add(read(args[1])),
                    BinaryOp::Isub => read(args[0]).wrapping_sub(read(args[1])),
                    BinaryOp::Umulhi => {
                        let width = func.value_type(args[0]).bytes().unwrap() * 8;
                        ((u128::from(read(args[0])) * u128::from(read(args[1]))) >> width) as u64
                    }
                    BinaryOp::Imul => read(args[0]).wrapping_mul(read(args[1])),
                    BinaryOp::And => read(args[0]) & read(args[1]),
                    BinaryOp::Or => read(args[0]) | read(args[1]),
                    BinaryOp::Xor => read(args[0]) ^ read(args[1]),
                    BinaryOp::Udiv => read(args[0]) / read(args[1]),
                    BinaryOp::Urem => read(args[0]) % read(args[1]),
                    BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                        let width = func.value_type(args[0]).bytes().unwrap() * 8;
                        let shift = read(args[1]) % u64::from(width);
                        match op {
                            BinaryOp::Shl => read(args[0]) << shift,
                            BinaryOp::Ushr => read(args[0]) >> shift,
                            BinaryOp::Sshr => {
                                (((read(args[0]) << (64 - width)) as i64 >> (64 - width)) >> shift)
                                    as u64
                            }
                            _ => unreachable!(),
                        }
                    }
                    _ => panic!("unsupported {op:?}"),
                }),
                InstData::Unary {
                    op: UnaryOp::Ineg,
                    arg,
                } => Some(read(*arg).wrapping_neg()),
                InstData::Unary {
                    op: UnaryOp::Bnot,
                    arg,
                } => Some(!read(*arg)),
                InstData::Convert { op, ty: _, arg } => Some(match op {
                    ConvertOp::Uext | ConvertOp::Trunc => read(*arg),
                    ConvertOp::Sext => {
                        let shift = 64 - func.value_type(*arg).bytes().unwrap() * 8;
                        ((read(*arg) << shift) as i64 >> shift) as u64
                    }
                    _ => panic!("unsupported {op:?}"),
                }),
                InstData::Icmp { cond, args } => Some(u64::from(match cond {
                    IntCC::Ne => read(args[0]) != read(args[1]),
                    IntCC::Ult => read(args[0]) < read(args[1]),
                    IntCC::Ule => read(args[0]) <= read(args[1]),
                    IntCC::Ugt => read(args[0]) > read(args[1]),
                    IntCC::Uge => read(args[0]) >= read(args[1]),
                    IntCC::Eq => read(args[0]) == read(args[1]),
                    IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge => {
                        let shift = 64 - func.value_type(args[0]).bytes().unwrap() * 8;
                        let a = (read(args[0]) << shift) as i64 >> shift;
                        let b = (read(args[1]) << shift) as i64 >> shift;
                        match cond {
                            IntCC::Slt => a < b,
                            IntCC::Sle => a <= b,
                            IntCC::Sgt => a > b,
                            IntCC::Sge => a >= b,
                            _ => unreachable!(),
                        }
                    }
                })),
                InstData::Select { args } => {
                    Some(read(args[if read(args[0]) != 0 { 1 } else { 2 }]))
                }
                InstData::Return { args } => return read(args[0]),
                InstData::Jump { dest } => {
                    next = Some(dest);
                    None
                }
                InstData::Brif { cond, dests } => {
                    next = Some(&dests[usize::from(read(*cond) == 0)]);
                    None
                }
                InstData::Switch { arg, dests, cases } => {
                    let m = mask(func.value_type(*arg));
                    next = Some(
                        &dests[cases
                            .iter()
                            .position(|&c| c as u64 & m == read(*arg))
                            .map_or(0, |i| i + 1)],
                    );
                    None
                }
                data => panic!("unsupported {data:?}"),
            };
            if let Some(result) = result {
                let value = func.first_result(inst);
                values[value.index()] = result & mask(func.value_type(value));
            }
        }
        let dest = next.expect("terminator");
        let arguments: Vec<_> = dest
            .args
            .iter()
            .map(|&v| values[func.resolve(v).index()])
            .collect();
        block = dest.block;
        for (&param, arg) in func.block_params(block).iter().zip(arguments) {
            // Aliased parameters no longer carry runtime data.
            if func.resolve(param) == param {
                values[param.index()] = arg;
            }
        }
    }
    panic!("execution exceeded step limit")
}
