//! LLVM 23 import through a dynamically loaded C API, without LLVM link dependencies.
mod contracts;
mod ffi;
mod globals;
mod instructions;
mod linkage;
use ffi::{Api, Ref, Session};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::LazyLock,
};
type Result<T> = std::result::Result<T, String>;

/// Import policies that require an explicit caller assertion.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// No outside translation unit references ODR candidates copied as local functions.
    pub localize_odr_functions: bool,
}
/// Reusable LLVM library handle. Each translation owns an independent LLVM context.
pub struct Importer {
    api: Api,
}
impl std::fmt::Debug for Importer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Importer").finish_non_exhaustive()
    }
}
/// LLVM loading, input validation or translation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError(String);
impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ImportError {}
impl Importer {
    /// Load a trusted LLVM 23 shared library and validate required symbols.
    ///
    /// The path must refer to a trusted native library: loading runs its initializers.
    /// # Safety
    /// The library must implement LLVM 23's C ABI and have safe initializers and
    /// finalizers. Symbol presence and a version number cannot establish ABI safety.
    /// # Errors
    /// Returns an error for missing symbols, incompatible versions or attributes.
    pub unsafe fn load(path: &Path) -> std::result::Result<Self, ImportError> {
        let api = Api::load(path).map_err(ImportError)?;
        for name in [
            "sret",
            "byval",
            "byref",
            "inalloca",
            "preallocated",
            "inreg",
            "nest",
            "swiftself",
            "swifterror",
            "swiftasync",
            "alignstack",
            "signext",
            "zeroext",
            "naked",
            "returns_twice",
            "null_pointer_is_valid",
            "strictfp",
            "nofpclass",
        ] {
            if api.kind(name) == 0 {
                return Err(ImportError(format!(
                    "loaded LLVM does not recognize required attribute {name}"
                )));
            }
        }
        Ok(Self { api })
    }
    /// Parse and verify LLVM IR, translate it, and verify the complete Keel result.
    ///
    /// Supported scalar `undef` and `poison` operands are refined to zero.
    /// Scalar `freeze` preserves the concretized operand. This discards source
    /// nondeterminism; it does not represent LLVM's full deferred UB semantics.
    /// # Errors
    /// Returns a diagnostic without partial output for invalid or unsupported input.
    pub fn translate(
        &self,
        source: &[u8],
        options: Options,
    ) -> std::result::Result<String, ImportError> {
        let session = Session::parse(&self.api, source).map_err(ImportError)?;
        let mut t = Translation::new(&session);
        let text = t.translate(options).map_err(ImportError)?;
        let m = keel_ir::parse(&text).map_err(|e| ImportError(format!("Keel parse: {e}")))?;
        keel_ir::verify_module(&m).map_err(|e| ImportError(format!("Keel verification: {e:?}")))?;
        Ok(text)
    }
}
fn sequence(first: impl FnOnce() -> Ref, next: impl Fn(Ref) -> Ref) -> Vec<Ref> {
    let mut out = Vec::new();
    let mut v = first();
    while v != 0 {
        out.push(v);
        v = next(v);
    }
    out
}
fn local(link: u32) -> bool {
    matches!(link, 8 | 9)
}
fn odr(link: u32) -> bool {
    matches!(link, 3 | 6)
}
fn word(text: &str, needle: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .any(|w| w == needle)
}
static VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"v[0-9]+\b").expect("constant value regex"));

