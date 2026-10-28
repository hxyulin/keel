//! Expressions, with C's implicit conversions made explicit as they are
//! parsed.

use crate::ast::*;
use crate::eval::{
    Const, eval_constant, eval_constant_with_layout, eval_int, eval_int_with_layout,
    fits_i64_with_layout, normalize_with_layout,
};
use crate::lex::Tok;
use crate::parse::{Parser, R};
use crate::types::*;
use crate::{Error, Loc};
use keel_ir::Target;
use keel_ir::{Intrinsic, OverflowOp, RmwOp};

/// Bounds the number of frame-record loads emitted by one C builtin.
const MAX_FRAME_ADDRESS_DEPTH: i64 = 64;

pub(crate) fn mk<'a>(kind: ExprKind<'a>, ty: Ty<'a>, loc: Loc) -> Expr<'a> {
    Expr {
        kind,
        ty,
        loc,
        sugar: Sugar::Plain,
    }
}

/// The general constant evaluator stores integer values in 64 bits. Do not
/// use it as a representability proof through i128 (or extended FP/pointer)
/// intermediates, even when the final scalar type is narrower.
// Native-only proof boundary; migrating the scalar evaluator does not migrate
// the vector range/shape checks that consume this proof.
fn broadcast_constant(e: &Expr<'_>, layout: CDataLayout) -> Option<Const> {
    if layout != CDataLayout::NATIVE_LP64 {
        return None;
    }
    fn supported(e: &Expr<'_>) -> bool {
        if !(e.ty.int_kind().is_some_and(|k| k.size() <= 8)
            || matches!(e.ty, Ty::Float | Ty::Double))
        {
            return false;
        }
        match e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) => true,
            ExprKind::Cast(a) | ExprKind::Unary(_, a) => supported(a),
            ExprKind::Binary(_, a, b) | ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
                supported(a) && supported(b)
            }
            ExprKind::Cond(a, b, c) => supported(a) && supported(b) && supported(c),
            _ => false,
        }
    }
    supported(e).then(|| eval_constant(e, false)).flatten()
}

