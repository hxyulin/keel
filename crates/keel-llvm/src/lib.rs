//! Diagnostic exporter, deliberately independent of LLVM libraries.
//! See the crate README for its supported subset and semantic boundaries.
use keel_ir::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub type ExportResult<T> = Result<T, String>;

/// The only validated ABI profile initially supported. No host inference.
#[derive(Clone, Copy, Debug)]
pub enum TargetProfile {
    AppleArm64,
}
impl TargetProfile {
    pub fn triple(self) -> &'static str {
        "arm64-apple-macosx11.0.0"
    }
    pub fn data_layout(self) -> &'static str {
        "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"
    }
}

fn ty(t: Type) -> ExportResult<String> {
    Ok(match t {
        Type::F32 => "float".into(),
        Type::F64 => "double".into(),
        Type::F80 => return Err("f80 is unsupported on the AppleArm64 exporter".into()),
        Type::Vector(_) => return Err("vector types are unsupported".into()),
        _ => t.to_string(),
    })
}
fn symbol(s: &str) -> String {
    // LLVM adds the Mach-O prefix itself. Quote/escape every byte, including
    // unusual assembler names, without applying a second underscore prefix.
    let escaped: String = s.bytes().map(|b| format!("\\{b:02X}")).collect();
    format!("@\"{escaped}\"")
}
fn abi(p: &AbiParam) -> ExportResult<String> {
    if p.purpose != Purpose::Normal {
        return Err("aggregate byval/sret ABI is unsupported".into());
    }
    if p.ty == Type::I128 {
        return Err("i128 ABI values are unsupported".into());
    }
    let ext = match p.ext {
        ArgExt::None => "",
        ArgExt::Sext => " signext",
        ArgExt::Uext => " zeroext",
    };
    Ok(format!("{}{ext}", ty(p.ty)?))
}
fn signature(s: &Signature) -> ExportResult<(String, Vec<String>)> {
    if s.x86_vector_abi.is_some() {
        return Err("explicit x86 vector ABI is unsupported by this exporter".into());
    }
    if s.variadic || s.returns_twice || s.returns.len() > 1 {
        return Err(
            "variadic, returns_twice and multiple-result signatures are unsupported".into(),
        );
    }
    let ret = if let Some(r) = s.returns.first() {
        // Return extension attributes precede the type in LLVM.
        let t = abi(r)?;
        match r.ext {
            ArgExt::None => t,
            ArgExt::Sext => format!("signext {}", ty(r.ty)?),
            ArgExt::Uext => format!("zeroext {}", ty(r.ty)?),
        }
    } else {
        "void".into()
    };
    Ok((ret, s.params.iter().map(abi).collect::<ExportResult<_>>()?))
}
fn attrs(a: &FunctionAttrs) -> String {
    // Ordinary Keel calls have no name-based libc contract. Declaration-level
    // nobuiltin also prevents LLVM21 InferFunctionAttrs from inferring one;
    // call-site-only protection would leave that module pass enabled.
    let mut attributes = String::from(" nobuiltin");
    attributes.push_str(
        &[
            (a.inlinehint, "inlinehint"),
            (a.alwaysinline, "alwaysinline"),
            (a.noinline, "noinline"),
            (a.noreturn, "noreturn"),
            (a.nounwind, "nounwind"),
        ]
        .into_iter()
        .filter_map(|(set, s)| set.then_some(format!(" {s}")))
        .collect::<String>(),
    );
    if let Some(align) = a.align {
        attributes.push_str(&format!(" align {align}"));
    }
    attributes
}

