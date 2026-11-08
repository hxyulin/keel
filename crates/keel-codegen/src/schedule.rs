//! Bounded scheduling of pure integer operations before target covers form.
//! Memory operations retain their order. Moving a ready reduction earlier can
//! shorten its inputs' live ranges across a load or call without moving either.
use std::borrow::Cow;

use keel_ir::{EntityRef, Function, FunctionOptLevel, InsertPoint, InstData, Value, ValueDef};

use crate::regalloc::SequenceMotion;

/// Bound each selection decision independently of basic block size.
const WINDOW: usize = 64;

pub(crate) fn prepare(func: &Function, enabled: bool) -> Cow<'_, Function> {
    if !enabled || !eligible(func) {
        return Cow::Borrowed(func);
    }
    let mut remaining = vec![0u32; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut remaining[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut result = Cow::Borrowed(func);
    // Positions are block-local and refreshed before each block. Definitions
    // outside the current block are already available on every reachable path.
    let mut positions = vec![usize::MAX; func.num_insts()];
    for block in func.blocks() {
        let mut order = func.block_insts(block).to_vec();
        for (position, &inst) in order.iter().enumerate() {
            positions[inst.index()] = position;
        }
        for at in 0..order.len() {
            let end = (at + WINDOW).min(order.len());
            let candidate = (at..end).find(|&i| {
                let inst = order[i];
                if SequenceMotion::of(func, inst) != SequenceMotion::Integer {
                    return false;
                }
                let mut operands = [Value::new(0); 2];
                let InstData::Binary { args, .. } = *func.inst_data(inst) else {
                    return false;
                };
                for (slot, arg) in operands.iter_mut().zip(args) {
                    *slot = func.resolve(arg);
                    if let ValueDef::Result(producer, _) = func.value_def(*slot)
                        && func.inst_block(producer) == block
                        && positions[producer.index()] >= at
                    {
                        return false;
                    }
                }
                // Require a strict decrease in live values, not just a shorter
                // first input at the expense of a newly live output. Duplicate
                // input occurrences consume one register, not two.
                let killed = if operands[0] == operands[1] {
                    usize::from(remaining[operands[0].index()] == 2)
                } else {
                    operands
                        .iter()
                        .filter(|v| remaining[v.index()] == 1)
                        .count()
                };
                let produced = usize::from(remaining[func.first_result(inst).index()] != 0);
                killed > produced
            });
            if let Some(i) = candidate.filter(|&i| i > at) {
                let inst = order[i];
                // move_inst intentionally clears producer facts. Preserve them:
                // neither the operation nor its operands change here.
                let facts = func.inst_facts(inst).copied();
                let output = result.to_mut();
                output.move_inst(inst, InsertPoint::Before(order[at]));
                if let Some(facts) = facts {
                    output
                        .set_inst_facts(inst, facts)
                        .expect("unchanged scheduled instruction facts remain valid");
                }
                order[at..=i].rotate_right(1);
                for (position, &inst) in order.iter().enumerate().take(i + 1).skip(at) {
                    positions[inst.index()] = position;
                }
            }
            func.inst_data(order[at]).for_each_operand(|v| {
                let count = &mut remaining[func.resolve(v).index()];
                *count = count.saturating_sub(1);
            });
        }
    }
    result
}

fn eligible(func: &Function) -> bool {
    if func.sig.returns_twice
        || func.personality.is_some()
        || func.debug_loc.is_some()
        || !func.debug_vars.is_empty()
    {
        return false;
    }
    let policy = keel_ir::FunctionPolicy::resolve(&func.attrs);
    if policy.is_err()
        || policy.ok().and_then(|p| p.level).is_some_and(|level| {
            matches!(
                level,
                FunctionOptLevel::None | FunctionOptLevel::Fast | FunctionOptLevel::Size
            )
        })
    {
        return false;
    }
    !func.layout_insts().any(|(_, inst)| {
        func.inst_loc(inst).is_some()
            || matches!(
                func.inst_data(inst),
                InstData::InlineAsm { .. }
                    | InstData::X86RflagsRegion { .. }
                    | InstData::Invoke { .. }
                    | InstData::LandingPad { .. }
            )
            || (matches!(func.inst_data(inst), InstData::Intrinsic { id, .. }
                if !matches!(id, keel_ir::Intrinsic::Memcpy | keel_ir::Intrinsic::Memmove
                    | keel_ir::Intrinsic::Memset | keel_ir::Intrinsic::Memcmp))
                && !func.inst_data(inst).is_lifetime_marker())
            || crate::lowering::returns_twice(func, func.inst_data(inst))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "decl @sink(ptr)\nfunc @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n v2 = load.i64 v0\n v3 = load.i64 v0\n call @sink(v0)\n v4 = iadd v1, v2\n v5 = iadd v4, v3\n return v5\n}";

    #[test]
    fn reduction_moves_between_loads_and_before_call() {
        let module = keel_ir::parse(SOURCE).unwrap();
        let func = &module.functions[0];
        let output = prepare(func, true);
        assert!(matches!(output, Cow::Owned(_)));
        let block = func.entry_block().unwrap();
        let old = func.block_insts(block);
        assert_eq!(
            output.block_insts(block),
            &[old[0], old[1], old[4], old[2], old[5], old[3], old[6]]
        );
        let mut verified = module.clone();
        verified.functions[0] = output.clone().into_owned();
        keel_ir::verify_module(&verified).unwrap();
        assert!(matches!(prepare(&output, true), Cow::Borrowed(_)));
        assert!(matches!(prepare(func, false), Cow::Borrowed(_)));
        // The borrowed original remains unchanged.
        assert_eq!(func.block_insts(block)[3], old[3]);
    }

    #[test]
    fn returns_twice_and_source_locations_are_boundaries() {
        let mut module = keel_ir::parse(SOURCE).unwrap();
        let f = &mut module.functions[0];
        let first = f.block_insts(f.entry_block().unwrap())[0];
        f.set_inst_loc(
            first,
            Some(keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(0),
                line: 1,
                col: 1,
                scope: None,
            }),
        );
        assert!(matches!(prepare(f, true), Cow::Borrowed(_)));
        f.set_inst_loc(first, None);
        f.sig.returns_twice = true;
        assert!(matches!(prepare(f, true), Cow::Borrowed(_)));
    }

    #[test]
    fn repeated_input_is_one_live_value_and_does_not_trigger_motion() {
        let module = keel_ir::parse("decl @sink(ptr)\nfunc @f(ptr) -> i64 {\nblock0(v0:ptr):\n v1 = load.i64 v0\n call @sink(v0)\n v2 = iadd v1, v1\n return v2\n}").unwrap();
        assert!(matches!(
            prepare(&module.functions[0], true),
            Cow::Borrowed(_)
        ));
    }
}
