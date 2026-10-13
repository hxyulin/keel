//! IR verifier. Every pass must leave the function in a state this accepts.
//!
//! Checks, in order: block structure (one terminator, at the end), operand
//! and result types, block-call arguments against block parameters, and
//! that every use is dominated by its definition. All errors are collected.
//! The module verifier adds names, data objects and cross-references.

use crate::data::PTR_BYTES;
use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{
    BinaryOp, BlockCall, ConvertOp, EntityRef, Function, Inst, InstData, Type, UnaryOp, Value,
    ValueDef,
};
use crate::{DataInit, DataItem, Module};
use std::collections::{HashMap, HashSet};
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

/// Verifies every function and data object, that names are unique, and that
/// each function's callees and data match the module's declarations.
pub fn verify_module(module: &Module) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    let names = module
        .decls
        .iter()
        .map(|d| &d.name)
        .chain(module.data.iter().map(|d| &d.name))
        .chain(module.functions.iter().map(|f| &f.name));
    for name in names {
        if !seen.insert(name) {
            errors.push(VerifierError {
                location: format!("@{name}"),
                message: "is declared or defined more than once".into(),
            });
        }
    }
    for data in &module.data {
        if let Some(init) = &data.init {
            for message in check_data(module, init) {
                errors.push(VerifierError {
                    location: format!("@{}", data.name),
                    message,
                });
            }
        }
    }
    for func in &module.functions {
        for (_, name) in func.data_symbols() {
            if module.data_object(name).is_none() {
                errors.push(VerifierError {
                    location: format!("@{}", func.name),
                    message: format!("uses data @{name}, which is not declared"),
                });
            }
        }
        if let Err(func_errors) = verify(func) {
            errors.extend(func_errors.into_iter().map(|e| VerifierError {
                location: format!("@{} {}", func.name, e.location),
                message: e.message,
            }));
        }
        for (_, callee) in func.callees() {
            let message = match module.signature(&callee.name) {
                None => format!("calls @{}, which is not declared", callee.name),
                Some(sig) if *sig != callee.sig => {
                    format!(
                        "calls @{} as {} but it is declared as {sig}",
                        callee.name, callee.sig
                    )
                }
                Some(_) => continue,
            };
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message,
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// The largest alignment a data object may ask for: a page, which is as
/// much as the linker can guarantee.
const MAX_DATA_ALIGN: u32 = 4096;

fn check_data(module: &Module, init: &DataInit) -> Vec<String> {
    let mut errors = Vec::new();
    if !init.align.is_power_of_two() || init.align > MAX_DATA_ALIGN {
        errors.push(format!(
            "alignment must be a power of two up to {MAX_DATA_ALIGN}, got {}",
            init.align
        ));
    }
    if init.size() == 0 {
        // Two empty objects could share an address, which is surprising.
        errors.push("data object is empty".into());
    }
    let mut offset = 0;
    let mut has_aligned_ptrs = u64::from(init.align) >= PTR_BYTES;
    for item in &init.items {
        match item {
            DataItem::Int { ty, value } => match ty.bytes() {
                Some(bytes @ 1..=8) if ty.is_int() => {
                    // Fits as a signed or as an unsigned number of that width.
                    let bits = 8 * bytes;
                    let fits =
                        bits == 64 || (-(1i64 << (bits - 1))..(1i64 << bits)).contains(value);
                    if !fits {
                        errors.push(format!("{value} does not fit in {ty}"));
                    }
                }
                _ => errors.push(format!("data integers must be i8 to i64, got {ty}")),
            },
            DataItem::Ptr { symbol, addend } => {
                if !module.has_symbol(symbol) {
                    errors.push(format!("points to @{symbol}, which is not declared"));
                } else if let Some(target) =
                    module.data_object(symbol).and_then(|d| d.init.as_ref())
                    && u64::from(*addend) > target.size()
                {
                    errors.push(format!(
                        "@{symbol} + {addend} is past the end of @{symbol} ({} bytes)",
                        target.size()
                    ));
                }
                // The loader rewrites pointers in place; Mach-O requires them
                // to be pointer-aligned.
                if offset % PTR_BYTES != 0 {
                    errors.push(format!(
                        "ptr @{symbol} is at offset {offset}, which is not a multiple of {PTR_BYTES}"
                    ));
                }
                if !has_aligned_ptrs {
                    errors.push(format!(
                        "holds pointers, so it needs align {PTR_BYTES} or more, got {}",
                        init.align
                    ));
                    has_aligned_ptrs = true; // report once
                }
            }
            DataItem::Bytes(_) | DataItem::Zero(_) => {}
        }
        offset += item.size();
    }
    errors
}

/// Verifies one function on its own; calls are checked against the
/// signatures in its callee table.
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
        for (ss, data) in func.stack_slots() {
            if !data.align.is_power_of_two() || data.align > 16 {
                self.error(
                    ss.to_string(),
                    format!(
                        "alignment must be a power of two up to 16, got {}",
                        data.align
                    ),
                );
            }
            if data.size == 0 {
                self.error(ss.to_string(), "stack slot size must be non-zero".into());
            }
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
                // Report each value once, even if it is used several times.
                let mut operands = Vec::new();
                func.inst_data(inst).for_each_operand(|v| {
                    let v = func.resolve(v);
                    if !operands.contains(&v) {
                        operands.push(v);
                    }
                });
                for value in operands {
                    if !func.is_value_defined(value) {
                        self.inst_error(
                            inst,
                            format!("uses {value}, whose definition was removed"),
                        );
                    } else if self.domtree.is_reachable(block) {
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
            InstData::Fconst { ty, bits } => {
                if !ty.is_float() {
                    self.inst_error(inst, format!("fconst type must be f32 or f64, got {ty}"));
                } else if *ty == Type::F32 && *bits > u64::from(u32::MAX) {
                    self.inst_error(inst, format!("{bits:#x} is too wide for f32 bits"));
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
                    _ if op.is_float() => {
                        if !a.is_float() || a != b {
                            self.inst_error(
                                inst,
                                format!("{op} takes two floats of the same type, got ({a}, {b})"),
                            );
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
                if op.is_float() {
                    if !a.is_float() {
                        self.inst_error(inst, format!("{op} takes a float, got {a}"));
                    }
                } else if !a.is_int() {
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
                if let Err(rule) = check_convert(*op, ty(*arg), *to) {
                    let from = ty(*arg);
                    self.inst_error(inst, format!("{op} {rule}, got {from} to {to}"));
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
            InstData::Fcmp { args, .. } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a != b || !a.is_float() {
                    self.inst_error(
                        inst,
                        format!("fcmp takes two floats of the same type, got ({a}, {b})"),
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
            InstData::Call { func: callee, args } => {
                let decl = func.callee(*callee);
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if got != decl.sig.params {
                    self.inst_error(
                        inst,
                        format!(
                            "@{} takes ({}), got ({})",
                            decl.name,
                            type_list(&decl.sig.params),
                            type_list(&got)
                        ),
                    );
                }
            }
            InstData::CallIndirect { sig, callee, args } => {
                let sig = func.signature(*sig);
                if ty(*callee) != Type::Ptr {
                    self.inst_error(
                        inst,
                        format!("call_indirect callee must be ptr, got {}", ty(*callee)),
                    );
                }
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if got != sig.params {
                    self.inst_error(
                        inst,
                        format!(
                            "the signature takes ({}), got ({})",
                            type_list(&sig.params),
                            type_list(&got)
                        ),
                    );
                }
            }
            InstData::SymbolAddr { .. } => {}
            InstData::Intrinsic { id, args } => {
                let info = id.info();
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if got != info.params {
                    self.inst_error(
                        inst,
                        format!(
                            "intrinsic {id} takes ({}), got ({})",
                            type_list(info.params),
                            type_list(&got)
                        ),
                    );
                }
            }
            InstData::Trap => {}
            InstData::StackAddr { slot } => {
                if slot.index() >= func.num_stack_slots() {
                    self.inst_error(inst, format!("{slot} is not declared"));
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

/// Checks a conversion other than `ptrtoint`/`inttoptr`; the error is the
/// rule it breaks.
fn check_convert(op: ConvertOp, from: Type, to: Type) -> Result<(), &'static str> {
    // Integers that float conversions take: no i128, which no target
    // converts in one instruction.
    let small_int = |t: Type| t.is_int() && t != Type::I128;
    let ok = match op {
        ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc => match (from.bytes(), to.bytes()) {
            (Some(f), Some(t)) if from.is_int() && to.is_int() => match op {
                ConvertOp::Trunc => t < f,
                _ => t > f,
            },
            _ => false,
        },
        ConvertOp::Fpext => (from, to) == (Type::F32, Type::F64),
        ConvertOp::Fptrunc => (from, to) == (Type::F64, Type::F32),
        ConvertOp::Fptosi | ConvertOp::Fptoui => {
            from.is_float() && matches!(to, Type::I32 | Type::I64)
        }
        ConvertOp::Sitofp | ConvertOp::Uitofp => small_int(from) && to.is_float(),
        ConvertOp::Bitcast => {
            from.is_float() != to.is_float()
                && (from.is_int() || to.is_int())
                && from.bytes() == to.bytes()
        }
        ConvertOp::PtrToInt => (from, to) == (Type::Ptr, Type::I64),
        ConvertOp::IntToPtr => (from, to) == (Type::I64, Type::Ptr),
    };
    if ok {
        return Ok(());
    }
    Err(match op {
        ConvertOp::Sext | ConvertOp::Uext => "must convert an integer to a wider integer",
        ConvertOp::Trunc => "must convert an integer to a narrower integer",
        ConvertOp::Fpext => "converts f32 to f64",
        ConvertOp::Fptrunc => "converts f64 to f32",
        ConvertOp::Fptosi | ConvertOp::Fptoui => "converts a float to i32 or i64",
        ConvertOp::Sitofp | ConvertOp::Uitofp => "converts an integer up to i64 to a float",
        ConvertOp::Bitcast => "converts between an integer and a float of the same size",
        ConvertOp::PtrToInt => "converts ptr to i64",
        ConvertOp::IntToPtr => "converts i64 to ptr",
    })
}

fn type_list(tys: &[Type]) -> String {
    tys.iter()
        .map(Type::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
