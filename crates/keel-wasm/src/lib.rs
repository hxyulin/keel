//! Experimental pointer-free scalar core WebAssembly output. No native or C ABI.
use keel_ir::{
    ArgExt, BinaryOp, Block, BlockCall, CallConv, EntityRef, Function, FunctionAttrs, InstData,
    IntCC, Module, Purpose, Signature, Type, Value,
};
use std::collections::{HashMap, HashSet};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, FunctionSection, Instruction as W,
    TypeSection, ValType,
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
        Type::I8 | Type::I32 => Ok(ValType::I32),
        Type::I64 => Ok(ValType::I64),
        _ => Err(reject(format!("unsupported value type {t}"))),
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
    keel_ir::verify_module(module).map_err(|errors| {
        reject(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    if !module.data.is_empty() {
        return Err(reject("data/memory is unsupported"));
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
                || f.num_stack_slots() != 0
                || f.data_symbols().next().is_some()
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
                    local_type(f.value_type(v))?;
                }
                for &i in f.block_insts(b) {
                    let d = f.inst_data(i);
                    let ty = |v| f.value_type(f.resolve(v));
                    for &v in f.inst_results(i) {
                        local_type(f.value_type(v))?;
                    }
                    let mut bad = None;
                    d.for_each_operand(|v| {
                        if let Err(e) = local_type(ty(v)) {
                            bad = Some(e);
                        }
                    });
                    if let Some(e) = bad {
                        return Err(e);
                    }
                    let supported = match d {
                        InstData::Iconst { ty, .. } => {
                            matches!(ty, Type::I8 | Type::I32 | Type::I64)
                        }
                        InstData::Binary { op, args } => binary(*op, ty(args[0])).is_some(),
                        InstData::Icmp { cond, args } => comparison(*cond, ty(args[0])).is_some(),
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
        code.function(&Body::new(f, &indices).emit());
    }
    for e in &options.exports {
        exports.export(&e.name, ExportKind::Func, indices[e.function.as_str()]);
    }
    let mut output = wasm_encoder::Module::new();
    output
        .section(&types)
        .section(&functions)
        .section(&exports)
        .section(&code);
    let bytes = output.finish();
    // Keep the output contract independent of the validator's evolving defaults.
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&bytes)
        .map_err(|e| reject(format!("generated binary failed validation: {e}")))?;
    Ok(bytes)
}

struct Body<'a> {
    ir: &'a Function,
    indices: &'a HashMap<&'a str, u32>,
    locals: Vec<Option<u32>>,
    pc: u32,
    blocks: HashMap<Block, i32>,
    out: wasm_encoder::Function,
}
impl<'a> Body<'a> {
    fn new(ir: &'a Function, indices: &'a HashMap<&'a str, u32>) -> Self {
        let mut locals = vec![None; ir.num_values()];
        let entry = ir.entry_block().unwrap();
        for (n, &v) in ir.block_params(entry).iter().enumerate() {
            locals[v.index()] = Some(n as u32);
        }
        let mut declarations = Vec::new();
        let mut next = ir.sig.params.len() as u32;
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
        declarations.push((1, ValType::I32));
        Self {
            ir,
            indices,
            locals,
            pc: next,
            blocks: ir
                .blocks()
                .enumerate()
                .map(|(i, b)| (b, i as i32))
                .collect(),
            out: wasm_encoder::Function::new(declarations),
        }
    }
    fn op(&mut self, op: W<'_>) {
        self.out.instruction(&op);
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
            } else {
                imm as i32
            })
        });
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
        self.op(W::Loop(BlockType::Empty));
        for b in self.ir.blocks() {
            self.op(W::LocalGet(self.pc));
            self.op(W::I32Const(self.blocks[&b]));
            self.op(W::I32Eq);
            self.op(W::If(BlockType::Empty));
            for &i in self.ir.block_insts(b) {
                match self.ir.inst_data(i) {
                    InstData::Iconst { ty, imm } => self.constant(*ty, *imm),
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
                        self.op(W::Return);
                    }
                    _ => unreachable!("preflight"),
                }
                for &v in self.ir.inst_results(i).iter().rev() {
                    self.op(W::LocalSet(self.locals[v.index()].unwrap()));
                }
            }
            self.op(W::End);
        }
        self.op(W::Unreachable); // No valid PC may fall through the dispatch.
        self.op(W::End);
        self.op(W::Unreachable); // A loop has no normal function exit.
        self.op(W::End);
        self.out
    }
}
