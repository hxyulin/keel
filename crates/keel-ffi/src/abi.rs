//! Aggregate C layouts, full signatures and explicit call-operand contracts.
use super::systems::{abi, call_output};
use super::{
    KeelAbiParam, KeelAbiValue, KeelBlock, KeelCallArgument, KeelLayout, KeelModule, KeelSignature,
    KeelSignatureOptions, KeelStatus, KeelSymbol, KeelValue, Result, input, invalid, output, run,
    ty,
};
use keel_ir::{
    AbiParam, CallArgs, CallConv, InsertPoint, Layout, Purpose, Signature, Type, X86VectorAbi,
};

impl KeelModule {
    fn layout(&self, layout: KeelLayout) -> Result<&Layout> {
        if layout.owner != self.owner {
            return Err(invalid("foreign or stale layout ID"));
        }
        self.layouts
            .get(layout.index as usize)
            .ok_or_else(|| invalid("invalid layout ID"))
    }

    fn add_layout(&mut self, layout: Layout) -> Result<KeelLayout> {
        if layout.checked_size().is_none() {
            return Err(invalid("layout size overflows u64"));
        }
        let index = u32::try_from(self.layouts.len()).map_err(|_| invalid("too many layouts"))?;
        self.layouts.push(layout);
        Ok(KeelLayout {
            owner: self.owner,
            index,
        })
    }

    fn abi_value(&self, value: &KeelAbiValue) -> Result<AbiParam> {
        let mut param = abi(&KeelAbiParam {
            kind: value.kind,
            extension: value.extension,
        })?;
        param.purpose = match value.purpose {
            0 => Purpose::Normal,
            1 | 2 => {
                if param.ty != Type::Ptr {
                    return Err(invalid("aggregate ABI operands require ptr type"));
                }
                let layout = self.layout(value.layout)?.clone();
                let size = layout
                    .checked_size()
                    .ok_or_else(|| invalid("layout size overflows"))?;
                if size == 0 || size > u64::from(u32::MAX) {
                    return Err(invalid("aggregate ABI size must be in 1..=UINT32_MAX"));
                }
                if value.purpose == 1 {
                    Purpose::ByVal(layout)
                } else {
                    Purpose::Sret(layout)
                }
            }
            _ => return Err(invalid("unknown ABI purpose")),
        };
        Ok(param)
    }

    fn explicit_args(
        &self,
        function: u32,
        signature: &Signature,
        args: &[KeelCallArgument],
    ) -> Result<CallArgs> {
        let params = args
            .iter()
            .map(|a| Ok((self.value(function, a.value)?, self.abi_value(&a.abi)?)))
            .collect::<Result<Vec<_>>>()?;
        if if signature.variadic {
            params.len() < signature.params.len()
        } else {
            params.len() != signature.params.len()
        } {
            return Err(invalid("call argument count does not match signature"));
        }
        for (index, (v, abi)) in params.iter().enumerate() {
            if self.ir.functions[function as usize].value_type(*v) != abi.ty {
                return Err(invalid("explicit ABI type does not match value"));
            }
            if let Some(expected) = signature.params.get(index) {
                if abi != expected {
                    return Err(invalid(
                        "explicit fixed argument ABI does not match signature",
                    ));
                }
            } else if !matches!(abi.purpose, Purpose::ByVal(_))
                && (!matches!(abi.ty, Type::I32 | Type::I64 | Type::Ptr | Type::F64)
                    || abi.purpose != Purpose::Normal)
            {
                return Err(invalid(
                    "variadic tail requires promoted scalar or byval aggregate",
                ));
            }
        }
        // Fixed operands already carry this ABI in the callee signature. Keep
        // overrides only for aggregate varargs; redundant contracts would make
        // AArch64 reject an otherwise supported fixed aggregate call.
        Ok(CallArgs::from_optional(params.into_iter().enumerate().map(
            |(index, (value, abi))| {
                let explicit = (index >= signature.params.len() && abi.purpose != Purpose::Normal)
                    .then_some(abi);
                (value, explicit)
            },
        )))
    }
}