struct Translation<'a> {
    l: &'a Api,
    module: Ref,
    td: Ref,
    fast: HashSet<Ref>,
    personalities: HashSet<Ref>,
    copies: HashSet<Ref>,
    signatures: HashMap<Ref, (String, Vec<String>)>,
    globals: globals::Globals,
    needs_resume: bool,
    lines: Vec<String>,
    next_value: usize,
    values: HashMap<Ref, String>,
    aggregate_params: HashMap<Ref, [String; 2]>,
    aggregate_slots: HashMap<(Ref, u32), String>,
    blocks: HashMap<Ref, String>,
    phis: HashMap<Ref, Vec<Ref>>,
    pairs: HashMap<Ref, [String; 2]>,
    invoke_edges: Vec<(String, Ref, Ref, Option<Ref>)>,
    pad_insts: HashMap<Ref, Ref>,
    split_pads: HashSet<Ref>,
    slots: HashMap<Ref, String>,
    block: Ref,
}
impl<'a> Translation<'a> {
    fn new(s: &'a Session<'_>) -> Self {
        Self {
            l: s.api,
            module: s.module,
            td: s.td,
            fast: HashSet::new(),
            personalities: HashSet::new(),
            copies: HashSet::new(),
            signatures: HashMap::new(),
            globals: Default::default(),
            needs_resume: false,
            lines: Vec::new(),
            next_value: 0,
            values: HashMap::new(),
            aggregate_params: HashMap::new(),
            aggregate_slots: HashMap::new(),
            blocks: HashMap::new(),
            phis: HashMap::new(),
            pairs: HashMap::new(),
            invoke_edges: Vec::new(),
            pad_insts: HashMap::new(),
            split_pads: HashSet::new(),
            slots: HashMap::new(),
            block: 0,
        }
    }
    fn symbol(&self, v: Ref) -> Result<String> {
        let n = self.l.name(v);
        if n.is_empty()
            || !n
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
        {
            return Err(format!("unsupported symbol spelling {n:?}"));
        }
        Ok(format!("@{n}"))
    }
    fn ty(&self, t: Ref) -> Result<String> {
        Ok(match self.l.get_type_kind(t) {
        0=>"void".into(),2=>"f32".into(),3=>"f64".into(),
        8=>match self.l.get_int_type_width(t) {1=>"i8".into(),w @ (8|16|32|64)=>format!("i{w}"),_=>return Err("unsupported integer width".into())},
        12 if self.l.get_pointer_address_space(t)==0=>"ptr".into(),_=>return Err("supported types are scalar i1/i8/i16/i32/i64/f32/f64 and address-space-zero pointers".into())})
    }
    fn width(&self, v: Ref) -> u32 {
        let t = self.l.type_of(v);
        if self.l.get_type_kind(t) == 8 {
            self.l.get_int_type_width(t)
        } else {
            0
        }
    }
    fn integer_pair(&self, t: Ref) -> bool {
        let l = self.l;
        l.get_type_kind(t) == 11
            && l.get_array_length2(t) == 2
            && l.get_type_kind(l.get_element_type(t)) == 8
            && l.get_int_type_width(l.get_element_type(t)) == 64
            && l.abi_size_of_type(self.td, t) == 16
            && l.abi_alignment_of_type(self.td, t) == 8
    }
    fn landing_pair(&self, t: Ref) -> Result<bool> {
        let l = self.l;
        Ok(l.get_type_kind(t) == 10
            && l.is_opaque_struct(t) == 0
            && l.is_packed_struct(t) == 0
            && l.count_struct_element_types(t) == 2
            && self.ty(l.struct_get_type_at_index(t, 0))? == "ptr"
            && self.ty(l.struct_get_type_at_index(t, 1))? == "i32")
    }
    fn emit(&mut self, expr: impl AsRef<str>) -> String {
        let v = format!("v{}", self.next_value);
        self.next_value += 1;
        self.lines.push(format!("    {v} = {}", expr.as_ref()));
        v
    }
    fn fresh_pair(&mut self) -> [String; 2] {
        let p = [
            format!("v{}", self.next_value),
            format!("v{}", self.next_value + 1),
        ];
        self.next_value += 2;
        p
    }
    fn check_function_attrs(&self, v: Ref, call: bool) -> Result<()> {
        let l = self.l;
        for n in [
            "naked",
            "returns_twice",
            "alignstack",
            "null_pointer_is_valid",
            "strictfp",
        ] {
            if l.attribute(v, u32::MAX, n, call) != 0 {
                return Err(format!("unsupported function/call attribute {n}"));
            }
        }
        let allowed = [
            "nounwind",
            "willreturn",
            "nofree",
            "nosync",
            "nocallback",
            "noinline",
            "alwaysinline",
            "inlinehint",
            "cold",
            "hot",
            "memory",
            "allocsize",
            "allockind",
            "speculatable",
            "mustprogress",
            "norecurse",
            "nobuiltin",
            "builtin",
            "noreturn",
        ]
        .map(|n| l.kind(n));
        for a in l.attributes(v, u32::MAX, call) {
            if call
                && (l.is_string_attribute(a) != 0
                    || !allowed.contains(&l.get_enum_attribute_kind(a)))
            {
                return Err("unsupported call function ABI contract".into());
            }
            if l.is_string_attribute(a) != 0 {
                let n = l.string_attribute(a);
                if [
                    "denormal-fp-math",
                    "rounding-mode",
                    "aarch64_",
                    "instrument-function-",
                    "patchable-",
                ]
                .iter()
                .any(|s| n.starts_with(s))
                    || [
                        "unsafe-fp-math",
                        "no-infs-fp-math",
                        "no-nans-fp-math",
                        "no-signed-zeros-fp-math",
                        "approx-func-fp-math",
                    ]
                    .contains(&n.as_str())
                {
                    return Err(format!("unsupported function/call string attribute {n}"));
                }
            }
        }
        Ok(())
    }
    fn function_attrs(&self, function: Ref) -> keel_ir::FunctionAttrs {
        let has = |name| self.l.attribute(function, u32::MAX, name, false) != 0;
        let cold = has("cold");
        let hot = has("hot");
        keel_ir::FunctionAttrs {
            inlinehint: has("inlinehint"),
            alwaysinline: has("alwaysinline"),
            noinline: has("noinline"),
            noreturn: has("noreturn"),
            nounwind: has("nounwind"),
            // LLVM permits both hints; Keel requires them to be exclusive.
            cold: cold && !hot,
            hot: hot && !cold,
            optnone: has("optnone"),
            contract: self.contract(function, self.l.global_get_value_type(function), false),
            ..Default::default()
        }
    }
    fn memory_name(&self, v: Ref) -> Option<&'static str> {
        match self.l.name(v).as_str() {
            "llvm.memcpy.p0.p0.i64" => Some("memcpy"),
            "llvm.memmove.p0.p0.i64" => Some("memmove"),
            "llvm.memset.p0.i64" => Some("memset"),
            _ => None,
        }
    }
    fn abi_signature(
        &self,
        v: Ref,
        fty: Ref,
        call: bool,
        definition: bool,
        memory: bool,
        fast: bool,
    ) -> Result<(String, Vec<String>)> {
        let l = self.l;
        if l.is_function_var_arg(fty) != 0 {
            return Err("variadic signature".into());
        }
        let mut types = vec![l.get_return_type(fty)];
        types.extend(l.params(fty));
        if memory {
            let callee = if call { l.get_called_value(v) } else { v };
            let expected = vec![
                "void",
                "ptr",
                if self.memory_name(callee) == Some("memset") {
                    "i8"
                } else {
                    "ptr"
                },
                "i64",
                "i8",
            ];
            let texts = types
                .iter()
                .map(|&t| self.ty(t))
                .collect::<Result<Vec<_>>>()?;
            if definition
                || texts != expected
                || l.get_type_kind(types[4]) != 8
                || l.get_int_type_width(types[4]) != 1
            {
                return Err("unsupported memory intrinsic signature".into());
            }
        }
        let allowed = [
            "signext",
            "zeroext",
            "sret",
            "noundef",
            "nonnull",
            "noalias",
            "nocapture",
            "captures",
            "nofree",
            "readonly",
            "readnone",
            "writeonly",
            "dereferenceable",
            "dereferenceable_or_null",
            "align",
            "range",
            "dead_on_unwind",
            "writable",
            "initializes",
            "returned",
        ]
        .map(|n| l.kind(n));
        let mut result = Vec::new();
        for (index, &t) in types.iter().enumerate() {
            let index = index as u32;
            if l.attribute(v, index, "nofpclass", call) != 0 {
                return Err("unsupported nofpclass promise".into());
            }
            for a in l.attributes(v, index, call) {
                if memory
                    && index == 4
                    && l.is_string_attribute(a) == 0
                    && l.get_enum_attribute_kind(a) == l.kind("immarg")
                {
                    continue;
                }
                if l.is_string_attribute(a) != 0 || !allowed.contains(&l.get_enum_attribute_kind(a))
                {
                    return Err("unsupported parameter/return ABI attribute".into());
                }
                if l.get_enum_attribute_kind(a) == l.kind("returned")
                    && (index == 0 || t != types[0])
                {
                    return Err("returned requires a parameter with the exact result type".into());
                }
            }
            let mut text = if matches!(l.get_type_kind(t), 10 | 11) {
                if index == 0 || !self.integer_pair(t) {
                    return Err("aggregate ABI supports only exact [2 x i64] parameters; aggregate returns unsupported".into());
                }
                "ptr byval {i64, i64}".into()
            } else {
                self.ty(t)?
            };
            let sret = l.attribute(v, index, "sret", call);
            if sret != 0 {
                if index != 1 || text != "ptr" || l.get_type_kind(types[0]) != 0 {
                    return Err("sret requires first pointer parameter and void result".into());
                }
                let layout = l.get_type_attribute_value(sret);
                if l.get_type_kind(layout) != 10
                    || l.is_opaque_struct(layout) != 0
                    || l.is_packed_struct(layout) != 0
                    || l.count_struct_element_types(layout) != 3
                    || (0..3).any(|n| {
                        let t = l.struct_get_type_at_index(layout, n);
                        l.get_type_kind(t) != 8
                            || l.get_int_type_width(t) != 64
                            || l.offset_of_element(self.td, layout, n) != u64::from(n) * 8
                    })
                    || l.abi_size_of_type(self.td, layout) != 24
                    || l.abi_alignment_of_type(self.td, layout) != 8
                {
                    return Err("sret layout outside proven unpacked three-i64 contract".into());
                }
                text.push_str(" sret {i64, i64, i64}");
            }
            let mut ext = Vec::new();
            for n in ["signext", "zeroext"] {
                if l.attribute(v, index, n, call) != 0 {
                    ext.push(n);
                }
            }
            if fast && l.get_type_kind(t) == 8 && l.get_int_type_width(t) == 1 && ext.is_empty() {
                ext.push("zeroext");
            }
            if !ext.is_empty() && (sret != 0 || l.get_type_kind(t) != 8) {
                return Err("extension requires integer ABI type".into());
            }
            if ext.len() > 1 {
                return Err("conflicting ABI extensions".into());
            }
            if l.get_type_kind(t) == 8
                && l.get_int_type_width(t) == 1
                && ext != ["zeroext"]
                && !(memory && index == 4)
            {
                return Err("i1 ABI parameters/returns require explicit zeroext".into());
            }
            if index == 0
                && matches!(text.as_str(), "i8" | "i16")
                && (definition || call)
                && ext.is_empty()
                && !fast
            {
                return Err("i8/i16 return definitions/calls require signext or zeroext".into());
            }
            for e in ext {
                text.push_str(if e == "signext" { " sext" } else { " uext" });
            }
            result.push(text);
        }
        let ret = result.remove(0);
        Ok((ret, result))
    }
    fn signature(&self, v: Ref) -> Result<(String, Vec<String>)> {
        let l = self.l;
        let fty = l.global_get_value_type(v);
        let defined = l.get_first_basic_block(v) != 0;
        if l.get_function_call_conv(v) != 0 && !self.fast.contains(&v) {
            return Err("non-C calling convention".into());
        }
        if self.personalities.contains(&v) {
            if l.name(v) != "__gxx_personality_v0"
                || defined
                || l.is_function_var_arg(fty) == 0
                || l.count_param_types(fty) != 0
                || self.ty(l.get_return_type(fty))? != "i32"
                || l.get_linkage(v) != 0
                || l.get_visibility(v) != 0
            {
                return Err("unsupported personality declaration".into());
            }
        } else if l.is_function_var_arg(fty) != 0 {
            return Err("variadic signature".into());
        }
        let link = l.get_linkage(v);
        if (link != 0 && !local(link) && !(defined && odr(link)) && !self.copies.contains(&v))
            || l.get_visibility(v) != 0
        {
            return Err("unsupported linkage/visibility".into());
        }
        if l.get_alignment(v) > 4 {
            return Err("explicit function alignment".into());
        }
        if l.get_comdat(v) != 0 {
            return Err("COMDAT membership".into());
        }
        if !l.text_property(v, "gc").is_empty()
            || !l.text_property(v, "section").is_empty()
            || l.has_prefix_data(v) != 0
            || l.has_prologue_data(v) != 0
            || l.get_dll_storage_class(v) != 0
        {
            return Err("GC, section, prefix/prologue data or DLL storage".into());
        }
        self.check_function_attrs(v, false)?;
        if self.personalities.contains(&v) {
            Ok(("void".into(), vec![]))
        } else {
            self.abi_signature(
                v,
                fty,
                false,
                defined,
                self.memory_name(v).is_some(),
                self.fast.contains(&v),
            )
        }
    }
    fn layout_type(&self, t: Ref) -> Result<()> {
        let l = self.l;
        match l.get_type_kind(t) {
            2 | 3 | 8 | 12 => {
                self.ty(t)?;
            }
            11 => self.layout_type(l.get_element_type(t))?,
            10 if l.is_opaque_struct(t) == 0 => {
                for n in 0..l.count_struct_element_types(t) {
                    self.layout_type(l.struct_get_type_at_index(t, n))?;
                }
            }
            _ => return Err("unsupported/unsized memory layout type".into()),
        }
        Ok(())
    }
    fn fp_bits(&self, v: Ref, width: u32) -> u64 {
        let l = self.l;
        let t = l.type_of(v);
        l.const_int_get_z_ext_value(
            l.const_bit_cast(v, l.int_type_in_context(l.get_type_context(t), width)),
        )
    }
    fn value(&mut self, v: Ref) -> Result<String> {
        if let Some(text) = self.values.get(&v) {
            return Ok(text.clone());
        }
        let l = self.l;
        if l.is_undef(v) || l.is_poison(v) {
            // Choosing a concrete value refines deferred UB, rather than making
            // its producer trap. Do not cache emitted constants: LLVM reuses
            // constant handles across blocks that need not dominate each other.
            let ty = self.ty(l.type_of(v))?;
            return Ok(match ty.as_str() {
                "f32" | "f64" => self.emit(format!("fconst.{ty} 0x0")),
                "ptr" => {
                    let zero = self.emit("iconst.i64 0");
                    self.emit(format!("inttoptr.ptr {zero}"))
                }
                _ => self.emit(format!("iconst.{ty} 0")),
            });
        }
        if l.is_a_constant_fp(v) != 0 {
            let t = self.ty(l.type_of(v))?;
            let width = if t == "f32" { 32 } else { 64 };
            let bits = self.fp_bits(v, width);
            return Ok(self.emit(format!(
                "fconst.{t} 0x{bits:0digits$x}",
                digits = width as usize / 4
            )));
        }
        if l.is_a_constant_int(v) != 0 {
            let t = self.ty(l.type_of(v))?;
            let n = if self.width(v) == 1 {
                l.const_int_get_z_ext_value(v).to_string()
            } else {
                l.const_int_get_s_ext_value(v).to_string()
            };
            return Ok(self.emit(format!("iconst.{t} {n}")));
        }
        if self.personalities.contains(&v) {
            return Err("personality may only be a symbolic EH reference".into());
        }
        if l.is_a_function(v) != 0 {
            return Ok(self.emit(format!("symbol_addr {}", self.symbol(v)?)));
        }
        if l.is_a_constant_pointer_null(v) != 0 {
            let zero = self.emit("iconst.i64 0");
            return Ok(self.emit(format!("inttoptr.ptr {zero}")));
        }
        if let Some((symbol, offset)) = self.address(v)? {
            let mut addr = self.emit(format!("symbol_addr {}", self.symbol(symbol)?));
            if offset != 0 {
                let offset = self.emit(format!("iconst.i64 {offset}"));
                addr = self.emit(format!("ptradd {addr}, {offset}"));
            }
            return Ok(addr);
        }
        Err(format!(
            "unsupported operand or constant expression: {}",
            l.printed(v).chars().take(160).collect::<String>()
        ))
    }
    fn aggregate_component(&mut self, mut v: Ref, index: u32) -> Result<String> {
        let l = self.l;
        if !self.integer_pair(l.type_of(v)) || index > 1 {
            return Err("aggregate component outside exact two-i64 contract".into());
        }
        while l.is_a_insert_value_inst(v) != 0 {
            let indices = l.indices(v);
            if indices.len() != 1 || indices[0] > 1 {
                return Err("insertvalue outside exact two-i64 contract".into());
            }
            if indices[0] == index {
                return self.value(l.get_operand(v, 1));
            }
            v = l.get_operand(v, 0);
        }
        self.aggregate_params.get(&v).map(|p|p[index as usize].clone()).ok_or("unsupported aggregate SSA: requires parameter or insertvalue with defined observed lanes".into())
    }
    fn edge(&mut self, target: Ref) -> Result<String> {
        let mut args = Vec::new();
        let l = self.l;
        for phi in self.phis[&target].clone() {
            let incoming: Vec<_> = (0..l.count_incoming(phi))
                .filter(|&n| l.get_incoming_block(phi, n) == self.block)
                .map(|n| l.get_incoming_value(phi, n))
                .collect();
            if incoming.is_empty() || incoming.iter().any(|&v| v != incoming[0]) {
                return Err("inconsistent PHI incoming values for predecessor".into());
            }
            if self.pairs.contains_key(&phi) {
                args.extend(
                    self.pairs
                        .get(&incoming[0])
                        .ok_or("landingpad PHI requires only landingpad pairs")?
                        .clone(),
                );
            } else {
                args.push(self.value(incoming[0])?);
            }
        }
        Ok(format!(
            "{}{}",
            self.blocks[&target],
            if args.is_empty() {
                String::new()
            } else {
                format!("({})", args.join(", "))
            }
        ))
    }
    fn clauses(&self, inst: Ref) -> Result<String> {
        let l = self.l;
        let mut clauses = Vec::new();
        for n in 0..l.get_num_clauses(inst) {
            let c = l.get_clause(inst, n);
            if l.get_type_kind(l.type_of(c)) != 12 {
                return Err("landingpad filters are unsupported".into());
            }
            if l.is_a_constant_pointer_null(c) != 0 {
                clauses.push("catch null".into());
            } else if self.globals.objects.contains(&c) {
                clauses.push(format!("catch {}", self.symbol(c)?));
            } else {
                return Err("catch requires a named global typeinfo or null".into());
            }
        }
        if l.is_cleanup(inst) != 0 {
            clauses.push("cleanup".into());
        }
        if clauses.is_empty() {
            return Err("empty landingpad".into());
        }
        Ok(clauses.join(", "))
    }
    fn translate(&mut self, options: Options) -> Result<String> {
        let l = self.l;
        let target = l.text_property(self.module, "target");
        static TARGET: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^(?:arm64|aarch64)-apple-(?:darwin|macosx)[0-9.]*$")
                .expect("constant target regex")
        });
        if !TARGET.is_match(&target) {
            return Err(format!(
                "only Apple AArch64 modules supported; input target {target:?}"
            ));
        }
        let layout = l.text_property(self.module, "layout");
        if layout.is_empty() || l.byte_order(self.td) != 1 || l.pointer_size(self.td) != 8 {
            return Err("requires explicit little-endian 64-bit pointer data layout".into());
        }
        for c in layout.split('-') {
            if (c.starts_with("p:") || c.starts_with("p0:"))
                && ![
                    "p:64:64",
                    "p:64:64:64",
                    "p:64:64:64:64",
                    "p0:64:64",
                    "p0:64:64:64",
                    "p0:64:64:64:64",
                ]
                .contains(&c)
            {
                return Err("unsupported address-space-zero pointer/index layout".into());
            }
        }
        if l.has_inline_asm(self.module) {
            return Err("module inline assembly".into());
        }
        if l.get_first_global_alias(self.module) != 0 || l.get_first_global_i_func(self.module) != 0
        {
            return Err("aliases and ifuncs are unsupported".into());
        }
        if l.has_linker_options(self.module) {
            return Err("module autolink requests (llvm.linker.options)".into());
        }
        let functions = sequence(
            || l.get_first_function(self.module),
            |f| l.get_next_function(f),
        );
        self.setup_functions(&functions, options)?;
        let mut output = self.setup_globals(&functions)?;
        for &f in &functions {
            if self.memory_name(f).is_some() {
                for u in sequence(|| l.get_first_use(f), |u| l.get_next_use(u)) {
                    let user = l.get_user(u);
                    if l.is_a_instruction(user) == 0
                        || l.get_instruction_opcode(user) != 45
                        || l.get_called_value(user) != f
                    {
                        return Err("memory intrinsic requires a direct call".into());
                    }
                }
                continue;
            }
            output.push(self.function(f)?);
        }
        if self.needs_resume {
            if let Some(&f) = functions.iter().find(|&&f| l.name(f) == "_Unwind_Resume") {
                if self.signatures[&f] != ("void".into(), vec!["ptr".into()])
                    || l.get_first_basic_block(f) != 0
                {
                    return Err("incompatible _Unwind_Resume runtime declaration".into());
                }
            } else {
                output.insert(0, "decl @_Unwind_Resume(ptr) cc(c)".into());
            }
        }
        Ok(output.join("\n\n") + "\n")
    }
    fn function(&mut self, f: Ref) -> Result<String> {
        let l = self.l;
        let (ret, params) = self.signatures[&f].clone();
        let name = self.symbol(f)?;
        let signature = format!(
            "{name}({}){}",
            params.join(", "),
            if ret == "void" {
                String::new()
            } else {
                format!(" -> {ret}")
            }
        );
        let blocks = sequence(|| l.get_first_basic_block(f), |b| l.get_next_basic_block(b));
        let attrs = self.function_attrs(f);
        let restricted: Vec<_> = l
            .params(l.global_get_value_type(f))
            .into_iter()
            .enumerate()
            .filter(|(index, ty)| {
                l.get_type_kind(*ty) == 12
                    && l.attribute(f, *index as u32 + 1, "noalias", false) != 0
                    && l.attribute(f, *index as u32 + 1, "sret", false) == 0
                    && l.attribute(f, *index as u32 + 1, "byval", false) == 0
            })
            .map(|(index, _)| index.to_string())
            .collect();
        let restriction = if blocks.is_empty() || restricted.is_empty() {
            String::new()
        } else {
            format!(" restrict_params({})", restricted.join(", "))
        };

        if blocks.is_empty() {
            return Ok(format!("decl {signature} cc(c){attrs}"));
        }
        let local_flag = if local(l.get_linkage(f)) || self.copies.contains(&f) {
            " local"
        } else if odr(l.get_linkage(f)) {
            " weak_odr"
        } else {
            ""
        };
        let personality = if l.has_personality_fn(f) != 0 {
            format!(" personality {}", self.symbol(l.get_personality_fn(f))?)
        } else {
            String::new()
        };
        self.lines = vec![format!(
            "func {signature} cc({}){attrs}{restriction}{local_flag}{personality} {{",
            if self.fast.contains(&f) { "fast" } else { "c" }
        )];
        self.values = (0..l.count_params(f))
            .map(|n| (l.get_param(f, n), format!("v{n}")))
            .collect();
        self.next_value = params.len();
        self.aggregate_params.clear();
        self.aggregate_slots.clear();
        self.pairs.clear();
        self.invoke_edges.clear();
        self.pad_insts.clear();
        self.slots.clear();
        self.blocks = blocks
            .iter()
            .enumerate()
            .map(|(n, &b)| (b, format!("block{n}")))
            .collect();
        let instructions: HashMap<_, _> = blocks
            .iter()
            .map(|&b| {
                (
                    b,
                    sequence(|| l.get_first_instruction(b), |i| l.get_next_instruction(i)),
                )
            })
            .collect();
        self.phis = blocks
            .iter()
            .map(|&b| {
                (
                    b,
                    instructions[&b]
                        .iter()
                        .copied()
                        .filter(|&i| l.get_instruction_opcode(i) == 44)
                        .collect(),
                )
            })
            .collect();
        let mut caught = HashSet::new();
        for &b in &blocks {
            for &i in &instructions[&b] {
                if l.get_instruction_opcode(i) == 59 {
                    self.pad_insts.insert(b, i);
                    for n in 0..l.get_num_clauses(i) {
                        caught.insert(l.get_clause(i, n));
                    }
                }
            }
        }
        if caught.len() > 63 {
            return Err("more than 63 caught types in one function".into());
        }
        self.split_pads = self
            .pad_insts
            .keys()
            .copied()
            .filter(|b| !self.phis[b].is_empty())
            .collect();
        for &b in &blocks {
            for &i in &instructions[&b] {
                let op = l.get_instruction_opcode(i);
                let t = l.type_of(i);
                if op == 59 || (op == 44 && l.get_type_kind(t) == 10) {
                    if !self.landing_pair(t)? {
                        return Err("landingpad result must be unpacked {ptr, i32}".into());
                    }
                    let pair = self.fresh_pair();
                    self.pairs.insert(i, pair);
                    continue;
                }
                if op == 54 && self.integer_pair(t) {
                    continue;
                }
                if l.get_type_kind(t) != 0 {
                    self.ty(t)?;
                    self.values.insert(i, format!("v{}", self.next_value));
                    self.next_value += 1;
                }
            }
        }
        let mut grounded: HashSet<_> = self.pad_insts.values().copied().collect();
        let mut pending: HashSet<_> = self
            .pairs
            .keys()
            .copied()
            .filter(|i| !grounded.contains(i))
            .collect();
        while !pending.is_empty() {
            let ready: Vec<_> = pending
                .iter()
                .copied()
                .filter(|&i| {
                    (0..l.count_incoming(i)).all(|n| grounded.contains(&l.get_incoming_value(i, n)))
                })
                .collect();
            if ready.is_empty() {
                return Err("landingpad PHIs must be acyclic merges of landingpad pairs".into());
            }
            for i in ready {
                grounded.insert(i);
                pending.remove(&i);
            }
        }
        for &b in &blocks {
            for &i in &instructions[&b] {
                if l.get_instruction_opcode(i) != 26 {
                    continue;
                }
                let count = l.get_operand(i, 0);
                if b != blocks[0] || l.is_a_constant_int(count) == 0 || self.width(count) > 64 {
                    return Err(format!(
                        "{name}: only constant entry-block alloca supported"
                    ));
                }
                let printed = l.printed(i);
                if word(&printed, "inalloca") || word(&printed, "swifterror") {
                    return Err(format!("{name}: special alloca ABI"));
                }
                self.ty(l.type_of(i))?;
                let t = l.get_allocated_type(i);
                self.layout_type(t)?;
                let size = u128::from(l.abi_size_of_type(self.td, t))
                    * u128::from(l.const_int_get_z_ext_value(count));
                let align = l.get_alignment(i).max(l.abi_alignment_of_type(self.td, t));
                if size == 0 || size > u128::from(u32::MAX) || align > 16 {
                    return Err(format!(
                        "{name}: alloca size/alignment exceeds fixed stack slot contract"
                    ));
                }
                let slot = format!("ss{}", self.slots.len());
                self.lines
                    .push(format!("    {slot} = stack_slot {size}, align {align}"));
                self.slots.insert(i, slot);
            }
        }
        for &b in &blocks {
            for &i in &instructions[&b] {
                if !matches!(l.get_instruction_opcode(i), 45 | 5) {
                    continue;
                }
                for n in 0..l.get_num_arg_operands(i) {
                    if self.integer_pair(l.type_of(l.get_operand(i, n))) {
                        let slot = format!("ss{}", self.slots.len() + self.aggregate_slots.len());
                        self.lines
                            .push(format!("    {slot} = stack_slot 16, align 8"));
                        self.aggregate_slots.insert((i, n), slot);
                    }
                }
            }
        }
        let mut aliases = HashMap::new();
        for &b in &blocks {
            self.block = b;
            let mut parameters = Vec::new();
            if b == blocks[0] {
                for n in 0..params.len() {
                    let t = l.type_of(l.get_param(f, n as u32));
                    parameters.push(format!(
                        "v{n}: {}",
                        if self.integer_pair(t) {
                            "ptr".into()
                        } else {
                            self.ty(t)?
                        }
                    ));
                }
            }
            if self.split_pads.contains(&b) {
                let [a, c] = &self.pairs[&self.pad_insts[&b]];
                parameters.extend([format!("{a}: ptr"), format!("{c}: i32")]);
            }
            for &phi in &self.phis[&b] {
                if let Some([a, c]) = self.pairs.get(&phi) {
                    parameters.extend([format!("{a}: ptr"), format!("{c}: i32")]);
                } else {
                    parameters.push(format!(
                        "{}: {}",
                        self.values[&phi],
                        self.ty(l.type_of(phi))?
                    ));
                }
            }
            self.lines.push(format!(
                "{}{}:",
                self.blocks[&b],
                if parameters.is_empty() {
                    String::new()
                } else {
                    format!("({})", parameters.join(", "))
                }
            ));
            if b == blocks[0] {
                for n in 0..params.len() {
                    let arg = l.get_param(f, n as u32);
                    if self.integer_pair(l.type_of(arg)) {
                        let low = self.emit(format!("load.i64 v{n}"));
                        let offset = self.emit("iconst.i64 8");
                        let addr = self.emit(format!("ptradd v{n}, {offset}"));
                        let high = self.emit(format!("load.i64 {addr}"));
                        self.aggregate_params.insert(arg, [low, high]);
                    }
                }
            }
            for &i in &instructions[&b] {
                if l.get_instruction_opcode(i) == 44 {
                    continue;
                }
                if let Some(v) = self.instruction(i).map_err(|e| format!("{name}: {e}"))? {
                    aliases.insert(self.values[&i].clone(), v);
                }
            }
        }
        for (bridge, pred, target, pad) in self.invoke_edges.clone() {
            self.block = pred;
            self.lines.push(format!("{bridge}:"));
            let edge = if let Some(pad) = pad {
                let pair = self.fresh_pair();
                self.lines.push(format!(
                    "    {}, {} = landingpad {}",
                    pair[0],
                    pair[1],
                    self.clauses(pad)?
                ));
                self.edge(target)?
                    .replacen('(', &format!("({}, {}, ", pair[0], pair[1]), 1)
            } else {
                self.edge(target)?
            };
            self.lines.push(format!("    jump {edge}"));
        }
        for line in &mut self.lines {
            let original = line.clone();
            *line = VALUE
                .replace_all(&original, |m: &regex::Captures<'_>| {
                    let matched = m.get(0).expect("regex matched a value");
                    let before = original[..matched.start()].chars().next_back();
                    if before.is_some_and(|c| c.is_ascii_alphanumeric() || "_.$@".contains(c)) {
                        return matched.as_str().to_string();
                    }
                    let mut v = matched.as_str().to_string();
                    let mut path = Vec::new();
                    while let Some(next) = aliases.get(&v) {
                        path.push(v.clone());
                        v = next.clone();
                    }
                    for alias in path {
                        aliases.insert(alias, v.clone());
                    }
                    v
                })
                .into_owned();
        }
        self.lines.push("}".into());
        Ok(self.lines.join("\n"))
    }
}
