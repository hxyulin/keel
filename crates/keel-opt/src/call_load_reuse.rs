//! Bounded transitive no-memory summaries and successful-load availability.
//!
//! The summary is deliberately not a general capture analysis: accepted bodies
//! cannot persist any operand, but may return it (or a value derived from it).
//! Calls and all original successful reads stay pinned. Only later redundant
//! reads disappear; there is no speculation, load motion, or store forwarding.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InstData, Intrinsic, Module, Purpose, Signature,
    Type, Value, ValueDef,
};

pub struct CallLoadReuse;
impl crate::Pass for CallLoadReuse {
    fn name(&self) -> &'static str {
        "call-load-reuse"
    }
    fn description(&self) -> &'static str {
        "reuse successful loads across bounded no-memory direct call chains (opt-in)"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
}

// Leaves have depth zero; at most eight contained call edges may follow.
const MAX_CALL_DEPTH: usize = 8;
const MAX_INSTS: usize = 50_000;
const MAX_VALUES: usize = 100_000;
const MAX_FACTS: usize = 128;
const MAX_EDGES: usize = 8;
const MAX_BLOCK_INDICES: usize = 8192;
const MAX_DESTINATIONS: usize = 16_384;
const MAX_BLOCK_DESTINATIONS: usize = 1024;

// CFG construction deduplicates destinations within a block. Bound both that
// quadratic work and total syntactic edges before allocating or computing it.
fn eligible_caller(f: &Function) -> bool {
    if f.num_blocks() > MAX_BLOCK_INDICES {
        return false;
    }
    let mut remaining = MAX_DESTINATIONS;
    let mut instructions = 0;
    let mut reads = 0;
    for b in f.blocks() {
        let mut block_remaining = MAX_BLOCK_DESTINATIONS;
        for &i in f.block_insts(b) {
            instructions += 1;
            if instructions > MAX_INSTS {
                return false;
            }
            match f.inst_data(i) {
                InstData::Intrinsic {
                    id: Intrinsic::FrameAddress,
                    ..
                } => return false,
                InstData::Load { flags, .. } if !flags.volatile => reads = (reads + 1).min(2),
                _ => {}
            }
            let count = f.inst_data(i).branch_dests().len();
            if count > remaining || count > block_remaining {
                return false;
            }
            remaining -= count;
            block_remaining -= count;
        }
    }
    // Without two ordinary reads there can be no reusable successful read.
    // Avoid CFG construction and availability snapshots for these callers.
    reads == 2
}

// Whitelist excludes division, floating environment effects, frame observers,
// stack lifetime operations, nonlocal control, assembly and all memory.
// Ordinary direct calls are checked separately against completed summaries.
fn scalar(data: &InstData) -> bool {
    matches!(
        data,
        InstData::Iconst { .. }
            | InstData::Icmp { .. }
            | InstData::Select { .. }
            | InstData::Convert {
                op: ConvertOp::Uext | ConvertOp::Sext | ConvertOp::Trunc,
                ..
            }
            | InstData::Binary {
                op: BinaryOp::Iadd
                    | BinaryOp::Isub
                    | BinaryOp::Imul
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Xor
                    | BinaryOp::Shl
                    | BinaryOp::Ushr
                    | BinaryOp::Sshr
                    | BinaryOp::Ptradd,
                ..
            }
    )
}

fn summary_candidate(f: &Function) -> bool {
    !f.weak_odr
        && (f.local || f.attrs.noninterposable)
        && !f.sig.returns_twice
        && !f.sig.variadic
        // ABI aggregate copies are memory operations even without an IR load.
        && f.sig.params.iter().chain(&f.sig.returns).all(|p| p.purpose == Purpose::Normal)
        && !f.attrs.noreturn
        && f.debug_vars.is_empty()
        && f.debug_loc.is_none()
        && f.num_values() <= 512
        && f.blocks().take(2).count() == 1
        && f.entry_block().is_some_and(|b| {
            let insts = f.block_insts(b);
            insts.len() <= 128
                && insts.iter().all(|&i| {
                    f.inst_loc(i).is_none()
                        && (scalar(f.inst_data(i))
                            || matches!(f.inst_data(i), InstData::Return { .. } | InstData::Call { .. }))
                })
        })
}

