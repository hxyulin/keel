//! Sparse conditional constant propagation (Wegman and Zadeck).
//!
//! Values move only upward: unknown -> constant -> overdefined. Edges
//! become executable monotonically. A block parameter meets arguments
//! from executable *edges*, including distinct arms from the same block.
//! Analysis never mutates the IR; rewriting starts after convergence.

use crate::const_fold;
use keel_ir::{EntityRef, Function, InsertPoint, Inst, InstData, Type, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Unknown,
    Constant(InstData),
    Overdefined,
}

impl State {
    fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) => other.clone(),
            (_, Self::Unknown) => self.clone(),
            (Self::Constant(a), Self::Constant(b)) if a == b => self.clone(),
            _ => Self::Overdefined,
        }
    }
}

struct Solver<'a> {
    func: &'a Function,
    values: Vec<State>,
    users: Vec<Vec<Inst>>,
    executable: Vec<bool>,
    edges: Vec<Vec<bool>>,
    work: Vec<Inst>,
    queued: Vec<bool>,
}

impl<'a> Solver<'a> {
    fn new(func: &'a Function) -> Self {
        let mut users = vec![Vec::new(); func.num_values()];
        let mut edges = vec![Vec::new(); func.num_insts()];
        for (_, inst) in func.layout_insts() {
            func.inst_data(inst).for_each_operand(|v| {
                users[func.resolve(v).index()].push(inst);
            });
            edges[inst.index()] = vec![false; func.inst_data(inst).branch_dests().len()];
        }
        Self {
            func,
            values: vec![State::Unknown; func.num_values()],
            users,
            executable: vec![false; func.num_blocks()],
            edges,
            work: Vec::new(),
            queued: vec![false; func.num_insts()],
        }
    }

    fn state(&self, v: Value) -> &State {
        &self.values[self.func.resolve(v).index()]
    }

    fn enqueue(&mut self, inst: Inst) {
        if !std::mem::replace(&mut self.queued[inst.index()], true) {
            self.work.push(inst);
        }
    }

    fn update(&mut self, value: Value, state: State) {
        let value = self.func.resolve(value);
        let next = self.state(value).meet(&state);
        if next == *self.state(value) {
            return;
        }
        self.values[value.index()] = next;
        for i in 0..self.users[value.index()].len() {
            self.enqueue(self.users[value.index()][i]);
        }
    }

    fn activate(&mut self, inst: Inst, edge: usize) {
        self.edges[inst.index()][edge] = true;
        let dest = &self.func.inst_data(inst).branch_dests()[edge];
        let block = dest.block;
        if !std::mem::replace(&mut self.executable[block.index()], true) {
            for &inst in self.func.block_insts(block) {
                self.enqueue(inst);
            }
        }
    }

    fn evaluate(&self, data: &InstData) -> State {
        if let InstData::Select { args } = data {
            return match self.state(args[0]) {
                State::Constant(InstData::Iconst { imm, .. }) => {
                    self.state(args[if *imm == 0 { 2 } else { 1 }]).clone()
                }
                State::Unknown => State::Unknown,
                _ => self.state(args[1]).meet(self.state(args[2])),
            };
        }
        // Unsupported instructions are immediately overdefined, including
        // calls/loads with unknown operands. They must not stall control flow.
        if !matches!(
            data,
            InstData::Iconst { .. }
                | InstData::Fconst { .. }
                | InstData::Binary { .. }
                | InstData::Unary { .. }
                | InstData::Convert { .. }
                | InstData::Icmp { .. }
                | InstData::Fcmp { .. }
                | InstData::Fma { .. }
        ) {
            return State::Overdefined;
        }
        let mut unknown = false;
        let mut overdefined = false;
        data.for_each_operand(|v| match self.state(v) {
            State::Unknown => unknown = true,
            State::Overdefined => overdefined = true,
            _ => {}
        });
        if overdefined {
            return State::Overdefined;
        }
        if unknown {
            return State::Unknown;
        }
        const_fold::fold(data, |v| match self.state(v) {
            State::Constant(c) => Some(c),
            _ => None,
        })
        .map(normalize)
        .map_or(State::Overdefined, State::Constant)
    }

