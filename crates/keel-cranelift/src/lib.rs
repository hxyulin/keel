//! Experimental, diagnostic Keel IR → Cranelift bridge. It is not a
//! production backend and does not replace the native one. See README.md for
//! the supported subset, ABI limits and semantic mapping.
//!
//! Translation is strict: any unsupported module, signature or instruction
//! feature fails with the function, block and instruction that caused it, and
//! no partial object is produced.
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::condcodes::{FloatCC as CFloatCC, IntCC as CIntCC};
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::{
    self, AbiParam as CAbiParam, Endianness, InstBuilder, MemFlagsData, StackSlotData,
    StackSlotKind, TrapCode, UserFuncName, types,
};
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa, TargetFrontendConfig};
use cranelift_codegen::{Context, settings, settings::Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module as _, ModuleReloc};
use cranelift_object::{ObjectBuilder, ObjectModule};
use keel_ir::{self as k, EntityRef};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub type Result<T> = std::result::Result<T, String>;

/// The only validated target: Apple arm64, Mach-O, Apple's AAPCS64 variant.
/// The CLI spells it as `keel-llvm` does; it never infers the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetProfile {
    AppleArm64,
}
impl TargetProfile {
    pub const CLI_NAME: &'static str = "arm64-apple-macosx11.0.0";
    pub fn from_cli(name: &str) -> Option<Self> {
        (name == Self::CLI_NAME).then_some(Self::AppleArm64)
    }
    /// target-lexicon triple; the version becomes LC_BUILD_VERSION minos.
    pub fn cranelift_triple(self) -> &'static str {
        "aarch64-apple-macosx11.0.0"
    }
}

/// Cranelift's `opt_level` setting. `speed_and_size` is deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptLevel {
    None,
    Speed,
}
impl OptLevel {
    pub fn setting(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Speed => "speed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "speed" => Some(Self::Speed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub target: TargetProfile,
    pub opt: OptLevel,
    /// Run Cranelift's CLIF verifier inside every compile (its default).
    /// Benchmarks disable it and verify separately; output is identical.
    pub verify_clif: bool,
}

/// Every non-default Cranelift setting this bridge applies, for evidence.
pub fn explicit_settings(options: &Options) -> Vec<(&'static str, String)> {
    vec![
        ("opt_level", options.opt.setting().into()),
        ("is_pic", "true".into()),
        ("unwind_info", "false".into()),
        ("enable_verifier", options.verify_clif.to_string()),
    ]
}

/// Build the target ISA. Only shared flags are set; AArch64 ISA flags keep
/// their baseline defaults (no LSE/PAuth/BTI assumptions, no host probing).
pub fn build_isa(options: &Options) -> Result<OwnedTargetIsa> {
    let mut flags = settings::builder();
    for (name, value) in explicit_settings(options) {
        flags
            .set(name, &value)
            .map_err(|e| format!("setting {name}: {e}"))?;
    }
    let triple: target_lexicon::Triple = options
        .target
        .cranelift_triple()
        .parse()
        .map_err(|e| format!("triple: {e}"))?;
    cranelift_codegen::isa::lookup(triple)
        .map_err(|e| format!("isa lookup: {e}"))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| format!("isa: {e}"))
}

/// Wall-clock time of each stage, measured in-process.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// ISA construction and ObjectModule creation.
    pub setup: Duration,
    /// Symbol declaration and Keel IR → CLIF construction (no Keel verify).
    pub translate: Duration,
    /// `Context::compile` per function: CLIF verify (if enabled), optimizer,
    /// lowering, register allocation and machine-code emission to a buffer,
    /// plus relocation capture.
    pub codegen: Duration,
    /// Defining function bytes/data in the object and serializing Mach-O.
    pub object: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub functions: usize,
    pub data_objects: usize,
    pub clif_instructions: usize,
    pub code_bytes: usize,
}

/// Verify Keel IR, translate, compile and serialize a complete Mach-O object.
pub fn compile_object(module: &k::Module, options: &Options) -> Result<Vec<u8>> {
    k::verify_module(module).map_err(|e| format!("invalid Keel IR: {e:?}"))?;
    compile_verified(module, options).map(|(object, _, _)| object)
}

/// As [`compile_object`] for input the caller has already verified, with
/// per-stage timings. Benchmarks use this so shared parsing and Keel
/// verification are timed once, outside either backend.
pub fn compile_verified(
    module: &k::Module,
    options: &Options,
) -> Result<(Vec<u8>, Timings, Stats)> {
    let mut timings = Timings::default();
    let start = Instant::now();
    let isa = build_isa(options)?;
    let object = ObjectModule::new(
        ObjectBuilder::new(isa, "keel", cranelift_module::default_libcall_names())
            .map_err(|e| e.to_string())?,
    );
    timings.setup = start.elapsed();

    let start = Instant::now();
    let translation = translate_into(module, object)?;
    timings.translate = start.elapsed();

    let mut stats = Stats {
        functions: translation.functions.len(),
        data_objects: translation.data.len(),
        clif_instructions: translation
            .functions
            .iter()
            .map(|(_, _, f)| f.dfg.num_insts())
            .sum(),
        code_bytes: 0,
    };
    let start = Instant::now();
    let compiled = translation.codegen()?;
    timings.codegen = start.elapsed();
    stats.code_bytes = compiled.functions.iter().map(|f| f.bytes.len()).sum();

    let start = Instant::now();
    let bytes = compiled.emit()?;
    timings.object = start.elapsed();
    Ok((bytes, timings, stats))
}

/// Translate to textual CLIF, one function after another, for inspection.
pub fn translate_to_clif(module: &k::Module, options: &Options) -> Result<String> {
    k::verify_module(module).map_err(|e| format!("invalid Keel IR: {e:?}"))?;
    let isa = build_isa(options)?;
    let object = ObjectModule::new(
        ObjectBuilder::new(isa, "keel", cranelift_module::default_libcall_names())
            .map_err(|e| e.to_string())?,
    );
    let translation = translate_into(module, object)?;
    let mut out = String::new();
    for (_, name, f) in &translation.functions {
        out.push_str(&format!("; @{name}\n{}\n", f.display()));
    }
    Ok(out)
}

