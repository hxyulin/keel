//! Builtin calls: argument checks and the typed builtin expressions. One
//! dispatch match routes each `__builtin_*` name to its family.

use super::*;

mod atomic;
mod float;
mod frame;
mod hint;
mod memory;
mod overflow;
mod query;
mod varargs;
mod x86;

/// An integer constant expression used as a builtin immediate, in the AST's
/// integer encoding. Any undefined behavior makes it invalid, unlike the
/// general evaluator, which wraps overflow and saturates float conversions.
fn integer_constant_control(e: &Expr<'_>, layout: CDataLayout) -> Option<i64> {
    if layout != CDataLayout::NATIVE_LP64 || !e.ty.is_integer() {
        return None;
    }
    match Eval::new(layout, &|_| false, Policy::STRICT).eval(e, Comma::Reject)? {
        Folded {
            value: Const::Int(c),
            class: Class::Integer,
        } => c.literal(),
        _ => None,
    }
}

impl<'a> Parser<'a> {
    pub(super) fn builtin(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        let base = &name["__builtin_".len()..];
        self.x86_preconditions(name, base, loc)?;
        match base {
            "cpu_supports" | "cpu_init" => self.cpu_builtin(base, loc),
            "ia32_pclmulqdq128" | "ia32_pclmulqdq256" | "ia32_pclmulqdq512" => {
                self.pclmulqdq(base, loc)
            }
            "ia32_sha256msg1" | "ia32_sha256msg2" | "ia32_sha256rnds2" => self.sha256(base, loc),
            "ia32_cmpps256" | "ia32_cmppd256" => self.avx_compare(base, loc),
            "shufflevector" => self.shufflevector(loc),
            "complex" => self.complex(loc),
            "memcpy" | "memmove" | "memset" | "memcmp" => self.library_memory_builtin(base, loc),
            "__memcpy_chk" | "__memmove_chk" | "__memset_chk" => {
                self.checked_memory_builtin(base, loc)
            }
            "va_start" | "va_arg" | "va_end" | "va_copy" => self.va_builtin(base, loc),
            "offsetof" => self.offsetof(loc),
            "expect" => self.builtin_call(Builtin::Hint, &[LONG, LONG], LONG, loc),
            "prefetch" => self.prefetch(loc),
            "expect_with_probability" => self.expect_with_probability(loc),
            "assume_aligned" => self.assume_aligned(loc),
            "object_size" | "dynamic_object_size" => self.object_size(loc),
            "has_attribute" => self.has_attribute(loc),
            "types_compatible_p" => self.types_compatible(loc),
            "choose_expr" => self.choose_expr(loc),
            "constant_p" => self.constant_p(loc),
            "inf" | "inff" | "infl" | "huge_val" | "huge_valf" | "huge_vall" | "nan" | "nanf"
            | "nanl" => self.float_constant(base, loc),
            "isnan" | "isinf" | "isinf_sign" | "isfinite" | "isnormal" | "signbit" | "signbitf"
            | "signbitl" | "fpclassify" | "isgreater" | "isgreaterequal" | "isless"
            | "islessequal" | "islessgreater" | "isunordered" => self.float_class(base, loc),
            "fabs" | "fabsf" | "fabsl" => self.fabs(base, loc),
            "bswap16" => self.builtin_call(
                Builtin::Bswap,
                &[Ty::Int(IntKind::UShort)],
                Ty::Int(IntKind::UShort),
                loc,
            ),
            "bswap32" => self.builtin_call(Builtin::Bswap, &[UINT], UINT, loc),
            "bswap64" => self.builtin_call(Builtin::Bswap, &[ULONG], ULONG, loc),
            "clz" | "ctz" | "popcount" | "clzl" | "ctzl" | "popcountl" | "clzll" | "ctzll"
            | "popcountll" => self.bit_count(base, loc),
            "ffs" | "ffsl" | "ffsll" => self.ffs(base, loc),
            "isdigit" => self.isdigit(loc),
            "alloca" => {
                let ptr = self.ptr_type(Ty::Void);
                self.builtin_call(Builtin::Alloca, &[ULONG], ptr, loc)
            }
            "return_address" => self.return_address(loc),
            "extract_return_addr" | "frob_return_addr" => self.return_address_conversion(base, loc),
            "frame_address" => self.frame_address(loc),
            "trap" | "unreachable" => self.builtin_call(Builtin::Trap, &[], Ty::Void, loc),
            "debugtrap" => self.builtin_call(Builtin::Debugtrap, &[], Ty::Void, loc),
            _ => self.table_builtin(name, base, loc),
        }
    }

