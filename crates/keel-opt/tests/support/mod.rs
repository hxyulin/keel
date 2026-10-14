use keel_ir::{BinaryOp, EntityRef, Function, InstData, IntCC, Type, Value};

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
            let result = match func.inst_data(inst) {
                InstData::Iconst { imm, .. } => Some(*imm as u64),
                InstData::Binary { op, args } => Some(match op {
                    BinaryOp::Iadd => read(args[0]).wrapping_add(read(args[1])),
                    BinaryOp::Imul => read(args[0]).wrapping_mul(read(args[1])),
                    BinaryOp::Xor => read(args[0]) ^ read(args[1]),
                    _ => panic!("unsupported {op:?}"),
                }),
                InstData::Icmp { cond, args } => Some(u64::from(match cond {
                    IntCC::Ult => read(args[0]) < read(args[1]),
                    IntCC::Eq => read(args[0]) == read(args[1]),
                    _ => panic!("unsupported {cond:?}"),
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