/// Translate each function independently, reporting which are unsupported.
/// For coverage reporting only; objects are never produced from a partial
/// module.
pub fn function_support(module: &k::Module) -> Vec<(String, Result<()>)> {
    let options = Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    };
    let mut out = Vec::new();
    if let Err(e) = check_module(module) {
        return vec![("<module>".into(), Err(e))];
    }
    for f in &module.functions {
        let mut single = module.clone();
        single.functions.retain(|g| g.name == f.name);
        // Other definitions remain callable as imports.
        for g in &module.functions {
            if g.name != f.name && !single.decls.iter().any(|d| d.name == g.name) {
                single.decls.push(k::FuncDecl {
                    attrs: g.attrs,
                    name: g.name.clone(),
                    sig: g.sig.clone(),
                });
            }
        }
        let result = (|| {
            let isa = build_isa(&options)?;
            let object = ObjectModule::new(
                ObjectBuilder::new(isa, "keel", cranelift_module::default_libcall_names())
                    .map_err(|e| e.to_string())?,
            );
            translate_into(&single, object)?.codegen().map(|_| ())
        })();
        out.push((f.name.clone(), result));
    }
    out
}

/// Translated CLIF functions and data, before code generation.
pub struct Translation {
    object: ObjectModule,
    functions: Vec<(FuncId, String, ir::Function)>,
    data: Vec<(DataId, DataDescription)>,
}

struct CompiledFunction {
    id: FuncId,
    alignment: u64,
    bytes: Vec<u8>,
    relocs: Vec<ModuleReloc>,
}

pub struct Compiled {
    object: ObjectModule,
    functions: Vec<CompiledFunction>,
    data: Vec<(DataId, DataDescription)>,
}

impl Translation {
    pub fn functions(&self) -> impl Iterator<Item = (&str, &ir::Function)> {
        self.functions.iter().map(|(_, n, f)| (n.as_str(), f))
    }

    /// Cranelift code generation for every function, in module order.
    pub fn codegen(self) -> Result<Compiled> {
        let mut ctx = Context::new();
        let mut out = Vec::with_capacity(self.functions.len());
        for (id, name, func) in self.functions {
            ctx.clear();
            ctx.func = func;
            ctx.compile(self.object.isa(), &mut ControlPlane::default())
                .map_err(|e| {
                    format!(
                        "function @{name}: cranelift codegen: {}",
                        cranelift_codegen::print_errors::pretty_error(e.func, e.inner)
                    )
                })?;
            let code = ctx.compiled_code().expect("compiled");
            let alignment = u64::from(code.buffer.alignment);
            let bytes = code.code_buffer().to_vec();
            let relocs = code
                .buffer
                .relocs()
                .iter()
                .map(|r| ModuleReloc::from_mach_reloc(r, &ctx.func, id))
                .collect();
            out.push(CompiledFunction {
                id,
                alignment,
                bytes,
                relocs,
            });
        }
        Ok(Compiled {
            object: self.object,
            functions: out,
            data: self.data,
        })
    }
}

impl Compiled {
    /// Define everything in the object module and serialize Mach-O bytes.
    pub fn emit(mut self) -> Result<Vec<u8>> {
        for f in &self.functions {
            self.object
                .define_function_bytes(f.id, f.alignment, &f.bytes, &f.relocs)
                .map_err(|e| format!("define function: {e}"))?;
        }
        for (id, desc) in &self.data {
            self.object
                .define_data(*id, desc)
                .map_err(|e| format!("define data: {e}"))?;
        }
        self.object
            .finish()
            .emit()
            .map_err(|e| format!("object write: {e}"))
    }
}

/// Module-level features this bridge rejects before translating any body.
fn check_module(module: &k::Module) -> Result<()> {
    if !module.files.is_empty()
        || !module.debug_types.is_empty()
        || !module.debug_globals.is_empty()
        || module.debug_language.is_some()
    {
        return Err("debug information is unsupported; generate IR without -g".into());
    }
    for d in &module.data {
        if d.thread_local {
            return Err(format!(
                "data @{}: thread-local storage is unsupported",
                d.name
            ));
        }
    }
    Ok(())
}

fn ty(t: k::Type) -> Result<ir::Type> {
    Ok(match t {
        k::Type::I8 => types::I8,
        k::Type::I16 => types::I16,
        k::Type::I32 => types::I32,
        k::Type::I64 | k::Type::Ptr => types::I64,
        k::Type::I128 => types::I128,
        k::Type::F32 => types::F32,
        k::Type::F64 => types::F64,
        k::Type::Vector(v) => match v {
            k::VectorType::I8x16 => types::I8X16,
            k::VectorType::I16x8 => types::I16X8,
            k::VectorType::I32x4 => types::I32X4,
            k::VectorType::I64x2 => types::I64X2,
            k::VectorType::F32x4 => types::F32X4,
            k::VectorType::F64x2 => types::F64X2,
        },
    })
}

/// Map an ABI parameter. Only ordinary scalar C values are accepted.
fn abi_param(p: &k::AbiParam) -> Result<CAbiParam> {
    if p.purpose != k::Purpose::Normal {
        return Err("aggregate byval/sret ABI is unsupported".into());
    }
    match p.ty {
        k::Type::I128 => return Err("i128 ABI values are unsupported".into()),
        k::Type::Vector(_) => return Err("vector ABI values are unsupported".into()),
        _ => {}
    }
    let param = CAbiParam::new(ty(p.ty)?);
    Ok(match p.ext {
        k::ArgExt::None => param,
        k::ArgExt::Sext => param.sext(),
        k::ArgExt::Uext => param.uext(),
    })
}

