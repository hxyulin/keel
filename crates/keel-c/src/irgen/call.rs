//! Calls, including library builtins, overflow builtins and checked memory builtins.

use super::*;

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    /// The signature to call global function `g` with.
    /// Function `g` (of type `fty`), imported into this function under its
    /// symbol. Once imported, later imports find it by name, so a fixed
    /// signature need not be built again.
    pub(super) fn import_global(
        &mut self,
        g: usize,
        fty: &FuncTy,
        args: Option<&[Ty]>,
    ) -> R<keel_ir::FuncRef> {
        let fixed = fty.has_prototype() || self.g.p.globals[g].func.is_some();
        if fixed && let Some(&fref) = self.callees.get(&g) {
            return Ok(fref);
        }
        let sig = self.callee_signature(g, fty, args)?;
        let fref = self
            .b
            .import_function(self.g.p.globals[g].symbol, sig.clone());
        // A preceding address-of may have imported an unspecified empty
        // signature. The first unprototyped call supplies its promoted ABI.
        if !fixed {
            self.b.function_mut().callee_mut(fref).sig = sig;
        }
        if fixed {
            self.callees.insert(g, fref);
        }
        Ok(fref)
    }

    fn callee_signature(&mut self, g: usize, fty: &FuncTy, args: Option<&[Ty]>) -> R<Signature> {
        let global = &self.g.p.globals[g];
        let final_ty = global.ty.func().unwrap();
        if global.func.is_some() || final_ty.has_prototype() {
            return self.g.signature(final_ty, g, &[]);
        }
        if fty.has_prototype() {
            return self.g.signature(fty, g, &[]);
        }
        if let Some(sig) = self.g.unprototyped.get(&g) {
            return Ok(sig.clone());
        }
        let sig = self.g.signature(fty, g, args.unwrap_or(&[]))?;
        // An actual zero-argument call fixes the signature too. Address-only
        // imports do not, and are completed after all bodies are lowered.
        if args.is_some() {
            self.g.unprototyped.insert(g, sig.clone());
        }
        Ok(sig)
    }

    /// Proof used only to defer ordinary aggregate argument copying. Any
    /// unknown effect, volatile access, call or initialization is a barrier.
    fn may_change_memory(&self, e: &Expr<'p>) -> bool {
        if self.object_flags(&e.ty, &e.sugar).volatile {
            return true;
        }
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::LabelAddr(_)
            | ExprKind::Local(_)
            | ExprKind::Global(_) => false,
            ExprKind::Unary(_, a)
            | ExprKind::Cast(a)
            | ExprKind::Deref(a)
            | ExprKind::Addr(a)
            | ExprKind::Member { base: a, .. } => self.may_change_memory(a),
            ExprKind::VectorIndex(a, b)
            | ExprKind::Binary(_, a, b)
            | ExprKind::PtrAdd(a, b, _)
            | ExprKind::PtrDiff(a, b, _)
            | ExprKind::LogAnd(a, b)
            | ExprKind::LogOr(a, b)
            | ExprKind::Comma(a, b) => self.may_change_memory(a) || self.may_change_memory(b),
            ExprKind::Cond(a, b, c) => {
                self.may_change_memory(a) || self.may_change_memory(b) || self.may_change_memory(c)
            }
            _ => true,
        }
    }

    pub(super) fn call(
        &mut self,
        f: &Expr<'p>,
        args: &[Expr<'p>],
        e: &Expr<'p>,
    ) -> R<Option<Value>> {
        let fty = f.ty.pointee().and_then(Ty::func).unwrap();
        // The complete translation unit supplies symbol identity and all
        // declarations, including definitions after this call's source position.
        if self.g.p.target != keel_ir::Target::Wasm32UnknownUnknown
            && let ExprKind::Addr(inner) = f.kind
            && let ExprKind::Global(index) = inner.kind
        {
            let global = &self.g.p.globals[index];
            if let Some(operation) = crate::LibraryBuiltin::named(global.name)
                && self.g.library_builtins.allows(operation)
                && self.g.library_builtin_symbols[index]
                && global.library_builtin_compatible
                && operation.prototype(&Ty::Func(fty))
                && global.name == global.symbol
                && !global.defined
                && !global.internal
                && global.weak.is_none()
                && !global.returns_twice
                && !global.incompatible_return
                && (operation != crate::LibraryBuiltin::Sqrtl
                    || self.g.p.target == keel_ir::Target::Aarch64AppleDarwin)
            {
                return match operation.intrinsic() {
                    Some(id) => self.builtin(Builtin::Memory(id), args, e),
                    None => self.library_sqrt(index, fty, &args[0], e.loc).map(Some),
                };
            }
        }
        let mapping = self.g.abi_mapping(fty, usize::MAX, &[])?;
        self.set_loc(e.loc);
        let mut vals = Vec::new();
        let ret_slot = if fty.ret.is_pair_or_struct() {
            let slot = self.temporary_slot(&fty.ret, e.loc)?;
            if matches!(mapping.result, ResultMapping::Indirect) {
                vals.push(self.b.ins().stack_addr(slot));
            }
            Some(slot)
        } else {
            None
        };
        // The ABI consumes aggregate addresses at the call instruction.
        // Freeze an argument's bytes now when later evaluation could change
        // them. With no intervening effects, immediate ABI copying suffices.
        let mut snapshots = Vec::new();
        if args.iter().any(|arg| arg.ty.is_pair_or_struct()) {
            snapshots.resize(args.len(), false);
            let mut later_effect = self.may_change_memory(f);
            for (index, arg) in args.iter().enumerate().rev() {
                snapshots[index] = later_effect;
                later_effect |= self.may_change_memory(arg);
            }
        }
        for (index, arg) in args.iter().enumerate() {
            if !fty.has_prototype() && arg.ty.is_pair_or_struct() {
                return Err(Error::new(arg.loc, "struct argument without a prototype"));
            }
            if index >= fty.params.len()
                && (arg.ty.is_pair_or_struct()
                    || arg
                        .ty
                        .int_kind()
                        .is_some_and(|k| k.size_with_layout(self.g.p.c_layout) == 16)
                        && self.g.p.target != keel_ir::Target::Aarch64LinuxGnu)
            {
                return Err(Error::new(
                    arg.loc,
                    "this variadic argument type is not supported",
                ));
            }
            let mut value = self.val(arg)?;
            if matches!(mapping.params.get(index), Some(ParamMapping::Omitted)) {
                continue;
            }
            if arg.ty.is_pair_or_struct() && snapshots[index] {
                let layout =
                    self.aggregate_place(value, &arg.ty, MemoryFlags::default(), arg.loc)?;
                let Place::Aggregate { size, align, .. } = layout else {
                    unreachable!()
                };
                let slot = self.b.create_stack_slot(size.max(1), align);
                let copy = self.b.ins().stack_addr(slot);
                self.copy(copy, value, &arg.ty);
                value = copy;
            }
            vals.push(value);
        }
        // Argument types decide the signature only without a prototype.
        let extra: Vec<Ty> = if fty.has_prototype() {
            Vec::new()
        } else {
            args.iter().map(|a| a.ty).collect()
        };
        self.set_loc(e.loc);
        let direct = match &f.kind {
            ExprKind::Addr(inner) => match inner.kind {
                ExprKind::Global(g) if matches!(inner.ty, Ty::Func(_)) => Some(g),
                _ => None,
            },
            _ => None,
        };
        let inst = match direct {
            Some(g) => {
                let global = &self.g.p.globals[g];
                if self.has_cleanup() && crate::parse::nonlocal_exit(global) {
                    return Err(Error::new(
                        e.loc,
                        "nonlocal exit/returns-twice call in a function containing cleanup locals is not supported yet",
                    ));
                }
                self.g.use_global(g);
                let fref = self.import_global(g, fty, Some(&extra))?;
                let function = self.b.function();
                let sig = &function.callee(fref).sig;
                if !fty.has_prototype()
                    && (sig.params.len() != vals.len()
                        || sig
                            .params
                            .iter()
                            .zip(&vals)
                            .any(|(p, &v)| p.ty != function.value_type(function.resolve(v))))
                {
                    return Err(Error::new(
                        e.loc,
                        "unprototyped call arguments do not match the known function ABI",
                    ));
                }
                self.b.ins().call(fref, &vals)
            }
            None => {
                let callee = self.val(f)?;
                let unprototyped = FuncTy {
                    params: &[],
                    ..*fty
                };
                let sig = self.g.signature(
                    if fty.has_prototype() {
                        fty
                    } else {
                        &unprototyped
                    },
                    usize::MAX,
                    &extra,
                )?;
                let sig = self.b.import_signature(sig);
                self.b.ins().call_indirect(sig, callee, &vals)
            }
        };
        if direct.is_some_and(|g| self.g.p.globals[g].function_attrs.noreturn) {
            self.b.ins().trap();
            self.terminated = true;
            self.ensure_block();
            // A GNU noreturn function can have a non-void return type.
            // Its nominal result must not escape into this unreachable block.
            return Ok(if fty.ret == Ty::Void {
                None
            } else {
                Some(self.zero(if ret_slot.is_some() {
                    Type::Ptr
                } else {
                    fty.ret.ir_with_layout(self.g.p.c_layout)
                }))
            });
        }
        if let Some(slot) = ret_slot {
            return Ok(Some(self.b.ins().stack_addr(slot)));
        }
        Ok(self.b.function().inst_results(inst).first().copied())
    }

    /// A recognized `sqrt`, `sqrtf` or `sqrtl` call. Darwin's libm never sets
    /// `errno`, so it is just `fsqrt`. Linux libm sets `errno` for negative
    /// inputs, so those and NaNs still call the library, as Clang does.
    fn library_sqrt(&mut self, g: usize, fty: &FuncTy, arg: &Expr<'p>, loc: Loc) -> R<Value> {
        let x = self.val(arg)?;
        self.set_loc(loc);
        if self.g.p.target == keel_ir::Target::Aarch64AppleDarwin {
            return Ok(self.b.ins().fsqrt(x));
        }
        let ty = fty.ret.ir_with_layout(self.g.p.c_layout);
        let zero = if ty == Type::F32 {
            self.b.ins().f32const(0.0)
        } else {
            self.b.ins().f64const(0.0)
        };
        // Ordered: false for NaN.
        let in_domain = self.b.ins().fcmp(FloatCC::Ge, x, zero);
        let fast = self.new_block();
        let slow = self.new_block();
        let done = self.new_block();
        let result = self.b.append_block_param(done, ty);
        self.b.ins().brif(in_domain, fast, &[], slow, &[]);
        self.b.seal_block(fast);
        self.b.seal_block(slow);
        self.switch_to(fast);
        let root = self.b.ins().fsqrt(x);
        self.b.ins().jump(done, &[root]);
        self.switch_to(slow);
        self.g.use_global(g);
        let fref = self.import_global(g, fty, Some(&[]))?;
        let call = self.b.ins().call(fref, &[x]);
        let root = self.b.function().inst_results(call)[0];
        self.b.ins().jump(done, &[root]);
        self.b.seal_block(done);
        self.switch_to(done);
        Ok(result)
    }

    /// Compute using unsigned magnitudes, retaining a carry beyond 128 bits.
    /// This avoids both signed overflow and an IR integer wider than i128.
    pub(super) fn generic_overflow(
        &mut self,
        op: OverflowOp,
        args: &[Expr<'p>],
        vals: &[Value],
        loc: Loc,
    ) -> R<Value> {
        let result_ty = args[2].ty.pointee().unwrap();
        let max_size = args[..2]
            .iter()
            .map(|a| a.ty.int_kind().unwrap().size())
            .chain([result_ty.int_kind().unwrap().size()])
            .max()
            .unwrap();
        let wide_ty = Ty::Int(if max_size <= 4 {
            IntKind::ULong
        } else {
            IntKind::UInt128
        });
        let wide = wide_ty.ir_with_layout(self.g.p.c_layout);
        let a = self.convert(vals[0], &args[0].ty, &wide_ty, loc)?;
        let b = self.convert(vals[1], &args[1].ty, &wide_ty, loc)?;
        let zero = self.b.ins().iconst(wide, 0);
        let no = self.b.ins().iconst(Type::I8, 0);
        let yes = self.b.ins().iconst(Type::I8, 1);
        let an = if args[0].ty.is_signed() {
            self.b.ins().icmp(IntCC::Slt, a, zero)
        } else {
            no
        };
        let bn = if args[1].ty.is_signed() {
            self.b.ins().icmp(IntCC::Slt, b, zero)
        } else {
            no
        };
        let na = self.b.ins().ineg(a);
        let nb = self.b.ins().ineg(b);
        let am = self.b.ins().select(an, na, a);
        let bm = self.b.ins().select(bn, nb, b);
        let (magnitude, negative, carry) = if op == OverflowOp::Umul {
            let negative = self.b.ins().xor(an, bn);
            let (product, carry) = if args[..2]
                .iter()
                .any(|a| a.ty.int_kind().unwrap().size() == 16)
            {
                // a0*b0 + (a0*b1 + a1*b0)*2^64. Detect every
                // discarded high limb and carry without computing i256.
                let shift = self.b.ins().iconst(wide, 64);
                let mask = self.b.ins().iconst(wide, -1);
                let mask = self.b.ins().ushr(mask, shift);
                let al = self.b.ins().and(am, mask);
                let bl = self.b.ins().and(bm, mask);
                let ah = self.b.ins().ushr(am, shift);
                let bh = self.b.ins().ushr(bm, shift);
                let ll = self.b.ins().imul(al, bl);
                let lh = self.b.ins().imul(al, bh);
                let hl = self.b.ins().imul(ah, bl);
                let ah_nonzero = self.b.ins().icmp(IntCC::Ne, ah, zero);
                let bh_nonzero = self.b.ins().icmp(IntCC::Ne, bh, zero);
                let hh_nonzero = self.b.ins().and(ah_nonzero, bh_nonzero);
                let lh_high = self.b.ins().ushr(lh, shift);
                let hl_high = self.b.ins().ushr(hl, shift);
                let high = self.b.ins().or(lh_high, hl_high);
                let high_nonzero = self.b.ins().icmp(IntCC::Ne, high, zero);
                let lh_low = self.b.ins().shl(lh, shift);
                let hl_low = self.b.ins().shl(hl, shift);
                let (sum, c1) = self.b.ins().overflow(OverflowOp::Uadd, ll, lh_low);
                let (product, c2) = self.b.ins().overflow(OverflowOp::Uadd, sum, hl_low);
                let c = self.b.ins().or(c1, c2);
                let high = self.b.ins().or(hh_nonzero, high_nonzero);
                (product, self.b.ins().or(c, high))
            } else {
                (self.b.ins().imul(am, bm), no)
            };
            (product, negative, carry)
        } else {
            // Subtraction is addition with the second mathematical sign
            // reversed; negative zero is harmless and normalized below.
            let bn = if op == OverflowOp::Usub {
                self.b.ins().xor(bn, yes)
            } else {
                bn
            };
            let same_sign = self.b.ins().icmp(IntCC::Eq, an, bn);
            let (sum, carry) = self.b.ins().overflow(OverflowOp::Uadd, am, bm);
            let a_larger = self.b.ins().icmp(IntCC::Uge, am, bm);
            let ab = self.b.ins().isub(am, bm);
            let ba = self.b.ins().isub(bm, am);
            let difference = self.b.ins().select(a_larger, ab, ba);
            let difference_sign = self.b.ins().select(a_larger, an, bn);
            let magnitude = self.b.ins().select(same_sign, sum, difference);
            let negative = self.b.ins().select(same_sign, an, difference_sign);
            (magnitude, negative, self.b.ins().and(same_sign, carry))
        };
        let nonzero = self.b.ins().icmp(IntCC::Ne, magnitude, zero);
        // A wrapped zero with carry is still a nonzero mathematical value.
        let nonzero = self.b.ins().or(nonzero, carry);
        let negative = self.b.ins().and(negative, nonzero);
        let kind = result_ty.int_kind().unwrap();
        let bits = kind.size_with_layout(self.g.p.c_layout) * 8;
        let all = self.b.ins().iconst(wide, -1);
        let wide_bits = wide.bytes().unwrap() * 8;
        let limit = if kind.signed() {
            let shift = self
                .b
                .ins()
                .iconst(wide, i64::from(wide_bits) - bits as i64 + 1);
            let positive_max = self.b.ins().ushr(all, shift);
            let one = self.b.ins().uext(wide, negative);
            self.b.ins().iadd(positive_max, one)
        } else if bits == u64::from(wide_bits) {
            all
        } else {
            let shift = self
                .b
                .ins()
                .iconst(wide, i64::from(wide_bits) - bits as i64);
            self.b.ins().ushr(all, shift)
        };
        let too_large = self.b.ins().icmp(IntCC::Ugt, magnitude, limit);
        let overflow = self.b.ins().or(carry, too_large);
        let overflow = if kind.signed() {
            overflow
        } else {
            self.b.ins().or(overflow, negative)
        };
        let negated = self.b.ins().ineg(magnitude);
        let wrapped = self.b.ins().select(negative, negated, magnitude);
        let value = self.convert(wrapped, &wide_ty, result_ty, loc)?;
        let flags = self.object_flags(result_ty, &args[2].sugar.pointee(self.g.p.typedefs));
        self.b.ins().store_with_flags(value, vals[2], flags);
        Ok(overflow)
    }

    /// Checked memory builtins carry a canonical C prototype even when a
    /// fortified header has not declared the runtime entry point.
    pub(super) fn checked_memory_builtin(
        &mut self,
        id: Intrinsic,
        args: &[Expr<'p>],
        vals: &[Value],
        e: &Expr<'p>,
    ) -> R<Value> {
        let name = match id {
            Intrinsic::Memcpy => "__memcpy_chk",
            Intrinsic::Memmove => "__memmove_chk",
            Intrinsic::Memset => "__memset_chk",
            _ => unreachable!(),
        };
        let params: Vec<_> = args.iter().map(|arg| arg.ty).collect();
        let mut declared = None;
        // Inspect the completed translation unit, including declarations
        // after the builtin's source position. Never use an incompatible
        // ordinary declaration to choose the builtin's ABI.
        for (g, global) in self.g.p.globals.iter().enumerate() {
            if global.name != name && global.symbol != name {
                continue;
            }
            if !crate::expr::checked_memory_prototype(id, &global.ty) {
                return Err(Error::new(
                    e.loc,
                    "invalid checked memory builtin prototype",
                ));
            }
            if global.returns_twice {
                return Err(Error::new(
                    e.loc,
                    "incompatible checked memory builtin attributes: returns_twice",
                ));
            }
            declared = Some(g);
        }
        // Prove the values actually emitted, after all argument evaluation.
        // The AST evaluator represents integers as i64 and is not a sound
        // proof for expression trees containing __int128 arithmetic. A
        // canonical size_t constant in IR already has its final 64-bit bits.
        fn constant(f: &Function, v: Value, depth: u8) -> Option<u64> {
            let depth = depth.checked_sub(1)?;
            let v = f.resolve(v);
            let ValueDef::Result(i, 0) = f.value_def(v) else {
                return None;
            };
            let mask = |ty: Type| match ty {
                Type::I8 => Some(0xff),
                Type::I16 => Some(0xffff),
                Type::I32 => Some(0xffff_ffff),
                Type::I64 => Some(u64::MAX),
                _ => None,
            };
            if f.inst_results(i) != [v] {
                return None;
            }
            match f.inst_data(i) {
                InstData::Iconst { ty, imm } => Some(*imm as u64 & mask(*ty)?),
                InstData::Convert {
                    op: op @ (ConvertOp::Uext | ConvertOp::Sext | ConvertOp::Trunc),
                    ty,
                    arg,
                } => {
                    let from = f.value_type(f.resolve(*arg));
                    let from_mask = mask(from)?;
                    let mut value = constant(f, *arg, depth)? & from_mask;
                    if *op == ConvertOp::Sext {
                        let shift = 64 - from.bytes()? * 8;
                        value = (((value << shift) as i64) >> shift) as u64;
                    }
                    Some(value & mask(*ty)?)
                }
                _ => None,
            }
        }
        let bound = constant(self.b.function(), vals[3], 32);
        let length = constant(self.b.function(), vals[2], 32);
        if bound
            .is_some_and(|bound| bound == u64::MAX || length.is_some_and(|length| length <= bound))
        {
            let mut ordinary = vals[..3].to_vec();
            if id == Intrinsic::Memset {
                ordinary[1] = self.b.ins().trunc(Type::I8, ordinary[1]);
            }
            self.b.ins().intrinsic(id, &ordinary);
            return Ok(vals[0]);
        }
        let sig = Signature::new(&params.iter().map(Ty::ir).collect::<Vec<_>>(), &[Type::Ptr]);
        let symbol = if let Some(g) = declared {
            self.g.use_global(g);
            self.g.p.globals[g].symbol
        } else {
            if !self.g.module.decls.iter().any(|decl| decl.name == name) {
                self.g.module.decls.push(FuncDecl {
                    name: name.to_owned(),
                    attrs: Default::default(),
                    sig: sig.clone(),
                });
            }
            name
        };
        let callee = self.b.import_function(symbol, sig);
        let call = self.b.ins().call(callee, vals);
        Ok(self.b.function().inst_results(call)[0])
    }
}