/// Verify and translate the complete module in memory. Errors never return
/// partial LLVM text. The caller must not publish output until this succeeds.
pub fn export(module: &Module, target: TargetProfile) -> ExportResult<String> {
    if let Some((name, contract)) = module.special_entry_contract() {
        return Err(format!(
            "@{name}: {contract} function entry/return lowering is not implemented"
        ));
    }
    if !module.assembly.is_empty() {
        return Err("module assembly is unsupported by the LLVM bridge".into());
    }

    if module.functions.iter().any(|f| f.layout_insts().any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::AggregateVaArg { .. }) || f.inst_data(i).call_args().is_some_and(|a| a.has_explicit_abi()))) {
        return Err("explicit call operand ABI and aggregate va_arg homes are unsupported by LLVM".into());
    }
    if module.decls.iter().any(|d| d.sig.x86_vector_abi.is_some())
        || module.functions.iter().any(|f| {
            f.sig.x86_vector_abi.is_some()
                || f.callees().any(|(_, d)| d.sig.x86_vector_abi.is_some())
                || f.signatures().any(|(_, s)| s.x86_vector_abi.is_some())
                || f.layout_insts().any(|(_,i)| matches!(f.inst_data(i), keel_ir::InstData::VaArg { x86_vector_abi: Some(_), .. }))
        })
    {
        return Err("explicit x86 vector ABI is unsupported by LLVM".into());
    }

    if module
        .functions
        .iter()
        .any(|f| f.attrs.retain || f.attrs.section.is_some())
        || module
            .decls
            .iter()
            .any(|f| f.attrs.retain || f.attrs.section.is_some())
        || module.data.iter().any(|d| d.retain || d.section.is_some())
    {
        return Err("export does not support custom sections or linker retention".into());
    }

    if module
        .functions
        .iter()
        .any(|f| f.attrs.constructor || f.attrs.destructor)
    {
        return Err("LLVM export does not support lifecycle registration".into());
    }
    if module.decls.iter().any(|d| d.attrs.weak)
        || module
            .functions
            .iter()
            .any(|f| f.attrs.weak || f.weak_odr || f.hidden)
        || module.data.iter().any(|d| {
            d.weak
                || d.weak_odr
                || d.hidden
                || d.init.as_ref().is_some_and(|i| {
                    i.items
                        .iter()
                        .any(|i| matches!(i, DataItem::RttiName { .. }))
                })
        })
    {
        return Err("weak, weak_odr and hidden linkage are unsupported by this exporter".into());
    }
    verify_module(module).map_err(|e| format!("invalid Keel IR: {e:?}"))?;
    if !module.files.is_empty()
        || !module.debug_types.is_empty()
        || !module.debug_globals.is_empty()
        || module.debug_language.is_some()
    {
        return Err("debug information is unsupported; generate IR without -g".into());
    }
    // Merge function-local imports as well as module declarations. Verification
    // checks module-level consistency; also reject ambiguous symbol namespaces.
    let mut decls: BTreeMap<String, FuncDecl> = BTreeMap::new();
    for d in module.decls.iter().chain(
        module
            .functions
            .iter()
            .flat_map(|f| f.callees().map(|(_, d)| d)),
    ) {
        signature(&d.sig).map_err(|e| format!("@{}: {e}", d.name))?;
        if let Some(old) = decls.insert(d.name.clone(), d.clone()) {
            if old.sig != d.sig || old.attrs != d.attrs {
                return Err(format!("@{}: conflicting declarations", d.name));
            }
        }
    }
    let mut names = BTreeSet::new();
    for d in &module.data {
        if !names.insert(d.name.clone()) || decls.contains_key(&d.name) {
            return Err(format!("@{}: duplicate symbol", d.name));
        }
    }
    for f in &module.functions {
        if !names.insert(f.name.clone()) {
            return Err(format!("@{}: duplicate symbol", f.name));
        }
        if let Some(d) = decls.get(&f.name) {
            if d.sig != f.sig {
                return Err(format!("@{}: conflicting definition signature", f.name));
            }
        }
    }
    // Standard comparison intrinsics use the same ABI as libc when not
    // expanded. Merge this import before emitting declarations, including
    // modules that also contain an ordinary call to memcmp.
    if module.functions.iter().any(|f| {
        f.layout_insts().any(|(_, i)| {
            matches!(
                f.inst_data(i),
                InstData::Intrinsic {
                    id: Intrinsic::Memcmp,
                    ..
                }
            )
        })
    }) {
        let sig = Signature::new(&[Type::Ptr, Type::Ptr, Type::I64], &[Type::I32]);
        if module.data.iter().any(|d| d.name == "memcmp")
            || module
                .functions
                .iter()
                .any(|f| f.name == "memcmp" && f.sig != sig)
            || decls.get("memcmp").is_some_and(|d| d.sig != sig)
        {
            return Err("@memcmp: incompatible builtin fallback signature".into());
        }
        decls.entry("memcmp".into()).or_insert(FuncDecl {
            name: "memcmp".into(),
            sig,
            attrs: Default::default(),
        });
    }
    let mut out = format!(
        "; Diagnostic Keel export; no production backend guarantee\ntarget datalayout = \"{}\"\ntarget triple = \"{}\"\n\n",
        target.data_layout(),
        target.triple()
    );
    for d in &module.data {
        emit_data(module, d, &mut out).map_err(|e| format!("data @{}: {e}", d.name))?;
    }
    for d in decls
        .values()
        .filter(|d| !module.functions.iter().any(|f| f.name == d.name))
    {
        if d.name.starts_with("llvm.") {
            return Err("user symbols in LLVM's intrinsic namespace are unsupported".into());
        }
        let (r, p) = signature(&d.sig)?;
        writeln!(
            out,
            "declare {r} {}({}){}",
            symbol(&d.name),
            p.join(", "),
            attrs(&d.attrs)
        )
        .unwrap();
    }
    let mut intrinsics = BTreeSet::new();
    for f in &module.functions {
        if f.name.starts_with("llvm.") {
            return Err("user symbols in LLVM's intrinsic namespace are unsupported".into());
        }
        emit_function(f, &mut out, &mut intrinsics)
            .map_err(|e| format!("function @{}: {e}", f.name))?;
    }
    // Keep compiler-emission roots through LLVM optimization without granting
    // linker retention (`llvm.used` would impose that stronger contract).
    let used: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.attrs.used)
        .map(|f| format!("ptr {}", symbol(&f.name)))
        .collect();
    if !used.is_empty() {
        writeln!(
            out,
            "@llvm.compiler.used = appending global [{} x ptr] [{}], section \"llvm.metadata\"",
            used.len(),
            used.join(", ")
        )
        .unwrap();
    }
    for decl in intrinsics {
        writeln!(out, "{decl}").unwrap();
    }
    Ok(out)
}