    fn visit(&mut self, inst: Inst) {
        let data = self.func.inst_data(inst);
        let results = self.func.inst_results(inst);
        if results.len() == 1 {
            self.update(results[0], self.evaluate(data));
        } else if matches!(data, InstData::Overflow { .. }) {
            if let Some(constants) = const_fold::fold_overflow(data, |v| match self.state(v) {
                State::Constant(c) => Some(c),
                _ => None,
            }) {
                for (&result, constant) in results.iter().zip(constants) {
                    self.update(result, State::Constant(constant));
                }
            } else {
                let mut state = State::Unknown;
                data.for_each_operand(|v| {
                    if *self.state(v) == State::Overdefined {
                        state = State::Overdefined;
                    }
                });
                // Known but unsupported widths must not stall control flow.
                let mut all_known = true;
                data.for_each_operand(|v| all_known &= !matches!(self.state(v), State::Unknown));
                if all_known {
                    state = State::Overdefined;
                }
                for &result in results {
                    self.update(result, state.clone());
                }
            }
        } else {
            for &result in results {
                self.update(result, State::Overdefined);
            }
        }
        let chosen = match data {
            InstData::Brif { cond, .. } => match self.state(*cond) {
                State::Constant(InstData::Iconst { imm, .. }) => Some(usize::from(*imm == 0)),
                State::Unknown => return,
                _ => None,
            },
            InstData::Switch { arg, cases, .. } => match self.state(*arg) {
                State::Constant(InstData::Iconst { ty, imm }) if *ty != Type::I128 => {
                    let mask = u64::MAX >> (64 - 8 * ty.bytes().unwrap());
                    Some(
                        cases
                            .iter()
                            .position(|&c| c as u64 & mask == *imm as u64 & mask)
                            .map_or(0, |i| i + 1),
                    )
                }
                State::Unknown => return,
                _ => None,
            },
            _ => None,
        };
        for edge in 0..data.branch_dests().len() {
            if chosen.is_none_or(|chosen| chosen == edge) {
                self.activate(inst, edge);
            }
            if self.edges[inst.index()][edge] {
                let dest = &data.branch_dests()[edge];
                for (&param, &arg) in self.func.block_params(dest.block).iter().zip(&dest.args) {
                    self.update(param, self.state(arg).clone());
                }
            }
        }
    }

    fn solve(mut self) -> Self {
        profiling::scope!("sccp analysis");
        let Some(entry) = self.func.entry_block() else {
            return self;
        };
        self.executable[entry.index()] = true;
        for &param in self.func.block_params(entry) {
            self.update(param, State::Overdefined);
        }
        for &inst in self.func.block_insts(entry) {
            self.enqueue(inst);
        }
        loop {
            while let Some(inst) = self.work.pop() {
                self.queued[inst.index()] = false;
                if self.executable[self.func.inst_block(inst).index()] {
                    self.visit(inst);
                }
            }
            // Unresolved cycles in executable code are not evidence of a
            // constant. Promote them and expose all possible outgoing edges.
            let mut unresolved = Vec::new();
            for block in self.func.blocks().filter(|b| self.executable[b.index()]) {
                for &param in self.func.block_params(block) {
                    if *self.state(param) == State::Unknown {
                        unresolved.push(param);
                    }
                }
                for &inst in self.func.block_insts(block) {
                    for &result in self.func.inst_results(inst) {
                        if *self.state(result) == State::Unknown {
                            unresolved.push(result);
                        }
                    }
                }
            }
            if unresolved.is_empty() {
                return self;
            }
            for value in unresolved {
                self.update(value, State::Overdefined);
            }
        }
    }
}

fn normalize(data: InstData) -> InstData {
    match data {
        InstData::Iconst { ty, imm } if ty != Type::I128 => {
            let shift = 64 - 8 * ty.bytes().unwrap();
            InstData::Iconst {
                ty,
                imm: (imm << shift) >> shift,
            }
        }
        data => data,
    }
}

pub fn run(func: &mut Function) -> bool {
    let solver = Solver::new(func).solve();
    profiling::scope!("sccp rewrite");
    let values = solver.values;
    let executable = solver.executable;
    let mut changed = false;
    let blocks: Vec<_> = func.blocks().collect();
    for block in blocks {
        if !executable[block.index()] {
            continue;
        }
        // Materialize parameter constants locally, preserving dominance
        // even when an unexecuted predecessor passes a different value.
        for param in func.block_params(block).to_vec() {
            if func.resolve(param) != param {
                continue;
            }
            if let State::Constant(data) = &values[param.index()] {
                let first = func.block_insts(block)[0];
                let copy = func.insert_inst(InsertPoint::Before(first), data.clone());
                func.replace_all_uses(param, func.first_result(copy));
                changed = true;
            }
        }
        for inst in func.block_insts(block).to_vec() {
            if let InstData::Select { args } = *func.inst_data(inst)
                && let Some(c) = const_fold::constant(func, args[0])
            {
                let chosen = args[if c.is_zero() { 2 } else { 1 }];
                func.replace_all_uses(func.first_result(inst), chosen);
                func.remove_inst(inst);
                changed = true;
                continue;
            }
            let results = func.inst_results(inst);
            if results.len() == 2 && matches!(func.inst_data(inst), InstData::Overflow { .. }) {
                let constants: Option<Vec<_>> = results
                    .iter()
                    .map(|result| match &values[result.index()] {
                        State::Constant(data) => Some(data.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(constants) = constants {
                    const_fold::replace_with_constants(func, inst, &constants);
                    changed = true;
                    continue;
                }
            }
            if results.len() == 1
                && results[0].index() < values.len()
                && let State::Constant(data) = &values[results[0].index()]
                && func.inst_data(inst) != data
            {
                *func.inst_data_mut(inst) = data.clone();
                changed = true;
            }
        }
    }
    changed
}
