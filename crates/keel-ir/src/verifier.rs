//! IR verifier. Every pass must leave the function in a state this accepts.
//!
//! Checks, in order: block structure (one terminator, at the end), operand
//! and result signatures (including in-place rewrites), block-call arguments, and
//! that every use is dominated by its definition. All errors are collected.
//! The module verifier adds names, data objects and cross-references.

use crate::data::PTR_BYTES;
use crate::dominators::{ControlFlowGraph, DominatorTree};
use crate::fxhash::{FxHashMap, FxHashSet};
use crate::{
    ArgExt, AsmOperand, BinaryOp, Block, BlockCall, CallConv, Clause, ConvertOp, EntityRef,
    Function, InlineAsm, Inst, InstData, Intrinsic, MAX_DATA_ALIGN, Ordering, PhysReg, Purpose,
    RmwOp, Signature, Type, UnaryOp, Value, ValueDef,
};
use crate::{
    DataInit, DataItem, DataLayout, DataObject, DebugScope, DebugType, DebugTypeData,
    FunctionAttrs, Module,
};
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

/// Module name lookups for verification, built once. Each lookup answers
/// exactly as the corresponding linear `Module` method does, including for
/// duplicate names (which are reported separately): `signature` and
/// `function_attrs` take the first `decl`, else the first function; and
/// `data_object` takes the first data object.
struct Names<'m> {
    functions: FxHashMap<&'m str, (&'m Signature, &'m FunctionAttrs)>,
    data: FxHashMap<&'m str, &'m DataObject>,
}

impl<'m> Names<'m> {
    fn new(module: &'m Module) -> Self {
        let mut functions = FxHashMap::with_capacity_and_hasher(
            module.decls.len() + module.functions.len(),
            Default::default(),
        );
        for decl in &module.decls {
            functions
                .entry(decl.name.as_str())
                .or_insert((&decl.sig, &decl.attrs));
        }
        for func in &module.functions {
            functions
                .entry(func.name.as_str())
                .or_insert((&func.sig, &func.attrs));
        }
        let mut data = FxHashMap::with_capacity_and_hasher(module.data.len(), Default::default());
        for object in &module.data {
            data.entry(object.name.as_str()).or_insert(object);
        }
        Self { functions, data }
    }

    fn signature(&self, name: &str) -> Option<&'m Signature> {
        self.functions.get(name).map(|&(sig, _)| sig)
    }

    fn function_attrs(&self, name: &str) -> Option<&FunctionAttrs> {
        self.functions.get(name).map(|&(_, attrs)| attrs)
    }

    fn data_object(&self, name: &str) -> Option<&'m DataObject> {
        self.data.get(name).copied()
    }

    fn has_symbol(&self, name: &str) -> bool {
        self.signature(name).is_some() || self.data_object(name).is_some()
    }
}

/// Verifies every function and data object, that names are unique, that
/// each function's callees and data match the module's declarations, and
/// that instruction locations name the module's source files.
pub fn verify_module(module: &Module) -> Result<(), Vec<VerifierError>> {
    verify_module_with_layout(module, DataLayout::NATIVE64)
}

/// Verifies the actual module using explicit storage properties only for data
/// initializer cursors, RTTI alignment, and relocation target sizes. All
/// structural, function, reference, debug, type and metadata checks are retained.
/// This neither infers a target nor authorizes backend support. RTTI names keep
/// their native eight-byte storage and alignment under either layout.
pub fn verify_module_with_layout(
    module: &Module,
    layout: DataLayout,
) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();
    let lookup = Names::new(module);
    let mut seen = FxHashSet::with_capacity_and_hasher(
        module.decls.len() + module.data.len() + module.functions.len(),
        Default::default(),
    );
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
        if decl.attrs.constructor
            || decl.attrs.destructor
            || decl.attrs.retain
            || decl.attrs.section.is_some()
        {
            errors.push(VerifierError {
                location: format!("@{}", decl.name),
                message: "lifecycle/retain/section requires a definition".into(),
            });
        }
        if decl.sig.cc == CallConv::Fast {
            errors.push(VerifierError {
                location: format!("@{}", decl.name),
                message: "cc(fast) requires a local definition in this module".into(),
            });
        }
        for message in check_signature(&decl.sig).into_iter().chain(
            decl.attrs
                .validate(decl.sig.returns_twice)
                .err()
                .map(str::to_owned),
        ) {
            errors.push(VerifierError {
                location: format!("@{}", decl.name),
                message,
            });
        }
    }
    for decl in &module.decls {
        if let Err(message) = decl.attrs.contract.validate(&decl.sig) {
            errors.push(VerifierError {
                location: format!("@{}", decl.name),
                message,
            });
        }
    }
    for data in &module.data {
        if (data.retain || data.section.is_some()) && (data.weak_odr || data.init.is_none()) {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "section/retain requires a non-COMDAT definition".into(),
            });
        }
        if data.weak && (data.local || data.thread_local || data.weak_odr) {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "weak requires nonlocal non-TLS storage and cannot combine with weak_odr"
                    .into(),
            });
        }
        if data.hidden && (data.local || data.init.is_none()) {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "hidden requires a nonlocal definition".into(),
            });
        }
        if data.weak_odr && (data.local || data.init.is_none() || data.thread_local) {
            errors.push(VerifierError {
                location: format!("@{}", data.name),
                message: "weak_odr requires a nonlocal, non-thread-local definition".into(),
            });
        }
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
            for message in check_data(&lookup, init, layout) {
                errors.push(VerifierError {
                    location: format!("@{}", data.name),
                    message,
                });
            }
        }
    }
    errors.extend(check_debug_info(module, &lookup));
    let mut scratch = Scratch::default();
    for func in &module.functions {
        if func
            .personality
            .as_ref()
            .and_then(|p| lookup.signature(p))
            .is_some_and(|s| s.cc == CallConv::Fast)
        {
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message: "cc(fast) cannot be an exception personality".into(),
            });
        }
        if let Some(personality) = &func.personality
            && lookup.signature(personality).is_none()
        {
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message: format!("personality @{personality} is not a declared function"),
            });
        }
        // An instruction can have a bad location only if the table has one.
        let bad_file = |loc: &crate::SourceLoc| loc.file.index() >= module.files.len();
        let suspects = func
            .locs()
            .iter()
            .any(bad_file)
            .then(|| func.layout_insts());
        for (_, inst) in suspects.into_iter().flatten() {
            if let Some(loc) = func.inst_loc(inst)
                && bad_file(&loc)
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
            if lookup.data_object(name).is_none() {
                errors.push(VerifierError {
                    location: format!("@{}", func.name),
                    message: format!("uses data @{name}, which is not declared"),
                });
            }
        }
        if let Err(func_errors) = verify_with(func, &mut scratch) {
            errors.extend(func_errors.into_iter().map(|e| VerifierError {
                location: format!("@{} {}", func.name, e.location),
                message: e.message,
            }));
        }
        for (_, callee) in func.callees() {
            if lookup
                .function_attrs(&callee.name)
                .is_some_and(|a| a != &callee.attrs)
            {
                errors.push(VerifierError {
                    location: format!("@{}", func.name),
                    message: format!("callee @{} attributes disagree with module", callee.name),
                });
            }
            let message = match lookup.signature(&callee.name) {
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
fn check_debug_info(module: &Module, lookup: &Names<'_>) -> Vec<VerifierError> {
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
        // Its size is the base type's.
        if let DebugTypeData::Enum { ty, .. } = data
            && known(*ty)
            && !matches!(module.debug_types[ty.index()], DebugTypeData::Base { .. })
        {
            errors.push(VerifierError {
                location: DebugType::new(i).to_string(),
                message: format!("is an enum of {ty}, which is not a base type"),
            });
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
        if lookup.data_object(&global.symbol).is_none() {
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
        if let Some(loc) = func.debug_loc
            && loc.file.index() >= module.files.len()
        {
            errors.push(VerifierError {
                location: format!("@{}", func.name),
                message: format!(
                    "is declared at {loc}, but the module has no file {}",
                    loc.file
                ),
            });
        }
        let scopes = func.debug_scopes.len();
        for (i, parent) in func.debug_scopes.iter().enumerate() {
            if let Some(parent) = parent
                && parent.index() >= i
            {
                errors.push(VerifierError {
                    location: format!("@{} {}", func.name, DebugScope::new(i)),
                    message: format!("is in {parent}, which does not come before it"),
                });
            }
        }
        let bad_scope = |loc: &crate::SourceLoc| loc.scope.is_some_and(|s| s.index() >= scopes);
        let suspects = func
            .locs()
            .iter()
            .any(bad_scope)
            .then(|| func.layout_insts());
        for (_, inst) in suspects.into_iter().flatten() {
            if let Some(scope) = func.inst_loc(inst).and_then(|loc| loc.scope)
                && scope.index() >= scopes
            {
                errors.push(VerifierError {
                    location: format!("@{} {inst}", func.name),
                    message: format!("is in {scope}, which is not defined"),
                });
            }
        }
        for var in &func.debug_vars {
            let location = format!("@{} {var}", func.name);
            if let Some(scope) = var.scope
                && scope.index() >= scopes
            {
                errors.push(VerifierError {
                    location: location.clone(),
                    message: format!("is in {scope}, which is not defined"),
                });
            }
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
            } else if var.indirect
                && func
                    .stack_slots()
                    .any(|(slot, data)| slot == var.slot && data.size < 8)
            {
                errors.push(VerifierError {
                    location,
                    message: "an indirect debug location needs a pointer-sized stack slot".into(),
                });
            }
        }
    }
    errors
}

fn check_data(lookup: &Names<'_>, init: &DataInit, layout: DataLayout) -> Vec<String> {
    let mut errors = Vec::new();
    if !init.align.is_power_of_two() || init.align > MAX_DATA_ALIGN {
        errors.push(format!(
            "alignment must be a power of two up to {MAX_DATA_ALIGN}, got {}",
            init.align
        ));
    }
    if init.checked_size_with_layout(layout) == Some(0) {
        // Two empty objects could share an address, which is surprising.
        errors.push("data object is empty".into());
    }
    let mut offset = Some(0u64);
    let mut reported_pointer_align = 0;
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
            DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } => {
                let addend = match item {
                    DataItem::Ptr { addend, .. } => *addend,
                    _ => 0,
                };
                if matches!(item, DataItem::RttiName { .. }) && lookup.data_object(symbol).is_none()
                {
                    errors.push(format!("rtti_name requires a data symbol, got @{symbol}"));
                }
                if lookup
                    .signature(symbol)
                    .is_some_and(|s| s.cc == CallConv::Fast)
                {
                    errors.push(format!(
                        "cc(fast) function @{symbol} cannot have its address taken"
                    ));
                }
                if !lookup.has_symbol(symbol) {
                    errors.push(format!("points to @{symbol}, which is not declared"));
                } else if lookup.data_object(symbol).is_some_and(|d| d.thread_local) {
                    errors.push(format!(
                        "points to thread-local @{symbol}, whose address differs per thread"
                    ));
                } else if let Some(target) =
                    lookup.data_object(symbol).and_then(|d| d.init.as_ref())
                {
                    match target.checked_size_with_layout(layout) {
                        Some(size) if u64::from(addend) > size => errors.push(format!(
                            "@{symbol} + {addend} is past the end of @{symbol} ({size} bytes)"
                        )),
                        None => errors.push(format!("target @{symbol} size overflows u64")),
                        Some(_) => {}
                    }
                }
                // Ordinary pointers are byte storage relocations. Native RTTI
                // tagging retains its separate eight-byte alignment contract.
                if matches!(item, DataItem::RttiName { .. }) {
                    if let Some(offset) = offset.filter(|offset| !offset.is_multiple_of(PTR_BYTES))
                    {
                        errors.push(format!(
                            "ptr @{symbol} is at offset {offset}, which is not a multiple of {PTR_BYTES}"
                        ));
                    }
                    if u64::from(init.align) < PTR_BYTES && reported_pointer_align < PTR_BYTES {
                        errors.push(format!(
                            "holds pointers, so it needs align {PTR_BYTES} or more, got {}",
                            init.align
                        ));
                        reported_pointer_align = PTR_BYTES;
                    }
                }
            }
            DataItem::Bytes(_) | DataItem::Zero(_) => {}
        }
        if let Some(current) = offset {
            offset = match checked_data_offset(current, item.size_with_layout(layout)) {
                Ok(next) => Some(next),
                Err(message) => {
                    errors.push(message.into());
                    None // Keep checking items, but do not invent a wrapped cursor.
                }
            };
        }
    }
    errors
}

