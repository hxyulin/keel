//! Conservative fixed-width loop vectorization with a scalar fallback.
//!
//! Recognizes unit-stride, two-block counted loops with invariant starts, one store,
//! a uniform element type and no carried values except the induction variable.
//! The separate loop-reduce passes handle read-only wrapping integer sums.
//! Runtime range checks permit identical or disjoint streams, rejecting partial
//! overlap. The original loop handles short inputs, aliasing, and the tail.
use keel_ir::dominators::DominatorTree;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, Inst, InstData as D, IntCC, Type, Value,
    ValueDef, VectorBinaryOp as V, VectorType,
};
use std::collections::BTreeMap;
mod scan;

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
    fn deterministic(&self) -> bool {
        true
    }
}

pub(crate) struct LoopReduce;
impl crate::Pass for LoopReduce {
    fn name(&self) -> &'static str {
        "loop-reduce"
    }
    fn description(&self) -> &'static str {
        "unrolled read-only wrapping i32/i64 sum loops with independent vector accumulators"
    }
    fn run(&self, f: &mut Function) -> bool {
        self.run_for_target(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        let changed = run_candidates(f, target, true);
        normalize_conditional_loop(f, target) || changed
    }
    fn deterministic(&self) -> bool {
        true
    }
}

/// Opt-in affine stream recognition; deliberately absent from tier pipelines.
pub(crate) struct AffineLoopReduce;
impl crate::Pass for AffineLoopReduce {
    fn name(&self) -> &'static str {
        "loop-reduce-affine"
    }
    fn description(&self) -> &'static str {
        "vectorize integer reductions with guarded invariant stream offsets"
    }
    fn run(&self, f: &mut Function) -> bool {
        self.run_for_target(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        run_extended(f, target, false, false)
    }
}

/// Packed truncate/extend round trips, using only the portable integer ISA.
pub(crate) struct CastLoopReduce;
impl crate::Pass for CastLoopReduce {
    fn name(&self) -> &'static str {
        "loop-reduce-casts"
    }
    fn description(&self) -> &'static str {
        "vectorize integer reduction truncate/extend round trips with packed masks"
    }
    fn run(&self, f: &mut Function) -> bool {
        self.run_for_target(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        run_extended(f, target, true, false)
    }
}

