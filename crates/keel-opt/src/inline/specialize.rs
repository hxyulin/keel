//! Bounded constant-argument templates for otherwise unprofitable Full sites.
use super::cost::{CALL_SITE_COST, acyclic_cfg, inline_cost, profitability_reason};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{Function, InsertPoint, Inst, InstData, Purpose, Type, ValueDef};

const MAX_ATTEMPTS: usize = 64;
const MAX_PER_TEMPLATE: usize = 4;

/// Bound cloning work, including dead arena entries and metadata tables.
pub(super) fn eligible(f: &Function) -> bool {
    f.num_stack_slots() <= 128
        && f.num_insts() <= 4096
        && f.num_values() <= 4096
        && f.num_blocks() <= 128
        && f.blocks().take(65).count() <= 64
        && f.layout_insts().take(257).count() <= 256
        && f.callees().take(129).count() <= 128
        && f.data_symbols().take(129).count() <= 128
        && f.signatures().take(65).count() <= 64
        && f.debug_scopes.len() <= 64
        && f.debug_vars.is_empty()
        && f.layout_insts().all(|(_, i)| f.inst_loc(i).is_none())
        && f.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
        && acyclic_cfg(f)
}

type Key = (usize, Vec<(usize, Type, i64)>);

pub(super) struct Body {
    pub function: Box<Function>,
    pub cost: (usize, u64),
}

#[derive(Default)]
pub(super) struct Cache {
    entries: FxHashMap<Key, Option<usize>>,
    attempts: FxHashMap<usize, usize>,
    pub bodies: Vec<Body>,
}

impl Cache {
    pub fn get(
        &mut self,
        index: usize,
        callee: &Function,
        caller: &Function,
        call: Inst,
    ) -> Option<usize> {
        let InstData::Call { func, args } = caller.inst_data(call) else {
            return None;
        };
        if caller.sig.returns_twice
            || caller.personality.is_some()
            || caller.attrs.fast_math != callee.attrs.fast_math
            || caller.callee(*func).sig != callee.sig
            || !caller
                .attrs
                .target_features
                .permits_inlining(&callee.attrs.target_features)
        {
            return None;
        }
        let literals: Vec<_> = args
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                let ValueDef::Result(inst, 0) = caller.value_def(caller.resolve(v)) else {
                    return None;
                };
                match *caller.inst_data(inst) {
                    InstData::Iconst { ty, imm } if ty != Type::I128 => Some((i, ty, imm)),
                    _ => None,
                }
            })
            .collect();
        if literals.is_empty() {
            return None;
        }
        let key = (index, literals);
        if let Some(&result) = self.entries.get(&key) {
            return result;
        }
        let attempts = self.attempts.entry(index).or_default();
        if self.entries.len() >= MAX_ATTEMPTS || *attempts >= MAX_PER_TEMPLATE {
            return None;
        }
        if !eligible(callee) {
            return None;
        }
        *attempts += 1;
        let mut function = Box::new(callee.clone());
        let entry = function
            .entry_block()
            .expect("eligible template has an entry");
        let first = function.block_insts(entry)[0];
        crate::canonicalize::run(&mut function);
        let mut replacements = FxHashMap::default();
        for &(i, ty, imm) in &key.1 {
            let parameter = function.block_params(entry)[i];
            let literal = function.ins(InsertPoint::Before(first)).iconst(ty, imm);
            replacements.insert(parameter, literal);
        }
        // Keep entry parameter definitions intact. Aliasing one to a literal
        // would make DCE retain that literal as an ABI root after its branch
        // disappears, charging and cloning an otherwise dead instruction.
        let instructions: Vec<_> = function.layout_insts().map(|(_, i)| i).collect();
        for instruction in instructions {
            function
                .inst_data_mut(instruction)
                .for_each_operand_mut(|value| {
                    if let Some(&literal) = replacements.get(value) {
                        *value = literal;
                    }
                });
        }
        // These existing transforms preserve trap and memory order. A fixed
        // number of rounds bounds speculative work; no module or recursive pass.
        for _ in 0..2 {
            crate::canonicalize::run(&mut function);
            crate::instcombine::run(&mut function);
            crate::simplify_cfg::run(&mut function);
            crate::phi_simplify::run(&mut function);
            crate::dce::run(&mut function);
        }
        let cost = inline_cost(&function);
        // Constant context must reveal a body no more expensive than the
        // existing call-cost allowance. Larger contexts still need a more
        // precise profitability model, rather than wider ordinary thresholds.
        let result = if cost.0 <= CALL_SITE_COST && profitability_reason(&function, true).is_none()
        {
            let result = self.bodies.len();
            self.bodies.push(Body { function, cost });
            Some(result)
        } else {
            crate::analysis::forget(&function);
            None
        };
        self.entries.insert(key, result);
        result
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        // Speculative templates never belong to the module. Drop their
        // address-keyed analyses before their heap storage can be reused.
        for body in &self.bodies {
            crate::analysis::forget(&body.function);
        }
    }
}
