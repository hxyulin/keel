//! Proved 16-bit contributions to an i64 reduction. Arithmetic below a
//! truncation stays modular; splitting an i32 add tree requires no overflow.
use super::*;
use keel_ir::{MemoryFlags, VectorReduceOp};

// These bounds limit recursive proofs and emitted expression growth. Unsupported
// or expensive DAGs retain the existing four-lane reduction recipe.
const MAX_DEPTH: usize = 32;
const RANGE_BUDGET: usize = 8192;
const MAX_NODES: usize = 96;
const MAX_TERMS: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Range(i64, i64);
impl Range {
    fn signed(self) -> bool {
        self.0 >= -32768 && self.1 <= 32767
    }
    fn unsigned(self) -> bool {
        self.0 >= 0 && self.1 <= 65535
    }
    fn full(ty: Type) -> Option<Self> {
        let bits = ty.bytes()? * 8;
        (ty.is_int() && bits <= 32).then(|| Self(-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1))
    }
    fn intersect(self, other: Self) -> Option<Self> {
        let r = Self(self.0.max(other.0), self.1.min(other.1));
        (r.0 <= r.1).then_some(r)
    }
}
struct Ranges<'a> {
    f: &'a Function,
    budget: usize,
    cache: FxHashMap<Value, Range>,
}
impl Ranges<'_> {
    fn get(&mut self, v: Value) -> Option<Range> {
        self.with(v, &[], 0)
    }
    fn with(&mut self, v: Value, facts: &[(Value, Range)], depth: usize) -> Option<Range> {
        let v = self.f.resolve(v);
        if facts.is_empty()
            && let Some(&r) = self.cache.get(&v)
        {
            return Some(r);
        }
        if depth > MAX_DEPTH || self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        let full = Range::full(self.f.value_type(v))?;
        let mut r = if let Some(n) = int_const(self.f, v) {
            Range(n, n)
        } else {
            match *self.f.inst_data(def(self.f, v)?) {
                D::Load { ty: Type::I16, .. } => full,
                D::Convert { op, arg, .. } => {
                    let from = self.f.value_type(arg).bytes()? * 8;
                    let a = self.with(arg, facts, depth + 1)?;
                    match op {
                        ConvertOp::Sext => a,
                        ConvertOp::Uext if a.0 >= 0 => a,
                        ConvertOp::Uext if a.1 < 0 => {
                            Range(a.0 + (1i64 << from), a.1 + (1i64 << from))
                        }
                        ConvertOp::Uext => Range(0, (1i64 << from) - 1),
                        ConvertOp::Trunc if a.0 >= full.0 && a.1 <= full.1 => a,
                        ConvertOp::Trunc => full,
                        _ => return None,
                    }
                }
                D::Binary { op, args } => {
                    let a = self.with(args[0], facts, depth + 1)?;
                    let b = self.with(args[1], facts, depth + 1)?;
                    match op {
                        B::Iadd => Range(a.0 + b.0, a.1 + b.1),
                        B::Isub => Range(a.0 - b.1, a.1 - b.0),
                        B::Imul => {
                            let p = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
                            Range(*p.iter().min()?, *p.iter().max()?)
                        }
                        B::Sdiv if b.0 == b.1 && b.0 != 0 && !(a.0 == full.0 && b.0 == -1) => {
                            let x = a.0 / b.0;
                            let y = a.1 / b.0;
                            Range(x.min(y), x.max(y))
                        }
                        B::And if b.0 == b.1 && b.0 >= 0 => Range(0, b.0),
                        B::Sshr if b.0 == b.1 => {
                            let k = b.0 as u32 & (self.f.value_type(v).bytes()? * 8 - 1);
                            Range(a.0 >> k, a.1 >> k)
                        }
                        B::Ushr if b.0 == b.1 && a.0 >= 0 => {
                            let k = b.0 as u32 & (self.f.value_type(v).bytes()? * 8 - 1);
                            Range(a.0 >> k, a.1 >> k)
                        }
                        _ => full,
                    }
                }
                D::Select { args } => {
                    let mut arms = Vec::new();
                    for (arm, truth) in [(args[1], true), (args[2], false)] {
                        let mut fs = facts.to_vec();
                        if let Some((x, limit)) = self.fact(args[0], truth) {
                            let prior = self.with(x, facts, depth + 1)?;
                            let Some(limit) = prior.intersect(limit) else {
                                continue;
                            };
                            fs.push((x, limit));
                        }
                        arms.push(self.with(arm, &fs, depth + 1)?);
                    }
                    Range(
                        arms.iter().map(|r| r.0).min()?,
                        arms.iter().map(|r| r.1).max()?,
                    )
                }
                _ => return None,
            }
        };
        // Wrapped arithmetic has the full signed range of its own scalar ring.
        if r.0 < full.0 || r.1 > full.1 {
            r = full;
        }
        for &(x, limit) in facts {
            if self.f.resolve(x) == v {
                r = r.intersect(limit)?;
            }
        }
        if facts.is_empty() {
            self.cache.insert(v, r);
        }
        Some(r)
    }
    fn fact(&self, predicate: Value, truth: bool) -> Option<(Value, Range)> {
        let D::Icmp { cond, args } = *self.f.inst_data(def(self.f, predicate)?) else {
            return None;
        };
        let k = int_const(self.f, args[1])?;
        let all = Range::full(self.f.value_type(args[0]))?;
        let r = match (cond, truth) {
            (IntCC::Slt, true) | (IntCC::Sge, false) => Range(all.0, k - 1),
            (IntCC::Sle, true) | (IntCC::Sgt, false) => Range(all.0, k),
            (IntCC::Sgt, true) | (IntCC::Sle, false) => Range(k + 1, all.1),
            (IntCC::Sge, true) | (IntCC::Slt, false) => Range(k, all.1),
            _ => return None,
        };
        Some((self.f.resolve(args[0]), r))
    }
}
enum Node {
    Constant(i64),
    Load(MemoryFlags),
    Binary(V, usize, usize),
    Shift(V, usize, i64),
    Cast(usize, u32, bool),
    Select(IntCC, [usize; 4]),
    Divide(usize, NarrowDivision),
}
pub(super) struct Plan {
    nodes: Vec<(Node, Inst)>,
    signed: Vec<(usize, bool)>,
    unsigned: Vec<usize>,
}
struct Build<'a> {
    f: &'a Function,
    c: &'a Candidate,
    ranges: Ranges<'a>,
    nodes: Vec<(Node, Inst)>,
    values: FxHashMap<Value, usize>,
}
impl Build<'_> {
    fn value(&mut self, v: Value) -> Option<usize> {
        let v = self.f.resolve(v);
        if let Some(&n) = self.values.get(&v) {
            return Some(n);
        }
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let node = if let Some(n) = int_const(self.f, v) {
            Node::Constant(n)
        } else {
            if self.f.inst_block(def(self.f, v)?) != self.c.body {
                return None;
            }
            match *self.f.inst_data(def(self.f, v)?) {
                D::Load {
                    ty: Type::I16,
                    flags,
                    addr,
                } if !flags.volatile && self.c.addresses.contains_key(&self.f.resolve(addr)) => {
                    Node::Load(flags)
                }
                D::Convert { op, arg, .. } => {
                    let bits = match op {
                        ConvertOp::Trunc => self.f.value_type(v).bytes()? * 8,
                        ConvertOp::Sext | ConvertOp::Uext => self.f.value_type(arg).bytes()? * 8,
                        _ => return None,
                    };
                    if bits > 16 {
                        return None;
                    }
                    let mut input = arg;
                    if matches!(op, ConvertOp::Sext | ConvertOp::Uext)
                        && let D::Convert {
                            op: ConvertOp::Trunc,
                            arg,
                            ..
                        } = *self.f.inst_data(def(self.f, input)?)
                    {
                        input = arg;
                    }
                    let r = self.ranges.get(input)?;
                    let identity = match op {
                        ConvertOp::Sext => {
                            r.0 >= -(1i64 << (bits - 1)) && r.1 < (1i64 << (bits - 1))
                        }
                        ConvertOp::Uext => r.0 >= 0 && r.1 < (1i64 << bits),
                        _ => false,
                    };
                    if identity {
                        let n = self.value(input)?;
                        self.values.insert(v, n);
                        return Some(n);
                    }
                    Node::Cast(self.value(input)?, bits, op == ConvertOp::Sext)
                }
                D::Binary { op: B::Sdiv, args } => {
                    let plan = *self.c.divisions.get(&v)?;
                    if !plan.signed_product || !self.ranges.get(args[0])?.signed() {
                        return None;
                    }
                    Node::Divide(self.value(args[0])?, plan)
                }
                D::Binary {
                    op: op @ (B::Shl | B::Sshr | B::Ushr),
                    args,
                } => {
                    let bits = self.f.value_type(v).bytes()? * 8;
                    let k = (int_const(self.f, args[1])? as u64 & u64::from(bits - 1)) as i64;
                    let a = self.value(args[0])?;
                    let range = self.ranges.get(args[0])?;
                    if k == 0 {
                        self.values.insert(v, a);
                        return Some(a);
                    }
                    match op {
                        B::Shl if k >= 16 => Node::Constant(0),
                        B::Shl => Node::Shift(V::Shl, a, k),
                        B::Sshr if range.signed() => Node::Shift(V::Sshr, a, k.min(15)),
                        B::Sshr | B::Ushr if range.unsigned() && k >= 16 => Node::Constant(0),
                        B::Sshr | B::Ushr if range.unsigned() => Node::Shift(V::Ushr, a, k),
                        _ => return None,
                    }
                }
                D::Binary { op, args } if matches!(self.f.value_type(v), Type::I16 | Type::I32) => {
                    let op = match op {
                        B::Iadd => V::Iadd,
                        B::Isub => V::Isub,
                        B::Imul => V::Imul,
                        B::And => V::And,
                        B::Or => V::Or,
                        B::Xor => V::Xor,
                        _ => return None,
                    };
                    Node::Binary(op, self.value(args[0])?, self.value(args[1])?)
                }
                D::Select { args } => {
                    let D::Icmp {
                        cond,
                        args: compared,
                    } = *self.f.inst_data(def(self.f, args[0])?)
                    else {
                        return None;
                    };
                    let a = self.ranges.get(compared[0])?;
                    let b = self.ranges.get(compared[1])?;
                    let valid = match cond {
                        IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge => {
                            a.signed() && b.signed()
                        }
                        IntCC::Ult | IntCC::Ule | IntCC::Ugt | IntCC::Uge => {
                            a.unsigned() && b.unsigned()
                        }
                        IntCC::Eq | IntCC::Ne => {
                            a.signed() && b.signed() || a.unsigned() && b.unsigned()
                        }
                    };
                    if !valid {
                        return None;
                    }
                    if let Some(op) =
                        reduction_minmax(self.f, args).or_else(|| clamp_minmax(self.f, args))
                    {
                        Node::Binary(op, self.value(args[1])?, self.value(args[2])?)
                    } else {
                        Node::Select(
                            cond,
                            [
                                self.value(compared[0])?,
                                self.value(compared[1])?,
                                self.value(args[1])?,
                                self.value(args[2])?,
                            ],
                        )
                    }
                }
                _ => return None,
            }
        };
        let n = self.nodes.len();
        self.nodes.push((node, def(self.f, v)?));
        self.values.insert(v, n);
        Some(n)
    }
    fn terms(&mut self, v: Value, negative: bool, out: &mut Vec<(Value, bool)>) -> Option<()> {
        if out.len() >= MAX_TERMS {
            return None;
        }
        if let D::Binary {
            op: op @ (B::Iadd | B::Isub),
            args,
        } = *self.f.inst_data(def(self.f, v)?)
        {
            let a = self.ranges.get(args[0])?;
            let b = self.ranges.get(args[1])?;
            let r = if op == B::Iadd {
                Range(a.0 + b.0, a.1 + b.1)
            } else {
                Range(a.0 - b.1, a.1 - b.0)
            };
            if self.f.value_type(v) != Type::I32 || r.0 < i32::MIN as i64 || r.1 > i32::MAX as i64 {
                return None;
            }
            self.terms(args[0], negative, out)?;
            self.terms(args[1], negative ^ (op == B::Isub), out)?;
        } else {
            out.push((v, negative));
        }
        Some(())
    }
}
impl Plan {
    pub(super) fn find(f: &Function, c: &Candidate) -> Option<Self> {
        let (_, root, op) = split_narrow_sum(f, c, VectorTarget::Aarch64Neon)?;
        if op != ConvertOp::SwidenLow
            || c.stream_bytes != 2
            || f.value_type(c.iv) != Type::I64
            || int_const(f, c.start) != Some(0)
        {
            return None;
        }
        let mut b = Build {
            f,
            c,
            ranges: Ranges {
                f,
                budget: RANGE_BUDGET,
                cache: FxHashMap::default(),
            },
            nodes: vec![],
            values: FxHashMap::default(),
        };
        let mut terms = vec![];
        b.terms(root, false, &mut terms)?;
        let mut signed = vec![];
        let mut unsigned = vec![];
        let mut sum = Range(0, 0);
        for (v, negative) in terms {
            let r = b.ranges.get(v)?;
            let node = b.value(v)?;
            if r.signed() {
                sum = if negative {
                    Range(sum.0 - r.1, sum.1 - r.0)
                } else {
                    Range(sum.0 + r.0, sum.1 + r.1)
                };
                if !sum.signed() {
                    return None;
                }
                signed.push((node, negative));
            } else if r.unsigned() && !negative {
                unsigned.push(node);
            } else {
                return None;
            }
        }
        if signed.is_empty()
            || unsigned.len() > 2
            || b.nodes
                .iter()
                .filter(|(n, _)| matches!(n, Node::Load(_)))
                .count()
                != 1
        {
            return None;
        }
        Some(Self {
            nodes: b.nodes,
            signed,
            unsigned,
        })
    }
    pub(super) fn emit(
        self,
        f: &mut Function,
        c: &Candidate,
        setup: Block,
        body: Block,
        remaining: Value,
        count: Value,
    ) {
        let Output::Sum { initial, .. } = c.output else {
            unreachable!()
        };
        let shift = iconst(f, setup, Type::I64, 3);
        let groups = bin(f, setup, B::Ushr, count, shift);
        let length = bin(f, setup, B::Shl, groups, shift);
        let zero = iconst(f, setup, Type::I64, 0);
        let z = emit(
            f,
            setup,
            D::VectorSplat {
                ty: Type::Vector(VectorType::I64x2),
                arg: zero,
            },
        );
        let lo = f.append_block_param(body, Type::Vector(VectorType::I64x2));
        let hi = f.append_block_param(body, Type::Vector(VectorType::I64x2));
        let pointer = f.append_block_param(body, Type::Ptr);
        let base = *c.addresses.values().next().unwrap();
        jump(f, setup, body, vec![length, z, z, base]);
        let mut constants = FxHashMap::default();
        let mut values = vec![];
        for (node, origin) in self.nodes {
            let before = f.block_insts(body).len();
            let value = match node {
                Node::Constant(n) => splat(f, setup, &mut constants, VectorType::I16x8, n),
                Node::Load(flags) => emit(
                    f,
                    body,
                    D::Load {
                        ty: Type::Vector(VectorType::I16x8),
                        addr: pointer,
                        flags,
                    },
                ),
                Node::Binary(op, a, b) => vbin(f, body, op, values[a], values[b]),
                Node::Shift(op, a, k) => {
                    let k = splat(f, setup, &mut constants, VectorType::I16x8, k);
                    vbin(f, body, op, values[a], k)
                }
                Node::Cast(a, bits, signed) => {
                    if bits == 16 {
                        values[a]
                    } else if signed {
                        let k = splat(
                            f,
                            setup,
                            &mut constants,
                            VectorType::I16x8,
                            i64::from(16 - bits),
                        );
                        let shifted = vbin(f, body, V::Shl, values[a], k);
                        vbin(f, body, V::Sshr, shifted, k)
                    } else {
                        let mask = splat(
                            f,
                            setup,
                            &mut constants,
                            VectorType::I16x8,
                            (1i64 << bits) - 1,
                        );
                        vbin(f, body, V::And, values[a], mask)
                    }
                }
                Node::Select(cond, [a, b, t, e]) => {
                    let mask = emit(
                        f,
                        body,
                        D::VectorIcmp {
                            cond,
                            args: [values[a], values[b]],
                        },
                    );
                    emit(
                        f,
                        body,
                        D::VectorSelect {
                            args: [mask, values[t], values[e]],
                        },
                    )
                }
                Node::Divide(a, plan) => {
                    let m = splat(f, setup, &mut constants, VectorType::I32x4, plan.multiplier);
                    let shift = splat(f, setup, &mut constants, VectorType::I32x4, plan.shift);
                    let sign = splat(f, setup, &mut constants, VectorType::I32x4, 31);
                    let mut halves = vec![];
                    for op in [ConvertOp::SwidenLow, ConvertOp::SwidenHigh] {
                        let input = emit(
                            f,
                            body,
                            D::Convert {
                                op,
                                ty: Type::Vector(VectorType::I32x4),
                                arg: values[a],
                            },
                        );
                        let product = vbin(f, body, V::Imul, input, m);
                        let q = vbin(f, body, V::Sshr, product, shift);
                        let correction = vbin(f, body, V::Ushr, input, sign);
                        let q = vbin(f, body, V::Iadd, q, correction);
                        halves.push(emit(
                            f,
                            body,
                            D::Convert {
                                op: ConvertOp::NarrowLow,
                                ty: Type::Vector(VectorType::I16x8),
                                arg: q,
                            },
                        ));
                    }
                    let q = emit(
                        f,
                        body,
                        D::VectorShuffle {
                            args: [halves[0], halves[1]],
                            lanes: vec![0, 1, 2, 3, 8, 9, 10, 11],
                        },
                    );
                    if plan.negative {
                        let z = splat(f, setup, &mut constants, VectorType::I16x8, 0);
                        vbin(f, body, V::Isub, z, q)
                    } else {
                        q
                    }
                }
            };
            if f.block_insts(body).len() > before {
                let last = *f.block_insts(body).last().unwrap();
                f.set_inst_loc(last, f.inst_loc(origin));
            }
            values.push(value);
        }
        let mut signed = None;
        for (n, negative) in self.signed {
            signed = Some(match (signed, negative) {
                (None, false) => values[n],
                (None, true) => {
                    let z = splat(f, setup, &mut constants, VectorType::I16x8, 0);
                    vbin(f, body, V::Isub, z, values[n])
                }
                (Some(s), negative) => vbin(
                    f,
                    body,
                    if negative { V::Isub } else { V::Iadd },
                    s,
                    values[n],
                ),
            });
        }
        let signed = signed.unwrap();
        let mut sum = emit(
            f,
            body,
            D::Convert {
                op: ConvertOp::SpairwiseAddLong,
                ty: Type::Vector(VectorType::I32x4),
                arg: signed,
            },
        );
        for n in self.unsigned {
            let u = emit(
                f,
                body,
                D::Convert {
                    op: ConvertOp::UpairwiseAddLong,
                    ty: Type::Vector(VectorType::I32x4),
                    arg: values[n],
                },
            );
            sum = vbin(f, body, V::Iadd, sum, u);
        }
        let mut nexts = vec![];
        for (acc, op) in [(lo, ConvertOp::SwidenLow), (hi, ConvertOp::SwidenHigh)] {
            let w = emit(
                f,
                body,
                D::Convert {
                    op,
                    ty: Type::Vector(VectorType::I64x2),
                    arg: sum,
                },
            );
            nexts.push(vbin(f, body, V::Iadd, acc, w));
        }
        let step = iconst(f, body, Type::I64, 8);
        let next = bin(f, body, B::Isub, remaining, step);
        let bytes = iconst(f, body, Type::I64, 16);
        let p = bin(f, body, B::Ptradd, pointer, bytes);
        let zero = iconst(f, body, Type::I64, 0);
        let more = cmp(f, body, IntCC::Ne, next, zero);
        let finish = f.create_block();
        branch(
            f,
            body,
            more,
            body,
            vec![next, nexts[0], nexts[1], p],
            finish,
            vec![],
        );
        let sum = vbin(f, finish, V::Iadd, nexts[0], nexts[1]);
        let total = emit(
            f,
            finish,
            D::VectorReduce {
                op: VectorReduceOp::Add,
                arg: sum,
            },
        );
        let total = bin(f, finish, B::Iadd, initial, total);
        jump(f, finish, c.header, vec![length, total]);
    }
}
fn vbin(f: &mut Function, b: Block, op: V, a: Value, c: Value) -> Value {
    emit(f, b, D::VectorBinary { op, args: [a, c] })
}
// max(lo,min(x,hi)) and min(hi,max(x,lo)), with ordered literal bounds.
fn clamp_minmax(f: &Function, args: [Value; 3]) -> Option<V> {
    let D::Icmp { cond, args: cmp } = *f.inst_data(def(f, args[0])?) else {
        return None;
    };
    let bound = int_const(f, cmp[1])?;
    if int_const(f, args[1]) != Some(bound) {
        return None;
    }
    let D::Select { args: inner } = *f.inst_data(def(f, args[2])?) else {
        return None;
    };
    let op = reduction_minmax(f, inner)?;
    let other = if f.resolve(inner[1]) == f.resolve(cmp[0]) {
        inner[2]
    } else if f.resolve(inner[2]) == f.resolve(cmp[0]) {
        inner[1]
    } else {
        return None;
    };
    let other = int_const(f, other)?;
    match (cond, op) {
        (IntCC::Slt | IntCC::Sle, V::Smin) if bound <= other => Some(V::Smax),
        (IntCC::Sgt | IntCC::Sge, V::Smax) if bound >= other => Some(V::Smin),
        _ => None,
    }
}
fn splat(
    f: &mut Function,
    b: Block,
    cache: &mut FxHashMap<(VectorType, i64), Value>,
    shape: VectorType,
    n: i64,
) -> Value {
    *cache
        .entry((shape, n))
        .or_insert_with(|| reduction_splat(f, b, shape, n))
}
