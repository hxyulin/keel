//! Check allocation dataflow independently of register-choice heuristics.
//! Every definition produces a fresh symbolic token; reads must observe that
//! token after the actual parallel copies and call clobbers have executed.
use super::*;
use std::path::Path;

fn location(loc: Loc) -> usize {
    match loc {
        Loc::Reg(r) => r as usize,
        Loc::FReg(r) => 256 + r as usize,
        Loc::Stack(s) => 512 + s as usize,
        Loc::Remat(_) => unreachable!("immutable recipes have no mutable storage"),
    }
}
fn read(state: &[Option<u64>], values: &[Vec<Option<u64>>; 2], loc: Loc) -> Option<u64> {
    match loc {
        Loc::Remat(v) => values[0][v.index()],
        _ => state[location(loc)],
    }
}
fn copies(state: &mut [Option<u64>], moves: &[(Loc, Loc)], current: &[Vec<Option<u64>>; 2]) {
    let values: Vec<_> = moves
        .iter()
        .map(|&(_, src)| read(state, current, src).expect("copy reads an uninitialized location"))
        .collect();
    for (&(dst, _), value) in moves.iter().zip(values) {
        state[location(dst)] = Some(value);
    }
}
pub(super) fn check(input: &AllocationInput<'_>, allocation: &Allocation, mut seed: u64) {
    let func = input.func;
    let mut state = vec![None; 512 + allocation.spill_slots as usize];
    let mut values = [vec![None; func.num_values()], vec![None; func.num_values()]];
    let mut token = 0;
    let Some(mut block) = func.entry_block() else {
        return;
    };
    for &param in func.block_params(block) {
        for high in [false, true] {
            if high && func.value_type(param) != Type::I128 {
                continue;
            }
            token += 1;
            values[usize::from(high)][param.index()] = Some(token);
            if let Some(loc) = allocation.param_loc(block, ValuePart { value: param, high }) {
                state[location(loc)] = Some(token);
            }
        }
    }
    for _ in 0..40 {
        for &inst in func.block_insts(block) {
            if input.selection.covered(inst) {
                continue;
            }
            let index = allocation.edits.instruction_index(inst);
            if let Some(i) = index {
                copies(&mut state, &allocation.edits.instruction(i).before, &values);
            }
            input.for_each_operand(inst, |op| {
                if op.kind == OperandKind::Use {
                    let loc = allocation
                        .operand_loc(index, op.part, OperandKind::Use)
                        .expect("missing use location");
                    assert_eq!(
                        read(&state, &values, loc),
                        values[usize::from(op.part.high)][op.part.value.index()],
                        "{} {inst}: {:?} in {loc:?}",
                        func.name,
                        op.part
                    );
                    assert!(
                        read(&state, &values, loc).is_some(),
                        "use has no reaching definition"
                    );
                }
            });
            input.for_each_clobber(inst, |loc| state[location(loc)] = None);
            input.for_each_operand(inst, |op| {
                if op.kind == OperandKind::Def {
                    token += 1;
                    values[usize::from(op.part.high)][op.part.value.index()] = Some(token);
                    if let Some(loc) = allocation.operand_loc(index, op.part, OperandKind::Def) {
                        if !matches!(loc, Loc::Remat(_)) {
                            state[location(loc)] = Some(token);
                        }
                    }
                }
            });
            if let Some(i) = index {
                copies(&mut state, &allocation.edits.instruction(i).after, &values);
            }
        }
        let Some(&term) = func.block_insts(block).last() else {
            return;
        };
        let dests = func.inst_data(term).branch_dests();
        if dests.is_empty() {
            return;
        }
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let successor = (seed >> 32) as usize % dests.len();
        let dest = &dests[successor];
        let index = allocation.edits.instruction_index(term);
        let mut moves = allocation.edits.edge(term, successor).to_vec();
        let mut arguments = Vec::new();
        for (&param, &arg) in func.block_params(dest.block).iter().zip(&dest.args) {
            let arg = func.resolve(arg);
            for high in [false, true] {
                if high && func.value_type(param) != Type::I128 {
                    continue;
                }
                let value = values[usize::from(high)][arg.index()];
                arguments.push((param, high, value));
                if let Some(dst) =
                    allocation.param_loc(dest.block, ValuePart { value: param, high })
                {
                    let src = allocation
                        .operand_loc(index, ValuePart { value: arg, high }, OperandKind::Use)
                        .unwrap();
                    moves.push((dst, src));
                }
            }
        }
        copies(&mut state, &moves, &values);
        for (param, high, value) in arguments {
            values[usize::from(high)][param.index()] = value;
        }
        block = dest.block;
    }
}
#[test]
fn selected_corpus_transfers_preserve_values_across_calls_and_edges() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "keel") {
            continue;
        }
        let module = keel_ir::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for func in &module.functions {
            let is_call = |data: &InstData| {
                matches!(
                    data,
                    InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. }
                )
            };
            let returns_twice = |data: &InstData| match data {
                InstData::Call { func: c, .. } | InstData::Invoke { func: c, .. } => {
                    func.callee(*c).sig.returns_twice
                }
                InstData::CallIndirect { sig, .. } => func.signature(*sig).returns_twice,
                _ => false,
            };
            for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                let mut selection = crate::select::Selection::compute(func, target);
                selection.fuse_compares(func, |_| vec![]);
                let pools = Pools {
                    caller_saved: &[9, 10],
                    callee_saved: &[19, 20],
                    float_caller_saved: &[16, 17],
                    float_callee_saved: &[8, 9],
                };
                let input = AllocationInput {
                    func,
                    pools: &pools,
                    selection: &selection,
                    is_call: &is_call,
                    returns_twice: &returns_twice,
                };
                let tiers = vec![AllocationTier::Fast, AllocationTier::Full];
                #[cfg(feature = "experimental-regalloc")]
                let tiers = {
                    let mut tiers = tiers;
                    tiers.extend([
                        AllocationTier::Local,
                        AllocationTier::Split,
                        AllocationTier::Weighted,
                        AllocationTier::FullAffinity,
                    ]);
                    tiers
                };
                for tier in tiers {
                    let allocations = vec![tier.allocate(&input)];
                    #[cfg(feature = "experimental-regalloc")]
                    let allocations = {
                        let mut allocations = allocations;
                        if tier != AllocationTier::FullAffinity {
                            allocations.push(
                                RematerializingAllocator {
                                    base: &tier,
                                    prefer_spilling: false,
                                    constants: true,
                                    stack_addresses: true,
                                    null_pointers: false,
                                }
                                .allocate(&input),
                            );
                        }
                        if matches!(tier, AllocationTier::Fast | AllocationTier::Full) {
                            allocations.push(
                                RematerializingAllocator {
                                    base: &tier,
                                    prefer_spilling: false,
                                    constants: false,
                                    stack_addresses: false,
                                    null_pointers: true,
                                }
                                .allocate(&input),
                            );
                        }
                        if matches!(tier, AllocationTier::Fast | AllocationTier::Full) {
                            allocations.push(
                                RematerializingAllocator {
                                    base: &tier,
                                    prefer_spilling: true,
                                    constants: true,
                                    stack_addresses: true,
                                    null_pointers: false,
                                }
                                .allocate(&input),
                            );
                        }
                        allocations
                    };
                    for mut allocation in allocations {
                        allocation.finish(&input);
                        allocation.validate(&input).unwrap();
                        for seed in 0..4 {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    check(&input, &allocation, seed)
                                }));
                            assert!(
                                result.is_ok(),
                                "{} {} {tier:?} seed {seed}",
                                path.display(),
                                func.name
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn range_intersections_match_pairwise_inclusive_overlap() {
    let make = |ranges| Interval {
        value: Value::new(0),
        half: 0,
        ranges,
        crosses_call: false,
        preserved_gprs: 0,
        float: false,
        vector: false,
    };
    for a in 0..32 {
        for b in 0..64 {
            let x = make((0..8).map(|i| (a + 10 * i, a + 10 * i + 2)).collect());
            let y = make((0..8).map(|i| (b + 13 * i, b + 13 * i + 3)).collect());
            let expected = x
                .ranges
                .iter()
                .any(|x| y.ranges.iter().any(|y| x.0 <= y.1 && y.0 <= x.1));
            assert_eq!(x.intersects(&y), expected);
            assert_eq!(y.intersects(&x), expected);
        }
    }
    let prefix = make((0..10_000).map(|i| (4 * i, 4 * i + 1)).collect());
    assert!(prefix.intersects(&make(vec![(39_996, 40_000)])));
    assert!(!prefix.intersects(&make(vec![(39_994, 39_995)])));
}
