//! Builtin calls: argument checks and the typed builtin expressions,
//! including memory, atomic and floating-classification builtins.

use super::*;

/// Bounds the number of frame-record loads emitted by one C builtin.
const MAX_FRAME_ADDRESS_DEPTH: i64 = 64;

impl<'a> Parser<'a> {
    fn x86_value_type(&mut self, ty: keel_ir::Type) -> Ty<'a> {
        use keel_ir::Type;
        match ty {
            Type::F32 => Ty::Float,
            Type::F64 => Ty::Double,
            Type::Ptr => Ty::Ptr(self.type_ref(Ty::Void)),
            Type::I8 => Ty::Int(IntKind::SChar),
            Type::I16 => Ty::Int(IntKind::Short),
            Type::I32 => INT,
            Type::I64 => Ty::Int(IntKind::LLong),
            Type::Vector(shape) => {
                let element = match shape.lane_type() {
                    Type::F32 => Ty::Float,
                    Type::F64 => Ty::Double,
                    Type::I8 => Ty::Int(IntKind::SChar),
                    Type::I16 => Ty::Int(IntKind::Short),
                    Type::I32 => INT,
                    Type::I64 => Ty::Int(IntKind::LLong),
                    _ => unreachable!("reviewed x86 value lane type"),
                };
                Ty::Vector(self.type_ref(element), shape)
            }
            _ => unreachable!("reviewed x86 value signature"),
        }
    }

    /// Record a call to `name`, which some feature set in `sets` permits,
    /// when the enclosing function enables none of them. Like the backend,
    /// report it only if the function is emitted.
    fn check_x86_features(&mut self, name: &str, sets: crate::x86_builtins::FeatureSets, loc: Loc) {
        use keel_asm_ir::x86::Features;
        let Some(fs) = &mut self.fs else { return };
        let Some(enabled) = fs.x86_features.filter(|_| fs.x86_feature_error.is_none()) else {
            return;
        };
        let has = |feature: &&str| Features::parse(feature).is_ok_and(|f| enabled.contains(f));
        if sets.iter().any(|set| set.iter().all(has)) {
            return;
        }
        let missing: Vec<_> = sets[0]
            .iter()
            .filter(|feature| !has(feature))
            .map(|feature| format!("`{feature}`"))
            .collect();
        let plural = if missing.len() == 1 { "" } else { "s" };
        fs.x86_feature_error = Some(Error::new(
            loc,
            format!(
                "`{name}` requires target feature{plural} {}, which `{}` does not enable",
                missing.join(", "),
                fs.name
            ),
        ));
    }

    pub(super) fn builtin(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        let base = &name["__builtin_".len()..];
        if let Some(sets) = crate::x86_builtins::required_features(base) {
            self.check_x86_features(name, sets, loc);
        }
        if base.starts_with("ia32_")
            || base.starts_with("keel_sse2_")
            || base.starts_with("keel_avx2_")
        {
            self.require_native_layout(loc, "x86 builtins")?;
        }
        if matches!(base, "cpu_supports" | "cpu_init") {
            if self.target != Target::X86_64LinuxGnu {
                return Err(Error::new(loc, "CPU capability builtins require x86-64"));
            }
            self.require_native_layout(loc, "CPU capability builtins")?;
            if base == "cpu_init" {
                self.builtin_args(0, loc)?;
                return Ok(mk(
                    ExprKind::Builtin(Builtin::X86CpuInit, self.slice(&[])),
                    Ty::Void,
                    loc,
                ));
            }
            let args = self.builtin_args(1, loc)?;
            let mut argument = &args[0];
            loop {
                match argument.kind {
                    ExprKind::Cast(inner) => argument = inner,
                    ExprKind::Addr(inner) if matches!(inner.kind, ExprKind::Str(_)) => {
                        argument = inner
                    }
                    _ => break,
                }
            }
            let ExprKind::Str(index) = argument.kind else {
                return Err(Error::new(
                    loc,
                    "CPU capability name must be a string literal",
                ));
            };
            let string = &self.strings[index];
            let feature = string
                .bytes
                .strip_suffix(&[0])
                .filter(|_| !string.wide)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(crate::x86_cpu::lookup)
                .ok_or_else(|| Error::new(loc, "unknown CPU capability name"))?;
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86CpuSupports(feature), self.slice(&[])),
                INT,
                loc,
            ));
        }
        if let Some((op, shape)) = x86_reduction_builtin(base) {
            if self.target != Target::X86_64LinuxGnu {
                return Err(Error::new(loc, "x86 reduction builtin requires x86-64"));
            }
            self.require_native_layout(loc, "x86 reduction builtins")?;
            let args = self.builtin_args(1, loc)?;
            let expected = self.x86_value_type(keel_ir::Type::Vector(shape));
            if args[0].ty != expected {
                return Err(Error::new(
                    args[0].loc,
                    "x86 reduction builtin has the wrong vector type",
                ));
            }
            let result = self.x86_value_type(shape.lane_type());
            return Ok(mk(
                ExprKind::Builtin(Builtin::VectorReduce(op), self.slice(&args)),
                result,
                loc,
            ));
        }
        if let Some((id, controls)) = crate::x86_builtins::semantic_builtin(base) {
            if self.target != Target::X86_64LinuxGnu {
                return Err(Error::new(loc, "x86 value builtin requires x86-64"));
            }
            self.require_native_layout(loc, "x86 value builtins")?;
            let info = id.info();
            let mut args = self.builtin_args(info.params.len(), loc)?;
            for (i, (&expected, arg)) in info.params.iter().zip(&mut args).enumerate() {
                if let Some(control) = controls.iter().find(|control| i == control.index) {
                    let Some(value) = integer_constant_control(arg, self.c_layout).filter(|n| {
                        (0..=control.maximum).contains(n)
                            && (control.allowed_values.is_empty()
                                || control.allowed_values.contains(n))
                    }) else {
                        return Err(Error::new(loc, control.diagnostic));
                    };
                    // Each reviewed recipe retains only the control bits exposed by IR.
                    *arg = mk(ExprKind::Int(value & control.mask), INT, arg.loc);
                }
                let expected = self.x86_value_type(expected);
                if arg.ty != expected {
                    return Err(Error::new(
                        arg.loc,
                        format!("x86 value builtin argument {} has the wrong type", i + 1),
                    ));
                }
            }
            let result = info
                .returns
                .first()
                .map_or(Ty::Void, |&ty| self.x86_value_type(ty));
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86Value(id), self.slice(&args)),
                result,
                loc,
            ));
        }
        if let Some(id) = packed_conversion_builtin(base) {
            let args = self.builtin_args(1, loc)?;
            let to_float = id == Intrinsic::X86Ssecvtdq2ps128;
            let input = if to_float {
                Ty::Vector(&INT, keel_ir::VectorType::I32x4)
            } else {
                Ty::Vector(&Ty::Float, keel_ir::VectorType::F32x4)
            };
            if self.target != Target::X86_64LinuxGnu || args[0].ty != input {
                return Err(Error::new(
                    loc,
                    "SSE2 packed conversion requires its matching signed i32x4 or f32x4 vector on x86-64",
                ));
            }
            let result = if to_float {
                Ty::Vector(&Ty::Float, keel_ir::VectorType::F32x4)
            } else {
                Ty::Vector(&INT, keel_ir::VectorType::I32x4)
            };
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86PackedConversion(id), self.slice(&args)),
                result,
                loc,
            ));
        }
        if let Some((id, shape, immediate)) = packed_shift_builtin(base) {
            let mut args = self.builtin_args(2, loc)?;
            if self.target != Target::X86_64LinuxGnu
                || !matches!(args[0].ty, Ty::Vector(Ty::Int(_), v) if v == shape)
            {
                return Err(Error::new(
                    loc,
                    "x86 packed shift requires its matching integer lane shape on x86-64",
                ));
            }
            if immediate {
                // Reuse the reviewed bounded ICE proof, including undefined-expression checks.
                let Some(n) = integer_constant_control(&args[1], self.c_layout)
                    .filter(|n| (0..=255).contains(n))
                else {
                    return Err(Error::new(
                        loc,
                        "x86 packed shift control must be an integer constant expression in 0..255",
                    ));
                };
                args[1] = mk(ExprKind::Int(n), Ty::Int(IntKind::ULLong), args[1].loc);
            } else if args[1].ty != Ty::Int(IntKind::ULLong) {
                return Err(Error::new(
                    loc,
                    "x86 packed shift count must be unsigned long long",
                ));
            }
            let ty = args[0].ty;
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86PackedShift(id), self.slice(&args)),
                ty,
                loc,
            ));
        }
        if let Some((id, unsigned, word)) = saturating_builtin(base) {
            let args = self.builtin_args(2, loc)?;
            let elem = match (unsigned, word) {
                (false, false) => IntKind::SChar,
                (true, false) => IntKind::UChar,
                (false, true) => IntKind::Short,
                (true, true) => IntKind::UShort,
            };
            let shape = if word {
                keel_ir::VectorType::I16x8
            } else {
                keel_ir::VectorType::I8x16
            };
            if self.target != Target::X86_64LinuxGnu
                || !matches!(args[0].ty, Ty::Vector(Ty::Int(k), v) if *k == elem && v == shape)
                || args[0].ty != args[1].ty
            {
                return Err(Error::new(
                    loc,
                    "SSE2 saturation requires matching signed/unsigned 128-bit byte/word vectors on x86-64",
                ));
            }
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86Saturating(id), self.slice(&args)),
                args[0].ty,
                loc,
            ));
        }
        if matches!(
            base,
            "ia32_pclmulqdq128" | "ia32_pclmulqdq256" | "ia32_pclmulqdq512"
        ) {
            let (id, shape) = match base {
                "ia32_pclmulqdq256" => (
                    keel_ir::Intrinsic::X86Vpclmulqdq256,
                    keel_ir::VectorType::I64x4,
                ),
                "ia32_pclmulqdq512" => (
                    keel_ir::Intrinsic::X86Vpclmulqdq512,
                    keel_ir::VectorType::I64x8,
                ),
                _ => (
                    keel_ir::Intrinsic::X86Pclmulqdq128,
                    keel_ir::VectorType::I64x2,
                ),
            };
            let args = self.builtin_args(3, loc)?;
            if self.target != Target::X86_64LinuxGnu
                || !matches!(args[0].ty, Ty::Vector(Ty::Int(_), v) if v == shape)
                || args[0].ty != args[1].ty
            {
                return Err(Error::new(
                    loc,
                    "PCLMULQDQ requires matching integer vectors of the requested width on x86-64",
                ));
            }
            let Some(n) = integer_constant_control(&args[2], self.c_layout) else {
                return Err(Error::new(
                    loc,
                    "PCLMULQDQ control must be an integer constant expression in0..=255",
                ));
            };
            if !(0..=255).contains(&n) {
                return Err(Error::new(loc, "PCLMULQDQ control must be in0..=255"));
            }
            return Ok(mk(
                ExprKind::Builtin(Builtin::Pclmulqdq(id, n as u8), self.slice(&args[..2])),
                args[0].ty,
                loc,
            ));
        }
        if let Some(id) = match base {
            "ia32_sha256msg1" => Some(keel_ir::Intrinsic::X86Sha256msg1),
            "ia32_sha256msg2" => Some(keel_ir::Intrinsic::X86Sha256msg2),
            "ia32_sha256rnds2" => Some(keel_ir::Intrinsic::X86Sha256rnds2),
            _ => None,
        } {
            let args = self.builtin_args(id.info().params.len(), loc)?;
            if self.target != Target::X86_64LinuxGnu
                || args.iter().any(|arg| {
                    !matches!(arg.ty, Ty::Vector(Ty::Int(_), keel_ir::VectorType::I32x4))
                })
            {
                return Err(Error::new(
                    loc,
                    "SHA-256 requires i32x4 integer vectors on x86-64",
                ));
            }
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86Value(id), self.slice(&args)),
                args[0].ty,
                loc,
            ));
        }
        if let Some(id) = aes_builtin(base) {
            let keygen = id == keel_ir::Intrinsic::X86Aeskeygenassist128;
            let unary = id == keel_ir::Intrinsic::X86Aesimc128 || keygen;
            let mut args = self.builtin_args(if unary && !keygen { 1 } else { 2 }, loc)?;
            if self.target != Target::X86_64LinuxGnu
                || !matches!(
                    args[0].ty,
                    Ty::Vector(Ty::Int(IntKind::LLong), keel_ir::VectorType::I64x2)
                )
                || (!unary && args[0].ty != args[1].ty)
            {
                return Err(Error::new(
                    loc,
                    "AES builtin requires matching __m128i vectors on x86-64",
                ));
            }
            if keygen {
                // Reuse the reviewed PCLMUL bounded ICE proof unchanged.
                let Some(n) = integer_constant_control(&args[1], self.c_layout) else {
                    return Err(Error::new(
                        loc,
                        "AESKEYGENASSIST requires an integer constant expression in 0..255",
                    ));
                };
                if !(0..=255).contains(&n) {
                    return Err(Error::new(
                        loc,
                        "AESKEYGENASSIST requires an integer constant expression in 0..255",
                    ));
                }
                args[1] = mk(ExprKind::Int(n), INT, args[1].loc);
            }
            let ty = args[0].ty;
            return Ok(mk(
                ExprKind::Builtin(Builtin::X86Aes(id), self.slice(&args)),
                ty,
                loc,
            ));
        }
        if matches!(base, "ia32_cmpps256" | "ia32_cmppd256") {
            use keel_ir::{FloatCC, VectorType};
            let args = self.builtin_args(3, loc)?;
            let expected = if base == "ia32_cmpps256" {
                VectorType::F32x8
            } else {
                VectorType::F64x4
            };
            if self.target != Target::X86_64LinuxGnu
                || !matches!(args[0].ty, Ty::Vector(_, shape) if shape == expected)
                || args[0].ty != args[1].ty
            {
                return Err(Error::new(
                    loc,
                    "AVX comparison requires matching packed floating vectors on x86-64",
                ));
            }
            let cc = match args[2]
                .ty
                .is_integer()
                .then(|| eval_constant_with_layout(&args[2], false, self.c_layout))
                .flatten()
                .and_then(Const::int)
            {
                Some(0) => FloatCC::Eq,
                Some(3) => FloatCC::Uno,
                Some(4) => FloatCC::Ne,
                Some(7) => FloatCC::Ord,
                Some(17) => FloatCC::Lt,
                Some(18) => FloatCC::Le,
                Some(29) => FloatCC::Ge,
                Some(30) => FloatCC::Gt,
                _ => {
                    return Err(Error::new(
                        loc,
                        "AVX comparison supports constant quiet predicates 0, 3, 4, 7, 17, 18, 29, 30",
                    ));
                }
            };
            let ty = args[0].ty;
            return Ok(mk(
                ExprKind::Builtin(Builtin::VectorFloatCompare(cc), self.slice(&args[..2])),
                ty,
                loc,
            ));
        }
        if base == "shufflevector" {
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
            return Ok(mk(
                ExprKind::Builtin(Builtin::VectorShuffle, self.slice(&args)),
                ty,
                loc,
            ));
        }
        if base == "complex" {
            let args = self.builtin_args(2, loc)?;
            let elem = args[0].ty;
            if !matches!(elem, Ty::Float | Ty::Double) || elem != args[1].ty {
                return Err(Error::new(
                    loc,
                    "__builtin_complex requires two matching float or double operands",
                ));
            }
            let ty = Ty::Complex(self.type_ref(elem));
            return Ok(mk(
                ExprKind::Builtin(Builtin::Complex, self.slice(&args)),
                ty,
                loc,
            ));
        }
        if let Some(op) = generic_overflow(base) {
            let args = self.builtin_args(3, loc)?;
            if !args[0].ty.is_integer() || !args[1].ty.is_integer() {
                return Err(Error::new(
                    loc,
                    "generic overflow operands must be integers",
                ));
            }
            let result_sugar = args[2].sugar.pointee(&self.typedefs);
            fn is_const(sugar: &Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
                match sugar {
                    Sugar::Const(_) => true,
                    Sugar::Volatile(inner) | Sugar::Restrict(inner) => is_const(inner, defs),
                    Sugar::Typedef(i) => is_const(&defs[*i].sugar, defs),
                    _ => false,
                }
            }
            if !args[2].ty.is_ptr()
                || !matches!(args[2].ty.pointee(), Some(Ty::Int(k)) if *k != IntKind::Bool)
                || matches!(result_sugar.unqualified(&self.typedefs), Sugar::Enum(_))
                || is_const(&result_sugar, &self.typedefs)
            {
                return Err(Error::new(
                    loc,
                    "generic overflow result must point to a non-const, non-boolean, non-enum integer",
                ));
            }
            return Ok(mk(
                ExprKind::Builtin(Builtin::GenericOverflow(op), self.slice(&args)),
                Ty::Int(IntKind::Bool),
                loc,
            ));
        }
        if let Some((op, ty)) = typed_overflow(base) {
            let args = self.builtin_args(3, loc)?;
            if !args[0].ty.is_arith() || !args[1].ty.is_arith() {
                return Err(Error::new(
                    loc,
                    "overflow operands must be arithmetic values",
                ));
            }
            if !args[2].ty.is_ptr() && !is_null(&args[2]) {
                return Err(Error::new(loc, "overflow result must be a pointer"));
            }
            if let Some(result_ty) = args[2].ty.pointee()
                && *result_ty != ty
                && *result_ty != Ty::Void
            {
                return Err(Error::new(
                    loc,
                    "overflow result pointer has an incompatible type",
                ));
            }
            let params = [ty, ty, self.ptr_type(ty)];
            let args = args
                .into_iter()
                .zip(&params)
                .map(|(arg, ty)| self.convert_for_assign(arg, ty))
                .collect::<R<Vec<_>>>()?;
            return Ok(mk(
                ExprKind::Builtin(Builtin::Overflow(op), self.slice(&args)),
                Ty::Int(IntKind::Bool),
                loc,
            ));
        }
        match base {
            "memcpy" | "memmove" | "memset" | "memcmp" => {
                if !self.is("(") {
                    // Library builtins also denote their ordinary library
                    // function when used as an address or parenthesized callee.
                    let base = self.text.intern(base);
                    let function = self.lookup_value(base, loc)?;
                    if !matches!(function.ty, Ty::Func(_)) {
                        return Err(Error::new(loc, "memory builtin needs a library function"));
                    }
                    return Ok(function);
                }
                self.memory_builtin(base, false, loc)
            }
            "__memcpy_chk" | "__memmove_chk" | "__memset_chk" => {
                let plain = base
                    .strip_prefix("__")
                    .unwrap()
                    .strip_suffix("_chk")
                    .unwrap();
                self.memory_builtin(plain, true, loc)
            }
            "va_start" | "va_arg" | "va_end" | "va_copy" => {
                self.expect("(")?;
                let ap = self.assign()?;
                let e = match base {
                    "va_start" => {
                        self.expect(",")?;
                        self.assign()?;
                        let ap = self.va_list_addr(ap)?;
                        mk(ExprKind::VaStart(self.node(ap)), Ty::Void, loc)
                    }
                    "va_arg" => {
                        self.expect(",")?;
                        let ty = self.type_name()?;
                        if ty.is_complex() {
                            return Err(Error::new(loc, "complex va_arg is not supported"));
                        }
                        if !ty.is_scalar() {
                            return Err(Error::new(loc, "`va_arg` of a non-scalar"));
                        }
                        let ap = self.va_list_addr(ap)?;
                        mk(ExprKind::VaArg(self.node(ap)), ty, loc)
                    }
                    // SysV's va_list is an array, so copying is copying the
                    // struct it holds.
                    "va_copy" if self.target == Target::X86_64LinuxGnu => {
                        self.expect(",")?;
                        let src = self.assign()?;
                        let [dst, src] = [ap, src].map(|e| -> R<_> {
                            let p = self.va_list_addr(e)?;
                            let ty = p.ty.pointee().cloned().expect("a pointer");
                            Ok(mk(ExprKind::Deref(self.node(p)), ty, loc))
                        });
                        self.assignment("=", dst?, src?, loc)?
                    }
                    "va_copy" => {
                        self.expect(",")?;
                        let src = self.assign()?;
                        self.assignment("=", ap, src, loc)?
                    }
                    _ => self.cast(ap, Ty::Void),
                };
                self.expect(")")?;
                Ok(e)
            }
            "offsetof" => {
                self.expect("(")?;
                let ty = self.type_name()?;
                self.expect(",")?;
                let mut ty = ty;
                let layout = self.c_layout;
                let constant = |value: u64| {
                    mk(
                        ExprKind::Int(wrap_literal(value as i64, &ULONG, layout)),
                        ULONG,
                        loc,
                    )
                };
                let mut offset = constant(0);
                let mut first = true;
                loop {
                    if !first && self.eat("[") {
                        let index = self.expr()?;
                        self.expect("]")?;
                        let Ty::Array(array) = ty else {
                            return Err(Error::new(loc, "`offsetof` subscript of a non-array"));
                        };
                        let index = self.rvalue(index);
                        if !index.ty.is_integer() {
                            return Err(Error::new(
                                index.loc,
                                "`offsetof` array index must be an integer",
                            ));
                        }
                        // Compute a byte offset in size_t arithmetic. No object
                        // exists here, so array subscripts must not create a
                        // pointer or dereference memory. Negative indices wrap
                        // in the target's unsigned size domain, as GNU does.
                        self.elem_size(&ty, loc)?;
                        let index = self.cast(index, ULONG);
                        let stride = self.size_expr(&array.elem, loc);
                        let scaled = self.binop("*", index, stride, loc)?;
                        offset = self.binop("+", offset, scaled, loc)?;
                        ty = array.elem;
                        continue;
                    }
                    if !offset_member_follows(self, first) {
                        if first {
                            return Err(Error::new(loc, "`offsetof` requires a member designator"));
                        }
                        break;
                    }
                    first = false;
                    let name = self.ident()?;
                    let Ty::Struct(id) = ty else {
                        return Err(Error::new(loc, "`offsetof` member of a non-struct"));
                    };
                    let Some((off, m)) =
                        self.structs[id].find(name, &self.structs, &self.typedefs, self.arena)
                    else {
                        return Err(Error::new(loc, format!("no member `{name}`")));
                    };
                    if m.bits.is_some() {
                        return Err(Error::new(loc, "`offsetof` cannot name a bit-field"));
                    }
                    offset = self.binop("+", offset, constant(off), loc)?;
                    ty = m.ty;
                }
                self.expect(")")?;
                // Keep static designators as integer constants for enum values,
                // initializers and existing ICE consumers. The checked-operand
                // evaluator preserves side effects in runtime index expressions.
                Ok(
                    match eval_constant_with_layout(&offset, true, layout).and_then(Const::int) {
                        Some(value) => mk(ExprKind::Int(value), ULONG, loc),
                        _ => offset,
                    },
                )
            }
            "expect" => self.builtin_call(Builtin::Hint, &[LONG, LONG], LONG, loc),
            "prefetch" => {
                let args = self.builtin_args_any()?;
                if !(1..=3).contains(&args.len()) {
                    return Err(Error::new(
                        loc,
                        "`__builtin_prefetch` takes 1 to 3 arguments",
                    ));
                }
                if !args[0].ty.is_ptr() && !is_null(&args[0]) {
                    return Err(Error::new(loc, "prefetch address must be a pointer"));
                }
                for (arg, (name, max)) in args[1..].iter().zip([("rw", 2), ("locality", 3)]) {
                    if !integer_constant_control(arg, self.c_layout)
                        .is_some_and(|value| (0..=max).contains(&value))
                    {
                        return Err(Error::new(
                            loc,
                            format!("prefetch {name} must be an integer constant in 0..{max}"),
                        ));
                    }
                }
                let ptr = self.ptr_type(Ty::Void);
                let address = self.convert_for_assign(args[0], &ptr)?;
                Ok(mk(
                    ExprKind::Builtin(Builtin::Prefetch, self.slice(&[address])),
                    Ty::Void,
                    loc,
                ))
            }
            "expect_with_probability" => {
                let args = self.builtin_args(3, loc)?;
                let mut args = args.into_iter();
                let value = self.convert_for_assign(args.next().unwrap(), &LONG)?;
                let expected = self.convert_for_assign(args.next().unwrap(), &LONG)?;
                let probability = self.convert_for_assign(args.next().unwrap(), &Ty::Double)?;
                let Some(Const::Float(p)) =
                    eval_constant_with_layout(&probability, true, self.c_layout)
                else {
                    return Err(Error::new(
                        loc,
                        "probability must be a constant floating-point expression",
                    ));
                };
                if !p.is_finite() || !(0.0..=1.0).contains(&p) {
                    return Err(Error::new(loc, "probability must be between 0 and 1"));
                }
                Ok(mk(
                    ExprKind::Builtin(Builtin::Hint, self.slice(&[value, expected])),
                    LONG,
                    loc,
                ))
            }
            "assume_aligned" => {
                let args = self.builtin_args_any()?;
                if !(2..=3).contains(&args.len()) {
                    return Err(Error::new(
                        loc,
                        "`__builtin_assume_aligned` takes 2 or 3 arguments",
                    ));
                }
                if !args[0].ty.is_ptr() && !is_null(&args[0]) {
                    return Err(Error::new(loc, "alignment hint needs a pointer"));
                }
                if !args[1].ty.is_integer() {
                    return Err(Error::new(loc, "alignment must be a constant integer"));
                }
                let alignment = self.cast(args[1], ULONG);
                let Some(n) = eval_constant_with_layout(&alignment, false, self.c_layout)
                    .and_then(Const::int)
                else {
                    return Err(Error::new(loc, "alignment must be a constant integer"));
                };
                if !(n as u64).is_power_of_two() {
                    return Err(Error::new(loc, "alignment must be a nonzero power of two"));
                }
                let ptr = self.ptr_type(Ty::Void);
                let mut args = args.into_iter();
                let value = self.convert_for_assign(args.next().unwrap(), &ptr)?;
                args.next(); // The checked constant alignment carries no runtime effect.
                let mut values = vec![value];
                if let Some(offset) = args.next() {
                    if !offset.ty.is_integer() {
                        return Err(Error::new(loc, "alignment offset must be an integer"));
                    }
                    values.push(self.cast(offset, ULONG));
                }
                Ok(mk(
                    ExprKind::Builtin(Builtin::AssumeAligned(n as u64), self.slice(&values)),
                    ptr,
                    loc,
                ))
            }
            "object_size" | "dynamic_object_size" => {
                // The size is never known here: "unknown" is -1 for types 0
                // and 1 (maximum), 0 for types 2 and 3 (minimum).
                let args = self.builtin_args(2, loc)?;
                let Some(kind) = eval_int_with_layout(&args[1], self.c_layout) else {
                    return Err(Error::new(
                        loc,
                        "`__builtin_object_size` type must be constant",
                    ));
                };
                let v = if kind & 2 == 0 { -1 } else { 0 };
                Ok(mk(ExprKind::Int(v), ULONG, loc))
            }
            "has_attribute" => {
                self.expect("(")?;
                let nonstring = if self.is_typename() {
                    self.type_name()?;
                    false
                } else {
                    let operand = self.assign()?;
                    self.has_nonstring(&operand)
                };
                self.expect(",")?;
                let attribute = self.ident()?;
                if attribute.trim_matches('_') != "nonstring" {
                    return Err(Error::new(
                        loc,
                        "__builtin_has_attribute currently supports only nonstring queries",
                    ));
                }
                self.expect(")")?;
                Ok(mk(
                    ExprKind::Int(i64::from(nonstring)),
                    Ty::Int(IntKind::Bool),
                    loc,
                ))
            }
            "types_compatible_p" => {
                self.expect("(")?;
                let a = self.type_name()?;
                self.expect(",")?;
                let b = self.type_name()?;
                self.expect(")")?;
                let same = a.unatomic() == b.unatomic();
                Ok(mk(ExprKind::Int(i64::from(same)), INT, loc))
            }
            "choose_expr" => {
                // Like `?:` with a constant condition, but the chosen
                // operand keeps its own type.
                self.expect("(")?;
                let c = self.assign()?;
                let Some(c) = eval_int_with_layout(&c, self.c_layout) else {
                    return Err(Error::new(loc, "`__builtin_choose_expr` needs a constant"));
                };
                self.expect(",")?;
                let a = self.assign()?;
                self.expect(",")?;
                let b = self.assign()?;
                self.expect(")")?;
                Ok(if c != 0 { a } else { b })
            }
            "constant_p" => {
                let args = self.builtin_args(1, loc)?;
                let known = args[0].ty.is_integer()
                    && matches!(
                        eval_constant_with_layout(&args[0], true, self.c_layout),
                        Some(Const::Int(_))
                    );
                Ok(mk(ExprKind::Int(i64::from(known)), INT, loc))
            }
            "inf" | "inff" | "infl" | "huge_val" | "huge_valf" | "huge_vall" | "nan" | "nanf"
            | "nanl" => {
                let n = usize::from(base.starts_with("nan"));
                self.builtin_args(n, loc)?;
                let value = if n == 1 { f64::NAN } else { f64::INFINITY };
                let ty = if base.ends_with('f') {
                    Ty::Float
                } else {
                    Ty::Double
                };
                Ok(mk(ExprKind::Float(value), ty, loc))
            }
            "isnan" | "isinf" | "isinf_sign" | "isfinite" | "isnormal" | "signbit" | "signbitf"
            | "signbitl" | "fpclassify" | "isgreater" | "isgreaterequal" | "isless"
            | "islessequal" | "islessgreater" | "isunordered" => self.float_class(base, loc),
            "fabs" | "fabsf" | "fabsl" => {
                let ty = if base == "fabsf" {
                    Ty::Float
                } else {
                    Ty::Double
                };
                self.builtin_call(Builtin::Fabs, std::slice::from_ref(&ty), ty, loc)
            }
            "bswap16" => self.builtin_call(
                Builtin::Bswap,
                &[Ty::Int(IntKind::UShort)],
                Ty::Int(IntKind::UShort),
                loc,
            ),
            "bswap32" => self.builtin_call(Builtin::Bswap, &[UINT], UINT, loc),
            "bswap64" => self.builtin_call(Builtin::Bswap, &[ULONG], ULONG, loc),
            "clz" | "ctz" | "popcount" | "clzl" | "ctzl" | "popcountl" | "clzll" | "ctzll"
            | "popcountll" => {
                let op = match base.trim_end_matches('l') {
                    "clz" => Builtin::Clz,
                    "ctz" => Builtin::Ctz,
                    _ => Builtin::Popcount,
                };
                let arg = if base.ends_with('l') { ULONG } else { UINT };
                self.builtin_call(op, &[arg], INT, loc)
            }
            "ffs" | "ffsl" | "ffsll" => {
                let arg = match base {
                    "ffs" => INT,
                    "ffsl" => LONG,
                    _ => Ty::Int(IntKind::LLong),
                };
                self.builtin_call(Builtin::Ffs, &[arg], INT, loc)
            }
            "isdigit" => {
                let mut args = self.builtin_args(1, loc)?;
                let arg = self.convert_for_assign(args.remove(0), &INT)?;
                // Unsigned subtraction covers the decimal range with one use
                // of the argument, including EOF and values below '0'.
                let arg = self.cast(arg, UINT);
                let shifted = self.binop("-", arg, mk(ExprKind::Int(48), UINT, loc), loc)?;
                self.binop("<=", shifted, mk(ExprKind::Int(9), UINT, loc), loc)
            }
            "alloca" => {
                let ptr = self.ptr_type(Ty::Void);
                self.builtin_call(Builtin::Alloca, &[ULONG], ptr, loc)
            }
            "return_address" => {
                let args = self.builtin_args(1, loc)?;
                if self.target != Target::X86_64LinuxGnu {
                    return Err(Error::new(
                        loc,
                        "`__builtin_return_address` requires x86-64 Linux",
                    ));
                }
                let depth = integer_constant_control(&args[0], self.c_layout);
                let Some(depth @ 0..=64) = depth else {
                    return Err(Error::new(
                        loc,
                        "`__builtin_return_address` requires constant integer depth 0..64",
                    ));
                };
                Ok(mk(
                    ExprKind::Builtin(Builtin::ReturnAddress(depth as u8), &[]),
                    self.ptr_type(Ty::Void),
                    loc,
                ))
            }
            "extract_return_addr" | "frob_return_addr" => {
                let args = self.builtin_args(1, loc)?;
                if self.target != Target::X86_64LinuxGnu {
                    return Err(Error::new(
                        loc,
                        format!("`__builtin_{base}` requires x86-64 Linux"),
                    ));
                }
                // SysV x86-64 stores the actual continuation address. Other
                // targets may encode or sign it and need their own contract.
                if !args[0].ty.is_ptr()
                    && integer_constant_control(&args[0], self.c_layout) != Some(0)
                {
                    return Err(Error::new(
                        loc,
                        "return-address conversion requires a pointer",
                    ));
                }
                let ptr = self.ptr_type(Ty::Void);
                self.convert_for_assign(args[0], &ptr)
            }
            "frame_address" => {
                let args = self.builtin_args(1, loc)?;
                if !Target::NATIVE.contains(&self.target) {
                    return Err(Error::new(
                        loc,
                        "`__builtin_frame_address` requires a native frame-record ABI",
                    ));
                }
                // Bound emitted pointer loads without relaxing the shared ICE
                // evaluator's checks for overflow, shifts and side effects.
                let Some(depth) = integer_constant_control(&args[0], self.c_layout)
                    .filter(|n| (0..=MAX_FRAME_ADDRESS_DEPTH).contains(n))
                else {
                    return Err(Error::new(
                        loc,
                        "`__builtin_frame_address` requires an integer constant expression in 0..64",
                    ));
                };
                Ok(mk(
                    ExprKind::Builtin(Builtin::FrameAddress(depth as u8), &[]),
                    self.ptr_type(Ty::Void),
                    loc,
                ))
            }
            "trap" | "unreachable" => self.builtin_call(Builtin::Trap, &[], Ty::Void, loc),
            "debugtrap" => self.builtin_call(Builtin::Debugtrap, &[], Ty::Void, loc),
            _ => {
                // Other builtins with declared library equivalents.
                let base = self.text.intern(base);
                if let Ok(e) = self.lookup_value(base, loc)
                    && matches!(e.ty, Ty::Func(_))
                {
                    return Ok(e);
                }
                // Legacy fallback for other fortified library functions.
                // The three memory _chk names are handled above and must
                // retain their bound. This fallback remains a compatibility
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
        }
    }

    /// These explicit names have standard prototypes independent of whether
    /// the translation unit declares the corresponding ordinary libc symbol.
    fn memory_builtin(&mut self, name: &str, checked: bool, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(if checked { 4 } else { 3 }, loc)?;
        let fill = name == "memset";
        for (i, arg) in args.iter().enumerate() {
            if i >= 2 || (i == 1 && fill) {
                if !arg.ty.is_arith() {
                    return Err(Error::new(
                        arg.loc,
                        "memory builtin needs an arithmetic argument",
                    ));
                }
                continue;
            }
            let null = arg.ty.is_integer()
                && eval_constant_with_layout(arg, false, self.c_layout).and_then(Const::int)
                    == Some(0);
            if (!arg.ty.is_ptr() && !null) || matches!(arg.ty.pointee(), Some(Ty::Func(_))) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin needs an object pointer",
                ));
            }
            let pointee = arg.sugar.pointee(&self.typedefs);
            if pointee.is_volatile(&self.typedefs) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin cannot discard volatile qualification",
                ));
            }
            if i == 0 && name != "memcmp" && memory_builtin_const(&pointee, &self.typedefs) {
                return Err(Error::new(
                    arg.loc,
                    "memory builtin destination must not be const",
                ));
            }
        }
        let ptr = self.ptr_type(Ty::Void);
        let mut params = vec![ptr, if fill { INT } else { ptr }, ULONG];
        if checked {
            params.push(ULONG);
        }
        let args = args
            .into_iter()
            .zip(&params)
            .map(|(arg, ty)| self.convert_for_assign(arg, ty))
            .collect::<R<Vec<_>>>()?;
        let id = match name {
            "memcpy" => Intrinsic::Memcpy,
            "memmove" => Intrinsic::Memmove,
            "memcmp" => Intrinsic::Memcmp,
            _ => Intrinsic::Memset,
        };
        let result = if name == "memcmp" { INT } else { ptr };
        Ok(mk(
            ExprKind::Builtin(Builtin::Memory(id), self.slice(&args)),
            result,
            loc,
        ))
    }

    /// Validate constant memory orders; runtime values select seq_cst in IR.
    fn atomic_orders(
        &self,
        args: &[Expr<'a>],
        success: usize,
        failure: Option<usize>,
        load: bool,
        store: bool,
        loc: Loc,
    ) -> R<()> {
        let valid = |n| (0..=5).contains(&n);
        let order = eval_int_with_layout(&args[success], self.c_layout);
        if order.is_some_and(|n| {
            !valid(n) || (load && matches!(n, 3 | 4)) || (store && matches!(n, 1 | 2 | 4))
        }) {
            return Err(Error::new(loc, "invalid atomic memory order"));
        }
        if let Some(failure) = failure {
            let fail = eval_int_with_layout(&args[failure], self.c_layout);
            if fail.is_some_and(|n| !valid(n) || matches!(n, 3 | 4)) {
                return Err(Error::new(loc, "invalid compare-exchange failure order"));
            }
            if let (Some(success), Some(fail)) = (order, fail) {
                let allowed = match success {
                    0 | 3 => fail == 0,
                    1 => fail <= 1,
                    2 | 4 => fail <= 2,
                    _ => true,
                };
                if !allowed {
                    return Err(Error::new(
                        loc,
                        "compare-exchange failure order is stronger than success order",
                    ));
                }
            }
        }
        Ok(())
    }

    fn atomic_writable(&self, arg: &Expr<'a>, loc: Loc) -> R<()> {
        fn is_const(s: &Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
            match s {
                Sugar::Const(_) => true,
                Sugar::Volatile(s) | Sugar::Restrict(s) => is_const(s, defs),
                Sugar::Typedef(i) => is_const(&defs[*i].sugar, defs),
                _ => false,
            }
        }
        if is_const(&arg.sugar.pointee(&self.typedefs), &self.typedefs) {
            return Err(Error::new(
                loc,
                "atomic destination must point to a non-const object",
            ));
        }
        Ok(())
    }

    /// GCC's `__atomic_load_n` and `__atomic_store_n` on a scalar.
    /// The atomic builtins: GCC's `__atomic_*` (with an order) and
    /// `__sync_*` (seq_cst), and clang's `__c11_atomic_*`, which
    /// `<stdatomic.h>` expands to. See [`Builtin::AtomicLoad`] for the
    /// shape their arguments are put in.
    pub(super) fn atomic(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        let (op, sync) = match name.strip_prefix("__sync_") {
            Some(op) => (op, true),
            None => (
                name.strip_prefix("__c11_atomic_")
                    .or_else(|| name.strip_prefix("__atomic_"))
                    .unwrap(),
                false,
            ),
        };
        let mut args = self.builtin_args_any()?;
        let unsupported = || Error::new(loc, format!("`{name}` is not supported"));
        let order = |v: i64| mk(ExprKind::Int(v), INT, loc);
        // `__sync_*` are seq_cst, except the lock ones.
        if sync {
            match op {
                "lock_test_and_set" => args.push(order(2)),
                "lock_release" => {
                    args.push(mk(ExprKind::Int(0), INT, loc));
                    args.push(order(3));
                }
                _ => args.push(order(5)),
            }
        }
        let want = |args: &[Expr<'a>], n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(Error::new(loc, format!("`{name}` takes {n} argument(s)")))
            }
        };
        let rmw = |op: &str| {
            Some(match op {
                "add" => RmwOp::Add,
                "sub" => RmwOp::Sub,
                "and" => RmwOp::And,
                "or" => RmwOp::Or,
                "xor" => RmwOp::Xor,
                _ => return None,
            })
        };
        if name.starts_with("__atomic_")
            && matches!(op, "load" | "store" | "exchange" | "compare_exchange")
        {
            want(
                &args,
                match op {
                    "load" | "store" => 3,
                    "exchange" => 4,
                    _ => 6,
                },
            )?;
        }
        if name.starts_with("__atomic_")
            && matches!(
                (op, args.len()),
                ("load", 3) | ("store", 3) | ("exchange", 4) | ("compare_exchange", 6)
            )
        {
            let ty = args[0]
                .ty
                .pointee()
                .map(|t| *t.unatomic())
                .ok_or_else(|| Error::new(loc, "generic atomic needs an object pointer"))?;
            let size = self.types().size(&ty);
            let align = if let ExprKind::Addr(obj) = args[0].kind {
                self.expr_alignment(obj)
            } else {
                self.type_alignment(&ty, &args[0].sugar.pointee(&self.typedefs))
            };
            if self.types().atomic_size(&ty, align).is_none() {
                return Err(Error::new(
                    loc,
                    "generic atomic requires a naturally aligned 1/2/4/8-byte object",
                ));
            }
            let kind = match op {
                "load" => GenericAtomicOp::Load,
                "store" => GenericAtomicOp::Store,
                "exchange" => GenericAtomicOp::Exchange,
                _ => GenericAtomicOp::CompareExchange,
            };
            let buffers = if matches!(
                kind,
                GenericAtomicOp::Exchange | GenericAtomicOp::CompareExchange
            ) {
                1..3
            } else {
                1..2
            };
            for i in buffers {
                if args[i].ty.pointee().copied() != Some(ty) {
                    return Err(Error::new(
                        loc,
                        "generic atomic buffer must point to the object type",
                    ));
                }
            }
            match kind {
                GenericAtomicOp::Load => self.atomic_writable(&args[1], loc)?,
                GenericAtomicOp::Store => self.atomic_writable(&args[0], loc)?,
                GenericAtomicOp::Exchange => {
                    self.atomic_writable(&args[0], loc)?;
                    self.atomic_writable(&args[2], loc)?;
                }
                GenericAtomicOp::CompareExchange => {
                    self.atomic_writable(&args[0], loc)?;
                    self.atomic_writable(&args[1], loc)?;
                }
            }
            self.atomic_orders(
                &args,
                if kind == GenericAtomicOp::CompareExchange {
                    4
                } else {
                    args.len() - 1
                },
                if kind == GenericAtomicOp::CompareExchange {
                    Some(5)
                } else {
                    None
                },
                kind == GenericAtomicOp::Load,
                kind == GenericAtomicOp::Store,
                loc,
            )?;
            return Ok(mk(
                ExprKind::Builtin(
                    Builtin::AtomicGeneric {
                        kind,
                        size: size as u8,
                    },
                    self.slice(&args),
                ),
                if kind == GenericAtomicOp::CompareExchange {
                    Ty::Int(IntKind::Bool)
                } else {
                    Ty::Void
                },
                loc,
            ));
        }
        // The operation, and how many arguments it has once in shape.
        let (builtin, n) = match op {
            "synchronize" | "thread_fence" | "signal_fence" => {
                // A signal fence only has to stop the compiler; a real
                // fence is stronger than needed but correct.
                want(&args, 1)?;
                self.atomic_orders(&args, 0, None, false, false, loc)?;
                return Ok(mk(
                    ExprKind::Builtin(Builtin::Fence, self.slice(&args)),
                    Ty::Void,
                    loc,
                ));
            }
            "always_lock_free" | "is_lock_free" => {
                let c11 = name.starts_with("__c11_");
                want(&args, if c11 { 1 } else { 2 })?;
                let size = eval_int_with_layout(&args[0], self.c_layout);
                let align = if c11 {
                    size.unwrap_or(0).max(0) as u64
                } else if let ExprKind::Addr(obj) = args[1].kind {
                    self.expr_alignment(obj)
                } else {
                    args[1]
                        .ty
                        .pointee()
                        .map(|t| self.type_alignment(t, &args[1].sugar.pointee(&self.typedefs)))
                        .unwrap_or(1)
                };
                let free = matches!(size, Some(1 | 2 | 4 | 8))
                    && (c11
                        || eval_int_with_layout(&args[1], self.c_layout) == Some(0)
                        || size.is_some_and(|n| align >= n as u64));
                let result = mk(ExprKind::Int(i64::from(free)), Ty::Int(IntKind::Bool), loc);
                if op == "always_lock_free" {
                    return Ok(result);
                }
                let mut evaluated = vec![result];
                evaluated.extend(args);
                return Ok(mk(
                    ExprKind::Builtin(Builtin::Hint, self.slice(&evaluated)),
                    result.ty,
                    loc,
                ));
            }
            "load_n" | "load" if args.len() == 2 => (Builtin::AtomicLoad, 2),
            "store_n" | "store" | "init" | "lock_release" if args.len() >= 2 => {
                if op == "init" {
                    args.push(order(5));
                }
                (Builtin::AtomicStore, 3)
            }
            "exchange_n" | "exchange" | "lock_test_and_set" if args.len() == 3 => (
                Builtin::AtomicRmw {
                    op: RmwOp::Xchg,
                    new: false,
                },
                3,
            ),
            "val_compare_and_swap" => (Builtin::AtomicCas, 4),
            "bool_compare_and_swap" => (Builtin::AtomicCasBool, 4),
            "compare_exchange_n" | "compare_exchange_strong" | "compare_exchange_weak" => {
                // (ptr, expected, desired[, weak], success, failure): the
                // failure order is never stronger, so the success one does.
                let weak_arg = op == "compare_exchange_n";
                want(&args, if weak_arg { 6 } else { 5 })?;
                if weak_arg {
                    let weak = args.remove(3);
                    args.push(weak);
                }
                (Builtin::AtomicCompareExchange, args.len())
            }
            "test_and_set" | "clear" => {
                // On a byte: set to 1 (giving whether it was set) or 0.
                want(&args, 2)?;
                if !args[0].ty.is_ptr() {
                    return Err(Error::new(loc, "atomic flag operation needs a pointer"));
                }
                self.atomic_writable(&args[0], loc)?;
                self.atomic_orders(&args, 1, None, false, op == "clear", loc)?;
                let ptr = args.remove(0);
                let ty = self.ptr_type(Ty::Int(IntKind::UChar));
                let ptr = self.cast(ptr, ty);
                let value = mk(ExprKind::Int(i64::from(op == "test_and_set")), INT, loc);
                let value = self.cast(value, Ty::Int(IntKind::UChar));
                let args = vec![ptr, value, args.remove(0)];
                if op == "clear" {
                    return Ok(mk(
                        ExprKind::Builtin(Builtin::AtomicStore, self.slice(&args)),
                        Ty::Void,
                        loc,
                    ));
                }
                let builtin = Builtin::AtomicRmw {
                    op: RmwOp::Xchg,
                    new: false,
                };
                let old = mk(
                    ExprKind::Builtin(builtin, self.slice(&args)),
                    Ty::Int(IntKind::UChar),
                    loc,
                );
                return Ok(self.cast(old, Ty::Int(IntKind::Bool)));
            }
            _ => {
                // fetch_add, add_fetch, fetch_and_add, add_and_fetch, ...
                let (rmw_op, new) = if let Some(o) = op.strip_prefix("fetch_and_") {
                    (o, false)
                } else if let Some(o) = op.strip_prefix("fetch_") {
                    (o, false)
                } else if let Some(o) = op.strip_suffix("_and_fetch") {
                    (o, true)
                } else if let Some(o) = op.strip_suffix("_fetch") {
                    (o, true)
                } else {
                    return Err(unsupported());
                };
                let ptr_ty = args.first().map(|a| a.ty);
                let signed = ptr_ty
                    .as_ref()
                    .and_then(|t| t.pointee())
                    .is_some_and(|t| t.unatomic().is_signed());
                let op = match (rmw_op, new) {
                    ("min", false) if signed => RmwOp::Smin,
                    ("max", false) if signed => RmwOp::Smax,
                    ("min", false) => RmwOp::Umin,
                    ("max", false) => RmwOp::Umax,
                    _ => rmw(rmw_op).ok_or_else(unsupported)?,
                };
                (Builtin::AtomicRmw { op, new }, 3)
            }
        };
        want(&args, n)?;
        let pointee = args[0].ty.pointee();
        if name.starts_with("__c11_atomic_") {
            if !matches!(pointee, Some(Ty::Atomic(_))) {
                return Err(Error::new(
                    loc,
                    "C11 atomic builtin needs a pointer to an _Atomic object",
                ));
            }
        } else if !pointee.is_some_and(|t| t.unatomic().is_integer() || t.unatomic().is_ptr()) {
            return Err(Error::new(
                loc,
                "scalar GCC atomic builtin needs an integer or pointer object",
            ));
        }
        let Some(ty) = args[0].ty.pointee().map(|t| *t.unatomic()) else {
            return Err(Error::new(loc, format!("`{name}` needs a pointer")));
        };
        if !(ty.is_integer() || ty.is_ptr() || matches!(ty, Ty::Float | Ty::Double))
            || !matches!(self.types().size(&ty), 1 | 2 | 4 | 8)
        {
            return Err(Error::new(
                loc,
                format!("`{name}` requires a supported 1/2/4/8-byte scalar"),
            ));
        }
        self.atomic_orders(
            &args,
            if builtin == Builtin::AtomicCompareExchange {
                3
            } else {
                args.len() - 1
            },
            if builtin == Builtin::AtomicCompareExchange {
                Some(4)
            } else {
                None
            },
            builtin == Builtin::AtomicLoad,
            builtin == Builtin::AtomicStore,
            loc,
        )?;
        if builtin != Builtin::AtomicLoad {
            self.atomic_writable(&args[0], loc)?;
        }
        if builtin == Builtin::AtomicCompareExchange {
            self.atomic_writable(&args[1], loc)?;
        }
        // Arithmetic on a pointer is on its address, in bytes.
        let arith = matches!(builtin, Builtin::AtomicRmw { op, .. } if op != RmwOp::Xchg);
        if arith && ty.is_float() {
            return Err(Error::new(
                loc,
                "floating fetch arithmetic builtin is not supported; use C compound arithmetic",
            ));
        }
        let align = if let ExprKind::Addr(obj) = args[0].kind {
            self.expr_alignment(obj)
        } else {
            self.type_alignment(&ty, &args[0].sugar.pointee(&self.typedefs))
        };
        if self.types().atomic_size(&ty, align).is_none() {
            return Err(Error::new(loc, "atomic object is not naturally aligned"));
        }
        let value_ty = if arith && ty.is_ptr() { ULONG } else { ty };
        let pointee = ty;
        let value_args = match builtin {
            Builtin::AtomicLoad => 0..0,
            Builtin::AtomicCas | Builtin::AtomicCasBool => 1..3,
            Builtin::AtomicCompareExchange => 2..3,
            _ => 1..2,
        };
        for i in value_args {
            let v = args[i];
            args[i] = self.convert_for_assign(v, &value_ty)?;
        }
        if builtin == Builtin::AtomicCompareExchange && args[1].ty.pointee().copied() != Some(ty) {
            return Err(Error::new(
                loc,
                format!("`{name}` needs a pointer to the expected value"),
            ));
        }
        let ty = match builtin {
            // A store's value is only needed in an assignment.
            Builtin::AtomicStore => Ty::Void,
            Builtin::AtomicCasBool | Builtin::AtomicCompareExchange => Ty::Int(IntKind::Bool),
            _ => value_ty,
        };
        let sugar = args[0].sugar.pointee(&self.typedefs);
        let result = mk(ExprKind::Builtin(builtin, self.slice(&args)), ty, loc);
        // Back from the address to the pointer.
        let result = if value_ty != pointee && result.ty == value_ty {
            self.cast(result, pointee)
        } else {
            result
        };
        Ok(if result.ty.is_ptr() {
            result.with_sugar(sugar)
        } else {
            result
        })
    }

    /// `(a, b, ...)`, as values.
    /// The classification and comparison builtins `<math.h>` uses on
    /// glibc (`isnan(x)` is `__builtin_isnan(x)`), as C: each operand is
    /// evaluated once, into a made-up local, and tested with compares,
    /// which are false for NaN except `!=`. `isfinite(x)` is
    /// `fabs(x) < INFINITY`, `isnan(x)` is `x != x`.
    fn float_class(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let mut args = self.builtin_args_any()?;
        let (n, first) = match base {
            "fpclassify" => (6, 5),
            "isgreater" | "isgreaterequal" | "isless" | "islessequal" | "islessgreater"
            | "isunordered" => (2, 0),
            _ => (1, 0),
        };
        if args.len() != n {
            return Err(Error::new(loc, format!("builtin takes {n} argument(s)")));
        }
        if self.fs.is_none() {
            return Err(Error::new(
                loc,
                format!("`__builtin_{base}` outside a function"),
            ));
        }
        // The operands, each in a local; `sets` assigns them.
        let mut sets = Vec::new();
        let mut operands = Vec::new();
        for arg in args.drain(first..) {
            if !arg.ty.is_arith() || arg.ty.is_complex() {
                return Err(Error::new(
                    loc,
                    format!("`__builtin_{base}` of a non-number"),
                ));
            }
            let ty = if arg.ty == Ty::Float {
                Ty::Float
            } else {
                Ty::Double
            };
            let arg = self.cast(arg, ty);
            let name = self.unique_name("fclass");
            let t = self.new_local(name, ty, Sugar::Plain, false);
            sets.push(self.assignment("=", mk(ExprKind::Local(t), ty, loc), arg, loc)?);
            operands.push(mk(ExprKind::Local(t), ty, loc));
        }
        let x = operands[0];
        let ty = x.ty;
        let float = |v: f64| mk(ExprKind::Float(v), ty, loc);
        let fabs = mk(ExprKind::Builtin(Builtin::Fabs, self.slice(&[x])), ty, loc);
        let int = |v: i64| mk(ExprKind::Int(v), INT, loc);
        let min_normal = if ty == Ty::Float {
            f64::from(f32::MIN_POSITIVE)
        } else {
            f64::MIN_POSITIVE
        };
        let cmp = |this: &mut Self, p: &str, a: Expr<'a>, b: Expr<'a>| this.binop(p, a, b, loc);
        let result = match base {
            "isnan" => cmp(self, "!=", x, x)?,
            "isinf" => cmp(self, "==", fabs, float(f64::INFINITY))?,
            "isfinite" => cmp(self, "<", fabs, float(f64::INFINITY))?,
            "isnormal" => {
                let low = cmp(self, ">=", fabs, float(min_normal))?;
                let high = cmp(self, "<", fabs, float(f64::INFINITY))?;
                cmp(self, "&&", low, high)?
            }
            "isinf_sign" => {
                let pos = cmp(self, "==", x, float(f64::INFINITY))?;
                let neg = cmp(self, "==", x, float(f64::NEG_INFINITY))?;
                let neg = self.conditional_of(neg, int(-1), int(0), loc)?;
                self.conditional_of(pos, int(1), neg, loc)?
            }
            "signbit" | "signbitf" | "signbitl" => mk(
                ExprKind::Builtin(Builtin::Signbit, self.slice(&[x])),
                INT,
                loc,
            ),
            // fpclassify(nan, infinite, normal, subnormal, zero, x)
            "fpclassify" => {
                let [nan, inf, normal, subnormal, zero]: [Expr; 5] =
                    args.try_into().expect("five classes");
                let is_zero = cmp(self, "==", x, float(0.0))?;
                let tiny = self.conditional_of(is_zero, zero, subnormal, loc)?;
                let is_normal = cmp(self, ">=", fabs, float(min_normal))?;
                let finite = self.conditional_of(is_normal, normal, tiny, loc)?;
                let is_inf = cmp(self, "==", fabs, float(f64::INFINITY))?;
                let number = self.conditional_of(is_inf, inf, finite, loc)?;
                let is_number = cmp(self, "==", x, x)?;
                self.conditional_of(is_number, number, nan, loc)?
            }
            _ => {
                let y = operands[1];
                match base {
                    "isgreater" => cmp(self, ">", x, y)?,
                    "isgreaterequal" => cmp(self, ">=", x, y)?,
                    "isless" => cmp(self, "<", x, y)?,
                    "islessequal" => cmp(self, "<=", x, y)?,
                    "islessgreater" => {
                        let lt = cmp(self, "<", x, y)?;
                        let gt = cmp(self, ">", x, y)?;
                        cmp(self, "||", lt, gt)?
                    }
                    _ => {
                        let a = cmp(self, "!=", x, x)?;
                        let b = cmp(self, "!=", y, y)?;
                        cmp(self, "||", a, b)?
                    }
                }
            }
        };
        let ty = result.ty;
        Ok(sets.into_iter().rev().fold(result, |e, set| {
            mk(ExprKind::Comma(self.node(set), self.node(e)), ty, loc)
        }))
    }

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
