//! Opt-in, exact-shape adjacent signed-i32 absolute-difference reduction.
//!
//! The first iteration retains the original a[1], a[0] read order. Thereafter
//! ordinary read-only samples can be reused: chunk k reads exactly a[k..k+2]
//! as eight bytes, in ascending stream order. No notrap promise is invented.
//! A nonwrapping byte-range guard retains the scalar loop for wrapping inputs.
//! This deliberately does not relax the generic multi-stream trapping rule.
//!
//! At each SIMD check, remain is in [0,n-2], ptr=base+4*(n-remain),
//! and previous lane 1 is a[n-remain-1]. The scalar seed is |a[1]-a[0]|;
//! the wrapping lane sums cover differences 2..n-remain (four independent
//! accumulators in main, one merged accumulator in pair cleanup).
//! Prefix initializes remain=n-2 and ptr=base+8, after reading a[1], a[0].
//! A main trip requires remain>=8, reads exact ascending eight-byte ranges
//! ptr+[0,8), [8,16), [16,24), [24,32), then subtracts 8 and advances 32.
//! Pair cleanup requires remain>=2, reads ptr+[0,8), subtracts 2, advances 8.
//! Both updates preserve the invariant without oversubtraction; consecutive
//! trips neither overlap nor skip samples. The four sums merge exactly once.
//! At exit remain<2; remain==1 reads one signed i32 at ptr, sign-extends it,
//! and subtracts the signed i64 history. n=2 never enters either body.
//!
//! The original n>1 and nonwrapping guards dominate prefix and pointer setup:
//! n<=UINT64_MAX/4 and base+4*n>=base. Every pointer update is at most
//! base+4*n, including the one-past pointer when remain becomes zero; thus
//! neither setup, offsets nor updates introduce wrap. Every guarded packed
//! load ends at or before base+4*n. A false guard executes the original scalar
//! loop, retaining its wrapping address/induction behavior. Guards are total.
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
    if !(4..=5).contains(&f.blocks().take(6).count()) || f.layout_insts().take(41).count() > 40 {
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
    let (
        compare,
        D::Icmp {
            cond: IntCC::Ugt,
            args: [bound, compared_one],
        },
    ) = r.next()?
    else {
        return None;
    };
    if compared_one != one
        || f.value_type(bound) != Type::I64
        || !matches!(f.value_def(bound), keel_ir::ValueDef::Param(b, _) if b == entry)
    {
        return None;
    }
    let cond = f.first_result(compare);
    if f.resolve(cond) != cond {
        return None;
    }
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
    let (
        address,
        D::Binary {
            op: B::Ptradd,
            args: [base, offset],
        },
    ) = r.next()?
    else {
        return None;
    };
    if offset != off
        || f.value_type(base) != Type::Ptr
        || !matches!(f.value_def(base), keel_ir::ValueDef::Param(b, _) if b == entry)
    {
        return None;
    }
    let addr = f.first_result(address);
    if f.resolve(addr) != addr {
        return None;
    }
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
    let (update, data) = r.next()?;
    let (latch, total) = match data {
        D::Jump { dest } if dest.args == [abs] => {
            let latch = dest.block;
            r.end()?;
            let p = params(f, latch, &[Type::I64])?;
            r = Reader::new(f, latch);
            (latch, r.value(bin(B::Iadd, sum, p[0]))?)
        }
        D::Binary { op: B::Iadd, args } if args == [sum, abs] => {
            let total = f.first_result(update);
            if f.resolve(total) != total {
                return None;
            }
            (body, total)
        }
        _ => return None,
    };
    let one3 = if matches!(
        f.inst_data(*r.insts.get(r.pos)?),
        D::Iconst {
            ty: Type::I64,
            imm: 1
        }
    ) {
        r.value(icon(1))?
    } else {
        one2
    };
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
        || (latch != body && cfg.preds(latch) != [body])
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
// Keep each exact load/splat/bitcast/widen chain adjacent for packed-low selection.
fn chunk(
    f: &mut Function,
    block: Block,
    addr: Value,
    previous: Value,
    acc: Value,
    z: Value,
    c: &Candidate,
) -> (Value, Value) {
    let e = |f: &mut Function, b, d| emit(f, b, d, c.math_loc);
    // Exactly 8 bytes, even though the arithmetic registers are 128 bits.
    let bits = emit(
        f,
        block,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I64,
            addr,
        },
        c.load_loc,
    );
    let packed = e(
        f,
        block,
        D::VectorSplat {
            ty: WIDE,
            arg: bits,
        },
    );
    let packed = e(
        f,
        block,
        D::Convert {
            op: C::Bitcast,
            ty: Type::Vector(VectorType::I32x4),
            arg: packed,
        },
    );
    let current = e(
        f,
        block,
        D::Convert {
            op: C::SwidenLow,
            ty: WIDE,
            arg: packed,
        },
    );
    let prior = e(
        f,
        block,
        D::VectorShuffle {
            args: [previous, current],
            lanes: vec![1, 2],
        },
    );
    let diff = e(
        f,
        block,
        D::VectorBinary {
            op: V::Isub,
            args: [current, prior],
        },
    );
    let mask = e(
        f,
        block,
        D::VectorIcmp {
            cond: IntCC::Slt,
            args: [diff, z],
        },
    );
    let neg = e(
        f,
        block,
        D::VectorUnary {
            op: UnaryOp::Ineg,
            arg: diff,
        },
    );
    let abs = e(
        f,
        block,
        D::VectorSelect {
            args: [mask, neg, diff],
        },
    );
    let next_acc = e(
        f,
        block,
        D::VectorBinary {
            op: V::Iadd,
            args: [acc, abs],
        },
    );
    (current, next_acc)
}

