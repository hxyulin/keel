//! Fully unroll tiny literal-trip loops without changing scalar operation order.
//! Both counted header/body loops and rotated one-block loops are supported.
//! Every load stays in its original iteration; no recurrence is reassociated.
use crate::counted_loop::CountedLoop;
use crate::ir_match::{def, int_bits, int_const};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp as B, Block, BlockCall, ConvertOp, Function, InsertPoint, Inst, InstData as D, IntCC,
    Type, UnaryOp, Value,
};

const MAX_TRIPS: usize = 8;
const MAX_BODY: usize = 24;
const MAX_CLONED: usize = 128;
const FUNCTION_BUDGET: usize = 512;

pub(crate) struct LoopUnrollExact;
impl crate::Pass for LoopUnrollExact {
    fn name(&self) -> &'static str {
        "loop-unroll-exact"
    }
    fn description(&self) -> &'static str {
        "fully unroll tiny literal-trip loops in scalar order"
    }
    fn family(&self) -> crate::PassFamily {
        crate::PassFamily::Unroll
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        run(f)
    }
    fn deterministic(&self) -> bool {
        true
    }
}

struct Candidate {
    header: Block,
    body: Option<Block>,
    entry: Inst,
    initial: Vec<Value>,
    next: Vec<Value>,
    exit: BlockCall,
    trips: usize,
    cost: usize,
}