fn signature(s: &k::Signature) -> Result<ir::Signature> {
    if s.variadic {
        return Err("variadic signatures are unsupported".into());
    }
    if s.returns_twice {
        return Err("returns_twice signatures are unsupported".into());
    }
    if s.returns.len() > 1 {
        return Err("multiple-result signatures are unsupported".into());
    }
    let mut sig = ir::Signature::new(CallConv::AppleAarch64);
    for p in &s.params {
        sig.params.push(abi_param(p)?);
    }
    for p in &s.returns {
        sig.returns.push(abi_param(p)?);
    }
    Ok(sig)
}

enum Sym {
    Func(FuncId),
    Data(DataId),
}

/// Declare all symbols, then build each function and data description.
pub fn translate(module: &k::Module, options: &Options) -> Result<Translation> {
    k::verify_module(module).map_err(|e| format!("invalid Keel IR: {e:?}"))?;
    let isa = build_isa(options)?;
    let object = ObjectModule::new(
        ObjectBuilder::new(isa, "keel", cranelift_module::default_libcall_names())
            .map_err(|e| e.to_string())?,
    );
    translate_into(module, object)
}

fn translate_into(module: &k::Module, mut object: ObjectModule) -> Result<Translation> {
    check_module(module)?;
    let mut symbols: HashMap<&str, Sym> = HashMap::new();
    for d in &module.data {
        let linkage = match &d.init {
            None => Linkage::Import,
            Some(_) if d.local => Linkage::Local,
            Some(_) => Linkage::Export,
        };
        let writable = d.init.as_ref().is_none_or(|i| !i.constant);
        let id = object
            .declare_data(&d.name, linkage, writable, false)
            .map_err(|e| format!("data @{}: {e}", d.name))?;
        if symbols.insert(&d.name, Sym::Data(id)).is_some() {
            return Err(format!("@{}: duplicate symbol", d.name));
        }
    }
    let mut signatures = Vec::with_capacity(module.functions.len());
    for f in &module.functions {
        let sig = signature(&f.sig).map_err(|e| format!("function @{}: {e}", f.name))?;
        if f.personality.is_some() {
            return Err(format!(
                "function @{}: exception handling is unsupported",
                f.name
            ));
        }
        let linkage = if f.local {
            Linkage::Local
        } else {
            Linkage::Export
        };
        let id = object
            .declare_function(&f.name, linkage, &sig)
            .map_err(|e| format!("function @{}: {e}", f.name))?;
        if symbols.insert(&f.name, Sym::Func(id)).is_some() {
            return Err(format!("@{}: duplicate symbol", f.name));
        }
        signatures.push((id, sig));
    }
    // Function-local callees (calls and function addresses), and module
    // declarations named by data relocations, become imports. Unreferenced
    // declarations, such as header prototypes, need no symbol and are not
    // checked. cranelift-module rejects any signature that disagrees.
    let relocated: std::collections::HashSet<&str> = module
        .data
        .iter()
        .filter_map(|d| d.init.as_ref())
        .flat_map(|i| &i.items)
        .filter_map(|item| match item {
            k::DataItem::Ptr { symbol, .. } => Some(symbol.as_str()),
            _ => None,
        })
        .collect();
    let decls = module
        .functions
        .iter()
        .flat_map(|f| f.callees().map(|(_, d)| d))
        .chain(
            module
                .decls
                .iter()
                .filter(|d| relocated.contains(d.name.as_str())),
        );
    for d in decls {
        let sig = signature(&d.sig).map_err(|e| format!("@{}: {e}", d.name))?;
        match symbols.get(d.name.as_str()) {
            Some(Sym::Data(_)) => {
                return Err(format!("@{}: declared as data and function", d.name));
            }
            Some(Sym::Func(_)) | None => {}
        }
        let id = object
            .declare_function(&d.name, Linkage::Import, &sig)
            .map_err(|e| format!("@{}: conflicting declaration: {e}", d.name))?;
        symbols.entry(&d.name).or_insert(Sym::Func(id));
    }

    let mut data = Vec::new();
    for d in &module.data {
        let Some(init) = &d.init else { continue };
        let Some(Sym::Data(id)) = symbols.get(d.name.as_str()) else {
            unreachable!()
        };
        let mut desc = DataDescription::new();
        desc.set_align(u64::from(init.align));
        if init.is_zero() {
            desc.define_zeroinit(init.size() as usize);
        } else {
            let mut bytes = Vec::with_capacity(init.size() as usize);
            for item in &init.items {
                match item {
                    k::DataItem::Bytes(b) => bytes.extend_from_slice(b),
                    k::DataItem::Zero(n) => bytes.resize(bytes.len() + *n as usize, 0),
                    k::DataItem::Int { ty: t, value } => {
                        // A `ptr` integer item is an 8-byte address-sized value.
                        let n = t.bytes().unwrap_or(8) as usize;
                        bytes.extend_from_slice(&value.to_le_bytes()[..n.min(8)]);
                        // i128 data: sign-extend the 64-bit value.
                        if n == 16 {
                            let fill = if *value < 0 { 0xff } else { 0 };
                            bytes.extend_from_slice(&[fill; 8]);
                        }
                    }
                    k::DataItem::Ptr { symbol, addend } => {
                        let offset = bytes.len() as u32;
                        bytes.extend_from_slice(&[0; 8]);
                        match symbols.get(symbol.as_str()) {
                            Some(Sym::Data(target)) => {
                                let gv = object.declare_data_in_data(*target, &mut desc);
                                desc.write_data_addr(offset, gv, i64::from(*addend));
                            }
                            Some(Sym::Func(target)) if *addend == 0 => {
                                let fref = object.declare_func_in_data(*target, &mut desc);
                                desc.write_function_addr(offset, fref);
                            }
                            Some(Sym::Func(_)) => {
                                return Err(format!(
                                    "data @{}: function address with addend is unsupported",
                                    d.name
                                ));
                            }
                            None => {
                                return Err(format!(
                                    "data @{}: unknown relocation symbol @{symbol}",
                                    d.name
                                ));
                            }
                        }
                    }
                }
            }
            desc.define(bytes.into_boxed_slice());
        }
        data.push((*id, desc));
    }

    let config = object.target_config();
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut functions = Vec::with_capacity(module.functions.len());
    for (f, (id, sig)) in module.functions.iter().zip(signatures) {
        let mut func = ir::Function::with_name_signature(UserFuncName::user(0, id.as_u32()), sig);
        FnTranslator::run(
            f,
            &mut func,
            &mut builder_ctx,
            &mut object,
            &symbols,
            config,
        )
        .map_err(|e| format!("function @{}: {e}", f.name))?;
        functions.push((id, f.name.clone(), func));
    }
    Ok(Translation {
        object,
        functions,
        data,
    })
}

