//! Checked wrappers for scalar ABI signatures, symbols, calls and storage.

use keel_ir::{
    AbiParam, ArgExt, ConvertOp, DataInit, DataItem, DataObject, EntityRef, FuncDecl, Function,
    InsertPoint, MemoryFlags, Signature, StackSlot, Symbol, Type, Value,
};

use super::{
    KeelAbiParam, KeelBlock, KeelFunction, KeelMemory, KeelModule, KeelSignature, KeelStackSlot,
    KeelStatus, KeelSymbol, KeelValue, Result, input, invalid, output, run, ty,
};

#[derive(Debug)]
pub(super) struct SymbolEntry {
    pub(super) name: String,
    // None means a data symbol; function signatures are immutable.
    pub(super) signature: Option<Signature>,
}

pub(super) fn name(bytes: &[u8]) -> Result<&str> {
    let name = std::str::from_utf8(bytes).map_err(|_| invalid("symbol name is not UTF-8"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.$".contains(&c))
        || name.as_bytes()[0].is_ascii_digit()
    {
        return Err(invalid("symbol name must be an identifier"));
    }
    Ok(name)
}

pub(super) fn abi(param: &KeelAbiParam) -> Result<AbiParam> {
    let kind = ty(param.kind)?;
    let ext = match param.extension {
        0 => ArgExt::None,
        1 => ArgExt::Sext,
        2 => ArgExt::Uext,
        _ => return Err(invalid("unknown ABI extension")),
    };
    if matches!(kind, Type::I8 | Type::I16) == (ext == ArgExt::None) {
        return Err(invalid(
            "i8/i16 require signed or unsigned ABI extension; other types require none",
        ));
    }
    Ok(AbiParam {
        ext,
        ..AbiParam::new(kind)
    })
}

fn memory(raw: KeelMemory) -> Result<MemoryFlags> {
    if !raw.align.is_power_of_two() || raw.flags & !7 != 0 {
        return Err(invalid(
            "memory alignment must be a power of two and flags must be known",
        ));
    }
    if raw.flags & 5 == 5 {
        return Err(invalid(
            "volatile memory cannot have undefined fault behavior",
        ));
    }
    Ok(MemoryFlags {
        align: raw.align,
        volatile: raw.flags & 1 != 0,
        notrap: raw.flags & 2 != 0,
        trap_ub: raw.flags & 4 != 0,
    })
}

fn global_options(align: u32, flags: u32) -> Result<()> {
    if !align.is_power_of_two() || align > keel_ir::MAX_DATA_ALIGN || flags & !3 != 0 {
        return Err(invalid(
            "global alignment must be a power of two up to 65536 and flags must be known",
        ));
    }
    Ok(())
}

impl KeelModule {
    pub(super) fn unused_name(&self, name: &str) -> Result<()> {
        if self.ir.signature(name).is_some() {
            return Err(invalid("duplicate function name"));
        }
        if self.ir.data.iter().any(|d| d.name == name) {
            return Err(invalid("duplicate symbol name"));
        }
        Ok(())
    }

    pub(super) fn signature(&self, signature: KeelSignature) -> Result<&Signature> {
        if signature.owner != self.owner {
            return Err(invalid("foreign or stale signature ID"));
        }
        self.signatures
            .get(signature.index as usize)
            .ok_or_else(|| invalid("invalid signature ID"))
    }

    pub(super) fn symbol(&self, symbol: KeelSymbol) -> Result<&SymbolEntry> {
        if symbol.owner != self.owner {
            return Err(invalid("foreign or stale symbol ID"));
        }
        self.symbols
            .get(symbol.index as usize)
            .ok_or_else(|| invalid("invalid symbol ID"))
    }

    pub(super) fn intern_symbol(
        &mut self,
        name: &str,
        signature: Option<Signature>,
    ) -> Result<KeelSymbol> {
        let index = if let Some(index) = self.symbols.iter().position(|s| s.name == name) {
            index as u32
        } else {
            let index =
                u32::try_from(self.symbols.len()).map_err(|_| invalid("too many symbols"))?;
            self.symbols.push(SymbolEntry {
                name: name.into(),
                signature,
            });
            index
        };
        Ok(KeelSymbol {
            owner: self.owner,
            index,
        })
    }

    pub(super) fn pointer(&self, function: u32, value: KeelValue) -> Result<Value> {
        let v = self.value(function, value)?;
        if self.ir.functions[function as usize].value_type(v) != Type::Ptr {
            return Err(invalid("address must have pointer type"));
        }
        Ok(v)
    }

    pub(super) fn slot(&self, block: KeelBlock, slot: KeelStackSlot) -> Result<StackSlot> {
        if slot.owner != self.owner || slot.function != block.function {
            return Err(invalid("foreign or stale stack slot ID"));
        }
        if slot.index as usize >= self.ir.functions[block.function as usize].num_stack_slots() {
            return Err(invalid("invalid stack slot ID"));
        }
        Ok(StackSlot::new(slot.index as usize))
    }

    pub(super) fn call_arguments(
        &self,
        function: u32,
        sig: &Signature,
        args: &[KeelValue],
    ) -> Result<Vec<Value>> {
        let values = self.values(function, args)?;
        if (if sig.variadic {
            values.len() < sig.params.len()
        } else {
            values.len() != sig.params.len()
        }) || values
            .iter()
            .zip(&sig.params)
            .any(|(v, p)| self.ir.functions[function as usize].value_type(*v) != p.ty)
        {
            return Err(invalid("call arguments do not match signature"));
        }
        if sig.variadic
            && values.iter().skip(sig.params.len()).any(|v| {
                !matches!(
                    self.ir.functions[function as usize].value_type(*v),
                    Type::I32 | Type::I64 | Type::Ptr | Type::F64
                )
            })
        {
            return Err(invalid(
                "variadic arguments require C default promotions or an explicit aggregate ABI",
            ));
        }
        Ok(values)
    }

    pub(super) fn define_data(
        &mut self,
        name: &str,
        init: DataInit,
        flags: u32,
    ) -> Result<KeelSymbol> {
        global_options(init.align, flags)?;
        if self.ir.signature(name).is_some() {
            return Err(invalid("duplicate symbol name"));
        }
        if let Some(data) = self.ir.data.iter_mut().find(|d| d.name == name) {
            if data.init.is_some() {
                return Err(invalid("duplicate global definition"));
            }
            data.init = Some(init);
            data.local = flags & 2 != 0;
        } else {
            self.ir.data.push(DataObject {
                name: name.into(),
                init: Some(init),
                local: flags & 2 != 0,
                ..DataObject::default()
            });
        }
        self.intern_symbol(name, None)
    }
}

// Validate call output shape before mutation. Allocation validity is checked
// by the caller's pointer contract when the result is actually written.
pub(super) fn call_output(ptr: *mut KeelValue, count: usize, sig: &Signature) -> Result<()> {
    if count != sig.returns.len() || (count != 0 && ptr.is_null()) {
        return Err(invalid(
            "call result count must match signature and nonempty output cannot be null",
        ));
    }
    Ok(())
}

api! {
    /// Creates an immutable fixed-arity C ABI signature with explicit extensions.
    fn keel_signature_create(module: *mut KeelModule, params: *const KeelAbiParam, param_count: usize,
        returns: *const KeelAbiParam, return_count: usize, out: *mut KeelSignature) {
        // SAFETY: Public pointer contract covers all borrowed arrays and output.
        unsafe { output(module, out, |m| {
            if return_count > 1 { return Err(invalid("at most one return value is supported")); }
            let params = input(params, param_count)?.iter().map(abi).collect::<Result<Vec<_>>>()?;
            let returns = input(returns, return_count)?.iter().map(abi).collect::<Result<Vec<_>>>()?;
            let sig = Signature { params, returns, ..Signature::new(&[], &[]) };
            let index = u32::try_from(m.signatures.len()).map_err(|_| invalid("too many signatures"))?;
            m.signatures.push(sig);
            Ok(KeelSignature { owner: m.owner, index })
        }) }
    }
}

api! {
    /// Adds a function definition using a previously created ABI signature.
    fn keel_module_add_function_with_signature(module: *mut KeelModule, raw_name: *const u8, name_len: usize,
        signature: KeelSignature, out: *mut KeelFunction) {
        // SAFETY: Public pointer contract covers the name and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            m.unused_name(name)?;
            let sig = m.signature(signature)?.clone();
            let index = u32::try_from(m.ir.functions.len()).map_err(|_| invalid("too many functions"))?;
            let mut function = Function::new(name, sig);
            function.local = function.sig.cc == keel_ir::CallConv::Fast;
            m.ir.functions.push(function);
            Ok(KeelFunction { owner: m.owner, index })
        }) }
    }
}