fn emit_data(m: &Module, d: &DataObject, out: &mut String) -> ExportResult<()> {
    if d.name.starts_with("llvm.") {
        return Err("LLVM intrinsic namespace is reserved".into());
    }
    if d.thread_local {
        return Err("thread-local storage is unsupported".into());
    }
    let Some(init) = &d.init else {
        writeln!(out, "{} = external global i8", symbol(&d.name)).unwrap();
        return Ok(());
    };
    let mut types = Vec::new();
    let mut values = Vec::new();
    for item in &init.items {
        let (t, v) = match item {
            DataItem::Bytes(b) => (
                format!("[{} x i8]", b.len()),
                format!(
                    "c\"{}\"",
                    b.iter().map(|b| format!("\\{b:02X}")).collect::<String>()
                ),
            ),
            DataItem::Zero(n) => (format!("[{n} x i8]"), "zeroinitializer".into()),
            DataItem::Int { ty: t, value } => (ty(*t)?, value.to_string()),
            DataItem::RttiName { .. } => {
                return Err("rtti_name is unsupported by this exporter".into());
            }
            DataItem::Ptr { symbol: s, addend } => {
                if !m.has_symbol(s) {
                    return Err(format!("unknown relocation symbol @{s}"));
                }
                (
                    "ptr".into(),
                    format!("getelementptr (i8, ptr {}, i64 {addend})", symbol(s)),
                )
            }
        };
        values.push(format!("{t} {v}"));
        types.push(t);
    }
    let linkage = if d.local { "internal " } else { "" };
    let storage = if init.constant { "constant" } else { "global" };
    // Packed fields preserve byte-for-byte placement without implicit padding.
    writeln!(
        out,
        "{} = {linkage}{storage} <{{ {} }}> <{{ {} }}>, align {}",
        symbol(&d.name),
        types.join(", "),
        values.join(", "),
        init.align
    )
    .unwrap();
    Ok(())
}

