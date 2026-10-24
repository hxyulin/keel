//! Experimental scalar core WebAssembly output with opt-in fixed wasm32 memory.
//! No native or C ABI or host imports. Runtime placement is a separate opt-in.
mod runtime;
use keel_ir::{
    ArgExt, BinaryOp, Block, BlockCall, CallConv, ConvertOp, DataLayout, EntityRef, Function,
    FunctionAttrs, InstData, IntCC, Module, Purpose, Signature, Symbol, Type, UnaryOp, Value,
};
pub use runtime::Wasm32RuntimeOptions;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ExportKind, ExportSection, FunctionSection, GlobalSection,
    GlobalType, Instruction as W, MemArg, MemorySection, MemoryType, TypeSection, ValType,
};

/// One explicitly requested function export; names are exact UTF-8 strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub function: String,
}

/// No implicit exports or host imports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub exports: Vec<Export>,
}

/// Explicit opt-in to one owned, unshared, exported wasm32 memory.
/// Initial and maximum size are both `pages` (64 KiB each, 0..=65536).
/// `export_name` must be nonempty and distinct from all function export names.
/// Function signatures retain the scalar i32/i64 contract of `Options`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wasm32MemoryOptions {
    pub scalar: Options,
    pub pages: u32,
    pub export_name: String,
}

