//! IR verifier. Every pass must leave the function in a state this accepts.
//!
//! Checks, in order: block structure (one terminator, at the end), operand
//! and result types, block-call arguments against block parameters, and
//! that every use is dominated by its definition. All errors are collected.

use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{
    BinaryOp, BlockCall, ConvertOp, Function, Inst, InstData, Type, UnaryOp, Value, ValueDef,
};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierError {
    /// Where the error is: a block, or an instruction with its text.
    pub location: String,
    pub message: String,
}

impl fmt::Display for VerifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

pub fn verify(func: &Function) -> Result<(), Vec<VerifierError>> {
    let cfg = ControlFlowGraph::compute(func);
    let domtree = DominatorTree::compute(func, &cfg);
    let mut v = Verifier {
        func,
        cfg,
        domtree,
        errors: Vec::new(),
    };
    v.run();
    if v.errors.is_empty() {
        Ok(())
    } else {
        Err(v.errors)
    }
}

struct Verifier<'a> {
    func: &'a Function,
    cfg: ControlFlowGraph,
    domtree: DominatorTree,
    errors: Vec<VerifierError>,
}

impl Verifier<'_> {
    fn run(&mut self) {
        let func = self.func;
        let Some(entry) = func.entry_block() else {
            self.error("function".into(), "function has no blocks".into());
            return;
        };

        let param_tys: Vec<Type> = func
            .block_params(entry)
            .iter()
            .map(|&p| func.value_type(p))
            .collect();
        if param_tys != func.sig.params {
            self.error(
                entry.to_string(),
                format!(
                    "entry block parameters ({}) do not match the signature ({})",
                    type_list(&param_tys),
                    type_list(&func.sig.params)
                ),
            );
        }
        if !self.cfg.preds(entry).is_empty() {
            self.error(
                entry.to_string(),
                "the entry block cannot be a branch target".into(),
            );
        }

        // Position of each instruction within its block, for same-block dominance.
        let mut position = HashMap::new();
        for block in func.blocks() {
            for (i, &inst) in func.block_insts(block).iter().enumerate() {
                position.insert(inst, i);
            }
        }

        for block in func.blocks() {
            let insts = func.block_insts(block);
            match insts.last() {
                None => self.error(block.to_string(), "block is empty".into()),
                Some(&last) if !func.inst_data(last).is_terminator() => self.error(
                    block.to_string(),
                    "block does not end in a terminator".into(),
                ),
                _ => {}
            }
            for (i, &inst) in insts.iter().enumerate() {
                if i + 1 < insts.len() && func.inst_data(inst).is_terminator() {
                    self.inst_error(inst, "terminator in the middle of a block".into());
                }
                self.check_types(inst);
                for dest in func.inst_data(inst).branch_dests() {
                    self.check_block_call(inst, dest);
                }
                if self.domtree.is_reachable(block) {
                    // Report each value once, even if it is used several times.
                    let mut operands = Vec::new();
                    func.inst_data(inst).for_each_operand(|v| {
                        let v = func.resolve(v);
                        if !operands.contains(&v) {
                            operands.push(v);
                        }
                    });
                    for value in operands {
                        self.check_dominance(inst, value, &position);
                    }
                }
            }
        }
    }

    fn check_types(&mut self, inst: Inst) {
        let func = self.func;
        let ty = |v: Value| func.value_type(func.resolve(v));
        match func.inst_data(inst) {
            InstData::Iconst { ty, .. } => {
                if !ty.is_int() {
                    self.inst_error(inst, format!("iconst type must be an integer, got {ty}"));
                }
            }
            InstData::Binary { op, args } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                match op {
                    BinaryOp::Ptradd => {
                        if a != Type::Ptr || b != Type::I64 {
                            self.inst_error(
                                inst,
                                format!("ptradd takes (ptr, i64), got ({a}, {b})"),
                            );
                        }
                    }
                    BinaryOp::Shl
                    | BinaryOp::Ushr
                    | BinaryOp::Sshr
                    | BinaryOp::Rotl
                    | BinaryOp::Rotr => {
                        if !a.is_int() || !b.is_int() {
                            self.inst_error(inst, format!("{op} takes integers, got ({a}, {b})"));
                        }
                    }
                    _ => {
                        if !a.is_int() || a != b {
                            self.inst_error(
                                inst,
                                format!("{op} takes two integers of the same type, got ({a}, {b})"),
                            );
                        }
                    }
                }
            }
            InstData::Unary { op, arg } => {
                let a = ty(*arg);
                if !a.is_int() {
                    self.inst_error(inst, format!("{op} takes an integer, got {a}"));
                } else if *op == UnaryOp::Bswap && a == Type::I8 {
                    self.inst_error(inst, "bswap needs at least i16".into());
                }
            }
            InstData::Select { args } => {
                let (c, a, b) = (ty(args[0]), ty(args[1]), ty(args[2]));
                if !c.is_int() {
                    self.inst_error(
                        inst,
                        format!("select condition must be an integer, got {c}"),
                    );
                }
                if a != b {
                    self.inst_error(
                        inst,
                        format!("select arms must have the same type, got ({a}, {b})"),
                    );
                }
            }
            InstData::Convert { op, ty: to, arg } => {
                let from = ty(*arg);
                let ok = match (from.bytes(), to.bytes()) {
                    (Some(f), Some(t)) if from.is_int() && to.is_int() => match op {
                        ConvertOp::Sext | ConvertOp::Uext => t > f,
                        ConvertOp::Trunc => t < f,
                    },
                    _ => false,
                };
                if !ok {
                    let rule = match op {
                        ConvertOp::Sext | ConvertOp::Uext => "a wider",
                        ConvertOp::Trunc => "a narrower",
                    };
                    self.inst_error(
                        inst,
                        format!(
                            "{op} must convert an integer to {rule} integer, got {from} to {to}"
                        ),
                    );
                }
            }
            InstData::Icmp { args, .. } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a != b || !(a.is_int() || a == Type::Ptr) {
                    self.inst_error(
                        inst,
                        format!(
                            "icmp takes two integers or pointers of the same type, got ({a}, {b})"
                        ),
                    );
                }
            }
            InstData::Load { addr, .. } => {
                if ty(*addr) != Type::Ptr {
                    self.inst_error(inst, format!("load address must be ptr, got {}", ty(*addr)));
                }
            }
            InstData::Store { addr, .. } => {
                if ty(*addr) != Type::Ptr {
                    self.inst_error(
                        inst,
                        format!("store address must be ptr, got {}", ty(*addr)),
                    );
                }
            }
            InstData::Jump { .. } => {}
            InstData::Brif { cond, .. } => {
                if !ty(*cond).is_int() {
                    self.inst_error(
                        inst,
                        format!("brif condition must be an integer, got {}", ty(*cond)),
                    );
                }
            }
            InstData::Return { args } => {
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if got != func.sig.returns {
                    self.inst_error(
                        inst,
                        format!(
                            "returns ({}) but the signature returns ({})",
                            type_list(&got),
                            type_list(&func.sig.returns)
                        ),
                    );
                }
            }
        }
    }

    fn check_block_call(&mut self, inst: Inst, dest: &BlockCall) {
        let func = self.func;
        if !func.blocks().any(|b| b == dest.block) {
            self.inst_error(inst, format!("{} is not in the function", dest.block));
            return;
        }
        let params = func.block_params(dest.block);
        if params.len() != dest.args.len() {
            self.inst_error(
                inst,
                format!(
                    "{} takes {} argument(s), got {}",
                    dest.block,
                    params.len(),
                    dest.args.len()
                ),
            );
            return;
        }
        for (&param, &arg) in params.iter().zip(&dest.args) {
            let (want, got) = (func.value_type(param), func.value_type(func.resolve(arg)));
            if want != got {
                self.inst_error(
                    inst,
                    format!(
                        "argument for {} parameter {param} must be {want}, got {got}",
                        dest.block
                    ),
                );
            }
        }
    }

    fn check_dominance(&mut self, inst: Inst, value: Value, position: &HashMap<Inst, usize>) {
        let func = self.func;
        let value = func.resolve(value);
        let use_block = func.inst_block(inst);
        let ok = match func.value_def(value) {
            ValueDef::Param(def_block, _) => self.domtree.dominates(def_block, use_block),
            ValueDef::Result(def_inst, _) => {
                let def_block = func.inst_block(def_inst);
                if def_block == use_block {
                    position[&def_inst] < position[&inst]
                } else {
                    self.domtree.dominates(def_block, use_block)
                }
            }
            ValueDef::Alias(_) => unreachable!("resolved values are never aliases"),
        };
        if !ok {
            self.inst_error(
                inst,
                format!("use of {value} is not dominated by its definition"),
            );
        }
    }

    fn inst_error(&mut self, inst: Inst, message: String) {
        let location = format!("{} `{}`", inst, self.func.display_inst(inst));
        self.error(location, message);
    }

    fn error(&mut self, location: String, message: String) {
        self.errors.push(VerifierError { location, message });
    }
}

fn type_list(tys: &[Type]) -> String {
    tys.iter()
        .map(Type::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