fn edges(i: &InstData) -> Vec<&BlockCall> {
    match i {
        InstData::Jump { dest } => vec![dest],
        InstData::Brif { dests, .. } => dests.iter().collect(),
        InstData::Switch { dests, .. } => dests.iter().collect(),
        _ => Vec::new(),
    }
}
fn edge_name(i: Inst, n: usize) -> String {
    format!("edge{}_{}", i.index(), n)
}
fn emit_function(
    f: &Function,
    out: &mut String,
    intrinsics: &mut BTreeSet<String>,
) -> ExportResult<()> {
    if f.personality.is_some()
        || f.debug_return.is_some()
        || f.debug_loc.is_some()
        || !f.debug_scopes.is_empty()
        || !f.debug_vars.is_empty()
        || f.layout_insts().any(|(_, i)| f.inst_loc(i).is_some())
    {
        return Err("exception handling/debug information is unsupported".into());
    }
    let (r, p) = signature(&f.sig)?;
    let entry = f.entry_block().ok_or("missing entry block")?;
    let params = p
        .iter()
        .zip(f.block_params(entry))
        .map(|(t, v)| format!("{t} %{v}"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        out,
        "\ndefine {}{r} {}({params}){} {{",
        if f.local { "internal " } else { "" },
        symbol(&f.name),
        attrs(&f.attrs)
    )
    .unwrap();
    let mut incoming: BTreeMap<Block, Vec<(String, &BlockCall)>> = BTreeMap::new();
    for (_, i) in f.layout_insts() {
        for (n, e) in edges(f.inst_data(i)).into_iter().enumerate() {
            if e.block == entry {
                return Err("branches to entry block are unsupported".into());
            }
            incoming
                .entry(e.block)
                .or_default()
                .push((edge_name(i, n), e));
        }
    }
    let mut emitter = Emitter { f, out, intrinsics };
    for b in f.blocks() {
        writeln!(emitter.out, "{b}:").unwrap();
        if b == entry {
            for (s, d) in f.stack_slots() {
                writeln!(
                    emitter.out,
                    "  %{s} = alloca i8, i64 {}, align {}",
                    d.size, d.align
                )
                .unwrap();
            }
        } else {
            for (n, v) in f.block_params(b).iter().enumerate() {
                let ins = incoming
                    .get(&b)
                    .ok_or_else(|| format!("{b}: parameter without incoming edges"))?;
                let pairs = ins
                    .iter()
                    .map(|(label, e)| format!("[ {}, %{label} ]", emitter.v(e.args[n])))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    emitter.out,
                    "  %{v} = phi {} {pairs}",
                    ty(f.value_type(*v))?
                )
                .unwrap();
            }
        }
        for &i in f.block_insts(b) {
            emitter
                .emit(i)
                .map_err(|e| format!("{b}, {i} ({}): {e}", f.display_inst(i)))?;
        }
    }
    // Every edge gets its own predecessor, even parallel switch/brif edges.
    for (_, i) in f.layout_insts() {
        for (n, e) in edges(f.inst_data(i)).into_iter().enumerate() {
            writeln!(emitter.out, "{}:\n  br label %{}", edge_name(i, n), e.block).unwrap();
        }
    }
    emitter.out.push_str("}\n");
    Ok(())
}
struct Emitter<'a> {
    f: &'a Function,
    out: &'a mut String,
    intrinsics: &'a mut BTreeSet<String>,
}
impl Emitter<'_> {
    fn v(&self, v: Value) -> String {
        format!("%{}", self.f.resolve(v))
    }
    fn typed(&self, v: Value) -> ExportResult<String> {
        Ok(format!("{} {}", ty(self.f.value_type(v))?, self.v(v)))
    }
    fn line(&mut self, s: String) {
        writeln!(self.out, "  {s}").unwrap();
    }
    fn intrinsic(&mut self, name: &str, ret: &str, params: &str) {
        self.intrinsics
            .insert(format!("declare {ret} @{name}({params})"));
    }
    fn emit(&mut self, i: Inst) -> ExportResult<()> {
        let f = self.f;
        let results = f.inst_results(i);
        let dst = results.first().map(|v| format!("%{v}")).unwrap_or_default();
        let tmp = format!("%t{}", i.index());
        match f.inst_data(i) {
            InstData::Iconst { ty: t, imm } => {
                self.line(format!("{dst} = add {} 0, {imm}", ty(*t)?))
            }
            InstData::Fconst { ty: t, bits } => {
                let (it, b) = if *t == Type::F32 {
                    ("i32", u64::from(*bits as u32))
                } else {
                    ("i64", *bits)
                };
                self.line(format!("{dst} = bitcast {it} {b} to {}", ty(*t)?));
            }
            InstData::Binary { op, args: [a, b] } => {
                let input_type = f.value_type(*a);
                let amount_type = f.value_type(*b);
                let t = ty(input_type)?;
                let (a, b) = (self.v(*a), self.v(*b));
                let opcode = match op {
                    BinaryOp::Iadd => "add",
                    BinaryOp::Isub => "sub",
                    BinaryOp::Imul => "mul",
                    BinaryOp::And => "and",
                    BinaryOp::Or => "or",
                    BinaryOp::Xor => "xor",
                    BinaryOp::Fadd => "fadd",
                    BinaryOp::Fsub => "fsub",
                    BinaryOp::Fmul => "fmul",
                    BinaryOp::Fdiv => "fdiv",
                    BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                        let width = input_type.bytes().ok_or("shift on pointer")? * 8;
                        let amount = if input_type != amount_type {
                            let opcode =
                                if input_type.bytes().unwrap() < amount_type.bytes().unwrap() {
                                    "trunc"
                                } else {
                                    "zext"
                                };
                            self.line(format!(
                                "{tmp}.amount = {opcode} {} {b} to {t}",
                                ty(amount_type)?
                            ));
                            format!("{tmp}.amount")
                        } else {
                            b
                        };
                        self.line(format!("{tmp} = and {t} {amount}, {}", width - 1));
                        let op = match op {
                            BinaryOp::Shl => "shl",
                            BinaryOp::Ushr => "lshr",
                            _ => "ashr",
                        };
                        self.line(format!("{dst} = {op} {t} {a}, {tmp}"));
                        return Ok(());
                    }
                    BinaryOp::Ptradd => {
                        self.line(format!("{dst} = getelementptr i8, ptr {a}, i64 {b}"));
                        return Ok(());
                    }
                    BinaryOp::Fmin | BinaryOp::Fmax | BinaryOp::Fcopysign => {
                        let stem = match op {
                            BinaryOp::Fmin => "minimum",
                            BinaryOp::Fmax => "maximum",
                            _ => "copysign",
                        };
                        let name = format!("llvm.{stem}.{}", input_type);
                        self.intrinsic(&name, &t, &format!("{t}, {t}"));
                        self.line(format!("{dst} = call {t} @{name}({t} {a}, {t} {b})"));
                        return Ok(());
                    }
                    // LLVM div/rem introduce poison/UB where Keel requires traps.
                    // Reject until explicit trap CFG and srem MIN/-1 are validated.
                    _ => return Err(format!("unsupported binary operation {op:?}")),
                };
                self.line(format!("{dst} = {opcode} {t} {a}, {b}"));
            }
            InstData::Unary { op, arg } => {
                let t = ty(f.value_type(*arg))?;
                let v = self.v(*arg);
                match op {
                    UnaryOp::Ineg => self.line(format!("{dst} = sub {t} 0, {v}")),
                    UnaryOp::Bnot => self.line(format!("{dst} = xor {t} {v}, -1")),
                    UnaryOp::Fneg => self.line(format!("{dst} = fneg {t} {v}")),
                    _ => {
                        let stem = match op {
                            UnaryOp::Clz => "ctlz",
                            UnaryOp::Ctz => "cttz",
                            UnaryOp::Popcnt => "ctpop",
                            UnaryOp::Bswap => "bswap",
                            UnaryOp::Fabs => "fabs",
                            UnaryOp::Fsqrt => "sqrt",
                            UnaryOp::Fceil => "ceil",
                            UnaryOp::Ffloor => "floor",
                            UnaryOp::Ftrunc => "trunc",
                            UnaryOp::Fnearest => "roundeven",
                            _ => unreachable!(),
                        };
                        let name = format!("llvm.{stem}.{}", f.value_type(*arg));
                        let zero = matches!(op, UnaryOp::Clz | UnaryOp::Ctz);
                        self.intrinsic(
                            &name,
                            &t,
                            &format!("{t}{}", if zero { ", i1" } else { "" }),
                        );
                        self.line(format!(
                            "{dst} = call {t} @{name}({t} {v}{})",
                            if zero { ", i1 false" } else { "" }
                        ));
                    }
                }
            }
            InstData::Convert { op, ty: t, arg } => {
                let src = ty(f.value_type(*arg))?;
                let target = ty(*t)?;
                let v = self.v(*arg);
                let opcode = match op {
                    // This diagnostic exporter intentionally rejects every
                    // vector type; keep the new vector conversions in that
                    // same explicit unsupported subset.
                    ConvertOp::SwidenLow
                    | ConvertOp::UwidenLow
                    | ConvertOp::SwidenHigh
                    | ConvertOp::UwidenHigh
                    | ConvertOp::NarrowLow
                    | ConvertOp::SnarrowSatLow
                    | ConvertOp::UnarrowSatLow
                    | ConvertOp::SunarrowSatLow
                    | ConvertOp::SpairwiseAddLong
                    | ConvertOp::UpairwiseAddLong => {
                        return Err("vector types are unsupported".into());
                    }
                    ConvertOp::Sext => "sext",
                    ConvertOp::Uext => "zext",
                    ConvertOp::Trunc => "trunc",
                    ConvertOp::PtrToInt => "ptrtoint",
                    ConvertOp::IntToPtr => "inttoptr",
                    ConvertOp::Fpext => "fpext",
                    ConvertOp::Fptrunc => "fptrunc",
                    ConvertOp::Sitofp => "sitofp",
                    ConvertOp::Uitofp => "uitofp",
                    ConvertOp::Bitcast => "bitcast",
                    ConvertOp::Fptosi | ConvertOp::Fptoui => {
                        let stem = if *op == ConvertOp::Fptosi {
                            "fptosi"
                        } else {
                            "fptoui"
                        };
                        let name = format!("llvm.{stem}.sat.{t}.{}", f.value_type(*arg));
                        self.intrinsic(&name, &target, &src);
                        self.line(format!("{dst} = call {target} @{name}({src} {v})"));
                        return Ok(());
                    }
                };
                self.line(format!("{dst} = {opcode} {src} {v} to {target}"));
            }
            InstData::Icmp { cond, args: [a, b] } => {
                self.line(format!(
                    "{tmp} = icmp {cond} {}, {}",
                    self.typed(*a)?,
                    self.v(*b)
                ));
                self.line(format!("{dst} = zext i1 {tmp} to i8"));
            }
            InstData::Fcmp { cond, args: [a, b] } => {
                let cc = match cond {
                    FloatCC::Eq => "oeq",
                    FloatCC::Ne => "une",
                    FloatCC::Lt => "olt",
                    FloatCC::Le => "ole",
                    FloatCC::Gt => "ogt",
                    FloatCC::Ge => "oge",
                    FloatCC::Ord => "ord",
                    FloatCC::Uno => "uno",
                };
                self.line(format!(
                    "{tmp} = fcmp {cc} {}, {}",
                    self.typed(*a)?,
                    self.v(*b)
                ));
                self.line(format!("{dst} = zext i1 {tmp} to i8"));
            }
            InstData::Select { args: [c, a, b] } => {
                self.line(format!("{tmp} = icmp ne {}, 0", self.typed(*c)?));
                self.line(format!(
                    "{dst} = select i1 {tmp}, {}, {}",
                    self.typed(*a)?,
                    self.typed(*b)?
                ));
            }
            InstData::Load { flags, ty: t, addr } => self.line(format!(
                "{dst} = load {}{}, ptr {}, align {}",
                if flags.volatile { "volatile " } else { "" },
                ty(*t)?,
                self.v(*addr),
                flags.align
            )),
            InstData::Store { flags, value, addr } => self.line(format!(
                "store {}{}, ptr {}, align {}",
                if flags.volatile { "volatile " } else { "" },
                self.typed(*value)?,
                self.v(*addr),
                flags.align
            )),
            InstData::StackAddr { slot } => {
                self.line(format!("{dst} = getelementptr i8, ptr %{slot}, i64 0"))
            }
            InstData::SymbolAddr { symbol: s } => self.line(format!(
                "{dst} = getelementptr i8, ptr {}, i64 0",
                symbol(f.symbol_name(*s))
            )),
            InstData::Call { func, args } => {
                let d = f.callee(*func);
                let (ret, params) = signature(&d.sig)?;
                let args = args
                    .iter()
                    .zip(params)
                    .map(|(v, t)| format!("{t} {}", self.v(*v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.line(format!(
                    "{}call {ret} {}({args}) nobuiltin",
                    if results.is_empty() {
                        String::new()
                    } else {
                        format!("{dst} = ")
                    },
                    symbol(&d.name)
                ));
            }
            InstData::Jump { .. } => self.line(format!("br label %{}", edge_name(i, 0))),
            InstData::Brif { cond, .. } => {
                self.line(format!("{tmp} = icmp ne {}, 0", self.typed(*cond)?));
                self.line(format!(
                    "br i1 {tmp}, label %{}, label %{}",
                    edge_name(i, 0),
                    edge_name(i, 1)
                ));
            }
            InstData::Switch { arg, cases, .. } => {
                let t = ty(f.value_type(*arg))?;
                self.line(format!(
                    "switch {}, label %{} [",
                    self.typed(*arg)?,
                    edge_name(i, 0)
                ));
                for (n, c) in cases.iter().enumerate() {
                    self.line(format!("  {t} {c}, label %{}", edge_name(i, n + 1)));
                }
                self.line("]".into());
            }
            InstData::Return { args } => self.line(if let Some(v) = args.first() {
                format!("ret {}", self.typed(*v)?)
            } else {
                "ret void".into()
            }),
            InstData::Trap => {
                self.intrinsic("llvm.trap", "void", "");
                self.line("call void @llvm.trap()".into());
                self.line("unreachable".into());
            }
            InstData::Intrinsic { id, args } => self.emit_intrinsic(*id, args, &dst)?,
            InstData::Overflow { op, args: [a, b] } => {
                let t = ty(f.value_type(*a))?;
                let stem = match op {
                    OverflowOp::Uadd => "uadd",
                    OverflowOp::Sadd => "sadd",
                    OverflowOp::Usub => "usub",
                    OverflowOp::Ssub => "ssub",
                    OverflowOp::Umul => "umul",
                    OverflowOp::Smul => "smul",
                };
                let name = format!("llvm.{stem}.with.overflow.{}", f.value_type(*a));
                let ret = format!("{{ {t}, i1 }}");
                self.intrinsic(&name, &ret, &format!("{t}, {t}"));
                self.line(format!(
                    "{tmp} = call {ret} @{name}({}, {})",
                    self.typed(*a)?,
                    self.typed(*b)?
                ));
                self.line(format!("{dst} = extractvalue {ret} {tmp}, 0"));
                self.line(format!("{tmp}.flag = extractvalue {ret} {tmp}, 1"));
                self.line(format!("%{} = zext i1 {tmp}.flag to i8", results[1]));
            }
            InstData::Fma { args: [a, b, c] } => {
                let t = ty(f.value_type(*a))?;
                let name = format!("llvm.fma.{}", f.value_type(*a));
                self.intrinsic(&name, &t, &format!("{t}, {t}, {t}"));
                self.line(format!(
                    "{dst} = call {t} @{name}({}, {}, {})",
                    self.typed(*a)?,
                    self.typed(*b)?,
                    self.typed(*c)?
                ));
            }
            _ => {
                return Err(
                    "unsupported instruction (atomic, asm, vector, indirect/tail call or EH)"
                        .into(),
                );
            }
        }
        Ok(())
    }
    fn emit_intrinsic(&mut self, id: Intrinsic, args: &[Value], dst: &str) -> ExportResult<()> {
        let a = args
            .iter()
            .map(|v| self.typed(*v))
            .collect::<ExportResult<Vec<_>>>()?;
        match id {
            Intrinsic::Memcmp => {
                self.line(format!(
                    "{dst} = call i32 {}({})",
                    symbol("memcmp"),
                    a.join(", ")
                ));
            }
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset => {
                let (name, params) = match id {
                    Intrinsic::Memcpy => ("llvm.memcpy.p0.p0.i64", "ptr, ptr, i64, i1"),
                    Intrinsic::Memmove => ("llvm.memmove.p0.p0.i64", "ptr, ptr, i64, i1"),
                    _ => ("llvm.memset.p0.i64", "ptr, i8, i64, i1"),
                };
                self.intrinsic(name, "void", params);
                self.line(format!("call void @{name}({}, i1 false)", a.join(", ")));
            }
            Intrinsic::Debugtrap => {
                self.intrinsic("llvm.debugtrap", "void", "");
                self.line("call void @llvm.debugtrap()".into());
            }
            Intrinsic::StackAlloc => self.line(format!("{dst} = alloca i8, {}, align 16", a[0])),
            Intrinsic::StackSave => {
                self.intrinsic("llvm.stacksave.p0", "ptr", "");
                self.line(format!("{dst} = call ptr @llvm.stacksave.p0()"));
            }
            Intrinsic::StackRestore => {
                self.intrinsic("llvm.stackrestore.p0", "void", "ptr");
                self.line(format!("call void @llvm.stackrestore.p0({})", a[0]));
            }
            _ => return Err(format!("unsupported intrinsic {id:?}")),
        }
        Ok(())
    }
}