/// A verifier, unsupported-contract, or binary-validation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic(pub String);
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Diagnostic {}
fn reject(message: impl Into<String>) -> Diagnostic {
    Diagnostic(message.into())
}
fn wide(t: Type) -> bool {
    matches!(t, Type::I32 | Type::I64)
}
fn local_type(t: Type) -> Result<ValType, Diagnostic> {
    match t {
        Type::I8 | Type::I16 | Type::I32 | Type::Ptr => Ok(ValType::I32),
        Type::I64 => Ok(ValType::I64),
        _ => Err(reject(format!("unsupported value type {t}"))),
    }
}
fn check_value_type(t: Type, memory: bool) -> Result<(), Diagnostic> {
    if !memory && matches!(t, Type::I16 | Type::Ptr) {
        return Err(reject(format!("unsupported value type {t}")));
    }
    local_type(t).map(|_| ())
}
fn memory_bytes(t: Type) -> Option<u64> {
    match t {
        Type::Ptr => Some(DataLayout::WASM32.pointer_bytes()),
        Type::I8 | Type::I16 | Type::I32 | Type::I64 => t.bytes().map(u64::from),
        _ => None,
    }
}
fn conversion(op: ConvertOp, from: Type, to: Type) -> bool {
    match op {
        ConvertOp::IntToPtr => (from, to) == (Type::I64, Type::Ptr),
        ConvertOp::PtrToInt => (from, to) == (Type::Ptr, Type::I64),
        ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc => {
            matches!(from, Type::I8 | Type::I16 | Type::I32 | Type::I64)
                && matches!(to, Type::I8 | Type::I16 | Type::I32 | Type::I64)
        }
        _ => false,
    }
}
fn signature(sig: &Signature) -> Result<(), Diagnostic> {
    if sig.variadic
        || sig.returns_twice
        || sig.cc != CallConv::C
        || sig.x86_vector_abi.is_some()
        || sig.returns.len() > 1
        || sig
            .params
            .iter()
            .chain(&sig.returns)
            .any(|p| !wide(p.ty) || p.ext != ArgExt::None || p.purpose != Purpose::Normal)
    {
        return Err(reject(format!("unsupported scalar signature {sig}")));
    }
    Ok(())
}
fn attrs(a: FunctionAttrs) -> Result<(), Diagnostic> {
    if a.weak || a.constructor || a.destructor || a.used || a.retain || a.section.is_some() {
        return Err(reject("unsupported linkage/lifecycle/section attributes"));
    }
    Ok(())
}
fn binary(op: BinaryOp, t: Type) -> Option<W<'static>> {
    use BinaryOp::*;
    Some(match (op, t) {
        (Iadd, Type::I32) => W::I32Add,
        (Iadd, Type::I64) => W::I64Add,
        (Isub, Type::I32) => W::I32Sub,
        (Isub, Type::I64) => W::I64Sub,
        (Imul, Type::I32) => W::I32Mul,
        (Imul, Type::I64) => W::I64Mul,
        (Udiv, Type::I32) => W::I32DivU,
        (Udiv, Type::I64) => W::I64DivU,
        (Sdiv, Type::I32) => W::I32DivS,
        (Sdiv, Type::I64) => W::I64DivS,
        (Urem, Type::I32) => W::I32RemU,
        (Urem, Type::I64) => W::I64RemU,
        (Srem, Type::I32) => W::I32RemS,
        (Srem, Type::I64) => W::I64RemS,
        (And, Type::I32) => W::I32And,
        (And, Type::I64) => W::I64And,
        (Or, Type::I32) => W::I32Or,
        (Or, Type::I64) => W::I64Or,
        (Xor, Type::I32) => W::I32Xor,
        (Xor, Type::I64) => W::I64Xor,
        (Shl, Type::I32) => W::I32Shl,
        (Shl, Type::I64) => W::I64Shl,
        (Ushr, Type::I32) => W::I32ShrU,
        (Ushr, Type::I64) => W::I64ShrU,
        (Sshr, Type::I32) => W::I32ShrS,
        (Sshr, Type::I64) => W::I64ShrS,
        (Rotl, Type::I32) => W::I32Rotl,
        (Rotl, Type::I64) => W::I64Rotl,
        (Rotr, Type::I32) => W::I32Rotr,
        (Rotr, Type::I64) => W::I64Rotr,
        _ => return None,
    })
}
fn comparison(cc: IntCC, t: Type) -> Option<W<'static>> {
    use IntCC::*;
    // Pointer relations use normalized unsigned wasm32 address bits. Signed
    // pointer predicates have no accepted contract in this profile.
    if t == Type::Ptr {
        return matches!(cc, Eq | Ne | Ult | Ule | Ugt | Uge)
            .then(|| comparison(cc, Type::I32).unwrap());
    }
    Some(match (cc, t) {
        (Eq, Type::I32) => W::I32Eq,
        (Eq, Type::I64) => W::I64Eq,
        (Ne, Type::I32) => W::I32Ne,
        (Ne, Type::I64) => W::I64Ne,
        (Slt, Type::I32) => W::I32LtS,
        (Slt, Type::I64) => W::I64LtS,
        (Sle, Type::I32) => W::I32LeS,
        (Sle, Type::I64) => W::I64LeS,
        (Sgt, Type::I32) => W::I32GtS,
        (Sgt, Type::I64) => W::I64GtS,
        (Sge, Type::I32) => W::I32GeS,
        (Sge, Type::I64) => W::I64GeS,
        (Ult, Type::I32) => W::I32LtU,
        (Ult, Type::I64) => W::I64LtU,
        (Ule, Type::I32) => W::I32LeU,
        (Ule, Type::I64) => W::I64LeU,
        (Ugt, Type::I32) => W::I32GtU,
        (Ugt, Type::I64) => W::I64GtU,
        (Uge, Type::I32) => W::I32GeU,
        (Uge, Type::I64) => W::I64GeU,
        _ => return None,
    })
}

/// Verify the entire input, preflight every laid-out instruction and all call
/// contracts (including unused declarations), then encode and independently
/// validate a complete core module. Errors never publish partial bytes.
pub fn emit_module(module: &Module, options: &Options) -> Result<Vec<u8>, Diagnostic> {
    emit(module, options, None, None, None)
}

