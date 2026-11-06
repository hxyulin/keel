//! Places: lvalue addresses, loads and stores, including bit-fields.

use super::*;

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    /// Conservatively retain volatile subobjects during aggregate copies.
    pub(super) fn object_flags(&self, ty: &Ty, sugar: &Sugar) -> MemoryFlags {
        MemoryFlags {
            volatile: self.g.object_volatile(ty, sugar),
            ..MemoryFlags::default()
        }
    }

    pub(super) fn aggregate_place(
        &self,
        addr: Value,
        ty: &Ty,
        flags: MemoryFlags,
        loc: Loc,
    ) -> R<Place> {
        let size = u32::try_from(self.size(ty)).map_err(|_| {
            Error::new(loc, "aggregate access exceeds the IR stack-slot size limit")
        })?;
        Ok(Place::Aggregate {
            addr,
            size,
            align: self.g.types().align(ty) as u32,
            flags,
        })
    }

    /// Where an lvalue lives. `None` for a void-typed dereference.
    pub(super) fn place(&mut self, e: &Expr<'p>) -> R<Option<Place>> {
        if e.ty == Ty::Void {
            return Ok(None);
        }
        if let ExprKind::VectorIndex(base, index) = &e.kind {
            let address = self.addr(base)?;
            let offset = self.scaled(index, self.size(&e.ty))?;
            let address = self.b.ins().ptradd(address, offset);
            return Ok(Some(Place::Mem(
                address,
                e.ty.ir_with_layout(self.g.p.c_layout),
                self.object_flags(&e.ty, &e.sugar),
            )));
        }
        if let ExprKind::Local(id) = e.kind
            && let Storage::Var(var) = self.locals[id]
        {
            return Ok(Some(Place::Var(var)));
        }
        if let ExprKind::Member {
            base,
            offset,
            bits: Some(field),
            ..
        } = &e.kind
        {
            let (base, off) = self.addr_parts(base)?;
            let addr = self.offset_by(base, off.wrapping_add(*offset as i64));
            return Ok(Some(Place::Bits {
                flags: self.object_flags(&e.ty, &e.sugar),
                addr,
                unit: e.ty.ir_with_layout(self.g.p.c_layout),
                field: *field,
                signed: e.ty.is_signed(),
            }));
        }
        let addr = self.addr(e)?;
        let flags = self.object_flags(&e.ty, &e.sugar);
        Ok(Some(if Self::is_aggregate(&e.ty) {
            self.aggregate_place(addr, &e.ty, flags, e.loc)?
        } else {
            if let Some(name) = e.ty.unusable() {
                return Err(Error::new(e.loc, format!("`{name}` is not supported")));
            }
            Place::Mem(addr, e.ty.ir_with_layout(self.g.p.c_layout), flags)
        }))
    }

    pub(super) fn load_place(&mut self, place: &Place) -> Value {
        match *place {
            Place::Var(var) => self.b.use_var(var),
            Place::Mem(addr, ty, flags) => self.b.ins().load_with_flags(ty, addr, flags),
            Place::Aggregate {
                addr,
                size,
                align,
                flags,
            } => {
                if !flags.volatile {
                    return addr;
                }
                // Materialize a volatile aggregate read now, before an
                // assignment, return or by-value ABI copy loses the lvalue.
                // This private byte snapshot is never exposed as an aligned C object.
                let slot = self.b.create_stack_slot(size, align.min(16));
                let copy = self.full_temporary(slot);
                self.copy_bytes_with_flags(
                    copy,
                    addr,
                    u64::from(size),
                    MemoryFlags::default(),
                    flags,
                );
                copy
            }
            Place::Bits {
                flags,
                addr,
                unit,
                field,
                signed,
            } => {
                let bits = i64::from(unit.bytes().unwrap() * 8);
                if field.bytewise {
                    return self.load_packed_bits(addr, unit, field, signed, flags);
                }
                let word = self.b.ins().load_with_flags(unit, addr, flags);
                let left = bits - i64::from(field.shift) - i64::from(field.width);
                // A field in the top bits needs no shift up.
                let up = if left == 0 {
                    word
                } else {
                    let l = self.b.ins().iconst(unit, left);
                    self.b.ins().shl(word, l)
                };
                let r = self.b.ins().iconst(unit, bits - i64::from(field.width));
                if signed {
                    self.b.ins().sshr(up, r)
                } else {
                    self.b.ins().ushr(up, r)
                }
            }
        }
    }

    /// Return the converted value actually stored, including bit-field truncation.
    pub(super) fn store_place(&mut self, place: &Place, v: Value) -> Value {
        let v = match *place {
            Place::Bits {
                unit,
                field,
                signed,
                ..
            } => {
                let extension = unit.bytes().unwrap() * 8 - field.width;
                if extension == 0 {
                    v
                } else if !signed && field.width < 64 {
                    let bits = (u64::MAX >> (64 - field.width)) as i64;
                    let mask = self.b.ins().iconst(unit, bits);
                    self.b.ins().and(v, mask)
                } else {
                    let shift = self.b.ins().iconst(unit, i64::from(extension));
                    let shifted = self.b.ins().shl(v, shift);
                    if signed {
                        self.b.ins().sshr(shifted, shift)
                    } else {
                        self.b.ins().ushr(shifted, shift)
                    }
                }
            }
            _ => v,
        };
        match *place {
            Place::Var(var) => self.b.def_var(var, v),
            Place::Mem(addr, _, flags) => {
                self.b.ins().store_with_flags(v, addr, flags);
            }
            Place::Aggregate {
                addr, size, flags, ..
            } => {
                self.copy_bytes_with_flags(addr, v, u64::from(size), flags, MemoryFlags::default())
            }
            Place::Bits {
                flags,
                addr,
                unit,
                field,
                ..
            } => {
                if field.bytewise {
                    self.store_packed_bits(addr, unit, field, v, flags);
                    return v;
                }
                let mask = (u128::MAX >> (128 - field.width)) << field.shift;
                let word = self.b.ins().load_with_flags(unit, addr, flags);
                let keep = self.b.ins().iconst(unit, !mask as i64);
                let kept = self.b.ins().and(word, keep);
                // A field in the low bits needs no shift into place.
                let moved = if field.shift == 0 {
                    v
                } else {
                    let shift = self.b.ins().iconst(unit, i64::from(field.shift));
                    self.b.ins().shl(v, shift)
                };
                let m = self.b.ins().iconst(unit, mask as i64);
                let new = self.b.ins().and(moved, m);
                let merged = self.b.ins().or(kept, new);
                self.b.ins().store_with_flags(merged, addr, flags);
            }
        }
        v
    }

    /// Assemble only the field's bytes, keeping all shifts below the value width.
    fn load_packed_bits(
        &mut self,
        addr: Value,
        unit: Type,
        field: BitField,
        signed: bool,
        flags: MemoryFlags,
    ) -> Value {
        let mut value = self.b.ins().iconst(unit, 0);
        let mut consumed = 0;
        while consumed < field.width {
            let position = field.shift + consumed;
            let shift = position % 8;
            let width = (8 - shift).min(field.width - consumed);
            let address = self.offset(addr, u64::from(position / 8));
            let mut byte = self.b.ins().load_with_flags(Type::I8, address, flags);
            if shift != 0 {
                let amount = self.b.ins().iconst(Type::I8, i64::from(shift));
                byte = self.b.ins().ushr(byte, amount);
            }
            if width < 8 {
                let mask = self.b.ins().iconst(Type::I8, (1 << width) - 1);
                byte = self.b.ins().and(byte, mask);
            }
            let mut part = if unit == Type::I8 {
                byte
            } else {
                self.b.ins().convert(ConvertOp::Uext, unit, byte)
            };
            if consumed != 0 {
                let amount = self.b.ins().iconst(unit, i64::from(consumed));
                part = self.b.ins().shl(part, amount);
            }
            value = self.b.ins().or(value, part);
            consumed += width;
        }
        let extension = unit.bytes().unwrap() * 8 - field.width;
        if signed && extension != 0 {
            let amount = self.b.ins().iconst(unit, i64::from(extension));
            let shifted = self.b.ins().shl(value, amount);
            value = self.b.ins().sshr(shifted, amount);
        }
        value
    }

    /// Preserve neighboring bits without reading or writing outside the field's span.
    fn store_packed_bits(
        &mut self,
        addr: Value,
        unit: Type,
        field: BitField,
        value: Value,
        flags: MemoryFlags,
    ) {
        let mut consumed = 0;
        while consumed < field.width {
            let position = field.shift + consumed;
            let shift = position % 8;
            let width = (8 - shift).min(field.width - consumed);
            let address = self.offset(addr, u64::from(position / 8));
            let mut part = value;
            if consumed != 0 {
                let amount = self.b.ins().iconst(unit, i64::from(consumed));
                part = self.b.ins().ushr(part, amount);
            }
            if unit != Type::I8 {
                part = self.b.ins().convert(ConvertOp::Trunc, Type::I8, part);
            }
            if shift != 0 {
                let amount = self.b.ins().iconst(Type::I8, i64::from(shift));
                part = self.b.ins().shl(part, amount);
            }
            if width != 8 {
                let mask = ((1i64 << width) - 1) << shift;
                let bits = self.b.ins().iconst(Type::I8, mask);
                part = self.b.ins().and(part, bits);
                let old = self.b.ins().load_with_flags(Type::I8, address, flags);
                let keep = self.b.ins().iconst(Type::I8, !mask);
                let kept = self.b.ins().and(old, keep);
                part = self.b.ins().or(kept, part);
            }
            self.b.ins().store_with_flags(part, address, flags);
            consumed += width;
        }
    }

    pub(super) fn offset(&mut self, base: Value, offset: u64) -> Value {
        self.offset_by(base, offset as i64)
    }

    pub(super) fn offset_by(&mut self, base: Value, offset: i64) -> Value {
        if offset == 0 {
            return base;
        }
        let n = self.b.ins().iconst(Type::I64, offset);
        self.b.ins().ptradd(base, n)
    }

    /// Pointer value `e` as a base and a constant byte offset, so that a
    /// chain of constant steps (`&p[2]`, `(char *)q + 8`, a member of a
    /// member) adds one offset rather than one per step. Every operand
    /// that is not a literal is still evaluated, once and in order.
    pub(super) fn ptr_parts(&mut self, e: &Expr<'p>) -> R<(Value, i64)> {
        match &e.kind {
            ExprKind::PtrAdd(p, n, size)
                if let Some(k) = fold_int(n, FOLD_DEPTH, self.g.p.c_layout) =>
            {
                let (base, off) = self.ptr_parts(p)?;
                Ok((base, off.wrapping_add(k.wrapping_mul(*size as i64))))
            }
            ExprKind::Cast(p) if e.ty.is_ptr() && p.ty.is_ptr() => self.ptr_parts(p),
            ExprKind::Addr(lvalue) => self.addr_parts(lvalue),
            _ => Ok((self.val(e)?, 0)),
        }
    }

    /// `ptr_parts` of the address of lvalue `e`.
    fn addr_parts(&mut self, e: &Expr<'p>) -> R<(Value, i64)> {
        match &e.kind {
            ExprKind::Member { base, offset, .. } => {
                let (base, off) = self.addr_parts(base)?;
                Ok((base, off.wrapping_add(*offset as i64)))
            }
            ExprKind::Deref(p) => self.ptr_parts(p),
            _ => Ok((self.addr(e)?, 0)),
        }
    }

    /// The address of an lvalue (or of an aggregate rvalue's bytes).
    pub(super) fn addr(&mut self, e: &Expr<'p>) -> R<Value> {
        Ok(match &e.kind {
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Slot(slot) => self.b.ins().stack_addr(slot),
                Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
                Storage::Var(var) if e.ty.runtime_sized() => self.b.use_var(var),
                Storage::Var(_) => {
                    return Err(Error::new(e.loc, "internal error: address of a register"));
                }
            },
            ExprKind::Global(g) => {
                self.g.use_global(*g);
                let global = &self.g.p.globals[*g];
                let symbol = if let Ty::Func(fty) = &global.ty {
                    Symbol::Func(self.import_global(*g, fty, None)?)
                } else {
                    Symbol::Data(self.b.import_data(global.symbol))
                };
                self.b.ins().symbol_addr(symbol)
            }
            ExprKind::Str(i) => {
                let name = self.g.string_symbol(*i);
                let data = self.b.import_data(&name);
                self.b.ins().symbol_addr(Symbol::Data(data))
            }
            ExprKind::Deref(_) | ExprKind::Member { .. } => {
                let (base, off) = self.addr_parts(e)?;
                self.offset_by(base, off)
            }
            ExprKind::CompoundLit(id, init) => {
                self.init_local(*id, init)?;
                match self.locals[*id] {
                    Storage::Slot(slot) => self.b.ins().stack_addr(slot),
                    Storage::Addr(v) | Storage::Aligned { addr: v, .. } => v,
                    Storage::Var(_) => unreachable!(),
                }
            }
            // Struct-valued calls, assignments, conditionals: their value
            // is already an address.
            _ => self.val(e)?,
        })
    }
}
