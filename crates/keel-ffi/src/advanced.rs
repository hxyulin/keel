//! Open-ended intrinsic/assembly input and branch tables.
use super::{
    KeelBlock, KeelModule, KeelStatus, KeelSymbol, KeelValue, Result, input, invalid, output, run,
    ty,
};
use keel_ir::{BlockCall, InlineAsm, InsertPoint, Intrinsic, NeonIntrinsic};

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| invalid("input must be UTF-8"))
}

api! {
    /// Appends any registry intrinsic by its core name, checking its declared signature.
    fn keel_build_intrinsic(module: *mut KeelModule, block: KeelBlock, name: *const u8, length: usize,
        args: *const KeelValue, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers text, input array and disjoint outputs.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let intrinsic = Intrinsic::from_name(text(input(name, length)?)?).ok_or_else(|| invalid("unknown intrinsic"))?;
            let info = intrinsic.info();
            if result_count != info.returns.len() || (result_count != 0 && results.is_null()) { return Err(invalid("intrinsic result count mismatch")); }
            let args = m.values(block.function, input(args, count)?)?;
            let f = &mut m.ir.functions[block.function as usize];
            if args.iter().map(|v| f.value_type(*v)).ne(info.params.iter().copied()) { return Err(invalid("intrinsic argument types do not match registry")); }
            let inst = f.ins(InsertPoint::End(b)).intrinsic(intrinsic, &args);
            let values = m.ir.functions[block.function as usize].inst_results(inst);
            for (index, value) in values.iter().enumerate() { results.add(index).write(m.value_id(block.function, *value)); }
            Ok(())
        }) }
    }
}
api! {
    /// Appends NEON operations by core name, including structured memory operations.
    fn keel_build_neon(module: *mut KeelModule, block: KeelBlock, name: *const u8, length: usize, kind: u32,
        args: *const KeelValue, count: usize, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers text, input array and disjoint outputs.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            let op = text(input(name, length)?)?.parse::<NeonIntrinsic>().map_err(|_| invalid("unknown NEON operation"))?;
            let kind = ty(kind)?;
            if !op.accepts(kind) || result_count != op.result_count() || (result_count != 0 && results.is_null()) { return Err(invalid("invalid NEON result shape/count")); }
            let args = m.values(block.function, input(args, count)?)?;
            let f = &mut m.ir.functions[block.function as usize];
            let expected = op.operand_types(kind).ok_or_else(|| invalid("invalid NEON operand shape"))?;
            if args.iter().map(|v| f.value_type(*v)).ne(expected) { return Err(invalid("NEON argument type mismatch")); }
            let inst = f.ins(InsertPoint::End(b)).build(keel_ir::InstData::Neon { op, ty: kind, args });
            for (index, value) in m.ir.functions[block.function as usize].inst_results(inst).iter().enumerate() { results.add(index).write(m.value_id(block.function, *value)); }
            Ok(())
        }) }
    }
}
api! {
    /// Appends inline assembly using the core constraint parser and explicit result types.
    fn keel_build_inline_asm(module: *mut KeelModule, block: KeelBlock, template: *const u8, template_length: usize,
        constraints: *const u8, constraint_length: usize, kinds: *const u32, result_count: usize,
        args: *const KeelValue, count: usize, results: *mut KeelValue) {
        // SAFETY: Public pointer contract covers borrowed text/arrays and disjoint output.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            if result_count != 0 && results.is_null() { return Err(invalid("null assembly output")); }
            let kinds = input(kinds, result_count)?.iter().map(|k| ty(*k)).collect::<Result<Vec<_>>>()?;
            let asm = InlineAsm::new(text(input(template, template_length)?)?, text(input(constraints, constraint_length)?)?, &kinds)
                .map_err(|e| (super::INVALID,e))?;
            let args = m.values(block.function, input(args, count)?)?;
            if args.len() != asm.inputs.len() { return Err(invalid("assembly input count does not match constraints")); }
            let inst = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).inline_asm(asm, &args);
            for (index, value) in m.ir.functions[block.function as usize].inst_results(inst).iter().enumerate() { results.add(index).write(m.value_id(block.function, *value)); }
            Ok(())
        }) }
    }
}
api! {
    /// Terminates with an integer switch; each case has its own argument span.
    fn keel_build_switch(module: *mut KeelModule, block: KeelBlock, value: KeelValue,
        default_block: KeelBlock, default_args: *const KeelValue, default_count: usize,
        cases: *const super::KeelSwitchCase, count: usize) {
        // SAFETY: Public pointer contract also covers every case's nested argument span.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?; let value = m.value(block.function, value)?;
            if !m.ir.functions[block.function as usize].value_type(value).is_int() { return Err(invalid("switch selector must be integer")); }
            let args = m.values(block.function, input(default_args, default_count)?)?;
            let default = BlockCall { block: m.destination(block, default_block, &args)?, args };
            let mut destinations = Vec::new(); let mut values = std::collections::HashSet::new();
            for case in input(cases, count)? {
                if !values.insert(case.value) { return Err(invalid("duplicate switch case")); }
                let args = m.values(block.function, input(case.args, case.count)?)?;
                destinations.push((case.value, BlockCall { block: m.destination(block, case.block, &args)?, args }));
            }
            m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).switch(value, default, destinations);
            Ok(())
        }) }
    }
}
api! {
    /// Sets thread-local, weak, hidden and retained data properties with core validation.
    fn keel_global_set_properties(module: *mut KeelModule, symbol: KeelSymbol, flags: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let entry = m.symbol(symbol)?;
            if entry.signature.is_some() { return Err(invalid("symbol is not data")); }
            let name = entry.name.clone();
            let index = m.ir.data.iter().position(|d| d.name == name).ok_or_else(|| invalid("data symbol not found"))?;
            if flags & !31 != 0 { return Err(invalid("unknown data property flags")); }
            let old = &m.ir.data[index];
            let mut data = old.clone();
            data.thread_local = flags & 1 != 0; data.weak = flags & 2 != 0; data.hidden = flags & 4 != 0;
            data.retain = flags & 8 != 0; data.weak_odr = flags & 16 != 0;
            if (data.thread_local && data.init.as_ref().is_some_and(|i| i.constant))
                || (data.weak && (data.local || data.thread_local || data.weak_odr))
                || (data.hidden && (data.local || data.init.is_none()))
                || (data.retain && (data.weak_odr || data.init.is_none()))
                || (data.weak_odr && (data.local || data.thread_local || data.init.is_none())) {
                return Err(invalid("incompatible global properties"));
            }
            m.ir.data[index] = data; Ok(())
        }) }
    }
}

