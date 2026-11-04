//! Atomic builtins: `__atomic_*`, `__sync_*` and `__c11_atomic_*`.

use super::*;

impl<'a> Parser<'a> {
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
    pub(in crate::expr) fn atomic(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
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
}
