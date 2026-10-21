//! Body-proven no-memory leaves and bounded successful-load availability.
//!
//! The summary is deliberately not a general capture analysis: accepted bodies
//! cannot persist any operand, but may return it (or a value derived from it).
//! Calls and all original successful reads stay pinned. Only later redundant
//! reads disappear; there is no speculation, load motion, or store forwarding.
use keel_ir::dominators::ControlFlowGraph;
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InstData, Intrinsic, Module, Signature, Type, Value,
    ValueDef,
};
use std::collections::HashMap;

pub struct CallLoadReuse;
impl crate::Pass for CallLoadReuse {
    fn name(&self) -> &'static str {
        "call-load-reuse"
    }
    fn description(&self) -> &'static str {
        "reuse successful loads across body-proven no-memory direct leaves (opt-in)"
    }
    fn run(&self, _: &mut Function) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module) -> Option<bool> {
        Some(run(module))
    }
}

const MAX_INSTS: usize = 50_000;
const MAX_VALUES: usize = 100_000;
const MAX_FACTS: usize = 128;
const MAX_EDGES: usize = 8;

// Whitelist excludes division, floating environment effects, frame observers,
// stack lifetime operations, nonlocal control, calls, assembly and all memory.
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

fn leaf(f: &Function) -> bool {
    (f.local || f.attrs.noninterposable)
        && !f.sig.returns_twice
        && !f.sig.variadic
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
                            || matches!(f.inst_data(i), InstData::Return { .. }))
                })
        })
}

type Facts = HashMap<(Value, Type), Value>;
fn run(module: &mut Module) -> bool {
    // Names must resolve uniquely even for callers that bypass module verification.
    let mut counts = HashMap::new();
    for f in &module.functions {
        *counts.entry(f.name.clone()).or_insert(0usize) += 1;
    }
    let summaries: HashMap<String, Signature> = module
        .functions
        .iter()
        .filter(|f| counts[&f.name] == 1 && leaf(f))
        .map(|f| (f.name.clone(), f.sig.clone()))
        .collect();
    let mut changed = false;
    for f in &mut module.functions {
        if f.num_values() > MAX_VALUES
            || f.sig.returns_twice
            || !f.debug_vars.is_empty()
            || f.debug_loc.is_some()
            || f.layout_insts().take(MAX_INSTS + 1).count() > MAX_INSTS
            || f.layout_insts().any(|(_, i)| {
                matches!(
                    f.inst_data(i),
                    InstData::Intrinsic {
                        id: Intrinsic::FrameAddress,
                        ..
                    }
                )
            })
        {
            continue;
        }
        let cfg = ControlFlowGraph::compute(f);
        let mut exits: Vec<Option<(usize, Facts)>> = vec![None; f.num_blocks()];
        for b in cfg.reverse_postorder(f) {
            let mut facts = Facts::new();
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
                            !decl.sig.returns_twice && summaries.get(&decl.name) == Some(&decl.sig)
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