// Kept separate so the otherwise impractically large initializer overflow
// boundary can be exercised without allocating billions of actual items.
fn checked_data_offset(offset: u64, size: u64) -> Result<u64, &'static str> {
    offset
        .checked_add(size)
        .ok_or("data object size overflows u64")
}

/// Check handles before building a CFG, resolving operands or formatting an
/// instruction: each of those otherwise assumes in-range references. Passes
/// can invalidate handles through `inst_data_mut`; report errors, not panics.
/// Fills `inserted` with whether each block is in the layout, and returns
/// whether any instruction branches to the entry block.
fn check_references(func: &Function, inserted: &mut Vec<bool>) -> Result<bool, Vec<VerifierError>> {
    let mut errors = Vec::new();
    let callees = func.callees().count();
    let signatures = func.signatures().count();
    let symbols = func.data_symbols().count();
    inserted.clear();
    inserted.resize(func.num_blocks(), false);
    for block in func.blocks() {
        inserted[block.index()] = true;
    }
    let entry = func.entry_block();
    let mut entry_is_target = false;
    for (_, inst) in func.layout_insts() {
        let mut bad = |kind: &str, index: usize| {
            errors.push(VerifierError {
                location: inst.to_string(),
                message: format!("{kind} reference {index} is not in the function"),
            })
        };
        let data = func.inst_data(inst);
        let values = func.num_values();
        let mut out_of_range = false;
        data.for_each_operand(|value| out_of_range |= value.index() >= values);
        if out_of_range {
            data.for_each_operand(|value| {
                if value.index() >= values {
                    bad("value", value.index());
                }
            });
        }
        for dest in data.branch_dests() {
            if inserted.get(dest.block.index()) != Some(&true) {
                bad("block", dest.block.index());
            }
            entry_is_target |= Some(dest.block) == entry;
        }
        let mut callee = |reference: crate::FuncRef| {
            if reference.index() >= callees {
                bad("function", reference.index());
            }
        };
        match data {
            InstData::Call { func, .. }
            | InstData::Invoke { func, .. }
            | InstData::ReturnCall { func, .. }
            | InstData::SymbolAddr {
                symbol: crate::Symbol::Func(func),
            } => callee(*func),
            InstData::CallIndirect { sig, .. } | InstData::ReturnCallIndirect { sig, .. } => {
                if sig.index() >= signatures {
                    bad("signature", sig.index());
                }
            }
            InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                if slot.index() >= func.num_stack_slots() {
                    bad("stack slot", slot.index());
                }
            }
            InstData::SymbolAddr {
                symbol: crate::Symbol::Data(symbol),
            }
            | InstData::EhTypeid { typeinfo: symbol } => {
                if symbol.index() >= symbols {
                    bad("data symbol", symbol.index());
                }
            }
            InstData::LandingPad { clauses } => {
                for clause in clauses {
                    if let Clause::Catch(Some(symbol)) = clause
                        && symbol.index() >= symbols
                    {
                        bad("data symbol", symbol.index());
                    }
                }
            }
            // Checked here, before anything formats the instruction, since
            // printing a switch reads its default destination.
            InstData::Switch { dests, cases, .. } if dests.len() != cases.len() + 1 => {
                errors.push(VerifierError {
                    location: inst.to_string(),
                    message: format!(
                        "switch has {} destinations for {} cases, expected {}",
                        dests.len(),
                        cases.len(),
                        cases.len() + 1
                    ),
                });
            }
            _ => {}
        }
    }
    if errors.is_empty() {
        Ok(entry_is_target)
    } else {
        Err(errors)
    }
}

/// Buffers for verifying a function, reused across a module's functions.
#[derive(Default)]
struct Scratch {
    /// Whether each block is in the layout.
    inserted: Vec<bool>,
    /// For each instruction, the number of the block visit that reached it
    /// (from 1), so a use in the same block can tell if its definition came
    /// first.
    visited: Vec<u32>,
    operands: Vec<Value>,
    cases: Vec<u64>,
}

/// Verifies one function on its own; calls are checked against the
/// signatures in its callee table.
pub fn verify(func: &Function) -> Result<(), Vec<VerifierError>> {
    verify_with(func, &mut Scratch::default())
}