/// Up to four independent read-only recurrences of a common lane width.
pub(crate) struct MultiLoopReduce;
impl crate::Pass for MultiLoopReduce {
    fn name(&self) -> &'static str {
        "loop-reduce-multi"
    }
    fn description(&self) -> &'static str {
        "vectorize bounded same-width independent integer sum recurrences"
    }
    fn run(&self, f: &mut Function) -> bool {
        self.run_for_target(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        run_extended(f, target, true, true)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

pub(crate) struct FastLoopReduce;
impl crate::Pass for FastLoopReduce {
    fn name(&self) -> &'static str {
        "fast-loop-reduce"
    }
    fn description(&self) -> &'static str {
        "bounded small-function single-stream integer sum vectorization"
    }
    fn run(&self, f: &mut Function) -> bool {
        self.run_for_target(f, VectorTarget::Portable)
    }
    fn run_for_target(&self, f: &mut Function, target: VectorTarget) -> bool {
        // Bound eligibility work before constructing CFG/dominators. Large
        // functions stay on Fast's local-only path. These limits apply before
        // any transformation, not to the scalar loop retained after unrolling.
        if f.layout_insts().take(257).count() > 256 || f.blocks().take(17).count() > 16 {
            return false;
        }
        run_candidates_with_budget(f, target, true, true)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

struct Candidate {
    header: Block,
    body: Block,
    pre: Block,
    iv: Value,
    start: Value,
    bound: Value,
    signed: bool,
    output: Output,
    shape: VectorType,
    // Original scalar stream address -> invariant base pointer.
    addresses: FxHashMap<Value, Value>,
    vectors: FxHashSet<Value>,
    offsets: FxHashMap<Value, Value>,
    casts: FxHashMap<Value, NarrowRoundTrip>,
    extra_sums: Vec<Reduction>,
    // All streams use this scalar memory width. Accumulator lanes may be wider.
    stream_bytes: u32,
    widening_inputs: FxHashMap<Value, (Value, ConvertOp)>,
    // Each expression keeps its scalar ring; only explicit extensions widen.
    widths: FxHashMap<Value, VectorType>,
}
struct Reduction {
    accumulator: Value,
    initial: Value,
    update: Value,
    value: Value,
}
#[derive(Clone, Copy)]
struct NarrowRoundTrip {
    input: Value,
    bits: u32,
    signed: bool,
}
#[derive(Clone, Copy)]
enum Output {
    Store(Inst),
    Sum {
        accumulator: Value,
        initial: Value,
        update: Value,
    },
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
    allowed: FxHashSet<Inst>,
    affine: bool,
    casts: bool,
    conditional: bool,
}
impl Analyze<'_> {
    fn index(&mut self, v: Value) -> Option<Option<Value>> {
        let v = self.f.resolve(v);
        if v == self.c.iv {
            return Some(None);
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
                Some(None)
            }
            D::Binary { op: B::Iadd, args } if self.affine && self.f.value_type(v) == Type::I64 => {
                // Only addition at address width: never reassociate a narrow
                // wrapping add through an extension. The guard checks this sum.
                for (index, offset) in [(args[0], args[1]), (args[1], args[0])] {
                    if invariant(self.f, self.dom, self.c.pre, offset)
                        && (self.f.resolve(index) == self.c.iv
                            || matches!(
                                def(self.f, index).map(|i| self.f.inst_data(i)),
                                Some(D::Convert {
                                    op: ConvertOp::Uext | ConvertOp::Sext,
                                    ..
                                })
                            ))
                        && self.index(index)? == None
                    {
                        self.allowed.insert(i);
                        return Some(Some(self.f.resolve(offset)));
                    }
                }
                None
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
        let bytes = self.c.stream_bytes;
        let offset = self.f.resolve(offset);
        let invariant_offset = if bytes == 1 {
            self.index(offset)?
        } else {
            let scale = def(self.f, offset)?;
            let D::Binary { op, args } = *self.f.inst_data(scale) else {
                return None;
            };
            let (index, amount) = match op {
                B::Shl if constant(self.f, args[1]) == Some(i64::from(bytes.trailing_zeros())) => {
                    (args[0], args[1])
                }
                B::Imul if constant(self.f, args[1]) == Some(i64::from(bytes)) => {
                    (args[0], args[1])
                }
                B::Imul if constant(self.f, args[0]) == Some(i64::from(bytes)) => {
                    (args[1], args[0])
                }
                _ => return None,
            };
            let invariant_offset = self.index(index)?;
            self.allowed.insert(scale);
            if let Some(i) = def(self.f, amount) {
                self.allowed.insert(i);
            }
            invariant_offset
        };
        if let Some(offset) = invariant_offset {
            self.c.offsets.insert(v, offset);
        }
        self.allowed.insert(i);
        self.c.addresses.insert(v, self.f.resolve(base));
        Some(())
    }
    fn expression(&mut self, v: Value, depth: usize) -> Option<()> {
        self.expression_width(v, depth, self.c.shape)
    }
    fn expression_width(&mut self, v: Value, depth: usize, shape: VectorType) -> Option<()> {
        let v = self.f.resolve(v);
        if depth > 32 || self.f.value_type(v) != shape.lane_type() {
            return None;
        }
        if matches!(self.c.output, Output::Sum { accumulator, update, .. } if v == accumulator || v == update)
            || self
                .c
                .extra_sums
                .iter()
                .any(|r| v == r.accumulator || v == r.update)
        {
            return None;
        }
        if self.c.vectors.contains(&v) {
            return (self.c.widths.get(&v).copied().unwrap_or(self.c.shape) == shape).then_some(());
        }
        self.c.widths.insert(v, shape);
        if invariant(self.f, self.dom, self.c.pre, v) {
            self.c.vectors.insert(v);
            return Some(());
        }
        let i = def(self.f, v)?;
        if self.f.inst_block(i) != self.c.body {
            return None;
        }
        match *self.f.inst_data(i) {
            D::Load { flags, addr, .. } if !flags.volatile => {
                let bytes = shape.lane_type().bytes()?;
                if !self.c.addresses.is_empty() && self.c.stream_bytes != bytes {
                    return None;
                }
                self.c.stream_bytes = bytes;
                self.address(addr)?;
            }
            D::Binary { op, args } => {
                self.target.binary(op, shape.lane_type())?;
                for arg in args {
                    self.expression_width(arg, depth + 1, shape)?;
                }
            }
            D::Select { args } if self.conditional => {
                let mask = self.f.resolve(args[0]);
                let compare = def(self.f, mask)?;
                let D::Icmp {
                    cond,
                    args: operands,
                } = *self.f.inst_data(compare)
                else {
                    return None;
                };
                let predicate_ty = self.f.value_type(operands[0]);
                let predicate_shape = VectorType::ALL_128
                    .into_iter()
                    .find(|s| s.lane_type() == predicate_ty)?;
                if self.f.inst_block(compare) != self.c.body
                    || !matches!(predicate_ty, Type::I32 | Type::I64)
                    || predicate_shape != shape
                        && !(matches!(
                            self.target,
                            VectorTarget::Aarch64Neon | VectorTarget::X86Sse2
                        ) && predicate_ty == Type::I32
                            && shape.lane_type() == Type::I64)
                    || predicate_ty == Type::I64
                        && !matches!(
                            self.target,
                            VectorTarget::Aarch64Neon | VectorTarget::X86Sse2
                        )
                        && !matches!(cond, IntCC::Eq | IntCC::Ne)
                    || self.target == VectorTarget::Portable
                        && matches!(cond, IntCC::Ult | IntCC::Ule | IntCC::Ugt | IntCC::Uge)
                {
                    return None;
                }
                for operand in operands {
                    self.expression_width(operand, depth + 1, predicate_shape)?;
                }
                for arm in &args[1..] {
                    self.expression_width(*arm, depth + 1, shape)?;
                }
                self.allowed.insert(compare);
                self.c.widths.insert(mask, predicate_shape);
                self.c.vectors.insert(mask);
            }
            D::Convert {
                op: op @ (ConvertOp::Uext | ConvertOp::Sext),
                arg,
                ..
            } if self.conditional
                && matches!(
                    self.target,
                    VectorTarget::Aarch64Neon | VectorTarget::X86Sse2
                )
                && matches!(self.c.output, Output::Sum { .. })
                && !def(self.f, arg).is_some_and(|i| {
                    matches!(
                        self.f.inst_data(i),
                        D::Convert {
                            op: ConvertOp::Trunc,
                            ..
                        }
                    )
                }) =>
            {
                let source = self.f.value_type(arg);
                let source_shape = VectorType::ALL_128
                    .into_iter()
                    .find(|s| s.lane_type() == source)?;
                if !matches!(source, Type::I8 | Type::I16 | Type::I32)
                    || source.bytes()? >= shape.lane_type().bytes()?
                    // Bound arithmetic/select support to i32 -> i64. Existing
                    // smaller direct loads keep their original eligibility.
                    || !(source == Type::I32 && shape.lane_type() == Type::I64)
                        && !def(self.f, arg).is_some_and(|i| matches!(self.f.inst_data(i), D::Load { .. }))
                {
                    return None;
                }
                self.expression_width(arg, depth + 1, source_shape)?;
                self.c.widening_inputs.insert(v, (self.f.resolve(arg), op));
            }
            D::Convert {
                op: ConvertOp::Uext | ConvertOp::Sext,
                arg,
                ..
            } if self.casts && shape == self.c.shape => {
                // Keep lane count and address stride unchanged. A narrow value
                // may only occur as the private intermediate of a round trip.
                let narrow = def(self.f, arg)?;
                let D::Convert {
                    op: ConvertOp::Trunc,
                    arg: input,
                    ty,
                } = *self.f.inst_data(narrow)
                else {
                    return None;
                };
                if self.f.inst_block(narrow) != self.c.body
                    || self.f.value_type(input) != self.c.shape.lane_type()
                    || !matches!(ty, Type::I8 | Type::I16 | Type::I32)
                    || ty.bytes()? >= self.c.shape.lane_type().bytes()?
                {
                    return None;
                }
                self.expression(input, depth + 1)?;
                self.allowed.insert(narrow);
                self.c.casts.insert(
                    v,
                    NarrowRoundTrip {
                        input: self.f.resolve(input),
                        bits: ty.bytes()? * 8,
                        signed: matches!(
                            self.f.inst_data(i),
                            D::Convert {
                                op: ConvertOp::Sext,
                                ..
                            }
                        ),
                    },
                );
            }
            D::Iconst { .. } | D::Fconst { .. } => {}
            _ => return None,
        }
        self.allowed.insert(i);
        self.c.vectors.insert(v);
        Some(())
    }
}
fn analyze(
    f: &Function,
    header: Block,
    target: VectorTarget,
    analysis: &mut Option<std::rc::Rc<crate::analysis::Analysis>>,
    affine: bool,
    casts: bool,
    multi: bool,
    conditional: bool,
) -> Option<Candidate> {
    let params = f.block_params(header);
    if !(1..=if multi { 5 } else { 2 }).contains(&params.len())
        || params.iter().any(|&v| f.resolve(v) != v)
    {
        return None;
    }
    let iv = params[0];
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
    if f.resolve(cond) != f.first_result(cmp) {
        return None;
    }
    let (bound, signed, body_index) = match (
        predicate,
        f.resolve(args[0]) == iv,
        f.resolve(args[1]) == iv,
    ) {
        (IntCC::Ult, true, false) => (args[1], false, 0),
        (IntCC::Ugt, false, true) => (args[0], false, 0),
        (IntCC::Slt, true, false) => (args[1], true, 0),
        (IntCC::Sgt, false, true) => (args[0], true, 0),
        (IntCC::Uge, true, false) => (args[1], false, 1),
        (IntCC::Ule, false, true) => (args[0], false, 1),
        (IntCC::Sge, true, false) => (args[1], true, 1),
        (IntCC::Sle, false, true) => (args[0], true, 1),
        _ => return None,
    };
    let analysis = &**analysis.get_or_insert_with(|| crate::analysis::get(f));
    let (cfg, dom) = (&analysis.cfg, &analysis.dom);
    let body = dests[body_index].block;
    if body == header
        || cfg.preds(body) != [header]
        || !f.block_params(body).is_empty()
        || !dests[body_index].args.is_empty()
    {
        return None;
    }
    let preds = cfg.preds(header);
    if preds.len() != 2 {
        return None;
    }
    let pre = *preds.iter().find(|&&b| b != body)?;
    if pre == header || !dom.dominates(header, body) || !invariant(f, dom, pre, bound) {
        return None;
    }
    let pre_jump = *f.block_insts(pre).last()?;
    let D::Jump { dest } = f.inst_data(pre_jump) else {
        return None;
    };
    if dest.block != header
        || dest.args.len() != params.len()
        || !dest.args.iter().all(|&v| invariant(f, dom, pre, v))
    {
        return None;
    }
    let start = f.resolve(dest.args[0]);
    let initial = dest.args.get(1).copied().map(|v| f.resolve(v));
    let back = *f.block_insts(body).last()?;
    let D::Jump { dest } = f.inst_data(back) else {
        return None;
    };
    if dest.block != header || dest.args.len() != params.len() {
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
    let (output, value, root) = match (stores.as_slice(), params) {
        ([store], [_]) => {
            let D::Store { flags, value, .. } = *f.inst_data(*store) else {
                unreachable!()
            };
            if flags.volatile {
                return None;
            }
            (Output::Store(*store), value, *store)
        }
        ([], [_, accumulator, ..])
            if matches!(f.value_type(*accumulator), Type::I32 | Type::I64) =>
        {
            let update = f.resolve(dest.args[1]);
            let root = def(f, update)?;
            if f.inst_block(root) != body {
                return None;
            }
            let D::Binary { op: B::Iadd, args } = *f.inst_data(root) else {
                return None;
            };
            let value = if f.resolve(args[0]) == *accumulator {
                args[1]
            } else if f.resolve(args[1]) == *accumulator {
                args[0]
            } else {
                return None;
            };
            (
                Output::Sum {
                    accumulator: *accumulator,
                    initial: initial?,
                    update,
                },
                value,
                root,
            )
        }
        _ => return None,
    };
    let mut extra_sums = Vec::new();
    let D::Jump { dest: entry } = f.inst_data(pre_jump) else {
        unreachable!()
    };
    for (index, &accumulator) in params.iter().enumerate().skip(2) {
        if f.value_type(accumulator) != f.value_type(value) {
            return None;
        }
        let update = f.resolve(dest.args[index]);
        let root = def(f, update)?;
        if f.inst_block(root) != body {
            return None;
        }
        let D::Binary { op: B::Iadd, args } = *f.inst_data(root) else {
            return None;
        };
        let value = if f.resolve(args[0]) == accumulator {
            args[1]
        } else if f.resolve(args[1]) == accumulator {
            args[0]
        } else {
            return None;
        };
        extra_sums.push(Reduction {
            accumulator,
            initial: f.resolve(entry.args[index]),
            update,
            value,
        });
    }
    let shape = VectorType::ALL_128
        .into_iter()
        .find(|s| s.lane_type() == f.value_type(value))?;
    let unsigned = |n: i64| {
        if f.value_type(iv) == Type::I32 {
            u64::from(n as u32)
        } else {
            n as u64
        }
    };
    let negative = |n: i64| {
        signed
            && if f.value_type(iv) == Type::I32 {
                (n as i32) < 0
            } else {
                n < 0
            }
    };
    if constant(f, start).is_some_and(negative) || constant(f, bound).is_some_and(negative) {
        return None;
    }
    if let (Some(start), Some(bound)) = (constant(f, start), constant(f, bound)) {
        let (start, bound) = (unsigned(start), unsigned(bound));
        if bound < start || bound - start < u64::from(shape.lanes()) * 4 {
            return None;
        }
    }
    let c = Candidate {
        header,
        body,
        pre,
        iv,
        start,
        bound: f.resolve(bound),
        signed,
        output,
        shape,
        addresses: FxHashMap::default(),
        vectors: FxHashSet::default(),
        offsets: FxHashMap::default(),
        casts: FxHashMap::default(),
        extra_sums,
        stream_bytes: shape.lane_type().bytes()?,
        widening_inputs: FxHashMap::default(),
        widths: FxHashMap::default(),
    };
    let mut a = Analyze {
        f,
        dom,
        c,
        target,
        allowed: FxHashSet::from_iter([back, step, root]),
        affine,
        casts,
        conditional,
    };
    if let Output::Store(store) = output {
        let D::Store { addr, .. } = *f.inst_data(store) else {
            unreachable!()
        };
        a.address(addr)?;
    }
    a.expression(value, 0)?;
    if let Output::Sum { update, .. } = output {
        a.c.vectors.insert(update);
        // Analyze every contribution independently. Any use of a carried value
        // inside a contribution fails expression(), excluding coupled sums.
        for index in 0..a.c.extra_sums.len() {
            let reduction = &a.c.extra_sums[index];
            let (value, update) = (reduction.value, reduction.update);
            a.expression(value, 0)?;
            a.allowed.insert(def(f, update)?);
            a.c.vectors.insert(update);
        }
        // Widening A then B across several iterations could fault at A[1]
        // before the scalar loop's B[0]. With at most one potentially faulting
        // read, and no writes/other traps, lane grouping preserves first-fault
        // order. Distinct streams require explicit nontrapping promises.
        let trapping_reads = a
            .allowed
            .iter()
            .filter(|&&i| matches!(f.inst_data(i), D::Load { flags, .. } if !flags.notrap))
            .count();
        if trapping_reads > 1 {
            return None;
        }
        // No point vectorizing an entirely invariant expression here. Keep
        // this initial reduction support tied to actual contiguous reads.
        if a.c.addresses.is_empty() {
            return None;
        }
    }
    if a.c
        .addresses
        .values()
        .copied()
        .collect::<FxHashSet<_>>()
        .len()
        > 4
        || a.c.vectors.len() > 24
    {
        return None;
    }
    // No hidden side effects, loads, extra recurrences, induction uses as data, or
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
// Source vectors can have more physical lanes than the reduction. Only the
// low reduction-lane count is observed, including after source-width arithmetic.
fn reduction_load(
    f: &mut Function,
    block: Block,
    reduction: VectorType,
    shape: VectorType,
    source: Type,
    mut flags: keel_ir::MemoryFlags,
    addr: Value,
) -> Value {
    flags.align = flags.align.min(source.bytes().unwrap());
    if shape == reduction {
        return emit(
            f,
            block,
            D::Load {
                ty: Type::Vector(shape),
                flags,
                addr,
            },
        );
    }
    let packed_bytes = u32::from(reduction.lanes()) * source.bytes().unwrap();
    let packed_ty = match packed_bytes {
        2 => Type::I16,
        4 => Type::I32,
        8 => Type::I64,
        _ => unreachable!(),
    };
    let bits = emit(
        f,
        block,
        D::Load {
            ty: packed_ty,
            flags,
            addr,
        },
    );
    let packed_shape = VectorType::ALL_128
        .into_iter()
        .find(|s| s.lane_type() == packed_ty)
        .unwrap();
    let packed = emit(
        f,
        block,
        D::VectorSplat {
            ty: Type::Vector(packed_shape),
            arg: bits,
        },
    );
    emit(
        f,
        block,
        D::Convert {
            op: ConvertOp::Bitcast,
            ty: Type::Vector(shape),
            arg: packed,
        },
    )
}
fn select_mask(
    f: &mut Function,
    block: Block,
    c: &Candidate,
    mapping: &FxHashMap<Value, Value>,
    mask: Value,
    shape: VectorType,
) -> Value {
    let mask = f.resolve(mask);
    let value = mapping[&mask];
    if c.widths[&mask] == shape {
        value
    } else {
        // Integer compares produce all-ones/zero masks. Sign extension keeps
        // that canonical representation across the explicit lane boundary.
        emit(
            f,
            block,
            D::Convert {
                op: ConvertOp::SwidenLow,
                ty: Type::Vector(shape),
                arg: value,
            },
        )
    }
}

/// Keep a select in the source ring when both arms are exactly representable
/// by the same extension. In particular, a nested i32 clamp needs only one
/// widening of its result, rather than widening each comparison mask to i64.
/// Inspect generated SSA, so shared arms and their other wide users keep their
/// original values. No arithmetic, predicates, or memory operations move.
fn narrow_select_arm(
    f: &Function,
    value: Value,
    op: ConvertOp,
    constants: &FxHashMap<Value, (i64, Value)>,
) -> Option<Value> {
    let value = f.resolve(value);
    if let Some(&(imm, narrow)) = constants.get(&value) {
        let fits = if op == ConvertOp::SwidenLow {
            i32::try_from(imm).is_ok()
        } else {
            u32::try_from(imm).is_ok()
        };
        return fits.then_some(narrow);
    }
    match *f.inst_data(def(f, value)?) {
        D::Convert {
            op: actual,
            arg,
            ty: Type::Vector(VectorType::I64x2),
        } if actual == op && f.value_type(arg) == Type::Vector(VectorType::I32x4) => Some(arg),
        _ => None,
    }
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

/// Unroll a read-only reduction into independent wrapping recurrences. Stream
/// pointers advance once per group; fixed 16-byte offsets expose load addressing
/// modes without a chain of post-increment loads. A countdown avoids carrying an
/// otherwise unused element induction through the vector body.
fn reduce_body(
    f: &mut Function,
    c: &Candidate,
    target: VectorTarget,
    setup: Block,
    vector: Block,
    remaining: Value,
    count: Value,
) {
    let Output::Sum {
        accumulator,
        initial,
        update,
    } = c.output
    else {
        unreachable!()
    };
    // Bound code growth/register pressure for larger expression DAGs/stream sets.
    let mut reductions = vec![(accumulator, initial, update)];
    reductions.extend(
        c.extra_sums
            .iter()
            .map(|r| (r.accumulator, r.initial, r.update)),
    );
    let unroll = if reductions.len() == 1 && c.vectors.len() <= 8 && c.addresses.len() <= 2 {
        4
    } else {
        2
    };
    let ty = f.value_type(c.iv);
    let group = i64::from(c.shape.lanes()) * unroll;
    let shift = iconst(f, setup, ty, i64::from(group.trailing_zeros()));
    let groups = bin(f, setup, B::Ushr, count, shift);
    let length = bin(f, setup, B::Shl, groups, shift);
    let end = if constant(f, c.start) == Some(0) {
        length
    } else {
        bin(f, setup, B::Iadd, c.start, length)
    };
    let zero = iconst(f, setup, c.shape.lane_type(), 0);
    let zero = emit(
        f,
        setup,
        D::VectorSplat {
            ty: Type::Vector(c.shape),
            arg: zero,
        },
    );
    let sums: Vec<_> = (0..unroll as usize * reductions.len())
        .map(|_| f.append_block_param(vector, Type::Vector(c.shape)))
        .collect();
    let mut invariants = FxHashMap::default();
    let mut narrow_constants = FxHashMap::default();
    let narrow_selects = matches!(target, VectorTarget::Aarch64Neon | VectorTarget::X86Sse2)
        && c.shape == VectorType::I64x2
        && c.widths.values().any(|&s| s == VectorType::I32x4);
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
                ty: Type::Vector(c.widths.get(&v).copied().unwrap_or(c.shape)),
                arg: scalar,
            },
        );
        invariants.insert(v, splat);
        if narrow_selects && f.value_type(scalar) == Type::I64 {
            if let Some(imm) = constant(f, scalar) {
                if i32::try_from(imm).is_ok() || u32::try_from(imm).is_ok() {
                    let bits = iconst(f, setup, Type::I32, imm as i32 as i64);
                    let narrow = emit(
                        f,
                        setup,
                        D::VectorSplat {
                            ty: Type::Vector(VectorType::I32x4),
                            arg: bits,
                        },
                    );
                    narrow_constants.insert(splat, (imm, narrow));
                }
            }
        }
    }
    let mut cast_masks = FxHashMap::default();
    let mut casts: Vec<_> = c.casts.iter().collect();
    casts.sort_by_key(|(v, _)| **v);
    for (&v, cast) in casts {
        let splat = |f: &mut Function, n| {
            let scalar = iconst(f, setup, c.shape.lane_type(), n);
            emit(
                f,
                setup,
                D::VectorSplat {
                    ty: Type::Vector(c.shape),
                    arg: scalar,
                },
            )
        };
        let mask = splat(f, (1i64 << cast.bits) - 1);
        let sign = cast.signed.then(|| splat(f, 1i64 << (cast.bits - 1)));
        cast_masks.insert(v, (mask, sign));
    }
    let mut bases: Vec<_> = c.addresses.values().copied().collect();
    bases.sort();
    bases.dedup();
    let mut pointers = FxHashMap::default();
    let mut args = vec![length];
    args.extend(sums.iter().map(|_| zero));
    for base in bases {
        let pointer = f.append_block_param(vector, Type::Ptr);
        pointers.insert(base, pointer);
        let start = if constant(f, c.start) == Some(0) {
            base
        } else {
            let index = if ty == Type::I64 {
                c.start
            } else {
                emit(
                    f,
                    setup,
                    D::Convert {
                        op: ConvertOp::Uext,
                        ty: Type::I64,
                        arg: c.start,
                    },
                )
            };
            let shift = iconst(
                f,
                setup,
                Type::I64,
                i64::from(c.stream_bytes.trailing_zeros()),
            );
            let offset = bin(f, setup, B::Shl, index, shift);
            bin(f, setup, B::Ptradd, base, offset)
        };
        args.push(start);
    }
    jump(f, setup, vector, args);
    let mut next_sums = Vec::new();
    let mut addresses: Vec<_> = c.addresses.iter().map(|(&a, &b)| (a, b)).collect();
    addresses.sort();
    for chunk in 0..unroll as usize {
        let mut mapping = invariants.clone();
        for (index, &(accumulator, _, _)) in reductions.iter().enumerate() {
            mapping.insert(accumulator, sums[chunk * reductions.len() + index]);
        }
        for &(addr, base) in &addresses {
            let p = pointers[&base];
            let p = if chunk == 0 {
                p
            } else {
                let offset = iconst(
                    f,
                    vector,
                    Type::I64,
                    chunk as i64 * i64::from(c.shape.lanes()) * i64::from(c.stream_bytes),
                );
                bin(f, vector, B::Ptradd, p, offset)
            };
            mapping.insert(addr, p);
        }
        for i in f.block_insts(c.body).to_vec() {
            let result = f.inst_results(i).first().copied().map(|v| f.resolve(v));
            if !result.is_some_and(|v| c.vectors.contains(&v)) {
                continue;
            }
            let shape = result
                .and_then(|v| c.widths.get(&v).copied())
                .unwrap_or(c.shape);
            if let D::Select { args } = *f.inst_data(i) {
                let mask = f.resolve(args[0]);
                if narrow_selects
                    && shape == VectorType::I64x2
                    && c.widths[&mask] == VectorType::I32x4
                {
                    let arms = [mapping[&f.resolve(args[1])], mapping[&f.resolve(args[2])]];
                    let narrowed = [ConvertOp::SwidenLow, ConvertOp::UwidenLow]
                        .into_iter()
                        .find_map(|op| {
                            Some((
                                op,
                                [
                                    narrow_select_arm(f, arms[0], op, &narrow_constants)?,
                                    narrow_select_arm(f, arms[1], op, &narrow_constants)?,
                                ],
                            ))
                        });
                    if let Some((op, arms)) = narrowed {
                        let narrow = f.append_inst(
                            vector,
                            D::VectorSelect {
                                args: [mapping[&mask], arms[0], arms[1]],
                            },
                        );
                        f.set_inst_loc(narrow, f.inst_loc(i));
                        let extended = f.append_inst(
                            vector,
                            D::Convert {
                                op,
                                ty: Type::Vector(shape),
                                arg: f.first_result(narrow),
                            },
                        );
                        f.set_inst_loc(extended, f.inst_loc(i));
                        mapping.insert(result.unwrap(), f.first_result(extended));
                        continue;
                    }
                }
            }
            let data = match *f.inst_data(i) {
                D::Load { flags, addr, ty } => {
                    let loaded = reduction_load(
                        f,
                        vector,
                        c.shape,
                        shape,
                        ty,
                        flags,
                        mapping[&f.resolve(addr)],
                    );
                    mapping.insert(result.unwrap(), loaded);
                    continue;
                }
                D::Binary { op, args } => D::VectorBinary {
                    op: target.binary(op, shape.lane_type()).unwrap(),
                    args: args.map(|v| mapping[&f.resolve(v)]),
                },
                D::Icmp { cond, args } => D::VectorIcmp {
                    cond,
                    args: args.map(|v| mapping[&f.resolve(v)]),
                },
                // Every accepted select predicate is an integer comparison:
                // its vector mask is canonical, so selecting zero needs only
                // a packed AND, with no second mask normalization or blending.
                D::Select { args } if constant(f, args[2]) == Some(0) => D::VectorBinary {
                    op: V::And,
                    args: [
                        select_mask(f, vector, c, &mapping, args[0], shape),
                        mapping[&f.resolve(args[1])],
                    ],
                },
                D::Select { args } => D::VectorSelect {
                    args: [
                        select_mask(f, vector, c, &mapping, args[0], shape),
                        mapping[&f.resolve(args[1])],
                        mapping[&f.resolve(args[2])],
                    ],
                },
                D::Convert { .. } if c.widening_inputs.contains_key(&result.unwrap()) => {
                    let (input, op) = c.widening_inputs[&result.unwrap()];
                    D::Convert {
                        op: if op == ConvertOp::Sext {
                            ConvertOp::SwidenLow
                        } else {
                            ConvertOp::UwidenLow
                        },
                        ty: Type::Vector(shape),
                        arg: mapping[&input],
                    }
                }
                D::Convert { .. } if c.casts.contains_key(&result.unwrap()) => {
                    let v = result.unwrap();
                    let cast = c.casts[&v];
                    let (mask, sign) = cast_masks[&v];
                    let masked = emit(
                        f,
                        vector,
                        D::VectorBinary {
                            op: V::And,
                            args: [mapping[&cast.input], mask],
                        },
                    );
                    // Sign extension in the destination ring:
                    // ((x & low_mask) ^ sign_bit) - sign_bit.
                    // No cross-lane packing or saturating conversion occurs.
                    if let Some(sign) = sign {
                        let biased = emit(
                            f,
                            vector,
                            D::VectorBinary {
                                op: V::Xor,
                                args: [masked, sign],
                            },
                        );
                        D::VectorBinary {
                            op: V::Isub,
                            args: [biased, sign],
                        }
                    } else {
                        mapping.insert(v, masked);
                        continue;
                    }
                }
                _ => continue,
            };
            let new = f.append_inst(vector, data);
            f.set_inst_loc(new, f.inst_loc(i));
            mapping.insert(result.unwrap(), f.first_result(new));
        }
        next_sums.extend(reductions.iter().map(|&(_, _, update)| mapping[&update]));
    }
    let step = iconst(f, vector, ty, group);
    let next = bin(f, vector, B::Isub, remaining, step);
    let mut args = vec![next];
    args.extend_from_slice(&next_sums);
    let mut bases: Vec<_> = pointers.keys().copied().collect();
    bases.sort();
    for base in bases {
        let step = iconst(
            f,
            vector,
            Type::I64,
            unroll * i64::from(c.shape.lanes()) * i64::from(c.stream_bytes),
        );
        args.push(bin(f, vector, B::Ptradd, pointers[&base], step));
    }
    let zero = iconst(f, vector, ty, 0);
    let more = cmp(f, vector, IntCC::Ne, next, zero);
    let finish = f.create_block();
    branch(f, vector, more, vector, args, finish, vec![]);
    let mut tail = vec![end];
    for (index, &(_, initial, _)) in reductions.iter().enumerate() {
        let mut sum = next_sums[index];
        for chunk in 1..unroll as usize {
            sum = emit(
                f,
                finish,
                D::VectorBinary {
                    op: V::Iadd,
                    args: [sum, next_sums[chunk * reductions.len() + index]],
                },
            );
        }
        let lanes = emit(
            f,
            finish,
            D::VectorReduce {
                op: keel_ir::VectorReduceOp::Add,
                arg: sum,
            },
        );
        let scalar = bin(f, finish, B::Iadd, initial, lanes);
        tail.push(scalar);
    }
    jump(f, finish, c.header, tail);
}

fn transform(f: &mut Function, mut c: Candidate, target: VectorTarget) {
    let guard = f.create_block();
    let entry = guard;
    let setup = f.create_block();
    let vector = f.create_block();
    let ty = f.value_type(c.iv);
    let vi = f.append_block_param(vector, ty);
    let lanes = i64::from(c.shape.lanes());
    let bytes = c.stream_bytes;
    let zero = iconst(f, guard, ty, 0);
    let min = iconst(f, guard, ty, lanes * 4);
    let from_zero = constant(f, c.start) == Some(0);
    let count = if from_zero {
        c.bound
    } else {
        bin(f, guard, B::Isub, c.bound, c.start)
    };
    let enough = cmp(
        f,
        guard,
        if c.signed && from_zero {
            IntCC::Sge
        } else {
            IntCC::Uge
        },
        count,
        min,
    );
    let mut safe = enough;
    if !from_zero {
        // Subtraction alone can turn an empty range into a huge unsigned count.
        let ordered = cmp(
            f,
            guard,
            if c.signed { IntCC::Sge } else { IntCC::Uge },
            c.bound,
            c.start,
        );
        safe = bin(f, guard, B::And, safe, ordered);
        if c.signed {
            // Vector address reconstruction zero-extends i32 indices. Negative
            // starts retain the original scalar sign-extension and arithmetic.
            let nonnegative = cmp(f, guard, IntCC::Sge, c.start, zero);
            safe = bin(f, guard, B::And, safe, nonnegative);
        }
    }
    let mut scalar_args = vec![c.start];
    if let Output::Sum { initial, .. } = c.output {
        scalar_args.push(initial);
        scalar_args.extend(c.extra_sums.iter().map(|r| r.initial));
    }
    // Avoid address-overflow checks and vector setup on short/reversed/negative
    // reductions. Keep the established map guard unchanged.
    let reduction = matches!(c.output, Output::Sum { .. });
    let guard = if reduction {
        let range = f.create_block();
        branch(f, guard, safe, range, vec![], c.header, scalar_args.clone());
        range
    } else {
        guard
    };
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
    let len = if bytes == 1 {
        n64
    } else {
        let shift = iconst(f, guard, Type::I64, i64::from(bytes.trailing_zeros()));
        let len = bin(f, guard, B::Shl, n64, shift);
        let recovered = bin(f, guard, B::Ushr, len, shift);
        let fits = cmp(f, guard, IntCC::Eq, n64, recovered);
        safe = if reduction {
            fits
        } else {
            bin(f, guard, B::And, safe, fits)
        };
        len
    };
    // Hoist base + offset*element_size only after checking that the
    // original address-width addition and scaling do not wrap anywhere in
    // the vector range. Rejecting valid wrapping inputs is fine: scalar
    // fallback retains their exact address arithmetic and first fault.
    let mut offsets: Vec<_> = c.offsets.iter().map(|(&a, &o)| (a, o)).collect();
    offsets.sort();
    for (address, offset) in offsets {
        let last = bin(f, guard, B::Iadd, offset, n64);
        let nowrap = cmp(f, guard, IntCC::Uge, last, offset);
        safe = bin(f, guard, B::And, safe, nowrap);
        let shift = iconst(f, guard, Type::I64, i64::from(bytes.trailing_zeros()));
        let scaled_end = bin(f, guard, B::Shl, last, shift);
        let recovered = bin(f, guard, B::Ushr, scaled_end, shift);
        let fits = cmp(f, guard, IntCC::Eq, last, recovered);
        safe = bin(f, guard, B::And, safe, fits);
        let scaled = bin(f, guard, B::Shl, offset, shift);
        let base = c.addresses[&address];
        let p = emit(
            f,
            guard,
            D::Convert {
                op: ConvertOp::PtrToInt,
                ty: Type::I64,
                arg: base,
            },
        );
        let adjusted = bin(f, guard, B::Iadd, p, scaled);
        let nowrap = cmp(f, guard, IntCC::Uge, adjusted, p);
        safe = bin(f, guard, B::And, safe, nowrap);
        let base = bin(f, guard, B::Ptradd, base, scaled);
        c.addresses.insert(address, base);
    }
    let dst = match c.output {
        Output::Store(store) => {
            let D::Store { addr, .. } = *f.inst_data(store) else {
                unreachable!()
            };
            Some(c.addresses[&f.resolve(addr)])
        }
        Output::Sum { .. } => None,
    };
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
        let Some(dst) = dst else {
            break;
        };
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
    branch(f, guard, safe, setup, vec![], c.header, scalar_args);
    if reduction {
        reduce_body(f, &c, target, setup, vector, vi, count);
        let pre = *f.block_insts(c.pre).last().unwrap();
        // The preheader must enter the cheap trip-count guard, not the later
        // pointer-range guard created above.
        *f.inst_data_mut(pre) = D::Jump {
            dest: BlockCall {
                block: entry,
                args: vec![],
            },
        };
        return;
    }
    let round = iconst(f, setup, ty, i64::from(lanes.trailing_zeros()));
    let groups = bin(f, setup, B::Ushr, count, round);
    let length = bin(f, setup, B::Shl, groups, round);
    let end = if from_zero {
        length
    } else {
        bin(f, setup, B::Iadd, c.start, length)
    };
    let mut mapping = FxHashMap::from_iter([(c.iv, vi)]);
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
    jump(f, setup, vector, vec![c.start]);
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
fn run_candidates(f: &mut Function, target: VectorTarget, reductions: bool) -> bool {
    run_candidates_with_budget(f, target, reductions, false)
}
fn run_candidates_with_budget(
    f: &mut Function,
    target: VectorTarget,
    reductions: bool,
    fast: bool,
) -> bool {
    let headers: Vec<_> = f.blocks().collect();
    let mut changed = false;
    // Rejected candidates do not change the function. Share these whole-function
    // analyses until a successful transformation adds blocks and redirects edges.
    let mut analysis = None;
    for header in headers {
        if f.block_params(header).len() != if reductions { 2 } else { 1 } {
            continue;
        }
        if let Some(c) = analyze(
            f,
            header,
            target,
            &mut analysis,
            false,
            false,
            false,
            reductions && !fast,
        ) {
            if fast && (c.addresses.len() != 1 || c.vectors.len() > 8) {
                continue;
            }
            transform(f, c, target);
            analysis = None;
            changed = true;
        }
    }
    changed
}
pub fn run(f: &mut Function, target: VectorTarget) -> bool {
    let changed = run_candidates(f, target, false);
    scan::run(f) || changed
}

fn run_extended(f: &mut Function, target: VectorTarget, casts: bool, multi: bool) -> bool {
    let headers: Vec<_> = f.blocks().collect();
    let mut changed = false;
    let mut analysis = None;
    for header in headers {
        if !(2..=if multi { 5 } else { 2 }).contains(&f.block_params(header).len()) {
            continue;
        }
        if let Some(c) = analyze(f, header, target, &mut analysis, true, casts, multi, true) {
            transform(f, c, target);
            analysis = None;
            changed = true;
        }
    }
    changed
}

/// Whether a reachable block has a successor at or before it in reverse
/// postorder, as every cycle of reachable blocks does.
fn reachable_cycle(f: &Function) -> bool {
    let analysis = crate::analysis::get(f);
    let rpo = analysis.dom.rpo();
    let mut position = vec![usize::MAX; f.num_blocks()];
    for (i, &block) in rpo.iter().enumerate() {
        position[keel_ir::EntityRef::index(block)] = i;
    }
    rpo.iter().enumerate().any(|(i, &block)| {
        analysis
            .cfg
            .succs(block)
            .iter()
            .any(|&s| position[keel_ir::EntityRef::index(s)] <= i)
    })
}

/// C conditional expressions initially have diamond CFGs. Normalize only a
/// bounded private trial and publish it only after a reduction vectorizes.
/// Rejected candidates do not leave extra selects or change unrelated layouts.
fn normalize_conditional_loop(f: &mut Function, target: VectorTarget) -> bool {
    if f.layout_insts().take(257).count() > 256 || f.blocks().take(17).count() > 16 {
        return false;
    }
    if !f
        .blocks()
        .any(|b| Some(b) != f.entry_block() && f.block_params(b).len() >= 2)
    {
        return false;
    }
    // A reduction needs a loop. The trial's cleanup only removes, bypasses
    // or splits edges, which never closes a cycle or reaches unreachable
    // code, so an acyclic function cannot gain one.
    if !reachable_cycle(f) {
        return false;
    }
    let mut trial = f.clone();
    crate::Pass::run(&crate::load_reuse::LoadReuse, &mut trial);
    // A C ternary often skips one arm and enters the join directly. Give
    // that edge an empty block so the existing, guarded diamond converter
    // can validate and normalize it without speculating memory operations.
    for head in trial.blocks().collect::<Vec<_>>() {
        let Some(&term) = trial.block_insts(head).last() else {
            continue;
        };
        let D::Brif { cond, mut dests } = trial.inst_data(term).clone() else {
            continue;
        };
        for direct in 0..2 {
            let arm = dests[1 - direct].block;
            let Some(&last) = trial.block_insts(arm).last() else {
                continue;
            };
            if !matches!(trial.inst_data(last), D::Jump { dest } if dest.block == dests[direct].block)
            {
                continue;
            }
            let bridge = trial.create_block();
            trial.append_inst(
                bridge,
                D::Jump {
                    dest: dests[direct].clone(),
                },
            );
            dests[direct] = BlockCall {
                block: bridge,
                args: vec![],
            };
            *trial.inst_data_mut(term) = D::Brif { cond, dests };
            break;
        }
    }
    if !crate::if_convert::run(&mut trial) {
        return false;
    }
    crate::simplify_cfg::run(&mut trial);
    if !run_candidates(&mut trial, target, true) {
        return false;
    }
    *f = trial;
    true
}