api! {
    /// Declares an external or forward function and returns its module symbol.
    fn keel_module_declare_function(module: *mut KeelModule, raw_name: *const u8, name_len: usize,
        signature: KeelSignature, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers the name and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            m.unused_name(name)?;
            let sig = m.signature(signature)?.clone();
            m.ir.decls.push(FuncDecl { name: name.into(), sig: sig.clone(), attrs: Default::default() });
            m.intern_symbol(name, Some(sig))
        }) }
    }
}

api! {
    /// Turns a forward declaration into a definition, preserving its symbol ID.
    fn keel_module_define_function(module: *mut KeelModule, symbol: KeelSymbol, out: *mut KeelFunction) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let entry = m.symbol(symbol)?;
            let sig = entry.signature.clone().ok_or_else(|| invalid("symbol is not a function"))?;
            let name = entry.name.clone();
            if m.ir.functions.iter().any(|f| f.name == name) { return Err(invalid("function is already defined")); }
            let index = u32::try_from(m.ir.functions.len()).map_err(|_| invalid("too many functions"))?;
            let attrs = m.ir.function_attrs(&name).cloned().unwrap_or_default();
            m.ir.decls.retain(|d| d.name != name);
            let mut function = Function::new(name, sig);
            function.attrs = attrs;
            function.local = function.sig.cc == keel_ir::CallConv::Fast;
            m.ir.functions.push(function);
            Ok(KeelFunction { owner: m.owner, index })
        }) }
    }
}

