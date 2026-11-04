//! x86 builtins: CPU queries and the reviewed intrinsic bindings.

use super::*;

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

    /// Feature and data-layout checks shared by every x86 builtin name.
    pub(super) fn x86_preconditions(&mut self, name: &str, base: &str, loc: Loc) -> R<()> {
        if let Some(sets) = crate::x86_builtins::required_features(base) {
            self.check_x86_features(name, sets, loc);
        }
        if base.starts_with("ia32_")
            || base.starts_with("keel_sse2_")
            || base.starts_with("keel_avx2_")
        {
            self.require_native_layout(loc, "x86 builtins")?;
        }
        Ok(())
    }

    /// `__builtin_cpu_supports` and `__builtin_cpu_init`.
    pub(super) fn cpu_builtin(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
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
                ExprKind::Addr(inner) if matches!(inner.kind, ExprKind::Str(_)) => argument = inner,
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86CpuSupports(feature), self.slice(&[])),
            INT,
            loc,
        ))
    }

    pub(super) fn x86_reduction(
        &mut self,
        op: keel_ir::VectorReduceOp,
        shape: keel_ir::VectorType,
        loc: Loc,
    ) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::VectorReduce(op), self.slice(&args)),
            result,
            loc,
        ))
    }

    pub(super) fn x86_value(
        &mut self,
        id: Intrinsic,
        controls: &[crate::x86_builtins::ImmediateControl],
        loc: Loc,
    ) -> R<Expr<'a>> {
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
                        && (control.allowed_values.is_empty() || control.allowed_values.contains(n))
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86Value(id), self.slice(&args)),
            result,
            loc,
        ))
    }

    pub(super) fn x86_packed_conversion(&mut self, id: Intrinsic, loc: Loc) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86PackedConversion(id), self.slice(&args)),
            result,
            loc,
        ))
    }

    pub(super) fn x86_packed_shift(
        &mut self,
        id: Intrinsic,
        shape: keel_ir::VectorType,
        immediate: bool,
        loc: Loc,
    ) -> R<Expr<'a>> {
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
            let Some(n) =
                integer_constant_control(&args[1], self.c_layout).filter(|n| (0..=255).contains(n))
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86PackedShift(id), self.slice(&args)),
            ty,
            loc,
        ))
    }

    pub(super) fn x86_saturating(
        &mut self,
        id: Intrinsic,
        unsigned: bool,
        word: bool,
        loc: Loc,
    ) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86Saturating(id), self.slice(&args)),
            args[0].ty,
            loc,
        ))
    }

    pub(super) fn pclmulqdq(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::Pclmulqdq(id, n as u8), self.slice(&args[..2])),
            args[0].ty,
            loc,
        ))
    }

    pub(super) fn sha256(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let id = match base {
            "ia32_sha256msg1" => keel_ir::Intrinsic::X86Sha256msg1,
            "ia32_sha256msg2" => keel_ir::Intrinsic::X86Sha256msg2,
            _ => keel_ir::Intrinsic::X86Sha256rnds2,
        };
        let args = self.builtin_args(id.info().params.len(), loc)?;
        if self.target != Target::X86_64LinuxGnu
            || args
                .iter()
                .any(|arg| !matches!(arg.ty, Ty::Vector(Ty::Int(_), keel_ir::VectorType::I32x4)))
        {
            return Err(Error::new(
                loc,
                "SHA-256 requires i32x4 integer vectors on x86-64",
            ));
        }
        Ok(mk(
            ExprKind::Builtin(Builtin::X86Value(id), self.slice(&args)),
            args[0].ty,
            loc,
        ))
    }

    pub(super) fn x86_aes(&mut self, id: Intrinsic, loc: Loc) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::X86Aes(id), self.slice(&args)),
            ty,
            loc,
        ))
    }

    /// `__builtin_ia32_cmpps256` and `__builtin_ia32_cmppd256`.
    pub(super) fn avx_compare(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
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
        Ok(mk(
            ExprKind::Builtin(Builtin::VectorFloatCompare(cc), self.slice(&args[..2])),
            ty,
            loc,
        ))
    }
}