fn value(f: &Function, v: Value, signed: bool) -> Option<i128> {
    if signed {
        int_const(f, v).map(i128::from)
    } else {
        int_bits(f, v).map(|n| n as i128)
    }
}
fn trips(f: &Function, start: Value, bound: Value, signed: bool, rotated: bool) -> Option<usize> {
    let ty = f.value_type(start);
    if !matches!(ty, Type::I32 | Type::I64) || f.value_type(bound) != ty {
        return None;
    }
    let (start, bound) = (value(f, start, signed)?, value(f, bound, signed)?);
    let n = (bound - start).max(if rotated { 1 } else { 0 });
    if n > MAX_TRIPS as i128 {
        return None;
    }
    let bits = ty.bytes()? * 8;
    let max = (1i128 << (bits - u32::from(signed))) - 1;
    // Prove the final induction update does not wrap in its comparison domain.
    if start + n > max {
        return None;
    }
    Some(n as usize)
}
fn analyze(f: &Function, header: Block) -> Option<Candidate> {
    if f.block_params(header).is_empty() || f.block_params(header).len() > 4 {
        return None;
    }
    let analysis = crate::analysis::get(f);
    let term = *f.block_insts(header).last()?;
    let D::Brif { cond, dests } = f.inst_data(term) else {
        return None;
    };
    let mut c = if dests.iter().any(|d| d.block == header) {
        let back = dests.iter().position(|d| d.block == header)?;
        if back != 0 || dests[1].block == header {
            return None;
        }
        let preds = analysis.cfg.preds(header);
        if preds.len() != 2 || !preds.contains(&header) {
            return None;
        }
        let pre = *preds.iter().find(|&&p| p != header)?;
        if !analysis.dom.dominates(pre, header) {
            return None;
        }
        let entry = *f.block_insts(pre).last()?;
        let D::Jump { dest } = f.inst_data(entry) else {
            return None;
        };
        if dest.block != header {
            return None;
        }
        let iv = f.block_params(header)[0];
        let next = f.resolve(*dests[0].args.first()?);
        let D::Binary { op: B::Iadd, args } = *f.inst_data(def(f, next)?) else {
            return None;
        };
        let step = if f.resolve(args[0]) == iv {
            args[1]
        } else if f.resolve(args[1]) == iv {
            args[0]
        } else {
            return None;
        };
        if int_const(f, step) != Some(1) {
            return None;
        }
        let D::Icmp { cond: cc, args } = *f.inst_data(def(f, *cond)?) else {
            return None;
        };
        let (bound, signed) = match (cc, args.map(|v| f.resolve(v))) {
            (IntCC::Slt, [a, b]) if a == next => (b, true),
            (IntCC::Ult, [a, b]) if a == next => (b, false),
            (IntCC::Sgt, [a, b]) if b == next => (a, true),
            (IntCC::Ugt, [a, b]) if b == next => (a, false),
            _ => return None,
        };
        Candidate {
            header,
            body: None,
            entry,
            initial: dest.args.clone(),
            next: dests[0].args.clone(),
            exit: dests[1].clone(),
            trips: trips(f, dest.args[0], bound, signed, true)?,
            cost: 0,
        }
    } else {
        let mut cached = Some(analysis);
        let counted = CountedLoop::find(f, &mut cached, header)?;
        let D::Jump { dest } = f.inst_data(counted.shape.entry) else {
            return None;
        };
        let D::Jump { dest: back } = f.inst_data(counted.shape.back) else {
            return None;
        };
        Candidate {
            header,
            body: Some(counted.shape.body),
            entry: counted.shape.entry,
            initial: dest.args.clone(),
            next: back.args.clone(),
            exit: dests[1 - counted.shape.body_edge].clone(),
            trips: trips(f, dest.args[0], counted.bound, counted.signed, false)?,
            cost: 0,
        }
    };
    let blocks: Vec<_> = [Some(header), c.body].into_iter().flatten().collect();
    let params = f.block_params(header);
    if params.iter().any(|&v| f.resolve(v) != v) {
        return None;
    }
    let mut definitions: FxHashSet<_> = params.iter().copied().collect();
    let mut body_len = 0;
    for &b in &blocks {
        let insts = f.block_insts(b);
        for &i in &insts[..insts.len() - 1] {
            if f.inst_loc(i).is_some()
                || f.inst_results(i).len() > 1
                || f.inst_results(i)
                    .first()
                    .is_some_and(|&v| f.resolve(v) != v)
            {
                return None;
            }
            let allowed = match f.inst_data(i) {
                D::Iconst { ty, .. } => ty.is_int(),
                D::SymbolAddr { .. } => true,
                D::Binary {
                    op:
                        B::Iadd
                        | B::Isub
                        | B::Imul
                        | B::And
                        | B::Or
                        | B::Xor
                        | B::Shl
                        | B::Ushr
                        | B::Sshr
                        | B::Rotl
                        | B::Rotr
                        | B::Ptradd,
                    ..
                } => true,
                D::Unary {
                    op:
                        UnaryOp::Ineg
                        | UnaryOp::Bnot
                        | UnaryOp::Clz
                        | UnaryOp::Ctz
                        | UnaryOp::Popcnt
                        | UnaryOp::Bswap,
                    ..
                } => true,
                D::Convert {
                    op: ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc,
                    ..
                } => true,
                D::Icmp { .. } => true,
                D::Store { flags, value, .. } => {
                    !flags.volatile
                        && matches!(
                            f.value_type(*value),
                            Type::I8 | Type::I16 | Type::I32 | Type::I64
                        )
                }
                D::Load { flags, ty, .. } => {
                    !flags.volatile && matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64)
                }
                _ => false,
            };
            if !allowed {
                return None;
            }
            definitions.extend(f.inst_results(i));
            body_len += 1;
        }
        if f.inst_loc(*insts.last()?).is_some() {
            return None;
        }
    }
    if body_len > MAX_BODY {
        return None;
    }
    c.cost = body_len.checked_mul(c.trips)?
        + if c.body.is_some() {
            f.block_insts(header).len() - 1
        } else {
            0
        };
    if c.cost > MAX_CLONED {
        return None;
    }
    // Only header parameters may be used outside the loop. Exit-edge arguments
    // are inside the region and are substituted from the final iteration.
    for (b, i) in f.layout_insts() {
        if !blocks.contains(&b) {
            let mut escapes = false;
            f.inst_data(i).for_each_operand(|v| {
                let v = f.resolve(v);
                escapes |= definitions.contains(&v) && !params.contains(&v);
            });
            if escapes {
                return None;
            }
        }
    }
    Some(c)
}
fn clone_block(
    f: &mut Function,
    b: Block,
    at: Inst,
    map: &mut FxHashMap<Value, Value>,
    invariants: &mut FxHashMap<Value, Value>,
) {
    let insts = f.block_insts(b).to_vec();
    for &i in &insts[..insts.len() - 1] {
        let original = f.inst_results(i).first().copied();
        // Literals and symbol addresses have no iteration-dependent operands.
        // Keep the first clone: later scalar steps reuse it without requiring
        // another GVN/GCM pass after the unrolled region is constructed.
        if let Some(original) = original
            && let Some(&shared) = invariants.get(&original)
        {
            map.insert(original, shared);
            continue;
        }
        let mut data = f.inst_data(i).clone();
        let invariant = matches!(data, D::Iconst { .. } | D::SymbolAddr { .. });
        data.for_each_operand_mut(|v| {
            let old = f.resolve(*v);
            *v = map.get(&old).copied().unwrap_or(old);
        });
        let new = f.insert_inst(InsertPoint::Before(at), data);
        if let Some(original) = original {
            let value = f.first_result(new);
            map.insert(original, value);
            if invariant {
                invariants.insert(original, value);
            }
        }
    }
}
fn transform(f: &mut Function, c: Candidate) {
    let params = f.block_params(c.header).to_vec();
    let mut current = c.initial.iter().map(|&v| f.resolve(v)).collect::<Vec<_>>();
    let mut map = FxHashMap::default();
    let mut invariants = FxHashMap::default();
    for _ in 0..c.trips {
        map = FxHashMap::from_iter(params.iter().copied().zip(current));
        clone_block(f, c.header, c.entry, &mut map, &mut invariants);
        if let Some(body) = c.body {
            clone_block(f, body, c.entry, &mut map, &mut invariants);
        }
        current = c
            .next
            .iter()
            .map(|&v| {
                let v = f.resolve(v);
                map.get(&v).copied().unwrap_or(v)
            })
            .collect();
    }
    // The counted form tests its final state before taking the exit. Rotated
    // loops take their exit directly from the last executed iteration.
    if c.body.is_some() {
        map = FxHashMap::from_iter(params.iter().copied().zip(current));
        clone_block(f, c.header, c.entry, &mut map, &mut invariants);
    }
    let mut exit = c.exit;
    for v in &mut exit.args {
        let old = f.resolve(*v);
        *v = map.get(&old).copied().unwrap_or(old);
    }
    *f.inst_data_mut(c.entry) = D::Jump { dest: exit };
    // Header parameters used after the loop denote its final header state.
    // A rotated loop's exit sees the state before its last update instead.
    for &p in &params {
        if let Some(&v) = map.get(&p) {
            f.replace_all_uses(p, v);
        }
    }
    if let Some(body) = c.body {
        f.remove_block(body);
    }
    f.remove_block(c.header);
}
fn run(f: &mut Function) -> bool {
    if f.debug_return.is_some() || f.debug_loc.is_some() || !f.debug_vars.is_empty() {
        return false;
    }
    let headers = f.blocks().collect::<Vec<_>>();
    let mut budget = FUNCTION_BUDGET;
    let mut changed = false;
    for header in headers {
        if !f.is_block_inserted(header) {
            continue;
        }
        if let Some(c) = analyze(f, header)
            && c.cost <= budget
        {
            budget -= c.cost;
            transform(f, c);
            changed = true;
        }
    }
    if changed {
        if crate::load_reuse::run_local(f) {
            crate::canonicalize::run(f);
            crate::instcombine::run(f);
        }
        collapse_stores(f);
    }
    changed
}

/// Forwarding the hash recurrence can expose successive writes of the same
/// scalar. Keep the last write, with no intervening access, trap or source
/// location. Reads from possibly aliased input bytes still separate writes.
fn collapse_stores(f: &mut Function) {
    let blocks: Vec<_> = f.blocks().collect();
    for b in blocks {
        let mut pending = None;
        for i in f.block_insts(b).to_vec() {
            if f.inst_loc(i).is_some() {
                pending = None;
                continue;
            }
            match *f.inst_data(i) {
                D::Store { addr, value, flags } if !flags.volatile => {
                    let key = (f.resolve(addr), f.value_type(value));
                    if let Some((previous_key, previous)) = pending
                        && key == previous_key
                    {
                        f.remove_inst(previous);
                    }
                    pending = Some((key, i));
                }
                ref data if data.is_pinned() => pending = None,
                _ => {}
            }
        }
    }
}