api! {
    /// Returns the module symbol of a function definition for calls or addresses.
    fn keel_function_symbol(module: *mut KeelModule, function: KeelFunction, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let f = &m.ir.functions[m.function_index(function)?];
            let (name, sig) = (f.name.clone(), f.sig.clone());
            m.intern_symbol(&name, Some(sig))
        }) }
    }
}

api! {
    /// Selects exported or module-local linkage for a function definition.
    fn keel_function_set_linkage(module: *mut KeelModule, function: KeelFunction, linkage: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let index = m.function_index(function)?;
            let local = match linkage { 0 => false, 1 => true, _ => return Err(invalid("unknown linkage")) };
            if !local && m.ir.functions[index].sig.cc == keel_ir::CallConv::Fast { return Err(invalid("private calling convention requires local linkage")); }
            m.ir.functions[index].local = local;
            Ok(())
        }) }
    }
}

api! {
    /// Creates a nonempty fixed-size stack slot with alignment up to 64 bytes.
    fn keel_stack_slot_create(module: *mut KeelModule, function: KeelFunction, size: u32, align: u32, out: *mut KeelStackSlot) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let f = m.function_index(function)?;
            if size == 0 || !align.is_power_of_two() || align > 64 {
                return Err(invalid("stack size must be nonzero and alignment a power of two up to 64"));
            }
            let index = m.ir.functions[f].create_stack_slot(size, align).index() as u32;
            Ok(KeelStackSlot { owner: m.owner, function: function.index, index })
        }) }
    }
}

