//! Opt-in, exact-shape adjacent signed-i32 absolute-difference reduction.
//!
//! The first iteration retains the original a[1], a[0] read order. Thereafter
//! ordinary read-only samples can be reused: chunk k reads exactly a[k..k+2]
//! as eight bytes, in ascending stream order. No notrap promise is invented.
//! A nonwrapping byte-range guard retains the scalar loop for wrapping inputs.
//! This deliberately does not relax the generic multi-stream trapping rule.
//!
//! Invariant at the chunk check: 2 <= k <= n; previous lane 1 is a[k-1];
//! the scalar seed contains |a[1]-a[0]|, and the two wrapping vector sums
//! contain exactly differences 2..k. The initial splat of a[1] establishes
//! this without reading a[0] again. The loaded low lanes are [a[k],a[k+1]];
//! shuffle(previous,current,[1,2]) gives [a[k-1],a[k]]. Updating k by two
//! establishes the invariant again. With n-k<2, the optional scalar tail
//! reads only a[k], reusing previous lane 1. n=2 never enters either body.
//!
//! The n>1 guard executes before all loads. The fast-path guard requires
//! n <= UINT64_MAX/4 and base+4*n >= base. Thus every byte in [base,base+4*n)
//! has a nonwrapping address; k+2<=n proves the exact eight-byte load fits.
//! A false guard executes the original scalar loop, including wrapping
//! address/induction behavior. All guard operations themselves are total.
//!
//! Only default ordinary memory loads are accepted, and the complete function
//! has no other memory effects. Previously successful reads can be reused
//! without a write/call/volatile/atomic barrier. After the two prefix reads,
//! newly visited samples are strictly increasing, including within each
//! packed load. The packed load covers exactly two consecutive scalar loads,
//! without an overread. Either earlier prefix fault therefore stays first;
//! subsequent failing bytes belong to the same first newly visited sample.
//! This relies on ordinary read-only memory semantics, not volatile/MMIO or
//! asynchronous mapping mutation. It adds neither alignment nor notrap flags.
//!
//! Sign widening precedes subtraction. Differences lie in [-4294967295,
//! 4294967295], so i64 negation cannot overflow. Summation is in the u64
//! wrapping ring: regrouping into independent lanes preserves the result.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp as C, Function, Inst, InstData as D, IntCC,
    MemoryFlags, Type, UnaryOp, Value, VectorBinaryOp as V, VectorType,
};

const WIDE: Type = Type::Vector(VectorType::I64x2);

/// Root-owned registration can use `run` directly; no pipeline is changed here.
pub fn run(f: &mut Function) -> bool {
    let Some(c) = recognize(f) else { return false };
    transform(f, c);
    true
}

struct Candidate {
    guard: Inst,
    body: Block,
    exit: Block,
    base: Value,
    bound: Value,
    zero: Value,
    one: Value,
    load_loc: Option<keel_ir::SourceLoc>,
    second_loc: Option<keel_ir::SourceLoc>,
    math_loc: Option<keel_ir::SourceLoc>,
}

