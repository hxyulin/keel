//! Lazy scalar facts tied to one immutable final function. No optimizer cache,
//! CFG stamp or enabled pass is trusted to establish a value's current bits.
use keel_ir::{BinaryOp, ConvertOp, EntityRef, Function, InstData, Type, Value, ValueDef};
use std::cell::{Cell, OnceCell};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Bits {
    zero: u64,
    one: u64,
}

pub(crate) struct ValueBits<'a> {
    func: &'a Function,
    bits: OnceCell<Vec<Cell<Option<Bits>>>>,
}
impl<'a> ValueBits<'a> {
    pub(crate) fn new(func: &'a Function) -> Self {
        Self {
            func,
            bits: OnceCell::new(),
        }
    }

    pub(crate) fn unsigned_bounds(&self, value: Value) -> Option<(u64, u64)> {
        let width = width(self.func.value_type(self.func.resolve(value)))?;
        let bits = self.get(value, 0);
        Some((bits.one, mask(width) & !bits.zero))
    }

    fn get(&self, value: Value, depth: usize) -> Bits {
        let value = self.func.resolve(value);
        let Some(width) = width(self.func.value_type(value)) else {
            return Bits::default();
        };
        // Phi values are unknown. Bound expression depth independently of the
        // input size; reaching the limit loses precision, never correctness.
        if depth == 32 {
            return Bits::default();
        }
        // Most switch operands are unknown parameters, loads or arithmetic.
        // Do not allocate a function-sized memo table for those queries.
        match self.func.value_def(value) {
            ValueDef::Result(inst, _) => match *self.func.inst_data(inst) {
                InstData::Iconst { imm, .. } => {
                    return Bits {
                        zero: !(imm as u64) & mask(width),
                        one: imm as u64 & mask(width),
                    };
                }
                InstData::Binary {
                    op:
                        BinaryOp::And | BinaryOp::Or | BinaryOp::Xor | BinaryOp::Shl | BinaryOp::Ushr,
                    ..
                }
                | InstData::Convert {
                    op: ConvertOp::Uext | ConvertOp::Trunc,
                    ..
                } => {}
                _ => return Bits::default(),
            },
            _ => return Bits::default(),
        }
        let cells = self.bits.get_or_init(|| {
            (0..self.func.num_values())
                .map(|_| Cell::new(None))
                .collect()
        });
        if let Some(bits) = cells[value.index()].get() {
            return bits;
        }
        let full = mask(width);
        let mut result = Bits::default();
        if let ValueDef::Result(inst, _) = self.func.value_def(value) {
            let child = |v| self.get(v, depth + 1);
            result = match *self.func.inst_data(inst) {
                InstData::Iconst { imm, .. } => Bits {
                    zero: !(imm as u64) & full,
                    one: imm as u64 & full,
                },
                InstData::Binary { op, args } => {
                    let (a, b) = (child(args[0]), child(args[1]));
                    match op {
                        BinaryOp::And => Bits {
                            zero: a.zero | b.zero,
                            one: a.one & b.one,
                        },
                        BinaryOp::Or => Bits {
                            zero: a.zero & b.zero,
                            one: a.one | b.one,
                        },
                        BinaryOp::Xor => Bits {
                            zero: (a.zero & b.zero) | (a.one & b.one),
                            one: (a.zero & b.one) | (a.one & b.zero),
                        },
                        BinaryOp::Shl | BinaryOp::Ushr if b.zero | b.one == full => {
                            let amount = (b.one & u64::from(width - 1)) as u32;
                            if op == BinaryOp::Shl {
                                Bits {
                                    zero: (a.zero << amount) | mask(amount),
                                    one: a.one << amount,
                                }
                            } else {
                                Bits {
                                    zero: (a.zero >> amount) | (full ^ mask(width - amount)),
                                    one: a.one >> amount,
                                }
                            }
                        }
                        _ => Bits::default(),
                    }
                }
                InstData::Convert {
                    op: ConvertOp::Uext | ConvertOp::Trunc,
                    arg,
                    ..
                } => {
                    let mut a = child(arg);
                    if let Some(from) = width_of(self.func, arg) {
                        a.zero |= full & !mask(from);
                    }
                    a
                }
                _ => Bits::default(),
            };
        }
        result.zero &= full;
        result.one &= full;
        cells[value.index()].set(Some(result));
        result
    }
}
fn width_of(func: &Function, value: Value) -> Option<u32> {
    width(func.value_type(func.resolve(value)))
}
fn width(ty: Type) -> Option<u32> {
    match ty {
        Type::I8 => Some(8),
        Type::I16 => Some(16),
        Type::I32 => Some(32),
        Type::I64 => Some(64),
        _ => None,
    }
}
fn mask(width: u32) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lazy_facts_follow_current_body_and_aliases_without_any_pass() {
        let mut module = keel_ir::parse("func @f(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 7\n v2 = and v0, v1\n return v2\n}").unwrap();
        let func = &mut module.functions[0];
        let inst = func.block_insts(func.entry_block().unwrap())[1];
        let result = func.first_result(inst);
        {
            let facts = ValueBits::new(func);
            assert!(facts.bits.get().is_none());
            assert_eq!(facts.unsigned_bounds(Value::new(0)), Some((0, u64::MAX)));
            assert!(facts.bits.get().is_none());
            assert_eq!(facts.unsigned_bounds(result), Some((0, 7)));
            assert!(facts.bits.get().is_some());
        }
        *func.inst_data_mut(inst) = InstData::Binary {
            op: BinaryOp::Or,
            args: [Value::new(0), Value::new(1)],
        };
        assert_eq!(
            ValueBits::new(func).unsigned_bounds(result),
            Some((7, u64::MAX))
        );
        func.replace_all_uses(result, Value::new(1));
        assert_eq!(ValueBits::new(func).unsigned_bounds(result), Some((7, 7)));
    }

    #[test]
    fn masked_negative_shifts_and_extensions_keep_exact_widths() {
        let module = keel_ir::parse("func @f(i8) -> i64 {\nblock0(v0:i8):\n v1 = iconst.i8 7\n v2 = and v0, v1\n v3 = iconst.i8 -1\n v4 = ushr v2, v3\n v5 = uext.i64 v4\n return v5\n}").unwrap();
        let facts = ValueBits::new(&module.functions[0]);
        assert_eq!(facts.unsigned_bounds(Value::new(2)), Some((0, 7)));
        assert_eq!(facts.unsigned_bounds(Value::new(4)), Some((0, 0)));
        assert_eq!(facts.unsigned_bounds(Value::new(5)), Some((0, 0)));
    }
}