type Facts = FxHashMap<(Value, Type), Value>;
fn run(module: &mut Module) -> bool {
    // Names must resolve uniquely even for callers that bypass module verification.
    let mut counts = FxHashMap::default();
    for f in &module.functions {
        *counts.entry(f.name.clone()).or_insert(0usize) += 1;
    }
    let candidates: Vec<&Function> = module
        .functions
        .iter()
        .filter(|f| counts[&f.name] == 1 && summary_candidate(f))
        .collect();
    let mut summaries = FxHashMap::<String, Signature>::default();
    // Synchronous bottom-up rounds: each round may use only earlier proofs.
    // This rejects recursion (including SCCs with an outgoing pure leaf), makes
    // the depth limit independent of module order, and avoids recursive DFS or
    // unbounded fixed-point work. Each body has at most 128 instructions.
    for _ in 0..=MAX_CALL_DEPTH {
        let proven: Vec<_> = candidates
            .iter()
            .filter(|f| !summaries.contains_key(&f.name))
            .filter(|f| {
                f.layout_insts().all(|(_, i)| match f.inst_data(i) {
                    InstData::Call { func, .. } => {
                        let decl = f.callee(*func);
                        !decl.attrs.noreturn && summaries.get(&decl.name) == Some(&decl.sig)
                    }
                    _ => true,
                })
            })
            .map(|f| (f.name.clone(), f.sig.clone()))
            .collect();
        if proven.is_empty() {
            break;
        }
        summaries.extend(proven);
    }
    let mut changed = false;
    for f in &mut module.functions {
        if f.num_values() > MAX_VALUES
            || f.sig.returns_twice
            || !f.debug_vars.is_empty()
            || f.debug_loc.is_some()
        {
            continue;
        }
        if !eligible_caller(f) {
            continue;
        }
        let cfg = ControlFlowGraph::compute(f);
        let mut exits: Vec<Option<(usize, Facts)>> = vec![None; f.num_blocks()];
        for b in cfg.reverse_postorder(f) {
            let mut facts = Facts::default();
            let mut depth = 0;
            let preds = cfg.preds(b);
            // No fixed point: missing exits (including backedges) lose all facts.
            if Some(b) != f.entry_block()
                && !preds.is_empty()
                && preds.iter().all(|p| {
                    exits[p.index()]
                        .as_ref()
                        .is_some_and(|(d, _)| *d < MAX_EDGES)
                })
            {
                let (d, first) = exits[preds[0].index()].as_ref().unwrap();
                depth = d + 1;
                facts = first.clone();
                for p in &preds[1..] {
                    let (d, other) = exits[p.index()].as_ref().unwrap();
                    depth = depth.max(d + 1);
                    facts.retain(|key, value| other.get(key) == Some(value));
                }
            }
            for i in f.block_insts(b).to_vec() {
                if f.inst_loc(i).is_some() {
                    facts.clear();
                    continue;
                }
                match f.inst_data(i) {
                    InstData::Load { ty, addr, flags } if !flags.volatile => {
                        let key = (f.resolve(*addr), *ty);
                        let result = f.first_result(i);
                        if !matches!(f.value_def(result), ValueDef::Result(owner, 0) if owner == i)
                        {
                            continue;
                        }
                        if let Some(&old) = facts.get(&key) {
                            f.replace_all_uses(result, f.resolve(old));
                            f.remove_inst(i);
                            changed = true;
                        } else {
                            if facts.len() == MAX_FACTS {
                                facts.clear();
                            }
                            facts.insert(key, result);
                        }
                    }
                    InstData::Call { func, .. }
                        if {
                            let decl = f.callee(*func);
                            !decl.attrs.noreturn && summaries.get(&decl.name) == Some(&decl.sig)
                        } => {}
                    InstData::Jump { .. }
                    | InstData::Brif { .. }
                    | InstData::Switch { .. }
                    | InstData::Return { .. } => {}
                    data if scalar(data) => {}
                    _ => facts.clear(),
                }
            }
            exits[b.index()] = Some((depth, facts));
        }
    }
    changed
}
