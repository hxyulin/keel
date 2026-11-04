//! Floating-point builtins: constants, classification and `fabs`.

use super::*;

impl<'a> Parser<'a> {
    /// `__builtin_inf`, `__builtin_huge_val` and `__builtin_nan`, in each width.
    pub(super) fn float_constant(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
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

    /// `__builtin_fabs` in each width.
    pub(super) fn fabs(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let ty = if base == "fabsf" {
            Ty::Float
        } else {
            Ty::Double
        };
        self.builtin_call(Builtin::Fabs, std::slice::from_ref(&ty), ty, loc)
    }

    /// `__builtin_complex`.
    pub(super) fn complex(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(2, loc)?;
        let elem = args[0].ty;
        if !matches!(elem, Ty::Float | Ty::Double) || elem != args[1].ty {
            return Err(Error::new(
                loc,
                "__builtin_complex requires two matching float or double operands",
            ));
        }
        let ty = Ty::Complex(self.type_ref(elem));
        Ok(mk(
            ExprKind::Builtin(Builtin::Complex, self.slice(&args)),
            ty,
            loc,
        ))
    }

    /// The classification and comparison builtins `<math.h>` uses on
    /// glibc (`isnan(x)` is `__builtin_isnan(x)`), as C: each operand is
    /// evaluated once, into a made-up local, and tested with compares,
    /// which are false for NaN except `!=`. `isfinite(x)` is
    /// `fabs(x) < INFINITY`, `isnan(x)` is `x != x`.
    pub(super) fn float_class(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
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
}
