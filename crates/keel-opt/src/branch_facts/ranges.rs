//! Scoped constant bounds. Every expression query has a fixed work budget.
use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, EntityRef, Function, InstData, IntCC, OverflowOp, Type, Value, ValueDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Interval {
    lo: i128,
    hi: i128,
}
impl Interval {
    fn intersect(self, other: Self) -> Option<Self> {
        let result = Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.min(other.hi),
        };
        (result.lo <= result.hi).then_some(result)
    }
}
// Endpoint arithmetic must fit both the host and the IR domain. Otherwise a
// wrapping result supplies no interval fact.
fn arithmetic(op: BinaryOp, a: Interval, b: Interval, domain: Interval) -> Option<Interval> {
    let endpoints = match op {
        BinaryOp::Iadd => [
            a.lo.checked_add(b.lo),
            a.hi.checked_add(b.hi),
            a.lo.checked_add(b.hi),
            a.hi.checked_add(b.lo),
        ],
        BinaryOp::Isub => [
            a.lo.checked_sub(b.lo),
            a.hi.checked_sub(b.hi),
            a.lo.checked_sub(b.hi),
            a.hi.checked_sub(b.lo),
        ],
        BinaryOp::Imul => [
            a.lo.checked_mul(b.lo),
            a.hi.checked_mul(b.hi),
            a.lo.checked_mul(b.hi),
            a.hi.checked_mul(b.lo),
        ],
        _ => return None,
    };
    let [Some(a), Some(b), Some(c), Some(d)] = endpoints else {
        return None;
    };
    let result = Interval {
        lo: a.min(b).min(c).min(d),
        hi: a.max(b).max(c).max(d),
    };
    (result.lo >= domain.lo && result.hi <= domain.hi).then_some(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bounds {
    unsigned: Interval,
    signed: Interval,
}
fn width(ty: Type) -> Option<u32> {
    matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64).then(|| ty.bytes().unwrap() * 8)
}
impl Bounds {
    fn full(w: u32) -> Self {
        let sign = 1i128 << (w - 1);
        Self {
            unsigned: Interval {
                lo: 0,
                hi: (1i128 << w) - 1,
            },
            signed: Interval {
                lo: -sign,
                hi: sign - 1,
            },
        }
    }
    fn literal(imm: i64, w: u32) -> Self {
        let unsigned = i128::from(imm) & ((1i128 << w) - 1);
        // Iconst is sign-extended from its declared width.
        Self {
            unsigned: Interval {
                lo: unsigned,
                hi: unsigned,
            },
            signed: Interval {
                lo: i128::from(imm),
                hi: i128::from(imm),
            },
        }
    }
    fn normalize(mut self, w: u32) -> Option<Self> {
        let sign = 1i128 << (w - 1);
        let modulus = 1i128 << w;
        if self.unsigned.hi < sign {
            self.signed = self.signed.intersect(self.unsigned)?;
        } else if self.unsigned.lo >= sign {
            self.signed = self.signed.intersect(Interval {
                lo: self.unsigned.lo - modulus,
                hi: self.unsigned.hi - modulus,
            })?;
        }
        if self.signed.lo >= 0 {
            self.unsigned = self.unsigned.intersect(self.signed)?;
        } else if self.signed.hi < 0 {
            self.unsigned = self.unsigned.intersect(Interval {
                lo: self.signed.lo + modulus,
                hi: self.signed.hi + modulus,
            })?;
        }
        Some(self)
    }
}

#[derive(Default)]
pub(super) struct Ranges {
    facts: FxHashMap<Value, Bounds>,
    undo: Vec<(Value, Option<Bounds>)>,
    known: Vec<(u128, u128)>,
}
impl Ranges {
    pub fn mark(&self) -> usize {
        self.undo.len()
    }
    pub fn restore(&mut self, mark: usize) {
        while self.undo.len() > mark {
            let (value, previous) = self.undo.pop().expect("undo length exceeds mark");
            if let Some(previous) = previous {
                self.facts.insert(value, previous);
            } else {
                self.facts.remove(&value);
            }
        }
    }
    fn record(&mut self, value: Value, bounds: Bounds) {
        if self.facts.get(&value) == Some(&bounds) {
            return;
        }
        let previous = self.facts.insert(value, bounds);
        self.undo.push((value, previous));
    }
    fn bounds(&self, f: &Function, value: Value, fuel: &mut usize) -> Option<Bounds> {
        let value = f.resolve(value);
        let w = width(f.value_type(value))?;
        if let Some(bounds) = self.facts.get(&value) {
            return Some(*bounds);
        }
        let full = Bounds::full(w);
        let seed = self
            .known
            .get(value.index())
            .and_then(|&(lo, hi)| {
                Bounds {
                    unsigned: Interval {
                        lo: lo as i128,
                        hi: hi as i128,
                    },
                    ..full
                }
                .normalize(w)
            })
            .unwrap_or(full);
        if *fuel == 0 {
            return Some(seed);
        }
        *fuel -= 1;
        let ValueDef::Result(i, 0) = f.value_def(value) else {
            return Some(seed);
        };
        let result = match *f.inst_data(i) {
            InstData::Iconst { imm, .. } => Bounds::literal(imm, w),
            InstData::Binary {
                op: BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::Imul,
                args,
            }
            | InstData::Overflow { args, .. } => {
                let op = match *f.inst_data(i) {
                    InstData::Binary { op, .. } => op,
                    InstData::Overflow { op, .. } => match op {
                        OverflowOp::Uadd | OverflowOp::Sadd => BinaryOp::Iadd,
                        OverflowOp::Usub | OverflowOp::Ssub => BinaryOp::Isub,
                        OverflowOp::Umul | OverflowOp::Smul => BinaryOp::Imul,
                    },
                    _ => unreachable!("arithmetic definition"),
                };
                let a = self.bounds(f, args[0], fuel)?;
                let b = self.bounds(f, args[1], fuel)?;
                Bounds {
                    unsigned: arithmetic(op, a.unsigned, b.unsigned, full.unsigned)
                        .unwrap_or(full.unsigned),
                    signed: arithmetic(op, a.signed, b.signed, full.signed).unwrap_or(full.signed),
                }
            }
            InstData::Binary {
                op: BinaryOp::Urem,
                args,
            } => {
                let a = self.bounds(f, args[0], fuel)?;
                let b = self.bounds(f, args[1], fuel)?;
                if b.unsigned.lo > 0 {
                    Bounds {
                        unsigned: Interval {
                            lo: 0,
                            hi: a.unsigned.hi.min(b.unsigned.hi - 1),
                        },
                        ..full
                    }
                } else {
                    seed
                }
            }
            InstData::Binary {
                op: BinaryOp::And,
                args,
            } => {
                let a = self.bounds(f, args[0], fuel)?;
                let b = self.bounds(f, args[1], fuel)?;
                Bounds {
                    unsigned: Interval {
                        lo: 0,
                        hi: a.unsigned.hi.min(b.unsigned.hi),
                    },
                    ..full
                }
            }
            _ => seed,
        };
        Bounds {
            unsigned: result.unsigned.intersect(seed.unsigned)?,
            signed: result.signed.intersect(seed.signed)?,
        }
        .normalize(w)
    }
    /// A unique conditional incoming edge establishes the condition and,
    /// bounds on both comparison operands.
    pub fn on_edge(&mut self, f: &Function, condition: Value, taken: bool) {
        let condition = f.resolve(condition);
        let Some(w) = width(f.value_type(condition)) else {
            return;
        };
        let Some(data) = super::key(f, condition) else {
            let mut bounds = self
                .facts
                .get(&condition)
                .copied()
                .unwrap_or_else(|| Bounds::full(w));
            if taken {
                bounds.unsigned.lo = bounds.unsigned.lo.max(1);
            } else {
                bounds = Bounds::literal(0, w);
            }
            if let Some(bounds) = bounds.normalize(w) {
                self.record(condition, bounds);
            }
            return;
        };
        self.record(condition, Bounds::literal(i64::from(taken), w));
        let data = if taken { data } else { super::inverse(&data) };
        let InstData::Icmp { cond, args } = data else {
            unreachable!("comparison key")
        };
        self.restrict(f, cond, args);
        self.restrict(f, crate::gvn::reverse(cond), [args[1], args[0]]);
    }
    fn restrict(&mut self, f: &Function, cond: IntCC, args: [Value; 2]) {
        let value = f.resolve(args[0]);
        let Some(w) = width(f.value_type(value)) else {
            return;
        };
        let Some(c) = self.bounds(f, args[1], &mut 16) else {
            return;
        };
        let mut b = self
            .bounds(f, value, &mut 16)
            .unwrap_or_else(|| Bounds::full(w));
        match cond {
            IntCC::Ult => b.unsigned.hi = b.unsigned.hi.min(c.unsigned.hi - 1),
            IntCC::Ule => b.unsigned.hi = b.unsigned.hi.min(c.unsigned.hi),
            IntCC::Ugt => b.unsigned.lo = b.unsigned.lo.max(c.unsigned.lo + 1),
            IntCC::Uge => b.unsigned.lo = b.unsigned.lo.max(c.unsigned.lo),
            IntCC::Slt => b.signed.hi = b.signed.hi.min(c.signed.hi - 1),
            IntCC::Sle => b.signed.hi = b.signed.hi.min(c.signed.hi),
            IntCC::Sgt => b.signed.lo = b.signed.lo.max(c.signed.lo + 1),
            IntCC::Sge => b.signed.lo = b.signed.lo.max(c.signed.lo),
            IntCC::Eq => {
                let (Some(unsigned), Some(signed)) = (
                    b.unsigned.intersect(c.unsigned),
                    b.signed.intersect(c.signed),
                ) else {
                    return;
                };
                b = Bounds { unsigned, signed };
            }
            IntCC::Ne if c.unsigned.lo == c.unsigned.hi => {
                if b.unsigned.lo == c.unsigned.lo {
                    b.unsigned.lo += 1;
                }
                if b.unsigned.hi == c.unsigned.lo {
                    b.unsigned.hi -= 1;
                }
            }
            IntCC::Ne => {}
        }
        if b.unsigned.lo <= b.unsigned.hi
            && b.signed.lo <= b.signed.hi
            && let Some(b) = b.normalize(w)
        {
            self.record(value, b);
        }
    }
    pub fn truth(&self, f: &Function, condition: Value) -> Option<bool> {
        if self.facts.is_empty() {
            return None;
        }
        let condition = f.resolve(condition);
        let Some(InstData::Icmp { cond, args }) = super::key(f, condition) else {
            let b = self.bounds(f, condition, &mut 16)?;
            return if b.unsigned.hi == 0 {
                Some(false)
            } else if b.unsigned.lo > 0 {
                Some(true)
            } else {
                None
            };
        };
        let mut fuel = 16;
        let a = self.bounds(f, args[0], &mut fuel)?;
        let b = self.bounds(f, args[1], &mut fuel)?;
        let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
        let (a, b) = if signed {
            (a.signed, b.signed)
        } else {
            (a.unsigned, b.unsigned)
        };
        match cond {
            IntCC::Eq | IntCC::Ne => {
                let same = a.lo == a.hi && a == b;
                let different = a.hi < b.lo || b.hi < a.lo;
                if same {
                    Some(cond == IntCC::Eq)
                } else if different {
                    Some(cond == IntCC::Ne)
                } else {
                    None
                }
            }
            IntCC::Ult | IntCC::Slt if a.hi < b.lo => Some(true),
            IntCC::Ult | IntCC::Slt if a.lo >= b.hi => Some(false),
            IntCC::Ule | IntCC::Sle if a.hi <= b.lo => Some(true),
            IntCC::Ule | IntCC::Sle if a.lo > b.hi => Some(false),
            IntCC::Ugt | IntCC::Sgt if a.lo > b.hi => Some(true),
            IntCC::Ugt | IntCC::Sgt if a.hi <= b.lo => Some(false),
            IntCC::Uge | IntCC::Sge if a.lo >= b.hi => Some(true),
            IntCC::Uge | IntCC::Sge if a.hi < b.lo => Some(false),
            _ => None,
        }
    }
}

pub(super) fn run(f: &mut Function) -> bool {
    let known = crate::known_bits::unsigned_bounds(f);
    run_mode(f, false, known)
}
pub(super) fn run_checked(f: &mut Function, known: Vec<(u128, u128)>) -> bool {
    run_mode(f, true, known)
}

fn run_mode(f: &mut Function, checked_only: bool, known: Vec<(u128, u128)>) -> bool {
    if checked_only && !f.layout_insts().any(|(_, inst)| matches!(f.inst_data(inst), InstData::Overflow { args, .. } if width(f.value_type(f.resolve(args[0]))).is_some())) {
        return false;
    }
    let Some(entry) = f.entry_block() else {
        return false;
    };
    let analysis = crate::analysis::get(f);
    let children = analysis.dom.children();
    let mut ranges = Ranges {
        known,
        ..Ranges::default()
    };
    let mut stack = vec![(entry, false, 0)];
    let mut changed = false;
    while let Some((block, leaving, mark)) = stack.pop() {
        if leaving {
            ranges.restore(mark);
            continue;
        }
        let mark = ranges.mark();
        if block != entry
            && let [pred] = analysis.cfg.preds(block)
            && let Some(&i) = f.block_insts(*pred).last()
            && let InstData::Brif { cond, dests } = f.inst_data(i)
            && dests[0].block != dests[1].block
        {
            ranges.on_edge(f, *cond, dests[0].block == block);
        }
        stack.push((block, true, mark));
        if checked_only {
            for inst in f.block_insts(block).to_vec() {
                let InstData::Overflow { op, args } = *f.inst_data(inst) else {
                    continue;
                };
                let Some(w) = width(f.value_type(f.resolve(args[0]))) else {
                    continue;
                };
                let mut fuel = 32;
                let (Some(a), Some(b)) = (
                    ranges.bounds(f, args[0], &mut fuel),
                    ranges.bounds(f, args[1], &mut fuel),
                ) else {
                    continue;
                };
                let (plain, signed) = match op {
                    OverflowOp::Uadd => (BinaryOp::Iadd, false),
                    OverflowOp::Usub => (BinaryOp::Isub, false),
                    OverflowOp::Umul => (BinaryOp::Imul, false),
                    OverflowOp::Sadd => (BinaryOp::Iadd, true),
                    OverflowOp::Ssub => (BinaryOp::Isub, true),
                    OverflowOp::Smul => (BinaryOp::Imul, true),
                };
                let domain = Bounds::full(w);
                let proven = if signed {
                    arithmetic(plain, a.signed, b.signed, domain.signed)
                } else {
                    arithmetic(plain, a.unsigned, b.unsigned, domain.unsigned)
                };
                if proven.is_none() {
                    continue;
                }
                crate::known_bits::replace_checked_with_plain(f, inst, plain, args);
                changed = true;
            }
        }
        if !checked_only
            && let Some(&i) = f.block_insts(block).last()
            && let InstData::Brif { cond, dests } = f.inst_data(i)
            && let Some(taken) = ranges.truth(f, *cond)
        {
            let dest = dests[usize::from(!taken)].clone();
            *f.inst_data_mut(i) = InstData::Jump { dest };
            changed = true;
        }
        for &child in children[block].iter().rev() {
            stack.push((child, false, 0));
        }
    }
    if changed {
        crate::simplify_cfg::run(f);
        crate::dce::run(f);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_arithmetic_encloses_every_four_bit_concrete_result() {
        for domain in [Interval { lo: 0, hi: 15 }, Interval { lo: -8, hi: 7 }] {
            let intervals: Vec<_> = (domain.lo..=domain.hi)
                .flat_map(|lo| (lo..=domain.hi).map(move |hi| Interval { lo, hi }))
                .collect();
            for &a in &intervals {
                for &b in &intervals {
                    for op in [BinaryOp::Iadd, BinaryOp::Isub, BinaryOp::Imul] {
                        let Some(result) = arithmetic(op, a, b, domain) else {
                            continue;
                        };
                        for x in a.lo..=a.hi {
                            for y in b.lo..=b.hi {
                                let exact = match op {
                                    BinaryOp::Iadd => x + y,
                                    BinaryOp::Isub => x - y,
                                    _ => x * y,
                                };
                                assert!(
                                    exact >= result.lo
                                        && exact <= result.hi
                                        && exact >= domain.lo
                                        && exact <= domain.hi,
                                    "{op:?} {a:?} {b:?} {x} {y} -> {result:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
