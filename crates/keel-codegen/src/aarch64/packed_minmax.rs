//! Closed adjacent I32x4 integer compare/select covers, across selection tiers.
use super::*;
use keel_ir::{VectorBinaryOp, VectorType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Smin,
    Smax,
    Umin,
    Umax,
}
impl Kind {
    pub(super) fn encoding(self) -> (Opcode, &'static str) {
        match self {
            Self::Smin => (Opcode::SMIN_asimdsame_only, "smin"),
            Self::Smax => (Opcode::SMAX_asimdsame_only, "smax"),
            Self::Umin => (Opcode::UMIN_asimdsame_only, "umin"),
            Self::Umax => (Opcode::UMAX_asimdsame_only, "umax"),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct MinMax {
    pub kind: Kind,
    pub args: [Value; 2],
}
const TY: Type = Type::Vector(VectorType::I32x4);

// Inspect the actual producer, never a stale instruction whose result aliases
// another value. Operand aliases to fresh canonical leaves remain legal.
fn producer(f: &Function, i: Inst, ty: Type) -> Option<Value> {
    let [r] = *f.inst_results(i) else { return None };
    (f.resolve(r) == r && f.value_def(r) == ValueDef::Result(i, 0) && f.value_type(r) == ty)
        .then_some(r)
}

// Fresh exact literal proof; neither folded arithmetic nor a stale producer
// whose declared result was redirected to a parameter is a zero constant.
fn zero(f: &Function, v: Value) -> bool {
    let v = f.resolve(v);
    let ValueDef::Result(s, 0) = f.value_def(v) else {
        return false;
    };
    if producer(f, s, TY) != Some(v) {
        return false;
    }
    let InstData::VectorSplat { ty: TY, arg } = *f.inst_data(s) else {
        return false;
    };
    let arg = f.resolve(arg);
    let ValueDef::Result(c, 0) = f.value_def(arg) else {
        return false;
    };
    producer(f, c, Type::I32) == Some(arg)
        && matches!(
            f.inst_data(c),
            InstData::Iconst {
                ty: Type::I32,
                imm: 0
            }
        )
}

pub(super) fn select(f: &Function, plan: &mut crate::select::Selection) -> HashMap<Inst, MinMax> {
    let mut selected = HashMap::new();
    if f.layout_insts()
        .any(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }))
    {
        return selected;
    }
    // Whole-layout resolved occurrences include repeated successor arguments.
    let mut uses = vec![Vec::new(); f.num_values()];
    for (_, i) in f.layout_insts() {
        let mut n = 0;
        f.inst_data(i).for_each_operand(|v| {
            uses[f.resolve(v).index()].push((i, n));
            n += 1;
        });
    }
    for block in f.blocks() {
        for pair in f.block_insts(block).windows(2) {
            let (cmp, root) = (pair[0], pair[1]);
            if pair.iter().any(|&i| {
                let mut raw = Vec::new();
                f.inst_data(i).for_each_operand(|v| raw.push(f.resolve(v)));
                plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.compare_memory.contains_key(&i)
                    || plan.binary_memory.contains_key(&i)
                    || plan.division_remainders.contains_key(&i)
                    || plan.select_compares.contains_key(&i)
                    || plan.constraints().keys().any(|&(at, _, _)| at == i)
                    || plan.operands(f, i) != raw
            }) || f.inst_loc(cmp) != f.inst_loc(root)
            {
                continue;
            }
            let (Some(mask), Some(_)) = (producer(f, cmp, TY), producer(f, root, TY)) else {
                continue;
            };
            let InstData::VectorIcmp { cond, args } = *f.inst_data(cmp) else {
                continue;
            };
            let [a, b] = args.map(|v| f.resolve(v));
            if [a, b].iter().any(|&v| f.value_type(v) != TY) {
                continue;
            }
            let (kind, mask_operand) = match *f.inst_data(root) {
                InstData::VectorSelect { args } => {
                    let (signed, less) = match cond {
                        IntCC::Slt | IntCC::Sle => (true, true),
                        IntCC::Sgt | IntCC::Sge => (true, false),
                        IntCC::Ult | IntCC::Ule => (false, true),
                        IntCC::Ugt | IntCC::Uge => (false, false),
                        _ => continue,
                    };
                    let [m, yes, no] = args.map(|v| f.resolve(v));
                    if m != mask {
                        continue;
                    }
                    let minimum = if [yes, no] == [a, b] {
                        less
                    } else if [yes, no] == [b, a] {
                        !less
                    } else {
                        continue;
                    };
                    // At equality either arm has identical bits, including <= and >=.
                    let kind = match (signed, minimum) {
                        (true, true) => Kind::Smin,
                        (true, false) => Kind::Smax,
                        (false, true) => Kind::Umin,
                        (false, false) => Kind::Umax,
                    };
                    (kind, 0)
                }
                InstData::VectorBinary {
                    op: VectorBinaryOp::And,
                    args,
                } => {
                    // Strict X > 0 / X < 0 only. Every lane is preserved,
                    // regardless of whether a later widening reads just the low half.
                    let kind = match cond {
                        IntCC::Sgt => Kind::Smax,
                        IntCC::Slt => Kind::Smin,
                        _ => continue,
                    };
                    if !zero(f, b) {
                        continue;
                    }
                    let args = args.map(|v| f.resolve(v));
                    let position = if args == [mask, a] {
                        0
                    } else if args == [a, mask] {
                        1
                    } else {
                        continue;
                    };
                    (kind, position)
                }
                _ => continue,
            };
            if uses[mask.index()] != [(root, mask_operand)] {
                continue;
            }
            plan.cover(cmp);
            // Preserve occurrences even when A and B resolve to the same leaf.
            plan.set_operands(root, vec![a, b]);
            selected.insert(root, MinMax { kind, args: [a, b] });
        }
    }
    selected
}

#[cfg(test)]
#[path = "../../tests/arm_minmax/matcher.rs"]
mod tests;
