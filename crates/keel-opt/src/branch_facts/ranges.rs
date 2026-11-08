//! Scoped constant bounds. Every expression query has a fixed work budget.
use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, Function, InstData, IntCC, Type, Value, ValueDef};

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
        if *fuel == 0 {
            return Some(full);
        }
        *fuel -= 1;
        let ValueDef::Result(i, 0) = f.value_def(value) else {
            return Some(full);
        };
        let result = match *f.inst_data(i) {
            InstData::Iconst { imm, .. } => Bounds::literal(imm, w),
            InstData::Binary {
                op: BinaryOp::Iadd | BinaryOp::Isub,
                args,
            } => {
                let a = self.bounds(f, args[0], fuel)?;
                let b = self.bounds(f, args[1], fuel)?;
                let subtract = matches!(
                    f.inst_data(i),
                    InstData::Binary {
                        op: BinaryOp::Isub,
                        ..
                    }
                );
                let combine = |a: Interval, b: Interval, domain: Interval| {
                    let r = if subtract {
                        Interval {
                            lo: a.lo - b.hi,
                            hi: a.hi - b.lo,
                        }
                    } else {
                        Interval {
                            lo: a.lo + b.lo,
                            hi: a.hi + b.hi,
                        }
                    };
                    if r.lo >= domain.lo && r.hi <= domain.hi {
                        r
                    } else {
                        domain
                    }
                };
                Bounds {
                    unsigned: combine(a.unsigned, b.unsigned, full.unsigned),
                    signed: combine(a.signed, b.signed, full.signed),
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
            _ => full,
        };
        result.normalize(w)
    }
    /// A unique conditional incoming edge establishes the condition and,
    /// for a comparison with a literal, a bound on the other operand.
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
        let value = f.resolve(args[0]);
        let Some(w) = width(f.value_type(value)) else {
            return;
        };
        let Some(imm) = crate::ir_match::int_const(f, args[1]) else {
            return;
        };
        let c = Bounds::literal(imm, w);
        let mut b = self
            .bounds(f, value, &mut 16)
            .unwrap_or_else(|| Bounds::full(w));
        match cond {
            IntCC::Ult => b.unsigned.hi = b.unsigned.hi.min(c.unsigned.lo - 1),
            IntCC::Ule => b.unsigned.hi = b.unsigned.hi.min(c.unsigned.lo),
            IntCC::Ugt => b.unsigned.lo = b.unsigned.lo.max(c.unsigned.lo + 1),
            IntCC::Uge => b.unsigned.lo = b.unsigned.lo.max(c.unsigned.lo),
            IntCC::Slt => b.signed.hi = b.signed.hi.min(c.signed.lo - 1),
            IntCC::Sle => b.signed.hi = b.signed.hi.min(c.signed.lo),
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
            IntCC::Ne => {
                if b.unsigned.lo == c.unsigned.lo {
                    b.unsigned.lo += 1;
                }
                if b.unsigned.hi == c.unsigned.lo {
                    b.unsigned.hi -= 1;
                }
            }
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
    let Some(entry) = f.entry_block() else {
        return false;
    };
    let analysis = crate::analysis::get(f);
    let children = analysis.dom.children();
    let mut ranges = Ranges::default();
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
        if let Some(&i) = f.block_insts(block).last()
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
