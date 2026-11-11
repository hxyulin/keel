//! Alias-checked register tiles for complete square floating column reductions.
//! Every lane keeps its original row order; output is never read.
use crate::counted_loop::CountedLoop;
use crate::ir_match::{def, int_const};
use crate::{ModuleContext, OptimizationContext, Pass, PassFamily, VectorTarget};
use keel_ir::fxhash::FxHashSet;
use keel_ir::{
    BinaryOp as B, Block, BlockCall, CallConv, ConvertOp, Function, Inst, InstData as D, IntCC,
    MemoryFlags, Module, Purpose, Signature, Type, Value, VectorBinaryOp, VectorType,
};

const MIN_DIMENSION: i64 = 128;
// Bounds both n*n*8 and all scalar/vector offsets without checked arithmetic.
const MAX_DIMENSION: i64 = 1 << 20;
const FUNCTION_LIMIT: usize = 4;
const INSTRUCTION_LIMIT: usize = 64;

pub(crate) struct LoopColumns;
impl Pass for LoopColumns {
    fn name(&self) -> &'static str {
        "loop-column-reduce"
    }
    fn description(&self) -> &'static str {
        "tile square floating column sums behind range and alias guards"
    }
    fn family(&self) -> PassFamily {
        PassFamily::Vectorize
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, m: &mut Module, cx: &mut ModuleContext<'_>) -> Option<bool> {
        if cx.target == VectorTarget::Portable
            || cx.layout != Some(keel_ir::DataLayout::NATIVE64)
            || !m.assembly.is_empty()
            || m.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), D::InlineAsm { .. }))
            })
        {
            return Some(false);
        }
        let plans: Vec<_> = m
            .functions
            .iter()
            .enumerate()
            .filter_map(|(i, f)| cx.enabled(f).then(|| analyze(f)).flatten().map(|p| (i, p)))
            .take(FUNCTION_LIMIT)
            .collect();
        let mut names: FxHashSet<_> = m
            .functions
            .iter()
            .map(|f| f.name.clone())
            .chain(m.decls.iter().map(|d| d.name.clone()))
            .chain(m.data.iter().map(|d| d.name.clone()))
            .collect();
        for f in &m.functions {
            names.extend(f.callees().map(|(_, d)| d.name.clone()));
            names.extend(f.data_symbols().map(|(_, s)| s.to_owned()));
        }
        let changed = !plans.is_empty();
        cx.changed.resize(m.functions.len(), false);
        for (i, p) in plans {
            let mut suffix = 0;
            let name = loop {
                let s = format!("__keel_columns_{i}_{suffix}");
                if names.insert(s.clone()) {
                    break s;
                }
                suffix += 1;
            };
            let mut helper = recipe(&m.functions[i].sig, &name, &p);
            helper.attrs.optimize = m.functions[i].attrs.optimize.clone();
            helper.attrs.target_features = m.functions[i].attrs.target_features.clone();
            guard(&mut m.functions[i], &helper, &p);
            cx.changed[i] = true;
            m.functions.push(helper);
            cx.changed.push(true);
        }
        Some(changed)
    }
}
struct Plan {
    input: usize,
    output: usize,
    n: usize,
    ty: Type,
    read: MemoryFlags,
    write: MemoryFlags,
}
fn binary(f: &Function, v: Value, op: B, covered: &mut FxHashSet<Inst>) -> Option<[Value; 2]> {
    let i = def(f, v)?;
    let D::Binary { op: actual, args } = *f.inst_data(i) else {
        return None;
    };
    if actual != op {
        return None;
    }
    covered.insert(i);
    Some(args.map(|v| f.resolve(v)))
}
fn pair(args: [Value; 2], a: Value, b: Value) -> bool {
    args == [a, b] || args == [b, a]
}
fn scaled(f: &Function, v: Value, bytes: u32, covered: &mut FxHashSet<Inst>) -> Option<Value> {
    let i = def(f, v)?;
    let value = match *f.inst_data(i) {
        D::Binary { op: B::Shl, args }
            if int_const(f, args[1]) == Some(i64::from(bytes.trailing_zeros())) =>
        {
            args[0]
        }
        D::Binary { op: B::Imul, args } if int_const(f, args[1]) == Some(i64::from(bytes)) => {
            args[0]
        }
        D::Binary { op: B::Imul, args } if int_const(f, args[0]) == Some(i64::from(bytes)) => {
            args[1]
        }
        _ => return None,
    };
    covered.insert(i);
    Some(f.resolve(value))
}
fn analyze(original: &Function) -> Option<Plan> {
    if original.sig.params.len() != 3
        || !original.sig.returns.is_empty()
        || original.sig.variadic
        || original.sig.returns_twice
        || original.sig.x86_vector_abi.is_some()
        || original
            .sig
            .params
            .iter()
            .any(|p| p.purpose != Purpose::Normal)
        || original.attrs.optnone
        || original.attrs.naked
        || original.attrs.interrupt
        || original.personality.is_some()
        || original.debug_loc.is_some()
        || original.debug_return.is_some()
        || !original.debug_vars.is_empty()
        || !original.debug_scopes.is_empty()
        || original.stack_slots().next().is_some()
        || original.layout_insts().take(INSTRUCTION_LIMIT + 1).count() > INSTRUCTION_LIMIT
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
    if f.blocks().count() != 7 {
        return None;
    }
    let entry = f.entry_block()?;
    let params = f.block_params(entry);
    if params.iter().any(|&v| f.resolve(v) != v) {
        return None;
    }
    let n = params.iter().position(|&v| f.value_type(v) == Type::I64)?;
    if params
        .iter()
        .filter(|&&v| f.value_type(v) == Type::Ptr)
        .count()
        != 2
    {
        return None;
    }
    let bound = params[n];
    let mut covered = FxHashSet::default();
    let &initial = f.block_insts(entry).last()?;
    let D::Jump { dest } = f.inst_data(initial) else {
        return None;
    };
    let outer = dest.block;
    let &[column] = f.block_params(outer) else {
        return None;
    };
    if f.value_type(column) != Type::I64
        || dest.args.len() != 1
        || int_const(&f, dest.args[0]) != Some(0)
    {
        return None;
    }
    covered.insert(initial);
    let (literals, &[test, branch]) = f.block_insts(outer).split_last_chunk::<2>()?;
    if !literals
        .iter()
        .all(|&i| matches!(f.inst_data(i), D::Iconst { .. }))
    {
        return None;
    }
    let D::Icmp { cond, args } = *f.inst_data(test) else {
        return None;
    };
    let args = args.map(|v| f.resolve(v));
    let edge = match (cond, args) {
        (IntCC::Ult, [a, b]) if a == column && b == bound => 0,
        (IntCC::Ugt, [a, b]) if b == column && a == bound => 0,
        (IntCC::Uge, [a, b]) if a == column && b == bound => 1,
        (IntCC::Ule, [a, b]) if b == column && a == bound => 1,
        _ => return None,
    };
    let D::Brif { cond, dests } = f.inst_data(branch) else {
        return None;
    };
    if f.resolve(*cond) != f.first_result(test) || dests.iter().any(|d| !d.args.is_empty()) {
        return None;
    }
    let pre = dests[edge].block;
    let exit = dests[1 - edge].block;
    let &jump_inner = f.block_insts(pre).last()?;
    let D::Jump { dest: init } = f.inst_data(jump_inner) else {
        return None;
    };
    let inner = init.block;
    let mut analysis = None;
    let counted = CountedLoop::find(&f, &mut analysis, inner)?;
    if counted.signed || f.resolve(counted.bound) != bound || counted.shape.pre != pre {
        return None;
    }
    let analysis = analysis.as_ref()?;
    let body = counted.shape.body;
    let D::Brif { dests, .. } = f.inst_data(*f.block_insts(inner).last()?) else {
        return None;
    };
    let back = dests[1 - counted.shape.body_edge].block;
    let &[row, sum] = f.block_params(inner) else {
        return None;
    };
    let ty = f.value_type(sum);
    if !matches!(ty, Type::F32 | Type::F64)
        || init.args.len() != 2
        || int_const(&f, init.args[0]) != Some(0)
    {
        return None;
    }
    if !matches!(f.inst_data(def(&f,init.args[1])?),D::Fconst{ty:t,bits:0}if *t==ty) {
        return None;
    }
    let roles = [entry, outer, pre, exit, inner, body, back];
    if roles.into_iter().collect::<FxHashSet<_>>().len() != 7 {
        return None;
    }
    for (b, pred) in [(pre, outer), (exit, outer), (body, inner), (back, inner)] {
        if !f.block_params(b).is_empty() || analysis.cfg.preds(b) != [pred] {
            return None;
        }
    }
    let mut outer_preds = analysis.cfg.preds(outer).to_vec();
    outer_preds.sort();
    let mut expected = vec![entry, back];
    expected.sort();
    if outer_preds != expected {
        return None;
    }
    let D::Jump { dest: inner_back } = f.inst_data(counted.shape.back) else {
        return None;
    };
    let update = def(&f, inner_back.args[1])?;
    let D::Binary {
        op: B::Fadd,
        args: [acc, term],
    } = *f.inst_data(update)
    else {
        return None;
    };
    if f.resolve(acc) != sum || f.inst_block(update) != body {
        return None;
    }
    let load = def(&f, term)?;
    let D::Load {
        ty: load_ty,
        addr,
        flags: read,
    } = *f.inst_data(load)
    else {
        return None;
    };
    if load_ty != ty
        || f.inst_block(load) != body
        || read.volatile
        || !(read.notrap || read.trap_ub)
    {
        return None;
    }
    let bytes = ty.bytes()?;
    let [input, offset] = binary(&f, addr, B::Ptradd, &mut covered)?;
    let index = scaled(&f, offset, bytes, &mut covered)?;
    let indices = binary(&f, index, B::Iadd, &mut covered)?;
    let product = if indices[0] == column {
        indices[1]
    } else if indices[1] == column {
        indices[0]
    } else {
        return None;
    };
    if !pair(binary(&f, product, B::Imul, &mut covered)?, row, bound) {
        return None;
    }
    let stores: Vec<_> = f
        .block_insts(back)
        .iter()
        .copied()
        .filter(|&i| matches!(f.inst_data(i), D::Store { .. }))
        .collect();
    let &[store] = stores.as_slice() else {
        return None;
    };
    let D::Store {
        value,
        addr,
        flags: write,
    } = *f.inst_data(store)
    else {
        return None;
    };
    if f.resolve(value) != sum || write.volatile || !(write.notrap || write.trap_ub) {
        return None;
    }
    let [output, offset] = binary(&f, addr, B::Ptradd, &mut covered)?;
    if scaled(&f, offset, bytes, &mut covered)? != column || input == output {
        return None;
    }
    let input = params
        .iter()
        .position(|&v| v == input && f.value_type(v) == Type::Ptr)?;
    let output = params
        .iter()
        .position(|&v| v == output && f.value_type(v) == Type::Ptr)?;
    let &outer_back = f.block_insts(back).last()?;
    let D::Jump { dest } = f.inst_data(outer_back) else {
        return None;
    };
    if dest.block != outer || dest.args.len() != 1 {
        return None;
    }
    let step = def(&f, dest.args[0])?;
    let step_args = binary(&f, dest.args[0], B::Iadd, &mut covered)?;
    if f.inst_block(step) != back
        || !((step_args[0] == column && int_const(&f, step_args[1]) == Some(1))
            || (step_args[1] == column && int_const(&f, step_args[0]) == Some(1)))
    {
        return None;
    }
    let &ret = f.block_insts(exit).last()?;
    if !matches!(f.inst_data(ret),D::Return{args}if args.is_empty()) {
        return None;
    }
    covered.extend([
        test,
        branch,
        jump_inner,
        counted.shape.back,
        counted.step,
        update,
        load,
        store,
        outer_back,
        ret,
    ]);
    covered.extend(f.block_insts(inner));
    if f.layout_insts().any(|(_, i)| {
        !covered.contains(&i) && !matches!(f.inst_data(i), D::Iconst { .. } | D::Fconst { .. })
    }) {
        return None;
    }
    Some(Plan {
        input,
        output,
        n,
        ty,
        read,
        write,
    })
}
fn emit(f: &mut Function, b: Block, d: D) -> Value {
    let i = f.append_inst(b, d);
    f.first_result(i)
}
fn integer(f: &mut Function, b: Block, n: i64) -> Value {
    emit(
        f,
        b,
        D::Iconst {
            ty: Type::I64,
            imm: n,
        },
    )
}
fn bin(f: &mut Function, b: Block, op: B, a: Value, c: Value) -> Value {
    emit(f, b, D::Binary { op, args: [a, c] })
}
fn cmp(f: &mut Function, b: Block, cond: IntCC, a: Value, c: Value) -> Value {
    emit(f, b, D::Icmp { cond, args: [a, c] })
}
fn edge(b: Block, args: &[Value]) -> BlockCall {
    BlockCall {
        block: b,
        args: args.to_vec(),
    }
}
fn jump(f: &mut Function, b: Block, to: BlockCall) {
    f.append_inst(b, D::Jump { dest: to });
}
fn branch(f: &mut Function, b: Block, c: Value, yes: BlockCall, no: BlockCall) {
    f.append_inst(
        b,
        D::Brif {
            cond: c,
            dests: Box::new([yes, no]),
        },
    );
}
fn address(f: &mut Function, b: Block, base: Value, index: Value, bytes: Value) -> Value {
    let offset = bin(f, b, B::Imul, index, bytes);
    bin(f, b, B::Ptradd, base, offset)
}
fn guard(f: &mut Function, helper: &Function, p: &Plan) {
    let entry = f.entry_block().expect("matched entry");
    let args = f.block_params(entry).to_vec();
    let slow = f.create_block();
    let alias = f.create_block();
    let fast = f.create_block();
    f.move_insts_to_end(entry, 0, slow);
    let min = integer(f, entry, MIN_DIMENSION);
    let max = integer(f, entry, MAX_DIMENSION);
    let lower = cmp(f, entry, IntCC::Uge, args[p.n], min);
    let upper = cmp(f, entry, IntCC::Ule, args[p.n], max);
    let valid = bin(f, entry, B::And, lower, upper);
    branch(f, entry, valid, edge(alias, &[]), edge(slow, &[]));
    let bytes = integer(f, alias, i64::from(p.ty.bytes().expect("float size")));
    let square = bin(f, alias, B::Imul, args[p.n], args[p.n]);
    let length = bin(f, alias, B::Imul, square, bytes);
    let out_length = bin(f, alias, B::Imul, args[p.n], bytes);
    let input = emit(
        f,
        alias,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: args[p.input],
        },
    );
    let output = emit(
        f,
        alias,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: args[p.output],
        },
    );
    let input_end = bin(f, alias, B::Iadd, input, length);
    let output_end = bin(f, alias, B::Iadd, output, out_length);
    let in_wrap = cmp(f, alias, IntCC::Uge, input_end, input);
    let out_wrap = cmp(f, alias, IntCC::Uge, output_end, output);
    let before = cmp(f, alias, IntCC::Ule, input_end, output);
    let after = cmp(f, alias, IntCC::Ule, output_end, input);
    let separate = bin(f, alias, B::Or, before, after);
    let nowrap = bin(f, alias, B::And, in_wrap, out_wrap);
    let valid = bin(f, alias, B::And, nowrap, separate);
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
fn recipe(sig: &Signature, name: &str, p: &Plan) -> Function {
    let mut sig = sig.clone();
    sig.cc = CallConv::Fast;
    let mut f = Function::new(name, sig);
    f.local = true;
    f.attrs.noinline = true;
    let [
        entry,
        tile,
        row,
        body,
        store,
        tail,
        scalar_row,
        scalar_body,
        scalar_store,
        exit,
    ] = std::array::from_fn(|_| f.create_block());
    let args: Vec<_> = f
        .sig
        .params
        .clone()
        .iter()
        .map(|p| f.append_block_param(entry, p.ty))
        .collect();
    let (input, output, n) = (args[p.input], args[p.output], args[p.n]);
    let shape = if p.ty == Type::F32 {
        VectorType::F32x4
    } else {
        VectorType::F64x2
    };
    let vt = Type::Vector(shape);
    let width = i64::from(shape.lanes()) * 4;
    let zero = integer(&mut f, entry, 0);
    let one = integer(&mut f, entry, 1);
    let bytes = integer(&mut f, entry, i64::from(p.ty.bytes().expect("float size")));
    let step = integer(&mut f, entry, width);
    let stride = bin(&mut f, entry, B::Imul, n, bytes);
    let scalar_zero = emit(&mut f, entry, D::Fconst { ty: p.ty, bits: 0 });
    let vector_zero = emit(
        &mut f,
        entry,
        D::VectorSplat {
            ty: vt,
            arg: scalar_zero,
        },
    );
    jump(&mut f, entry, edge(tile, &[zero]));
    let col = f.append_block_param(tile, Type::I64);
    let remaining = bin(&mut f, tile, B::Isub, n, col);
    let enough = cmp(&mut f, tile, IntCC::Uge, remaining, step);
    let start = address(&mut f, tile, input, col, bytes);
    let target = address(&mut f, tile, output, col, bytes);
    branch(
        &mut f,
        tile,
        enough,
        edge(
            row,
            &[
                zero,
                start,
                vector_zero,
                vector_zero,
                vector_zero,
                vector_zero,
            ],
        ),
        edge(tail, &[col]),
    );
    let r = f.append_block_param(row, Type::I64);
    let pointer = f.append_block_param(row, Type::Ptr);
    let sums: [Value; 4] = std::array::from_fn(|_| f.append_block_param(row, vt));
    let more = cmp(&mut f, row, IntCC::Ult, r, n);
    branch(&mut f, row, more, edge(body, &[]), edge(store, &[]));
    let mut totals = Vec::new();
    for (i, sum) in sums.into_iter().enumerate() {
        let delta = integer(&mut f, body, (i * 16) as i64);
        let addr = bin(&mut f, body, B::Ptradd, pointer, delta);
        let load = emit(
            &mut f,
            body,
            D::Load {
                ty: vt,
                addr,
                flags: MemoryFlags {
                    align: p.read.align.min(16),
                    ..p.read
                },
            },
        );
        totals.push(emit(
            &mut f,
            body,
            D::VectorBinary {
                op: VectorBinaryOp::Fadd,
                args: [sum, load],
            },
        ));
    }
    let next_r = bin(&mut f, body, B::Iadd, r, one);
    let next_ptr = bin(&mut f, body, B::Ptradd, pointer, stride);
    let mut back = vec![next_r, next_ptr];
    back.extend(totals);
    jump(&mut f, body, edge(row, &back));
    for (i, sum) in sums.into_iter().enumerate() {
        let delta = integer(&mut f, store, (i * 16) as i64);
        let addr = bin(&mut f, store, B::Ptradd, target, delta);
        f.append_inst(
            store,
            D::Store {
                value: sum,
                addr,
                flags: MemoryFlags {
                    align: p.write.align.min(16),
                    ..p.write
                },
            },
        );
    }
    let next_col = bin(&mut f, store, B::Iadd, col, step);
    jump(&mut f, store, edge(tile, &[next_col]));
    let c = f.append_block_param(tail, Type::I64);
    let more = cmp(&mut f, tail, IntCC::Ult, c, n);
    let start = address(&mut f, tail, input, c, bytes);
    let target = address(&mut f, tail, output, c, bytes);
    branch(
        &mut f,
        tail,
        more,
        edge(scalar_row, &[zero, start, scalar_zero]),
        edge(exit, &[]),
    );
    let r = f.append_block_param(scalar_row, Type::I64);
    let pointer = f.append_block_param(scalar_row, Type::Ptr);
    let sum = f.append_block_param(scalar_row, p.ty);
    let more = cmp(&mut f, scalar_row, IntCC::Ult, r, n);
    branch(
        &mut f,
        scalar_row,
        more,
        edge(scalar_body, &[]),
        edge(scalar_store, &[]),
    );
    let value = emit(
        &mut f,
        scalar_body,
        D::Load {
            ty: p.ty,
            addr: pointer,
            flags: p.read,
        },
    );
    let sum_next = bin(&mut f, scalar_body, B::Fadd, sum, value);
    let next_r = bin(&mut f, scalar_body, B::Iadd, r, one);
    let next_ptr = bin(&mut f, scalar_body, B::Ptradd, pointer, stride);
    jump(
        &mut f,
        scalar_body,
        edge(scalar_row, &[next_r, next_ptr, sum_next]),
    );
    f.append_inst(
        scalar_store,
        D::Store {
            value: sum,
            addr: target,
            flags: p.write,
        },
    );
    let next_col = bin(&mut f, scalar_store, B::Iadd, c, one);
    jump(&mut f, scalar_store, edge(tail, &[next_col]));
    f.append_inst(exit, D::Return { args: vec![] });
    f
}
