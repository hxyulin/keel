//! Optimization hint builtins.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_prefetch`.
    pub(super) fn prefetch(&mut self, loc: Loc) -> R<Expr<'a>> {
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

    /// `__builtin_expect_with_probability`.
    pub(super) fn expect_with_probability(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(3, loc)?;
        let mut args = args.into_iter();
        let value = self.convert_for_assign(args.next().unwrap(), &LONG)?;
        let expected = self.convert_for_assign(args.next().unwrap(), &LONG)?;
        let probability = self.convert_for_assign(args.next().unwrap(), &Ty::Double)?;
        let Some(Const::Float(p)) = eval_constant_with_layout(&probability, true, self.c_layout)
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

    /// `__builtin_assume_aligned`.
    pub(super) fn assume_aligned(&mut self, loc: Loc) -> R<Expr<'a>> {
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
        let Some(n) =
            eval_constant_with_layout(&alignment, false, self.c_layout).and_then(Const::int)
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
}