fn verify_with(func: &Function, scratch: &mut Scratch) -> Result<(), Vec<VerifierError>> {
    let entry_is_target = check_references(func, &mut scratch.inserted)?;
    let mut v = Verifier {
        func,
        dom: None,
        scratch,
        errors: Vec::new(),
    };
    if func.weak_odr && (func.attrs.retain || func.attrs.section.is_some()) {
        v.error(
            "placement".into(),
            "section/retain with COMDAT is unsupported".into(),
        );
    }
    if func.sig.cc == CallConv::Fast && !func.local {
        v.error(
            "linkage".into(),
            "cc(fast) requires a local definition".into(),
        );
    }
    if (func.attrs.constructor || func.attrs.destructor)
        && (!func.sig.params.is_empty()
            || !func.sig.returns.is_empty()
            || func.sig.variadic
            || func.sig.cc != CallConv::C
            || func.sig.returns_twice
            || func.attrs.noreturn)
    {
        v.error(
            "lifecycle".into(),
            "lifecycle callback requires normally returning C void(void)".into(),
        );
    }
    if func.attrs.weak && (func.local || func.weak_odr) {
        v.error("linkage".into(), "weak cannot be local or weak_odr".into());
    }
    if func.weak_odr && func.local {
        v.error("linkage".into(), "weak_odr cannot be local".into());
    }
    if func.hidden && func.local {
        v.error("linkage".into(), "hidden cannot be local".into());
    }
    if let Err(message) = func.attrs.validate(func.sig.returns_twice) {
        v.error("attributes".into(), message.into());
    }
    if let Err(message) = func.attrs.contract.validate(&func.sig) {
        v.error("contract".into(), message);
    }
    for (_, decl) in func.callees() {
        if let Err(message) = decl.attrs.contract.validate(&decl.sig) {
            v.error(format!("callee @{}", decl.name), message);
        }
        if let Err(message) = decl.attrs.validate(decl.sig.returns_twice) {
            v.error(format!("callee @{}", decl.name), message.into());
        }
    }
    let mut seen = std::collections::HashSet::new();
    for &index in &func.restrict_params {
        let valid = func
            .sig
            .params
            .get(index as usize)
            .is_some_and(|p| p.ty == Type::Ptr && p.purpose == crate::Purpose::Normal);
        if !valid || !seen.insert(index) {
            v.error(
                "restrict_params".into(),
                format!("invalid or duplicate restrict parameter index {index}"),
            );
        }
    }
    v.run(entry_is_target);
    if v.errors.is_empty() {
        Ok(())
    } else {
        Err(v.errors)
    }
}

struct Verifier<'a> {
    func: &'a Function,
    /// Built on first need: most uses are in their definition's block or
    /// read an entry block value, which need no dominator tree.
    dom: Option<(ControlFlowGraph, DominatorTree)>,
    scratch: &'a mut Scratch,
    errors: Vec<VerifierError>,
}

