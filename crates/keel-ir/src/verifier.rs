//! IR verifier. Every pass must leave the function in a state this accepts.
//!
//! Checks, in order: block structure (one terminator, at the end), operand
//! and result types, block-call arguments against block parameters, and
//! that every use is dominated by its definition. All errors are collected.
//! The module verifier adds names, data objects and cross-references.

use crate::data::PTR_BYTES;
use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::{
    ArgExt, AsmOperand, BinaryOp, BlockCall, Clause, ConvertOp, EntityRef, Function, InlineAsm,
    Inst, InstData, Intrinsic, Ordering, PhysReg, Purpose, RmwOp, Signature, Type, UnaryOp, Value,
    ValueDef,
};
use crate::{DataInit, DataItem, DebugType, Module};
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

/// Verifies every function and data object, that names are unique, that
/// each function's callees and data match the module's declarations, and
/// that instruction locations name the module's source files.
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
    for decl in &module.decls {
        for message in check_signature(&decl.sig) {
            errors.push(VerifierError {
                location: format!("@{}", decl.name),
                message,
            });
        }
    }
    for data in &module.data {
        if data.thread_local && data.init.as_ref().is_some_and(|init| init.constant) {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "thread-local data cannot be const".into(),
            });
        }
        if data.local && data.init.is_none() {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "only a definition can be local".into(),
            });
        }
        if let Some(init) = &data.init {
            for message in check_data(module, init) {
                errors.push(VerifierError {
                    location: format!("@{}", data.name),
                    message,
                });
            }
        }
    }
    errors.extend(check_debug_info(module));
    for func in &module.functions {
        if let Some(personality) = &func.personality
            && module.signature(personality).is_none()
        {
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message: format!("personality @{personality} is not a declared function"),
            });
        }
        for (_, inst) in func.layout_insts() {
            if let Some(loc) = func.inst_loc(inst)
                && loc.file.index() >= module.files.len()
            {
                errors.push(VerifierError {
                    location: format!("@{} {inst}", func.name),
                    message: format!(
                        "has location {loc}, but the module has no file {}",
                        loc.file
                    ),
                });
            }
        }
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

