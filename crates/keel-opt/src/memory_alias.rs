//! Function-local address facts, never IR/ABI metadata. Recompute after edits.
//!
//! Only literal i64 ptradd paths are recognized; checked signed accumulation
//! rejects overflow. Addresses still wrap modulo 2^64 on both supported targets.
//! Disjointness compares circular byte footprints, not ordered signed intervals:
//! either footprint crossing address zero must still be checked for overlap.
//! No null, alignment, notrap, or global-pointer inequality facts are inferred.
//! Restrict is used only for an access pair involving a write, as required by
//! callers: read-only restricted pointers may alias. Both roots must retain
//! their definition-only entry contracts and have only recognized address uses.
//! Inlined/cloned parameters have no such entry contract; never synthesize one.
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{BinaryOp, Function, InstData, Type, Value, ValueDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Address {
    root: Value,
    offset: i64,
}

pub(crate) struct AliasInfo {
    addresses: FxHashMap<Value, Address>,
    restricted: FxHashSet<Value>,
}

impl AliasInfo {
    pub(crate) fn compute(func: &Function) -> Self {
        let mut info = Self {
            addresses: FxHashMap::default(),
            restricted: FxHashSet::default(),
        };
        if let Some(entry) = func.entry_block() {
            for &index in &func.restrict_params {
                if let Some(&v) = func.block_params(entry).get(index as usize) {
                    info.restricted.insert(func.resolve(v));
                }
            }
        }
        // Walk paths iteratively (including cycle detection), caching each
        // prefix. Unknown derivations remain opaque, exact SSA addresses.
        for (_, inst) in func.layout_insts() {
            func.inst_data(inst).for_each_operand(|v| {
                if func.value_type(v) == Type::Ptr {
                    info.record(func, v);
                }
            });
            for &v in func.inst_results(inst) {
                if func.value_type(v) == Type::Ptr {
                    info.record(func, v);
                }
            }
        }
        for (_, inst) in func.layout_insts() {
            let data = func.inst_data(inst);
            data.for_each_operand(|v| {
                let v = func.resolve(v);
                let address = info.address(func, v);
                if !info.restricted.contains(&address.root) {
                    return;
                }
                let allowed = match data {
                    InstData::Load { addr, flags, .. } => {
                        !flags.volatile && func.resolve(*addr) == v
                    }
                    InstData::Store { addr, value, flags } => {
                        !flags.volatile && func.resolve(*addr) == v && func.resolve(*value) != v
                    }
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        args,
                    } => {
                        func.resolve(args[0]) == v
                            && constant_offset(func, args[1]).is_some()
                            && info.address(func, func.first_result(inst)).root == address.root
                    }
                    _ => false,
                };
                if !allowed {
                    info.restricted.remove(&address.root);
                }
            });
        }
        info
    }

    fn record(&mut self, func: &Function, value: Value) {
        let mut v = func.resolve(value);
        let mut path = Vec::new();
        let mut seen = FxHashSet::default();
        while !self.addresses.contains_key(&v) {
            if !seen.insert(v) {
                for (v, _) in path {
                    self.addresses.insert(v, Address { root: v, offset: 0 });
                }
                return;
            }
            let next = match func.value_def(v) {
                ValueDef::Result(i, _) => match *func.inst_data(i) {
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        args,
                    } => constant_offset(func, args[1]).map(|n| (func.resolve(args[0]), n)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((base, n)) = next {
                path.push((v, n));
                v = base;
            } else {
                self.addresses.insert(v, Address { root: v, offset: 0 });
            }
        }
        let mut base = self.addresses[&v];
        for (v, n) in path.into_iter().rev() {
            base = base
                .offset
                .checked_add(n)
                .map(|offset| Address {
                    root: base.root,
                    offset,
                })
                .unwrap_or(Address { root: v, offset: 0 });
            self.addresses.insert(v, base);
        }
    }

    pub(crate) fn address(&self, func: &Function, value: Value) -> Address {
        let value = func.resolve(value);
        self.addresses.get(&value).copied().unwrap_or(Address {
            root: value,
            offset: 0,
        })
    }

    /// Query two nonzero byte footprints, with at least one modifying access.
    /// Unknown bases alias; distinct roots require *both* eligible contracts.
    /// Callers handle zero-sized operations and all call/lifetime/observable
    /// barriers separately. Footprints are modulo the supported 64-bit pointer
    /// width; widths and distances never use unchecked signed endpoint math.
    pub(crate) fn may_alias(
        &self,
        func: &Function,
        a: Value,
        a_bytes: u64,
        b: Value,
        b_bytes: u64,
    ) -> bool {
        if a_bytes == 0 || b_bytes == 0 {
            return true;
        }
        let a = self.address(func, a);
        let b = self.address(func, b);
        if a.root == b.root {
            let distance = (b.offset as u64).wrapping_sub(a.offset as u64);
            return distance < a_bytes || distance.wrapping_neg() < b_bytes;
        }
        !(self.restricted.contains(&a.root) && self.restricted.contains(&b.root))
    }
}

fn constant_offset(func: &Function, v: Value) -> Option<i64> {
    if let ValueDef::Result(i, _) = func.value_def(func.resolve(v))
        && let InstData::Iconst { ty: Type::I64, imm } = *func.inst_data(i)
    {
        Some(imm)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circular_footprints_and_checked_paths() {
        let m = keel_ir::parse("func @f(ptr) {\nblock0(v0: ptr):\n v1 = iconst.i64 -4\n v2 = ptradd v0, v1\n v3 = iconst.i64 4\n v4 = ptradd v0, v3\n v5 = iconst.i64 9223372036854775807\n v6 = ptradd v0, v5\n v7 = ptradd v6, v3\n return\n}").unwrap();
        let f = &m.functions[0];
        let values: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| f.inst_results(i).first().copied())
            .collect();
        let root = f.block_params(f.entry_block().unwrap())[0];
        let a = AliasInfo::compute(f);
        assert!(a.may_alias(f, root, 8, values[1], 8)); // [-4,4) wraps
        assert!(!a.may_alias(f, root, 4, values[3], 4));
        assert!(a.may_alias(f, root, 8, values[6], 8)); // checked accumulation failed
        assert!(a.may_alias(f, root, 0, values[3], 4));
    }
}