impl Verifier<'_> {
    fn dom(&mut self) -> &(ControlFlowGraph, DominatorTree) {
        let func = self.func;
        self.dom.get_or_insert_with(|| {
            let cfg = ControlFlowGraph::compute(func);
            let domtree = DominatorTree::compute(func, &cfg);
            (cfg, domtree)
        })
    }

    fn run(&mut self, entry_is_target: bool) {
        let func = self.func;
        let Some(entry) = func.entry_block() else {
            self.error("function".into(), "function has no blocks".into());
            return;
        };

        let param_tys = || func.block_params(entry).iter().map(|&p| func.value_type(p));
        if !param_tys().eq(func.sig.params.iter().map(|p| p.ty)) {
            let param_tys: Vec<Type> = param_tys().collect();
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
            if !data.align.is_power_of_two() || data.align > 64 {
                self.error(
                    ss.to_string(),
                    format!(
                        "alignment must be a power of two up to 64, got {}",
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
            if sig.cc == CallConv::Fast {
                self.error(
                    sig_ref.to_string(),
                    "cc(fast) does not support indirect calls".into(),
                );
            }
            for message in check_signature(sig) {
                self.error(sig_ref.to_string(), message);
            }
        }
        if entry_is_target {
            self.error(
                entry.to_string(),
                "the entry block cannot be a branch target".into(),
            );
        }

        self.scratch.visited.clear();
        self.scratch.visited.resize(func.num_insts(), 0);
        let mut operands = std::mem::take(&mut self.scratch.operands);
        for (visit, block) in func.blocks().enumerate() {
            let visit = u32::try_from(visit + 1).expect("block count overflow");
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
                operands.clear();
                func.inst_data(inst).for_each_operand(|v| {
                    let v = func.resolve(v);
                    if !operands.contains(&v) {
                        operands.push(v);
                    }
                });
                for &value in &operands {
                    self.check_dominance(inst, block, visit, value);
                }
                self.scratch.visited[inst.index()] = visit;
            }
        }
        self.scratch.operands = operands;
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
        // Explicit producers may bridge only when the target backend proves
        // compatible physical result and stack plans. Untagged policy cannot
        // supply that proof for a differently tagged endpoint.
        if callee.x86_vector_abi != self.func.sig.x86_vector_abi
            && (callee.x86_vector_abi.is_none() || self.func.sig.x86_vector_abi.is_none())
        {
            self.inst_error(
                inst,
                "tail calls require matching x86 vector ABI policies".into(),
            );
        }
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
    /// and tied inputs share their output's class. Scalar GPR ties may have
    /// different widths; each output may be initialized only once.
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
        if let Err(message) = asm.stack_pointer_input() {
            self.inst_error(inst, message);
        }
        let mut early = Vec::new();
        for &out in &asm.early_clobbers {
            if out as usize >= asm.outputs.len() || early.contains(&out) {
                self.inst_error(inst, "invalid or duplicate asm early-clobber output".into());
            }
            early.push(out);
        }
        let fits = |op: AsmOperand, ty: Type| match op {
            AsmOperand::Reg
            | AsmOperand::RegImm
            | AsmOperand::RegSignedImm32
            | AsmOperand::RegShiftImm5
            | AsmOperand::PortImm8
            | AsmOperand::Fixed(PhysReg { float: false, .. }) => {
                (ty.is_int() && ty != Type::I128) || ty == Type::Ptr
            }
            AsmOperand::StackPointer => ty == Type::I64,
            AsmOperand::FloatReg | AsmOperand::Fixed(PhysReg { float: true, .. }) => {
                ty.is_float() || ty.is_vector()
            }
            AsmOperand::Mem | AsmOperand::MemWrite | AsmOperand::MemReadWrite => ty == Type::Ptr,
            AsmOperand::Flag(cc) => cc < 16 && ty.is_int() && ty != Type::I128,
            AsmOperand::Imm | AsmOperand::NumericImm | AsmOperand::Tied(_) => true,
        };
        for (n, &(op, ty)) in asm.outputs.iter().enumerate() {
            if !matches!(
                op,
                AsmOperand::Reg | AsmOperand::FloatReg | AsmOperand::Fixed(_) | AsmOperand::Flag(_)
            ) || matches!(op, AsmOperand::Flag(_)) && early.contains(&(n as u16))
            {
                self.inst_error(inst, format!("invalid asm output {n} (`{op}`)"));
            }
            if !fits(op, ty) {
                self.inst_error(inst, format!("asm operand {n} (`{op}`) cannot be {ty}"));
            }
        }
        let mut tied: Vec<(u16, Value)> = Vec::new();
        for (i, (&op, &arg)) in asm.inputs.iter().zip(args).enumerate() {
            let n = asm.outputs.len() + i;
            let ty = func.value_type(func.resolve(arg));
            if !fits(op, ty) {
                self.inst_error(inst, format!("asm operand {n} (`{op}`) cannot be {ty}"));
            }
            match op {
                AsmOperand::StackPointer => {
                    if !crate::asm::is_stack_pointer_snapshot(func, arg) {
                        self.inst_error(
                            inst,
                            "rsp stack dependency requires a StackSave-to-i64 snapshot".into(),
                        );
                    }
                }
                AsmOperand::Flag(_) => {
                    self.inst_error(inst, "flag constraints require an output".into())
                }
                AsmOperand::Imm | AsmOperand::NumericImm => {
                    let is_const = matches!(
                        func.value_def(func.resolve(arg)),
                        ValueDef::Result(def, _) if matches!(func.inst_data(def), InstData::Iconst { .. })
                    );
                    if !is_const
                        && !(op == AsmOperand::Imm
                            && crate::asm::symbolic_immediate(func, arg).is_some())
                    {
                        self.inst_error(
                            inst,
                            format!("asm operand {n} (`{op}`) requires a constant immediate"),
                        );
                    }
                }
                AsmOperand::Tied(out) => match asm.outputs.get(out as usize) {
                    None => self.inst_error(
                        inst,
                        format!("asm operand {n} is tied to output {out}, which does not exist"),
                    ),
                    Some(&(kind, out_ty))
                        if out_ty != ty
                            && !(matches!(
                                kind,
                                AsmOperand::Reg | AsmOperand::Fixed(PhysReg { float: false, .. })
                            ) && fits(kind, ty)) =>
                    {
                        self.inst_error(
                            inst,
                            format!("asm operand {n} is {ty}, but its output {out} is {out_ty}"),
                        )
                    }
                    Some(&(AsmOperand::Flag(_), _)) => {
                        self.inst_error(inst, "asm ties require a register output".into())
                    }
                    Some(_) => {
                        for &(previous_out, previous_arg) in &tied {
                            if previous_out == out
                                && !crate::asm::same_register_input(func, previous_arg, arg)
                            {
                                self.inst_error(inst, format!(
                                    "output {out} has conflicting or unproven matching input values"
                                ));
                            }
                        }
                        tied.push((out, arg));
                    }
                },
                _ => {}
            }
        }
    }

    fn check_types(&mut self, inst: Inst) {
        let func = self.func;
        if let Some(facts) = func.inst_facts(inst) {
            match func.inst_results(inst) {
                [value] => {
                    if let Err(message) = facts.validate(func.value_type(*value)) {
                        self.inst_error(inst, message);
                    }
                }
                _ => self.inst_error(inst, "result facts require exactly one result".into()),
            }
        }
        if let Some(contract) = func.call_contract(inst) {
            match func.call_signature(inst) {
                Some(sig) => {
                    if let Err(message) = contract.validate(sig) {
                        self.inst_error(inst, message);
                    }
                }
                None => self.inst_error(
                    inst,
                    "call-site contract requires a call instruction".into(),
                ),
            }
        }
        let ty = |v: Value| func.value_type(func.resolve(v));
        let mut expected = 0;
        let results = func.inst_results(inst);
        func.for_each_result_type(func.inst_data(inst), |expected_ty| {
            if let Some(&value) = results.get(expected)
                && func.value_type(value) != expected_ty
            {
                self.inst_error(
                    inst,
                    format!(
                        "result {expected} must have type {expected_ty}, got {}",
                        func.value_type(value)
                    ),
                );
            }
            expected += 1;
        });
        if results.len() != expected {
            self.inst_error(
                inst,
                format!(
                    "instruction must have {expected} result(s), got {}",
                    results.len()
                ),
            );
        }
        if let InstData::Load { flags, .. } | InstData::Store { flags, .. } = func.inst_data(inst)
            && !flags.align.is_power_of_two()
        {
            self.inst_error(
                inst,
                format!(
                    "memory alignment must be a nonzero power of two, got {}",
                    flags.align
                ),
            );
        }
        match func.inst_data(inst) {
            InstData::VectorSplat { ty: to, arg } => {
                if to.vector().is_none_or(|v| v.lane_type() != ty(*arg)) {
                    self.inst_error(
                        inst,
                        format!(
                            "vsplat needs a vector result and matching scalar lane, got {} to {to}",
                            ty(*arg)
                        ),
                    );
                }
            }
            InstData::Neon {
                op,
                ty: result,
                args,
            } => {
                if !op.accepts(*result)
                    || args.len() != op.arity_for(*result)
                    || op
                        .operand_types(*result)
                        .is_none_or(|expected| args.iter().map(|v| ty(*v)).ne(expected))
                {
                    self.inst_error(
                        inst,
                        format!("{op} has invalid result/operand shapes or arity"),
                    );
                }
                if let Some(range) = op.immediate_range(*result) {
                    let imm = args.last().and_then(|&v| {
                        let crate::ValueDef::Result(i, _) = func.value_def(func.resolve(v)) else {
                            return None;
                        };
                        match *func.inst_data(i) {
                            InstData::Iconst { ty: Type::I32, imm } => Some(imm),
                            _ => None,
                        }
                    });
                    if imm.is_none_or(|n| !range.contains(&n)) {
                        self.inst_error(
                            inst,
                            format!("{op} requires a literal i32 shift in {range:?}"),
                        );
                    }
                }
            }
            InstData::VectorReduce { op, arg } => {
                if ty(*arg).vector().is_none_or(|s| !s.lane_type().is_int()) {
                    self.inst_error(inst, format!("{op} needs integer vector lanes"));
                }
            }
            InstData::VectorUnary { op, arg } => {
                let t = ty(*arg);
                if t.vector().is_none_or(|v| {
                    v.lane_type().is_float() != op.is_float()
                        || *op == UnaryOp::Bswap && v.lane_type() == Type::I8
                }) {
                    self.inst_error(
                        inst,
                        format!("v{op} needs compatible vector lanes, got {t}"),
                    );
                }
            }
            InstData::VectorBinary { op, args } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a != b
                    || a.vector()
                        .is_none_or(|v| v.lane_type().is_float() != op.is_float())
                {
                    self.inst_error(
                        inst,
                        format!(
                            "{op} needs equal vector shapes with compatible lanes, got ({a}, {b})"
                        ),
                    );
                }
            }
            InstData::VectorExtract { arg, lane } => {
                let t = ty(*arg);
                if t.vector().is_none_or(|v| *lane >= v.lanes()) {
                    self.inst_error(inst, format!("vextract lane {lane} is invalid for {t}"));
                }
            }
            InstData::VectorInsert { args, lane } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a.vector()
                    .is_none_or(|v| *lane >= v.lanes() || b != v.lane_type())
                {
                    self.inst_error(inst, format!("vinsert needs a valid lane and matching scalar, got ({a}, {b}), lane {lane}"));
                }
            }
            InstData::VectorShuffle { args, lanes } => {
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a != b
                    || a.vector().is_none_or(|v| {
                        lanes.len() != usize::from(v.lanes())
                            || lanes.iter().any(|i| *i >= 2 * v.lanes())
                    })
                {
                    self.inst_error(inst, format!("vshuffle needs equal vectors, one index per output lane and indices below twice the lane count, got ({a}, {b}), {lanes:?}"));
                }
            }
            InstData::VectorIcmp { args, .. } | InstData::VectorFcmp { args, .. } => {
                let float = matches!(func.inst_data(inst), InstData::VectorFcmp { .. });
                let (a, b) = (ty(args[0]), ty(args[1]));
                if a != b || a.vector().is_none_or(|v| v.lane_type().is_float() != float) {
                    self.inst_error(inst, format!("vector comparison needs equal vectors with compatible lanes, got ({a}, {b})"));
                }
            }
            InstData::VectorSelect { args } => {
                let (mask, a, b) = (ty(args[0]), ty(args[1]), ty(args[2]));
                if a != b || a.vector().is_none_or(|v| mask != Type::Vector(v.mask())) {
                    self.inst_error(inst, format!("vselect needs equal vector arms and an integer mask of the same lane shape, got ({mask}, {a}, {b})"));
                }
            }
            InstData::Iconst { ty, .. } => {
                if !ty.is_int() {
                    self.inst_error(inst, format!("iconst type must be an integer, got {ty}"));
                }
            }
            InstData::F80const { .. } => {}
            InstData::Fconst { ty, bits } => {
                if !matches!(ty, Type::F32 | Type::F64) {
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
                if !(a.is_float() || a.vector().is_some_and(|s| s.lane_type().is_float()))
                    || a != b
                    || a != c
                {
                    self.inst_error(
                        inst,
                        format!("fma takes three floating-point scalars or vectors of the same type, got ({a}, {b}, {c})"),
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
            InstData::AggregateVaArg {
                layout,
                list,
                home,
                x86_vector_abi,
            } => {
                let Some(layout_size) = layout.checked_size() else {
                    self.inst_error(inst, "aggregate va_arg layout size overflows u64".into());
                    return;
                };
                let slot = func
                    .stack_slots()
                    .nth(home.index())
                    .expect("checked slot reference")
                    .1;
                if u64::from(slot.size) < layout_size || u64::from(slot.align) < layout.align() {
                    self.inst_error(
                        inst,
                        "aggregate va_arg home is too small or under-aligned".into(),
                    );
                }
                if let Some((base, offset)) = local_aggregate_address(func, *list)
                    && base == *home
                    && offset < layout_size as i64
                    && offset.checked_add(24).is_some_and(|end| end > 0)
                {
                    self.inst_error(
                        inst,
                        "aggregate va_arg home overlaps its va_list object".into(),
                    );
                }
                if ty(*list) != Type::Ptr || (layout_size == 0 && x86_vector_abi.is_none()) {
                    self.inst_error(
                        inst,
                        "aggregate va_arg requires a ptr list and nonempty layout".into(),
                    );
                }
                if let Some(abi) = x86_vector_abi
                    && (!(layout_contains_f64x1(layout) || layout_contains_zmm(layout))
                        || !func.has_x86_va_arg_provenance(inst, *abi))
                {
                    self.inst_error(inst, "tagged aggregate va_arg requires an f64x1 or 512-bit vector layout and matching local current-signature provenance".into());
                }
            }
            InstData::VaArg {
                ty: t,
                list,
                x86_vector_abi,
            } => {
                if let Some(abi) = x86_vector_abi {
                    if *t != Type::Vector(crate::VectorType::F64x1) {
                        self.inst_error(
                            inst,
                            "x86 va_arg producer tag is only supported for f64x1".into(),
                        );
                    }
                    if !func.has_x86_va_arg_provenance(inst, *abi) {
                        self.inst_error(inst, "tagged va_arg requires a matching variadic C signature and uninterrupted local va_start provenance".into());
                    }
                }
                if !matches!(
                    t,
                    Type::I32 | Type::I64 | Type::I128 | Type::Ptr | Type::F64 | Type::F80
                ) && !t.is_vector()
                {
                    self.inst_error(
                        inst,
                        format!("va_arg reads C promoted scalars or vectors, got {t}"),
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
                let arg_types = || args.iter().map(|&a| ty(a));
                self.call_abi(inst, &decl.sig, args);
                if !args_match(&decl.sig, arg_types()) {
                    let got: Vec<Type> = arg_types().collect();
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
                let arg_types = || args.iter().map(|&a| ty(a));
                self.call_abi(inst, sig, args);
                if !args_match(sig, arg_types()) {
                    let got: Vec<Type> = arg_types().collect();
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
            InstData::SymbolAddr {
                symbol: crate::Symbol::Func(callee),
            } if func.callee(*callee).sig.cc == CallConv::Fast => {
                self.inst_error(
                    inst,
                    "cc(fast) functions cannot have their address taken".into(),
                );
            }
            InstData::SymbolAddr { .. } => {}
            InstData::Intrinsic { id, args } => {
                if matches!(id, Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd) {
                    let fixed_slot =
                        args.first()
                            .is_some_and(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(owner, 0) => match func.inst_data(owner) {
                                    InstData::StackAddr { slot } => {
                                        slot.index() < func.num_stack_slots()
                                    }
                                    _ => false,
                                },
                                _ => false,
                            });
                    if !fixed_slot {
                        self.inst_error(
                            inst,
                            "lifetime marker requires a direct fixed stack slot address".into(),
                        );
                    }
                }
                if id.x86_port_io().is_some_and(|(_, _, immediate)| immediate) {
                    let port = args
                        .first()
                        .and_then(|&v| match func.value_def(func.resolve(v)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I16, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !port.is_some_and(|n| (0..=255).contains(&n)) {
                        self.inst_error(
                            inst,
                            "port I/O immediate requires iconst.i16 in0..=255".into(),
                        );
                    }
                }
                if *id == Intrinsic::VaStart && !func.sig.variadic {
                    self.inst_error(inst, "va_start in a function that is not variadic".into());
                }
                if id.x86_packed_search().is_some_and(|(_, _, sad)| sad) {
                    let control =
                        args.get(2)
                            .and_then(|&v| match func.value_def(func.resolve(v)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(inst, "MPSADBW requires iconst.i32 in0..255".into());
                    }
                }
                if id.x86_byte_shift().is_some() {
                    let count = args
                        .get(1)
                        .and_then(|&v| match func.value_def(func.resolve(v)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !count.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(
                            inst,
                            "packed byte shift requires iconst.i32 in0..255".into(),
                        );
                    }
                }
                if id.is_x86_fp_compare() {
                    let mode = args
                        .get(2)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !mode.is_some_and(|n| (0..32).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX compare predicate must be iconst.i32 in0..=31".into(),
                        );
                    }
                }
                if id.has_x86_round_control() {
                    let mode = args
                        .get(1)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !mode.is_some_and(|n| (0..16).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX round control must be iconst.i32 in0..=15".into(),
                        );
                    }
                }
                if id.is_x86_legacy_fp_compare() {
                    let mode = args
                        .get(2)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !mode.is_some_and(|n| (0..8).contains(&n)) {
                        self.inst_error(
                            inst,
                            "legacy SSE compare predicate must be iconst.i32 in0..=7".into(),
                        );
                    }
                }
                if *id == Intrinsic::X86Aeskeygenassist128 {
                    let (index, limit) = (1, 256);
                    let mode =
                        args.get(index)
                            .and_then(|&a| match func.value_def(func.resolve(a)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !mode.is_some_and(|n| (0..limit).contains(&n)) {
                        self.inst_error(
                            inst,
                            format!(
                                "AESKEYGENASSIST control must be iconst.i32 in0..{}",
                                limit - 1
                            ),
                        );
                    }
                }
                if let Some((index, limit)) = id.x86_legacy_fp_v2_control() {
                    let mode =
                        args.get(index)
                            .and_then(|&a| match func.value_def(func.resolve(a)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !mode.is_some_and(|n| (0..limit).contains(&n)) {
                        self.inst_error(
                            inst,
                            format!("legacy FP control must be iconst.i32 in0..{}", limit - 1),
                        );
                    }
                }
                if let Some((_, index, _)) = id.x86_string_info() {
                    let mode =
                        args.get(index)
                            .and_then(|&a| match func.value_def(func.resolve(a)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !mode.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(
                            inst,
                            "packed string control must be iconst.i32 in 0..=255".into(),
                        );
                    }
                }
                if let Some((_, immediate, control)) = id.x86_avx512_special() {
                    for index in immediate.into_iter().chain(control) {
                        let value = args.get(index).and_then(|&arg| {
                            match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            }
                        });
                        if !value
                            .is_some_and(|value| id.valid_x86_avx512_special_control(index, value))
                        {
                            self.inst_error(
                                inst,
                                "invalid AVX512 floating immediate or SAE/rounding control".into(),
                            );
                        }
                    }
                }
                if let Some((index, max)) = id.x86_avx512_lane_control() {
                    let control =
                        args.get(index)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..=max).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX512 lane control must be constant in its instruction range".into(),
                        );
                    }
                }
                if id.has_x86_avx512_mask_shift_control() {
                    let control =
                        args.get(1)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(
                            inst,
                            "word mask shift control must be iconst.i32 in 0..=255".into(),
                        );
                    }
                }
                if let Some(index) = id.x86_avx512_integer_control() {
                    let control =
                        args.get(index)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX512 integer control must be iconst.i32 in 0..=255".into(),
                        );
                    }
                }
                if let Some(index) = id.x86_avx512_packed_control() {
                    let value =
                        args.get(index)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !value.is_some_and(|value| id.valid_x86_avx512_packed_control(value)) {
                        self.inst_error(inst, "invalid packed AVX512 SAE/rounding control".into());
                    }
                }
                if let Some((_, index)) = id.x86_avx512_scalar_convert() {
                    let value =
                        args.get(index)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !value.is_some_and(|value| id.valid_x86_avx512_scalar_convert_control(value))
                    {
                        self.inst_error(
                            inst,
                            "invalid scalar EVEX conversion/flag rounding or SAE control".into(),
                        );
                    }
                }
                if let Some((_, immediate, control)) = id.x86_avx512_scalar() {
                    for index in immediate.into_iter().chain(control) {
                        let value = args.get(index).and_then(|&arg| {
                            match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            }
                        });
                        if !value
                            .is_some_and(|value| id.valid_x86_avx512_scalar_control(index, value))
                        {
                            self.inst_error(
                                inst,
                                "invalid scalar EVEX immediate or SAE/rounding control".into(),
                            );
                        }
                    }
                }
                if let Some(index) = id
                    .x86_avx512_permute()
                    .and_then(|(_, kind)| kind.control_index())
                {
                    let control =
                        args.get(index)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..=255).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX512 permutation control must be iconst.i32 in 0..=255".into(),
                        );
                    }
                }
                if id.x86_avx512_fma_opcode().is_some()
                    || id.x86_avx512_scalar_fma_opcode().is_some()
                {
                    let control =
                        args.get(5)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| matches!(n, 4 | 8..=11)) {
                        self.inst_error(
                            inst,
                            "AVX512 FMA control must be iconst.i32 4 or8..=11".into(),
                        );
                    }
                }
                if id.is_x86_avx512_convert() {
                    let control =
                        args.get(3)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| id.valid_x86_avx512_convert_control(n)) {
                        self.inst_error(
                            inst,
                            "invalid AVX512 conversion rounding/SAE control".into(),
                        );
                    }
                }
                if id.is_x86_avx512_float_compare() {
                    let control =
                        args.get(3)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..32).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX512 floating predicate must be iconst.i32 in0..=31".into(),
                        );
                    }
                }
                if matches!(
                    id,
                    Intrinsic::X86Avx512CmpRoundps | Intrinsic::X86Avx512CmpRoundpd
                ) {
                    let control =
                        args.get(4)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !matches!(control, Some(4 | 8)) {
                        self.inst_error(
                            inst,
                            "AVX512 comparison SAE control must be iconst.i32 4 or 8".into(),
                        );
                    }
                }
                if id.is_x86_avx512_compare() {
                    let control =
                        args.get(2)
                            .and_then(|&arg| match func.value_def(func.resolve(arg)) {
                                ValueDef::Result(i, _) => match func.inst_data(i) {
                                    InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                    _ => None,
                                },
                                _ => None,
                            });
                    if !control.is_some_and(|n| (0..8).contains(&n)) {
                        self.inst_error(
                            inst,
                            "AVX512 integer predicate must be iconst.i32 in0..=7".into(),
                        );
                    }
                }
                if id.is_x86_pclmul() || id.has_x86_dot_control() {
                    let mode = args
                        .get(2)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !mode.is_some_and(|n| (0..256).contains(&n)) {
                        self.inst_error(
                            inst,
                            if id.is_x86_pclmul() {
                                "PCLMULQDQ control must be iconst.i32 in 0..=255".into()
                            } else {
                                "AVX dot control must be iconst.i32 in 0..=255".into()
                            },
                        );
                    }
                }
                if id.is_x86_gather() || id.is_x86_avx512_indexed() {
                    let scale = args
                        .get(4)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !scale.is_some_and(|n| matches!(n, 1 | 2 | 4 | 8)) {
                        self.inst_error(
                            inst,
                            "indexed memory scale must be iconst.i32 in1,2,4,8".into(),
                        );
                    }
                }
                if id.has_f16c_rounding() {
                    let mode = args
                        .get(1)
                        .and_then(|&a| match func.value_def(func.resolve(a)) {
                            ValueDef::Result(i, _) => match func.inst_data(i) {
                                InstData::Iconst { ty: Type::I32, imm } => Some(*imm),
                                _ => None,
                            },
                            _ => None,
                        });
                    if !mode.is_some_and(|n| (0..=7).contains(&n)) {
                        self.inst_error(
                            inst,
                            "F16C rounding control must be iconst.i32 in0..=7".into(),
                        );
                    }
                }
                let info = id.info();
                if !args.iter().map(|&a| ty(a)).eq(info.params.iter().copied()) {
                    let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
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
            InstData::X86RflagsRegion { plan, args } => {
                if let Err(e) = crate::rflags_region::analyze(plan) {
                    self.inst_error(inst, e.to_string());
                }
                if args.len() != plan.captures.len()
                    || args
                        .iter()
                        .any(|&v| func.value_type(func.resolve(v)) != Type::I64)
                {
                    self.inst_error(
                        inst,
                        "FLAGS region capture arity/types must match exact I64 images".into(),
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
                let fits = |case: i64| {
                    bits == 64 || (-(1i64 << (bits - 1))..(1i64 << bits)).contains(&case)
                };
                // Sorting finds whether there is any error at all; the
                // loop below reports them in order.
                let sorted = &mut self.scratch.cases;
                sorted.clear();
                sorted.extend(cases.iter().map(|&case| case as u64 & mask));
                sorted.sort_unstable();
                if cases.iter().all(|&case| fits(case)) && sorted.windows(2).all(|w| w[0] != w[1]) {
                    return;
                }
                let mut seen = FxHashSet::default();
                for &case in cases {
                    if !fits(case) {
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
                let arg_types = || args.iter().map(|&a| ty(a));
                self.call_abi(inst, &decl.sig, args);
                if !args_match(&decl.sig, arg_types()) {
                    let got: Vec<Type> = arg_types().collect();
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
                for pred in self.dom().0.preds(block).to_vec() {
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
                let arg_types = || args.iter().map(|&a| ty(a));
                self.call_abi(inst, &decl.sig, args);
                if !args_match(&decl.sig, arg_types()) {
                    let got: Vec<Type> = arg_types().collect();
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
                let arg_types = || args.iter().map(|&a| ty(a));
                self.call_abi(inst, sig, args);
                if !args_match(sig, arg_types()) {
                    let got: Vec<Type> = arg_types().collect();
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
                let returns = func.sig.returns.iter().map(|r| r.ty);
                if !args.iter().map(|&a| ty(a)).eq(returns) {
                    let got: Vec<Type> = args.iter().map(|&a| ty(a)).collect();
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
        // The preflight has checked that every destination is inserted.
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

    /// Checks that resolved `value` is still defined (as
    /// [`Function::is_value_defined`], with the layout's blocks already
    /// known) and dominates its use by `inst`, the `visit`th block in the
    /// layout, if that block is reachable.
    fn check_dominance(&mut self, inst: Inst, use_block: Block, visit: u32, value: Value) {
        let func = self.func;
        let removed = |this: &mut Self| {
            this.inst_error(inst, format!("uses {value}, whose definition was removed"));
        };
        // A definition in the entry block dominates every reachable block,
        // and one in the use's own block does if it came first. Only the
        // rest need the dominator tree.
        let (def_block, earlier) = match func.value_def(value) {
            ValueDef::Param(def_block, n) => {
                let defined = self.scratch.inserted[def_block.index()]
                    && func.block_params(def_block).get(n as usize) == Some(&value);
                if !defined {
                    return removed(self);
                }
                (def_block, true)
            }
            ValueDef::Result(def_inst, _) => {
                let Some(def_block) = func.inst_block_if_placed(def_inst) else {
                    return removed(self);
                };
                // An invoke's results exist only once it returns normally.
                if let InstData::Invoke { dests, .. } = func.inst_data(def_inst) {
                    let normal = dests[0].block;
                    let (_, domtree) = self.dom();
                    let ok = !domtree.is_reachable(use_block)
                        || domtree.dominates(normal, use_block) && def_block != use_block;
                    if !ok {
                        self.dominance_error(inst, value);
                    }
                    return;
                }
                (def_block, self.scratch.visited[def_inst.index()] == visit)
            }
            ValueDef::Alias(_) => unreachable!("resolved values are never aliases"),
        };
        let entry = func.entry_block();
        let ok = if def_block == use_block {
            earlier
        } else {
            Some(def_block) == entry
        };
        if ok {
            return;
        }
        let (_, domtree) = self.dom();
        let ok = !domtree.is_reachable(use_block)
            || def_block != use_block && domtree.dominates(def_block, use_block);
        if !ok {
            self.dominance_error(inst, value);
        }
    }

    fn dominance_error(&mut self, inst: Inst, value: Value) {
        self.inst_error(
            inst,
            format!("use of {value} is not dominated by its definition"),
        );
    }

    fn call_abi(&mut self, inst: Inst, sig: &Signature, args: &crate::CallArgs) {
        for (i, &v) in args.iter().enumerate() {
            let Some(p) = args.abi(i) else {
                continue;
            };
            if p.ty != self.func.value_type(self.func.resolve(v)) {
                self.inst_error(inst, format!("call operand {i} ABI type mismatch"));
            }
            if let Some(named) = sig.params.get(i) {
                if named != p {
                    self.inst_error(
                        inst,
                        format!("call operand {i} ABI differs from named parameter"),
                    );
                }
            } else {
                if !sig.variadic || matches!(p.purpose, Purpose::Sret(_)) {
                    self.inst_error(
                        inst,
                        "unnamed ABI descriptor requires variadic operand and cannot be sret"
                            .into(),
                    );
                }
                if p.ext != crate::ArgExt::None
                    || (p.purpose != Purpose::Normal && p.ty != Type::Ptr)
                {
                    self.inst_error(inst, "invalid unnamed call operand ABI".into());
                }
            }
            if let Some(l) = p.layout() {
                match l.checked_size() {
                    None => self.inst_error(
                        inst,
                        "call operand aggregate layout size overflows u64".into(),
                    ),
                    Some(0) if !(sig.x86_vector_abi.is_some() && layout_contains_f64x1(l)) => {
                        self.inst_error(inst, "invalid call operand aggregate layout".into())
                    }
                    Some(size) if size > u64::from(u32::MAX) => self.inst_error(
                        inst,
                        "call operand aggregate layout is larger than a stack slot".into(),
                    ),
                    _ => {}
                }
            }
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
        ConvertOp::SwidenLow
        | ConvertOp::UwidenLow
        | ConvertOp::SwidenHigh
        | ConvertOp::UwidenHigh => match (from.vector(), to.vector()) {
            (Some(f), Some(t)) => {
                f.lane_type().is_int()
                    && t.lane_type().is_int()
                    && f.lane_type().bytes() < t.lane_type().bytes()
                    && f.lanes() >= t.lanes()
            }
            _ => false,
        },
        ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong => {
            match (from.vector(), to.vector()) {
                (Some(f), Some(t)) => {
                    f.lane_type().is_int()
                        && t.lane_type().is_int()
                        && f.lane_type().bytes().unwrap() * 2 == t.lane_type().bytes().unwrap()
                        && u16::from(f.lanes()) >= 2 * u16::from(t.lanes())
                }
                _ => false,
            }
        }
        ConvertOp::NarrowLow
        | ConvertOp::SnarrowSatLow
        | ConvertOp::UnarrowSatLow
        | ConvertOp::SunarrowSatLow => match (from.vector(), to.vector()) {
            (Some(f), Some(t)) => {
                f.lane_type().is_int()
                    && t.lane_type().is_int()
                    && f.lane_type().bytes() > t.lane_type().bytes()
            }
            _ => false,
        },
        ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc => match (from.bytes(), to.bytes()) {
            (Some(f), Some(t)) if from.is_int() && to.is_int() => match op {
                ConvertOp::Trunc => t < f,
                _ => t > f,
            },
            _ => false,
        },
        ConvertOp::Fpext => from.is_float() && to.is_float() && from.bytes() < to.bytes(),
        ConvertOp::Fptrunc => from.is_float() && to.is_float() && from.bytes() > to.bytes(),
        ConvertOp::Fptosi | ConvertOp::Fptoui => {
            from.is_float() && matches!(to, Type::I32 | Type::I64)
        }
        ConvertOp::Sitofp | ConvertOp::Uitofp => small_int(from) && to.is_float(),
        ConvertOp::Bitcast => {
            if from.is_vector() || to.is_vector() {
                from.bytes() == to.bytes() && from.bytes().is_some()
            } else {
                from.is_float() != to.is_float()
                    && (from.is_int() || to.is_int())
                    && from.bytes() == to.bytes()
            }
        }
        ConvertOp::PtrToInt => (from, to) == (Type::Ptr, Type::I64),
        ConvertOp::IntToPtr => (from, to) == (Type::I64, Type::Ptr),
    };
    if ok {
        return Ok(());
    }
    Err(match op {
        ConvertOp::SwidenLow
        | ConvertOp::UwidenLow
        | ConvertOp::SwidenHigh
        | ConvertOp::UwidenHigh => "needs integer vectors with wider destination lanes",
        ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong => {
            "needs integer vectors with exactly double-width destination lanes"
        }
        ConvertOp::NarrowLow
        | ConvertOp::SnarrowSatLow
        | ConvertOp::UnarrowSatLow
        | ConvertOp::SunarrowSatLow => "needs integer vectors with narrower destination lanes",
        ConvertOp::Sext | ConvertOp::Uext => "must convert an integer to a wider integer",
        ConvertOp::Trunc => "must convert an integer to a narrower integer",
        ConvertOp::Fpext => "converts f32 to f64",
        ConvertOp::Fptrunc => "converts f64 to f32",
        ConvertOp::Fptosi | ConvertOp::Fptoui => "converts a float to i32 or i64",
        ConvertOp::Sitofp | ConvertOp::Uitofp => "converts an integer up to i64 to a float",
        ConvertOp::Bitcast => {
            "converts between same-size vectors/scalars, or a scalar integer and float"
        }
        ConvertOp::PtrToInt => "converts ptr to i64",
        ConvertOp::IntToPtr => "converts i64 to ptr",
    })
}

fn layout_contains_zmm(layout: &crate::Layout) -> bool {
    match layout {
        crate::Layout::Scalar(t) => t.vector().is_some_and(|shape| shape.bytes() == 64),
        crate::Layout::Struct(fields) => fields.iter().any(layout_contains_zmm),
        crate::Layout::Array(elem, _) => layout_contains_zmm(elem),
    }
}

fn layout_contains_f64x1(layout: &crate::Layout) -> bool {
    match layout {
        crate::Layout::Scalar(t) => *t == Type::Vector(crate::VectorType::F64x1),
        crate::Layout::Struct(fields) => fields.iter().any(layout_contains_f64x1),
        crate::Layout::Array(elem, _) => layout_contains_f64x1(elem),
    }
}

/// Checks the ABI attributes of a signature's parameters and returns.
fn check_signature(sig: &Signature) -> Vec<String> {
    let mut errors = Vec::new();
    if sig.x86_vector_abi.is_some() && sig.cc != CallConv::C {
        errors.push("x86_vector_abi requires cc(c), not the private Fast convention".into());
    }
    if sig.cc == CallConv::Fast && (sig.variadic || sig.returns_twice || sig.returns.len() > 1) {
        errors.push("cc(fast) requires a fixed, single-result, single-return call contract".into());
    }
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
            match layout.checked_size() {
                None => errors.push(format!("`{param}`: aggregate layout size overflows u64")),
                Some(0) if !(sig.x86_vector_abi.is_some() && layout_contains_f64x1(layout)) => {
                    errors.push(format!("`{param}`: the struct is empty"))
                }
                // The caller copies it into a stack slot, whose size is a u32.
                Some(size) if size > u64::from(u32::MAX) => errors.push(format!(
                    "`{param}`: the struct is larger than {} bytes",
                    u32::MAX
                )),
                _ => {}
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

/// Bounded local address proof; unknown external pointers retain the documented
/// memory-access preconditions rather than being rejected wholesale.
fn local_aggregate_address(func: &Function, mut value: Value) -> Option<(crate::StackSlot, i64)> {
    let mut offset = 0i64;
    for _ in 0..32 {
        let ValueDef::Result(inst, _) = func.value_def(func.resolve(value)) else {
            return None;
        };
        match func.inst_data(inst) {
            InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                return Some((*slot, offset));
            }
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } => {
                let ValueDef::Result(constant, _) = func.value_def(func.resolve(args[1])) else {
                    return None;
                };
                let InstData::Iconst { imm, .. } = func.inst_data(constant) else {
                    return None;
                };
                offset = offset.checked_add(*imm)?;
                value = args[0];
            }
            _ => return None,
        }
    }
    None
}

/// Whether arguments of types `got` fit `sig`: exactly its parameters, or
/// for a variadic signature its parameters followed by any number of the
/// types C's default argument promotions produce, or unpromoted SIMD vectors.
/// (An `i8` or `f32` would
/// be read back by the callee as an `int` or `double` it is not.)
/// Whether arguments of types `got` fit `sig`: its parameters exactly,
/// then for a variadic one any number of promoted variadic arguments.
/// Compares in place; the caller collects `got` only to report a mismatch.
fn args_match(sig: &Signature, mut got: impl ExactSizeIterator<Item = Type>) -> bool {
    let n = sig.params.len();
    if got.len() < n || (!sig.variadic && got.len() != n) {
        return false;
    }
    sig.params.iter().all(|p| got.next() == Some(p.ty))
        && got.all(|ty| {
            ty.is_vector()
                || matches!(
                    ty,
                    Type::I32 | Type::I64 | Type::I128 | Type::Ptr | Type::F64 | Type::F80
                )
        })
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

#[cfg(test)]
mod name_tests {
    use super::*;
    use crate::{FuncDecl, Function};

    fn data(name: &str, thread_local: bool, init: Option<u32>) -> DataObject {
        DataObject {
            name: name.into(),
            thread_local,
            local: false,
            weak: false,
            weak_odr: false,
            hidden: false,
            retain: false,
            section: None,
            init: init.map(|n| DataInit {
                constant: false,
                align: 8,
                items: vec![DataItem::Zero(n)],
            }),
        }
    }

    /// Duplicated names across and within every table, so precedence
    /// decides each answer.
    fn module() -> Module {
        let sig = |n: usize| Signature::new(&vec![Type::I64; n], &[]);
        let attrs = |noreturn| FunctionAttrs {
            noreturn,
            ..FunctionAttrs::default()
        };
        let decl = |name: &str, n, noreturn| FuncDecl {
            attrs: attrs(noreturn),
            name: name.into(),
            sig: sig(n),
        };
        let func = |name: &str, n, noreturn| {
            let mut f = Function::new(name, sig(n));
            f.attrs = attrs(noreturn);
            f
        };
        Module {
            decls: vec![
                decl("both", 1, false),
                decl("both", 2, true),
                decl("only_decl", 3, true),
            ],
            functions: vec![
                func("both", 4, true),
                func("only_func", 5, false),
                func("only_func", 6, true),
                func("shared", 7, true),
            ],
            data: vec![
                data("dup", true, Some(8)),
                data("dup", false, Some(64)),
                data("shared", false, None),
                data("only_data", false, Some(16)),
            ],
            ..Module::default()
        }
    }

    #[test]
    fn index_matches_linear_module_lookups() {
        let module = module();
        let names = Names::new(&module);
        for name in [
            "both",
            "only_decl",
            "only_func",
            "shared",
            "dup",
            "only_data",
            "missing",
            "",
        ] {
            assert_eq!(names.signature(name), module.signature(name), "{name}");
            assert_eq!(
                names.function_attrs(name),
                module.function_attrs(name),
                "{name}"
            );
            assert_eq!(names.data_object(name), module.data_object(name), "{name}");
            assert_eq!(names.has_symbol(name), module.has_symbol(name), "{name}");
        }
        // The first entries win, not the last.
        assert_eq!(names.signature("both").unwrap().params.len(), 1);
        assert_eq!(names.signature("only_func").unwrap().params.len(), 5);
        assert!(names.data_object("dup").unwrap().thread_local);
    }

    #[test]
    fn data_cursor_overflow_is_a_diagnostic_in_both_profiles() {
        assert_eq!(checked_data_offset(u64::MAX - 4, 4), Ok(u64::MAX));
        assert_eq!(checked_data_offset(u64::MAX, 0), Ok(u64::MAX));
        assert_eq!(
            checked_data_offset(u64::MAX - 3, 4),
            Err("data object size overflows u64")
        );
        assert_eq!(
            checked_data_offset(u64::MAX, 8),
            Err("data object size overflows u64")
        );
    }

    #[test]
    fn data_checks_use_the_first_duplicate_object() {
        let module = module();
        let names = Names::new(&module);
        let init = |addend| DataInit {
            constant: false,
            align: 8,
            items: vec![DataItem::Ptr {
                symbol: "dup".into(),
                addend,
            }],
        };
        // The first `dup` is thread-local, so this is reported before size.
        assert_eq!(
            check_data(&names, &init(32), DataLayout::NATIVE64),
            ["points to thread-local @dup, whose address differs per thread"]
        );
        let ptr = |symbol: &str, addend| DataInit {
            constant: false,
            align: 8,
            items: vec![DataItem::Ptr {
                symbol: symbol.into(),
                addend,
            }],
        };
        assert_eq!(
            check_data(&names, &ptr("only_data", 17), DataLayout::NATIVE64),
            ["@only_data + 17 is past the end of @only_data (16 bytes)"]
        );
        assert!(check_data(&names, &ptr("only_data", 16), DataLayout::NATIVE64).is_empty());
        // A function, and a name that is both a function and declared data.
        assert!(check_data(&names, &ptr("only_func", 0), DataLayout::NATIVE64).is_empty());
        assert!(check_data(&names, &ptr("shared", 0), DataLayout::NATIVE64).is_empty());
        assert_eq!(
            check_data(&names, &ptr("missing", 0), DataLayout::NATIVE64),
            ["points to @missing, which is not declared"]
        );
    }
}

#[cfg(test)]
mod switch_tests {
    use crate::InstData;

    #[test]
    fn rejects_wrong_destination_counts() {
        let module = crate::parse(
            "func @f(i64) {
block0(v0: i64):
    switch v0, block1, [1: block1]
block1:
    return
}",
        )
        .unwrap();
        let func = &module.functions[0];
        let inst = func.block_insts(func.entry_block().unwrap())[0];
        for count in [0, 3] {
            let mut func = func.clone();
            let InstData::Switch { dests, .. } = func.inst_data_mut(inst) else {
                panic!("expected a switch");
            };
            let dest = dests[0].clone();
            dests.clear();
            dests.resize(count, dest);
            let errors = super::verify(&func).unwrap_err();
            assert!(
                errors[0]
                    .to_string()
                    .contains(&format!("switch has {count} destinations for 1 cases")),
                "{errors:?}"
            );
        }
    }
}

#[cfg(test)]
mod dominance_tests {
    fn errors(text: &str) -> Vec<String> {
        let module = crate::parse(text).unwrap();
        match super::verify_module(&module) {
            Ok(()) => Vec::new(),
            Err(errors) => errors.into_iter().map(|e| e.to_string()).collect(),
        }
    }

    /// The shortcuts that avoid the dominator tree (entry definitions, same
    /// block order) still skip unreachable blocks and still catch the rest.
    #[test]
    fn shortcuts_agree_with_the_dominator_tree() {
        let func = |ret: &str| {
            format!(
                "func @f(i8) -> i64 cc(c) {{
block0(v0: i8):
    v1 = iconst.i64 1
    brif v0, block1, block2
block1:
    v2 = iconst.i64 2
    jump block3
block2:
    jump block3
block3:
    {ret}
block4:
    v4 = iadd v5, v2
    v5 = iconst.i64 3
    return v4
}}"
            )
        };
        assert_eq!(errors(&func("return v1")), Vec::<String>::new());
        assert_eq!(
            errors(&func("return v2")),
            ["@f inst5 `return v2`: use of v2 is not dominated by its definition"]
        );
    }
}

#[cfg(test)]
mod args_tests {
    use super::*;
    use crate::VectorType;

    /// The comparison before it stopped allocating.
    fn reference(sig: &Signature, got: &[Type]) -> bool {
        let params = sig.param_types();
        let n = params.len();
        if !sig.variadic {
            return got == params;
        }
        got.len() >= n
            && got[..n] == params[..]
            && got[n..].iter().all(|ty| {
                ty.is_vector()
                    || matches!(
                        ty,
                        Type::I32 | Type::I64 | Type::I128 | Type::Ptr | Type::F64 | Type::F80
                    )
            })
    }

    #[test]
    fn args_match_agrees_with_the_allocating_comparison() {
        let vector = Type::Vector(VectorType::I32x4);
        let types = [
            Type::I8,
            Type::I32,
            Type::I64,
            Type::Ptr,
            Type::F32,
            Type::F64,
            vector,
        ];
        let params: [&[Type]; 4] = [
            &[],
            &[Type::I32],
            &[Type::Ptr, Type::F64],
            &[Type::I8, Type::I8, Type::I64],
        ];
        // Every argument list of up to four of these types.
        let mut lists: Vec<Vec<Type>> = vec![Vec::new()];
        for len in 1..=4 {
            let mut next = Vec::new();
            for list in lists.iter().filter(|l| l.len() == len - 1) {
                for &ty in &types {
                    let mut l = list.clone();
                    l.push(ty);
                    next.push(l);
                }
            }
            lists.extend(next);
        }
        let mut matched = 0;
        for params in params {
            for variadic in [false, true] {
                let mut sig = Signature::new(params, &[]);
                sig.variadic = variadic;
                for got in &lists {
                    let want = reference(&sig, got);
                    assert_eq!(
                        args_match(&sig, got.iter().copied()),
                        want,
                        "{sig} with {got:?}"
                    );
                    matched += usize::from(want);
                }
            }
        }
        assert!(matched > 100, "exercise matches as well as mismatches");
    }
}