/// Emit raw, unoptimized IR with fixed wasm32 memory. Offsets cross the scalar
/// export boundary as i64 followed by `inttoptr`, or i32 followed by `uext.i64`
/// and `inttoptr`. Pointer function signatures, including internal, are rejected.
pub fn emit_wasm32_memory_module(
    module: &Module,
    options: &Wasm32MemoryOptions,
) -> Result<Vec<u8>, Diagnostic> {
    validate_memory_options(options)?;
    emit(module, &options.scalar, Some(options), None, None)
}

fn validate_memory_options(options: &Wasm32MemoryOptions) -> Result<(), Diagnostic> {
    // Wide arithmetic also handles the valid 4 GiB maximum on 32-bit hosts.
    let bytes = u64::from(options.pages) * 65536;
    if bytes > (1u64 << 32) {
        return Err(reject("wasm32 memory pages must be in 0..=65536"));
    }
    if options.export_name.is_empty() {
        return Err(reject("memory export name must be nonempty"));
    }
    if options
        .scalar
        .exports
        .iter()
        .any(|e| e.name == options.export_name)
    {
        return Err(reject("memory export name collides with a function export"));
    }
    Ok(())
}

/// Emit raw wasm32 IR with static data and a bounded owned-memory runtime.
/// See `Wasm32RuntimeOptions` for placement and trap lifetime requirements.
pub fn emit_wasm32_runtime_module(
    module: &Module,
    options: &Wasm32RuntimeOptions,
) -> Result<Vec<u8>, Diagnostic> {
    let (placement, prepared) = prepare_runtime(module, options)?;
    encode(
        module,
        &options.memory.scalar,
        Some(&options.memory),
        Some(&placement),
        Some(options),
        prepared,
    )
}

/// Check admission to the bounded wasm32 runtime profile without encoding.
/// This checks the real module, placement, exports, calls and static frames;
/// it neither mutates the input nor validates a generated Wasm binary.
pub fn validate_wasm32_runtime_module(
    module: &Module,
    options: &Wasm32RuntimeOptions,
) -> Result<(), Diagnostic> {
    prepare_runtime(module, options).map(|_| ())
}

struct Prepared<'a> {
    indices: HashMap<&'a str, u32>,
    frames: Option<Vec<runtime::Frame>>,
}

fn prepare_runtime<'a>(
    module: &'a Module,
    options: &Wasm32RuntimeOptions,
) -> Result<(runtime::Placement<'a>, Prepared<'a>), Diagnostic> {
    validate_memory_options(&options.memory)?;
    let placement = runtime::Placement::new(module, options)?;
    let prepared = prepare(module, &options.memory.scalar, true, true, true)?;
    Ok((placement, prepared))
}

fn emit(
    module: &Module,
    options: &Options,
    memory: Option<&Wasm32MemoryOptions>,
    placement: Option<&runtime::Placement<'_>>,
    runtime: Option<&Wasm32RuntimeOptions>,
) -> Result<Vec<u8>, Diagnostic> {
    let prepared = prepare(
        module,
        options,
        memory.is_some(),
        placement.is_some(),
        runtime.is_some(),
    )?;
    encode(module, options, memory, placement, runtime, prepared)
}