/// Keel blocks reachable from the entry, in reverse postorder, so every
/// dominating definition is translated before its uses. Unreachable blocks
/// cannot be targeted by reachable code and are omitted.
fn reverse_postorder(f: &k::Function) -> Vec<k::Block> {
    let Some(entry) = f.entry_block() else {
        return Vec::new();
    };
    let mut visited = vec![false; f.num_blocks()];
    let mut post = Vec::new();
    let mut stack: Vec<(k::Block, usize)> = vec![(entry, 0)];
    visited[entry.index()] = true;
    while let Some((block, next)) = stack.pop() {
        let succs = f
            .block_insts(block)
            .last()
            .map(|&i| f.inst_data(i).branch_dests())
            .unwrap_or(&[]);
        if let Some(dest) = succs.get(next) {
            stack.push((block, next + 1));
            if !visited[dest.block.index()] {
                visited[dest.block.index()] = true;
                stack.push((dest.block, 0));
            }
        } else {
            post.push(block);
        }
    }
    post.reverse();
    post
}

struct FnTranslator<'a, 'b> {
    f: &'a k::Function,
    b: FunctionBuilder<'b>,
    object: &'a mut ObjectModule,
    symbols: &'a HashMap<&'a str, Sym>,
    config: TargetFrontendConfig,
    values: Vec<Option<ir::Value>>,
    blocks: Vec<Option<ir::Block>>,
    slots: Vec<ir::StackSlot>,
    callees: Vec<Option<ir::FuncRef>>,
    sigs: Vec<Option<ir::SigRef>>,
}

fn unsupported<T>(what: impl std::fmt::Display) -> Result<T> {
    Err(format!("unsupported: {what}"))
}

