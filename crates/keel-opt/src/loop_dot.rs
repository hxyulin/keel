//! Floating dot products: ordered scalar folds or explicitly relaxed SIMD sums.
//!
//! Read-only, zero-based unsigned i64 loops over contiguous streams qualify.
//! Fast math also admits a single squared stream and squared differences.
//! Reordered loads must be nonvolatile and either notrap or trap_ub. Strict
//! batches retain scalar addition order; relaxed batches use four independent
//! vector accumulators. Original loops handle short ranges, wrap and tails.
use crate::counted_loop::{CountedLoop, invariant};
use crate::ir_match::{def, int_const};
use keel_ir::fxhash::FxHashSet;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, Inst, InstData as D, IntCC, MemoryFlags,
    Type, Value, VectorBinaryOp, VectorType,
};

pub(crate) struct FastLoopDot;
impl crate::Pass for FastLoopDot {
    fn name(&self) -> &'static str {
        "fast-loop-dot"
    }
    fn description(&self) -> &'static str {
        "vectorize floating products, squares and squared differences in fast_math functions"
    }
    fn family(&self) -> crate::PassFamily {
        crate::PassFamily::Vectorize
    }
    fn deterministic(&self) -> bool {
        true
    }
    fn run(&self, f: &mut Function, context: &crate::OptimizationContext) -> bool {
        f.attrs.fast_math && LoopDot.run(f, context)
    }
}
pub(crate) struct LoopDot;
impl crate::Pass for LoopDot {
    fn name(&self) -> &'static str {
        "loop-dot"
    }
    fn description(&self) -> &'static str {
        "batch floating dot products while preserving scalar addition order"
    }
    fn family(&self) -> crate::PassFamily {
        crate::PassFamily::Vectorize
    }
    fn deterministic(&self) -> bool {
        true
    }
    fn run(&self, f: &mut Function, context: &crate::OptimizationContext) -> bool {
        if context.layout != Some(keel_ir::DataLayout::NATIVE64)
            || context.target == crate::VectorTarget::Portable
            || f.debug_return.is_some()
            || f.debug_loc.is_some()
            || !f.debug_vars.is_empty()
            || !f.debug_scopes.is_empty()
        {
            return false;
        }
        let mut changed = false;
        for header in f.blocks().collect::<Vec<_>>() {
            if let Some(c) = analyze(f, header) {
                transform(f, c);
                changed = true;
            }
        }
        changed
    }
}
struct Stream {
    base: Value,
    flags: MemoryFlags,
}
#[derive(Clone, Copy)]
enum Product {
    Multiply,
    Square,
    SquaredDifference,
}
struct Candidate {
    header: Block,
    entry: Inst,
    bound: Value,
    zero: Value,
    seed: Value,
    shape: VectorType,
    streams: Vec<Stream>,
    product: Product,
}
fn analyze(f: &Function, header: Block) -> Option<Candidate> {
    let &[iv, sum] = f.block_params(header) else {
        return None;
    };
    let shape = match f.value_type(sum) {
        Type::F32 => VectorType::F32x4,
        Type::F64 => VectorType::F64x2,
        _ => return None,
    };
    if f.value_type(iv) != Type::I64 || f.resolve(sum) != sum {
        return None;
    }
    let mut analysis = None;
    let counted = CountedLoop::find(f, &mut analysis, header)?;
    let shape_loop = &counted.shape;
    if counted.signed {
        return None;
    }
    let dom = &analysis.as_ref()?.dom;
    let bound = f.resolve(counted.bound);
    if !invariant(f, dom, shape_loop.pre, bound) {
        return None;
    }
    let D::Jump { dest: initial } = f.inst_data(shape_loop.entry) else {
        return None;
    };
    if int_const(f, initial.args[0]) != Some(0)
        || !invariant(f, dom, shape_loop.pre, initial.args[1])
    {
        return None;
    }
    let D::Jump { dest: back } = f.inst_data(shape_loop.back) else {
        return None;
    };
    let update = def(f, back.args[1])?;
    let D::Binary {
        op: B::Fadd,
        args: [acc, contribution],
    } = *f.inst_data(update)
    else {
        return None;
    };
    // Preserve operand order, including the behavior of NaN operands.
    if f.resolve(acc) != sum || f.inst_block(update) != shape_loop.body {
        return None;
    }
    let product = def(f, contribution)?;
    let D::Binary {
        op: B::Fmul,
        args: factors,
    } = *f.inst_data(product)
    else {
        return None;
    };
    if f.inst_block(product) != shape_loop.body {
        return None;
    }
    let mut allowed = FxHashSet::from_iter([update, product, counted.step, shape_loop.back]);
    let (product_kind, factors) = if f.resolve(factors[0]) == f.resolve(factors[1]) {
        let square = f.resolve(factors[0]);
        let difference = def(f, square)?;
        if f.attrs.fast_math
            && let D::Binary { op: B::Fsub, args } = *f.inst_data(difference)
        {
            if f.inst_block(difference) != shape_loop.body {
                return None;
            }
            allowed.insert(difference);
            (Product::SquaredDifference, args.to_vec())
        } else {
            (Product::Square, vec![square])
        }
    } else {
        (Product::Multiply, factors.to_vec())
    };
    let bytes = shape.lane_type().bytes()?;
    let mut streams = Vec::new();
    for factor in factors {
        let load = def(f, factor)?;
        let D::Load { ty, addr, flags } = *f.inst_data(load) else {
            return None;
        };
        if ty != shape.lane_type()
            || flags.volatile
            || !(flags.notrap || flags.trap_ub)
            || f.inst_block(load) != shape_loop.body
        {
            return None;
        }
        let address = def(f, addr)?;
        let D::Binary {
            op: B::Ptradd,
            args: [base, offset],
        } = *f.inst_data(address)
        else {
            return None;
        };
        if !invariant(f, dom, shape_loop.pre, base) || f.inst_block(address) != shape_loop.body {
            return None;
        }
        let scale = def(f, offset)?;
        let D::Binary { op, args } = *f.inst_data(scale) else {
            return None;
        };
        if f.inst_block(scale) != shape_loop.body || f.value_type(offset) != Type::I64 {
            return None;
        }
        let scaled = match op {
            B::Shl => {
                f.resolve(args[0]) == iv
                    && int_const(f, args[1]) == Some(i64::from(bytes.trailing_zeros()))
            }
            B::Imul => {
                (f.resolve(args[0]) == iv && int_const(f, args[1]) == Some(i64::from(bytes)))
                    || (f.resolve(args[1]) == iv && int_const(f, args[0]) == Some(i64::from(bytes)))
            }
            _ => false,
        };
        if !scaled {
            return None;
        }
        allowed.extend([load, address, scale]);
        streams.push(Stream {
            base: f.resolve(base),
            flags,
        });
    }
    if f.block_insts(header)
        .iter()
        .any(|&i| f.inst_loc(i).is_some())
    {
        return None;
    }
    let body = f.block_insts(shape_loop.body);
    // No unrelated work can disappear with a batched iteration. Alias definitions
    // are rejected rather than accidentally resurrected when checking escapes.
    for &inst in body {
        if !allowed.contains(&inst) && !matches!(f.inst_data(inst), D::Iconst { .. }) {
            return None;
        }
        if f.inst_loc(inst).is_some() || f.inst_results(inst).iter().any(|&v| f.resolve(v) != v) {
            return None;
        }
    }
    let local: FxHashSet<_> = body
        .iter()
        .flat_map(|&i| f.inst_results(i).iter().copied())
        .collect();
    for (block, inst) in f.layout_insts() {
        if block != shape_loop.body {
            let mut escapes = false;
            f.inst_data(inst)
                .for_each_operand(|v| escapes |= local.contains(&f.resolve(v)));
            if escapes {
                return None;
            }
        }
    }
    Some(Candidate {
        header,
        entry: shape_loop.entry,
        bound,
        zero: f.resolve(initial.args[0]),
        seed: f.resolve(initial.args[1]),
        shape,
        streams,
        product: product_kind,
    })
}
fn emit(f: &mut Function, block: Block, data: D) -> Value {
    let i = f.append_inst(block, data);
    f.first_result(i)
}
fn bin(f: &mut Function, block: Block, op: B, a: Value, b: Value) -> Value {
    emit(f, block, D::Binary { op, args: [a, b] })
}
fn integer(f: &mut Function, block: Block, n: i64) -> Value {
    emit(f, block, D::iconst(Type::I64, n))
}
fn cmp(f: &mut Function, block: Block, cond: IntCC, a: Value, b: Value) -> Value {
    emit(f, block, D::Icmp { cond, args: [a, b] })
}
fn transform(f: &mut Function, c: Candidate) {
    let relaxed = f.attrs.fast_math;
    // Four independent recurrences amortize loop control without exhausting
    // the baseline register banks. Strict left folds retain eight elements.
    let group_size: i64 = if relaxed {
        i64::from(c.shape.lanes()) * 4
    } else {
        8
    };
    let gate = f.create_block();
    let guard = f.create_block();
    let body = f.create_block();
    let groups_left = f.append_block_param(body, Type::I64);
    let sum = f.append_block_param(body, c.shape.lane_type());
    let pointers: Vec<_> = c
        .streams
        .iter()
        .map(|_| f.append_block_param(body, Type::Ptr))
        .collect();
    let accumulators: Vec<_> = if relaxed {
        (0..group_size / i64::from(c.shape.lanes()))
            .map(|_| f.append_block_param(body, Type::Vector(c.shape)))
            .collect()
    } else {
        Vec::new()
    };
    let bytes = c
        .shape
        .lane_type()
        .bytes()
        .expect("floating lane has a size");
    let group = integer(f, gate, group_size);
    let enough = cmp(f, gate, IntCC::Uge, c.bound, group);
    f.append_inst(
        gate,
        D::Brif {
            cond: enough,
            dests: Box::new([
                BlockCall {
                    block: guard,
                    args: vec![],
                },
                BlockCall {
                    block: c.header,
                    args: vec![c.zero, c.seed],
                },
            ]),
        },
    );
    let shift = integer(f, guard, i64::from(bytes.trailing_zeros()));
    let length = bin(f, guard, B::Shl, c.bound, shift);
    let recovered = bin(f, guard, B::Ushr, length, shift);
    let mut safe = cmp(f, guard, IntCC::Eq, recovered, c.bound);
    // A vector load may not span a modular pointer wrap. The scalar fallback
    // retains the original address arithmetic on ranges that fail this guard.
    for stream in &c.streams {
        let pointer = emit(
            f,
            guard,
            D::Convert {
                op: ConvertOp::PtrToInt,
                ty: Type::I64,
                arg: stream.base,
            },
        );
        let end = bin(f, guard, B::Iadd, pointer, length);
        let nowrap = cmp(f, guard, IntCC::Uge, end, pointer);
        safe = bin(f, guard, B::And, safe, nowrap);
    }
    let group_shift = integer(f, guard, i64::from(group_size.trailing_zeros()));
    let groups = bin(f, guard, B::Ushr, c.bound, group_shift);
    let end = bin(f, guard, B::Shl, groups, group_shift);
    let mut initial = vec![groups, c.seed];
    initial.extend(c.streams.iter().map(|stream| stream.base));
    if relaxed {
        let zero = emit(
            f,
            guard,
            D::Fconst {
                ty: c.shape.lane_type(),
                bits: 0,
            },
        );
        let zero = emit(
            f,
            guard,
            D::VectorSplat {
                ty: Type::Vector(c.shape),
                arg: zero,
            },
        );
        initial.extend(accumulators.iter().map(|_| zero));
    }
    f.append_inst(
        guard,
        D::Brif {
            cond: safe,
            dests: Box::new([
                BlockCall {
                    block: body,
                    args: initial,
                },
                BlockCall {
                    block: c.header,
                    args: vec![c.zero, c.seed],
                },
            ]),
        },
    );
    let mut products = Vec::new();
    for chunk in 0..group_size / i64::from(c.shape.lanes()) {
        let mut inputs = Vec::new();
        for (index, stream) in c.streams.iter().enumerate() {
            let addr = if chunk == 0 {
                pointers[index]
            } else {
                let delta = integer(f, body, chunk * 16);
                bin(f, body, B::Ptradd, pointers[index], delta)
            };
            let flags = MemoryFlags {
                align: stream.flags.align.min(16),
                ..stream.flags
            };
            inputs.push(emit(
                f,
                body,
                D::Load {
                    ty: Type::Vector(c.shape),
                    addr,
                    flags,
                },
            ));
        }
        let factors = match c.product {
            Product::Multiply => [inputs[0], inputs[1]],
            Product::Square => [inputs[0], inputs[0]],
            Product::SquaredDifference => {
                let difference = emit(
                    f,
                    body,
                    D::VectorBinary {
                        op: VectorBinaryOp::Fsub,
                        args: [inputs[0], inputs[1]],
                    },
                );
                [difference, difference]
            }
        };
        products.push(emit(
            f,
            body,
            D::VectorBinary {
                op: VectorBinaryOp::Fmul,
                args: factors,
            },
        ));
    }
    let mut total = sum;
    let mut next_accumulators = Vec::new();
    if relaxed {
        for (acc, product) in accumulators.iter().zip(products) {
            next_accumulators.push(emit(
                f,
                body,
                D::VectorBinary {
                    op: VectorBinaryOp::Fadd,
                    args: [*acc, product],
                },
            ));
        }
    } else {
        for product in products {
            for lane in 0..c.shape.lanes() {
                let value = emit(f, body, D::VectorExtract { arg: product, lane });
                total = bin(f, body, B::Fadd, total, value);
            }
        }
    }
    let stride = integer(f, body, group_size * i64::from(bytes));
    let next_pointers: Vec<_> = pointers
        .iter()
        .map(|&p| bin(f, body, B::Ptradd, p, stride))
        .collect();
    let one = integer(f, body, 1);
    let next_groups = bin(f, body, B::Isub, groups_left, one);
    let mut back_args = vec![next_groups, total];
    back_args.extend(next_pointers);
    let exit = if relaxed {
        let reduce = f.create_block();
        let mut vectors = next_accumulators.iter().copied();
        let mut packed = vectors
            .next()
            .expect("eight-element group has vector accumulators");
        for other in vectors {
            packed = emit(
                f,
                reduce,
                D::VectorBinary {
                    op: VectorBinaryOp::Fadd,
                    args: [packed, other],
                },
            );
        }
        for lane in 0..c.shape.lanes() {
            let value = emit(f, reduce, D::VectorExtract { arg: packed, lane });
            total = bin(f, reduce, B::Fadd, total, value);
        }
        f.append_inst(
            reduce,
            D::Jump {
                dest: BlockCall {
                    block: c.header,
                    args: vec![end, total],
                },
            },
        );
        back_args.extend(next_accumulators);
        BlockCall {
            block: reduce,
            args: vec![],
        }
    } else {
        BlockCall {
            block: c.header,
            args: vec![end, total],
        }
    };
    f.append_inst(
        body,
        D::Brif {
            cond: next_groups,
            dests: Box::new([
                BlockCall {
                    block: body,
                    args: back_args,
                },
                exit,
            ]),
        },
    );
    *f.inst_data_mut(c.entry) = D::Jump {
        dest: BlockCall {
            block: gate,
            args: vec![],
        },
    };
}
