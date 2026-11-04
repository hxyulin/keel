//! Static data: the bytes and relocations of objects with static storage.

use super::*;

fn write_le(bytes: &mut [u8], v: u128) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (v >> (8 * i)) as u8;
    }
}

/// Data items for `bytes`: runs of zeros as `zero n`, the rest as strings.
fn push_bytes(items: &mut Vec<DataItem>, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        let zeros = bytes[i..].iter().take_while(|&&b| b == 0).count();
        if zeros > 0 && (zeros >= 16 || i + zeros == bytes.len()) {
            items.push(DataItem::Zero(zeros as u32));
            i += zeros;
            continue;
        }
        let mut end = i;
        while end < bytes.len() {
            let run = bytes[end..].iter().take_while(|&&b| b == 0).count();
            if run >= 16 || end + run == bytes.len() && run > 0 {
                break;
            }
            end += run.max(1);
        }
        items.push(DataItem::Bytes(bytes[i..end].to_vec()));
        i = end;
    }
}

impl<'p> Gen<'p> {
    /// A defined object: its bytes, with relocations for addresses.
    pub(super) fn data(&mut self, g: usize, record_references: bool) -> R<DataObject> {
        let global = &self.p.globals[g];
        let types = self.types();
        let nominal = types.size(&global.ty);
        let items = global.init.as_ref().map_or(&[][..], |i| i.items);
        fn flexible_start(defs: &[StructDef<'_>], ty: Ty<'_>) -> Option<u64> {
            let Ty::Struct(id) = ty else { return None };
            let def = defs.get(id)?;
            if def.is_union {
                return None;
            }
            let member = def.members.last()?;
            match member.ty {
                Ty::Array(array) if array.len.is_none() => Some(member.offset),
                ty => flexible_start(defs, ty)?.checked_add(member.offset),
            }
        }
        let flexible = flexible_start(self.p.structs, global.ty);
        let mut extent = nominal;
        for item in items {
            let (offset, width) = match item {
                InitItem::Bytes { offset, bytes } => (*offset, bytes.len() as u64),
                InitItem::Expr { offset, expr, bits } => (
                    *offset,
                    bits.map_or_else(
                        || types.size(&expr.ty),
                        |field| u64::from(field.shift + field.width).div_ceil(8),
                    ),
                ),
            };
            let end = offset
                .checked_add(width)
                .ok_or_else(|| Error::new(Loc::default(), "static initializer extent overflows"))?;
            if end > nominal && !flexible.is_some_and(|start| offset >= start) {
                return Err(Error::new(
                    Loc::default(),
                    "initializer exceeds static object storage",
                ));
            }
            extent = extent.max(end);
        }
        let size = usize::try_from(extent)
            .ok()
            .filter(|&size| size <= u32::MAX as usize)
            .ok_or_else(|| {
                Error::new(
                    Loc::default(),
                    "static object storage exceeds the supported size",
                )
            })?;
        let align = if self.p.c_layout.pointer_bytes() == 4
            && matches!(global.ty, Ty::Array(_))
            && size >= 16
        {
            global.align.max(16)
        } else {
            global.align.max(1)
        } as u32;
        let pointer_bytes = self.p.c_layout.pointer_bytes() as usize;
        let mut bytes = vec![0u8; size];
        let mut relocs: Vec<(usize, String, i64)> = Vec::new();
        for item in items {
            match item {
                InitItem::Bytes { offset, bytes: b } => {
                    let start = *offset as usize;
                    let n = b.len().min(size - start);
                    bytes[start..start + n].copy_from_slice(&b[..n]);
                }
                InitItem::Expr { offset, expr, bits } => {
                    let offset = *offset as usize;
                    // All declarations have merged: only final strong binding
                    // proves a global address nonnull in a static initializer.
                    let value = crate::eval::eval_constant_with_binding_and_layout(
                        expr,
                        true,
                        &|g| self.p.globals[g].weak.is_none(),
                        self.p.c_layout,
                    );
                    let Some(value) = value else {
                        return Err(Error::new(expr.loc, "initializer is not a constant"));
                    };
                    let width = self.types().size(&expr.ty) as usize;
                    match value {
                        Const::Int(v) => match bits {
                            Some(bf) => {
                                // Write the field itself, including a possible ninth byte for
                                // an unaligned 64-bit field. Never touch its declared unit's tail.
                                for bit in 0..bf.width {
                                    let target = bf.shift + bit;
                                    let byte = &mut bytes[offset + (target / 8) as usize];
                                    let mask = 1 << (target % 8);
                                    *byte =
                                        (*byte & !mask) | (((v.bits() >> bit) as u8 & 1) * mask);
                                }
                            }
                            // The value is extended to 128 bits as its type's
                            // signedness says.
                            None => write_le(&mut bytes[offset..offset + width], v.bits()),
                        },
                        Const::Complex(r, i) => {
                            let Ty::Complex(elem) = expr.ty else {
                                unreachable!()
                            };
                            let n = width / 2;
                            for (index, f) in [r, i].into_iter().enumerate() {
                                let bits = if *elem == Ty::Float {
                                    u128::from((f as f32).to_bits())
                                } else {
                                    u128::from(f.to_bits())
                                };
                                let start = offset + index * n;
                                write_le(&mut bytes[start..start + n], bits);
                            }
                        }
                        Const::Float(f) => {
                            let v = if expr.ty == Ty::Float {
                                u128::from((f as f32).to_bits())
                            } else {
                                u128::from(f.to_bits())
                            };
                            write_le(&mut bytes[offset..offset + width], v);
                        }
                        Const::Addr(base, addend) | Const::Reloc(base, addend) => {
                            if bits.is_some() {
                                return Err(Error::new(
                                    expr.loc,
                                    "address relocation in a bit-field initializer is not supported",
                                ));
                            }
                            if width != pointer_bytes
                                || (pointer_bytes == 4 && !matches!(expr.ty, Ty::Ptr(_)))
                            {
                                return Err(Error::new(expr.loc, "address in a narrow integer"));
                            }
                            let symbol = match base {
                                Base::Global(target) => {
                                    if record_references {
                                        self.use_global(target);
                                    }
                                    self.p.globals[target].symbol.to_owned()
                                }
                                Base::Str(s) => {
                                    if record_references {
                                        self.string_symbol(s)
                                    } else {
                                        "placement.validation.string".into()
                                    }
                                }
                            };
                            relocs.push((offset, symbol, addend));
                        }
                    }
                }
            }
        }
        relocs.sort_by_key(|r| r.0);
        let mut items = Vec::new();
        let mut pos = 0;
        for (offset, symbol, addend) in relocs {
            if offset < pos
                || offset
                    .checked_add(pointer_bytes)
                    .is_none_or(|end| end > size)
            {
                return Err(Error::new(
                    Loc::default(),
                    "pointer relocations overlap or exceed object storage",
                ));
            }
            push_bytes(&mut items, &bytes[pos..offset]);
            let Ok(addend) = u32::try_from(addend) else {
                return Err(Error::new(
                    Loc::default(),
                    format!(
                        "`{}`: pointer relocation addend {addend} from `{symbol}` is outside 0..=4294967295",
                        global.name
                    ),
                ));
            };
            items.push(DataItem::Ptr { symbol, addend });
            pos = offset + pointer_bytes;
        }
        push_bytes(&mut items, &bytes[pos..]);
        if items.is_empty() {
            items.push(DataItem::Zero(1));
        }
        Ok(DataObject {
            name: global.symbol.to_owned(),
            thread_local: global.thread_local,
            local: global.internal,
            weak: global.weak.is_some(),
            weak_odr: false,
            hidden: global.hidden(),
            retain: global.retain.is_some(),
            section: global.section,
            init: Some(DataInit {
                constant: global.is_const && !global.thread_local,
                align,
                items,
            }),
        })
    }
}