impl<'a, 'b> FnTranslator<'a, 'b> {
    fn run(
        f: &'a k::Function,
        func: &'b mut ir::Function,
        builder_ctx: &'b mut FunctionBuilderContext,
        object: &'a mut ObjectModule,
        symbols: &'a HashMap<&'a str, Sym>,
        config: TargetFrontendConfig,
    ) -> Result<()> {
        // Bare lexical scopes (the inliner adds them even without -g) carry
        // no locations or variables and produce no debug info; ignore them.
        if f.debug_return.is_some()
            || f.debug_loc.is_some()
            || !f.debug_vars.is_empty()
            || f.layout_insts().any(|(_, i)| f.inst_loc(i).is_some())
        {
            return Err("debug information is unsupported".into());
        }
        let entry = f.entry_block().ok_or("missing entry block")?;
        let order = reverse_postorder(f);
        let mut t = FnTranslator {
            f,
            b: FunctionBuilder::new(func, builder_ctx),
            object,
            symbols,
            config,
            values: vec![None; f.num_values()],
            blocks: vec![None; f.num_blocks()],
            slots: Vec::new(),
            callees: vec![None; f.callees().count()],
            sigs: vec![None; f.signatures().count()],
        };
        for (_, data) in f.stack_slots() {
            let slot = t.b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                data.size,
                data.align.trailing_zeros() as u8,
            ));
            t.slots.push(slot);
        }
        for &block in &order {
            let cb = t.b.create_block();
            for &param in f.block_params(block) {
                let v = t.b.append_block_param(cb, ty(f.value_type(param))?);
                t.values[param.index()] = Some(v);
            }
            t.blocks[block.index()] = Some(cb);
        }
        // The Keel verifier forbids branches to the entry block, whose
        // parameters are exactly the signature's, as CLIF requires.
        let mapped_entry = t.blocks[entry.index()].unwrap();
        let types: Vec<_> =
            t.b.func
                .signature
                .params
                .iter()
                .map(|p| p.value_type)
                .collect();
        let actual: Vec<_> =
            t.b.block_params(mapped_entry)
                .iter()
                .map(|&v| t.b.func.dfg.value_type(v))
                .collect();
        if types != actual {
            return Err("entry block parameters do not match the signature".into());
        }
        for &block in &order {
            t.b.switch_to_block(t.blocks[block.index()].unwrap());
            for &inst in f.block_insts(block) {
                t.inst(inst)
                    .map_err(|e| format!("{block}, {inst} ({}): {e}", f.display_inst(inst)))?;
            }
        }
        t.b.seal_all_blocks();
        t.b.finalize(config);
        Ok(())
    }

    fn v(&self, v: k::Value) -> Result<ir::Value> {
        let v = self.f.resolve(v);
        self.values[v.index()].ok_or_else(|| format!("{v} used before translation"))
    }

    fn vs(&self, vs: &[k::Value]) -> Result<Vec<ir::Value>> {
        vs.iter().map(|&v| self.v(v)).collect()
    }

    fn def(&mut self, inst: k::Inst, n: usize, v: ir::Value) {
        let r = self.f.inst_results(inst)[n];
        self.values[r.index()] = Some(v);
    }

    fn block_call(&self, call: &k::BlockCall) -> Result<(ir::Block, Vec<ir::BlockArg>)> {
        let block = self.blocks[call.block.index()].ok_or("branch to untranslated block")?;
        let args = call
            .args
            .iter()
            .map(|&a| self.v(a).map(ir::BlockArg::Value))
            .collect::<Result<_>>()?;
        Ok((block, args))
    }

    fn iconst(&mut self, t: k::Type, imm: i64) -> Result<ir::Value> {
        Ok(match t {
            k::Type::I8 | k::Type::I16 | k::Type::I32 => {
                // CLIF immediates must not carry bits above the type width.
                let bits = t.bytes().unwrap() * 8;
                let masked = (imm as u64) & ((1u64 << bits) - 1);
                self.b.ins().iconst(ty(t)?, masked as i64)
            }
            k::Type::I64 | k::Type::Ptr => self.b.ins().iconst(types::I64, imm),
            k::Type::I128 => {
                let low = self.b.ins().iconst(types::I64, imm);
                self.b.ins().sextend(types::I128, low)
            }
            _ => return unsupported(format!("iconst of type {t}")),
        })
    }

    fn mem_flags(&self, flags: k::MemoryFlags, access: k::Type) -> MemFlagsData {
        let mut m = MemFlagsData::new().with_endianness(Endianness::Little);
        if flags.align >= access.bytes().unwrap_or(8) {
            m = m.with_aligned();
        }
        // A volatile access must stay an observable, side-effecting operation.
        if flags.notrap && !flags.volatile {
            m = m.with_notrap();
        }
        m
    }

    /// Bit-preserving reinterpretation between same-width CLIF types.
    fn bitcast(&mut self, to: ir::Type, v: ir::Value) -> ir::Value {
        if self.b.func.dfg.value_type(v) == to {
            return v;
        }
        let flags = MemFlagsData::new().with_endianness(Endianness::Little);
        self.b.ins().bitcast(to, flags, v)
    }

    fn inst(&mut self, inst: k::Inst) -> Result<()> {
        use k::InstData as I;
        let f = self.f;
        match f.inst_data(inst) {
            I::Iconst { ty: t, imm } => {
                let v = self.iconst(*t, *imm)?;
                self.def(inst, 0, v);
            }
            I::Fconst { ty: t, bits } => {
                let v = match t {
                    k::Type::F32 => self.b.ins().f32const(Ieee32::with_bits(*bits as u32)),
                    k::Type::F64 => self.b.ins().f64const(Ieee64::with_bits(*bits)),
                    _ => return unsupported(format!("fconst of type {t}")),
                };
                self.def(inst, 0, v);
            }
            I::Unary { op, arg } => {
                let x = self.v(*arg)?;
                let ins = self.b.ins();
                let v = match op {
                    k::UnaryOp::Ineg => ins.ineg(x),
                    k::UnaryOp::Bnot => ins.bnot(x),
                    k::UnaryOp::Clz => ins.clz(x),
                    k::UnaryOp::Ctz => ins.ctz(x),
                    k::UnaryOp::Popcnt => ins.popcnt(x),
                    k::UnaryOp::Bswap => ins.bswap(x),
                    k::UnaryOp::Fneg => ins.fneg(x),
                    k::UnaryOp::Fabs => ins.fabs(x),
                    k::UnaryOp::Fsqrt => ins.sqrt(x),
                    k::UnaryOp::Fceil => ins.ceil(x),
                    k::UnaryOp::Ffloor => ins.floor(x),
                    k::UnaryOp::Ftrunc => ins.trunc(x),
                    k::UnaryOp::Fnearest => ins.nearest(x),
                };
                self.def(inst, 0, v);
            }
            I::Binary { op, args: [a, c] } => {
                let lhs_ty = f.value_type(*a);
                let x = self.v(*a)?;
                let mut y = self.v(*c)?;
                use k::BinaryOp as B;
                if lhs_ty == k::Type::I128
                    && matches!(
                        op,
                        B::Udiv | B::Sdiv | B::Urem | B::Srem | B::Umulhi | B::Smulhi
                    )
                {
                    return unsupported(format!("i128 {op}"));
                }
                if matches!(op, B::Shl | B::Ushr | B::Sshr | B::Rotl | B::Rotr)
                    && f.value_type(*c) == k::Type::I128
                {
                    // Only the low bits of an amount matter after masking.
                    y = self.b.ins().ireduce(types::I64, y);
                }
                let ins = self.b.ins();
                let v = match op {
                    B::Iadd | B::Ptradd => ins.iadd(x, y),
                    B::Isub => ins.isub(x, y),
                    B::Imul => ins.imul(x, y),
                    // CLIF division traps exactly where Keel's does: zero
                    // divisors, and signed MIN / -1 for sdiv only.
                    B::Udiv => ins.udiv(x, y),
                    B::Sdiv => ins.sdiv(x, y),
                    B::Urem => ins.urem(x, y),
                    B::Srem => ins.srem(x, y),
                    B::And => ins.band(x, y),
                    B::Or => ins.bor(x, y),
                    B::Xor => ins.bxor(x, y),
                    // CLIF masks shift and rotate amounts modulo the width of
                    // the shifted operand, independent of the amount's type.
                    B::Shl => ins.ishl(x, y),
                    B::Ushr => ins.ushr(x, y),
                    B::Sshr => ins.sshr(x, y),
                    B::Rotl => ins.rotl(x, y),
                    B::Rotr => ins.rotr(x, y),
                    B::Umulhi => ins.umulhi(x, y),
                    B::Smulhi => ins.smulhi(x, y),
                    B::Fadd => ins.fadd(x, y),
                    B::Fsub => ins.fsub(x, y),
                    B::Fmul => ins.fmul(x, y),
                    B::Fdiv => ins.fdiv(x, y),
                    // NaN-propagating, -0 < +0: IEEE 754-2019 minimum/maximum.
                    B::Fmin => ins.fmin(x, y),
                    B::Fmax => ins.fmax(x, y),
                    B::Fcopysign => ins.fcopysign(x, y),
                };
                self.def(inst, 0, v);
            }
            I::Overflow { op, args: [a, c] } => {
                let t = f.value_type(*a);
                if t == k::Type::I128 && matches!(op, k::OverflowOp::Umul | k::OverflowOp::Smul) {
                    return unsupported(format!("i128 {op}"));
                }
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                let ins = self.b.ins();
                let (r, flag) = match op {
                    k::OverflowOp::Uadd => ins.uadd_overflow(x, y),
                    k::OverflowOp::Sadd => ins.sadd_overflow(x, y),
                    k::OverflowOp::Usub => ins.usub_overflow(x, y),
                    k::OverflowOp::Ssub => ins.ssub_overflow(x, y),
                    k::OverflowOp::Umul => ins.umul_overflow(x, y),
                    k::OverflowOp::Smul => ins.smul_overflow(x, y),
                };
                self.def(inst, 0, r);
                self.def(inst, 1, flag);
            }
            I::Fma { args } => {
                let [x, y, z] = [self.v(args[0])?, self.v(args[1])?, self.v(args[2])?];
                let v = self.b.ins().fma(x, y, z);
                self.def(inst, 0, v);
            }
            I::Select { args } => {
                let [c, x, y] = [self.v(args[0])?, self.v(args[1])?, self.v(args[2])?];
                let v = self.b.ins().select(c, x, y);
                self.def(inst, 0, v);
            }
            I::Convert { op, ty: to, arg } => {
                let from = f.value_type(*arg);
                let x = self.v(*arg)?;
                let to_ty = ty(*to)?;
                use k::ConvertOp as C;
                let v = match op {
                    C::SwidenLow | C::UwidenLow => {
                        // Keel observes only the lowest destination-lane-count
                        // source lanes, including multi-step width changes.
                        let dest = to.vector().expect("verified widening destination");
                        let lane_ty = ty(dest.lane_type())?;
                        let zero = self.b.ins().iconst(lane_ty, 0);
                        let mut value = self.b.ins().splat(to_ty, zero);
                        for lane in 0..dest.lanes() {
                            let part = self.b.ins().extractlane(x, lane);
                            let part = if *op == C::SwidenLow {
                                self.b.ins().sextend(lane_ty, part)
                            } else {
                                self.b.ins().uextend(lane_ty, part)
                            };
                            value = self.b.ins().insertlane(value, part, lane);
                        }
                        value
                    }
                    C::Sext => self.b.ins().sextend(to_ty, x),
                    C::Uext => self.b.ins().uextend(to_ty, x),
                    C::Trunc => self.b.ins().ireduce(to_ty, x),
                    // Pointers are 64-bit integers in CLIF.
                    C::PtrToInt | C::IntToPtr => x,
                    C::Fpext => self.b.ins().fpromote(to_ty, x),
                    C::Fptrunc => self.b.ins().fdemote(to_ty, x),
                    // Saturating, NaN → 0, exactly Keel's conversion contract.
                    C::Fptosi => self.b.ins().fcvt_to_sint_sat(to_ty, x),
                    C::Fptoui => self.b.ins().fcvt_to_uint_sat(to_ty, x),
                    C::Sitofp | C::Uitofp => {
                        let signed = *op == C::Sitofp;
                        let x = if matches!(from, k::Type::I8 | k::Type::I16) {
                            if signed {
                                self.b.ins().sextend(types::I32, x)
                            } else {
                                self.b.ins().uextend(types::I32, x)
                            }
                        } else {
                            x
                        };
                        if signed {
                            self.b.ins().fcvt_from_sint(to_ty, x)
                        } else {
                            self.b.ins().fcvt_from_uint(to_ty, x)
                        }
                    }
                    C::Bitcast => self.bitcast(to_ty, x),
                };
                self.def(inst, 0, v);
            }
            I::Icmp { cond, args: [a, c] } => {
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                // CLIF icmp produces i8 0/1 for scalars, as Keel does.
                let v = self.b.ins().icmp(int_cc(*cond), x, y);
                self.def(inst, 0, v);
            }
            I::Fcmp { cond, args: [a, c] } => {
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                let v = self.b.ins().fcmp(float_cc(*cond), x, y);
                self.def(inst, 0, v);
            }
            I::Load { flags, ty: t, addr } => {
                let p = self.v(*addr)?;
                let mf = self.mem_flags(*flags, *t);
                if flags.volatile {
                    self.b.ins().sequence_point();
                }
                let v = self.b.ins().load(ty(*t)?, mf, p, 0);
                if flags.volatile {
                    self.b.ins().sequence_point();
                }
                self.def(inst, 0, v);
            }
            I::Store { flags, value, addr } => {
                let (x, p) = (self.v(*value)?, self.v(*addr)?);
                let mf = self.mem_flags(*flags, f.value_type(*value));
                if flags.volatile {
                    self.b.ins().sequence_point();
                }
                self.b.ins().store(mf, x, p, 0);
                if flags.volatile {
                    self.b.ins().sequence_point();
                }
            }
            I::StackAddr { slot } => {
                let v = self
                    .b
                    .ins()
                    .stack_addr(types::I64, self.slots[slot.index()], 0);
                self.def(inst, 0, v);
            }
            I::SymbolAddr { symbol } => {
                let name = f.symbol_name(*symbol);
                let v = match self.symbols.get(name) {
                    Some(Sym::Func(id)) => {
                        let fref = self.object.declare_func_in_func(*id, self.b.func);
                        self.b.ins().func_addr(types::I64, fref)
                    }
                    Some(Sym::Data(id)) => {
                        let gv = self.object.declare_data_in_func(*id, self.b.func);
                        self.b.ins().symbol_value(types::I64, gv)
                    }
                    None => return Err(format!("unknown symbol @{name}")),
                };
                self.def(inst, 0, v);
            }
            I::Call { func, args } => {
                let fref = match self.callees[func.index()] {
                    Some(r) => r,
                    None => {
                        let name = &f.callee(*func).name;
                        let Some(Sym::Func(id)) = self.symbols.get(name.as_str()) else {
                            return Err(format!("unknown callee @{name}"));
                        };
                        let r = self.object.declare_func_in_func(*id, self.b.func);
                        self.callees[func.index()] = Some(r);
                        r
                    }
                };
                let args = self.vs(args)?;
                let call = self.b.ins().call(fref, &args);
                let results = self.b.inst_results(call).to_vec();
                for (n, r) in results.into_iter().enumerate() {
                    self.def(inst, n, r);
                }
            }
            I::CallIndirect { sig, callee, args } => {
                let sref = match self.sigs[sig.index()] {
                    Some(r) => r,
                    None => {
                        let s = signature(f.signature(*sig))?;
                        let r = self.b.import_signature(s);
                        self.sigs[sig.index()] = Some(r);
                        r
                    }
                };
                let target = self.v(*callee)?;
                let args = self.vs(args)?;
                let call = self.b.ins().call_indirect(sref, target, &args);
                let results = self.b.inst_results(call).to_vec();
                for (n, r) in results.into_iter().enumerate() {
                    self.def(inst, n, r);
                }
            }
            I::Intrinsic { id, args } => {
                let a = self.vs(args)?;
                match id {
                    // Calls to the C library, as the native backend does
                    // without small-memory expansion.
                    k::Intrinsic::Memcpy => self.b.call_memcpy(self.config, a[0], a[1], a[2]),
                    k::Intrinsic::Memmove => self.b.call_memmove(self.config, a[0], a[1], a[2]),
                    k::Intrinsic::Memset => self.b.call_memset(self.config, a[0], a[1], a[2]),
                    k::Intrinsic::Debugtrap => {
                        self.b.ins().debugtrap();
                    }
                    other => return unsupported(format!("intrinsic {other}")),
                }
            }
            I::Jump { dest } => {
                let (block, args) = self.block_call(dest)?;
                self.b.ins().jump(block, &args);
            }
            I::Brif { cond, dests } => {
                let c = self.v(*cond)?;
                let (t, targs) = self.block_call(&dests[0])?;
                let (e, eargs) = self.block_call(&dests[1])?;
                self.b.ins().brif(c, t, &targs, e, &eargs);
            }
            I::Switch { arg, dests, cases } => self.switch(*arg, dests, cases)?,
            I::Return { args } => {
                let args = self.vs(args)?;
                self.b.ins().return_(&args);
            }
            I::Trap => {
                self.b.ins().trap(TrapCode::unwrap_user(1));
            }
            I::VectorSplat { ty: t, arg } => {
                let x = self.v(*arg)?;
                let v = self.b.ins().splat(ty(*t)?, x);
                self.def(inst, 0, v);
            }
            I::VectorUnary { op, arg } => {
                let x = self.v(*arg)?;
                let ins = self.b.ins();
                let v = match op {
                    k::UnaryOp::Ineg => ins.ineg(x),
                    k::UnaryOp::Bnot => ins.bnot(x),
                    k::UnaryOp::Fneg => ins.fneg(x),
                    k::UnaryOp::Fabs => ins.fabs(x),
                    k::UnaryOp::Fsqrt => ins.sqrt(x),
                    k::UnaryOp::Fceil => ins.ceil(x),
                    k::UnaryOp::Ffloor => ins.floor(x),
                    k::UnaryOp::Ftrunc => ins.trunc(x),
                    k::UnaryOp::Fnearest => ins.nearest(x),
                    k::UnaryOp::Clz | k::UnaryOp::Ctz | k::UnaryOp::Popcnt | k::UnaryOp::Bswap => {
                        return unsupported(format!("vector {op}"));
                    }
                };
                self.def(inst, 0, v);
            }
            I::VectorBinary { op, args: [a, c] } => {
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                use k::VectorBinaryOp as V;
                let v = match op {
                    V::Shl | V::Ushr | V::Sshr => self.lanewise_shift(*op, x, y),
                    _ => {
                        let ins = self.b.ins();
                        match op {
                            V::Iadd => ins.iadd(x, y),
                            V::Isub => ins.isub(x, y),
                            V::Imul => ins.imul(x, y),
                            V::And => ins.band(x, y),
                            V::Or => ins.bor(x, y),
                            V::Xor => ins.bxor(x, y),
                            V::Fadd => ins.fadd(x, y),
                            V::Fsub => ins.fsub(x, y),
                            V::Fmul => ins.fmul(x, y),
                            V::Fdiv => ins.fdiv(x, y),
                            V::Fmin => ins.fmin(x, y),
                            V::Fmax => ins.fmax(x, y),
                            V::Fcopysign => ins.fcopysign(x, y),
                            V::Shl | V::Ushr | V::Sshr => unreachable!(),
                        }
                    }
                };
                self.def(inst, 0, v);
            }
            I::VectorExtract { arg, lane } => {
                let x = self.v(*arg)?;
                let v = self.b.ins().extractlane(x, *lane);
                self.def(inst, 0, v);
            }
            I::VectorInsert { args: [a, s], lane } => {
                let (x, y) = (self.v(*a)?, self.v(*s)?);
                let v = self.b.ins().insertlane(x, y, *lane);
                self.def(inst, 0, v);
            }
            I::VectorShuffle {
                args: [a, c],
                lanes,
            } => {
                let vt = f.value_type(*a).vector().unwrap();
                let lane_bytes = vt.lane_type().bytes().unwrap() as u8;
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                // Lane i of a concatenated pair starts at byte i * lane size,
                // which is also CLIF's byte index into (x ++ y).
                let mut mask = [0u8; 16];
                for (i, &lane) in lanes.iter().enumerate() {
                    for byte in 0..lane_bytes {
                        mask[i * lane_bytes as usize + byte as usize] = lane * lane_bytes + byte;
                    }
                }
                let orig = self.b.func.dfg.value_type(x);
                let (bx, by) = (self.bitcast(types::I8X16, x), self.bitcast(types::I8X16, y));
                let imm = self
                    .b
                    .func
                    .dfg
                    .immediates
                    .push(ir::ConstantData::from(mask.as_slice()));
                let s = self.b.ins().shuffle(bx, by, imm);
                let v = self.bitcast(orig, s);
                self.def(inst, 0, v);
            }
            I::VectorIcmp { cond, args: [a, c] } => {
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                let v = self.b.ins().icmp(int_cc(*cond), x, y);
                self.def(inst, 0, v);
            }
            I::VectorFcmp { cond, args: [a, c] } => {
                let (x, y) = (self.v(*a)?, self.v(*c)?);
                let v = self.b.ins().fcmp(float_cc(*cond), x, y);
                self.def(inst, 0, v);
            }
            I::VectorSelect { args } => {
                let [m, x, y] = [self.v(args[0])?, self.v(args[1])?, self.v(args[2])?];
                // Keel selects whole lanes on any nonzero mask lane; CLIF
                // bitselect blends bits, so normalize the mask to all ones.
                let mt = self.b.func.dfg.value_type(m);
                let zero = self.b.ins().iconst(mt.lane_type(), 0);
                let zero = self.b.ins().splat(mt, zero);
                let full = self.b.ins().icmp(CIntCC::NotEqual, m, zero);
                let at = self.b.func.dfg.value_type(x);
                let full = self.bitcast(at, full);
                let v = self.b.ins().bitselect(full, x, y);
                self.def(inst, 0, v);
            }
            I::AtomicLoad { .. }
            | I::AtomicStore { .. }
            | I::AtomicRmw { .. }
            | I::AtomicCas { .. }
            | I::Fence { .. } => return unsupported("atomics and fences"),
            I::VaArg { .. } => return unsupported("va_arg"),
            I::InlineAsm { .. } => return unsupported("inline assembly"),
            I::ReturnCall { .. } | I::ReturnCallIndirect { .. } => {
                return unsupported("guaranteed tail calls");
            }
            I::Invoke { .. } | I::LandingPad { .. } | I::EhTypeid { .. } => {
                return unsupported("exception handling");
            }
        }
        Ok(())
    }

    /// Keel vector shifts take per-lane amounts; CLIF vector shifts take one
    /// scalar amount. Split into lanes; each scalar shift masks as Keel does.
    fn lanewise_shift(&mut self, op: k::VectorBinaryOp, x: ir::Value, y: ir::Value) -> ir::Value {
        let vt = self.b.func.dfg.value_type(x);
        let mut out = x;
        for lane in 0..vt.lane_count() as u8 {
            let a = self.b.ins().extractlane(x, lane);
            let s = self.b.ins().extractlane(y, lane);
            let r = match op {
                k::VectorBinaryOp::Shl => self.b.ins().ishl(a, s),
                k::VectorBinaryOp::Ushr => self.b.ins().ushr(a, s),
                _ => self.b.ins().sshr(a, s),
            };
            out = self.b.ins().insertlane(out, r, lane);
        }
        out
    }

    /// Switch via cranelift-frontend's search tree / jump tables. Edges
    /// carrying arguments go through a trampoline block each, so parallel
    /// edges to one block may pass different values.
    fn switch(&mut self, arg: k::Value, dests: &[k::BlockCall], cases: &[i64]) -> Result<()> {
        let t = self.f.value_type(arg);
        let bits = match t {
            k::Type::I8 | k::Type::I16 | k::Type::I32 | k::Type::I64 => t.bytes().unwrap() * 8,
            _ => return unsupported(format!("switch on {t}")),
        };
        let x = self.v(arg)?;
        let mut targets = Vec::with_capacity(dests.len());
        let mut trampolines = Vec::new();
        for d in dests {
            let (block, args) = self.block_call(d)?;
            if args.is_empty() {
                targets.push(block);
            } else {
                let tramp = self.b.create_block();
                trampolines.push((tramp, block, args));
                targets.push(tramp);
            }
        }
        let mut switch = Switch::new();
        for (case, &target) in cases.iter().zip(&targets[1..]) {
            let key = if bits == 64 {
                *case as u64
            } else {
                (*case as u64) & ((1u64 << bits) - 1)
            };
            switch.set_entry(u128::from(key), target);
        }
        switch.emit(&mut self.b, x, targets[0]);
        for (tramp, block, args) in trampolines {
            self.b.switch_to_block(tramp);
            self.b.ins().jump(block, &args);
        }
        Ok(())
    }
}