/// Debug types refer only to types in the table, variables live in slots
/// of their function, and globals name data objects.
fn check_debug_info(module: &Module) -> Vec<VerifierError> {
    let mut errors = Vec::new();
    let known = |ty: DebugType| ty.index() < module.debug_types.len();
    for (i, data) in module.debug_types.iter().enumerate() {
        for ty in data.refs() {
            if !known(ty) {
                errors.push(VerifierError {
                    location: DebugType::new(i).to_string(),
                    message: format!("refers to {ty}, which is not defined"),
                });
            }
        }
    }
    for global in &module.debug_globals {
        let location = format!("dglobal {:?}", global.name);
        if !known(global.ty) {
            errors.push(VerifierError {
                location: location.clone(),
                message: format!("has type {}, which is not defined", global.ty),
            });
        }
        if module.data_object(&global.symbol).is_none() {
            errors.push(VerifierError {
                location,
                message: format!("@{} is not a data object", global.symbol),
            });
        }
    }
    for func in &module.functions {
        if let Some(ty) = func.debug_return
            && !known(ty)
        {
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message: format!("returns {ty}, which is not defined"),
            });
        }
        for var in &func.debug_vars {
            let location = format!("@{} {var}", func.name);
            if !known(var.ty) {
                errors.push(VerifierError {
                    location: location.clone(),
                    message: format!("has type {}, which is not defined", var.ty),
                });
            }
            if var.slot.index() >= func.num_stack_slots() {
                errors.push(VerifierError {
                    location,
                    message: format!("{} is not a stack slot of the function", var.slot),
                });
            }
        }
    }
    errors
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
                } else if module.data_object(symbol).is_some_and(|d| d.thread_local) {
                    errors.push(format!(
                        "points to thread-local @{symbol}, whose address differs per thread"
                    ));
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
        if param_tys != func.sig.param_types() {
            self.error(
                entry.to_string(),
                format!(
                    "entry block parameters ({}) do not match the signature ({})",
                    type_list(&param_tys),
                    type_list(&func.sig.param_types())
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
        for message in check_signature(&func.sig) {
            self.error("signature".into(), message);
        }
        for (sig_ref, sig) in func.signatures() {
            for message in check_signature(sig) {
                self.error(sig_ref.to_string(), message);
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

    /// An atomic access of a `value` type through an `addr`: integers up
    /// to 64 bits, and `ptr` where it makes sense.
    fn check_atomic(&mut self, inst: Inst, what: &str, value: Type, addr: Type, allows_ptr: bool) {
        let ok = matches!(value, Type::I8 | Type::I16 | Type::I32 | Type::I64)
            || (allows_ptr && value == Type::Ptr);
        if !ok {
            let types = if allows_ptr {
                "integers up to i64 and ptr"
            } else {
                "integers up to i64"
            };
            self.inst_error(inst, format!("{what} takes {types}, got {value}"));
        }
        if addr != Type::Ptr {
            self.inst_error(inst, format!("{what} address must be ptr, got {addr}"));
        }
    }

    /// A tail call returns the callee's results as this function's, so
    /// they must be the same, extensions included.
    fn check_tail_returns(&mut self, inst: Inst, callee: &Signature) {
        if callee.returns != self.func.sig.returns {
            let list = |sig: &Signature| {
                let returns: Vec<String> = sig.returns.iter().map(|r| r.to_string()).collect();
                returns.join(", ")
            };
            self.inst_error(
                inst,
                format!(
                    "a tail call to a function returning ({}) from one returning ({})",
                    list(callee),
                    list(&self.func.sig)
                ),
            );
        }
    }

    /// Each operand's type fits its constraint: registers hold scalars of
    /// their class, up to 64 bits, `m` takes an address, `i` a constant,
    /// and a tied input the type of its output, which it may initialize
    /// only once.
    fn check_asm(&mut self, inst: Inst, asm: &InlineAsm, args: &[Value]) {
        let func = self.func;
        if args.len() != asm.inputs.len() {
            self.inst_error(
                inst,
                format!(
                    "{} input constraint(s) but {} argument(s)",
                    asm.inputs.len(),
                    args.len()
                ),
            );
            return;
        }
        let fits = |op: AsmOperand, ty: Type| match op {
            AsmOperand::Reg | AsmOperand::Fixed(PhysReg { float: false, .. }) => {
                (ty.is_int() && ty != Type::I128) || ty == Type::Ptr
            }
            AsmOperand::FloatReg | AsmOperand::Fixed(PhysReg { float: true, .. }) => ty.is_float(),
            AsmOperand::Mem => ty == Type::Ptr,
            AsmOperand::Imm | AsmOperand::Tied(_) => true,
        };
        for (n, &(op, ty)) in asm.outputs.iter().enumerate() {
            if !fits(op, ty) {
                self.inst_error(inst, format!("asm operand {n} (`{op}`) cannot be {ty}"));
            }
        }
        let mut tied = Vec::new();
        for (i, (&op, &arg)) in asm.inputs.iter().zip(args).enumerate() {
            let n = asm.outputs.len() + i;
            let ty = func.value_type(func.resolve(arg));
            if !fits(op, ty) {
                self.inst_error(inst, format!("asm operand {n} (`{op}`) cannot be {ty}"));
            }
            match op {
                AsmOperand::Imm => {
                    let is_const = matches!(
                        func.value_def(func.resolve(arg)),
                        ValueDef::Result(def, _) if matches!(func.inst_data(def), InstData::Iconst { .. })
                    );
                    if !is_const {
                        self.inst_error(inst, format!("asm operand {n} (`i`) must be an iconst"));
                    }
                }
                AsmOperand::Tied(out) => match asm.outputs.get(out as usize) {
                    None => self.inst_error(
                        inst,
                        format!("asm operand {n} is tied to output {out}, which does not exist"),
                    ),
                    Some(&(_, out_ty)) if out_ty != ty => self.inst_error(
                        inst,
                        format!("asm operand {n} is {ty}, but its output {out} is {out_ty}"),
                    ),
                    Some(_) if tied.contains(&out) => self
                        .inst_error(inst, format!("output {out} is tied to more than one input")),
                    Some(_) => tied.push(out),
                },
                _ => {}
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
            InstData::Overflow { op, args } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if !a.is_int() || a != b {
                    self.inst_error(
                        inst,
                        format!("{op} takes two integers of the same type, got ({a}, {b})"),
                    );
                }
            }
            InstData::Fma { args } => {
                let (a, b, c) = (ty(args[0]), ty(args[1]), ty(args[2]));
                if !a.is_float() || a != b || a != c {
                    self.inst_error(
                        inst,
                        format!("fma takes three floats of the same type, got ({a}, {b}, {c})"),
                    );
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
            InstData::VaArg { ty: t, list } => {
                if !matches!(t, Type::I32 | Type::I64 | Type::Ptr | Type::F64) {
                    self.inst_error(
                        inst,
                        format!("va_arg reads i32, i64, ptr or f64 (C's promoted types), got {t}"),
                    );
                }
                if ty(*list) != Type::Ptr {
                    self.inst_error(inst, format!("va_arg list must be ptr, got {}", ty(*list)));
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
            InstData::AtomicLoad {
                ty: t,
                ordering,
                addr,
            } => {
                self.check_atomic(inst, "atomic_load", *t, ty(*addr), true);
                if matches!(ordering, Ordering::Release | Ordering::AcqRel) {
                    self.inst_error(inst, format!("a load cannot be {ordering}"));
                }
            }
            InstData::AtomicStore {
                ordering,
                value,
                addr,
            } => {
                self.check_atomic(inst, "atomic_store", ty(*value), ty(*addr), true);
                if matches!(ordering, Ordering::Acquire | Ordering::AcqRel) {
                    self.inst_error(inst, format!("a store cannot be {ordering}"));
                }
            }
            InstData::AtomicRmw { op, args, .. } => {
                let what = format!("atomic_rmw.{op}");
                let allows_ptr = *op == RmwOp::Xchg;
                self.check_atomic(inst, &what, ty(args[1]), ty(args[0]), allows_ptr);
            }
            InstData::AtomicCas { args, .. } => {
                let (expected, new) = (ty(args[1]), ty(args[2]));
                self.check_atomic(inst, "atomic_cas", expected, ty(args[0]), true);
                if expected != new {
                    self.inst_error(
                        inst,
                        format!(
                            "atomic_cas values must have the same type, got ({expected}, {new})"
                        ),
                    );
                }
            }
            InstData::Fence { ordering } => {
                if *ordering == Ordering::Relaxed {
                    self.inst_error(inst, "a fence cannot be relaxed".into());
                }
            }
            InstData::Call { func: callee, args } => {
                let decl = func.callee(*callee);
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if !args_match(&decl.sig, &got) {
                    self.inst_error(
                        inst,
                        format!(
                            "@{} takes ({}), got ({})",
                            decl.name,
                            param_list(&decl.sig),
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
                if !args_match(sig, &got) {
                    self.inst_error(
                        inst,
                        format!(
                            "the signature takes ({}), got ({})",
                            param_list(sig),
                            type_list(&got)
                        ),
                    );
                }
            }
            InstData::SymbolAddr { .. } => {}
            InstData::Intrinsic { id, args } => {
                if *id == Intrinsic::VaStart && !func.sig.variadic {
                    self.inst_error(inst, "va_start in a function that is not variadic".into());
                }
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
            InstData::InlineAsm { asm, args } => self.check_asm(inst, asm, args),
            InstData::Trap => {}
            InstData::StackAddr { slot } => {
                if slot.index() >= func.num_stack_slots() {
                    self.inst_error(inst, format!("{slot} is not declared"));
                }
            }
            InstData::Jump { .. } => {}
            InstData::Switch { arg, cases, .. } => {
                let t = ty(*arg);
                if !t.is_int() || t == Type::I128 {
                    self.inst_error(inst, format!("switch takes i8 to i64, got {t}"));
                    return;
                }
                let bits = 8 * t.bytes().unwrap();
                let mask = u64::MAX >> (64 - bits);
                let mut seen = HashSet::new();
                for &case in cases {
                    let fits =
                        bits == 64 || (-(1i64 << (bits - 1))..(1i64 << bits)).contains(&case);
                    if !fits {
                        self.inst_error(inst, format!("case {case} does not fit in {t}"));
                    } else if !seen.insert(case as u64 & mask) {
                        self.inst_error(
                            inst,
                            format!("case {case} is the same {t} as an earlier case"),
                        );
                    }
                }
            }
            InstData::Brif { cond, .. } => {
                if !ty(*cond).is_int() {
                    self.inst_error(
                        inst,
                        format!("brif condition must be an integer, got {}", ty(*cond)),
                    );
                }
            }
            InstData::Invoke {
                func: callee,
                args,
                dests,
            } => {
                let decl = func.callee(*callee);
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if !args_match(&decl.sig, &got) {
                    self.inst_error(
                        inst,
                        format!(
                            "@{} takes ({}), got ({})",
                            decl.name,
                            param_list(&decl.sig),
                            type_list(&got)
                        ),
                    );
                }
                // Arguments would be copied after the call returns, so
                // the call could clobber them.
                if dests.iter().any(|d| !d.args.is_empty()) {
                    self.inst_error(inst, "invoke destinations take no arguments".into());
                }
                if dests[0].block == dests[1].block {
                    self.inst_error(
                        inst,
                        "invoke needs different normal and unwind destinations".into(),
                    );
                }
                let pad = func.block_insts(dests[1].block).first();
                if !pad.is_some_and(|&p| matches!(func.inst_data(p), InstData::LandingPad { .. })) {
                    self.inst_error(
                        inst,
                        format!(
                            "unwind destination {} does not start with landingpad",
                            dests[1].block
                        ),
                    );
                }
                if func.personality.is_none() {
                    self.inst_error(inst, "invoke in a function without a personality".into());
                }
            }
            InstData::LandingPad { clauses } => {
                let block = func.inst_block(inst);
                if func.block_insts(block).first() != Some(&inst) {
                    self.inst_error(
                        inst,
                        "landingpad must be the first instruction of its block".into(),
                    );
                }
                // Only unwind edges lead here: the exception arrives in
                // registers that nothing else sets.
                for pred in self.cfg.preds(block).to_vec() {
                    let term = *func.block_insts(pred).last().expect("a terminator");
                    let unwind = matches!(func.inst_data(term), InstData::Invoke { dests, .. } if dests[1].block == block && dests[0].block != block);
                    if !unwind {
                        self.inst_error(
                            inst,
                            format!("{pred} branches to a landing pad other than by unwinding"),
                        );
                    }
                }
                if let Some(i) = clauses.iter().position(|c| *c == Clause::Cleanup)
                    && i + 1 != clauses.len()
                {
                    self.inst_error(inst, "cleanup must be the last clause".into());
                }
            }
            InstData::EhTypeid { typeinfo } => {
                let caught = func.layout_insts().any(|(_, i)| match func.inst_data(i) {
                    InstData::LandingPad { clauses } => {
                        clauses.contains(&Clause::Catch(Some(*typeinfo)))
                    }
                    _ => false,
                });
                if !caught {
                    self.inst_error(
                        inst,
                        format!(
                            "@{} is not caught by any landing pad in this function",
                            func.symbol_name(crate::Symbol::Data(*typeinfo))
                        ),
                    );
                }
            }
            InstData::ReturnCall { func: callee, args } => {
                let decl = func.callee(*callee);
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if !args_match(&decl.sig, &got) {
                    self.inst_error(
                        inst,
                        format!(
                            "@{} takes ({}), got ({})",
                            decl.name,
                            param_list(&decl.sig),
                            type_list(&got)
                        ),
                    );
                }
                self.check_tail_returns(inst, &decl.sig);
            }
            InstData::ReturnCallIndirect { sig, callee, args } => {
                let sig = func.signature(*sig);
                if ty(*callee) != Type::Ptr {
                    self.inst_error(
                        inst,
                        format!(
                            "return_call_indirect callee must be ptr, got {}",
                            ty(*callee)
                        ),
                    );
                }
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if !args_match(sig, &got) {
                    self.inst_error(
                        inst,
                        format!(
                            "the signature takes ({}), got ({})",
                            param_list(sig),
                            type_list(&got)
                        ),
                    );
                }
                self.check_tail_returns(inst, sig);
            }
            InstData::Return { args } => {
                let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
                if got != func.sig.return_types() {
                    self.inst_error(
                        inst,
                        format!(
                            "returns ({}) but the signature returns ({})",
                            type_list(&got),
                            type_list(&func.sig.return_types())
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
            // An invoke's results exist only once it returns normally.
            ValueDef::Result(def_inst, _)
                if let InstData::Invoke { dests, .. } = func.inst_data(def_inst) =>
            {
                self.domtree.dominates(dests[0].block, use_block)
                    && func.inst_block(def_inst) != use_block
            }
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

/// Checks the ABI attributes of a signature's parameters and returns.
fn check_signature(sig: &Signature) -> Vec<String> {
    let mut errors = Vec::new();
    for param in sig.params.iter().chain(&sig.returns) {
        if param.ext != ArgExt::None && !matches!(param.ty, Type::I8 | Type::I16) {
            errors.push(format!(
                "`{param}`: only i8 and i16 are extended, since C passes them as 32 bits"
            ));
        }
        if let Some(layout) = param.layout() {
            if param.ty != Type::Ptr {
                errors.push(format!("`{param}`: a struct is passed as a ptr to it"));
            }
            if layout.size() == 0 {
                errors.push(format!("`{param}`: the struct is empty"));
            }
        }
    }
    for (i, param) in sig.params.iter().enumerate() {
        if matches!(param.purpose, Purpose::Sret(_)) && (i > 0 || !sig.returns.is_empty()) {
            errors.push(format!(
                "`{param}` must be the first parameter of a function that returns nothing"
            ));
        }
    }
    for ret in &sig.returns {
        if ret.purpose != Purpose::Normal {
            errors.push(format!(
                "`{ret}` cannot be a return value; return structs through an sret parameter"
            ));
        }
    }
    errors
}

/// Whether arguments of types `got` fit `sig`: exactly its parameters, or
/// for a variadic signature its parameters followed by any number of the
/// types C's default argument promotions produce. (An `i8` or `f32` would
/// be read back by the callee as an `int` or `double` it is not.)
fn args_match(sig: &Signature, got: &[Type]) -> bool {
    let params = sig.param_types();
    let n = params.len();
    if !sig.variadic {
        return got == params;
    }
    got.len() >= n
        && got[..n] == params[..]
        && got[n..]
            .iter()
            .all(|ty| matches!(ty, Type::I32 | Type::I64 | Type::Ptr | Type::F64))
}

/// A signature's parameters for messages, with `...` if variadic.
fn param_list(sig: &Signature) -> String {
    let mut list = type_list(&sig.param_types());
    if sig.variadic {
        list.push_str(if list.is_empty() { "..." } else { ", ..." });
    }
    list
}

fn type_list(tys: &[Type]) -> String {
    tys.iter()
        .map(Type::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