api! {
    /// Creates a natural scalar memory layout.
    fn keel_layout_scalar(module: *mut KeelModule, kind: u32, out: *mut KeelLayout) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| m.add_layout(Layout::Scalar(ty(kind)?))) }
    }
}
api! {
    /// Creates a natural C struct layout from copied field layouts.
    fn keel_layout_struct(module: *mut KeelModule, fields: *const KeelLayout, count: usize, out: *mut KeelLayout) {
        // SAFETY: Public pointer contract covers fields and output.
        unsafe { output(module, out, |m| {
            let fields = input(fields, count)?.iter().map(|l| m.layout(*l).cloned()).collect::<Result<Vec<_>>>()?;
            m.add_layout(Layout::Struct(fields))
        }) }
    }
}
api! {
    /// Creates an array layout with checked multiplication.
    fn keel_layout_array(module: *mut KeelModule, element: KeelLayout, count: u32, out: *mut KeelLayout) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| { let element = m.layout(element)?.clone(); m.add_layout(Layout::Array(Box::new(element), count)) }) }
    }
}
api! {
    /// Returns native layout size, including padding.
    fn keel_layout_size(module: *mut KeelModule, layout: KeelLayout, out: *mut u64) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| m.layout(layout)?.checked_size().ok_or_else(|| invalid("layout size overflows"))) }
    }
}
api! {
    /// Returns native natural alignment.
    fn keel_layout_align(module: *mut KeelModule, layout: KeelLayout, out: *mut u64) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| Ok(m.layout(layout)?.align())) }
    }
}
api! {
    /// Returns a struct field's byte offset, including preceding padding.
    fn keel_layout_field_offset(module: *mut KeelModule, layout: KeelLayout, index: usize, out: *mut u64) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let Layout::Struct(fields) = m.layout(layout)? else { return Err(invalid("field offset requires struct layout")); };
            if index >= fields.len() { return Err(invalid("field index out of range")); }
            let mut offset = 0u64;
            for (field_index, field) in fields[..=index].iter().enumerate() {
                let align = field.align();
                offset = offset.checked_add(align - 1).map(|v| v / align * align).ok_or_else(|| invalid("offset overflows"))?;
                if field_index == index { return Ok(offset); }
                offset = offset.checked_add(field.size()).ok_or_else(|| invalid("offset overflows"))?;
            }
            Err(invalid("field index out of range"))
        }) }
    }
}
api! {
    /// Creates a full C/private signature with aggregate argument contracts and varargs.
    fn keel_signature_create_full(module: *mut KeelModule, params: *const KeelAbiValue, count: usize,
        returns: *const KeelAbiValue, return_count: usize, options: KeelSignatureOptions, out: *mut KeelSignature) {
        // SAFETY: Public pointer contract covers arrays and output.
        unsafe { output(module, out, |m| {
            if return_count > 1 || options.flags & !3 != 0 { return Err(invalid("invalid signature flags or return count")); }
            let cc = match options.convention { 0 => CallConv::C, 1 => CallConv::Fast, _ => return Err(invalid("unknown calling convention")) };
            let x86_vector_abi = match options.x86_vector_abi { 0 => None, 1 => Some(X86VectorAbi::Gnu), 2 => Some(X86VectorAbi::Clang), _ => return Err(invalid("unknown x86 vector ABI")) };
            if cc == CallConv::Fast && (options.flags != 0 || x86_vector_abi.is_some()) { return Err(invalid("private convention needs fixed non-vector C boundary policy")); }
            let params = input(params, count)?.iter().map(|p| m.abi_value(p)).collect::<Result<Vec<_>>>()?;
            let returns = input(returns, return_count)?.iter().map(|p| m.abi_value(p)).collect::<Result<Vec<_>>>()?;
            if returns.iter().any(|p| p.purpose != Purpose::Normal) { return Err(invalid("aggregate returns use a first sret parameter, not a return layout")); }
            if params.iter().enumerate().any(|(i,p)| matches!(p.purpose, Purpose::Sret(_)) && (i != 0 || !returns.is_empty())) { return Err(invalid("sret must be the first parameter of a void function")); }
            let index = u32::try_from(m.signatures.len()).map_err(|_| invalid("too many signatures"))?;
            m.signatures.push(Signature { params, returns, cc, variadic: options.flags & 1 != 0, returns_twice: options.flags & 2 != 0, x86_vector_abi });
            Ok(KeelSignature { owner: m.owner, index })
        }) }
    }
}
api! {
    /// Appends a direct call with explicit ABI contracts for every operand.
    fn keel_build_call_abi(module: *mut KeelModule, block: KeelBlock, symbol: KeelSymbol,
        args: *const KeelCallArgument, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers arrays and their nonoverlapping outputs.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let entry = m.symbol(symbol)?;
            let signature = entry.signature.clone().ok_or_else(|| invalid("call target is not function"))?;
            let name = entry.name.clone();
            call_output(results, result_count, &signature)?;
            let args = m.explicit_args(block.function, &signature, input(args, count)?)?;
            let attrs = m.ir.function_attrs(&name).cloned().unwrap_or_default();
            let f = &mut m.ir.functions[block.function as usize];
            let callee = f.import_function(&name, signature);
            f.set_callee_attrs(callee, attrs);
            let inst = f.ins(InsertPoint::End(b)).call_abi(callee, args);
            if result_count != 0 { results.write(m.value_id(block.function, m.ir.functions[block.function as usize].inst_results(inst)[0])); }
            Ok(())
        }) }
    }
}
api! {
    /// Appends an indirect call with explicit ABI contracts for every operand.
    fn keel_build_call_indirect_abi(module: *mut KeelModule, block: KeelBlock, signature: KeelSignature, callee: KeelValue,
        args: *const KeelCallArgument, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers arrays and outputs.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let signature = m.signature(signature)?.clone();
            let callee = m.pointer(block.function, callee)?;
            call_output(results, result_count, &signature)?;
            let args = m.explicit_args(block.function, &signature, input(args, count)?)?;
            let f = &mut m.ir.functions[block.function as usize];
            let sig = f.import_signature(signature);
            let inst = f.ins(InsertPoint::End(b)).call_indirect_abi(sig, callee, args);
            if result_count != 0 { results.write(m.value_id(block.function, m.ir.functions[block.function as usize].inst_results(inst)[0])); }
            Ok(())
        }) }
    }
}
api! {
    /// Appends a promoted scalar variadic extraction from a va_list pointer.
    fn keel_build_va_arg(module: *mut KeelModule, block: KeelBlock, kind: u32, list: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let list = m.pointer(block.function, list)?; let kind = ty(kind)?;
            if !matches!(kind, Type::I32 | Type::I64 | Type::Ptr | Type::F64) { return Err(invalid("va_arg needs promoted scalar type")); }
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).va_arg(kind, list);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