fn transform(f: &mut Function, c: Candidate) {
    let main_check = f.create_block();
    let main = f.create_block();
    let merge = f.create_block();
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
    let eight = e(f, prefix, icon(8));
    let start_ptr = e(f, prefix, ptr(c.base, eight));
    let start = e(f, prefix, bin(B::Isub, c.bound, two));
    term(
        f,
        prefix,
        D::Jump {
            dest: call(main_check, &[start_ptr, start, previous, z, z, z, z, sum]),
        },
        loc,
    );
    // Count and pointer are carried together; only a successful check admits
    // subtraction or pointer advancement, including the one-past endpoint.
    let address = f.append_block_param(main_check, Type::Ptr);
    let remaining = f.append_block_param(main_check, Type::I64);
    let previous = f.append_block_param(main_check, WIDE);
    let accs: Vec<_> = (0..4)
        .map(|_| f.append_block_param(main_check, WIDE))
        .collect();
    let sum = f.append_block_param(main_check, Type::I64);
    let enough = e(f, main_check, cmp(IntCC::Uge, remaining, eight));
    term(
        f,
        main_check,
        D::Brif {
            cond: enough,
            dests: [call(main, &[]), call(merge, &[])],
        },
        loc,
    );
    let mut current = previous;
    let mut next_accs = Vec::with_capacity(4);
    for (j, acc) in accs.iter().copied().enumerate() {
        let addr = if j == 0 {
            address
        } else {
            let delta = e(f, main, icon(8 * j as i64));
            e(f, main, ptr(address, delta))
        };
        let (loaded, next_acc) = chunk(f, main, addr, current, acc, z, &c);
        current = loaded;
        next_accs.push(next_acc);
    }
    let next = e(f, main, bin(B::Isub, remaining, eight));
    let stride = e(f, main, icon(32));
    let next_ptr = e(f, main, ptr(address, stride));
    term(
        f,
        main,
        D::Jump {
            dest: call(
                main_check,
                &[
                    next_ptr,
                    next,
                    current,
                    next_accs[0],
                    next_accs[1],
                    next_accs[2],
                    next_accs[3],
                    sum,
                ],
            ),
        },
        loc,
    );
    // Merge once modulo 2^64, including a zero-trip main loop, then reuse
    // the original pair cleanup and scalar tail with their original invariants.
    let mut combined = accs[0];
    for &acc in &accs[1..] {
        combined = e(
            f,
            merge,
            D::VectorBinary {
                op: V::Iadd,
                args: [combined, acc],
            },
        );
    }
    term(
        f,
        merge,
        D::Jump {
            dest: call(check, &[address, remaining, previous, combined, sum]),
        },
        loc,
    );
    let address = f.append_block_param(check, Type::Ptr);
    let remaining = f.append_block_param(check, Type::I64);
    let previous = f.append_block_param(check, WIDE);
    let acc = f.append_block_param(check, WIDE);
    let sum = f.append_block_param(check, Type::I64);
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
    let (current, next_acc) = chunk(f, vector, address, previous, acc, z, &c);
    let next = e(f, vector, bin(B::Isub, remaining, two));
    let next_ptr = e(f, vector, ptr(address, eight));
    term(
        f,
        vector,
        D::Jump {
            dest: call(check, &[next_ptr, next, current, next_acc, sum]),
        },
        loc,
    );
    let lo = e(f, finish, D::VectorExtract { arg: acc, lane: 0 });
    let hi = e(f, finish, D::VectorExtract { arg: acc, lane: 1 });
    let lanes = e(f, finish, bin(B::Iadd, lo, hi));
    let total = e(f, finish, bin(B::Iadd, sum, lanes));
    let has_tail = e(f, finish, cmp(IntCC::Eq, remaining, c.one));
    term(
        f,
        finish,
        D::Brif {
            cond: has_tail,
            dests: [call(tail, &[]), call(c.exit, &[total])],
        },
        loc,
    );
    let x = emit(
        f,
        tail,
        D::Load {
            flags: MemoryFlags::default(),
            ty: Type::I32,
            addr: address,
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
