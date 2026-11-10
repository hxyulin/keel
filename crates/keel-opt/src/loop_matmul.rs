//! Four-column i32 square matrix tiling behind range and alias guards.
//! The original graph is the fallback. Recognition consumes the complete
//! normalized graph; no unmatched operation is removed or speculated.
use crate::ir_match::{def, int_const};
use crate::{ModuleContext, OptimizationContext, Pass, PassFamily, VectorTarget};
use keel_ir::dominators::DominatorTree;
use keel_ir::fxhash::FxHashSet;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, CallConv, ConvertOp, Function, Inst, InstData as D, IntCC,
    Module, Purpose, Signature, Type, Value, VectorBinaryOp as V, VectorType,
};

const FUNCTION_LIMIT: usize = 4;
const INSTRUCTION_LIMIT: usize = 96;
const DIMENSION_LIMIT: i64 = 46340;

pub(crate) struct LoopMatmul;
impl Pass for LoopMatmul {
    fn name(&self) -> &'static str {
        "loop-matmul"
    }
    fn description(&self) -> &'static str {
        "tile bounded square i32 matrix products with checked alias fallback"
    }
    fn family(&self) -> PassFamily {
        PassFamily::Vectorize
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        if context.target != VectorTarget::Aarch64Neon
            || context.layout != Some(keel_ir::DataLayout::NATIVE64)
            || !module.assembly.is_empty()
        {
            return Some(false);
        }
        let plans: Vec<_> = module
            .functions
            .iter()
            .enumerate()
            .filter_map(|(index, f)| {
                context
                    .enabled(f)
                    .then(|| analyze(f))
                    .flatten()
                    .map(|p| (index, p))
            })
            .take(FUNCTION_LIMIT)
            .collect();
        if plans.is_empty()
            || module.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), D::InlineAsm { .. }))
            })
        {
            return Some(false);
        }
        let mut names: FxHashSet<String> = module
            .functions
            .iter()
            .map(|f| f.name.clone())
            .chain(module.decls.iter().map(|d| d.name.clone()))
            .chain(module.data.iter().map(|d| d.name.clone()))
            .collect();
        for f in &module.functions {
            names.extend(f.callees().map(|(_, d)| d.name.clone()));
            names.extend(f.data_symbols().map(|(_, name)| name.to_owned()));
        }
        context.changed.resize(module.functions.len(), false);
        for (index, plan) in plans {
            let mut suffix = 0;
            let name = loop {
                let name = format!("__keel_matmul_{index}_{suffix}");
                if names.insert(name.clone()) {
                    break name;
                }
                suffix += 1;
            };
            let mut helper = recipe(&module.functions[index].sig, &name, &plan);
            helper.attrs.optimize = module.functions[index].attrs.optimize.clone();
            helper.attrs.target_features = module.functions[index].attrs.target_features.clone();
            guard(&mut module.functions[index], &helper, &plan);
            context.changed[index] = true;
            module.functions.push(helper);
            context.changed.push(true);
        }
        Some(true)
    }
}
struct Plan {
    a: usize,
    b: usize,
    c: usize,
    n: usize,
}
#[derive(Clone)]
struct Header {
    block: Block,
    iv: Value,
    body: Block,
    exit: Block,
    pre: Block,
    back: Block,
    bound: Value,
    seed: Vec<Value>,
    next: Vec<Value>,
    instructions: Vec<Inst>,
}
fn binary(f: &Function, value: Value, op: B) -> Option<(Inst, [Value; 2])> {
    let i = def(f, value)?;
    let D::Binary { op: actual, args } = *f.inst_data(i) else {
        return None;
    };
    (actual == op).then(|| (i, args.map(|v| f.resolve(v))))
}
fn pair(args: [Value; 2], a: Value, b: Value) -> bool {
    args == [a, b] || args == [b, a]
}
fn header(
    f: &Function,
    block: Block,
    dom: &DominatorTree,
    cfg: &keel_ir::dominators::ControlFlowGraph,
) -> Option<Header> {
    let params = f.block_params(block);
    if !(1..=2).contains(&params.len())
        || params.iter().any(|&v| f.resolve(v) != v)
        || f.value_type(params[0]) != Type::I32
    {
        return None;
    }
    let &[test, branch] = f.block_insts(block) else {
        return None;
    };
    let D::Icmp { cond, args } = *f.inst_data(test) else {
        return None;
    };
    let iv = params[0];
    let args = args.map(|v| f.resolve(v));
    let (bound, edge) = match (cond, args) {
        (IntCC::Slt | IntCC::Ult, [a, b]) if a == iv => (b, 0),
        (IntCC::Sge | IntCC::Uge, [a, b]) if a == iv => (b, 1),
        (IntCC::Sgt | IntCC::Ugt, [a, b]) if b == iv => (a, 0),
        (IntCC::Sle | IntCC::Ule, [a, b]) if b == iv => (a, 1),
        _ => return None,
    };
    let D::Brif { cond, dests } = f.inst_data(branch) else {
        return None;
    };
    if f.resolve(*cond) != f.first_result(test) || dests.iter().any(|d| !d.args.is_empty()) {
        return None;
    }
    let preds = cfg.preds(block);
    if preds.len() != 2 {
        return None;
    }
    let back = *preds.iter().find(|&&b| dom.dominates(block, b))?;
    let pre = *preds.iter().find(|&&b| !dom.dominates(block, b))?;
    let back_inst = *f.block_insts(back).last()?;
    let pre_inst = *f.block_insts(pre).last()?;
    let D::Jump { dest: back_edge } = f.inst_data(back_inst) else {
        return None;
    };
    let D::Jump { dest: entry } = f.inst_data(pre_inst) else {
        return None;
    };
    if back_edge.block != block || entry.block != block || int_const(f, entry.args[0]) != Some(0) {
        return None;
    }
    let (step, operands) = binary(f, back_edge.args[0], B::Iadd)?;
    if !((operands[0] == iv && int_const(f, operands[1]) == Some(1))
        || (operands[1] == iv && int_const(f, operands[0]) == Some(1)))
        || f.inst_block(step) != back
    {
        return None;
    }
    Some(Header {
        block,
        iv,
        body: dests[edge].block,
        exit: dests[1 - edge].block,
        pre,
        back,
        bound,
        seed: entry.args.iter().map(|&v| f.resolve(v)).collect(),
        next: back_edge.args.iter().map(|&v| f.resolve(v)).collect(),
        instructions: vec![test, branch, back_inst, pre_inst, step],
    })
}
fn address(
    f: &Function,
    value: Value,
    row: Value,
    col: Value,
    n: Value,
    covered: &mut FxHashSet<Inst>,
) -> Option<Value> {
    let (pointer, [base, offset]) = binary(f, value, B::Ptradd)?;
    let scale = def(f, offset)?;
    let D::Binary { op, args } = *f.inst_data(scale) else {
        return None;
    };
    let wide = match op {
        B::Imul if int_const(f, args[1]) == Some(4) => args[0],
        B::Imul if int_const(f, args[0]) == Some(4) => args[1],
        B::Shl if int_const(f, args[1]) == Some(2) => args[0],
        _ => return None,
    };
    let extend = def(f, wide)?;
    let D::Convert {
        op: ConvertOp::Sext | ConvertOp::Uext,
        ty: Type::I64,
        arg,
    } = *f.inst_data(extend)
    else {
        return None;
    };
    if f.value_type(arg) != Type::I32 {
        return None;
    }
    let (add, terms) = binary(f, arg, B::Iadd)?;
    for (scaled, column) in [(terms[0], terms[1]), (terms[1], terms[0])] {
        if column != col {
            continue;
        }
        if let Some((multiply, operands)) = binary(f, scaled, B::Imul)
            && pair(operands, row, n)
        {
            covered.extend([pointer, scale, extend, add, multiply]);
            return Some(base);
        }
    }
    None
}
fn analyze(original: &Function) -> Option<Plan> {
    if original.sig.params.len() != 4
        || !original.sig.returns.is_empty()
        || original.sig.variadic
        || original.sig.returns_twice
        || original.sig.x86_vector_abi.is_some()
        || original.attrs.optnone
        || original.attrs.naked
        || original.attrs.interrupt
        || original.stack_slots().next().is_some()
        || original
            .sig
            .params
            .iter()
            .any(|p| p.purpose != Purpose::Normal)
        || original
            .sig
            .params
            .iter()
            .filter(|p| p.ty == Type::Ptr)
            .count()
            != 3
        || original
            .sig
            .params
            .iter()
            .filter(|p| p.ty == Type::I32)
            .count()
            != 1
        || original.layout_insts().take(INSTRUCTION_LIMIT + 1).count() > INSTRUCTION_LIMIT
        || original.blocks().take(17).count() > 16
        || original
            .layout_insts()
            .any(|(_, i)| original.inst_loc(i).is_some())
    {
        return None;
    }
    let mut f = original.clone();
    crate::simplify_cfg::run(&mut f);
    crate::phi_simplify::run(&mut f);
    crate::dce::run(&mut f);
    crate::canonicalize::run(&mut f);
    let entry = f.entry_block()?;
    let params = f.block_params(entry);
    if params.iter().any(|&v| f.resolve(v) != v) {
        return None;
    }
    let analysis = crate::analysis::get(&f);
    let mut headers: Vec<_> = f
        .blocks()
        .filter_map(|b| header(&f, b, &analysis.dom, &analysis.cfg))
        .collect();
    let outer = headers.iter().position(|h| h.pre == entry)?;
    let outer = headers.remove(outer);
    let middle = headers.iter().position(|h| h.pre == outer.body)?;
    let middle = headers.remove(middle);
    let inner = headers.iter().find(|h| h.pre == middle.body)?;
    if !headers.iter().all(|h| h.block == inner.block)
        || f.block_params(outer.block).len() != 1
        || f.block_params(middle.block).len() != 1
        || f.block_params(inner.block).len() != 2
        || inner.body != inner.back
        || middle.exit != outer.back
        || inner.exit != middle.back
        || outer.bound != middle.bound
        || outer.bound != inner.bound
        || int_const(&f, inner.seed[1]) != Some(0)
    {
        return None;
    }
    let n = params
        .iter()
        .position(|&v| v == outer.bound && f.value_type(v) == Type::I32)?;
    let roles = [
        entry,
        outer.block,
        outer.body,
        outer.exit,
        outer.back,
        middle.block,
        middle.body,
        middle.back,
        inner.block,
        inner.body,
    ];
    if roles.into_iter().collect::<FxHashSet<_>>().len() != 10 || f.blocks().count() != 10 {
        return None;
    }
    for (block, predecessor) in [
        (outer.body, outer.block),
        (outer.exit, outer.block),
        (outer.back, middle.block),
        (middle.body, middle.block),
        (middle.back, inner.block),
        (inner.body, inner.block),
    ] {
        if !f.block_params(block).is_empty() || analysis.cfg.preds(block) != [predecessor] {
            return None;
        }
    }
    let mut covered: FxHashSet<_> = outer
        .instructions
        .iter()
        .chain(&middle.instructions)
        .chain(&inner.instructions)
        .copied()
        .collect();
    let sum = f.block_params(inner.block)[1];
    if f.value_type(sum) != Type::I32 {
        return None;
    }
    let (update, operands) = binary(&f, inner.next[1], B::Iadd)?;
    let term = if operands[0] == sum {
        operands[1]
    } else if operands[1] == sum {
        operands[0]
    } else {
        return None;
    };
    let (multiply, inputs) = binary(&f, term, B::Imul)?;
    let [left, right] = inputs.map(|v| def(&f, v));
    let (left, right) = (left?, right?);
    let load = |i| match *f.inst_data(i) {
        D::Load {
            ty: Type::I32,
            addr,
            flags,
        } if !flags.volatile
            && (flags.notrap || flags.trap_ub)
            && f.inst_block(i) == inner.body =>
        {
            Some(addr)
        }
        _ => None,
    };
    let (left_addr, right_addr) = (load(left)?, load(right)?);
    let read_roles = [(left_addr, right_addr), (right_addr, left_addr)]
        .into_iter()
        .find_map(|(a, b)| {
            let mut tentative = covered.clone();
            let a = address(&f, a, outer.iv, inner.iv, outer.bound, &mut tentative)?;
            let b = address(&f, b, inner.iv, middle.iv, outer.bound, &mut tentative)?;
            covered = tentative;
            Some((a, b))
        })?;
    let store = f
        .block_insts(inner.exit)
        .iter()
        .copied()
        .find(|&i| matches!(f.inst_data(i), D::Store { .. }))?;
    let D::Store { value, addr, flags } = *f.inst_data(store) else {
        unreachable!()
    };
    if f.resolve(value) != sum || flags.volatile || !(flags.notrap || flags.trap_ub) {
        return None;
    }
    let output = address(&f, addr, outer.iv, middle.iv, outer.bound, &mut covered)?;
    if output == read_roles.0 || output == read_roles.1 {
        return None;
    }
    let a = params
        .iter()
        .position(|&v| v == read_roles.0 && f.value_type(v) == Type::Ptr)?;
    let b = params
        .iter()
        .position(|&v| v == read_roles.1 && f.value_type(v) == Type::Ptr)?;
    let c = params
        .iter()
        .position(|&v| v == output && f.value_type(v) == Type::Ptr)?;
    covered.extend([update, multiply, left, right, store]);
    let ret = *f.block_insts(outer.exit).last()?;
    if !matches!(f.inst_data(ret), D::Return { args } if args.is_empty()) {
        return None;
    }
    covered.insert(ret);
    if f.layout_insts().any(|(_, i)| {
        !covered.contains(&i)
            && !matches!(
                f.inst_data(i),
                D::Iconst {
                    ty: Type::I32 | Type::I64,
                    ..
                }
            )
    }) {
        return None;
    }
    Some(Plan { a, b, c, n })
}