fn prepare<'a>(
    module: &'a Module,
    options: &Options,
    has_memory: bool,
    has_placement: bool,
    has_runtime: bool,
) -> Result<Prepared<'a>, Diagnostic> {
    let verified = if has_placement {
        keel_ir::verify_module_with_layout(module, DataLayout::WASM32)
    } else {
        keel_ir::verify_module(module)
    };
    verified.map_err(|errors| {
        reject(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    if !has_placement && !module.data.is_empty() {
        return Err(reject("module data is unsupported"));
    }
    let indices: HashMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i as u32))
        .collect();
    let check_decl = |d: &keel_ir::FuncDecl| -> Result<(), Diagnostic> {
        signature(&d.sig)?;
        attrs(d.attrs)?;
        let Some(&index) = indices.get(d.name.as_str()) else {
            return Err(reject(format!(
                "undefined function @{}; imports are unsupported",
                d.name
            )));
        };
        if module.functions[index as usize].sig != d.sig {
            return Err(reject(format!("incompatible signature for @{}", d.name)));
        }
        Ok(())
    };
    for d in &module.decls {
        check_decl(d)?;
    }
    let mut names = HashSet::new();
    for e in &options.exports {
        if !names.insert(&e.name) {
            return Err(reject(format!("duplicate export {:?}", e.name)));
        }
        if !indices.contains_key(e.function.as_str()) {
            return Err(reject(format!(
                "export {:?} has no defined function @{}",
                e.name, e.function
            )));
        }
    }
    for f in &module.functions {
        let check = || -> Result<(), Diagnostic> {
            signature(&f.sig)?;
            attrs(f.attrs)?;
            if f.weak_odr
                || f.personality.is_some()
                || (!has_runtime && f.num_stack_slots() != 0)
                || (!has_placement && f.data_symbols().next().is_some())
                || f.signatures().next().is_some()
            {
                return Err(reject(
                    "unsupported linkage, exception, stack, data, or indirect-call contract",
                ));
            }
            for (_, d) in f.callees() {
                check_decl(d)?;
            }
            for b in f.blocks() {
                for &v in f.block_params(b) {
                    check_value_type(f.value_type(v), has_memory)?;
                }
                for &i in f.block_insts(b) {
                    let d = f.inst_data(i);
                    let ty = |v| f.value_type(f.resolve(v));
                    for &v in f.inst_results(i) {
                        check_value_type(f.value_type(v), has_memory)?;
                    }
                    let mut bad = None;
                    d.for_each_operand(|v| {
                        if let Err(e) = check_value_type(ty(v), has_memory) {
                            bad = Some(e);
                        }
                    });
                    if let Some(e) = bad {
                        return Err(e);
                    }
                    let supported = match d {
                        InstData::Iconst { ty, .. } => {
                            matches!(ty, Type::I8 | Type::I32 | Type::I64)
                                || (has_memory && *ty == Type::I16)
                        }
                        InstData::Unary { op, arg } => *op == UnaryOp::Ineg && wide(ty(*arg)),
                        InstData::Binary { op, args } => {
                            binary(*op, ty(args[0])).is_some()
                                || (has_memory && *op == BinaryOp::Ptradd)
                        }
                        InstData::Icmp { cond, args } => {
                            comparison(*cond, ty(args[0])).is_some()
                                && (has_memory || ty(args[0]) != Type::Ptr)
                        }
                        InstData::Convert { op, ty: to, arg } => {
                            has_memory && conversion(*op, ty(*arg), *to)
                        }
                        InstData::Load { ty, flags, .. } => {
                            has_memory
                                && memory_bytes(*ty).is_some()
                                && flags.align.is_power_of_two()
                        }
                        InstData::Store { value, flags, .. } => {
                            has_memory
                                && memory_bytes(ty(*value)).is_some()
                                && flags.align.is_power_of_two()
                        }
                        InstData::StackAddr { .. } => has_runtime,
                        InstData::SymbolAddr {
                            symbol: Symbol::Data(_),
                        } => has_placement,
                        InstData::Select { .. }
                        | InstData::Jump { .. }
                        | InstData::Brif { .. }
                        | InstData::Return { .. } => true,
                        InstData::Switch { arg, .. } => wide(ty(*arg)),
                        InstData::Call { args, .. } => !args.has_explicit_abi(),
                        _ => false,
                    };
                    if !supported {
                        return Err(reject(format!("{i}: unsupported instruction {d:?}")));
                    }
                }
            }
            Ok(())
        };
        check().map_err(|e| reject(format!("@{}: {e}", f.name)))?;
    }
    // Complete frame preflight for every function before encoding any body.
    let frames = has_runtime
        .then(|| {
            module
                .functions
                .iter()
                .map(runtime::Frame::new)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Prepared { indices, frames })
}

fn encode(
    module: &Module,
    options: &Options,
    memory: Option<&Wasm32MemoryOptions>,
    placement: Option<&runtime::Placement<'_>>,
    runtime: Option<&Wasm32RuntimeOptions>,
    prepared: Prepared<'_>,
) -> Result<Vec<u8>, Diagnostic> {
    let Prepared { indices, frames } = prepared;
    let mut types = TypeSection::new();
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    for (i, f) in module.functions.iter().enumerate() {
        types.ty().function(
            f.sig.params.iter().map(|p| local_type(p.ty).unwrap()),
            f.sig.returns.iter().map(|p| local_type(p.ty).unwrap()),
        );
        functions.function(i as u32);
        let frame = frames.as_ref().map(|frames| &frames[i]);
        code.function(&Body::new(f, &indices, placement, frame, runtime).emit());
    }
    for e in &options.exports {
        exports.export(&e.name, ExportKind::Func, indices[e.function.as_str()]);
    }
    let mut output = wasm_encoder::Module::new();
    output.section(&types).section(&functions);
    if let Some(memory) = memory {
        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: u64::from(memory.pages),
            maximum: Some(u64::from(memory.pages)),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        output.section(&memories);
        exports.export(&memory.export_name, ExportKind::Memory, 0);
    }
    if let Some(runtime) = runtime {
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(runtime.stack_top as i32),
        );
        output.section(&globals);
    }
    output.section(&exports).section(&code);
    if let Some(placement) = placement {
        output.section(&placement.segments(module, u64::from(memory.unwrap().pages) * 65536));
    }
    let bytes = output.finish();
    // Keep the output contract independent of the validator's evolving defaults.
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .map_err(|e| reject(format!("generated binary failed validation: {e}")))?;
    Ok(bytes)
}