api! {
    /// Defines an ordered initializer containing bytes, zeros, integers and relocations without implicit padding.
    fn keel_module_add_global_items(module: *mut KeelModule, name: *const u8, length: usize,
        items: *const super::KeelDataItem, count: usize, align: u32, flags: u32, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name, item array, each selected byte span, and output.
        unsafe { output(module, out, |m| {
            let name = super::systems::name(input(name, length)?)?;
            let mut decoded = Vec::new();
            for item in input(items, count)? {
                decoded.push(match item.kind {
                    0 => keel_ir::DataItem::Bytes(input(item.bytes, item.length)?.to_vec()),
                    1 => keel_ir::DataItem::Zero(u32::try_from(item.length).map_err(|_| invalid("zero span exceeds UINT32_MAX"))?),
                    2 => {
                        let kind = ty(item.scalar_type)?;
                        if !kind.is_int() || kind == keel_ir::Type::I128 { return Err(invalid("integer initializer requires i8/i16/i32/i64")); }
                        let bits = kind.bytes().expect("integer type has fixed width") * 8;
                        if bits < 64 && (item.value < -(1i64 << (bits-1)) || item.value > (1i64 << bits)-1) { return Err(invalid("integer initializer does not fit type")); }
                        keel_ir::DataItem::Int { ty: kind, value: item.value }
                    }
                    3 => keel_ir::DataItem::Ptr { symbol: m.symbol(item.symbol)?.name.clone(), addend: item.addend },
                    _ => return Err(invalid("unknown initializer item kind")),
                });
            }
            let init = keel_ir::DataInit { align, constant: flags & 1 != 0, items: decoded };
            let size = init.checked_size_with_layout(keel_ir::DataLayout::NATIVE64).ok_or_else(|| invalid("global size overflows"))?;
            if size == 0 || size > u64::from(u32::MAX) { return Err(invalid("global size must be in 1..=UINT32_MAX")); }
            m.define_data(name, init, flags)
        }) }
    }
}

api! {
    /// Sets function declaration/definition flags and alignment, keeping existing call references consistent.
    fn keel_symbol_set_function_attributes(module: *mut KeelModule, symbol: KeelSymbol, flags: u64, align: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let entry = m.symbol(symbol)?;
            let signature = entry.signature.as_ref().ok_or_else(|| invalid("symbol is not a function"))?;
            if flags >> 20 != 0 || (align != 0 && (!align.is_power_of_two() || align > keel_ir::MAX_DATA_ALIGN)) { return Err(invalid("unknown function flags or invalid alignment")); }
            let name = entry.name.clone();
            let mut attrs = m.ir.function_attrs(&name).cloned().unwrap_or_default();
            attrs.weak = flags & (1u64 << 0) != 0;
            attrs.constructor = flags & (1u64 << 1) != 0;
            attrs.destructor = flags & (1u64 << 2) != 0;
            attrs.used = flags & (1u64 << 3) != 0;
            attrs.retain = flags & (1u64 << 4) != 0;
            attrs.noninterposable = flags & (1u64 << 5) != 0;
            attrs.no_semantic_interposition = flags & (1u64 << 6) != 0;
            attrs.inlinehint = flags & (1u64 << 7) != 0;
            attrs.alwaysinline = flags & (1u64 << 8) != 0;
            attrs.noinline = flags & (1u64 << 9) != 0;
            attrs.noreturn = flags & (1u64 << 10) != 0;
            attrs.nounwind = flags & (1u64 << 11) != 0;
            attrs.cold = flags & (1u64 << 12) != 0;
            attrs.hot = flags & (1u64 << 13) != 0;
            attrs.optnone = flags & (1u64 << 14) != 0;
            attrs.flatten = flags & (1u64 << 15) != 0;
            attrs.no_instrument_function = flags & (1u64 << 16) != 0;
            attrs.no_profile_instrument_function = flags & (1u64 << 17) != 0;
            attrs.naked = flags & (1u64 << 18) != 0;
            attrs.interrupt = flags & (1u64 << 19) != 0;

            attrs.align = (align != 0).then_some(align);
            attrs.validate(signature.returns_twice).map_err(invalid)?;
            if (attrs.constructor || attrs.destructor) && (!signature.params.is_empty() || !signature.returns.is_empty()) { return Err(invalid("lifecycle functions must be void(void)")); }
            if let Some(function) = m.ir.functions.iter_mut().find(|f| f.name == name) { function.attrs = attrs; }
            else {
                if attrs.constructor || attrs.destructor || attrs.retain { return Err(invalid("lifecycle/retention flags require definition")); }
                let decl = m.ir.decls.iter_mut().find(|d| d.name == name).ok_or_else(|| invalid("function symbol not found"))?;
                decl.attrs = attrs;
            }
            m.sync_callee_attributes(&name); Ok(())
        }) }
    }
}