/// Integer controls are bounded C ICEs, not arbitrary folds by the general
/// evaluator (which deliberately wraps shifts/overflow and saturates FP casts).
fn integer_constant_control(e: &Expr<'_>, layout: CDataLayout) -> Option<i64> {
    if layout != CDataLayout::NATIVE_LP64 {
        return None;
    }
    fn integer(e: &Expr<'_>) -> bool {
        e.ty.int_kind().is_some_and(|k| k.size() <= 8)
    }
    fn shape(e: &Expr<'_>) -> bool {
        if !integer(e) {
            return false;
        }
        match e.kind {
            ExprKind::Int(_) => true,
            ExprKind::Cast(a) if matches!(a.kind, ExprKind::Float(_)) => {
                matches!(a.ty, Ty::Float | Ty::Double)
            }
            ExprKind::Cast(a) | ExprKind::Unary(_, a) => shape(a),
            ExprKind::Binary(_, a, b) | ExprKind::LogAnd(a, b) | ExprKind::LogOr(a, b) => {
                shape(a) && shape(b)
            }
            ExprKind::Cond(a, b, c) => shape(a) && shape(b) && shape(c),
            _ => false,
        }
    }
    fn bounds(ty: Ty<'_>) -> (i128, i128) {
        let k = ty.int_kind().unwrap();
        let bits = k.size() * 8;
        if k.signed() {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        }
    }
    fn checked(e: &Expr<'_>) -> Option<i64> {
        match e.kind {
            ExprKind::Int(_) => {}
            ExprKind::Cast(a) => {
                if let ExprKind::Float(f) = a.kind {
                    let (min, max) = bounds(e.ty);
                    let f = f.trunc();
                    // Exclusive upper power-of-two limit avoids f64 rounding
                    // i64::MAX/u64::MAX upward into an unrepresentable value.
                    if !f.is_finite()
                        || (e.ty != Ty::Int(IntKind::Bool)
                            && (f < min as f64 || f >= (max + 1) as f64))
                    {
                        return None;
                    }
                } else {
                    checked(a)?;
                }
            }
            ExprKind::Unary(op, a) => {
                let n = checked(a)?;
                if op == UnOp::Neg && e.ty.is_signed() && i128::from(n) == bounds(e.ty).0 {
                    return None;
                }
            }
            ExprKind::Binary(op, a, b) => {
                let x = checked(a)?;
                let y = checked(b)?;
                let signed = a.ty.is_signed();
                let (min, max) = bounds(a.ty);
                let exact = match op {
                    BinOp::Add if signed => Some(i128::from(x) + i128::from(y)),
                    BinOp::Sub if signed => Some(i128::from(x) - i128::from(y)),
                    BinOp::Mul if signed => Some(i128::from(x) * i128::from(y)),
                    BinOp::Div | BinOp::Rem if y == 0 => return None,
                    BinOp::Div | BinOp::Rem if signed && i128::from(x) == min && y == -1 => {
                        return None;
                    }
                    BinOp::Shl | BinOp::Shr => {
                        if y < 0 || y >= (a.ty.int_kind()?.size() * 8) as i64 {
                            return None;
                        }
                        if op == BinOp::Shl && signed {
                            if x < 0 {
                                return None;
                            }
                            Some(i128::from(x) << y)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if exact.is_some_and(|n| n < min || n > max) {
                    return None;
                }
            }
            ExprKind::LogAnd(a, b) => {
                if checked(a)? != 0 {
                    checked(b)?;
                }
            }
            ExprKind::LogOr(a, b) => {
                if checked(a)? == 0 {
                    checked(b)?;
                }
            }
            ExprKind::Cond(c, a, b) => {
                checked(if checked(c)? != 0 { a } else { b })?;
            }
            _ => return None,
        }
        eval_int(e)
    }
    shape(e).then(|| checked(e)).flatten()
}

fn binary_prec(p: &str) -> Option<u8> {
    Some(match p {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    /// A full expression, commas included.
    pub(crate) fn expr(&mut self) -> R<Expr<'a>> {
        let mut e = self.assign()?;
        while self.is(",") {
            let loc = self.loc();
            self.advance();
            let r = self.assign()?;
            let r = self.rvalue(r);
            let ty = r.ty;
            let sugar = *r.sugar.unqualified(&self.typedefs);
            e = mk(ExprKind::Comma(self.node(e), self.node(r)), ty, loc).with_sugar(sugar);
        }
        Ok(e)
    }

    pub(crate) fn assign(&mut self) -> R<Expr<'a>> {
        let lhs = self.conditional()?;
        let Tok::Punct(p) = self.peek() else {
            return Ok(lhs);
        };
        if !matches!(
            p,
            "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
        ) {
            return Ok(lhs);
        }
        let loc = self.loc();
        self.advance();
        let rhs = self.assign()?;
        self.assignment(p, lhs, rhs, loc)
    }

    fn assignment(&mut self, op: &str, lhs: Expr<'a>, rhs: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        self.check_modifiable_lvalue(&lhs)?;
        if let Ty::Atomic(&ty) = lhs.ty {
            if ty.is_float() && op != "=" {
                let bin = match op {
                    "+=" => BinOp::Add,
                    "-=" => BinOp::Sub,
                    "*=" => BinOp::Mul,
                    "/=" => BinOp::Div,
                    _ => return Err(Error::new(loc, "invalid atomic floating arithmetic")),
                };
                let rhs = self.rvalue(rhs);
                // Use the usual arithmetic type, then round to the object type.
                let calc = if rhs.ty == Ty::Double || ty == Ty::Double {
                    Ty::Double
                } else {
                    Ty::Float
                };
                let rhs = self.cast(rhs, calc);
                let args = vec![
                    self.address_of_atomic(lhs),
                    rhs,
                    mk(ExprKind::Int(5), INT, loc),
                ];
                return Ok(mk(
                    ExprKind::Builtin(
                        Builtin::AtomicFloatRmw { op: bin, new: true },
                        self.slice(&args),
                    ),
                    ty,
                    loc,
                ));
            }
            let rmw = match op {
                "=" => None,
                "+=" => Some(RmwOp::Add),
                "-=" => Some(RmwOp::Sub),
                "&=" => Some(RmwOp::And),
                "|=" => Some(RmwOp::Or),
                "^=" => Some(RmwOp::Xor),
                _ => {
                    return Err(Error::new(
                        loc,
                        format!("`{op}` on an `_Atomic` object is not supported"),
                    ));
                }
            };
            if rmw.is_some() && !ty.is_integer() {
                return Err(Error::new(
                    loc,
                    format!("`{op}` on an `_Atomic` pointer is not supported"),
                ));
            }
            let rhs = self.rvalue(rhs);
            let rhs = self.convert_for_assign(rhs, &ty)?;
            let builtin = match rmw {
                None => Builtin::AtomicStore,
                Some(op) => Builtin::AtomicRmw { op, new: true },
            };
            let sugar = *lhs.sugar.unqualified(&self.typedefs);
            let args = vec![
                self.address_of_atomic(lhs),
                rhs,
                mk(ExprKind::Int(5), INT, loc),
            ];
            return Ok(mk(ExprKind::Builtin(builtin, self.slice(&args)), ty, loc).with_sugar(sugar));
        }
        let ty = lhs.ty;
        let result_sugar = *lhs.sugar.unqualified(&self.typedefs);
        if op == "=" {
            let rhs = self.convert_for_assign(rhs, &ty)?;
            let sugar = *lhs.sugar.unqualified(&self.typedefs);
            return Ok(
                mk(ExprKind::Assign(self.node(lhs), self.node(rhs)), ty, loc).with_sugar(sugar),
            );
        }
        let rhs = self.rvalue(rhs);
        let bin = &op[..op.len() - 1];
        if ty.is_ptr() && (bin == "+" || bin == "-") && rhs.ty.is_integer() {
            let size = self.elem_size(&ty, loc)?;
            let mut n = self.cast(rhs, LONG);
            let (scaled, size) = self.scale_vm_index(n, &ty, size, loc);
            n = scaled;
            if bin == "-" {
                n = mk(ExprKind::Unary(UnOp::Neg, self.node(n)), LONG, loc);
            }
            return Ok(mk(
                ExprKind::PtrOpAssign {
                    lhs: self.node(lhs),
                    rhs: self.node(n),
                    size,
                },
                ty,
                loc,
            )
            .with_sugar(result_sugar));
        }
        if ty.is_vector() || rhs.ty.is_vector() {
            // Validate through the same operator rules before building a single-evaluation assignment.
            if !ty.is_vector() {
                return Err(Error::new(
                    loc,
                    "scalar compound assignment requires a scalar right operand",
                ));
            }
            let checked = self.vector_binop(bin, lhs, rhs, loc)?;
            let ExprKind::Binary(_, _, rhs) = checked.kind else {
                unreachable!()
            };
            return Ok(mk(
                ExprKind::OpAssign {
                    op: bin_op(bin),
                    lhs: self.node(lhs),
                    rhs,
                },
                ty,
                loc,
            ));
        }
        let op = bin_op(bin);
        if (ty.is_complex() || rhs.ty.is_complex())
            && !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div)
        {
            return Err(Error::new(
                loc,
                "this complex compound operator is not supported",
            ));
        }
        let comp = match op {
            BinOp::Shl | BinOp::Shr => {
                if !ty.is_integer() || !rhs.ty.is_integer() {
                    return Err(Error::new(loc, "shift needs integers"));
                }
                promoted(&ty)
            }
            _ => {
                if !ty.is_arith() || !rhs.ty.is_arith() {
                    return Err(Error::new(loc, format!("invalid operands to `{op:?}=`")));
                }
                let comp = usual_with_layout(&ty, &rhs.ty, self.c_layout);
                if matches!(op, BinOp::Rem | BinOp::And | BinOp::Or | BinOp::Xor)
                    && !comp.is_integer()
                {
                    return Err(Error::new(loc, "operation needs integers"));
                }
                comp
            }
        };
        let rhs_ty = match comp {
            Ty::Complex(elem) if !rhs.ty.is_complex() => *elem,
            _ => comp,
        };
        let rhs = self.cast(rhs, rhs_ty);
        Ok(mk(
            ExprKind::OpAssign {
                op,
                lhs: self.node(lhs),
                rhs: self.node(rhs),
            },
            ty,
            loc,
        ))
    }

    pub(crate) fn check_lvalue(&self, e: &Expr<'a>) -> R<()> {
        let ok = match &e.kind {
            ExprKind::Local(_)
            | ExprKind::Deref(_)
            | ExprKind::Str(_)
            | ExprKind::CompoundLit(..) => true,
            ExprKind::Global(_) => !matches!(e.ty, Ty::Func(_)),
            ExprKind::Member { base, .. } | ExprKind::VectorIndex(base, _) => {
                self.check_lvalue(base).is_ok()
            }
            _ => false,
        };
        if ok && !matches!(e.ty, Ty::Array(..)) {
            Ok(())
        } else {
            Err(Error::new(e.loc, "not assignable"))
        }
    }

    fn check_modifiable_lvalue(&self, e: &Expr<'a>) -> R<()> {
        self.check_lvalue(e)?;
        if e.sugar.is_const(&self.typedefs) {
            Err(Error::new(e.loc, "cannot modify a const-qualified object"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn conditional(&mut self) -> R<Expr<'a>> {
        let cond = self.binary(1)?;
        if !self.is("?") {
            return Ok(cond);
        }
        let loc = self.loc();
        self.advance();
        let cond = self.rvalue(cond);
        if !cond.ty.is_scalar() {
            return Err(Error::new(loc, "condition must be a scalar"));
        }
        if self.eat(":") {
            // GNU's `a ?: b` is `a ? a : b` evaluating `a` once:
            // `(t = a, t ? t : b)`, with `t` a made-up local.
            let b = self.conditional()?;
            let Some(_) = self.fs else {
                return Err(Error::new(
                    loc,
                    "`?:` without a middle operand outside a function",
                ));
            };
            let name = self.unique_name("elvis");
            let ty = cond.ty;
            // Capture a value, not a qualified object. Keep the original
            // expression's read qualifiers and the pointer's pointee qualifiers.
            let sugar = *cond.sugar.unqualified(&self.typedefs);
            let t = self.new_local(name, ty, sugar, false);
            let local = || mk(ExprKind::Local(t), ty, loc).with_sugar(sugar);
            let set = self.assignment("=", local(), cond, loc)?;
            let pick = self.conditional_of(local(), local(), b, loc)?;
            let ty = pick.ty;
            let sugar = pick.sugar;
            return Ok(
                mk(ExprKind::Comma(self.node(set), self.node(pick)), ty, loc).with_sugar(sugar),
            );
        }
        let a = self.expr()?;
        self.expect(":")?;
        let b = self.conditional()?;
        self.conditional_of(cond, a, b, loc)
    }

    /// `cond ? a : b`, typed.
    fn conditional_of(
        &mut self,
        cond: Expr<'a>,
        a: Expr<'a>,
        b: Expr<'a>,
        loc: Loc,
    ) -> R<Expr<'a>> {
        let (a, b) = (self.rvalue(a), self.rvalue(b));
        let ty = if a.ty.is_arith() && b.ty.is_arith() {
            usual_with_layout(&a.ty, &b.ty, self.c_layout)
        } else if a.ty.is_ptr() && is_null(&b) {
            a.ty
        } else if b.ty.is_ptr() && is_null(&a) {
            b.ty
        } else if a.ty.is_ptr() && b.ty.is_ptr() {
            if a.ty.pointee() == Some(&Ty::Void) {
                a.ty
            } else {
                b.ty
            }
        } else if a.ty == b.ty || a.ty == Ty::Void || b.ty == Ty::Void {
            if a.ty == Ty::Void { a.ty } else { b.ty }
        } else {
            return Err(Error::new(loc, "mismatched types in `?:`"));
        };
        let (a, b) = if ty == Ty::Void {
            (a, b)
        } else {
            (self.cast(a, ty), self.cast(b, ty))
        };
        let sugar = if ty.is_ptr() {
            a.sugar.unqualified(&self.typedefs).merge(
                b.sugar.unqualified(&self.typedefs),
                &self.typedefs,
                self.arena,
            )
        } else {
            Sugar::Plain
        };
        Ok(mk(
            ExprKind::Cond(self.node(cond), self.node(a), self.node(b)),
            ty,
            loc,
        )
        .with_sugar(sugar))
    }

    fn binary(&mut self, min_prec: u8) -> R<Expr<'a>> {
        let mut lhs = self.cast_expr()?;
        loop {
            let Tok::Punct(p) = self.peek() else {
                return Ok(lhs);
            };
            let Some(prec) = binary_prec(p) else {
                return Ok(lhs);
            };
            if prec < min_prec {
                return Ok(lhs);
            }
            let loc = self.loc();
            self.advance();
            let rhs = self.binary(prec + 1)?;
            lhs = self.binop(p, lhs, rhs, loc)?;
        }
    }

    pub(crate) fn binop(&mut self, p: &str, l: Expr<'a>, r: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        let (l, r) = (self.rvalue(l), self.rvalue(r));
        if l.ty.is_vector() || r.ty.is_vector() {
            return self.vector_binop(p, l, r, loc);
        }
        if (l.ty.is_complex() || r.ty.is_complex())
            && !matches!(p, "+" | "-" | "*" | "/" | "==" | "!=" | "&&" | "||")
        {
            return Err(Error::new(
                loc,
                format!("complex operator `{p}` is not supported"),
            ));
        }
        match p {
            "&&" | "||" => {
                if !l.ty.is_scalar() || !r.ty.is_scalar() {
                    return Err(Error::new(loc, format!("`{p}` needs scalars")));
                }
                let kind = if p == "&&" {
                    ExprKind::LogAnd(self.node(l), self.node(r))
                } else {
                    ExprKind::LogOr(self.node(l), self.node(r))
                };
                Ok(mk(kind, INT, loc))
            }
            "+" if l.ty.is_ptr() && r.ty.is_integer() => self.ptr_add(l, r, false, loc),
            "+" if l.ty.is_integer() && r.ty.is_ptr() => self.ptr_add(r, l, false, loc),
            "-" if l.ty.is_ptr() && r.ty.is_integer() => self.ptr_add(l, r, true, loc),
            "-" if l.ty.is_ptr() && r.ty.is_ptr() => {
                let size = self.elem_size(&l.ty, loc)?;
                let diff = mk(
                    ExprKind::PtrDiff(self.node(l), self.node(r), size),
                    LONG,
                    loc,
                );
                if l.ty.pointee().unwrap().runtime_sized() {
                    let stride = self.size_expr(l.ty.pointee().unwrap(), loc);
                    let stride = self.cast(stride, LONG);
                    Ok(mk(
                        ExprKind::Binary(BinOp::Div, self.node(diff), self.node(stride)),
                        LONG,
                        loc,
                    ))
                } else {
                    Ok(diff)
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                let op = bin_op(p);
                let (l, r) = if l.ty.is_arith() && r.ty.is_arith() {
                    let ty = usual_with_layout(&l.ty, &r.ty, self.c_layout);
                    (self.cast(l, ty), self.cast(r, ty))
                } else if l.ty.is_ptr() && r.ty.is_integer() {
                    let ty = l.ty;
                    (l, self.cast(r, ty))
                } else if l.ty.is_integer() && r.ty.is_ptr() {
                    let ty = r.ty;
                    (self.cast(l, ty), r)
                } else if l.ty.is_ptr() && r.ty.is_ptr() {
                    let ty = l.ty;
                    (l, self.cast(r, ty))
                } else {
                    return Err(Error::new(loc, format!("invalid operands to `{p}`")));
                };
                Ok(mk(
                    ExprKind::Binary(op, self.node(l), self.node(r)),
                    INT,
                    loc,
                ))
            }
            "<<" | ">>" => {
                if !l.ty.is_integer() || !r.ty.is_integer() {
                    return Err(Error::new(loc, "shift needs integers"));
                }
                let l = self.promote(l);
                let ty = l.ty;
                let r = self.cast(r, ty);
                Ok(mk(
                    ExprKind::Binary(bin_op(p), self.node(l), self.node(r)),
                    ty,
                    loc,
                ))
            }
            _ => {
                let op = bin_op(p);
                let ints_only = matches!(op, BinOp::Rem | BinOp::And | BinOp::Or | BinOp::Xor);
                let ok = if ints_only {
                    l.ty.is_integer() && r.ty.is_integer()
                } else {
                    l.ty.is_arith() && r.ty.is_arith()
                };
                if !ok {
                    return Err(Error::new(loc, format!("invalid operands to `{p}`")));
                }
                let ty = usual_with_layout(&l.ty, &r.ty, self.c_layout);
                // Mixed real/complex addition preserves the untouched imaginary
                // component (including its zero sign), without adding +0.
                let domain = |t: Ty<'a>| match ty {
                    Ty::Complex(elem) if !t.is_complex() => *elem,
                    _ => ty,
                };
                let (lt, rt) = (domain(l.ty), domain(r.ty));
                let (l, r) = (self.cast(l, lt), self.cast(r, rt));
                Ok(mk(
                    ExprKind::Binary(op, self.node(l), self.node(r)),
                    ty,
                    loc,
                ))
            }
        }
    }

    fn vector_binop(&mut self, p: &str, l: Expr<'a>, r: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        let vector_ty = if l.ty.is_vector() { l.ty } else { r.ty };
        let Ty::Vector(elem, shape) = vector_ty else {
            unreachable!()
        };
        let (l, r) = if l.ty.is_vector() && r.ty.is_vector() {
            if l.ty != r.ty {
                return Err(Error::new(
                    loc,
                    "vector operations require matching vector operands; implicit vector conversions are not supported",
                ));
            }
            (l, r)
        } else {
            if matches!(p, "<<" | ">>") {
                return Err(Error::new(
                    loc,
                    "scalar vector shift counts are not supported",
                ));
            }
            if l.ty.is_vector() {
                (l, self.vector_broadcast(r, vector_ty, loc)?)
            } else {
                (self.vector_broadcast(l, vector_ty, loc)?, r)
            }
        };
        let allowed = if elem.is_float() {
            matches!(
                p,
                "+" | "-" | "*" | "/" | "==" | "!=" | "<" | "<=" | ">" | ">="
            )
        } else {
            matches!(
                p,
                "+" | "-"
                    | "*"
                    | "&"
                    | "|"
                    | "^"
                    | "<<"
                    | ">>"
                    | "=="
                    | "!="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
            )
        };
        if !allowed {
            return Err(Error::new(
                loc,
                format!("unsupported vector operation `{p}`"),
            ));
        }
        let op = bin_op(p);
        let ty = if op.is_comparison() {
            let kind = match shape.lane_type().bytes().unwrap() {
                1 => IntKind::SChar,
                2 => IntKind::Short,
                4 => IntKind::Int,
                _ => IntKind::Long,
            };
            Ty::Vector(self.type_ref(Ty::Int(kind)), shape.mask())
        } else {
            l.ty
        };
        Ok(mk(
            ExprKind::Binary(op, self.node(l), self.node(r)),
            ty,
            loc,
        ))
    }

    /// A bounded, lossless GNU scalar conversion before splatting. In particular,
    /// never use assignment conversion to silently truncate a wide scalar.
    fn vector_broadcast(&mut self, scalar: Expr<'a>, ty: Ty<'a>, loc: Loc) -> R<Expr<'a>> {
        self.require_native_layout(loc, "C vector broadcasts")?;
        let Ty::Vector(elem, _) = ty else {
            unreachable!()
        };
        let safe = match (scalar.ty.int_kind(), elem.int_kind()) {
            (Some(from), Some(to)) if from.size() <= 8 => {
                let range_fits = if from == IntKind::Bool {
                    true
                } else if from.signed() == to.signed() {
                    from.size() <= to.size()
                } else {
                    !from.signed() && from.size() < to.size()
                };
                range_fits
                    || broadcast_constant(&scalar, self.c_layout).is_some_and(|c| {
                        let Const::Int(bits) = c else { return false };
                        let value = if from.signed() {
                            bits as i128
                        } else {
                            (bits as u64) as i128
                        };
                        let width = to.size() * 8;
                        let (min, max) = if to.signed() {
                            (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1)
                        } else {
                            (0, (1i128 << width) - 1)
                        };
                        (min..=max).contains(&value)
                    })
            }
            _ => match (scalar.ty, *elem) {
                (Ty::Float, Ty::Float | Ty::Double) | (Ty::Double, Ty::Double) => true,
                (Ty::Double, Ty::Float) => broadcast_constant(&scalar, self.c_layout).is_some_and(
                    |c| matches!(c, Const::Float(f) if f.is_finite() && (f as f32) as f64 == f),
                ),
                _ => false,
            },
        };
        if !safe {
            return Err(Error::new(
                loc,
                "scalar broadcast requires a lossless supported conversion to the vector element type; use an explicit scalar cast",
            ));
        }
        let scalar = self.cast(scalar, *elem);
        Ok(mk(
            ExprKind::Builtin(Builtin::VectorSplat, self.slice(&[scalar])),
            ty,
            loc,
        ))
    }

    fn ptr_add(&mut self, ptr: Expr<'a>, n: Expr<'a>, negate: bool, loc: Loc) -> R<Expr<'a>> {
        let size = self.elem_size(&ptr.ty, loc)?;
        let mut n = self.cast(n, LONG);
        let (scaled, size) = self.scale_vm_index(n, &ptr.ty, size, loc);
        n = scaled;
        if negate {
            n = mk(ExprKind::Unary(UnOp::Neg, self.node(n)), LONG, loc);
        }
        let ty = ptr.ty;
        let sugar = *ptr.sugar.unqualified(&self.typedefs);
        Ok(mk(
            ExprKind::PtrAdd(self.node(ptr), self.node(n), size),
            ty,
            loc,
        )
        .with_sugar(sugar))
    }

    /// The size of what a pointer points to, for arithmetic. `void *`
    /// steps by bytes, as in GNU C.
    fn elem_size(&self, ptr: &Ty<'a>, loc: Loc) -> R<u64> {
        let elem = ptr.pointee().unwrap();
        match elem {
            Ty::Void => Ok(1),
            Ty::Func(_) => Err(Error::new(loc, "arithmetic on a function pointer")),
            Ty::Struct(id) if !self.structs[*id].complete => Err(Error::new(
                loc,
                "arithmetic on a pointer to an incomplete type",
            )),
            Ty::Array(a) if a.len.is_none() && a.vla.is_none() => Err(Error::new(
                loc,
                "arithmetic on a pointer to an incomplete type",
            )),
            _ if elem.runtime_sized() => Ok(1),
            _ => Ok(self.size_of(elem)),
        }
    }

    /// Runtime sizes are products of captured counts, stopping at pointers.
    pub(crate) fn size_expr(&self, ty: &Ty<'a>, loc: Loc) -> Expr<'a> {
        // Preserve constant sizeof nodes for fixed arrays and VM pointers.
        if !ty.runtime_sized() {
            return mk(ExprKind::Int(self.size_of(ty) as i64), ULONG, loc);
        }
        if let Ty::Array(a) = ty {
            let count = match a.vla {
                Some(id) => mk(ExprKind::Local(id), ULONG, loc),
                None => mk(ExprKind::Int(a.len.unwrap_or(0) as i64), ULONG, loc),
            };
            let stride = self.size_expr(&a.elem, loc);
            mk(
                ExprKind::Binary(BinOp::Mul, self.node(count), self.node(stride)),
                ULONG,
                loc,
            )
        } else {
            mk(ExprKind::Int(self.size_of(ty) as i64), ULONG, loc)
        }
    }

    fn scale_vm_index(
        &mut self,
        n: Expr<'a>,
        ptr: &Ty<'a>,
        size: u64,
        loc: Loc,
    ) -> (Expr<'a>, u64) {
        if ptr.pointee().unwrap().runtime_sized() {
            let stride = self.size_expr(ptr.pointee().unwrap(), loc);
            let stride = self.cast(stride, LONG);
            (
                mk(
                    ExprKind::Binary(BinOp::Mul, self.node(n), self.node(stride)),
                    LONG,
                    loc,
                ),
                1,
            )
        } else {
            (n, size)
        }
    }

    fn capture_vm_bounds(&mut self, mark: usize, mut e: Expr<'a>) -> Expr<'a> {
        let bounds = self.take_vm_bounds(mark);
        for (id, bound) in bounds.into_iter().rev() {
            let lhs = mk(ExprKind::Local(id), ULONG, bound.loc);
            let set = mk(
                ExprKind::Assign(self.node(lhs), self.node(bound)),
                ULONG,
                bound.loc,
            );
            e = mk(ExprKind::Comma(self.node(set), self.node(e)), e.ty, e.loc).with_sugar(e.sugar);
        }
        e
    }

    fn cast_expr(&mut self) -> R<Expr<'a>> {
        if self.is("(") && self.type_follows(1) {
            let loc = self.loc();
            self.advance();
            let mark = self.vm_bound_mark();
            let (ty, sugar) = self.type_name_sugar_mode(true)?;
            self.expect(")")?;
            if self.is("{") {
                let lit = self.compound_literal(ty, sugar, loc)?.with_sugar(sugar);
                if let ExprKind::CompoundLit(id, _) = &lit.kind {
                    self.fs.as_mut().unwrap().locals[*id].sugar = sugar;
                    self.mark_in_memory(*id);
                }
                return self.postfix_ops(lit);
            }
            let sugar = *sugar.unqualified(&self.typedefs);
            let e = self.cast_expr()?;
            let e = self.rvalue(e);
            if let Ty::Struct(id) = ty
                && self.structs[id].is_union
                && e.ty != ty
            {
                let qualifiers = self.parameter_qualifiers(e.ty, e.sugar);
                let member = self.structs[id]
                    .members
                    .iter()
                    .find(|member| {
                        member.ty.compatible(&e.ty)
                            && self.parameter_qualifiers(member.ty, member.sugar) == qualifiers
                            && self.union_cast_enums_compatible(member.ty, member.sugar, e.sugar)
                    })
                    .ok_or_else(|| Error::new(loc, "cast operand does not match a union member"))?;
                let bits = member.bits;
                let result = mk(ExprKind::UnionCast(self.node(e), bits), ty, loc).with_sugar(sugar);
                return Ok(self.capture_vm_bounds(mark, result));
            }
            let vector_cast = (ty.is_vector() || e.ty.is_vector())
                && (ty.is_vector() || ty.is_integer())
                && (e.ty.is_vector() || e.ty.is_integer())
                && self.size_of(&ty) == self.size_of(&e.ty);
            let ok = vector_cast
                || ty == Ty::Void
                || (ty.is_scalar()
                    && e.ty.is_scalar()
                    && !((ty.is_float() || ty.is_complex()) && e.ty.is_ptr())
                    && !(ty.is_ptr() && (e.ty.is_float() || e.ty.is_complex())))
                || (ty.is_struct() && ty == e.ty);
            if !ok {
                return Err(Error::new(loc, "invalid cast"));
            }
            // Literal conversion cannot hide a qualified memory read;
            // retain the frontend's existing constant normalization. Preserve
            // explicit i128 conversions for bounded constant proofs.
            if matches!(e.kind, ExprKind::Int(_))
                && !e.ty.int_kind().is_some_and(|k| k.size() == 16)
                && !ty.int_kind().is_some_and(|k| k.size() == 16)
            {
                let result = self.cast(e, ty).with_sugar(sugar);
                return Ok(self.capture_vm_bounds(mark, result));
            }
            // Keep a distinct value conversion even when the unqualified
            // types match: changing result qualifiers must not rewrite the
            // qualifiers on the operand's actual memory read.
            let result = mk(ExprKind::Cast(self.node(e)), ty, loc).with_sugar(sugar);
            return Ok(self.capture_vm_bounds(mark, result));
        }
        self.unary()
    }

    /// GCC matches compatible types, including an enum's underlying integer
    /// type, but two distinct enums remain incompatible.
    fn union_cast_enums_compatible(&self, ty: Ty<'a>, a: Sugar<'a>, b: Sugar<'a>) -> bool {
        match (a.unqualified(&self.typedefs), b.unqualified(&self.typedefs)) {
            (Sugar::Enum(a), Sugar::Enum(b)) => a == b,
            _ => match ty {
                Ty::Ptr(inner) => self.union_cast_enums_compatible(
                    *inner,
                    a.pointee(&self.typedefs),
                    b.pointee(&self.typedefs),
                ),
                Ty::Array(array) => self.union_cast_enums_compatible(
                    array.elem,
                    a.array_element(&self.typedefs, self.arena),
                    b.array_element(&self.typedefs, self.arena),
                ),
                _ => true,
            },
        }
    }

    /// Whether a type name starts `n` tokens ahead.
    fn type_follows(&mut self, n: usize) -> bool {
        let save = self.pos_save();
        for _ in 0..n {
            self.advance();
        }
        let yes = self.is_typename();
        self.pos_restore(save);
        yes
    }

    fn unary(&mut self) -> R<Expr<'a>> {
        let loc = self.loc();
        let Some(word) = (match self.peek() {
            Tok::Punct(p) => Some(p),
            Tok::Ident(s) => Some(s),
            _ => None,
        }) else {
            return self.postfix();
        };
        match word {
            "__real__" | "__real" | "__imag__" | "__imag" => {
                self.advance();
                let e = self.cast_expr()?;
                let imag = word.starts_with("__imag");
                // Keep declaration-only TC component queries canonical without
                // introducing opaque elements into the value-bearing Complex type.
                let component = match e.ty {
                    Ty::Complex(elem) => Some(elem),
                    ty if ty == COMPLEX_BINARY128 => Some(&BINARY128),
                    _ => None,
                };
                if let Some(elem) = component {
                    let sugar = e.sugar;
                    return Ok(mk(
                        ExprKind::Member {
                            base: self.node(e),
                            offset: if imag { self.size_of(elem) } else { 0 },
                            bits: None,
                            nonstring: false,
                            align: self.types().align(elem),
                        },
                        *elem,
                        loc,
                    )
                    .with_sugar(sugar));
                }
                if !e.ty.is_integer() && !e.ty.is_float() {
                    return Err(Error::new(
                        loc,
                        "real/imaginary extraction requires supported arithmetic type",
                    ));
                }
                if !imag {
                    return Ok(e);
                }
                let zero = self.cast(mk(ExprKind::Int(0), INT, loc), e.ty);
                Ok(mk(
                    ExprKind::Comma(self.node(e), self.node(zero)),
                    zero.ty,
                    loc,
                ))
            }
            "sizeof" | "_Alignof" | "__alignof__" | "__alignof" => {
                self.advance();
                let mark = self.vm_bound_mark();
                let mut operand = None;
                let (ty, alignment) = if self.is("(") && self.type_follows(1) {
                    self.advance();
                    let (ty, sugar) = self.type_name_sugar_mode(true)?;
                    self.expect(")")?;
                    if self.is("{") {
                        let e = self.compound_literal(ty, sugar, loc)?;
                        let align = self.expr_alignment(&e);
                        operand = Some(e);
                        (e.ty, align)
                    } else {
                        let align = self.type_alignment(&ty, &sugar);
                        (ty, align)
                    }
                } else {
                    let e = self.unary()?;
                    let align = self.expr_alignment(&e);
                    operand = Some(e);
                    (e.ty, align)
                };
                let incomplete = match &ty {
                    Ty::Func(_)
                    | Ty::Array(ArrayTy {
                        len: None,
                        vla: None,
                        ..
                    }) => true,
                    Ty::Struct(id) => !self.structs[*id].complete,
                    _ => false,
                };
                if incomplete {
                    return Err(Error::new(loc, "`sizeof` of an incomplete type"));
                }
                if word == "sizeof" {
                    let mut result = self.size_expr(&ty, loc);
                    // A VLA operand is evaluated (e.g. sizeof *p++); a VM
                    // pointer operand has fixed size and remains unevaluated.
                    if ty.runtime_sized()
                        && let Some(e) = operand
                    {
                        result = mk(ExprKind::Comma(self.node(e), self.node(result)), ULONG, loc);
                    }
                    Ok(self.capture_vm_bounds(mark, result))
                } else {
                    // Alignment queries never execute bound expressions.
                    self.take_vm_bounds(mark);
                    Ok(mk(ExprKind::Int(alignment as i64), ULONG, loc))
                }
            }
            "++" | "--" => {
                self.advance();
                let e = self.unary()?;
                self.incdec(e, word == "++", true, loc)
            }
            "&" => {
                self.advance();
                let e = self.cast_expr()?;
                self.address_of(e, loc)
            }
            "&&" => {
                // GNU's label address: the label's number. `goto *` only
                // goes to labels of the same function, so a number is as
                // good as an address, and is a constant, as static
                // dispatch tables need.
                self.advance();
                let name = self.ident()?;
                let name = self.resolve_label(name);
                let Some(fs) = self.fs.as_mut() else {
                    return Err(Error::new(loc, "`&&label` outside a function"));
                };
                let n = match fs.label_addrs.iter().position(|l| *l == name) {
                    Some(i) => i + 1,
                    None => {
                        fs.label_addrs.push(name);
                        fs.label_addrs.len()
                    }
                };
                Ok(mk(ExprKind::LabelAddr(n), self.ptr_type(Ty::Void), loc))
            }
            "*" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                let Some(pointee) = e.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "dereferencing a non-pointer"));
                };
                let sugar = e.sugar.pointee(&self.typedefs);
                Ok(mk(ExprKind::Deref(self.node(e)), pointee, loc).with_sugar(sugar))
            }
            "+" | "-" | "~" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                let vector_ok =
                    matches!(e.ty, Ty::Vector(elem, _) if word != "~" || elem.is_integer());
                if !vector_ok && (!e.ty.is_arith() || (word == "~" && !e.ty.is_integer())) {
                    return Err(Error::new(loc, format!("invalid operand to `{word}`")));
                }
                let e = self.promote(e);
                if word == "+" {
                    // Unary plus produces a value even when promotion does
                    // not change its type. Retain the operand's qualified
                    // read, but do not expose its lvalue through component
                    // extraction (e.g. `__real__ (+z) = 1`).
                    return Ok(mk(ExprKind::Cast(self.node(e)), e.ty, loc));
                }
                let op = if word == "-" { UnOp::Neg } else { UnOp::BitNot };
                let ty = e.ty;
                Ok(mk(ExprKind::Unary(op, self.node(e)), ty, loc))
            }
            "!" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                if !e.ty.is_scalar() {
                    return Err(Error::new(loc, "`!` needs a scalar"));
                }
                Ok(mk(ExprKind::Unary(UnOp::Not, self.node(e)), INT, loc))
            }
            _ => self.postfix(),
        }
    }

    pub(crate) fn address_of(&mut self, e: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        if let ExprKind::Local(id) = e.kind
            && self
                .fs
                .as_ref()
                .is_some_and(|fs| fs.locals[id].asm_register.is_some())
        {
            return Err(Error::new(
                loc,
                "cannot take the address of a named register variable",
            ));
        }
        if matches!(e.kind, ExprKind::VectorIndex(..)) {
            return Err(Error::new(
                loc,
                "cannot take the address of a vector element",
            ));
        }
        if let ExprKind::Member { bits: Some(_), .. } = e.kind {
            return Err(Error::new(loc, "address of a bit-field"));
        }
        if !matches!(e.ty, Ty::Func(_)) {
            self.check_lvalue(&e).or_else(|err| {
                if matches!(e.ty, Ty::Array(..)) {
                    Ok(())
                } else {
                    Err(err)
                }
            })?;
        }
        self.mark_address_taken(&e);
        let ty = self.ptr_type(e.ty);
        let sugar = Sugar::Ptr(self.sugar_ref(e.sugar));
        Ok(mk(ExprKind::Addr(self.node(e)), ty, loc).with_sugar(sugar))
    }

    /// A local whose address is taken must live in memory.
    fn mark_address_taken(&mut self, e: &Expr<'a>) {
        match &e.kind {
            ExprKind::Local(id) => self.mark_in_memory(*id),
            ExprKind::Member { base, .. } | ExprKind::VectorIndex(base, _) => {
                self.mark_address_taken(base)
            }
            _ => {}
        }
    }

    fn incdec(&mut self, e: Expr<'a>, inc: bool, prefix: bool, loc: Loc) -> R<Expr<'a>> {
        self.check_modifiable_lvalue(&e)?;
        if e.ty.is_complex() {
            return Err(Error::new(
                loc,
                "complex increment/decrement is not supported",
            ));
        }
        if let Ty::Atomic(&ty) = e.ty {
            // Pointer RMWs operate on the address bits, with a byte step
            // scaled by the pointee's size. Both targets have 64-bit
            // pointers; the IR's add/sub RMWs take integers, not pointers.
            if ty.variably_modified() {
                return Err(Error::new(
                    loc,
                    "atomic variably modified pointers are not supported yet",
                ));
            }
            let (step, rmw_ty) = if ty.is_ptr() {
                (self.elem_size(&ty, loc)? as i64, LONG)
            } else {
                (1, ty)
            };
            let step = mk(ExprKind::Int(step), LONG, loc);
            let step = self.cast(step, rmw_ty);
            let op = if inc { RmwOp::Add } else { RmwOp::Sub };
            let sugar = *e.sugar.unqualified(&self.typedefs);
            let args = vec![
                self.address_of_atomic(e),
                step,
                mk(ExprKind::Int(5), INT, loc),
            ];
            let builtin = if ty.is_float() {
                Builtin::AtomicFloatRmw {
                    op: if inc { BinOp::Add } else { BinOp::Sub },
                    new: prefix,
                }
            } else {
                Builtin::AtomicRmw { op, new: prefix }
            };
            let result = mk(ExprKind::Builtin(builtin, self.slice(&args)), rmw_ty, loc);
            return Ok(self.cast(result, ty).with_sugar(sugar));
        }
        if !e.ty.is_scalar() {
            return Err(Error::new(loc, "`++`/`--` needs a scalar"));
        }
        if e.ty.is_ptr() {
            self.elem_size(&e.ty, loc)?;
        }
        let ty = e.ty;
        let sugar = *e.sugar.unqualified(&self.typedefs);
        Ok(mk(
            ExprKind::IncDec {
                lhs: self.node(e),
                inc,
                prefix,
            },
            ty,
            loc,
        )
        .with_sugar(sugar))
    }

    fn postfix(&mut self) -> R<Expr<'a>> {
        let e = self.primary()?;
        self.postfix_ops(e)
    }

    fn postfix_ops(&mut self, mut e: Expr<'a>) -> R<Expr<'a>> {
        loop {
            let loc = self.loc();
            // One dispatch on the spelling rather than a test per operator.
            let Tok::Punct(p @ ("[" | "(" | "." | "->" | "++" | "--")) = self.peek() else {
                return Ok(e);
            };
            self.advance();
            if p == "[" {
                let idx = self.expr()?;
                self.expect("]")?;
                if let Ty::Vector(elem, shape) = e.ty {
                    if !idx.ty.is_integer() {
                        return Err(Error::new(loc, "vector subscript requires an integer"));
                    }
                    if e.sugar.is_volatile(&self.typedefs) {
                        return Err(Error::new(
                            loc,
                            "volatile vector lane access is not supported",
                        ));
                    }
                    if let Some(n) = crate::eval::eval_int_with_layout(&idx, self.c_layout)
                        && (n < 0 || n >= i64::from(shape.lanes()))
                    {
                        return Err(Error::new(loc, "vector subscript is out of range"));
                    }
                    self.mark_address_taken(&e);
                    let idx = self.cast(idx, LONG);
                    let sugar = e.sugar;
                    e = mk(
                        ExprKind::VectorIndex(self.node(e), self.node(idx)),
                        *elem,
                        loc,
                    )
                    .with_sugar(sugar);
                    continue;
                }
                let sum = self.binop("+", e, idx, loc)?;
                let Some(elem) = sum.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "subscript of a non-pointer"));
                };
                let sugar = sum.sugar.pointee(&self.typedefs);
                e = mk(ExprKind::Deref(self.node(sum)), elem, loc).with_sugar(sugar);
            } else if p == "(" {
                e = self.call(e, loc)?;
            } else if p == "." {
                e = self.member(e, loc)?;
            } else if p == "->" {
                let p = self.rvalue(e);
                let Some(pointee) = p.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "`->` on a non-pointer"));
                };
                let sugar = p.sugar.pointee(&self.typedefs);
                e = mk(ExprKind::Deref(self.node(p)), pointee, loc).with_sugar(sugar);
                e = self.member(e, loc)?;
            } else {
                e = self.incdec(e, p == "++", false, loc)?;
            }
        }
    }

    fn member(&mut self, base: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        let name = self.ident()?;
        let Ty::Struct(id) = base.ty else {
            return Err(Error::new(loc, format!("`.{name}` on a non-struct")));
        };
        let def = &self.structs[id];
        if !def.complete {
            return Err(Error::new(loc, "member of an incomplete struct"));
        }
        let Some((offset, m)) = def.find(name, &self.structs, &self.typedefs, self.arena) else {
            return Err(Error::new(loc, format!("no member `{name}`")));
        };
        let mut sugar = m.sugar;
        if base.sugar.is_volatile(&self.typedefs) {
            sugar = Sugar::Volatile(self.sugar_ref(sugar));
        }
        if base.sugar.is_const(&self.typedefs) {
            sugar = Sugar::Const(self.sugar_ref(sugar));
        }
        Ok(mk(
            ExprKind::Member {
                base: self.node(base),
                offset,
                bits: m.bits,
                nonstring: m.nonstring,
                align: m.align,
            },
            m.ty,
            loc,
        )
        .with_sugar(sugar))
    }

    fn call(&mut self, callee: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        let mut args = self.expr_bufs.pop().unwrap_or_default();
        while !self.eat(")") {
            if !args.is_empty() {
                self.expect(",")?;
            }
            args.push(self.assign()?);
        }
        self.make_call(callee, args, loc)
    }

    /// A call of `callee` with `args`, converted in place. The buffer is
    /// recycled.
    fn make_call(&mut self, callee: Expr<'a>, mut args: Vec<Expr<'a>>, loc: Loc) -> R<Expr<'a>> {
        let f = self.rvalue(callee);
        let Some(fty) = f.ty.pointee().and_then(Ty::func) else {
            return Err(Error::new(loc, "call of a non-function"));
        };
        for (i, arg) in args.iter_mut().enumerate() {
            *arg = if let Some(param) = fty.params.get(i).filter(|_| fty.has_prototype()) {
                self.convert_argument(*arg, param)?
            } else if fty.variadic || !fty.has_prototype() {
                self.default_promote(*arg)
            } else {
                return Err(Error::new(arg.loc, "too many arguments"));
            };
        }
        if fty.has_prototype() && args.len() < fty.params.len() {
            return Err(Error::new(loc, "too few arguments"));
        }
        if let Ty::Struct(id) = fty.ret
            && !self.structs[id].complete
        {
            return Err(Error::new(loc, "call returns an incomplete type"));
        }
        if let ExprKind::Addr(inner) = f.kind
            && let ExprKind::Global(g) = inner.kind
            && let Some(fs) = &mut self.fs
        {
            fs.direct_calls.push((g, loc));
            if crate::parse::nonlocal_exit(&self.globals[g]) {
                fs.cleanup_unsupported_exit
                    .get_or_insert((loc, "nonlocal exit/returns-twice call"));
            }
        }
        let returned = f.sugar.function_return(&self.typedefs);
        let sugar = *returned.unqualified(&self.typedefs);
        let call = ExprKind::Call(self.node(f), self.slice(&args));
        args.clear();
        self.expr_bufs.push(args);
        Ok(mk(call, fty.ret, loc).with_sugar(sugar))
    }

    /// Promotions for arguments without a parameter type.
    fn default_promote(&mut self, e: Expr<'a>) -> Expr<'a> {
        let e = self.rvalue(e);
        if e.ty == Ty::Float {
            return self.cast(e, Ty::Double);
        }
        self.promote(e)
    }

    fn primary(&mut self) -> R<Expr<'a>> {
        let loc = self.loc();
        if matches!(self.peek(), Tok::Eof | Tok::Punct(_)) && !self.is("(") {
            return Err(self.unexpected("an expression"));
        }
        match self.advance() {
            Tok::Int {
                value,
                decimal,
                unsigned,
                longs,
            } => Ok(mk(
                ExprKind::Int(value as i64),
                int_literal_type(value, decimal, unsigned, longs, self.c_layout),
                loc,
            )),
            Tok::Imaginary { value, single } => {
                let elem = if single { Ty::Float } else { Ty::Double };
                let args = [
                    mk(ExprKind::Float(0.0), elem, loc),
                    mk(ExprKind::Float(value), elem, loc),
                ];
                Ok(mk(
                    ExprKind::Builtin(Builtin::Complex, self.slice(&args)),
                    Ty::Complex(self.type_ref(elem)),
                    loc,
                ))
            }
            Tok::Float { value, single } => Ok(mk(
                ExprKind::Float(value),
                if single { Ty::Float } else { Ty::Double },
                loc,
            )),
            Tok::BadNumber(text) => Err(Error::new(loc, format!("invalid number `{text}`"))),
            Tok::Char(v) => Ok(mk(ExprKind::Int(v), INT, loc)),
            Tok::Str { bytes, wide } => {
                let mut bytes = bytes.to_vec();
                let mut wide = wide;
                while let Tok::Str {
                    bytes: more,
                    wide: w,
                } = self.peek()
                {
                    if w && !wide {
                        // Widen what came before.
                        bytes = bytes
                            .iter()
                            .flat_map(|&b| u32::from(b).to_le_bytes())
                            .collect();
                        wide = true;
                    }
                    if wide && !w {
                        bytes.extend(more.iter().flat_map(|&b| u32::from(b).to_le_bytes()));
                    } else {
                        bytes.extend(more);
                    }
                    self.advance();
                }
                Ok(self.string_expr(bytes, wide, loc))
            }
            Tok::Punct("(") if self.is("{") => {
                // GNU's statement expression.
                self.advance();
                if self.fs.is_none() {
                    return Err(Error::new(loc, "statement expression outside a function"));
                }
                let scope = self.push_block();
                let mut stmts = self.block_items()?;
                self.pop_block();
                self.expect(")")?;
                let value = stmts
                    .iter()
                    .rposition(|s| !matches!(s, Stmt::Block([])))
                    .and_then(|last| {
                        let mut tail = &stmts[last];
                        let mut labels = Vec::new();
                        // Named labels wrap their statement without opening a
                        // scope. Keep those labels before the result; scoped
                        // compounds and labeled null statements still yield void.
                        while let Stmt::Block([label @ Stmt::Label(_), next]) = tail {
                            labels.push(*label);
                            tail = next;
                        }
                        let Stmt::Expr(e) = tail else {
                            return None;
                        };
                        stmts = if labels.is_empty() {
                            &stmts[..last]
                        } else {
                            let mut prefix = stmts[..last].to_vec();
                            prefix.extend(labels);
                            self.slice(&prefix)
                        };
                        let e = self.rvalue(*e);
                        Some(self.node(e))
                    });
                let ty = value.as_ref().map_or(Ty::Void, |v| v.ty);
                let sugar = value
                    .as_ref()
                    .map_or(Sugar::Plain, |v| *v.sugar.unqualified(&self.typedefs));
                Ok(mk(
                    ExprKind::StmtExpr {
                        scope,
                        stmts,
                        value,
                    },
                    ty,
                    loc,
                )
                .with_sugar(sugar))
            }
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Ident(name) => {
                if name == "__func__" || name == "__FUNCTION__" || name == "__PRETTY_FUNCTION__" {
                    let func = self.fs.as_ref().map_or("", |f| f.name);
                    let bytes = func.as_bytes().to_vec();
                    return Ok(self.string_expr(bytes, false, loc));
                }
                if name == "_Generic" {
                    return self.generic(loc);
                }
                if name.starts_with("__builtin_") {
                    return self.builtin(name, loc);
                }
                if name.starts_with("__atomic_")
                    || name.starts_with("__sync_")
                    || name.starts_with("__c11_atomic_")
                {
                    return self.atomic(name, loc);
                }
                self.lookup_value(name, loc)
            }
            _ => unreachable!(),
        }
    }

    /// C11's `_Generic(e, type: a, ..., default: b)`: the association
    /// whose type is that of `e`'s value, unqualified.
    fn generic(&mut self, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let e = self.assign()?;
        let e = self.rvalue(e);
        let ty = *e.ty.unatomic();
        let qualifiers = self.parameter_qualifiers(ty, e.sugar);
        if ty.variably_modified() {
            return Err(Error::new(
                loc,
                "variably modified `_Generic` selectors are not supported yet",
            ));
        }
        let (mut chosen, mut default) = (None, None);
        while self.eat(",") {
            let is_default = self.eat("default");
            let assoc_ty = if is_default {
                None
            } else {
                Some(self.type_name_sugar()?)
            };
            self.expect(":")?;
            let value = self.assign()?;
            match assoc_ty {
                None => default = Some(value),
                Some((t, sugar))
                    if t.compatible(&ty)
                        && !sugar.is_const(&self.typedefs)
                        && !sugar.is_volatile(&self.typedefs)
                        && !sugar.is_restrict(&self.typedefs)
                        && self.parameter_qualifiers(t, sugar) == qualifiers =>
                {
                    chosen = Some(value);
                }
                Some(_) => {}
            }
        }
        self.expect(")")?;
        chosen
            .or(default)
            .ok_or_else(|| Error::new(loc, "no `_Generic` association matches"))
    }

    pub(crate) fn string_expr(&mut self, mut bytes: Vec<u8>, wide: bool, loc: Loc) -> Expr<'a> {
        let unit = if wide { 4 } else { 1 };
        bytes.extend(std::iter::repeat_n(0, unit));
        let len = (bytes.len() / unit) as u64;
        let i = self.add_string(bytes, wide);
        let elem = if wide {
            if self.c_layout == CDataLayout::NATIVE_LP64 && self.target == Target::Aarch64LinuxGnu {
                UINT
            } else {
                INT
            }
        } else if self.target == Target::Aarch64LinuxGnu {
            Ty::Int(IntKind::UPlainChar)
        } else {
            CHAR
        };
        mk(ExprKind::Str(i), self.array_type(elem, Some(len)), loc)
    }

    fn has_nonstring(&self, operand: &Expr<'a>) -> bool {
        match operand.kind {
            ExprKind::Local(id) => self.fs.as_ref().unwrap().locals[id].nonstring,
            ExprKind::Global(id) => self.globals[id].nonstring,
            ExprKind::Member { nonstring, .. } => nonstring,
            ExprKind::Deref(pointer) => self
                .attribute_referent(pointer)
                .is_some_and(|object| object.ty == operand.ty && self.has_nonstring(object)),
            _ => false,
        }
    }

    // GCC folds identity address expressions before looking at declaration
    // attributes. Preserve that distinction without following pointer values.
    fn attribute_referent(&self, pointer: &Expr<'a>) -> Option<&'a Expr<'a>> {
        match pointer.kind {
            ExprKind::Addr(object) => Some(object),
            ExprKind::Cast(inner) if inner.ty == pointer.ty => self.attribute_referent(inner),
            ExprKind::PtrAdd(base, index, _)
                if eval_int_with_layout(index, self.c_layout) == Some(0) =>
            {
                self.attribute_referent(base)
            }
            _ => None,
        }
    }

    fn builtin(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        let base = &name["__builtin_".len()..];
        if base.starts_with("ia32_") || base.starts_with("keel_sse2_") {
            self.require_native_layout(loc, "x86 builtins")?;
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
                    "SSE2 packed shift requires its matching 128-bit integer lane shape on x86-64",
                ));
            }
            if immediate {
                // Reuse the reviewed bounded ICE proof, including undefined-expression checks.
                let Some(n) = integer_constant_control(&args[1], self.c_layout)
                    .filter(|n| (0..=255).contains(n))
                else {
                    return Err(Error::new(
                        loc,
                        "SSE2 packed shift control must be an integer constant expression in 0..255",
                    ));
                };
                args[1] = mk(ExprKind::Int(n), Ty::Int(IntKind::ULLong), args[1].loc);
            } else if args[1].ty != Ty::Int(IntKind::ULLong) {
                return Err(Error::new(
                    loc,
                    "SSE2 packed shift count must be unsigned long long",
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
        if base == "ia32_pclmulqdq128" {
            let args = self.builtin_args(3, loc)?;
            if self.target != Target::X86_64LinuxGnu
                || !matches!(args[0].ty, Ty::Vector(_, keel_ir::VectorType::I64x2))
                || args[0].ty != args[1].ty
            {
                return Err(Error::new(
                    loc,
                    "PCLMULQDQ requires matching i64x2 integer vectors on x86-64",
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
                ExprKind::Builtin(Builtin::Pclmulqdq(n as u8), self.slice(&args[..2])),
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
            {
                Some(Const::Int(0)) => FloatCC::Eq,
                Some(Const::Int(3)) => FloatCC::Uno,
                Some(Const::Int(4)) => FloatCC::Ne,
                Some(Const::Int(7)) => FloatCC::Ord,
                Some(Const::Int(17)) => FloatCC::Lt,
                Some(Const::Int(18)) => FloatCC::Le,
                Some(Const::Int(29)) => FloatCC::Ge,
                Some(Const::Int(30)) => FloatCC::Gt,
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
                    || !matches!(eval_constant_with_layout(index, false, self.c_layout), Some(Const::Int(n)) if n >= 0 && n < i64::from(shape.lanes()) * 2)
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
                        ExprKind::Int(normalize_with_layout(value as i64, &ULONG, layout)),
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
                Ok(match eval_constant_with_layout(&offset, true, layout) {
                    Some(Const::Int(value)) => mk(ExprKind::Int(value), ULONG, loc),
                    _ => offset,
                })
            }
            "expect" => self.builtin_call(Builtin::Hint, &[LONG, LONG], LONG, loc),
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
                let Some(Const::Int(n)) =
                    eval_constant_with_layout(&alignment, false, self.c_layout)
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
                && eval_constant_with_layout(arg, false, self.c_layout) == Some(Const::Int(0));
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
    fn atomic(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
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

    /// `(type){...}`: an unnamed object, local in a function and static
    /// at file scope.
    fn compound_literal(&mut self, ty: Ty<'a>, sugar: Sugar<'a>, loc: Loc) -> R<Expr<'a>> {
        if ty.variably_modified() {
            return Err(Error::new(
                loc,
                "variably modified compound literals are not supported",
            ));
        }
        let (init, ty) = self.initializer(&ty)?;
        Ok(self.initialized_compound_literal(ty, sugar, loc, init))
    }

    fn initialized_compound_literal(
        &mut self,
        ty: Ty<'a>,
        sugar: Sugar<'a>,
        loc: Loc,
        init: Init<'a>,
    ) -> Expr<'a> {
        if self.fs.is_none() {
            let name = self.unique_name("__compound_literal");
            let g = self.add_static(name, ty, init, false);
            self.globals[g].align = self.type_alignment(&ty, &sugar);
            self.globals[g].sugar = sugar;
            return mk(ExprKind::Global(g), ty, loc);
        }
        // The name cannot clash: it is not an identifier.
        let name = self.unique_name("compound literal");
        let id = self.new_local(name, ty, sugar, true);
        self.fs.as_mut().unwrap().locals[id].compound_literal = true;
        mk(ExprKind::CompoundLit(id, self.init_node(init)), ty, loc)
    }

    // ---- conversions ----

    /// Array-to-pointer and function-to-pointer decay.
    /// The address of an `_Atomic` lvalue, for an atomic access.
    fn address_of_atomic(&mut self, e: Expr<'a>) -> Expr<'a> {
        self.mark_address_taken(&e);
        let ty = self.ptr_type(e.ty);
        let loc = e.loc;
        mk(ExprKind::Addr(self.node(e)), ty, loc)
    }

    /// The address of the `va_list` object `ap` names, which `va_start`
    /// and `va_arg` work on. Apple's `va_list` is a pointer variable, whose
    /// address it is; SysV's is an array, which decays to its own address,
    /// and a `va_list` parameter already holds that.
    fn va_list_addr(&mut self, ap: Expr<'a>) -> R<Expr<'a>> {
        Ok(match self.target {
            Target::Wasm32UnknownUnknown => {
                return Err(Error::new(
                    ap.loc,
                    "unsupported target wasm32-unknown-unknown: va_list is unavailable",
                ));
            }
            Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
                self.mark_address_taken(&ap);
                let ty = self.ptr_type(ap.ty);
                let loc = ap.loc;
                mk(ExprKind::Addr(self.node(ap)), ty, loc)
            }
            Target::X86_64LinuxGnu => self.rvalue(ap),
        })
    }

    /// An integer constant that is a whole initializer for an object of
    /// integer type `ty`, already converted: what `convert_for_assign`
    /// gives, without the descent through every level of expression
    /// syntax. Long tables of numbers are mostly these.
    pub(crate) fn lone_int_initializer(&mut self, ty: Ty<'a>) -> Option<Expr<'a>> {
        let Tok::Int {
            value,
            decimal,
            unsigned,
            longs,
        } = self.peek()
        else {
            return None;
        };
        if !matches!(ty, Ty::Int(_)) || !matches!(self.peek_at(1), Tok::Punct("," | "}" | ";")) {
            return None;
        }
        let lit = mk(
            ExprKind::Int(value as i64),
            int_literal_type(value, decimal, unsigned, longs, self.c_layout),
            self.loc(),
        );
        let e = if lit.ty == ty {
            lit
        } else {
            fold_int_cast(&lit, ty, self.c_layout)?
        };
        self.advance();
        Some(e)
    }

    pub(crate) fn rvalue(&mut self, e: Expr<'a>) -> Expr<'a> {
        match e.ty {
            Ty::Atomic(&ty) => {
                let loc = e.loc;
                let sugar = e.sugar;
                let args = vec![self.address_of_atomic(e), mk(ExprKind::Int(5), INT, loc)];
                mk(
                    ExprKind::Builtin(Builtin::AtomicLoad, self.slice(&args)),
                    ty,
                    loc,
                )
                .with_sugar(sugar)
            }
            Ty::Array(a) => {
                let ty = Ty::Ptr(&a.elem);
                let loc = e.loc;
                self.mark_address_taken(&e);
                let sugar =
                    Sugar::Ptr(self.sugar_ref(e.sugar.array_element(&self.typedefs, self.arena)));
                mk(ExprKind::Addr(self.node(e)), ty, loc).with_sugar(sugar)
            }
            Ty::Func(_) => {
                let ty = self.ptr_type(e.ty);
                let loc = e.loc;
                let sugar = Sugar::Ptr(self.sugar_ref(e.sugar));
                mk(ExprKind::Addr(self.node(e)), ty, loc).with_sugar(sugar)
            }
            _ => e,
        }
    }

    /// Converts `e` to `ty`; integer constants are converted right away.
    pub(crate) fn cast(&mut self, e: Expr<'a>, ty: Ty<'a>) -> Expr<'a> {
        if e.ty == ty {
            return e;
        }
        if let Some(folded) = fold_int_cast(&e, ty, self.c_layout) {
            return folded;
        }
        let loc = e.loc;
        let sugar = if e.ty.is_ptr() && ty.is_ptr() {
            e.sugar
        } else {
            Sugar::Plain
        };
        mk(ExprKind::Cast(self.node(e)), ty, loc).with_sugar(sugar)
    }

    /// The integer promotions.
    pub(crate) fn promote(&mut self, e: Expr<'a>) -> Expr<'a> {
        let e = self.rvalue(e);
        let ty = promoted(&e.ty);
        self.cast(e, ty)
    }

    /// A transparent union admits its member pointer types at call boundaries.
    fn convert_argument(&mut self, expr: Expr<'a>, ty: &Ty<'a>) -> R<Expr<'a>> {
        let expr = self.rvalue(expr);
        if let Ty::Struct(id) = *ty
            && self.structs[id].transparent_union
            && expr.ty != *ty
        {
            let null = expr.ty.is_integer()
                && matches!(
                    crate::eval::eval_constant_with_layout(&expr, false, self.c_layout),
                    Some(crate::eval::Const::Int(0))
                );
            let member = self.structs[id].members.iter().find(|member| {
                member.ty.compatible(&expr.ty) || null
                    || matches!((member.ty, expr.ty), (Ty::Ptr(a), Ty::Ptr(b)) if *a == Ty::Void || *b == Ty::Void)
            }).ok_or_else(|| Error::new(expr.loc, "argument does not match a transparent union member"))?;
            let member_ty = member.ty;
            let loc = expr.loc;
            let value = self.convert_for_assign(expr, &member_ty)?;
            let init = Init {
                items: self.slice(&[InitItem::Expr {
                    offset: 0,
                    expr: self.node(value),
                    bits: None,
                }]),
            };
            return Ok(self.initialized_compound_literal(*ty, Sugar::Plain, loc, init));
        }
        self.convert_for_assign(expr, ty)
    }

    /// Conversion as if by assignment, to parameters and returns too.
    pub(crate) fn convert_for_assign(&mut self, e: Expr<'a>, ty: &Ty<'a>) -> R<Expr<'a>> {
        let e = self.rvalue(e);
        // Matching opaque declarations can occur in unevaluated calls.
        // Evaluated uses still fail the recursive pre-pruning value guard.
        if ty.unusable().is_some() && e.ty == *ty {
            return Ok(e);
        }
        if ty.is_struct() || ty.is_vector() {
            if e.ty != *ty {
                return Err(Error::new(e.loc, "incompatible struct types"));
            }
            return Ok(e);
        }
        if !ty.is_scalar() || !e.ty.is_scalar() {
            return Err(Error::new(e.loc, "incompatible types in assignment"));
        }
        if let (Some(source), Some(destination)) = (
            e.ty.pointee().and_then(Ty::func),
            ty.pointee().and_then(Ty::func),
        ) && source.microsoft_x64 != destination.microsoft_x64
        {
            return Err(Error::new(
                e.loc,
                "incompatible function-pointer calling conventions",
            ));
        }
        if (ty.is_ptr() && (e.ty.is_float() || e.ty.is_complex()))
            || ((ty.is_float() || ty.is_complex()) && e.ty.is_ptr())
        {
            return Err(Error::new(e.loc, "incompatible types in assignment"));
        }
        Ok(self.cast(e, *ty))
    }
}

/// An integer constant converted to integer type `ty`, if it fits.
fn fold_int_cast<'a>(e: &Expr<'a>, ty: Ty<'a>, layout: CDataLayout) -> Option<Expr<'a>> {
    if let ExprKind::Int(v) = e.kind
        && let Ty::Int(_) = ty
        && e.ty.is_integer()
        && fits_i64_with_layout(v, &e.ty, &ty, layout)
    {
        return Some(mk(
            ExprKind::Int(normalize_with_layout(v, &ty, layout)),
            ty,
            e.loc,
        ));
    }
    None
}

pub(crate) fn checked_memory_intrinsic(name: &str) -> Option<Intrinsic> {
    match name {
        "__memcpy_chk" => Some(Intrinsic::Memcpy),
        "__memmove_chk" => Some(Intrinsic::Memmove),
        "__memset_chk" => Some(Intrinsic::Memset),
        _ => None,
    }
}

/// Canonical C ABI types, shared by declaration merging and builtin lowering.
/// Validate each declaration before the parser can discard its type.
pub(crate) fn checked_memory_prototype(id: Intrinsic, ty: &Ty<'_>) -> bool {
    let Ty::Func(fty) = ty else { return false };
    let void_ptr = |ty: &Ty<'_>| matches!(ty, Ty::Ptr(pointee) if **pointee == Ty::Void);
    fty.has_prototype()
        && !fty.variadic
        && void_ptr(&fty.ret)
        && fty.params.len() == 4
        && void_ptr(&fty.params[0])
        && if id == Intrinsic::Memset {
            fty.params[1] == INT
        } else {
            void_ptr(&fty.params[1])
        }
        && fty.params[2] == ULONG
        && fty.params[3] == ULONG
}

fn memory_builtin_const(sugar: &Sugar<'_>, defs: &[TypedefDef<'_>]) -> bool {
    match sugar {
        Sugar::Const(_) => true,
        Sugar::Volatile(inner) | Sugar::Restrict(inner) | Sugar::Array(inner) => {
            memory_builtin_const(inner, defs)
        }
        Sugar::Typedef(i) => memory_builtin_const(&defs[*i].sugar, defs),
        _ => false,
    }
}

/// Fixed-type GCC overflow families: int, long and long long, signed or
/// unsigned. Generic mixed-type overflow builtins have different semantics.
pub(crate) fn typed_overflow(name: &str) -> Option<(OverflowOp, Ty<'static>)> {
    let stem = name.strip_suffix("_overflow")?;
    let (op, suffix) = [
        ("sadd", OverflowOp::Sadd),
        ("uadd", OverflowOp::Uadd),
        ("ssub", OverflowOp::Ssub),
        ("usub", OverflowOp::Usub),
        ("smul", OverflowOp::Smul),
        ("umul", OverflowOp::Umul),
    ]
    .into_iter()
    .find_map(|(prefix, op)| stem.strip_prefix(prefix).map(|s| (op, s)))?;
    let signed = matches!(op, OverflowOp::Sadd | OverflowOp::Ssub | OverflowOp::Smul);
    let kind = match (suffix, signed) {
        ("", true) => IntKind::Int,
        ("", false) => IntKind::UInt,
        ("l", true) => IntKind::Long,
        ("l", false) => IntKind::ULong,
        ("ll", true) => IntKind::LLong,
        ("ll", false) => IntKind::ULLong,
        _ => return None,
    };
    Some((op, Ty::Int(kind)))
}

fn offset_member_follows(p: &mut Parser, first: bool) -> bool {
    if first {
        return matches!(p.peek(), Tok::Ident(_));
    }
    p.eat(".")
}

fn is_null(e: &Expr<'_>) -> bool {
    matches!(e.kind, ExprKind::Int(0)) && e.ty.is_integer()
        || matches!(&e.kind, ExprKind::Cast(inner) if is_null(inner) && e.ty.pointee() == Some(&Ty::Void))
}

pub(crate) fn bin_op(p: &str) -> BinOp {
    match p {
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        "%" => BinOp::Rem,
        "<<" => BinOp::Shl,
        ">>" => BinOp::Shr,
        "&" => BinOp::And,
        "|" => BinOp::Or,
        "^" => BinOp::Xor,
        "==" => BinOp::Eq,
        "!=" => BinOp::Ne,
        "<" => BinOp::Lt,
        "<=" => BinOp::Le,
        ">" => BinOp::Gt,
        ">=" => BinOp::Ge,
        _ => unreachable!("{p}"),
    }
}

/// A type after the integer promotions.
pub(crate) fn promoted<'a>(ty: &Ty<'a>) -> Ty<'a> {
    match ty {
        Ty::Int(k) if k.rank() < IntKind::Int.rank() => INT,
        Ty::Half => Ty::Float,
        _ => *ty,
    }
}

/// The type the usual arithmetic conversions give under the supplied model.
pub(crate) fn usual_with_layout<'a>(a: &Ty<'a>, b: &Ty<'a>, layout: CDataLayout) -> Ty<'a> {
    if a.is_complex() || b.is_complex() {
        let component = |t: &Ty<'a>| match t {
            Ty::Complex(e) => **e,
            _ => *t,
        };
        return if component(a) == Ty::Double || component(b) == Ty::Double {
            Ty::Complex(&Ty::Double)
        } else {
            Ty::Complex(&Ty::Float)
        };
    }
    if *a == Ty::Double || *b == Ty::Double {
        return Ty::Double;
    }
    if a.is_float() || b.is_float() {
        return Ty::Float;
    }
    let (Ty::Int(x), Ty::Int(y)) = (promoted(a), promoted(b)) else {
        unreachable!()
    };
    if x == y {
        return Ty::Int(x);
    }
    if x.signed() == y.signed() {
        return Ty::Int(if x.rank() >= y.rank() { x } else { y });
    }
    let (s, u) = if x.signed() { (x, y) } else { (y, x) };
    if u.rank() >= s.rank() {
        Ty::Int(u)
    } else if s.size_with_layout(layout) > u.size_with_layout(layout) {
        Ty::Int(s)
    } else {
        Ty::Int(s.to_unsigned())
    }
}

/// The type of an integer constant: the first of the allowed types its
/// value fits.
fn int_literal_type(
    value: u64,
    decimal: bool,
    unsigned: bool,
    longs: u8,
    layout: CDataLayout,
) -> Ty<'static> {
    use IntKind::*;
    let candidates: &[IntKind] = match (unsigned, longs, decimal) {
        (false, 0, true) => &[Int, Long, LLong],
        (false, 0, false) => &[Int, UInt, Long, ULong, LLong, ULLong],
        (true, 0, _) => &[UInt, ULong, ULLong],
        (false, 1, true) => &[Long, LLong],
        (false, 1, false) => &[Long, ULong, LLong, ULLong],
        (true, 1, _) => &[ULong, ULLong],
        (false, _, true) => &[LLong],
        (false, _, false) => &[LLong, ULLong],
        (true, _, _) => &[ULLong],
    };
    for &k in candidates {
        let max = if k.signed() {
            (1u64 << (k.size_with_layout(layout) * 8 - 1)) - 1
        } else if k.size_with_layout(layout) == 8 {
            u64::MAX
        } else {
            (1u64 << (k.size_with_layout(layout) * 8)) - 1
        };
        if value <= max {
            return Ty::Int(k);
        }
    }
    Ty::Int(ULLong)
}

/// Generic names retain operand types; the unsigned op identifies the arithmetic
/// operation only, not the signedness of any C operand or destination.
pub(crate) fn generic_overflow(name: &str) -> Option<OverflowOp> {
    match name {
        "add_overflow" => Some(OverflowOp::Uadd),
        "sub_overflow" => Some(OverflowOp::Usub),
        "mul_overflow" => Some(OverflowOp::Umul),
        _ => None,
    }
}

/// Frontend recognition is independent of the target CPU feature selection.
pub(crate) fn aes_builtin(base: &str) -> Option<keel_ir::Intrinsic> {
    use keel_ir::Intrinsic::*;
    Some(match base {
        "ia32_aesenc128" => X86Aesenc128,
        "ia32_aesenclast128" => X86Aesenclast128,
        "ia32_aesdec128" => X86Aesdec128,
        "ia32_aesdeclast128" => X86Aesdeclast128,
        "ia32_aesimc128" => X86Aesimc128,
        "ia32_aeskeygenassist128" => X86Aeskeygenassist128,
        _ => return None,
    })
}

/// Private scalar-count bridge to architectural shifts; not generic C shift semantics.
pub(crate) fn packed_shift_builtin(
    base: &str,
) -> Option<(keel_ir::Intrinsic, keel_ir::VectorType, bool)> {
    use keel_ir::{Intrinsic::*, VectorType::*};
    let (op, immediate) = if let Some(op) = base.strip_prefix("keel_sse2_imm_") {
        (op, true)
    } else {
        (base.strip_prefix("keel_sse2_count_")?, false)
    };
    let (id, shape) = match op {
        "psllw" => (X86Sse2Psllw128, I16x8),
        "pslld" => (X86Sse2Pslld128, I32x4),
        "psllq" => (X86Sse2Psllq128, I64x2),
        "psrlw" => (X86Sse2Psrlw128, I16x8),
        "psrld" => (X86Sse2Psrld128, I32x4),
        "psrlq" => (X86Sse2Psrlq128, I64x2),
        "psraw" => (X86Sse2Psraw128, I16x8),
        "psrad" => (X86Sse2Psrad128, I32x4),
        _ => return None,
    };
    Some((id, shape, immediate))
}

/// Bounded private frontend names; boolean fields are unsigned and word lanes.
pub(crate) fn saturating_builtin(base: &str) -> Option<(keel_ir::Intrinsic, bool, bool)> {
    use keel_ir::Intrinsic::*;
    Some(match base {
        "keel_sse2_paddsb" => (X86Sse2Paddsb128, false, false),
        "keel_sse2_paddsw" => (X86Sse2Paddsw128, false, true),
        "keel_sse2_paddusb" => (X86Sse2Paddusb128, true, false),
        "keel_sse2_paddusw" => (X86Sse2Paddusw128, true, true),
        "keel_sse2_psubsb" => (X86Sse2Psubsb128, false, false),
        "keel_sse2_psubsw" => (X86Sse2Psubsw128, false, true),
        "keel_sse2_psubusb" => (X86Sse2Psubusb128, true, false),
        "keel_sse2_psubusw" => (X86Sse2Psubusw128, true, true),
        _ => return None,
    })
}

/// Private typed bridge: preserve architectural rounding, invalid results and flags.
pub(crate) fn packed_conversion_builtin(base: &str) -> Option<keel_ir::Intrinsic> {
    use keel_ir::Intrinsic::*;
    Some(match base {
        "keel_sse2_cvtdq2ps" => X86Ssecvtdq2ps128,
        "keel_sse2_cvttps2dq" => X86Ssecvttps2dq128,
        "keel_sse2_cvtps2dq" => X86Ssecvtps2dq128,
        _ => return None,
    })
}

#[cfg(test)]
mod integer_layout_tests {
    use super::*;
    use crate::eval::{self, Base};
    use crate::strings::Strings;
    use bumpalo::Bump;

    // Expression-only seam: never invoke IR generation with the test model.
    // No production constructor selects WASM32.
    fn expression<'a>(src: &'a str, arena: &'a Bump, layout: CDataLayout) -> Expr<'a> {
        let mut p = Parser::new(
            src,
            "layout-expression.c",
            Target::default(),
            Strings::new(arena),
        );
        p.c_layout = layout;
        let e = p.expr().unwrap();
        assert!(matches!(p.peek(), Tok::Eof));
        e
    }

    #[test]
    fn literal_candidates_keep_rank_and_native_choices() {
        use IntKind::*;
        let arena = Bump::new();
        for (src, native, wasm) in [
            ("2147483647", Int, Int),
            ("2147483648", Long, LLong),
            ("4294967295", Long, LLong),
            ("0xffffffff", UInt, UInt),
            ("0x100000000", Long, LLong),
            ("0xffffffffL", Long, ULong),
            ("0x100000000L", Long, LLong),
            ("4294967295L", Long, LLong),
            ("4294967295UL", ULong, ULong),
            ("4294967296UL", ULong, ULLong),
            ("4294967296U", ULong, ULLong),
            ("9223372036854775807L", Long, LLong),
            ("0xffffffffffffffffL", ULong, ULLong),
            ("18446744073709551615", ULLong, ULLong),
            ("18446744073709551615LL", ULLong, ULLong),
            ("0xffffffffffffffffULL", ULLong, ULLong),
        ] {
            for (layout, expected) in [
                (CDataLayout::NATIVE_LP64, native),
                (CDataLayout::WASM32, wasm),
            ] {
                assert_eq!(
                    expression(src, &arena, layout).ty,
                    Ty::Int(expected),
                    "{src} {layout:?}"
                );
            }
        }
    }

    #[test]
    fn recursive_evaluation_and_conversions_interleave_models() {
        use IntKind::*;
        let arena = Bump::new();
        for (src, native_ty, native, wasm_ty, wasm) in [
            ("1U + -2L", Long, -1, ULong, 4294967295),
            ("0xffffffffUL + 1LL", ULLong, 4294967296, LLong, 4294967296),
            (
                "(long)(unsigned long)0xffffffffULL",
                Long,
                4294967295,
                Long,
                -1,
            ),
            ("(unsigned long)-1L", ULong, -1, ULong, 4294967295),
            (
                "(long)0x100000001ULL + (long)0xffffffffULL",
                Long,
                8589934592,
                Long,
                0,
            ),
            ("((long)0x80000000ULL >> 1) < 0", Int, 0, Int, 1),
            ("((unsigned long)-1L >> 31) == 1", Int, 0, Int, 1),
            (
                "(unsigned long)-1L / 2UL",
                ULong,
                i64::MAX,
                ULong,
                2147483647,
            ),
            (
                "(long)0x80000000ULL < 0 ? (long)0xffffffffULL : 7L",
                Long,
                7,
                Long,
                -1,
            ),
            ("(long)(0UL - 1UL)", Long, -1, Long, -1),
            ("~0UL", ULong, -1, ULong, 4294967295),
            ("(long)4294967295.0", Long, 4294967295, Long, -1),
        ] {
            // Return to native after wasm using shared interned type constants.
            for (layout, ty, value) in [
                (CDataLayout::NATIVE_LP64, native_ty, native),
                (CDataLayout::WASM32, wasm_ty, wasm),
                (CDataLayout::NATIVE_LP64, native_ty, native),
            ] {
                let e = expression(src, &arena, layout);
                assert_eq!(e.ty, Ty::Int(ty), "{src} {layout:?}");
                assert_eq!(
                    eval::eval_int_with_layout(&e, layout),
                    Some(value),
                    "{src} {layout:?}"
                );
                assert_eq!(
                    eval::eval_constant_with_layout(&e, false, layout),
                    Some(Const::Int(value))
                );
                if layout == CDataLayout::NATIVE_LP64 {
                    assert_eq!(eval_int(&e), Some(value));
                }
            }
        }
    }

    #[test]
    fn numeric_pointers_and_symbolic_relocation_boundary() {
        let arena = Bump::new();
        for (layout, pointer, difference) in [
            (CDataLayout::NATIVE_LP64, -1, 4294967296),
            (CDataLayout::WASM32, 4294967295, 0),
        ] {
            let e = expression("(void*)-1L", &arena, layout);
            assert_eq!(
                eval::eval_with_layout(&e, layout),
                Some(Const::Int(pointer))
            );
            let e = expression("(char*)0xffffffffULL + 2", &arena, layout);
            assert_eq!(
                eval::eval_with_layout(&e, layout),
                Some(Const::Int(if layout == CDataLayout::WASM32 {
                    1
                } else {
                    4294967297
                }))
            );
            let e = expression("(char*)0x100000001ULL - (char*)1", &arena, layout);
            assert_eq!(eval::eval_int_with_layout(&e, layout), Some(difference));
            let global = mk(ExprKind::Global(0), LONG, Loc::default());
            let addr = mk(ExprKind::Addr(&global), Ty::Ptr(&LONG), Loc::default());
            let cast = mk(ExprKind::Cast(&addr), LONG, Loc::default());
            assert_eq!(
                eval::eval_with_layout(&addr, layout),
                Some(Const::Addr(Base::Global(0), 0))
            );
            assert_eq!(
                eval::eval_with_layout(&cast, layout),
                if layout == CDataLayout::NATIVE_LP64 {
                    Some(Const::Addr(Base::Global(0), 0))
                } else {
                    None
                }
            );
            let truth = mk(ExprKind::Unary(UnOp::Not, &addr), INT, Loc::default());
            assert_eq!(eval::eval_with_layout(&truth, layout), None);
            assert_eq!(
                eval::eval_with_binding_and_layout(&truth, &|_| true, layout),
                Some(Const::Int(0))
            );
        }
    }

    #[test]
    fn recursive_addresses_hints_and_parser_queries_keep_the_model() {
        let arena = Bump::new();
        for (src, native, wasm) in [
            // Member address recursion must normalize after adding the offset;
            // the containing record also has different natural member offsets.
            (
                "(unsigned long)&((struct { char c; long n; }*)0xfffffffcULL)->n",
                4294967300,
                0,
            ),
            // Divide zero-extended address differences before long32 truncation.
            (
                "(long long*)0xfffffff8ULL - (long long*)0",
                536870911,
                536870911,
            ),
            ("(long*)0xfffffffcULL - (long*)4", 536870911, 1073741822),
            ("__builtin_expect((long)0xffffffffULL, 0)", 4294967295, -1),
            ("__builtin_choose_expr((long)0x100000000ULL, 9, 3)", 9, 3),
            (
                "((long)0x100000000ULL, (unsigned long)-1L) > 0xffffffffULL",
                1,
                0,
            ),
            ("(long)0x100000000ULL || (long)0xffffffffULL < 0", 1, 1),
            ("(long)0x100000000ULL && (1 / 0)", -1, 0),
        ] {
            for (layout, expected) in [
                (CDataLayout::NATIVE_LP64, native),
                (CDataLayout::WASM32, wasm),
            ] {
                let e = expression(src, &arena, layout);
                let expected = if src.contains("&&") && layout == CDataLayout::NATIVE_LP64 {
                    None
                } else {
                    Some(expected)
                };
                assert_eq!(
                    eval::eval_int_with_layout(&e, layout),
                    expected,
                    "{src} {layout:?}"
                );
            }
        }
    }

    #[test]
    fn native_proofs_explicitly_reject_the_private_model() {
        let arena = Bump::new();
        let e = expression("1L + 2L", &arena, CDataLayout::WASM32);
        assert_eq!(integer_constant_control(&e, CDataLayout::WASM32), None);
        assert_eq!(broadcast_constant(&e, CDataLayout::WASM32), None);
        let e = expression("1L + 2L", &arena, CDataLayout::NATIVE_LP64);
        assert_eq!(
            integer_constant_control(&e, CDataLayout::NATIVE_LP64),
            Some(3)
        );
        assert_eq!(
            broadcast_constant(&e, CDataLayout::NATIVE_LP64),
            Some(Const::Int(3))
        );

        // Exercise the broadcast entry even without a declaration-created vector.
        let mut p = Parser::new("", "proof.c", Target::default(), Strings::new(&arena));
        p.c_layout = CDataLayout::WASM32;
        let ty = Ty::Vector(&INT, keel_ir::VectorType::I32x4);
        assert!(p.vector_broadcast(e, ty, Loc::default()).is_err());
    }

    #[test]
    fn wide_casts_and_constant_proof_safeguards_stay_bounded() {
        let arena = Bump::new();
        for layout in [CDataLayout::NATIVE_LP64, CDataLayout::WASM32] {
            // A 64-bit unsigned value above i64::MAX cannot be represented by
            // this evaluator after widening; wasm32 unsigned long can be.
            let e = expression("(__int128)(unsigned long)-1", &arena, layout);
            assert_eq!(
                eval::eval_int_with_layout(&e, layout),
                if layout == CDataLayout::WASM32 {
                    Some(4294967295)
                } else {
                    None
                }
            );
            let e = expression("(__int128)0xffffffffffffffffULL", &arena, layout);
            assert_eq!(eval::eval_int_with_layout(&e, layout), None);
            let e = expression("1 / 0", &arena, layout);
            assert_eq!(eval::eval_int_with_layout(&e, layout), None);
            let e = expression("(1, 2)", &arena, layout);
            assert_eq!(eval::eval_constant_with_layout(&e, false, layout), None);
            assert_eq!(
                eval::eval_constant_with_layout(&e, true, layout),
                Some(Const::Int(2))
            );
            let e = expression("0 && (1 / 0)", &arena, layout);
            assert_eq!(
                eval::eval_constant_with_layout(&e, false, layout),
                Some(Const::Int(0))
            );
        }
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
        for src in ["(int)(__int128)1", "(int)(void*)1", "(1, 2)"] {
            let e = expression(src, &arena, CDataLayout::NATIVE_LP64);
            assert_eq!(
                broadcast_constant(&e, CDataLayout::NATIVE_LP64),
                None,
                "{src}"
            );
        }
    }
}