api! {
    /// Appends the address of a stack slot in this function.
    fn keel_build_stack_address(module: *mut KeelModule, block: KeelBlock, slot: KeelStackSlot, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let slot = m.slot(block, slot)?;
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).stack_addr(slot);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a byte offset to a pointer; the offset must be i64.
    fn keel_build_ptradd(module: *mut KeelModule, block: KeelBlock, address: KeelValue, offset: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let address = m.pointer(block.function, address)?;
            let offset = m.value(block.function, offset)?;
            let f = &mut m.ir.functions[block.function as usize];
            if f.value_type(offset) != Type::I64 { return Err(invalid("pointer offset must be i64")); }
            let v = f.ins(InsertPoint::End(b)).ptradd(address, offset);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a scalar load with explicit memory promises.
    fn keel_build_load(module: *mut KeelModule, block: KeelBlock, kind: u32, address: KeelValue, flags: KeelMemory, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let ty = ty(kind)?;
            let address = m.pointer(block.function, address)?;
            let flags = memory(flags)?;
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).load_with_flags(ty, address, flags);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a scalar store with explicit memory promises.
    fn keel_build_store(module: *mut KeelModule, block: KeelBlock, value: KeelValue, address: KeelValue, flags: KeelMemory) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let value = m.value(block.function, value)?;
            let address = m.pointer(block.function, address)?;
            let flags = memory(flags)?;
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).store_with_flags(value, address, flags);
            Ok(())
        }) }
    }
}

api! {
    /// Appends sign/zero extension, truncation, ptr-to-i64 or i64-to-ptr conversion.
    fn keel_build_convert(module: *mut KeelModule, block: KeelBlock, op: u32, kind: u32, value: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let value = m.value(block.function, value)?;
            let to = ty(kind)?;
            let f = &mut m.ir.functions[block.function as usize];
            let from = f.value_type(value);
            let op = match op {
                1 | 2 if from.is_int() && to.is_int() && from.bytes() < to.bytes() => {
                    if op == 1 { ConvertOp::Sext } else { ConvertOp::Uext }
                }
                3 if from.is_int() && to.is_int() && from.bytes() > to.bytes() => ConvertOp::Trunc,
                4 if (from, to) == (Type::Ptr, Type::I64) => ConvertOp::PtrToInt,
                5 if (from, to) == (Type::I64, Type::Ptr) => ConvertOp::IntToPtr,
                6 if from.is_float() && to.is_float() && from.bytes() < to.bytes() => ConvertOp::Fpext,
                7 if from.is_float() && to.is_float() && from.bytes() > to.bytes() => ConvertOp::Fptrunc,
                8 | 9 if from.is_float() && matches!(to, Type::I32 | Type::I64) => { if op == 8 { ConvertOp::Fptosi } else { ConvertOp::Fptoui } }
                10 | 11 if from.is_int() && from != Type::I128 && to.is_float() => { if op == 10 { ConvertOp::Sitofp } else { ConvertOp::Uitofp } }
                12 if from.bytes() == to.bytes() && ((from.is_float() && to.is_int()) || (from.is_int() && to.is_float()) || (from.is_vector() || to.is_vector())) => ConvertOp::Bitcast,
                _ => return Err(invalid("unknown conversion or invalid source/destination types")),
            };
            let v = f.ins(InsertPoint::End(b)).convert(op, to, value);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Takes a module function/data symbol's address in the current function.
    fn keel_build_symbol_address(module: *mut KeelModule, block: KeelBlock, symbol: KeelSymbol, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let entry = m.symbol(symbol)?;
            let (name, sig) = (entry.name.clone(), entry.signature.clone());
            let attrs = m.ir.function_attrs(&name).cloned().unwrap_or_default();
            let f = &mut m.ir.functions[block.function as usize];
            let symbol = match sig {
                Some(sig) => {
                    let reference = f.import_function(&name, sig);
                    f.set_callee_attrs(reference, attrs);
                    Symbol::Func(reference)
                },
                None => Symbol::Data(f.import_data(&name)),
            };
            let v = f.ins(InsertPoint::End(b)).symbol_addr(symbol);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a fixed-arity direct call, validating arguments and result capacity.
    fn keel_build_call(module: *mut KeelModule, block: KeelBlock, symbol: KeelSymbol,
        args: *const KeelValue, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers module, argument and result arrays.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let entry = m.symbol(symbol)?;
            let sig = entry.signature.clone().ok_or_else(|| invalid("call target is not a function"))?;
            let name = entry.name.clone();
            call_output(results, result_count, &sig)?;
            let args = m.call_arguments(block.function, &sig, input(args, count)?)?;
            let attrs = m.ir.function_attrs(&name).cloned().unwrap_or_default();
            let f = &mut m.ir.functions[block.function as usize];
            let callee = f.import_function(&name, sig);
            f.set_callee_attrs(callee, attrs);
            let inst = f.ins(InsertPoint::End(b)).call(callee, &args);
            if result_count != 0 { results.write(m.value_id(block.function, m.ir.functions[block.function as usize].inst_results(inst)[0])); }
            Ok(())
        }) }
    }
}

