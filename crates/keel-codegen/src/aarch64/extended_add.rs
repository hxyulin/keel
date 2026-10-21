//! Fold an exact i32 extension and optional scale into a pointer addition.
//! Only pure, single-use definitions in one block are consumed. Loads remain
//! at their original positions and the root publishes both allocated leaves.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ExtendedAdd {
    pub base: Value,
    pub index: Value,
    pub signed: bool,
    pub shift: u8,
}

fn producer(f: &Function, v: Value) -> Option<Inst> {
    let v = f.resolve(v);
    let ValueDef::Result(i, 0) = f.value_def(v) else {
        return None;
    };
    (f.inst_results(i) == [v] && !f.is_removed(i)).then_some(i)
}

/// `uses` are [`crate::fold::use_counts`] of `f`, which must have no inline
/// assembly.
pub(super) fn select(
    f: &Function,
    plan: &mut crate::select::Selection,
    uses: &[u8],
) -> HashMap<Inst, ExtendedAdd> {
    let mut out = HashMap::default();
    for (block, root) in f.layout_insts() {
        let InstData::Binary {
            op: BinaryOp::Ptradd,
            args,
        } = *f.inst_data(root)
        else {
            continue;
        };
        let [base, offset] = args.map(|v| f.resolve(v));
        let Some(&result) = f.inst_results(root).first() else {
            continue;
        };
        if f.resolve(result) != result || producer(f, result) != Some(root) {
            continue;
        }
        let Some(at) = producer(f, offset) else {
            continue;
        };
        let (extend, shift, scaled) = match *f.inst_data(at) {
            InstData::Binary {
                op: BinaryOp::Shl,
                args,
            } => {
                let Some(k) = crate::fold::iconst(f, args[1]) else {
                    continue;
                };
                if !(0..=4).contains(&k) || uses[offset.index()] != 1 {
                    continue;
                }
                let Some(e) = producer(f, args[0]) else {
                    continue;
                };
                (e, k as u8, Some(at))
            }
            _ => (at, 0, None),
        };
        let InstData::Convert {
            op: op @ (ConvertOp::Sext | ConvertOp::Uext),
            ty: Type::I64,
            arg,
        } = *f.inst_data(extend)
        else {
            continue;
        };
        let index = f.resolve(arg);
        let ext_result = f.inst_results(extend)[0];
        if f.value_type(index) != Type::I32 || uses[ext_result.index()] != 1 {
            continue;
        }
        let mut nodes = vec![extend, root];
        if let Some(i) = scaled {
            nodes.push(i);
        }
        if nodes.iter().any(|&i| {
            let mut raw = Vec::new();
            f.inst_data(i).for_each_operand(|v| raw.push(f.resolve(v)));
            let actual = plan.operands(f, i);
            // Baseline immediate folding may already remove the proven shift
            // literal from allocation; no other operand override is admitted.
            let operands_ok = actual == raw || (scaled == Some(i) && actual == [ext_result]);
            f.inst_block(i) != block
                || f.inst_loc(i) != f.inst_loc(root)
                || plan.covered(i)
                || plan.matches.contains_key(&i)
                || plan.has_constraint(i)
                || !operands_ok
        }) {
            continue;
        }
        // Do not cross source-level barriers or change an already selected
        // memory address. The intervening operations themselves stay put.
        let insts = f.block_insts(block);
        let Some(start) = insts.iter().position(|&i| i == extend) else {
            continue;
        };
        let Some(end) = insts.iter().position(|&i| i == root) else {
            continue;
        };
        if start >= end
            || insts[start..=end].iter().any(|&i| {
                f.inst_loc(i) != f.inst_loc(root)
                    || !matches!(
                        f.inst_data(i),
                        InstData::Convert { .. }
                            | InstData::Iconst { .. }
                            | InstData::Binary { .. }
                    )
            })
        {
            continue;
        }
        plan.cover(extend);
        if let Some(i) = scaled {
            plan.cover(i);
        }
        plan.set_operands(root, vec![base, index]);
        out.insert(
            root,
            ExtendedAdd {
                base,
                index,
                signed: op == ConvertOp::Sext,
                shift,
            },
        );
    }
    out
}