// Exact instruction/edge matching also excludes all writes, calls, atomics,
// volatile memory, custom operations and extra trapping streams. Definitions
// with aliases are rejected; operands are resolved before comparison.
struct Reader<'a> {
    f: &'a Function,
    insts: &'a [Inst],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(f: &'a Function, b: Block) -> Self {
        Self {
            f,
            insts: f.block_insts(b),
            pos: 0,
        }
    }
    fn next(&mut self) -> Option<(Inst, D)> {
        let i = *self.insts.get(self.pos)?;
        self.pos += 1;
        let mut d = self.f.inst_data(i).clone();
        d.for_each_operand_mut(|v| *v = self.f.resolve(*v));
        Some((i, d))
    }
    fn value(&mut self, expected: D) -> Option<Value> {
        let (i, d) = self.next()?;
        if d != expected {
            return None;
        }
        let [v] = *self.f.inst_results(i) else {
            return None;
        };
        (self.f.resolve(v) == v).then_some(v)
    }
    fn end(&self) -> Option<()> {
        (self.pos == self.insts.len()).then_some(())
    }
}
fn icon(n: i64) -> D {
    D::Iconst {
        ty: Type::I64,
        imm: n,
    }
}
fn bin(op: B, a: Value, b: Value) -> D {
    D::Binary { op, args: [a, b] }
}
fn cmp(cond: IntCC, a: Value, b: Value) -> D {
    D::Icmp { cond, args: [a, b] }
}
fn sext(a: Value) -> D {
    D::Convert {
        op: C::Sext,
        ty: Type::I64,
        arg: a,
    }
}
fn ptr(a: Value, b: Value) -> D {
    bin(B::Ptradd, a, b)
}
fn params(f: &Function, b: Block, types: &[Type]) -> Option<Vec<Value>> {
    let p = f.block_params(b);
    (p.len() == types.len()
        && p.iter()
            .zip(types)
            .all(|(&v, &t)| f.resolve(v) == v && f.value_type(v) == t))
    .then(|| p.to_vec())
}
fn recognize(f: &Function) -> Option<Candidate> {
    if f.blocks().take(6).count() != 5 || f.layout_insts().take(41).count() > 40 {
        return None;
    }
    let entry = f.entry_block()?;
    let mut r = Reader::new(f, entry);
    let zero = r.value(icon(0))?;
    let one = r.value(icon(1))?;
    let (_, D::Jump { dest }) = r.next()? else {
        return None;
    };
    if !dest.args.is_empty() {
        return None;
    }
    let guard_block = dest.block;
    r.end()?;
    params(f, guard_block, &[])?;
    let mut r = Reader::new(f, guard_block);
    let (compare, D::Icmp { cond: IntCC::Ugt, args: [bound, compared_one] }) = r.next()? else {
        return None;
    };
    if compared_one != one || f.value_type(bound) != Type::I64
        || !matches!(f.value_def(bound), keel_ir::ValueDef::Param(b, _) if b == entry) {
        return None;
    }
    let cond = f.first_result(compare);
    if f.resolve(cond) != cond { return None; }
    let (
        guard,
        D::Brif {
            cond: branch_cond,
            dests,
        },
    ) = r.next()?
    else {
        return None;
    };
    if cond != branch_cond || dests[0].args != [one, zero] || dests[1].args != [zero] {
        return None;
    }
    r.end()?;
    let (body, exit) = (dests[0].block, dests[1].block);
    let p = params(f, body, &[Type::I64, Type::I64])?;
    let (iv, sum) = (p[0], p[1]);
    let mut r = Reader::new(f, body);
    let two = r.value(icon(2))?;
    let off = r.value(bin(B::Shl, iv, two))?;
    let (address, D::Binary { op: B::Ptradd, args: [base, offset] }) = r.next()? else {
        return None;
    };
    if offset != off || f.value_type(base) != Type::Ptr
        || !matches!(f.value_def(base), keel_ir::ValueDef::Param(b, _) if b == entry) {
        return None;
    }
    let addr = f.first_result(address);
    if f.resolve(addr) != addr { return None; }
    let load_pos = r.pos;
    let x = r.value(D::Load {
        flags: MemoryFlags::default(),
        ty: Type::I32,
        addr,
    })?;
    let x64 = r.value(sext(x))?;
    let one2 = r.value(icon(1))?;
    let prev = r.value(bin(B::Isub, iv, one2))?;
    let off = r.value(bin(B::Shl, prev, two))?;
    let addr = r.value(ptr(base, off))?;
    let second_pos = r.pos;
    let y = r.value(D::Load {
        flags: MemoryFlags::default(),
        ty: Type::I32,
        addr,
    })?;
    let y64 = r.value(sext(y))?;
    let diff = r.value(bin(B::Isub, x64, y64))?;
    let zero2 = r.value(icon(0))?;
    let negative = r.value(cmp(IntCC::Slt, diff, zero2))?;
    let neg = r.value(D::Unary {
        op: UnaryOp::Ineg,
        arg: diff,
    })?;
    let abs = r.value(D::Select {
        args: [negative, neg, diff],
    })?;
    let (_, D::Jump { dest }) = r.next()? else {
        return None;
    };
    if dest.args != [abs] {
        return None;
    }
    let latch = dest.block;
    r.end()?;
    let p = params(f, latch, &[Type::I64])?;
    let mut r = Reader::new(f, latch);
    let total = r.value(bin(B::Iadd, sum, p[0]))?;
    let one3 = r.value(icon(1))?;
    let next = r.value(bin(B::Iadd, iv, one3))?;
    let cond = r.value(cmp(IntCC::Ugt, bound, next))?;
    let (_, d) = r.next()?;
    if d != (D::Brif {
        cond,
        dests: [call(body, &[next, total]), call(exit, &[total])],
    }) {
        return None;
    }
    r.end()?;
    let p = params(f, exit, &[Type::I64])?;
    let mut r = Reader::new(f, exit);
    let (_, d) = r.next()?;
    if d != (D::Return { args: p }) {
        return None;
    }
    r.end()?;
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
    if !dom.dominates(guard_block, body)
        || !dom.dominates(body, latch)
        || cfg.preds(guard_block) != [entry]
        || cfg.preds(latch) != [body]
        || cfg.preds(body).len() != 2
        || cfg.preds(exit).len() != 2
    {
        return None;
    }
    Some(Candidate {
        guard,
        body,
        exit,
        base,
        bound,
        zero,
        one,
        load_loc: f.inst_loc(f.block_insts(body)[load_pos]),
        second_loc: f.inst_loc(f.block_insts(body)[second_pos]),
        math_loc: f.inst_loc(f.block_insts(body)[11]),
    })
}