    /// Builtins whose names come from a binding table rather than a fixed
    /// list, then builtins with declared library equivalents.
    fn table_builtin(&mut self, name: &str, base: &str, loc: Loc) -> R<Expr<'a>> {
        if let Some((op, shape)) = x86_reduction_builtin(base) {
            return self.x86_reduction(op, shape, loc);
        }
        if let Some((id, controls)) = crate::x86_builtins::semantic_builtin(base) {
            return self.x86_value(id, controls, loc);
        }
        if let Some(id) = packed_conversion_builtin(base) {
            return self.x86_packed_conversion(id, loc);
        }
        if let Some((id, shape, immediate)) = packed_shift_builtin(base) {
            return self.x86_packed_shift(id, shape, immediate, loc);
        }
        if let Some((id, unsigned, word)) = saturating_builtin(base) {
            return self.x86_saturating(id, unsigned, word, loc);
        }
        if let Some(id) = aes_builtin(base) {
            return self.x86_aes(id, loc);
        }
        if let Some(op) = generic_overflow(base) {
            return self.generic_overflow(op, loc);
        }
        if let Some((op, ty)) = typed_overflow(base) {
            return self.typed_overflow(op, ty, loc);
        }
        self.library_builtin(name, base, loc)
    }

    /// `__builtin_shufflevector`.
    fn shufflevector(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args_any()?;
        let Some(Ty::Vector(_, shape)) = args.first().map(|e| e.ty) else {
            return Err(Error::new(loc, "shufflevector requires vector operands"));
        };
        if args.len() != usize::from(shape.lanes()) + 2 || args[0].ty != args[1].ty {
            return Err(Error::new(
                loc,
                "shufflevector requires matching vectors and exactly one index per result lane",
            ));
        }
        for index in &args[2..] {
            if !index.ty.is_integer()
                || !matches!(eval_constant_with_layout(index, false, self.c_layout).and_then(Const::int), Some(n) if n >= 0 && n < i64::from(shape.lanes()) * 2)
            {
                return Err(Error::new(
                    index.loc,
                    "shufflevector index must be a constant in the concatenated input lane range",
                ));
            }
        }
        let ty = args[0].ty;
        Ok(mk(
            ExprKind::Builtin(Builtin::VectorShuffle, self.slice(&args)),
            ty,
            loc,
        ))
    }

    /// `__builtin_clz`, `ctz` and `popcount`, in each width.
    fn bit_count(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let op = match base.trim_end_matches('l') {
            "clz" => Builtin::Clz,
            "ctz" => Builtin::Ctz,
            _ => Builtin::Popcount,
        };
        let arg = if base.ends_with('l') { ULONG } else { UINT };
        self.builtin_call(op, &[arg], INT, loc)
    }

    /// `__builtin_ffs` in each width.
    fn ffs(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let arg = match base {
            "ffs" => INT,
            "ffsl" => LONG,
            _ => Ty::Int(IntKind::LLong),
        };
        self.builtin_call(Builtin::Ffs, &[arg], INT, loc)
    }

    /// `__builtin_isdigit`.
    fn isdigit(&mut self, loc: Loc) -> R<Expr<'a>> {
        let mut args = self.builtin_args(1, loc)?;
        let arg = self.convert_for_assign(args.remove(0), &INT)?;
        // Unsigned subtraction covers the decimal range with one use
        // of the argument, including EOF and values below '0'.
        let arg = self.cast(arg, UINT);
        let shifted = self.binop("-", arg, mk(ExprKind::Int(48), UINT, loc), loc)?;
        self.binop("<=", shifted, mk(ExprKind::Int(9), UINT, loc), loc)
    }

    /// Builtins with declared library equivalents, by name.
    fn library_builtin(&mut self, name: &str, base: &str, loc: Loc) -> R<Expr<'a>> {
        let base = self.text.intern(base);
        if let Ok(e) = self.lookup_value(base, loc)
            && matches!(e.ty, Ty::Func(_))
        {
            return Ok(e);
        }
        // Legacy fallback for other fortified library functions.
        // The dispatch match handles the three memory _chk names, which
        // must retain their bound. This fallback remains a compatibility
        // limitation for unrelated _chk builtins.
        if let Some(plain) = base.strip_prefix("__").and_then(|b| b.strip_suffix("_chk"))
            && let plain = self.text.intern(plain)
            && let Ok(f) = self.lookup_value(plain, loc)
            && matches!(f.ty, Ty::Func(_))
        {
            self.expect("(")?;
            let mut args = Vec::new();
            while !self.eat(")") {
                if !args.is_empty() {
                    self.expect(",")?;
                }
                args.push(self.assign()?);
            }
            args.pop();
            return self.make_call(f, args, loc);
        }
        Err(Error::new(loc, format!("`{name}` is not supported")))
    }

    /// `(a, b, ...)`, as values.
    fn builtin_args_any(&mut self) -> R<Vec<Expr<'a>>> {
        self.expect("(")?;
        let mut args = Vec::new();
        while !self.eat(")") {
            if !args.is_empty() {
                self.expect(",")?;
            }
            let e = self.assign()?;
            args.push(self.rvalue(e));
        }
        Ok(args)
    }

    fn builtin_args(&mut self, n: usize, loc: Loc) -> R<Vec<Expr<'a>>> {
        self.expect("(")?;
        let mut args = Vec::new();
        while !self.eat(")") {
            if !args.is_empty() {
                self.expect(",")?;
            }
            let e = self.assign()?;
            args.push(self.rvalue(e));
        }
        if args.len() != n {
            return Err(Error::new(loc, format!("builtin takes {n} argument(s)")));
        }
        Ok(args)
    }

    fn builtin_call(
        &mut self,
        op: Builtin,
        params: &[Ty<'a>],
        ret: Ty<'a>,
        loc: Loc,
    ) -> R<Expr<'a>> {
        let args = self.builtin_args(params.len(), loc)?;
        let mut converted = Vec::new();
        for (arg, ty) in args.into_iter().zip(params) {
            converted.push(self.convert_for_assign(arg, ty)?);
        }
        Ok(mk(ExprKind::Builtin(op, self.slice(&converted)), ret, loc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::integer_layout_tests::expression;
    use bumpalo::Bump;

    #[test]
    fn integer_constant_control_is_a_strict_native_proof() {
        let arena = Bump::new();
        let e = expression("1L + 2L", &arena, CDataLayout::WASM32);
        assert_eq!(integer_constant_control(&e, CDataLayout::WASM32), None);
        let e = expression("1L + 2L", &arena, CDataLayout::NATIVE_LP64);
        assert_eq!(
            integer_constant_control(&e, CDataLayout::NATIVE_LP64),
            Some(3)
        );
        // These deliberately remain native proofs with stricter validity rules
        // than the wrapping/saturating general evaluator.
        for src in [
            "2147483647 + 1",
            "1 << 32",
            "1 << -1",
            "(-2147483647 - 1) / -1",
            "(int)1e100",
            "(1, 2)",
        ] {
            let e = expression(src, &arena, CDataLayout::NATIVE_LP64);
            assert_eq!(
                integer_constant_control(&e, CDataLayout::NATIVE_LP64),
                None,
                "{src}"
            );
        }
        for src in [
            "(int)(__int128)1 + 2",
            "__builtin_bswap16(0x0100)",
            "(int)1.5e0",
        ] {
            let e = expression(src, &arena, CDataLayout::NATIVE_LP64);
            assert!(
                integer_constant_control(&e, CDataLayout::NATIVE_LP64).is_some(),
                "{src}"
            );
        }
        for src in [
            "(int)(1.5 + 1.0)",
            "(long)(void *)1",
            "(int)(__int128)1 << 40",
        ] {
            let e = expression(src, &arena, CDataLayout::NATIVE_LP64);
            assert_eq!(
                integer_constant_control(&e, CDataLayout::NATIVE_LP64),
                None,
                "{src}"
            );
        }
    }
}