fn emit(f: &mut Function, block: Block, data: D) -> Value {
    let inst = f.append_inst(block, data);
    f.first_result(inst)
}
fn constant(f: &mut Function, block: Block, ty: Type, value: i64) -> Value {
    emit(f, block, D::Iconst { ty, imm: value })
}
fn bin(f: &mut Function, block: Block, op: B, a: Value, b: Value) -> Value {
    emit(f, block, D::Binary { op, args: [a, b] })
}
fn cmp(f: &mut Function, block: Block, cond: IntCC, a: Value, b: Value) -> Value {
    emit(f, block, D::Icmp { cond, args: [a, b] })
}
fn edge(block: Block, args: &[Value]) -> BlockCall {
    BlockCall {
        block,
        args: args.to_vec(),
    }
}
fn jump(f: &mut Function, block: Block, dest: BlockCall) {
    f.append_inst(block, D::Jump { dest });
}
fn branch(f: &mut Function, block: Block, cond: Value, yes: BlockCall, no: BlockCall) {
    f.append_inst(
        block,
        D::Brif {
            cond,
            dests: Box::new([yes, no]),
        },
    );
}
fn params<const N: usize>(f: &mut Function, block: Block, types: [Type; N]) -> [Value; N] {
    types.map(|ty| f.append_block_param(block, ty))
}
fn recipe(sig: &Signature, name: &str, plan: &Plan) -> Function {
    let mut sig = sig.clone();
    sig.cc = CallConv::Fast;
    for param in &mut sig.params {
        param.ext = keel_ir::ArgExt::None;
    }
    let mut f = Function::new(name, sig);
    f.local = true;
    f.attrs.noinline = true;
    let [
        entry,
        setup,
        row,
        start_col,
        col,
        vector_k,
        vector_body,
        vector_store,
        tail,
        scalar_k,
        scalar_body,
        scalar_store,
        next_row,
        exit,
    ] = std::array::from_fn(|_| f.create_block());
    let args: Vec<_> = f
        .sig
        .params
        .clone()
        .into_iter()
        .map(|p| f.append_block_param(entry, p.ty))
        .collect();
    let (a, b, c, n) = (args[plan.a], args[plan.b], args[plan.c], args[plan.n]);
    let zero32 = constant(&mut f, entry, Type::I32, 0);
    let positive = cmp(&mut f, entry, IntCC::Sgt, n, zero32);
    branch(&mut f, entry, positive, edge(setup, &[]), edge(exit, &[]));
    let n64 = emit(
        &mut f,
        setup,
        D::Convert {
            op: ConvertOp::Uext,
            ty: Type::I64,
            arg: n,
        },
    );
    let two = constant(&mut f, setup, Type::I64, 2);
    let stride = bin(&mut f, setup, B::Shl, n64, two);
    let four = constant(&mut f, setup, Type::I64, 4);
    let sixteen = constant(&mut f, setup, Type::I64, 16);
    let one = constant(&mut f, setup, Type::I64, 1);
    let zero = constant(&mut f, setup, Type::I64, 0);
    let zero_vec = emit(
        &mut f,
        setup,
        D::VectorSplat {
            ty: Type::Vector(VectorType::I32x4),
            arg: zero32,
        },
    );
    jump(&mut f, setup, edge(row, &[a, c, n64]));
    let [a_row, c_row, rows] = params(&mut f, row, [Type::Ptr, Type::Ptr, Type::I64]);
    let more = cmp(&mut f, row, IntCC::Ne, rows, zero);
    branch(&mut f, row, more, edge(start_col, &[]), edge(exit, &[]));
    jump(&mut f, start_col, edge(col, &[zero, b, c_row]));
    let [j, b_col, c_col] = params(&mut f, col, [Type::I64, Type::Ptr, Type::Ptr]);
    let remaining = bin(&mut f, col, B::Isub, n64, j);
    let full = cmp(&mut f, col, IntCC::Uge, remaining, four);
    branch(
        &mut f,
        col,
        full,
        edge(vector_k, &[n64, a_row, b_col, zero_vec]),
        edge(tail, &[j, b_col, c_col]),
    );
    let [left, ap, bp, acc] = params(
        &mut f,
        vector_k,
        [
            Type::I64,
            Type::Ptr,
            Type::Ptr,
            Type::Vector(VectorType::I32x4),
        ],
    );
    let more = cmp(&mut f, vector_k, IntCC::Ne, left, zero);
    branch(
        &mut f,
        vector_k,
        more,
        edge(vector_body, &[]),
        edge(vector_store, &[]),
    );
    let flags = keel_ir::MemoryFlags {
        trap_ub: true,
        ..Default::default()
    };
    let av = emit(
        &mut f,
        vector_body,
        D::Load {
            ty: Type::I32,
            addr: ap,
            flags,
        },
    );
    let bv = emit(
        &mut f,
        vector_body,
        D::Load {
            ty: Type::Vector(VectorType::I32x4),
            addr: bp,
            flags,
        },
    );
    let av = emit(
        &mut f,
        vector_body,
        D::VectorSplat {
            ty: Type::Vector(VectorType::I32x4),
            arg: av,
        },
    );
    let product = emit(
        &mut f,
        vector_body,
        D::VectorBinary {
            op: V::Imul,
            args: [av, bv],
        },
    );
    let acc_next = emit(
        &mut f,
        vector_body,
        D::VectorBinary {
            op: V::Iadd,
            args: [acc, product],
        },
    );
    let ap_next = bin(&mut f, vector_body, B::Ptradd, ap, four);
    let bp_next = bin(&mut f, vector_body, B::Ptradd, bp, stride);
    let left_next = bin(&mut f, vector_body, B::Isub, left, one);
    jump(
        &mut f,
        vector_body,
        edge(vector_k, &[left_next, ap_next, bp_next, acc_next]),
    );
    f.append_inst(
        vector_store,
        D::Store {
            value: acc,
            addr: c_col,
            flags,
        },
    );
    let j_next = bin(&mut f, vector_store, B::Iadd, j, four);
    let b_next = bin(&mut f, vector_store, B::Ptradd, b_col, sixteen);
    let c_next = bin(&mut f, vector_store, B::Ptradd, c_col, sixteen);
    jump(&mut f, vector_store, edge(col, &[j_next, b_next, c_next]));
    let [j, b_col, c_col] = params(&mut f, tail, [Type::I64, Type::Ptr, Type::Ptr]);
    let more = cmp(&mut f, tail, IntCC::Ult, j, n64);
    branch(
        &mut f,
        tail,
        more,
        edge(scalar_k, &[n64, a_row, b_col, zero32]),
        edge(next_row, &[]),
    );
    let [left, ap, bp, acc] = params(
        &mut f,
        scalar_k,
        [Type::I64, Type::Ptr, Type::Ptr, Type::I32],
    );
    let more = cmp(&mut f, scalar_k, IntCC::Ne, left, zero);
    branch(
        &mut f,
        scalar_k,
        more,
        edge(scalar_body, &[]),
        edge(scalar_store, &[]),
    );
    let av = emit(
        &mut f,
        scalar_body,
        D::Load {
            ty: Type::I32,
            addr: ap,
            flags,
        },
    );
    let bv = emit(
        &mut f,
        scalar_body,
        D::Load {
            ty: Type::I32,
            addr: bp,
            flags,
        },
    );
    let product = bin(&mut f, scalar_body, B::Imul, av, bv);
    let acc_next = bin(&mut f, scalar_body, B::Iadd, acc, product);
    let ap_next = bin(&mut f, scalar_body, B::Ptradd, ap, four);
    let bp_next = bin(&mut f, scalar_body, B::Ptradd, bp, stride);
    let left_next = bin(&mut f, scalar_body, B::Isub, left, one);
    jump(
        &mut f,
        scalar_body,
        edge(scalar_k, &[left_next, ap_next, bp_next, acc_next]),
    );
    f.append_inst(
        scalar_store,
        D::Store {
            value: acc,
            addr: c_col,
            flags,
        },
    );
    let j_next = bin(&mut f, scalar_store, B::Iadd, j, one);
    let b_next = bin(&mut f, scalar_store, B::Ptradd, b_col, four);
    let c_next = bin(&mut f, scalar_store, B::Ptradd, c_col, four);
    jump(&mut f, scalar_store, edge(tail, &[j_next, b_next, c_next]));
    let a_next = bin(&mut f, next_row, B::Ptradd, a_row, stride);
    let c_next = bin(&mut f, next_row, B::Ptradd, c_row, stride);
    let rows_next = bin(&mut f, next_row, B::Isub, rows, one);
    jump(&mut f, next_row, edge(row, &[a_next, c_next, rows_next]));
    f.append_inst(exit, D::Return { args: vec![] });
    f
}