fn call(block: Block, args: &[Value]) -> BlockCall {
    BlockCall {
        block,
        args: args.to_vec(),
    }
}
fn emit(f: &mut Function, b: Block, d: D, loc: Option<keel_ir::SourceLoc>) -> Value {
    let i = f.append_inst(b, d);
    f.set_inst_loc(i, loc);
    f.inst_results(i)[0]
}
fn term(f: &mut Function, b: Block, d: D, loc: Option<keel_ir::SourceLoc>) {
    let i = f.append_inst(b, d);
    f.set_inst_loc(i, loc);
}
fn transform(f: &mut Function, c: Candidate) {
    let guard = f.create_block();
    let prefix = f.create_block();
    let check = f.create_block();
    let vector = f.create_block();
    let finish = f.create_block();
    let tail = f.create_block();
    let loc = c.math_loc;
    let e = |f: &mut Function, b, d| emit(f, b, d, loc);
    // n*4 must fit, and base+n*4 must not wrap. All arithmetic is pure;
    // the original n>1 branch remains before this guard and before any load.
    let max = e(f, guard, icon((u64::MAX / 4) as i64));
    let fits = e(f, guard, cmp(IntCC::Ule, c.bound, max));
    let two = e(f, guard, icon(2));
    let bytes = e(f, guard, bin(B::Shl, c.bound, two));
    let base_bits = e(
        f,
        guard,
        D::Convert {
            op: C::PtrToInt,
            ty: Type::I64,
            arg: c.base,
        },
    );
    let end = e(f, guard, bin(B::Iadd, base_bits, bytes));
    let no_wrap = e(f, guard, cmp(IntCC::Uge, end, base_bits));
    let safe = e(f, guard, bin(B::And, fits, no_wrap));
    term(
        f,
        guard,
        D::Brif {
            cond: safe,
            dests: [call(prefix, &[]), call(c.body, &[c.one, c.zero])],
        },
        loc,
    );
    if let D::Brif { dests, .. } = f.inst_data_mut(c.guard) {
        dests[0] = call(guard, &[]);
    }
    let four = e(f, prefix, icon(4));
    let addr = e(f, prefix, ptr(c.base, four));
    let x = emit(
        f,
        prefix,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I32,
            addr,
        },
        c.load_loc,
    );
    let x = e(f, prefix, sext(x));
    let y = emit(
        f,
        prefix,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I32,
            addr: c.base,
        },
        c.second_loc,
    );
    let y = e(f, prefix, sext(y));
    let diff = e(f, prefix, bin(B::Isub, x, y));
    let neg = e(
        f,
        prefix,
        D::Unary {
            op: UnaryOp::Ineg,
            arg: diff,
        },
    );
    let mask = e(f, prefix, cmp(IntCC::Slt, diff, c.zero));
    let sum = e(
        f,
        prefix,
        D::Select {
            args: [mask, neg, diff],
        },
    );
    // Splat(a[1]) seeds lane 1; lane 0 is never used by [1,2].
    let previous = e(f, prefix, D::VectorSplat { ty: WIDE, arg: x });
    let z = e(
        f,
        prefix,
        D::VectorSplat {
            ty: WIDE,
            arg: c.zero,
        },
    );
    let start = e(f, prefix, icon(2));
    term(
        f,
        prefix,
        D::Jump {
            dest: call(check, &[start, previous, z, sum]),
        },
        loc,
    );
    let k = f.append_block_param(check, Type::I64);
    let previous = f.append_block_param(check, WIDE);
    let acc = f.append_block_param(check, WIDE);
    let sum = f.append_block_param(check, Type::I64);
    let remaining = e(f, check, bin(B::Isub, c.bound, k));
    let enough = e(f, check, cmp(IntCC::Uge, remaining, two));
    term(
        f,
        check,
        D::Brif {
            cond: enough,
            dests: [call(vector, &[]), call(finish, &[])],
        },
        loc,
    );
    let offset = e(f, vector, bin(B::Shl, k, two));
    let addr = e(f, vector, ptr(c.base, offset));
    // Exactly 8 bytes, even though the arithmetic registers are 128 bits.
    let bits = emit(
        f,
        vector,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I64,
            addr,
        },
        c.load_loc,
    );
    let packed = e(
        f,
        vector,
        D::VectorSplat {
            ty: WIDE,
            arg: bits,
        },
    );
    let packed = e(
        f,
        vector,
        D::Convert {
            op: C::Bitcast,
            ty: Type::Vector(VectorType::I32x4),
            arg: packed,
        },
    );
    let current = e(
        f,
        vector,
        D::Convert {
            op: C::SwidenLow,
            ty: WIDE,
            arg: packed,
        },
    );
    let prior = e(
        f,
        vector,
        D::VectorShuffle {
            args: [previous, current],
            lanes: vec![1, 2],
        },
    );
    let diff = e(
        f,
        vector,
        D::VectorBinary {
            op: V::Isub,
            args: [current, prior],
        },
    );
    let mask = e(
        f,
        vector,
        D::VectorIcmp {
            cond: IntCC::Slt,
            args: [diff, z],
        },
    );
    let neg = e(
        f,
        vector,
        D::VectorBinary {
            op: V::Isub,
            args: [z, diff],
        },
    );
    let abs = e(
        f,
        vector,
        D::VectorSelect {
            args: [mask, neg, diff],
        },
    );
    let next_acc = e(
        f,
        vector,
        D::VectorBinary {
            op: V::Iadd,
            args: [acc, abs],
        },
    );
    let next = e(f, vector, bin(B::Iadd, k, two));
    term(
        f,
        vector,
        D::Jump {
            dest: call(check, &[next, current, next_acc, sum]),
        },
        loc,
    );
    let lo = e(f, finish, D::VectorExtract { arg: acc, lane: 0 });
    let hi = e(f, finish, D::VectorExtract { arg: acc, lane: 1 });
    let lanes = e(f, finish, bin(B::Iadd, lo, hi));
    let total = e(f, finish, bin(B::Iadd, sum, lanes));
    let has_tail = e(f, finish, cmp(IntCC::Ult, k, c.bound));
    term(
        f,
        finish,
        D::Brif {
            cond: has_tail,
            dests: [call(tail, &[]), call(c.exit, &[total])],
        },
        loc,
    );
    let offset = e(f, tail, bin(B::Shl, k, two));
    let addr = e(f, tail, ptr(c.base, offset));
    let x = emit(
        f,
        tail,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I32,
            addr,
        },
        c.load_loc,
    );
    let x = e(f, tail, sext(x));
    let y = e(
        f,
        tail,
        D::VectorExtract {
            arg: previous,
            lane: 1,
        },
    );
    let diff = e(f, tail, bin(B::Isub, x, y));
    let mask = e(f, tail, cmp(IntCC::Slt, diff, c.zero));
    let neg = e(
        f,
        tail,
        D::Unary {
            op: UnaryOp::Ineg,
            arg: diff,
        },
    );
    let abs = e(
        f,
        tail,
        D::Select {
            args: [mask, neg, diff],
        },
    );
    let total = e(f, tail, bin(B::Iadd, total, abs));
    term(
        f,
        tail,
        D::Jump {
            dest: call(c.exit, &[total]),
        },
        loc,
    );
}