fn int_cc(c: k::IntCC) -> CIntCC {
    match c {
        k::IntCC::Eq => CIntCC::Equal,
        k::IntCC::Ne => CIntCC::NotEqual,
        k::IntCC::Slt => CIntCC::SignedLessThan,
        k::IntCC::Sle => CIntCC::SignedLessThanOrEqual,
        k::IntCC::Sgt => CIntCC::SignedGreaterThan,
        k::IntCC::Sge => CIntCC::SignedGreaterThanOrEqual,
        k::IntCC::Ult => CIntCC::UnsignedLessThan,
        k::IntCC::Ule => CIntCC::UnsignedLessThanOrEqual,
        k::IntCC::Ugt => CIntCC::UnsignedGreaterThan,
        k::IntCC::Uge => CIntCC::UnsignedGreaterThanOrEqual,
    }
}

/// Keel's ordered conditions map to CLIF's ordered ones; Keel `ne` is C's
/// `!=` (true on NaN), which is CLIF `ne` (unordered or not equal).
fn float_cc(c: k::FloatCC) -> CFloatCC {
    match c {
        k::FloatCC::Eq => CFloatCC::Equal,
        k::FloatCC::Ne => CFloatCC::NotEqual,
        k::FloatCC::Lt => CFloatCC::LessThan,
        k::FloatCC::Le => CFloatCC::LessThanOrEqual,
        k::FloatCC::Gt => CFloatCC::GreaterThan,
        k::FloatCC::Ge => CFloatCC::GreaterThanOrEqual,
        k::FloatCC::Ord => CFloatCC::Ordered,
        k::FloatCC::Uno => CFloatCC::Unordered,
    }
}
