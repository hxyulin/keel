//! Preserve arithmetic flags for a single branch without materializing its boolean.
use super::*;
use crate::cover::Cover;

pub(super) fn select(func: &Function, selection: &mut crate::select::Selection, uses: &[u8]) {
    if !selection.enabled(Cover::CompareBranch)
        || func
            .layout_insts()
            .any(|(_, i)| matches!(func.inst_data(i), InstData::InlineAsm { .. }))
    {
        return;
    }
    for block in func.blocks() {
        let insts = func.block_insts(block);
        let Some(&branch) = insts.last() else {
            continue;
        };
        let InstData::Brif { cond, .. } = *func.inst_data(branch) else {
            continue;
        };
        if !selection.is_free(branch) || selection.has_operand_override(branch) {
            continue;
        }
        let cond = func.resolve(cond);
        let mut flag = cond;
        let mut negate = None;
        if let ValueDef::Result(i, 0) = func.value_def(cond)
            && let InstData::Binary {
                op: BinaryOp::Xor,
                args,
            } = *func.inst_data(i)
        {
            let other = if crate::fold::iconst(func, args[0]) == Some(1) {
                args[1]
            } else if crate::fold::iconst(func, args[1]) == Some(1) {
                args[0]
            } else {
                continue;
            };
            if uses[cond.index()] != 1 || !selection.is_free(i) || selection.has_operand_override(i)
            {
                continue;
            }
            flag = func.resolve(other);
            negate = Some(i);
        }
        let ValueDef::Result(producer, 1) = func.value_def(flag) else {
            continue;
        };
        let InstData::Overflow { op, args } = *func.inst_data(producer) else {
            continue;
        };
        if !matches!(
            op,
            OverflowOp::Uadd | OverflowOp::Sadd | OverflowOp::Usub | OverflowOp::Ssub
        ) || !matches!(
            func.value_type(func.resolve(args[0])),
            Type::I32 | Type::I64
        ) || uses[flag.index()] != 1
            || !selection.is_free(producer)
            || selection.has_operand_override(producer)
            || func.inst_block(producer) != block
            || func.inst_loc(producer).is_some()
            || func.inst_loc(branch).is_some()
        {
            continue;
        }
        let at = insts
            .iter()
            .position(|&i| i == producer)
            .expect("producer in block");
        if insts[at + 1..insts.len() - 1].iter().any(|&i| {
            func.inst_loc(i).is_some()
                || (Some(i) != negate
                    && (!matches!(func.inst_data(i), InstData::Iconst { .. })
                        || !selection.is_free(i)
                        || selection.has_operand_override(i)))
        }) {
            continue;
        }
        let mut operands = selection.operands(func, branch);
        // Edge arguments remain live even when the condition is omitted.
        let Some(position) = operands.iter().position(|&v| func.resolve(v) == cond) else {
            continue;
        };
        operands.remove(position);
        selection.set_operands(branch, operands);
        selection.claim(Cover::CompareBranch, branch);
        selection.omitted_defs.insert(flag);
        if let Some(i) = negate {
            selection.cover(i);
            selection.omitted_defs.insert(cond);
            if let InstData::Binary { args, .. } = *func.inst_data(i) {
                for arg in args {
                    let arg = func.resolve(arg);
                    if uses[arg.index()] == 1
                        && let ValueDef::Result(literal, 0) = func.value_def(arg)
                        && matches!(func.inst_data(literal), InstData::Iconst { .. })
                        && selection.is_free(literal)
                        && !selection.has_operand_override(literal)
                    {
                        selection.cover(literal);
                    }
                }
            }
        }
        selection
            .overflow_branches
            .insert(branch, (producer, negate.is_some()));
    }
}
