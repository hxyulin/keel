//! Checked placement for the explicit owned-memory runtime profile.
use crate::{Diagnostic, Wasm32MemoryOptions, reject};
use keel_ir::{DataItem, DataLayout, Module};
use std::collections::HashMap;
use wasm_encoder::{ConstExpr, DataSection};

/// Library-only fixed owned-memory layout. No imports, heap, growth or C ABI.
/// Placement reserves `[0, data_floor)`; ordinary accesses can still reach it.
/// Static data must finish at or below `stack_bottom`. Stack bounds are inclusive
/// bottom/exclusive top, both aligned to 16, within the configured memory.
/// Any trap invalidates the instance: the host must discard it, because saved
/// software stack pointers are restored only on normal returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wasm32RuntimeOptions {
    pub memory: Wasm32MemoryOptions,
    pub data_floor: u32,
    pub stack_bottom: u32,
    pub stack_top: u32,
}

pub(crate) struct Placement<'a> {
    pub addresses: HashMap<&'a str, u32>,
    sizes: HashMap<&'a str, u64>,
}
impl<'a> Placement<'a> {
    pub fn new(module: &'a Module, options: &Wasm32RuntimeOptions) -> Result<Self, Diagnostic> {
        let memory_bytes = u64::from(options.memory.pages) * 65536;
        if options.data_floor == 0
            || options.data_floor > options.stack_bottom
            || options.stack_bottom > options.stack_top
            || u64::from(options.stack_top) > memory_bytes
            || options.stack_bottom % 16 != 0
            || options.stack_top % 16 != 0
        {
            return Err(reject(
                "invalid runtime data floor or 16-aligned stack bounds",
            ));
        }
        let mut addresses = HashMap::new();
        let mut sizes = HashMap::new();
        let mut cursor = u64::from(options.data_floor);
        for data in &module.data {
            if data.thread_local
                || data.weak
                || data.weak_odr
                || data.retain
                || data.section.is_some()
            {
                return Err(reject(format!(
                    "@{}: unsupported data TLS/linkage/section metadata",
                    data.name
                )));
            }
            let init = data
                .init
                .as_ref()
                .ok_or_else(|| reject(format!("@{}: external data is unsupported", data.name)))?;
            if !init.align.is_power_of_two() || init.align > 4096 {
                return Err(reject("unsupported static data alignment"));
            }
            let size = init
                .checked_size_with_layout(DataLayout::WASM32)
                .ok_or_else(|| reject("static data size overflow"))?;
            if size == 0 {
                return Err(reject("empty static data object"));
            }
            let mask = u64::from(init.align) - 1;
            cursor = cursor
                .checked_add(mask)
                .map(|n| n & !mask)
                .ok_or_else(|| reject("static data alignment overflow"))?;
            let end = cursor
                .checked_add(size)
                .ok_or_else(|| reject("static data end overflow"))?;
            if end > u64::from(options.stack_bottom) {
                return Err(reject("static data overlaps reserved stack allocation"));
            }
            if addresses
                .insert(data.name.as_str(), cursor as u32)
                .is_some()
            {
                return Err(reject("duplicate data symbol"));
            }
            sizes.insert(data.name.as_str(), size);
            cursor = end;
        }
        let placement = Self { addresses, sizes };
        // Reject all unsupported items and fixups before any segments/buffers
        // are materialized, including those in unreferenced data definitions.
        for data in &module.data {
            let init = data.init.as_ref().unwrap();
            let mut offset = 0u64;
            for item in &init.items {
                match item {
                    DataItem::Bytes(_) | DataItem::Zero(_) => (),
                    DataItem::Int { ty, .. }
                        if matches!(
                            ty,
                            keel_ir::Type::I8
                                | keel_ir::Type::I16
                                | keel_ir::Type::I32
                                | keel_ir::Type::I64
                        ) =>
                    {
                        ()
                    }
                    DataItem::Ptr { symbol, addend } => {
                        placement.fixup(symbol, *addend, memory_bytes)?;
                    }
                    _ => {
                        return Err(reject(
                            "unsupported data initializer (including RTTI or noninteger storage)",
                        ));
                    }
                }
                offset = offset
                    .checked_add(item.size_with_layout(DataLayout::WASM32))
                    .ok_or_else(|| reject("static data cursor overflow"))?;
            }
        }
        Ok(placement)
    }
    fn fixup(&self, symbol: &str, addend: u32, memory_bytes: u64) -> Result<u32, Diagnostic> {
        let base = self.addresses.get(symbol).ok_or_else(|| reject(format!("@{symbol}: fixup requires defined data; function pointers and external data are unsupported")))?;
        if u64::from(addend) > self.sizes[symbol] {
            return Err(reject("data pointer addend is past target object"));
        }
        let address = base
            .checked_add(addend)
            .ok_or_else(|| reject("data pointer fixup overflows wasm32"))?;
        if u64::from(address) > memory_bytes {
            return Err(reject("data pointer fixup exceeds memory"));
        }
        Ok(address)
    }
    pub fn segments(&self, module: &Module, memory_bytes: u64) -> DataSection {
        let mut section = DataSection::new();
        for data in &module.data {
            let mut cursor = u64::from(self.addresses[data.name.as_str()]);
            for item in &data.init.as_ref().unwrap().items {
                // Sparse zero/BSS never creates an object-sized temporary.
                let offset = ConstExpr::i32_const(cursor as u32 as i32);
                match item {
                    DataItem::Bytes(bytes) if !bytes.is_empty() => {
                        section.active(0, &offset, bytes.iter().copied());
                    }
                    DataItem::Int { ty, value } => {
                        section.active(
                            0,
                            &offset,
                            value.to_le_bytes()[..ty.bytes().unwrap() as usize]
                                .iter()
                                .copied(),
                        );
                    }
                    DataItem::Ptr { symbol, addend } => {
                        section.active(
                            0,
                            &offset,
                            self.fixup(symbol, *addend, memory_bytes)
                                .unwrap()
                                .to_le_bytes(),
                        );
                    }
                    DataItem::Bytes(_) | DataItem::Zero(_) => (),
                    _ => unreachable!("preflight"),
                }
                cursor += item.size_with_layout(DataLayout::WASM32);
            }
        }
        section
    }
}

pub(crate) struct Frame {
    pub size: u32,
    pub offsets: Vec<u32>,
}
impl Frame {
    pub fn new(function: &keel_ir::Function) -> Result<Self, Diagnostic> {
        let mut offsets = Vec::new();
        let mut cursor = 0u64;
        for (_, slot) in function.stack_slots() {
            if !slot.align.is_power_of_two() || slot.align > 16 {
                return Err(reject(
                    "software stack slot alignment must be a power of two <=16",
                ));
            }
            let mask = u64::from(slot.align) - 1;
            cursor = cursor
                .checked_add(mask)
                .map(|n| n & !mask)
                .ok_or_else(|| reject("stack slot alignment overflow"))?;
            offsets.push(u32::try_from(cursor).map_err(|_| reject("stack frame exceeds wasm32"))?);
            cursor = cursor
                .checked_add(u64::from(slot.size))
                .ok_or_else(|| reject("stack frame size overflow"))?;
        }
        let size = cursor
            .checked_add(15)
            .map(|n| n & !15)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| reject("16-aligned stack frame exceeds wasm32"))?;
        Ok(Self { size, offsets })
    }
}
