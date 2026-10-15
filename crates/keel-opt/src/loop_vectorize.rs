//! Conservative fixed-width loop vectorization with a scalar fallback.
//!
//! Recognizes zero-based, unit-stride, two-block counted loops with one store,
//! a uniform element type and no carried values except the induction variable.
//! Runtime range checks permit identical or disjoint streams, rejecting partial
//! overlap. The original loop handles short inputs, aliasing, and the tail.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, Inst, InstData as D, IntCC, Type, Value,
    ValueDef, VectorBinaryOp as V, VectorType,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Packed 128-bit capabilities used for vectorization profitability. Portable
/// uses the intersection of the two supported targets, not scalar fallbacks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VectorTarget {
    #[default]
    Portable,
    Aarch64Neon,
    X86Sse2,
}
impl VectorTarget {
    fn binary(self, op: B, ty: Type) -> Option<V> {
        Some(match op {
            B::Iadd => V::Iadd,
            B::Isub => V::Isub,
            B::And => V::And,
            B::Or => V::Or,
            B::Xor => V::Xor,
            B::Imul
                if ty == Type::I16
                    || self == Self::Aarch64Neon && matches!(ty, Type::I8 | Type::I32) =>
            {
                V::Imul
            }
            B::Fadd => V::Fadd,
            B::Fsub => V::Fsub,
            B::Fmul => V::Fmul,
            B::Fdiv => V::Fdiv,
            _ => return None,
        })
    }
}
pub(crate) struct LoopVectorize;
impl crate::Pass for LoopVectorize {
    fn name(&self) -> &'static str {
        "loop-vectorize"
    }
    fn description(&self) -> &'static str {
        "vectorize contiguous counted loops with runtime alias checks and scalar tails"
    }
    fn run(&self, f: &mut Function) -> bool {
        run(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        run(f, target)
    }
}