fn guard(f: &mut Function, helper: &Function, plan: &Plan) {
    let entry = f.entry_block().expect("recognized entry");
    let args = f.block_params(entry).to_vec();
    let slow = f.create_block();
    let alias = f.create_block();
    let fast = f.create_block();
    // Keep every original instruction and edge on the fallback path.
    f.move_insts_to_end(entry, 0, slow);
    let min = constant(f, entry, Type::I32, 4);
    let max = constant(f, entry, Type::I32, DIMENSION_LIMIT);
    let lower = cmp(f, entry, IntCC::Sge, args[plan.n], min);
    let upper = cmp(f, entry, IntCC::Sle, args[plan.n], max);
    let in_range = bin(f, entry, B::And, lower, upper);
    branch(f, entry, in_range, edge(alias, &[]), edge(slow, &[]));
    let n = emit(
        f,
        alias,
        D::Convert {
            op: ConvertOp::Uext,
            ty: Type::I64,
            arg: args[plan.n],
        },
    );
    let square = bin(f, alias, B::Imul, n, n);
    let two = constant(f, alias, Type::I64, 2);
    let bytes = bin(f, alias, B::Shl, square, two);
    let bases = [plan.a, plan.b, plan.c].map(|i| {
        emit(
            f,
            alias,
            D::Convert {
                op: ConvertOp::PtrToInt,
                ty: Type::I64,
                arg: args[i],
            },
        )
    });
    let ends = bases.map(|base| bin(f, alias, B::Iadd, base, bytes));
    let nowrap = std::array::from_fn::<_, 3, _>(|i| cmp(f, alias, IntCC::Uge, ends[i], bases[i]));
    let separate = [0, 1].map(|i| {
        let before = cmp(f, alias, IntCC::Ule, ends[i], bases[2]);
        let after = cmp(f, alias, IntCC::Ule, ends[2], bases[i]);
        bin(f, alias, B::Or, before, after)
    });
    let valid = nowrap
        .into_iter()
        .chain(separate)
        .reduce(|a, b| bin(f, alias, B::And, a, b))
        .expect("three ranges");
    branch(f, alias, valid, edge(fast, &[]), edge(slow, &[]));
    let callee = f.import_function(&helper.name, helper.sig.clone());
    f.set_callee_attrs(callee, helper.attrs.clone());
    f.append_inst(
        fast,
        D::Call {
            func: callee,
            args: args.into(),
        },
    );
    f.append_inst(fast, D::Return { args: vec![] });
}