api! {
    /// Appends a fixed-arity indirect call through a pointer and explicit signature.
    fn keel_build_call_indirect(module: *mut KeelModule, block: KeelBlock, signature: KeelSignature, callee: KeelValue,
        args: *const KeelValue, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers module, argument and result arrays.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let sig = m.signature(signature)?.clone();
            let callee = m.pointer(block.function, callee)?;
            call_output(results, result_count, &sig)?;
            let args = m.call_arguments(block.function, &sig, input(args, count)?)?;
            let f = &mut m.ir.functions[block.function as usize];
            let sig = f.import_signature(sig);
            let inst = f.ins(InsertPoint::End(b)).call_indirect(sig, callee, &args);
            if result_count != 0 { results.write(m.value_id(block.function, m.ir.functions[block.function as usize].inst_results(inst)[0])); }
            Ok(())
        }) }
    }
}

api! {
    /// Declares external storage. Later global definition APIs can supply its initializer.
    fn keel_module_declare_global(module: *mut KeelModule, raw_name: *const u8, name_len: usize, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            m.unused_name(name)?;
            m.ir.data.push(DataObject { name: name.into(), ..DataObject::default() });
            m.intern_symbol(name, None)
        }) }
    }
}

api! {
    /// Defines copied initialized bytes, or completes an external storage declaration.
    fn keel_module_add_global(module: *mut KeelModule, raw_name: *const u8, name_len: usize,
        bytes: *const u8, size: usize, align: u32, flags: u32, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name, byte input and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            global_options(align, flags)?;
            if size == 0 || size > u32::MAX as usize { return Err(invalid("global byte size must be in 1..=UINT32_MAX")); }
            let bytes = input(bytes, size)?.to_vec();
            m.define_data(name, DataInit { align, constant: flags & 1 != 0, items: vec![DataItem::Bytes(bytes)] }, flags)
        }) }
    }
}

api! {
    /// Defines zero-initialized storage, or completes an external storage declaration.
    fn keel_module_add_zero_global(module: *mut KeelModule, raw_name: *const u8, name_len: usize,
        size: u32, align: u32, flags: u32, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            if size == 0 { return Err(invalid("global size must be nonzero")); }
            m.define_data(name, DataInit { align, constant: flags & 1 != 0, items: vec![DataItem::Zero(size)] }, flags)
        }) }
    }
}

api! {
    /// Defines one native pointer relocation to a module symbol plus a byte addend.
    fn keel_module_add_pointer_global(module: *mut KeelModule, raw_name: *const u8, name_len: usize,
        target: KeelSymbol, addend: u32, flags: u32, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name and output.
        unsafe { output(module, out, |m| {
            let name = name(input(raw_name, name_len)?)?;
            let target = m.symbol(target)?.name.clone();
            m.define_data(name, DataInit { align: 8, constant: flags & 1 != 0,
                items: vec![DataItem::Ptr { symbol: target, addend }] }, flags)
        }) }
    }
}

#[cfg(test)]
#[path = "systems_tests.rs"]
mod tests;