struct Candidate {
    header: Block,
    body: Block,
    pre: Block,
    iv: Value,
    bound: Value,
    signed: bool,
    store: Inst,
    shape: VectorType,
    // Original scalar stream address -> invariant base pointer.
    addresses: HashMap<Value, Value>,
    vectors: HashSet<Value>,
}
fn def(f: &Function, v: Value) -> Option<Inst> {
    match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => Some(i),
        _ => None,
    }
}
fn constant(f: &Function, v: Value) -> Option<i64> {
    match f.inst_data(def(f, v)?) {
        D::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}
fn invariant(f: &Function, dom: &DominatorTree, pre: Block, v: Value) -> bool {
    let b = match f.value_def(f.resolve(v)) {
        ValueDef::Result(i, _) => f.inst_block(i),
        ValueDef::Param(b, _) => b,
        _ => return false,
    };
    dom.dominates(b, pre)
}
struct Analyze<'a> {
    f: &'a Function,
    dom: &'a DominatorTree,
    c: Candidate,
    target: VectorTarget,
    allowed: HashSet<Inst>,
}
impl Analyze<'_> {
    fn index(&mut self, v: Value) -> Option<()> {
        let v = self.f.resolve(v);
        if v == self.c.iv {
            return Some(());
        }
        let i = def(self.f, v)?;
        match self.f.inst_data(i) {
            D::Convert {
                op,
                ty: Type::I64,
                arg,
            } if self.f.value_type(*arg) == Type::I32
                && (*op == ConvertOp::Uext || *op == ConvertOp::Sext && self.c.signed) =>
            {
                if self.f.resolve(*arg) != self.c.iv {
                    return None;
                }
                self.allowed.insert(i);
                Some(())
            }
            _ => None,
        }
    }
    fn address(&mut self, v: Value) -> Option<()> {
        let v = self.f.resolve(v);
        if self.c.addresses.contains_key(&v) {
            return Some(());
        }
        let i = def(self.f, v)?;
        let D::Binary {
            op: B::Ptradd,
            args: [base, offset],
        } = *self.f.inst_data(i)
        else {
            return None;
        };
        if !invariant(self.f, self.dom, self.c.pre, base) {
            return None;
        }
        let bytes = self.c.shape.lane_type().bytes()?;
        let offset = self.f.resolve(offset);
        if bytes == 1 {
            self.index(offset)?;
        } else {
            let scale = def(self.f, offset)?;
            let D::Binary { op, args } = *self.f.inst_data(scale) else {
                return None;
            };
            let amount = constant(self.f, args[1])?;
            if !(op == B::Shl && amount == i64::from(bytes.trailing_zeros())
                || op == B::Imul && amount == i64::from(bytes))
            {
                return None;
            }
            self.index(args[0])?;
            self.allowed.insert(scale);
            if let Some(i) = def(self.f, args[1]) {
                self.allowed.insert(i);
            }
        }
        self.allowed.insert(i);
        self.c.addresses.insert(v, self.f.resolve(base));
        Some(())
    }
    fn expression(&mut self, v: Value, depth: usize) -> Option<()> {
        let v = self.f.resolve(v);
        if depth > 32 || self.f.value_type(v) != self.c.shape.lane_type() {
            return None;
        }
        if self.c.vectors.contains(&v) {
            return Some(());
        }
        if invariant(self.f, self.dom, self.c.pre, v) {
            self.c.vectors.insert(v);
            return Some(());
        }
        let i = def(self.f, v)?;
        if self.f.inst_block(i) != self.c.body {
            return None;
        }
        match *self.f.inst_data(i) {
            D::Load { flags, addr, .. } if !flags.volatile => self.address(addr)?,
            D::Binary { op, args } => {
                self.target.binary(op, self.c.shape.lane_type())?;
                for arg in args {
                    self.expression(arg, depth + 1)?;
                }
            }
            D::Iconst { .. } | D::Fconst { .. } => {}
            _ => return None,
        }
        self.allowed.insert(i);
        self.c.vectors.insert(v);
        Some(())
    }
}
fn analyze(f: &Function, header: Block, target: VectorTarget) -> Option<Candidate> {
    let [iv] = *f.block_params(header) else {
        return None;
    };
    let iv = f.resolve(iv);
    if !matches!(f.value_type(iv), Type::I32 | Type::I64) {
        return None;
    }
    let [cmp, branch] = *f.block_insts(header) else {
        return None;
    };
    let D::Brif { cond, ref dests } = *f.inst_data(branch) else {
        return None;
    };
    // A two-instruction block need not start with a comparison (it may
    // start with a store or a void call, which has no result at all).
    let D::Icmp {
        cond: predicate,
        args,
    } = *f.inst_data(cmp)
    else {
        return None;
    };
    if f.resolve(cond) != f.first_result(cmp) || !dests[0].args.is_empty() {
        return None;
    }
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
    let body = dests[0].block;
    if body == header || cfg.preds(body) != [header] || !f.block_params(body).is_empty() {
        return None;
    }
    let (bound, signed) = match (
        predicate,
        f.resolve(args[0]) == iv,
        f.resolve(args[1]) == iv,
    ) {
        (IntCC::Ult, true, false) => (args[1], false),
        (IntCC::Ugt, false, true) => (args[0], false),
        (IntCC::Slt, true, false) => (args[1], true),
        (IntCC::Sgt, false, true) => (args[0], true),
        _ => return None,
    };
    let preds = cfg.preds(header);
    if preds.len() != 2 {
        return None;
    }
    let pre = *preds.iter().find(|&&b| b != body)?;
    if pre == header || !dom.dominates(header, body) || !invariant(f, &dom, pre, bound) {
        return None;
    }
    let pre_jump = *f.block_insts(pre).last()?;
    let D::Jump { dest } = f.inst_data(pre_jump) else {
        return None;
    };
    if dest.block != header || dest.args.len() != 1 || constant(f, dest.args[0]) != Some(0) {
        return None;
    }
    let back = *f.block_insts(body).last()?;
    let D::Jump { dest } = f.inst_data(back) else {
        return None;
    };
    if dest.block != header || dest.args.len() != 1 {
        return None;
    }
    let step = def(f, dest.args[0])?;
    let D::Binary { op: B::Iadd, args } = *f.inst_data(step) else {
        return None;
    };
    if !((f.resolve(args[0]) == iv && constant(f, args[1]) == Some(1))
        || (f.resolve(args[1]) == iv && constant(f, args[0]) == Some(1)))
    {
        return None;
    }
    let stores: Vec<_> = f
        .block_insts(body)
        .iter()
        .copied()
        .filter(|&i| matches!(f.inst_data(i), D::Store { .. }))
        .collect();
    let [store] = *stores.as_slice() else {
        return None;
    };
    let D::Store { flags, value, addr } = *f.inst_data(store) else {
        return None;
    };
    if flags.volatile {
        return None;
    }
    let shape = VectorType::ALL
        .into_iter()
        .find(|s| s.lane_type() == f.value_type(value))?;
    if let Some(n) = constant(f, bound) {
        let n = if f.value_type(iv) == Type::I32 {
            u64::from(n as u32)
        } else {
            n as u64
        };
        let negative = signed
            && if f.value_type(iv) == Type::I32 {
                (n as i32) < 0
            } else {
                (n as i64) < 0
            };
        if negative || n < u64::from(shape.lanes()) * 4 {
            return None;
        }
    }
    let c = Candidate {
        header,
        body,
        pre,
        iv,
        bound: f.resolve(bound),
        signed,
        store,
        shape,
        addresses: HashMap::new(),
        vectors: HashSet::new(),
    };
    let mut a = Analyze {
        f,
        dom: &dom,
        c,
        target,
        allowed: HashSet::from([back, step, store]),
    };
    a.address(addr)?;
    a.expression(value, 0)?;
    if a.c
        .addresses
        .values()
        .copied()
        .collect::<HashSet<_>>()
        .len()
        > 4
        || a.c.vectors.len() > 24
    {
        return None;
    }
    // No hidden side effects, loads, reductions, induction uses as data, or
    // values escaping to the scalar tail. Literals used for address/step are OK.
    for &i in f.block_insts(body) {
        if !a.allowed.contains(&i) && !matches!(f.inst_data(i), D::Iconst { .. } | D::Fconst { .. })
        {
            return None;
        }
    }
    for (b, i) in f.layout_insts() {
        if b == body {
            continue;
        }
        let mut escapes = false;
        f.inst_data(i).for_each_operand(|v| {
            let v = f.resolve(v);
            if (v == iv && b != header) || def(f, v).is_some_and(|d| f.inst_block(d) == body) {
                escapes = true;
            }
        });
        if escapes {
            return None;
        }
    }
    Some(a.c)
}
fn emit(f: &mut Function, b: Block, d: D) -> Value {
    let i = f.append_inst(b, d);
    f.first_result(i)
}
fn iconst(f: &mut Function, b: Block, ty: Type, n: i64) -> Value {
    emit(f, b, D::Iconst { ty, imm: n })
}
fn bin(f: &mut Function, b: Block, op: B, a: Value, c: Value) -> Value {
    emit(f, b, D::Binary { op, args: [a, c] })
}
fn cmp(f: &mut Function, b: Block, cond: IntCC, a: Value, c: Value) -> Value {
    emit(f, b, D::Icmp { cond, args: [a, c] })
}
fn jump(f: &mut Function, b: Block, to: Block, args: Vec<Value>) {
    f.append_inst(
        b,
        D::Jump {
            dest: BlockCall { block: to, args },
        },
    );
}
fn branch(
    f: &mut Function,
    b: Block,
    c: Value,
    yes: Block,
    ya: Vec<Value>,
    no: Block,
    na: Vec<Value>,
) {
    f.append_inst(
        b,
        D::Brif {
            cond: c,
            dests: [
                BlockCall {
                    block: yes,
                    args: ya,
                },
                BlockCall {
                    block: no,
                    args: na,
                },
            ],
        },
    );
}
fn transform(f: &mut Function, c: Candidate, target: VectorTarget) {
    let guard = f.create_block();
    let setup = f.create_block();
    let vector = f.create_block();
    let ty = f.value_type(c.iv);
    let vi = f.append_block_param(vector, ty);
    let lanes = i64::from(c.shape.lanes());
    let bytes = c.shape.lane_type().bytes().unwrap();
    let zero = iconst(f, guard, ty, 0);
    let min = iconst(f, guard, ty, lanes * 4);
    let enough = cmp(
        f,
        guard,
        if c.signed { IntCC::Sge } else { IntCC::Uge },
        c.bound,
        min,
    );
    let n64 = if ty == Type::I64 {
        c.bound
    } else {
        emit(
            f,
            guard,
            D::Convert {
                op: ConvertOp::Uext,
                ty: Type::I64,
                arg: c.bound,
            },
        )
    };
    let mut safe = enough;
    let len = if bytes == 1 {
        n64
    } else {
        let shift = iconst(f, guard, Type::I64, i64::from(bytes.trailing_zeros()));
        let len = bin(f, guard, B::Shl, n64, shift);
        let recovered = bin(f, guard, B::Ushr, len, shift);
        let fits = cmp(f, guard, IntCC::Eq, n64, recovered);
        safe = bin(f, guard, B::And, safe, fits);
        len
    };
    let D::Store { addr, .. } = *f.inst_data(c.store) else {
        unreachable!()
    };
    let dst = c.addresses[&f.resolve(addr)];
    let mut bases: Vec<_> = c.addresses.values().copied().collect();
    bases.sort();
    bases.dedup();
    let mut ints = BTreeMap::new();
    for base in bases {
        let p = emit(
            f,
            guard,
            D::Convert {
                op: ConvertOp::PtrToInt,
                ty: Type::I64,
                arg: base,
            },
        );
        let end = bin(f, guard, B::Iadd, p, len);
        let nowrap = cmp(f, guard, IntCC::Uge, end, p);
        safe = bin(f, guard, B::And, safe, nowrap);
        ints.insert(base, p);
    }
    for (&base, &p) in &ints {
        if base == dst {
            continue;
        }
        let d = ints[&dst];
        let same = cmp(f, guard, IntCC::Eq, p, d);
        let delta = bin(f, guard, B::Isub, p, d);
        let reverse = bin(f, guard, B::Isub, d, p);
        let forward = cmp(f, guard, IntCC::Uge, delta, len);
        let backward = cmp(f, guard, IntCC::Uge, reverse, len);
        let disjoint = bin(f, guard, B::And, forward, backward);
        let ok = bin(f, guard, B::Or, same, disjoint);
        safe = bin(f, guard, B::And, safe, ok);
    }
    branch(f, guard, safe, setup, vec![], c.header, vec![zero]);
    let round = iconst(f, setup, ty, i64::from(lanes.trailing_zeros()));
    let groups = bin(f, setup, B::Ushr, c.bound, round);
    let end = bin(f, setup, B::Shl, groups, round);
    let mut mapping = HashMap::from([(c.iv, vi)]);
    // Broadcast invariants once, outside the vector loop. Local literals are
    // cloned here too; the scalar body retains its original definitions.
    let mut values: Vec<_> = c.vectors.iter().copied().collect();
    values.sort();
    for v in values {
        let scalar = match def(f, v) {
            Some(i) if f.inst_block(i) == c.body => match f.inst_data(i) {
                D::Iconst { .. } | D::Fconst { .. } => emit(f, setup, f.inst_data(i).clone()),
                _ => continue,
            },
            _ => v,
        };
        let splat = emit(
            f,
            setup,
            D::VectorSplat {
                ty: Type::Vector(c.shape),
                arg: scalar,
            },
        );
        mapping.insert(v, splat);
    }
    jump(f, setup, vector, vec![zero]);
    // Rebuild addresses from the vector induction, avoiding scalar address
    // conversion/sign-extension subtleties after grouping iterations.
    let index = if ty == Type::I64 {
        vi
    } else {
        emit(
            f,
            vector,
            D::Convert {
                op: ConvertOp::Uext,
                ty: Type::I64,
                arg: vi,
            },
        )
    };
    let offset = if bytes == 1 {
        index
    } else {
        let shift = iconst(f, vector, Type::I64, i64::from(bytes.trailing_zeros()));
        bin(f, vector, B::Shl, index, shift)
    };
    let mut addresses: Vec<_> = c.addresses.iter().map(|(&a, &b)| (a, b)).collect();
    addresses.sort();
    for (a, base) in addresses {
        let ptr = bin(f, vector, B::Ptradd, base, offset);
        mapping.insert(a, ptr);
    }
    for i in f.block_insts(c.body).to_vec() {
        let original = f.inst_data(i).clone();
        let result = f.inst_results(i).first().copied().map(|v| f.resolve(v));
        let data = match original {
            D::Load {
                mut flags, addr, ..
            } if result.is_some_and(|v| c.vectors.contains(&v)) => {
                flags.align = flags.align.min(bytes);
                D::Load {
                    flags,
                    ty: Type::Vector(c.shape),
                    addr: mapping[&f.resolve(addr)],
                }
            }
            D::Binary { op, args } if result.is_some_and(|v| c.vectors.contains(&v)) => {
                D::VectorBinary {
                    op: target.binary(op, c.shape.lane_type()).unwrap(),
                    args: args.map(|v| mapping[&f.resolve(v)]),
                }
            }
            D::Store {
                mut flags,
                value,
                addr,
            } => {
                flags.align = flags.align.min(bytes);
                D::Store {
                    flags,
                    value: mapping[&f.resolve(value)],
                    addr: mapping[&f.resolve(addr)],
                }
            }
            _ => continue,
        };
        let new = f.append_inst(vector, data);
        f.set_inst_loc(new, f.inst_loc(i));
        if let Some(v) = result {
            mapping.insert(v, f.first_result(new));
        }
    }
    let step = iconst(f, vector, ty, lanes);
    let next = bin(f, vector, B::Iadd, vi, step);
    let more = cmp(f, vector, IntCC::Ult, next, end);
    branch(f, vector, more, vector, vec![next], c.header, vec![next]);
    let pre = *f.block_insts(c.pre).last().unwrap();
    *f.inst_data_mut(pre) = D::Jump {
        dest: BlockCall {
            block: guard,
            args: vec![],
        },
    };
}
pub fn run(f: &mut Function, target: VectorTarget) -> bool {
    let headers: Vec<_> = f.blocks().collect();
    let mut changed = false;
    for header in headers {
        if let Some(c) = analyze(f, header, target) {
            transform(f, c, target);
            changed = true;
        }
    }
    changed
}
