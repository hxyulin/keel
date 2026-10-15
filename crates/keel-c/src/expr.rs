//! Expressions, with C's implicit conversions made explicit as they are
//! parsed.

use crate::ast::*;
use crate::eval::{Const, eval_constant, eval_int, fits_i64, normalize};
use crate::lex::Tok;
use crate::parse::{Parser, R};
use crate::types::*;
use crate::{Error, Loc};
use keel_ir::Target;
use keel_ir::{OverflowOp, RmwOp};

pub(crate) fn mk<'a>(kind: ExprKind<'a>, ty: Ty, loc: Loc) -> Expr<'a> {
    Expr {
        kind,
        ty,
        loc,
        sugar: Sugar::Plain,
    }
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

const ASSIGN_OPS: [&str; 11] = [
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
];

impl<'a> Parser<'a> {
    /// A full expression, commas included.
    pub(crate) fn expr(&mut self) -> R<Expr<'a>> {
        let mut e = self.assign()?;
        while self.is(",") {
            let loc = self.loc();
            self.advance();
            let r = self.assign()?;
            let r = self.rvalue(r);
            let ty = r.ty.clone();
            let sugar = r.sugar.unqualified(&self.typedefs).clone();
            e = mk(ExprKind::Comma(Box::new(e), Box::new(r)), ty, loc).with_sugar(sugar);
        }
        Ok(e)
    }

    pub(crate) fn assign(&mut self) -> R<Expr<'a>> {
        let lhs = self.conditional()?;
        let Tok::Punct(p) = self.peek() else {
            return Ok(lhs);
        };
        if !ASSIGN_OPS.contains(&p) {
            return Ok(lhs);
        }
        let loc = self.loc();
        self.advance();
        let rhs = self.assign()?;
        self.assignment(p, lhs, rhs, loc)
    }

    fn assignment(&mut self, op: &str, lhs: Expr<'a>, rhs: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        self.check_lvalue(&lhs)?;
        if let Ty::Atomic(ty) = &lhs.ty {
            let ty = ty.as_ref().clone();
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
            let sugar = lhs.sugar.unqualified(&self.typedefs).clone();
            let args = vec![
                self.address_of_atomic(lhs),
                rhs,
                mk(ExprKind::Int(5), INT, loc),
            ];
            return Ok(mk(ExprKind::Builtin(builtin, args), ty, loc).with_sugar(sugar));
        }
        let ty = lhs.ty.clone();
        let result_sugar = lhs.sugar.unqualified(&self.typedefs).clone();
        if op == "=" {
            let rhs = self.convert_for_assign(rhs, &ty)?;
            let sugar = lhs.sugar.unqualified(&self.typedefs).clone();
            return Ok(
                mk(ExprKind::Assign(Box::new(lhs), Box::new(rhs)), ty, loc).with_sugar(sugar)
            );
        }
        let rhs = self.rvalue(rhs);
        let bin = &op[..op.len() - 1];
        if ty.is_ptr() && (bin == "+" || bin == "-") && rhs.ty.is_integer() {
            let size = self.elem_size(&ty, loc)?;
            let mut n = self.cast(rhs, LONG);
            if bin == "-" {
                n = mk(ExprKind::Unary(UnOp::Neg, Box::new(n)), LONG, loc);
            }
            return Ok(mk(
                ExprKind::PtrOpAssign {
                    lhs: Box::new(lhs),
                    rhs: Box::new(n),
                    size,
                },
                ty,
                loc,
            )
            .with_sugar(result_sugar));
        }
        let op = bin_op(bin);
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
                let comp = usual(&ty, &rhs.ty);
                if matches!(op, BinOp::Rem | BinOp::And | BinOp::Or | BinOp::Xor)
                    && !comp.is_integer()
                {
                    return Err(Error::new(loc, "operation needs integers"));
                }
                comp
            }
        };
        let rhs = self.cast(rhs, comp.clone());
        Ok(mk(
            ExprKind::OpAssign {
                op,
                ty: comp,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            ty,
            loc,
        ))
    }

    fn check_lvalue(&self, e: &Expr<'a>) -> R<()> {
        let ok = match &e.kind {
            ExprKind::Local(_)
            | ExprKind::Deref(_)
            | ExprKind::Str(_)
            | ExprKind::CompoundLit(..) => true,
            ExprKind::Global(_) => !matches!(e.ty, Ty::Func(_)),
            ExprKind::Member { base, .. } => self.check_lvalue(base).is_ok(),
            _ => false,
        };
        if ok && !matches!(e.ty, Ty::Array(..)) {
            Ok(())
        } else {
            Err(Error::new(e.loc, "not assignable"))
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
            let ty = cond.ty.clone();
            let sugar = cond.sugar.clone();
            let t = self.new_local(name, ty.clone(), sugar.clone(), false);
            let local = || mk(ExprKind::Local(t), ty.clone(), loc).with_sugar(sugar.clone());
            let set = self.assignment("=", local(), cond, loc)?;
            let pick = self.conditional_of(local(), local(), b, loc)?;
            let ty = pick.ty.clone();
            let sugar = pick.sugar.clone();
            return Ok(
                mk(ExprKind::Comma(Box::new(set), Box::new(pick)), ty, loc).with_sugar(sugar)
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
            usual(&a.ty, &b.ty)
        } else if a.ty.is_ptr() && is_null(&b) {
            a.ty.clone()
        } else if b.ty.is_ptr() && is_null(&a) {
            b.ty.clone()
        } else if a.ty.is_ptr() && b.ty.is_ptr() {
            if a.ty.pointee() == Some(&Ty::Void) {
                a.ty.clone()
            } else {
                b.ty.clone()
            }
        } else if a.ty == b.ty || a.ty == Ty::Void || b.ty == Ty::Void {
            if a.ty == Ty::Void {
                a.ty.clone()
            } else {
                b.ty.clone()
            }
        } else {
            return Err(Error::new(loc, "mismatched types in `?:`"));
        };
        let (a, b) = if ty == Ty::Void {
            (a, b)
        } else {
            (self.cast(a, ty.clone()), self.cast(b, ty.clone()))
        };
        let sugar = if ty.is_ptr() {
            a.sugar
                .unqualified(&self.typedefs)
                .merge(b.sugar.unqualified(&self.typedefs), &self.typedefs)
        } else {
            Sugar::Plain
        };
        Ok(mk(
            ExprKind::Cond(Box::new(cond), Box::new(a), Box::new(b)),
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
        match p {
            "&&" | "||" => {
                if !l.ty.is_scalar() || !r.ty.is_scalar() {
                    return Err(Error::new(loc, format!("`{p}` needs scalars")));
                }
                let kind = if p == "&&" {
                    ExprKind::LogAnd(Box::new(l), Box::new(r))
                } else {
                    ExprKind::LogOr(Box::new(l), Box::new(r))
                };
                Ok(mk(kind, INT, loc))
            }
            "+" if l.ty.is_ptr() && r.ty.is_integer() => self.ptr_add(l, r, false, loc),
            "+" if l.ty.is_integer() && r.ty.is_ptr() => self.ptr_add(r, l, false, loc),
            "-" if l.ty.is_ptr() && r.ty.is_integer() => self.ptr_add(l, r, true, loc),
            "-" if l.ty.is_ptr() && r.ty.is_ptr() => {
                let size = self.elem_size(&l.ty, loc)?;
                Ok(mk(
                    ExprKind::PtrDiff(Box::new(l), Box::new(r), size),
                    LONG,
                    loc,
                ))
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                let op = bin_op(p);
                let (l, r) = if l.ty.is_arith() && r.ty.is_arith() {
                    let ty = usual(&l.ty, &r.ty);
                    (self.cast(l, ty.clone()), self.cast(r, ty))
                } else if l.ty.is_ptr() && r.ty.is_integer() {
                    let ty = l.ty.clone();
                    (l, self.cast(r, ty))
                } else if l.ty.is_integer() && r.ty.is_ptr() {
                    let ty = r.ty.clone();
                    (self.cast(l, ty), r)
                } else if l.ty.is_ptr() && r.ty.is_ptr() {
                    let ty = l.ty.clone();
                    (l, self.cast(r, ty))
                } else {
                    return Err(Error::new(loc, format!("invalid operands to `{p}`")));
                };
                Ok(mk(ExprKind::Binary(op, Box::new(l), Box::new(r)), INT, loc))
            }
            "<<" | ">>" => {
                if !l.ty.is_integer() || !r.ty.is_integer() {
                    return Err(Error::new(loc, "shift needs integers"));
                }
                let l = self.promote(l);
                let ty = l.ty.clone();
                let r = self.cast(r, ty.clone());
                Ok(mk(
                    ExprKind::Binary(bin_op(p), Box::new(l), Box::new(r)),
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
                let ty = usual(&l.ty, &r.ty);
                let (l, r) = (self.cast(l, ty.clone()), self.cast(r, ty.clone()));
                Ok(mk(ExprKind::Binary(op, Box::new(l), Box::new(r)), ty, loc))
            }
        }
    }

    fn ptr_add(&mut self, ptr: Expr<'a>, n: Expr<'a>, negate: bool, loc: Loc) -> R<Expr<'a>> {
        let size = self.elem_size(&ptr.ty, loc)?;
        let mut n = self.cast(n, LONG);
        if negate {
            n = mk(ExprKind::Unary(UnOp::Neg, Box::new(n)), LONG, loc);
        }
        let ty = ptr.ty.clone();
        let sugar = ptr.sugar.unqualified(&self.typedefs).clone();
        Ok(mk(ExprKind::PtrAdd(Box::new(ptr), Box::new(n), size), ty, loc).with_sugar(sugar))
    }

    /// The size of what a pointer points to, for arithmetic. `void *`
    /// steps by bytes, as in GNU C.
    fn elem_size(&self, ptr: &Ty, loc: Loc) -> R<u64> {
        let elem = ptr.pointee().unwrap();
        match elem {
            Ty::Void => Ok(1),
            Ty::Func(_) => Err(Error::new(loc, "arithmetic on a function pointer")),
            Ty::Struct(id) if !self.structs[*id].complete => Err(Error::new(
                loc,
                "arithmetic on a pointer to an incomplete type",
            )),
            _ => Ok(self.size_of(elem)),
        }
    }

    fn cast_expr(&mut self) -> R<Expr<'a>> {
        if self.is("(") && self.type_follows(1) {
            let loc = self.loc();
            self.advance();
            let (ty, sugar) = self.type_name_sugar()?;
            self.expect(")")?;
            if self.is("{") {
                let lit = self
                    .compound_literal(ty, sugar.clone(), loc)?
                    .with_sugar(sugar.clone());
                if let ExprKind::CompoundLit(id, _) = &lit.kind {
                    self.fs.as_mut().unwrap().locals[*id].sugar = sugar;
                    self.mark_in_memory(*id);
                }
                return self.postfix_ops(lit);
            }
            let sugar = sugar.unqualified(&self.typedefs).clone();
            let e = self.cast_expr()?;
            let e = self.rvalue(e);
            let ok = ty == Ty::Void
                || (ty.is_scalar()
                    && e.ty.is_scalar()
                    && !(ty.is_float() && e.ty.is_ptr())
                    && !(ty.is_ptr() && e.ty.is_float()))
                || (ty.is_struct() && ty == e.ty);
            if !ok {
                return Err(Error::new(loc, "invalid cast"));
            }
            // Literal conversion cannot hide a qualified memory read;
            // retain the frontend's existing constant normalization.
            if matches!(e.kind, ExprKind::Int(_)) {
                return Ok(self.cast(e, ty).with_sugar(sugar));
            }
            // Keep a distinct value conversion even when the unqualified
            // types match: changing result qualifiers must not rewrite the
            // qualifiers on the operand's actual memory read.
            return Ok(mk(ExprKind::Cast(Box::new(e)), ty, loc).with_sugar(sugar));
        }
        self.unary()
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
            "sizeof" | "_Alignof" | "__alignof__" | "__alignof" => {
                self.advance();
                let (ty, alignment) = if self.is("(") && self.type_follows(1) {
                    self.advance();
                    let (ty, sugar) = self.type_name_sugar()?;
                    self.expect(")")?;
                    if self.is("{") {
                        let e = self.compound_literal(ty, sugar.clone(), loc)?;
                        let align = self.expr_alignment(&e);
                        (e.ty, align)
                    } else {
                        let align = self.type_alignment(&ty, &sugar);
                        (ty, align)
                    }
                } else {
                    let e = self.unary()?;
                    let align = self.expr_alignment(&e);
                    (e.ty, align)
                };
                let incomplete = match &ty {
                    Ty::Func(_) | Ty::Array(_, None) => true,
                    Ty::Struct(id) => !self.structs[*id].complete,
                    _ => false,
                };
                if incomplete {
                    return Err(Error::new(loc, "`sizeof` of an incomplete type"));
                }
                let n = if word == "sizeof" {
                    self.size_of(&ty)
                } else {
                    alignment
                };
                Ok(mk(ExprKind::Int(n as i64), ULONG, loc))
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
                Ok(mk(ExprKind::Deref(Box::new(e)), pointee, loc).with_sugar(sugar))
            }
            "+" | "-" | "~" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                if !e.ty.is_arith() || (word == "~" && !e.ty.is_integer()) {
                    return Err(Error::new(loc, format!("invalid operand to `{word}`")));
                }
                let e = self.promote(e);
                if word == "+" {
                    return Ok(e);
                }
                let op = if word == "-" { UnOp::Neg } else { UnOp::BitNot };
                let ty = e.ty.clone();
                Ok(mk(ExprKind::Unary(op, Box::new(e)), ty, loc))
            }
            "!" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                if !e.ty.is_scalar() {
                    return Err(Error::new(loc, "`!` needs a scalar"));
                }
                Ok(mk(ExprKind::Unary(UnOp::Not, Box::new(e)), INT, loc))
            }
            _ => self.postfix(),
        }
    }

    pub(crate) fn address_of(&mut self, e: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
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
        let ty = self.ptr_type(e.ty.clone());
        let sugar = Sugar::Ptr(self.sugar_ref(e.sugar.clone()));
        Ok(mk(ExprKind::Addr(Box::new(e)), ty, loc).with_sugar(sugar))
    }

    /// A local whose address is taken must live in memory.
    fn mark_address_taken(&mut self, e: &Expr<'a>) {
        match &e.kind {
            ExprKind::Local(id) => self.mark_in_memory(*id),
            ExprKind::Member { base, .. } => self.mark_address_taken(base),
            _ => {}
        }
    }

    fn incdec(&mut self, e: Expr<'a>, inc: bool, prefix: bool, loc: Loc) -> R<Expr<'a>> {
        self.check_lvalue(&e)?;
        if let Ty::Atomic(ty) = &e.ty {
            let ty = ty.as_ref().clone();
            // Pointer RMWs operate on the address bits, with a byte step
            // scaled by the pointee's size. Both targets have 64-bit
            // pointers; the IR's add/sub RMWs take integers, not pointers.
            let (step, rmw_ty) = if ty.is_ptr() {
                (self.elem_size(&ty, loc)? as i64, LONG)
            } else {
                (1, ty.clone())
            };
            let step = mk(ExprKind::Int(step), LONG, loc);
            let step = self.cast(step, rmw_ty.clone());
            let op = if inc { RmwOp::Add } else { RmwOp::Sub };
            let sugar = e.sugar.unqualified(&self.typedefs).clone();
            let args = vec![
                self.address_of_atomic(e),
                step,
                mk(ExprKind::Int(5), INT, loc),
            ];
            let builtin = Builtin::AtomicRmw { op, new: prefix };
            let result = mk(ExprKind::Builtin(builtin, args), rmw_ty, loc);
            return Ok(self.cast(result, ty).with_sugar(sugar));
        }
        if !e.ty.is_scalar() {
            return Err(Error::new(loc, "`++`/`--` needs a scalar"));
        }
        if e.ty.is_ptr() {
            self.elem_size(&e.ty, loc)?;
        }
        let ty = e.ty.clone();
        let sugar = e.sugar.unqualified(&self.typedefs).clone();
        Ok(mk(
            ExprKind::IncDec {
                lhs: Box::new(e),
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
            if self.eat("[") {
                let idx = self.expr()?;
                self.expect("]")?;
                let sum = self.binop("+", e, idx, loc)?;
                let Some(elem) = sum.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "subscript of a non-pointer"));
                };
                let sugar = sum.sugar.pointee(&self.typedefs);
                e = mk(ExprKind::Deref(Box::new(sum)), elem, loc).with_sugar(sugar);
            } else if self.eat("(") {
                e = self.call(e, loc)?;
            } else if self.eat(".") {
                e = self.member(e, loc)?;
            } else if self.eat("->") {
                let p = self.rvalue(e);
                let Some(pointee) = p.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "`->` on a non-pointer"));
                };
                let sugar = p.sugar.pointee(&self.typedefs);
                e = mk(ExprKind::Deref(Box::new(p)), pointee, loc).with_sugar(sugar);
                e = self.member(e, loc)?;
            } else if self.is("++") || self.is("--") {
                let inc = self.is("++");
                self.advance();
                e = self.incdec(e, inc, false, loc)?;
            } else {
                return Ok(e);
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
        let Some((offset, m)) = def.find(&name, &self.structs, &self.typedefs) else {
            return Err(Error::new(loc, format!("no member `{name}`")));
        };
        let mut sugar = m.sugar.clone();
        if base.sugar.is_volatile(&self.typedefs) {
            sugar = Sugar::Volatile(self.sugar_ref(sugar));
        }
        Ok(mk(
            ExprKind::Member {
                base: Box::new(base),
                offset,
                bits: m.bits,
                align: m.align,
            },
            m.ty,
            loc,
        )
        .with_sugar(sugar))
    }

    fn call(&mut self, callee: Expr<'a>, loc: Loc) -> R<Expr<'a>> {
        let mut args = Vec::new();
        while !self.eat(")") {
            if !args.is_empty() {
                self.expect(",")?;
            }
            args.push(self.assign()?);
        }
        self.make_call(callee, args, loc)
    }

    fn make_call(&mut self, callee: Expr<'a>, raw_args: Vec<Expr<'a>>, loc: Loc) -> R<Expr<'a>> {
        let f = self.rvalue(callee);
        let Some(fty) = f.ty.pointee().and_then(Ty::func).cloned() else {
            return Err(Error::new(loc, "call of a non-function"));
        };
        let mut args = Vec::new();
        for (i, arg) in raw_args.into_iter().enumerate() {
            let arg = if let Some(param) = fty.params.get(i) {
                self.convert_for_assign(arg, param)?
            } else if fty.variadic || !fty.prototyped {
                self.default_promote(arg)
            } else {
                return Err(Error::new(arg.loc, "too many arguments"));
            };
            args.push(arg);
        }
        if args.len() < fty.params.len() {
            return Err(Error::new(loc, "too few arguments"));
        }
        if let Ty::Struct(id) = fty.ret
            && !self.structs[id].complete
        {
            return Err(Error::new(loc, "call returns an incomplete type"));
        }
        let returned = f.sugar.function_return(&self.typedefs);
        let sugar = returned.unqualified(&self.typedefs).clone();
        Ok(mk(ExprKind::Call(Box::new(f), args), fty.ret.clone(), loc).with_sugar(sugar))
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
                int_literal_type(value, decimal, unsigned, longs),
                loc,
            )),
            Tok::Float { value, single } => Ok(mk(
                ExprKind::Float(value),
                if single { Ty::Float } else { Ty::Double },
                loc,
            )),
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
                let value = match stmts.pop() {
                    Some(Stmt::Expr(e)) => Some(Box::new(self.rvalue(e))),
                    Some(s) => {
                        stmts.push(s);
                        None
                    }
                    None => None,
                };
                let ty = value.as_ref().map_or(Ty::Void, |v| v.ty.clone());
                let sugar = value.as_ref().map_or(Sugar::Plain, |v| {
                    v.sugar.unqualified(&self.typedefs).clone()
                });
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
                    return self.builtin(&name, loc);
                }
                if name.starts_with("__atomic_")
                    || name.starts_with("__sync_")
                    || name.starts_with("__c11_atomic_")
                {
                    return self.atomic(&name, loc);
                }
                self.lookup_value(&name, loc)
            }
            _ => unreachable!(),
        }
    }

    /// C11's `_Generic(e, type: a, ..., default: b)`: the association
    /// whose type is that of `e`'s value, unqualified.
    fn generic(&mut self, loc: Loc) -> R<Expr<'a>> {
        self.expect("(")?;
        let e = self.assign()?;
        let ty = self.rvalue(e).ty.unatomic().clone();
        let (mut chosen, mut default) = (None, None);
        while self.eat(",") {
            let is_default = self.eat("default");
            let assoc_ty = if is_default {
                None
            } else {
                Some(self.type_name()?)
            };
            self.expect(":")?;
            let value = self.assign()?;
            match assoc_ty {
                None => default = Some(value),
                Some(t) if t == ty => chosen = Some(value),
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
        let elem = if wide { INT } else { CHAR };
        mk(
            ExprKind::Str(i),
            Ty::Array(self.type_ref(elem), Some(len)),
            loc,
        )
    }

    fn builtin(&mut self, name: &str, loc: Loc) -> R<Expr<'a>> {
        let base = &name["__builtin_".len()..];
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
            let params = [ty.clone(), ty.clone(), self.ptr_type(ty)];
            let args = args
                .into_iter()
                .zip(&params)
                .map(|(arg, ty)| self.convert_for_assign(arg, ty))
                .collect::<R<Vec<_>>>()?;
            return Ok(mk(
                ExprKind::Builtin(Builtin::Overflow(op), args),
                Ty::Int(IntKind::Bool),
                loc,
            ));
        }
        match base {
            "va_start" | "va_arg" | "va_end" | "va_copy" => {
                self.expect("(")?;
                let ap = self.assign()?;
                let e = match base {
                    "va_start" => {
                        self.expect(",")?;
                        self.assign()?;
                        let ap = self.va_list_addr(ap);
                        mk(ExprKind::VaStart(Box::new(ap)), Ty::Void, loc)
                    }
                    "va_arg" => {
                        self.expect(",")?;
                        let ty = self.type_name()?;
                        if !ty.is_scalar() {
                            return Err(Error::new(loc, "`va_arg` of a non-scalar"));
                        }
                        let ap = self.va_list_addr(ap);
                        mk(ExprKind::VaArg(Box::new(ap)), ty, loc)
                    }
                    // SysV's va_list is an array, so copying is copying the
                    // struct it holds.
                    "va_copy" if self.target == Target::X86_64LinuxGnu => {
                        self.expect(",")?;
                        let src = self.assign()?;
                        let [dst, src] = [ap, src].map(|e| {
                            let p = self.va_list_addr(e);
                            let ty = p.ty.pointee().cloned().expect("a pointer");
                            mk(ExprKind::Deref(Box::new(p)), ty, loc)
                        });
                        self.assignment("=", dst, src, loc)?
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
                let mut offset = 0;
                loop {
                    if self.eat("[") {
                        let i = self.const_int()?;
                        self.expect("]")?;
                        let Some(elem) = ty.pointee().cloned() else {
                            return Err(Error::new(loc, "`offsetof` subscript of a non-array"));
                        };
                        offset += i as u64 * self.size_of(&elem);
                        ty = elem;
                        continue;
                    }
                    if !offset_member_follows(self, offset == 0 && matches!(ty, Ty::Struct(_))) {
                        break;
                    }
                    let name = self.ident()?;
                    let Ty::Struct(id) = ty else {
                        return Err(Error::new(loc, "`offsetof` member of a non-struct"));
                    };
                    let Some((off, m)) =
                        self.structs[id].find(&name, &self.structs, &self.typedefs)
                    else {
                        return Err(Error::new(loc, format!("no member `{name}`")));
                    };
                    offset += off;
                    ty = m.ty;
                }
                self.expect(")")?;
                Ok(mk(ExprKind::Int(offset as i64), ULONG, loc))
            }
            "expect" => self.builtin_call(Builtin::Hint, &[LONG, LONG], LONG, loc),
            "expect_with_probability" => {
                let args = self.builtin_args(3, loc)?;
                let mut args = args.into_iter();
                let value = self.convert_for_assign(args.next().unwrap(), &LONG)?;
                let expected = self.convert_for_assign(args.next().unwrap(), &LONG)?;
                let probability = self.convert_for_assign(args.next().unwrap(), &Ty::Double)?;
                let Some(Const::Float(p)) = eval_constant(&probability, true) else {
                    return Err(Error::new(
                        loc,
                        "probability must be a constant floating-point expression",
                    ));
                };
                if !p.is_finite() || !(0.0..=1.0).contains(&p) {
                    return Err(Error::new(loc, "probability must be between 0 and 1"));
                }
                Ok(mk(
                    ExprKind::Builtin(Builtin::Hint, vec![value, expected]),
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
                let alignment = self.cast(args[1].clone(), ULONG);
                let Some(Const::Int(n)) = eval_constant(&alignment, false) else {
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
                    ExprKind::Builtin(Builtin::AssumeAligned(n as u64), values),
                    ptr,
                    loc,
                ))
            }
            "object_size" | "dynamic_object_size" => {
                // The size is never known here: "unknown" is -1 for types 0
                // and 1 (maximum), 0 for types 2 and 3 (minimum).
                let args = self.builtin_args(2, loc)?;
                let Some(kind) = eval_int(&args[1]) else {
                    return Err(Error::new(
                        loc,
                        "`__builtin_object_size` type must be constant",
                    ));
                };
                let v = if kind & 2 == 0 { -1 } else { 0 };
                Ok(mk(ExprKind::Int(v), ULONG, loc))
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
                let Some(c) = eval_int(&c) else {
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
                let known = eval_int(&args[0]).is_some();
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
                self.builtin_call(Builtin::Fabs, std::slice::from_ref(&ty), ty.clone(), loc)
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
            "alloca" => {
                let ptr = self.ptr_type(Ty::Void);
                self.builtin_call(Builtin::Alloca, &[ULONG], ptr, loc)
            }
            "frame_address" => {
                let args = self.builtin_args(1, loc)?;
                if eval_int(&args[0]) != Some(0) {
                    return Err(Error::new(
                        loc,
                        "`__builtin_frame_address` is only supported for frame 0",
                    ));
                }
                Ok(mk(
                    ExprKind::Builtin(Builtin::FrameAddress, Vec::new()),
                    self.ptr_type(Ty::Void),
                    loc,
                ))
            }
            "trap" | "unreachable" => self.builtin_call(Builtin::Trap, &[], Ty::Void, loc),
            "debugtrap" => self.builtin_call(Builtin::Debugtrap, &[], Ty::Void, loc),
            _ => {
                // `__builtin_memcpy` and friends: the library function.
                if let Ok(e) = self.lookup_value(base, loc)
                    && matches!(e.ty, Ty::Func(_))
                {
                    return Ok(e);
                }
                // `_FORTIFY_SOURCE`'s `__builtin___memcpy_chk(dst, src, n,
                // size)`: the size of `dst` is never known here (see
                // `object_size`), so the check cannot fail and this is
                // `memcpy(dst, src, n)`.
                if let Some(plain) = base.strip_prefix("__").and_then(|b| b.strip_suffix("_chk"))
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
        // The operation, and how many arguments it has once in shape.
        let (builtin, n) = match op {
            "synchronize" | "thread_fence" | "signal_fence" => {
                // A signal fence only has to stop the compiler; a real
                // fence is stronger than needed but correct.
                want(&args, 1)?;
                return Ok(mk(ExprKind::Builtin(Builtin::Fence, args), Ty::Void, loc));
            }
            "always_lock_free" | "is_lock_free" => {
                let size = args.first().and_then(eval_int);
                let free = matches!(size, Some(1 | 2 | 4 | 8));
                return Ok(mk(
                    ExprKind::Int(i64::from(free)),
                    Ty::Int(IntKind::Bool),
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
                args.truncate(if weak_arg { 5 } else { 4 });
                if weak_arg {
                    args.remove(3);
                }
                (Builtin::AtomicCompareExchange, 4)
            }
            "test_and_set" | "clear" => {
                // On a byte: set to 1 (giving whether it was set) or 0.
                want(&args, 2)?;
                let ptr = args.remove(0);
                let ty = self.ptr_type(Ty::Int(IntKind::UChar));
                let ptr = self.cast(ptr, ty);
                let value = mk(ExprKind::Int(i64::from(op == "test_and_set")), INT, loc);
                let value = self.cast(value, Ty::Int(IntKind::UChar));
                let args = vec![ptr, value, args.remove(0)];
                if op == "clear" {
                    return Ok(mk(
                        ExprKind::Builtin(Builtin::AtomicStore, args),
                        Ty::Void,
                        loc,
                    ));
                }
                let builtin = Builtin::AtomicRmw {
                    op: RmwOp::Xchg,
                    new: false,
                };
                let old = mk(
                    ExprKind::Builtin(builtin, args),
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
                let ptr_ty = args.first().map(|a| a.ty.clone());
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
        let Some(ty) = args[0].ty.pointee().map(|t| t.unatomic().clone()) else {
            return Err(Error::new(loc, format!("`{name}` needs a pointer")));
        };
        if !(ty.is_integer() || ty.is_ptr()) {
            return Err(Error::new(
                loc,
                format!("`{name}` is only supported on integers and pointers"),
            ));
        }
        // Arithmetic on a pointer is on its address, in bytes.
        let arith = matches!(builtin, Builtin::AtomicRmw { op, .. } if op != RmwOp::Xchg);
        let value_ty = if arith && ty.is_ptr() {
            ULONG
        } else {
            ty.clone()
        };
        let pointee = ty;
        let value_args = match builtin {
            Builtin::AtomicLoad => 0..0,
            Builtin::AtomicCas | Builtin::AtomicCasBool => 1..3,
            Builtin::AtomicCompareExchange => 2..3,
            _ => 1..2,
        };
        for i in value_args {
            let v = args[i].clone();
            args[i] = self.convert_for_assign(v, &value_ty)?;
        }
        if builtin == Builtin::AtomicCompareExchange && !args[1].ty.is_ptr() {
            return Err(Error::new(
                loc,
                format!("`{name}` needs a pointer to the expected value"),
            ));
        }
        let ty = match builtin {
            // A store's value is only needed in an assignment.
            Builtin::AtomicStore => Ty::Void,
            Builtin::AtomicCasBool | Builtin::AtomicCompareExchange => Ty::Int(IntKind::Bool),
            _ => value_ty.clone(),
        };
        let sugar = args[0].sugar.pointee(&self.typedefs);
        let result = mk(ExprKind::Builtin(builtin, args), ty, loc);
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
            if !arg.ty.is_arith() {
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
            let arg = self.cast(arg, ty.clone());
            let name = self.unique_name("fclass");
            let t = self.new_local(name, ty.clone(), Sugar::Plain, false);
            sets.push(self.assignment("=", mk(ExprKind::Local(t), ty.clone(), loc), arg, loc)?);
            operands.push(mk(ExprKind::Local(t), ty, loc));
        }
        let x = operands[0].clone();
        let ty = x.ty.clone();
        let float = |v: f64| mk(ExprKind::Float(v), ty.clone(), loc);
        let fabs = mk(
            ExprKind::Builtin(Builtin::Fabs, vec![x.clone()]),
            ty.clone(),
            loc,
        );
        let int = |v: i64| mk(ExprKind::Int(v), INT, loc);
        let min_normal = if ty == Ty::Float {
            f64::from(f32::MIN_POSITIVE)
        } else {
            f64::MIN_POSITIVE
        };
        let cmp = |this: &mut Self, p: &str, a: Expr<'a>, b: Expr<'a>| this.binop(p, a, b, loc);
        let result = match base {
            "isnan" => cmp(self, "!=", x.clone(), x)?,
            "isinf" => cmp(self, "==", fabs, float(f64::INFINITY))?,
            "isfinite" => cmp(self, "<", fabs, float(f64::INFINITY))?,
            "isnormal" => {
                let low = cmp(self, ">=", fabs.clone(), float(min_normal))?;
                let high = cmp(self, "<", fabs, float(f64::INFINITY))?;
                cmp(self, "&&", low, high)?
            }
            "isinf_sign" => {
                let pos = cmp(self, "==", x.clone(), float(f64::INFINITY))?;
                let neg = cmp(self, "==", x, float(f64::NEG_INFINITY))?;
                let neg = self.conditional_of(neg, int(-1), int(0), loc)?;
                self.conditional_of(pos, int(1), neg, loc)?
            }
            "signbit" | "signbitf" | "signbitl" => {
                mk(ExprKind::Builtin(Builtin::Signbit, vec![x]), INT, loc)
            }
            // fpclassify(nan, infinite, normal, subnormal, zero, x)
            "fpclassify" => {
                let [nan, inf, normal, subnormal, zero]: [Expr; 5] =
                    args.try_into().expect("five classes");
                let is_zero = cmp(self, "==", x.clone(), float(0.0))?;
                let tiny = self.conditional_of(is_zero, zero, subnormal, loc)?;
                let is_normal = cmp(self, ">=", fabs.clone(), float(min_normal))?;
                let finite = self.conditional_of(is_normal, normal, tiny, loc)?;
                let is_inf = cmp(self, "==", fabs, float(f64::INFINITY))?;
                let number = self.conditional_of(is_inf, inf, finite, loc)?;
                let is_number = cmp(self, "==", x.clone(), x)?;
                self.conditional_of(is_number, number, nan, loc)?
            }
            _ => {
                let y = operands[1].clone();
                match base {
                    "isgreater" => cmp(self, ">", x, y)?,
                    "isgreaterequal" => cmp(self, ">=", x, y)?,
                    "isless" => cmp(self, "<", x, y)?,
                    "islessequal" => cmp(self, "<=", x, y)?,
                    "islessgreater" => {
                        let lt = cmp(self, "<", x.clone(), y.clone())?;
                        let gt = cmp(self, ">", x, y)?;
                        cmp(self, "||", lt, gt)?
                    }
                    _ => {
                        let a = cmp(self, "!=", x.clone(), x)?;
                        let b = cmp(self, "!=", y.clone(), y)?;
                        cmp(self, "||", a, b)?
                    }
                }
            }
        };
        let ty = result.ty.clone();
        Ok(sets.into_iter().rev().fold(result, |e, set| {
            mk(ExprKind::Comma(Box::new(set), Box::new(e)), ty.clone(), loc)
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

    fn builtin_call(&mut self, op: Builtin, params: &[Ty], ret: Ty, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(params.len(), loc)?;
        let mut converted = Vec::new();
        for (arg, ty) in args.into_iter().zip(params) {
            converted.push(self.convert_for_assign(arg, ty)?);
        }
        Ok(mk(ExprKind::Builtin(op, converted), ret, loc))
    }

    /// `(type){...}`: an unnamed object, local in a function and static
    /// at file scope.
    fn compound_literal(&mut self, ty: Ty, sugar: Sugar, loc: Loc) -> R<Expr<'a>> {
        let (init, ty) = self.initializer(&ty)?;
        if self.fs.is_none() {
            let name = self.unique_name("__compound_literal");
            let g = self.add_static(name, ty.clone(), init, false);
            self.globals[g].align = self.type_alignment(&ty, &sugar);
            self.globals[g].sugar = sugar;
            return Ok(mk(ExprKind::Global(g), ty, loc));
        }
        // The name cannot clash: it is not an identifier.
        let name = self.unique_name("compound literal");
        let id = self.new_local(name, ty.clone(), sugar, true);
        Ok(mk(ExprKind::CompoundLit(id, Box::new(init)), ty, loc))
    }

    // ---- conversions ----

    /// Array-to-pointer and function-to-pointer decay.
    /// The address of an `_Atomic` lvalue, for an atomic access.
    fn address_of_atomic(&mut self, e: Expr<'a>) -> Expr<'a> {
        self.mark_address_taken(&e);
        let ty = self.ptr_type(e.ty.clone());
        let loc = e.loc;
        mk(ExprKind::Addr(Box::new(e)), ty, loc)
    }

    /// The address of the `va_list` object `ap` names, which `va_start`
    /// and `va_arg` work on. Apple's `va_list` is a pointer variable, whose
    /// address it is; SysV's is an array, which decays to its own address,
    /// and a `va_list` parameter already holds that.
    fn va_list_addr(&mut self, ap: Expr<'a>) -> Expr<'a> {
        match self.target {
            Target::Aarch64AppleDarwin => {
                self.mark_address_taken(&ap);
                let ty = self.ptr_type(ap.ty.clone());
                let loc = ap.loc;
                mk(ExprKind::Addr(Box::new(ap)), ty, loc)
            }
            Target::X86_64LinuxGnu => self.rvalue(ap),
        }
    }

    pub(crate) fn rvalue(&mut self, e: Expr<'a>) -> Expr<'a> {
        match &e.ty {
            Ty::Atomic(ty) => {
                let ty = ty.as_ref().clone();
                let loc = e.loc;
                let sugar = e.sugar.clone();
                let args = vec![self.address_of_atomic(e), mk(ExprKind::Int(5), INT, loc)];
                mk(ExprKind::Builtin(Builtin::AtomicLoad, args), ty, loc).with_sugar(sugar)
            }
            Ty::Array(elem, _) => {
                let ty = Ty::Ptr(elem.clone());
                let loc = e.loc;
                self.mark_address_taken(&e);
                let sugar = Sugar::Ptr(self.sugar_ref(e.sugar.pointee(&self.typedefs)));
                mk(ExprKind::Addr(Box::new(e)), ty, loc).with_sugar(sugar)
            }
            Ty::Func(_) => {
                let ty = self.ptr_type(e.ty.clone());
                let loc = e.loc;
                let sugar = Sugar::Ptr(self.sugar_ref(e.sugar.clone()));
                mk(ExprKind::Addr(Box::new(e)), ty, loc).with_sugar(sugar)
            }
            _ => e,
        }
    }

    /// Converts `e` to `ty`; integer constants are converted right away.
    pub(crate) fn cast(&mut self, e: Expr<'a>, ty: Ty) -> Expr<'a> {
        if e.ty == ty {
            return e;
        }
        if let ExprKind::Int(v) = e.kind
            && let Ty::Int(_) = ty
            && e.ty.is_integer()
            && fits_i64(v, &e.ty, &ty)
        {
            return mk(ExprKind::Int(normalize(v, &ty)), ty, e.loc);
        }
        let loc = e.loc;
        let sugar = if e.ty.is_ptr() && ty.is_ptr() {
            e.sugar.clone()
        } else {
            Sugar::Plain
        };
        mk(ExprKind::Cast(Box::new(e)), ty, loc).with_sugar(sugar)
    }

    /// The integer promotions.
    pub(crate) fn promote(&mut self, e: Expr<'a>) -> Expr<'a> {
        let e = self.rvalue(e);
        let ty = promoted(&e.ty);
        self.cast(e, ty)
    }

    /// Conversion as if by assignment, to parameters and returns too.
    pub(crate) fn convert_for_assign(&mut self, e: Expr<'a>, ty: &Ty) -> R<Expr<'a>> {
        let e = self.rvalue(e);
        if ty.is_struct() {
            if e.ty != *ty {
                return Err(Error::new(e.loc, "incompatible struct types"));
            }
            return Ok(e);
        }
        if !ty.is_scalar() || !e.ty.is_scalar() {
            return Err(Error::new(e.loc, "incompatible types in assignment"));
        }
        if (ty.is_ptr() && e.ty.is_float()) || (ty.is_float() && e.ty.is_ptr()) {
            return Err(Error::new(e.loc, "incompatible types in assignment"));
        }
        Ok(self.cast(e, ty.clone()))
    }
}

/// Fixed-type GCC overflow families: int, long and long long, signed or
/// unsigned. Generic mixed-type overflow builtins have different semantics.
pub(crate) fn typed_overflow(name: &str) -> Option<(OverflowOp, Ty)> {
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
pub(crate) fn promoted(ty: &Ty) -> Ty {
    match ty {
        Ty::Int(k) if k.rank() < IntKind::Int.rank() => INT,
        Ty::Half => Ty::Float,
        _ => ty.clone(),
    }
}

/// The type the usual arithmetic conversions give two arithmetic types.
pub(crate) fn usual(a: &Ty, b: &Ty) -> Ty {
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
    } else if s.size() > u.size() {
        Ty::Int(s)
    } else {
        Ty::Int(s.to_unsigned())
    }
}

/// The type of an integer constant: the first of the allowed types its
/// value fits.
fn int_literal_type(value: u64, decimal: bool, unsigned: bool, longs: u8) -> Ty {
    use IntKind::*;
    let candidates: &[IntKind] = match (unsigned, longs, decimal) {
        (false, 0, true) => &[Int, Long],
        (false, 0, false) => &[Int, UInt, Long, ULong],
        (true, 0, _) => &[UInt, ULong],
        (false, 1, true) => &[Long],
        (false, 1, false) => &[Long, ULong],
        (true, 1, _) => &[ULong],
        (false, _, true) => &[LLong],
        (false, _, false) => &[LLong, ULLong],
        (true, _, _) => &[ULLong],
    };
    for &k in candidates {
        let max = if k.signed() {
            (1u64 << (k.size() * 8 - 1)) - 1
        } else if k.size() == 8 {
            u64::MAX
        } else {
            (1u64 << (k.size() * 8)) - 1
        };
        if value <= max {
            return Ty::Int(k);
        }
    }
    Ty::Int(ULLong)
}
