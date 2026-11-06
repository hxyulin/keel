//! Expression lowering: values, conditions, arithmetic and conversions.

use super::*;

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    /// Branches to `t` if `cond` is true (non-zero), else to `f`, without
    /// materializing `&&`, `||` and `!` as values.
    pub(super) fn branch(&mut self, cond: &Expr<'p>, t: Block, f: Block) -> R<()> {
        match &cond.kind {
            ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
                let mid = self.new_block();
                if matches!(cond.kind, ExprKind::LogAnd(..)) {
                    self.branch(a, mid, f)?;
                } else {
                    self.branch(a, t, mid)?;
                }
                self.b.seal_block(mid);
                self.switch_to(mid);
                self.branch(b, t, f)
            }
            ExprKind::Unary(UnOp::Not, a) => self.branch(a, f, t),
            _ if let Some(inner) = same_truth(cond, self.g.p.c_layout) => self.branch(inner, t, f),
            _ if let Some((ne, x)) = zero_test(cond) => {
                if ne {
                    self.branch(x, t, f)
                } else {
                    self.branch(x, f, t)
                }
            }
            // A literal condition (`if (sizeof(long) == 8)`): one constant.
            // Both edges stay: code built into the other block would
            // otherwise be an unreached predecessor of the join, passing
            // it values of its own.
            _ if let Some(v) = fold_int(cond, FOLD_DEPTH, self.g.p.c_layout) => {
                let c = self.b.ins().iconst(Type::I8, i64::from(v != 0));
                self.b.ins().brif(c, t, &[], f, &[]);
                self.terminated = true;
                Ok(())
            }
            _ => {
                let c = self.truth(cond)?;
                self.b.ins().brif(c, t, &[], f, &[]);
                self.terminated = true;
                Ok(())
            }
        }
    }

    /// A value that is non-zero exactly when `e` is true. Comparisons give
    /// their `i8` directly; integers are their own truth value.
    pub(super) fn truth(&mut self, e: &Expr<'p>) -> R<Value> {
        if let ExprKind::Binary(op, a, b) = &e.kind
            && op.is_comparison()
        {
            return self.compare(*op, a, b);
        }
        let v = self.val(e)?;
        Ok(match &e.ty {
            Ty::Int(k) if k.size_with_layout(self.g.p.c_layout) <= 8 => v,
            ty => self.nonzero(v, ty),
        })
    }

    /// `v != 0` as an `i8`.
    pub(super) fn nonzero(&mut self, v: Value, ty: &Ty) -> Value {
        if let Ty::Complex(elem) = ty {
            let (r, i) = self.complex_parts(v, elem);
            let r = self.nonzero(r, elem);
            let i = self.nonzero(i, elem);
            return self.b.ins().or(r, i);
        }
        let z = self.zero(ty.ir_with_layout(self.g.p.c_layout));
        if ty.is_float() {
            self.b.ins().fcmp(FloatCC::Ne, v, z)
        } else {
            self.b.ins().icmp(IntCC::Ne, v, z)
        }
    }

    // ---- expressions ----

    /// The value of a non-void expression; for aggregates, their address.
    pub(super) fn val(&mut self, e: &Expr<'p>) -> R<Value> {
        match self.emit(e)? {
            Some(v) if e.ty.is_complex() => {
                // Complex is a C scalar: capture its value now, even when a
                // later operand overwrites the source object.
                let slot = self.temporary_slot(&e.ty, e.loc)?;
                let addr = self.b.ins().stack_addr(slot);
                self.copy(addr, v, &e.ty);
                Ok(addr)
            }
            Some(v) => Ok(v),
            None => Err(Error::new(e.loc, "a void expression has no value")),
        }
    }

    pub(super) fn is_aggregate(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Struct(_) | Ty::Complex(_) | Ty::Array(..) | Ty::Func(_)
        )
    }

    /// Evaluates `e` for its effects only. Literals, and reads of local
    /// variables that are not volatile, have none.
    pub(super) fn discard(&mut self, e: &Expr<'p>) -> R<()> {
        let mut inner = e;
        while let ExprKind::Cast(a) = &inner.kind
            && convertible(&a.ty, &inner.ty)
        {
            inner = a;
        }
        match &inner.kind {
            ExprKind::Comma(a, b) => {
                self.discard(a)?;
                self.discard(b)
            }
            ExprKind::Local(_) if !self.object_flags(&inner.ty, &inner.sugar).volatile => Ok(()),
            _ if inert(inner, self.g.p.c_layout) => Ok(()),
            _ => self.emit(e).map(drop),
        }
    }

    pub(super) fn emit(&mut self, e: &Expr<'p>) -> R<Option<Value>> {
        // Operations on literals that macros leave outside constant
        // expressions: one constant instead of the operands and the
        // operation.
        if matches!(
            e.kind,
            ExprKind::Unary(..) | ExprKind::Binary(..) | ExprKind::Cast(_)
        ) && let Some(v) = fold_int(e, FOLD_DEPTH, self.g.p.c_layout)
        {
            return Ok(Some(
                self.b
                    .ins()
                    .iconst(e.ty.ir_with_layout(self.g.p.c_layout), v),
            ));
        }
        let v = match &e.kind {
            ExprKind::LabelAddr(n) => {
                let n = self.b.ins().iconst(Type::I64, *n as i64);
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            ExprKind::Int(v) => match &e.ty {
                Ty::Ptr(_) => {
                    let bits = crate::eval::wrap_literal(*v, &e.ty, self.g.p.c_layout);
                    let n = self.b.ins().iconst(Type::I64, bits);
                    self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
                }
                ty => self
                    .b
                    .ins()
                    .iconst(ty.ir_with_layout(self.g.p.c_layout), *v),
            },
            ExprKind::Float(f) => match e.ty {
                Ty::Float => self.b.ins().f32const(*f as f32),
                _ => self.b.ins().f64const(*f),
            },
            ExprKind::Str(_)
            | ExprKind::Global(_)
            | ExprKind::Deref(_)
            | ExprKind::Member { .. }
            | ExprKind::CompoundLit(..) => match self.place(e)? {
                Some(place) => self.load_place(&place),
                None => return Ok(None),
            },
            ExprKind::VectorIndex(base, index) => {
                let vector = self.val(base)?;
                if let Some(lane) = fold_int(index, FOLD_DEPTH, self.g.p.c_layout) {
                    self.b.ins().vextract(vector, lane as u8)
                } else {
                    let size = self.size(&base.ty) as u32;
                    let align = self.g.types().align(&base.ty) as u32;
                    let slot = self.b.create_stack_slot(size, align);
                    let base = self.start_temporary(slot);
                    self.b.ins().store(vector, base);
                    let offset = self.scaled(index, self.size(&e.ty))?;
                    let address = self.b.ins().ptradd(base, offset);
                    let lane = self
                        .b
                        .ins()
                        .load(e.ty.ir_with_layout(self.g.p.c_layout), address);
                    self.end_temporary(base);
                    lane
                }
            }
            ExprKind::Local(id) => match self.locals[*id] {
                Storage::Var(var) => self.b.use_var(var),
                _ => {
                    let place = self.place(e)?.unwrap();
                    self.load_place(&place)
                }
            },
            // `!(a < b)` is `a >= b` for integers and pointers.
            ExprKind::Unary(UnOp::Not, a)
                if let Some((op, x, y)) = int_comparison(a, self.g.p.c_layout) =>
            {
                let c = self.compare(negated(op), x, y)?;
                self.b.ins().uext(Type::I32, c)
            }
            ExprKind::Unary(op, a) if a.ty.is_complex() => {
                let v = self.val(a)?;
                let Ty::Complex(elem) = a.ty else {
                    unreachable!()
                };
                match op {
                    UnOp::Neg => {
                        let (r, i) = self.complex_parts(v, elem);
                        let r = self.b.ins().fneg(r);
                        let i = self.b.ins().fneg(i);
                        self.complex_pair(r, i, elem)
                    }
                    UnOp::Not => {
                        let c = self.nonzero(v, &a.ty);
                        let one = self.b.ins().iconst(Type::I8, 1);
                        let c = self.b.ins().xor(c, one);
                        self.b.ins().uext(Type::I32, c)
                    }
                    _ => return Err(Error::new(e.loc, "unsupported complex unary operation")),
                }
            }
            ExprKind::Unary(op, a) if a.ty.is_vector() => {
                let v = self.val(a)?;
                let Ty::Vector(elem, _) = a.ty else {
                    unreachable!()
                };
                let op = match op {
                    UnOp::Neg if elem.is_float() => keel_ir::UnaryOp::Fneg,
                    UnOp::Neg => keel_ir::UnaryOp::Ineg,
                    UnOp::BitNot => keel_ir::UnaryOp::Bnot,
                    _ => return Err(Error::new(e.loc, "unsupported vector unary operation")),
                };
                self.b.ins().vector_unary(op, v)
            }
            ExprKind::Unary(op, a) => {
                let v = self.val(a)?;
                match op {
                    UnOp::Neg if a.ty.is_float() => self.b.ins().fneg(v),
                    UnOp::Neg => self.b.ins().ineg(v),
                    UnOp::BitNot => self.b.ins().bnot(v),
                    UnOp::Not => {
                        let z = self.zero(a.ty.ir_with_layout(self.g.p.c_layout));
                        let c = if a.ty.is_float() {
                            self.b.ins().fcmp(FloatCC::Eq, v, z)
                        } else {
                            self.b.ins().icmp(IntCC::Eq, v, z)
                        };
                        self.b.ins().uext(Type::I32, c)
                    }
                }
            }
            ExprKind::Binary(op, a, b) if op.is_comparison() && a.ty.is_vector() => {
                self.compare(*op, a, b)?
            }
            ExprKind::Binary(op, a, b) if op.is_comparison() => {
                let c = self.compare(*op, a, b)?;
                self.b.ins().uext(Type::I32, c)
            }
            // `n * sizeof(char)`, `x << 0`, `0 | x` from macros: no
            // operation, and no evaluation skipped but the literal's.
            ExprKind::Binary(op, a, b)
                if small_int(&e.ty, self.g.p.c_layout)
                    && fold_int(b, FOLD_DEPTH, self.g.p.c_layout)
                        .is_some_and(|k| right_identity(*op, k, &e.ty, self.g.p.c_layout)) =>
            {
                self.val(a)?
            }
            ExprKind::Binary(op, a, b)
                if small_int(&e.ty, self.g.p.c_layout)
                    && fold_int(a, FOLD_DEPTH, self.g.p.c_layout)
                        .is_some_and(|k| left_identity(*op, k, &e.ty, self.g.p.c_layout)) =>
            {
                self.val(b)?
            }
            ExprKind::Binary(op, a, b) if e.ty.is_complex() => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                self.complex_arith(*op, &a.ty, &b.ty, x, y, e.loc)?
            }
            ExprKind::Binary(op, a, b) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                self.arith(*op, &e.ty, x, y, e.loc)?
            }
            ExprKind::PtrAdd(p, n, size) => {
                if fold_int(n, FOLD_DEPTH, self.g.p.c_layout).is_some() {
                    let (base, off) = self.ptr_parts(e)?;
                    self.offset_by(base, off)
                } else {
                    let (base, off) = self.ptr_parts(p)?;
                    let base = self.offset_by(base, off);
                    let offset = self.scaled(n, *size)?;
                    self.b.ins().ptradd(base, offset)
                }
            }
            ExprKind::PtrDiff(a, b, size) => {
                let (x, y) = (self.val(a)?, self.val(b)?);
                let x = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, x);
                let y = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, y);
                let d = self.b.ins().isub(x, y);
                // The difference is an exact multiple of the size.
                let difference = if *size <= 1 {
                    d
                } else if size.is_power_of_two() {
                    let s = self.b.ins().iconst(Type::I64, size.trailing_zeros() as i64);
                    self.b.ins().sshr(d, s)
                } else {
                    let s = self.b.ins().iconst(Type::I64, *size as i64);
                    self.b.ins().sdiv(d, s)
                };
                if self.g.p.c_layout.pointer_bytes() == 4 {
                    self.b.ins().trunc(Type::I32, difference)
                } else {
                    difference
                }
            }
            ExprKind::LogAnd(..) | ExprKind::LogOr(..) => {
                let var = self.b.declare_var(Type::I32);
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch(e, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                for (block, value) in [(t, 1), (f, 0)] {
                    self.switch_to(block);
                    let v = self.b.ins().iconst(Type::I32, value);
                    self.b.def_var(var, v);
                    self.jump_to(end);
                }
                self.b.seal_block(end);
                self.switch_to(end);
                self.b.use_var(var)
            }
            ExprKind::Assign(lhs, rhs) => {
                let place = self.place(lhs)?.unwrap();
                let v = self.val(rhs)?;
                // Normalize bit-fields without rereading volatile storage.
                self.store_place(&place, v)
            }
            ExprKind::OpAssign { op, lhs, rhs } if lhs.ty.is_complex() || rhs.ty.is_complex() => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let calc = crate::expr::usual_with_layout(&lhs.ty, &rhs.ty, self.g.p.c_layout);
                let lt = match calc {
                    Ty::Complex(elem) if !lhs.ty.is_complex() => *elem,
                    _ => calc,
                };
                let old = self.convert(old, &lhs.ty, &lt, e.loc)?;
                // Freeze the old pair before evaluating a side-effecting rhs.
                let old = if let Ty::Complex(elem) = lt {
                    let (r, i) = self.complex_parts(old, elem);
                    self.complex_pair(r, i, elem)
                } else {
                    old
                };
                let r = self.val(rhs)?;
                let new = self.complex_arith(*op, &lt, &rhs.ty, old, r, e.loc)?;
                let new = self.convert(new, &calc, &lhs.ty, e.loc)?;
                self.store_place(&place, new)
            }
            ExprKind::OpAssign { op, lhs, rhs } => {
                let ty = &rhs.ty;
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let old = self.convert(old, &lhs.ty, ty, e.loc)?;
                let r = self.val(rhs)?;
                let new = self.arith(*op, ty, old, r, e.loc)?;
                let new = self.convert(new, ty, &lhs.ty, e.loc)?;
                self.store_place(&place, new)
            }
            ExprKind::PtrOpAssign { lhs, rhs, size } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let offset = self.scaled(rhs, *size)?;
                let new = self.b.ins().ptradd(old, offset);
                self.store_place(&place, new)
            }
            ExprKind::IncDec { lhs, inc, prefix } => {
                let place = self.place(lhs)?.unwrap();
                let old = self.load_place(&place);
                let new = match &lhs.ty {
                    Ty::Ptr(_) => {
                        let elem = lhs.ty.pointee().unwrap();
                        let size = if elem.runtime_sized() {
                            self.runtime_size(elem)?
                        } else {
                            let size = self.size(elem).max(1);
                            self.b.ins().iconst(Type::I64, size as i64)
                        };
                        let d = if *inc { size } else { self.b.ins().ineg(size) };
                        self.b.ins().ptradd(old, d)
                    }
                    Ty::Float | Ty::Double => {
                        let one = if lhs.ty == Ty::Float {
                            self.b.ins().f32const(1.0)
                        } else {
                            self.b.ins().f64const(1.0)
                        };
                        if *inc {
                            self.b.ins().fadd(old, one)
                        } else {
                            self.b.ins().fsub(old, one)
                        }
                    }
                    Ty::Int(IntKind::Bool) => {
                        if *inc {
                            self.b.ins().iconst(Type::I8, 1)
                        } else {
                            let one = self.b.ins().iconst(Type::I8, 1);
                            self.b.ins().xor(old, one)
                        }
                    }
                    ty => {
                        let one = self.b.ins().iconst(ty.ir_with_layout(self.g.p.c_layout), 1);
                        if *inc {
                            self.b.ins().iadd(old, one)
                        } else {
                            self.b.ins().isub(old, one)
                        }
                    }
                };
                let stored = self.store_place(&place, new);
                if *prefix { stored } else { old }
            }
            // `sizeof(x) == sizeof(float) ? f(x) : g(x)`, as <math.h>
            // macros dispatch on the type: only the selected arm. The other
            // is never evaluated, and has no statement a jump could reach.
            ExprKind::Cond(c, a, b)
                if let Some(v) = fold_int(c, FOLD_DEPTH, self.g.p.c_layout)
                    && no_statements(if v != 0 { b } else { a }, 64) =>
            {
                let arm = if v != 0 { a } else { b };
                if e.ty == Ty::Void {
                    self.emit(arm)?;
                    return Ok(None);
                }
                self.val(arm)?
            }
            ExprKind::Cond(c, a, b) => {
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch(c, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                if e.ty == Ty::Void {
                    for (block, arm) in [(t, a), (f, b)] {
                        self.switch_to(block);
                        self.emit(arm)?;
                        self.jump_to(end);
                    }
                    self.b.seal_block(end);
                    self.switch_to(end);
                    return Ok(None);
                }
                let ir = if Self::is_aggregate(&e.ty) {
                    Type::Ptr
                } else {
                    e.ty.ir_with_layout(self.g.p.c_layout)
                };
                let var = self.b.declare_var(ir);
                for (block, arm) in [(t, a), (f, b)] {
                    self.switch_to(block);
                    let v = self.val(arm)?;
                    self.b.def_var(var, v);
                    self.jump_to(end);
                }
                self.b.seal_block(end);
                self.switch_to(end);
                self.b.use_var(var)
            }
            ExprKind::Comma(a, b) => {
                self.discard(a)?;
                return self.emit(b);
            }
            ExprKind::StmtExpr {
                scope,
                stmts,
                value,
            } => {
                let depth = self.exit_actions.len();
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
                let v = match value {
                    Some(v) => {
                        self.ensure_block();
                        let value = self.emit(v)?;
                        // Aggregate values are represented by addresses. Freeze
                        // the bytes before callbacks change them or VLA restores
                        // release their storage.
                        if self.exit_actions.len() > depth
                            && Self::is_aggregate(&v.ty)
                            && let Some(value) = value
                        {
                            let layout =
                                self.aggregate_place(value, &v.ty, MemoryFlags::default(), v.loc)?;
                            let Place::Aggregate { size, align, .. } = layout else {
                                unreachable!()
                            };
                            let slot = self.b.create_stack_slot(size.max(1), align.min(16));
                            let copy = self.b.ins().stack_addr(slot);
                            self.copy(copy, value, &v.ty);
                            Some(copy)
                        } else {
                            value
                        }
                    }
                    None => None,
                };
                self.set_scope(outer);
                self.leave_scope(depth)?;
                return Ok(v);
            }
            ExprKind::Call(f, args) => return self.call(f, args, e),
            ExprKind::UnionCast(inner, bits) => {
                let size = u32::try_from(self.size(&e.ty)).map_err(|_| {
                    Error::new(e.loc, "union cast exceeds the IR stack-slot size limit")
                })?;
                let align = self.g.types().align(&e.ty) as u32;
                let slot = self.b.create_stack_slot(size.max(1), align.min(16));
                let addr = self.b.ins().stack_addr(slot);
                self.zero_bytes_with_flags(addr, u64::from(size), MemoryFlags::default());
                let value = self.val(inner)?;
                let place = if let Some(field) = bits {
                    Place::Bits {
                        flags: MemoryFlags::default(),
                        addr,
                        unit: inner.ty.ir_with_layout(self.g.p.c_layout),
                        field: *field,
                        signed: inner.ty.is_signed(),
                    }
                } else if Self::is_aggregate(&inner.ty) {
                    self.aggregate_place(addr, &inner.ty, MemoryFlags::default(), inner.loc)?
                } else {
                    Place::Mem(
                        addr,
                        inner.ty.ir_with_layout(self.g.p.c_layout),
                        MemoryFlags::default(),
                    )
                };
                self.store_place(&place, value);
                addr
            }
            ExprKind::Cast(inner) => {
                if e.ty == Ty::Void {
                    self.discard(inner)?;
                    return Ok(None);
                }
                // Null pointers and other integer constants made pointers:
                // normalize to the compilation model before the i64 IR conversion.
                if e.ty.is_ptr()
                    && let Some(n) = fold_int(inner, FOLD_DEPTH, self.g.p.c_layout)
                {
                    let bits = crate::eval::wrap_literal(n, &e.ty, self.g.p.c_layout);
                    let n = self.b.ins().iconst(Type::I64, bits);
                    let p = self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n);
                    return Ok(Some(p));
                }
                // `x > 0` with a floating `x`: the converted constant,
                // rounded once to nearest as the conversion rounds.
                if matches!(e.ty, Ty::Float | Ty::Double)
                    && let Some(n) = fold_int(inner, FOLD_DEPTH, self.g.p.c_layout)
                {
                    let signed = inner.ty.is_signed();
                    return Ok(Some(match (&e.ty, signed) {
                        (Ty::Float, true) => self.b.ins().f32const(n as f32),
                        (Ty::Float, false) => self.b.ins().f32const(n as u64 as f32),
                        (_, true) => self.b.ins().f64const(n as f64),
                        (_, false) => self.b.ins().f64const(n as u64 as f64),
                    }));
                }
                let v = self.emit(inner)?;
                let v = v.ok_or_else(|| Error::new(e.loc, "a void expression has no value"))?;
                self.convert(v, &inner.ty, &e.ty, e.loc)?
            }
            ExprKind::Addr(inner) => self.addr(inner)?,
            ExprKind::VaStart(ap) => {
                let addr = self.val(ap)?;
                self.b.ins().intrinsic(Intrinsic::VaStart, &[addr]);
                return Ok(None);
            }
            ExprKind::VaArg(ap) => {
                if e.ty.is_pair_or_struct() {
                    return Err(Error::new(e.loc, "va_arg of an aggregate is not supported"));
                }
                let addr = self.val(ap)?;
                match &e.ty {
                    Ty::Vector(..) => {
                        return Err(Error::new(e.loc, "va_arg of a vector is not supported"));
                    }
                    Ty::Int(k) if k.size_with_layout(self.g.p.c_layout) < 4 => {
                        let v = self.b.ins().va_arg(Type::I32, addr);
                        self.b
                            .ins()
                            .trunc(e.ty.ir_with_layout(self.g.p.c_layout), v)
                    }
                    Ty::Int(k)
                        if k.size_with_layout(self.g.p.c_layout) == 16
                            && self.g.p.target != keel_ir::Target::Aarch64LinuxGnu =>
                    {
                        return Err(Error::new(e.loc, "`va_arg` of `__int128`"));
                    }
                    Ty::Float => {
                        let v = self.b.ins().va_arg(Type::F64, addr);
                        self.b.ins().convert(ConvertOp::Fptrunc, Type::F32, v)
                    }
                    ty => self
                        .b
                        .ins()
                        .va_arg(ty.ir_with_layout(self.g.p.c_layout), addr),
                }
            }
            ExprKind::Builtin(op, args) => return self.builtin(*op, args, e),
        };
        Ok(Some(v))
    }

    /// `n * size` as an `i64`, for pointer arithmetic.
    pub(super) fn scaled(&mut self, n: &Expr<'p>, size: u64) -> R<Value> {
        if let Some(k) = fold_int(n, FOLD_DEPTH, self.g.p.c_layout) {
            return Ok(self.b.ins().iconst(Type::I64, k.wrapping_mul(size as i64)));
        }
        let v = self.val(n)?;
        let v = if self.b.function().value_type(v) != Type::I64 {
            if n.ty.is_signed() {
                self.b.ins().sext(Type::I64, v)
            } else {
                self.b.ins().uext(Type::I64, v)
            }
        } else {
            v
        };
        if size == 1 {
            return Ok(v);
        }
        let s = self.b.ins().iconst(Type::I64, size as i64);
        Ok(self.b.ins().imul(v, s))
    }

    fn compare(&mut self, op: BinOp, a: &Expr<'p>, b: &Expr<'p>) -> R<Value> {
        // `(x < y) != 0` (as `__builtin_expect` and `!!` spell it) is
        // `x < y`, and `(x < y) == 0` is `x >= y`.
        if let Some(ne) = zero_test_op(op, a, b)
            && let Some((inner, x, y)) = int_comparison(a, self.g.p.c_layout)
        {
            return self.compare(if ne { inner } else { negated(inner) }, x, y);
        }
        let (x, y) = (self.val(a)?, self.val(b)?);
        let ty = &a.ty;
        if let Ty::Complex(elem) = ty {
            let (xr, xi) = self.complex_parts(x, elem);
            let (yr, yi) = self.complex_parts(y, elem);
            let cc = if op == BinOp::Eq {
                FloatCC::Eq
            } else {
                FloatCC::Ne
            };
            let r = self.b.ins().fcmp(cc, xr, yr);
            let i = self.b.ins().fcmp(cc, xi, yi);
            return Ok(if op == BinOp::Eq {
                self.b.ins().and(r, i)
            } else {
                self.b.ins().or(r, i)
            });
        }
        let elem = match ty {
            Ty::Vector(elem, _) => *elem,
            _ => ty,
        };
        if elem.is_float() {
            let cc = match op {
                BinOp::Eq => FloatCC::Eq,
                BinOp::Ne => FloatCC::Ne,
                BinOp::Lt => FloatCC::Lt,
                BinOp::Le => FloatCC::Le,
                BinOp::Gt => FloatCC::Gt,
                _ => FloatCC::Ge,
            };
            return Ok(if ty.is_vector() {
                self.b.ins().vfcmp(cc, x, y)
            } else {
                self.b.ins().fcmp(cc, x, y)
            });
        }
        let signed = elem.is_signed();
        let cc = match op {
            BinOp::Eq => IntCC::Eq,
            BinOp::Ne => IntCC::Ne,
            BinOp::Lt if signed => IntCC::Slt,
            BinOp::Le if signed => IntCC::Sle,
            BinOp::Gt if signed => IntCC::Sgt,
            BinOp::Ge if signed => IntCC::Sge,
            BinOp::Lt => IntCC::Ult,
            BinOp::Le => IntCC::Ule,
            BinOp::Gt => IntCC::Ugt,
            _ => IntCC::Uge,
        };
        Ok(if ty.is_vector() {
            self.b.ins().vicmp(cc, x, y)
        } else {
            self.b.ins().icmp(cc, x, y)
        })
    }

    pub(super) fn arith(&mut self, op: BinOp, ty: &Ty, x: Value, y: Value, loc: Loc) -> R<Value> {
        let ins = self.b.ins();
        if let Ty::Vector(elem, _) = ty {
            use keel_ir::VectorBinaryOp as V;
            let vector_op = match op {
                BinOp::Add if elem.is_float() => V::Fadd,
                BinOp::Sub if elem.is_float() => V::Fsub,
                BinOp::Mul if elem.is_float() => V::Fmul,
                BinOp::Div if elem.is_float() => V::Fdiv,
                BinOp::Add => V::Iadd,
                BinOp::Sub => V::Isub,
                BinOp::Mul => V::Imul,
                BinOp::And => V::And,
                BinOp::Or => V::Or,
                BinOp::Xor => V::Xor,
                BinOp::Shl => V::Shl,
                BinOp::Shr if elem.is_signed() => V::Sshr,
                BinOp::Shr => V::Ushr,
                _ => return Err(Error::new(loc, "unsupported vector arithmetic")),
            };
            return Ok(ins.vector_binary(vector_op, x, y));
        }
        if ty.is_float() {
            return Ok(match op {
                BinOp::Add => ins.fadd(x, y),
                BinOp::Sub => ins.fsub(x, y),
                BinOp::Mul => ins.fmul(x, y),
                BinOp::Div => ins.fdiv(x, y),
                _ => return Err(Error::new(loc, "invalid float operation")),
            });
        }
        let signed = ty.is_signed();
        Ok(match op {
            BinOp::Add => ins.iadd(x, y),
            BinOp::Sub => ins.isub(x, y),
            BinOp::Mul => ins.imul(x, y),
            BinOp::Div if signed => ins.sdiv(x, y),
            BinOp::Div => ins.udiv(x, y),
            BinOp::Rem if signed => ins.srem(x, y),
            BinOp::Rem => ins.urem(x, y),
            BinOp::Shl => ins.shl(x, y),
            BinOp::Shr if signed => ins.sshr(x, y),
            BinOp::Shr => ins.ushr(x, y),
            BinOp::And => ins.and(x, y),
            BinOp::Or => ins.or(x, y),
            BinOp::Xor => ins.xor(x, y),
            _ => unreachable!("comparison in arith"),
        })
    }

    /// Address-backed pairs share aggregate ABI machinery, but retain a
    /// distinct C type.
    pub(super) fn complex_pair(&mut self, r: Value, i: Value, elem: &Ty) -> Value {
        let size = self.size(elem) as u32;
        let slot = self.b.create_stack_slot(2 * size, size);
        let addr = self.b.ins().stack_addr(slot);
        self.b.ins().store(r, addr);
        let imag = self.offset(addr, u64::from(size));
        self.b.ins().store(i, imag);
        addr
    }

    fn complex_parts(&mut self, addr: Value, elem: &Ty) -> (Value, Value) {
        let r = self
            .b
            .ins()
            .load(elem.ir_with_layout(self.g.p.c_layout), addr);
        let imag = self.offset(addr, self.size(elem));
        let i = self
            .b
            .ins()
            .load(elem.ir_with_layout(self.g.p.c_layout), imag);
        (r, i)
    }

    fn complex_arith(
        &mut self,
        op: BinOp,
        lt: &Ty,
        rt: &Ty,
        x: Value,
        y: Value,
        loc: Loc,
    ) -> R<Value> {
        let ty = crate::expr::usual_with_layout(lt, rt, self.g.p.c_layout);
        let Ty::Complex(elem) = ty else {
            unreachable!()
        };
        let (xr, xi) = if lt.is_complex() {
            let (r, i) = self.complex_parts(x, elem);
            (r, Some(i))
        } else {
            (x, None)
        };
        let (yr, yi) = if rt.is_complex() {
            let (r, i) = self.complex_parts(y, elem);
            (r, Some(i))
        } else {
            (y, None)
        };
        if matches!(op, BinOp::Mul | BinOp::Div) {
            // Real multiplication and division by a real use scalar operations,
            // avoiding spurious 0*infinity and preserving component zero signs.
            if let (Some(i), None) = (xi, yi) {
                let r = self.arith(op, elem, xr, yr, loc)?;
                let i = self.arith(op, elem, i, yr, loc)?;
                return Ok(self.complex_pair(r, i, elem));
            }
            if op == BinOp::Mul
                && let (None, Some(i)) = (xi, yi)
            {
                let r = self.arith(op, elem, xr, yr, loc)?;
                let i = self.arith(op, elem, xr, i, loc)?;
                return Ok(self.complex_pair(r, i, elem));
            }
            let zero = self.zero(elem.ir_with_layout(self.g.p.c_layout));
            return self.complex_runtime(
                op,
                elem,
                [xr, xi.unwrap_or(zero), yr, yi.unwrap_or(zero)],
                loc,
            );
        }
        let r = self.arith(op, elem, xr, yr, loc)?;
        let i = match (xi, yi) {
            (Some(x), Some(y)) => self.arith(op, elem, x, y, loc)?,
            (Some(x), None) => x,
            (None, Some(y)) if op == BinOp::Sub => self.b.ins().fneg(y),
            (None, Some(y)) => y,
            _ => unreachable!(),
        };
        Ok(self.complex_pair(r, i, elem))
    }

    /// compiler-rt/libgcc ABI: four real arguments and a complex result. The
    /// existing Sret layout lowers to the platform FP return registers, not a
    /// foreign struct ABI. Linking requires the target compiler builtins runtime
    /// (Darwin also uses libSystem; see the complex implementation contract).
    fn complex_runtime(&mut self, op: BinOp, elem: &Ty, args: [Value; 4], loc: Loc) -> R<Value> {
        let name = match (op, elem) {
            (BinOp::Mul, Ty::Float) => "__mulsc3",
            (BinOp::Mul, _) => "__muldc3",
            (BinOp::Div, Ty::Float) => "__divsc3",
            (BinOp::Div, _) => "__divdc3",
            _ => unreachable!(),
        };
        let ty = Ty::Complex(elem);
        // These reserved runtime names must never acquire an incompatible
        // source-selected ABI or weak/internal binding.
        for global in self.g.p.globals {
            if global.name == name || global.symbol == name {
                return Err(Error::new(
                    loc,
                    "complex runtime symbol is reserved for the compiler builtins ABI",
                ));
            }
        }
        let mut sig = Signature::new(&[], &[]);
        sig.params.push(AbiParam {
            ty: Type::Ptr,
            ext: ArgExt::None,
            purpose: Purpose::Sret(
                self.g
                    .aggregate_layout(&ty)
                    .map_err(|m| Error::new(loc, m))?,
            ),
        });
        sig.params
            .extend((0..4).map(|_| AbiParam::new(elem.ir_with_layout(self.g.p.c_layout))));
        if !self.g.module.decls.iter().any(|d| d.name == name) {
            self.g.module.decls.push(FuncDecl {
                name: name.to_owned(),
                attrs: Default::default(),
                sig: sig.clone(),
            });
        }
        let slot = self.temporary_slot(&ty, loc)?;
        let result = self.b.ins().stack_addr(slot);
        let callee = self.b.import_function(name, sig);
        self.b
            .ins()
            .call(callee, &[result, args[0], args[1], args[2], args[3]]);
        Ok(result)
    }

    /// Converts `v` from C type `from` to `to`.
    pub(super) fn convert(&mut self, v: Value, from: &Ty, to: &Ty, loc: Loc) -> R<Value> {
        if from == to || (from.is_ptr() && to.is_ptr()) || to.is_struct() {
            return Ok(v);
        }
        if let Some(name) = from.unusable().or(to.unusable()) {
            return Err(Error::new(loc, format!("`{name}` is not supported")));
        }
        if *to == Ty::Int(IntKind::Bool) {
            let c = self.nonzero(v, from);
            return Ok(c);
        }
        if let Ty::Complex(elem) = to {
            let (r, i) = if let Ty::Complex(src) = from {
                let (r, i) = self.complex_parts(v, src);
                (
                    self.convert(r, src, elem, loc)?,
                    self.convert(i, src, elem, loc)?,
                )
            } else {
                let r = self.convert(v, from, elem, loc)?;
                (r, self.zero(elem.ir_with_layout(self.g.p.c_layout)))
            };
            return Ok(self.complex_pair(r, i, elem));
        }
        if let Ty::Complex(elem) = from {
            let (r, _) = self.complex_parts(v, elem);
            return self.convert(r, elem, to, loc);
        }
        let ins = self.b.ins();
        let (fi, ti) = (
            from.ir_with_layout(self.g.p.c_layout),
            to.ir_with_layout(self.g.p.c_layout),
        );
        if from.is_vector() || to.is_vector() {
            return Ok(if fi == ti {
                v
            } else {
                ins.convert(ConvertOp::Bitcast, ti, v)
            });
        }
        Ok(match (from, to) {
            (Ty::Int(_), Ty::Int(_)) => {
                let (fs, ts) = (fi.bytes().unwrap(), ti.bytes().unwrap());
                if ts > fs {
                    if from.is_signed() {
                        ins.sext(ti, v)
                    } else {
                        ins.uext(ti, v)
                    }
                } else if ts < fs {
                    ins.trunc(ti, v)
                } else {
                    v
                }
            }
            (Ty::Int(k), _) if to.is_float() => {
                if k.size_with_layout(self.g.p.c_layout) == 16 {
                    return Err(Error::new(loc, "`__int128` to float is not supported"));
                }
                let op = if k.signed() {
                    ConvertOp::Sitofp
                } else {
                    ConvertOp::Uitofp
                };
                ins.convert(op, ti, v)
            }
            (_, Ty::Int(k)) if from.is_float() => {
                if k.size_with_layout(self.g.p.c_layout) == 16 {
                    return Err(Error::new(loc, "float to `__int128` is not supported"));
                }
                let op = if k.signed() {
                    ConvertOp::Fptosi
                } else {
                    ConvertOp::Fptoui
                };
                let wide = if k.size_with_layout(self.g.p.c_layout) == 8 {
                    Type::I64
                } else {
                    Type::I32
                };
                let r = ins.convert(op, wide, v);
                if wide != ti {
                    self.b.ins().trunc(ti, r)
                } else {
                    r
                }
            }
            (Ty::Float, Ty::Double) => ins.convert(ConvertOp::Fpext, ti, v),
            (Ty::Double, Ty::Float) => ins.convert(ConvertOp::Fptrunc, ti, v),
            (Ty::Ptr(_), Ty::Int(_)) => {
                let n = ins.convert(ConvertOp::PtrToInt, Type::I64, v);
                if ti == Type::I64 {
                    n
                } else if ti == Type::I128 {
                    self.b.ins().uext(ti, n)
                } else {
                    self.b.ins().trunc(ti, n)
                }
            }
            (Ty::Int(k), Ty::Ptr(_)) => {
                let n = match k.size_with_layout(self.g.p.c_layout) {
                    8 => v,
                    16 => ins.trunc(Type::I64, v),
                    _ if k.signed() => ins.sext(Type::I64, v),
                    _ => ins.uext(Type::I64, v),
                };
                let n = if self.g.p.c_layout.pointer_bytes() == 4 {
                    let low = self.b.ins().trunc(Type::I32, n);
                    self.b.ins().uext(Type::I64, low)
                } else {
                    n
                };
                self.b.ins().convert(ConvertOp::IntToPtr, Type::Ptr, n)
            }
            _ => return Err(Error::new(loc, "unsupported conversion")),
        })
    }

    // ---- places ----
}