// Buffer only a raw local.set. Every other operator is an adjacency barrier.
// No instructions move across a barrier, including structured control or traps.
struct InstructionOutput {
    function: wasm_encoder::Function,
    pending_set: Option<u32>,
}
impl InstructionOutput {
    fn new(declarations: Vec<(u32, ValType)>) -> Self {
        Self {
            function: wasm_encoder::Function::new(declarations),
            pending_set: None,
        }
    }
    fn op(&mut self, op: W<'_>) {
        if let Some(local) = self.pending_set.take() {
            if matches!(op, W::LocalGet(index) if index == local) {
                self.function.instruction(&W::LocalTee(local));
                return;
            }
            self.function.instruction(&W::LocalSet(local));
        }
        if let W::LocalSet(local) = op {
            self.pending_set = Some(local);
        } else {
            self.function.instruction(&op);
        }
    }
    fn finish(mut self) -> wasm_encoder::Function {
        if let Some(local) = self.pending_set.take() {
            self.function.instruction(&W::LocalSet(local));
        }
        self.function
    }
}

#[cfg(test)]
#[path = "../tests/support/adjacent_tee_output.rs"]
mod adjacent_tee_output_tests;

struct Body<'a> {
    ir: &'a Function,
    indices: &'a HashMap<&'a str, u32>,
    placement: Option<&'a runtime::Placement<'a>>,
    frame: Option<&'a runtime::Frame>,
    runtime: Option<&'a Wasm32RuntimeOptions>,
    saved_sp: u32,
    frame_base: u32,
    locals: Vec<Option<u32>>,
    direct_return: bool,
    pc: u32,
    blocks: HashMap<Block, i32>,
    out: InstructionOutput,
}
impl<'a> Body<'a> {
    fn new(
        ir: &'a Function,
        indices: &'a HashMap<&'a str, u32>,
        placement: Option<&'a runtime::Placement<'a>>,
        frame: Option<&'a runtime::Frame>,
        runtime: Option<&'a Wasm32RuntimeOptions>,
    ) -> Self {
        let mut locals = vec![None; ir.num_values()];
        let entry = ir.entry_block().unwrap();
        for (n, &v) in ir.block_params(entry).iter().enumerate() {
            locals[v.index()] = Some(n as u32);
        }
        let mut declarations = Vec::new();
        let mut next = ir.sig.params.len() as u32;
        // This choice is made only after complete module and frame preflight.
        // Count layout blocks, not allocated block handles: removed blocks do
        // not participate. A one-block jump/self-loop still needs dispatch.
        let mut blocks = ir.blocks();
        let only = blocks.next().unwrap();
        let direct_return = blocks.next().is_none()
            && matches!(
                ir.block_insts(only).last().map(|&i| ir.inst_data(i)),
                Some(InstData::Return { .. })
            );
        // Definition destinations use raw values; only reads resolve aliases.
        // An aliased producer must never overwrite its replacement's local.
        for b in ir.blocks() {
            for &v in ir
                .block_params(b)
                .iter()
                .chain(ir.block_insts(b).iter().flat_map(|&i| ir.inst_results(i)))
            {
                if locals[v.index()].is_none() {
                    locals[v.index()] = Some(next);
                    next += 1;
                    declarations.push((1, local_type(ir.value_type(v)).unwrap()));
                }
            }
        }
        let dispatch_locals = u32::from(!direct_return);
        if !direct_return {
            declarations.push((1, ValType::I32));
        }
        if runtime.is_some() {
            declarations.push((2, ValType::I32));
        }
        Self {
            ir,
            indices,
            placement,
            frame,
            runtime,
            saved_sp: next + dispatch_locals,
            frame_base: next + dispatch_locals + 1,
            locals,
            direct_return,
            pc: next,
            blocks: ir
                .blocks()
                .enumerate()
                .map(|(i, b)| (b, i as i32))
                .collect(),
            out: InstructionOutput::new(declarations),
        }
    }
    fn op(&mut self, op: W<'_>) {
        self.out.op(op);
    }
    fn get(&mut self, v: Value) {
        self.op(W::LocalGet(
            self.locals[self.ir.resolve(v).index()].unwrap(),
        ));
    }
    fn condition(&mut self, v: Value) {
        self.get(v);
        if self.ir.value_type(v) == Type::I64 {
            self.op(W::I64Eqz);
            self.op(W::I32Eqz);
        }
    }
    fn constant(&mut self, t: Type, imm: i64) {
        self.op(if t == Type::I64 {
            W::I64Const(imm)
        } else {
            W::I32Const(if t == Type::I8 {
                imm as u8 as i32
            } else if t == Type::I16 {
                imm as u16 as i32
            } else {
                imm as i32
            })
        });
    }
    fn memarg(ty: Type, align: u32) -> MemArg {
        // Alignment is a hint, never a requirement or a widened footprint.
        // Offset zero preserves ptradd's wrap before the bounds-checked access.
        MemArg {
            offset: 0,
            align: align.ilog2().min(memory_bytes(ty).unwrap().ilog2()),
            memory_index: 0,
        }
    }
    fn convert(&mut self, op: ConvertOp, to: Type, arg: Value) {
        let from = self.ir.value_type(arg);
        self.get(arg);
        match op {
            ConvertOp::IntToPtr => self.op(W::I32WrapI64),
            ConvertOp::PtrToInt => self.op(W::I64ExtendI32U),
            ConvertOp::Trunc => {
                if from == Type::I64 {
                    self.op(W::I32WrapI64);
                }
                if matches!(to, Type::I8 | Type::I16) {
                    self.op(W::I32Const(if to == Type::I8 { 255 } else { 65535 }));
                    self.op(W::I32And);
                }
            }
            ConvertOp::Sext | ConvertOp::Uext => {
                if op == ConvertOp::Sext && matches!(from, Type::I8 | Type::I16) {
                    // MVP shifts, rather than proposal-only extend8/extend16.
                    let shift = if from == Type::I8 { 24 } else { 16 };
                    self.op(W::I32Const(shift));
                    self.op(W::I32Shl);
                    self.op(W::I32Const(shift));
                    self.op(W::I32ShrS);
                    if to == Type::I16 {
                        self.op(W::I32Const(65535));
                        self.op(W::I32And);
                    }
                }
                if to == Type::I64 {
                    self.op(if op == ConvertOp::Sext {
                        W::I64ExtendI32S
                    } else {
                        W::I64ExtendI32U
                    });
                }
            }
            _ => unreachable!("preflight"),
        }
    }
    fn edge(&mut self, edge: &BlockCall, depth: u32) {
        // Snapshot every source on the operand stack, then pop in reverse.
        // This is a parallel copy, including cycles and self-edges; the stack
        // is empty again before dispatch. Only the selected edge executes.
        for &v in &edge.args {
            self.get(v);
        }
        for &v in self.ir.block_params(edge.block).iter().rev() {
            self.op(W::LocalSet(self.locals[v.index()].unwrap()));
        }
        self.op(W::I32Const(self.blocks[&edge.block]));
        self.op(W::LocalSet(self.pc));
        self.op(W::Br(depth));
    }
    fn emit(mut self) -> wasm_encoder::Function {
        if let Some(runtime) = self.runtime {
            let size = self.frame.unwrap().size;
            self.op(W::GlobalGet(0));
            self.op(W::LocalSet(self.saved_sp));
            // Check unsigned underflow before subtracting. Neither failure path
            // updates SP or permits any frame store/address instruction.
            self.op(W::LocalGet(self.saved_sp));
            self.op(W::I32Const(size as i32));
            self.op(W::I32LtU);
            self.op(W::If(BlockType::Empty));
            self.op(W::Unreachable);
            self.op(W::End);
            self.op(W::LocalGet(self.saved_sp));
            self.op(W::I32Const(size as i32));
            self.op(W::I32Sub);
            self.op(W::LocalTee(self.frame_base));
            self.op(W::I32Const(runtime.stack_bottom as i32));
            self.op(W::I32LtU);
            self.op(W::If(BlockType::Empty));
            self.op(W::Unreachable);
            self.op(W::End);
            self.op(W::LocalGet(self.frame_base));
            self.op(W::GlobalSet(0));
        }
        if !self.direct_return {
            self.op(W::Loop(BlockType::Empty));
        }
        for b in self.ir.blocks() {
            if !self.direct_return {
                self.op(W::LocalGet(self.pc));
                self.op(W::I32Const(self.blocks[&b]));
                self.op(W::I32Eq);
                self.op(W::If(BlockType::Empty));
            }
            for &i in self.ir.block_insts(b) {
                match self.ir.inst_data(i) {
                    InstData::StackAddr { slot } => {
                        self.op(W::LocalGet(self.frame_base));
                        self.op(W::I32Const(
                            self.frame.unwrap().offsets[slot.index()] as i32,
                        ));
                        self.op(W::I32Add);
                    }
                    InstData::SymbolAddr { symbol } => {
                        self.op(W::I32Const(
                            self.placement.unwrap().addresses[self.ir.symbol_name(*symbol)] as i32,
                        ));
                    }
                    InstData::Iconst { ty, imm } => self.constant(*ty, *imm),
                    InstData::Unary {
                        op: UnaryOp::Ineg,
                        arg,
                    } => {
                        let ty = self.ir.value_type(self.ir.resolve(*arg));
                        // MVP subtraction wraps at the operand width, including
                        // INT_MIN; keep the input local live and resolve aliases.
                        self.constant(ty, 0);
                        self.get(*arg);
                        self.op(binary(BinaryOp::Isub, ty).unwrap());
                    }
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        args,
                    } => {
                        self.get(args[0]);
                        self.get(args[1]);
                        self.op(W::I32WrapI64);
                        self.op(W::I32Add);
                    }
                    InstData::Convert { op, ty, arg } => self.convert(*op, *ty, *arg),
                    InstData::Load { ty, addr, flags } => {
                        self.get(*addr);
                        let m = Self::memarg(*ty, flags.align);
                        self.op(match ty {
                            Type::I8 => W::I32Load8U(m),
                            Type::I16 => W::I32Load16U(m),
                            Type::I32 | Type::Ptr => W::I32Load(m),
                            Type::I64 => W::I64Load(m),
                            _ => unreachable!("preflight"),
                        });
                    }
                    InstData::Store { value, addr, flags } => {
                        self.get(*addr);
                        self.get(*value);
                        let ty = self.ir.value_type(*value);
                        let m = Self::memarg(ty, flags.align);
                        self.op(match ty {
                            Type::I8 => W::I32Store8(m),
                            Type::I16 => W::I32Store16(m),
                            Type::I32 | Type::Ptr => W::I32Store(m),
                            Type::I64 => W::I64Store(m),
                            _ => unreachable!("preflight"),
                        });
                    }
                    InstData::Binary { op, args } => {
                        self.get(args[0]);
                        self.get(args[1]);
                        let a = self.ir.value_type(args[0]);
                        let b = self.ir.value_type(args[1]);
                        // Shift counts may have a different integer width.
                        if a == Type::I64 && b != Type::I64 {
                            self.op(W::I64ExtendI32U);
                        }
                        if a != Type::I64 && b == Type::I64 {
                            self.op(W::I32WrapI64);
                        }
                        self.op(binary(*op, a).unwrap());
                    }
                    InstData::Icmp { cond, args } => {
                        self.get(args[0]);
                        self.get(args[1]);
                        self.op(comparison(*cond, self.ir.value_type(args[0])).unwrap());
                    }
                    InstData::Select { args } => {
                        self.get(args[1]);
                        self.get(args[2]);
                        self.condition(args[0]);
                        self.op(W::Select);
                    }
                    InstData::Call { func, args } => {
                        for &v in args {
                            self.get(v);
                        }
                        self.op(W::Call(self.indices[self.ir.callee(*func).name.as_str()]));
                    }
                    InstData::Jump { dest } => self.edge(dest, 1),
                    InstData::Brif { cond, dests } => {
                        self.condition(*cond);
                        self.op(W::If(BlockType::Empty));
                        self.edge(&dests[0], 2);
                        self.op(W::Else);
                        self.edge(&dests[1], 2);
                        self.op(W::End);
                    }
                    InstData::Switch { arg, dests, cases } => {
                        for (case, dest) in cases.iter().zip(&dests[1..]) {
                            self.get(*arg);
                            self.constant(self.ir.value_type(*arg), *case);
                            self.op(if self.ir.value_type(*arg) == Type::I64 {
                                W::I64Eq
                            } else {
                                W::I32Eq
                            });
                            self.op(W::If(BlockType::Empty));
                            self.edge(dest, 2);
                            self.op(W::End);
                        }
                        self.edge(&dests[0], 1);
                    }
                    InstData::Return { args } => {
                        for &v in args {
                            self.get(v);
                        }
                        if self.runtime.is_some() {
                            self.op(W::LocalGet(self.saved_sp));
                            self.op(W::GlobalSet(0));
                        }
                        self.op(W::Return);
                    }
                    _ => unreachable!("preflight"),
                }
                for &v in self.ir.inst_results(i).iter().rev() {
                    self.op(W::LocalSet(self.locals[v.index()].unwrap()));
                }
            }
            if !self.direct_return {
                self.op(W::End);
            }
        }
        if !self.direct_return {
            self.op(W::Unreachable); // No valid PC may fall through the dispatch.
            self.op(W::End);
            self.op(W::Unreachable); // A loop has no normal function exit.
        }
        self.op(W::End);
        self.out.finish()
    }
}
