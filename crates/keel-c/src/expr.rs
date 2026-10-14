//! Expressions, with C's implicit conversions made explicit as they are
//! parsed.

use crate::ast::*;
use crate::eval::{eval_int, normalize};
use crate::lex::Tok;
use crate::parse::{Parser, R};
use crate::types::*;
use crate::{Error, Loc};
use std::rc::Rc;

pub(crate) fn mk(kind: ExprKind, ty: Ty, loc: Loc) -> Expr {
    Expr { kind, ty, loc }
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

impl Parser {
    /// A full expression, commas included.
    pub(crate) fn expr(&mut self) -> R<Expr> {
        let mut e = self.assign()?;
        while self.is(",") {
            let loc = self.loc();
            self.advance();
            let r = self.assign()?;
            let r = self.rvalue(r);
            let ty = r.ty.clone();
            e = mk(ExprKind::Comma(Box::new(e), Box::new(r)), ty, loc);
        }
        Ok(e)
    }

    pub(crate) fn assign(&mut self) -> R<Expr> {
        let lhs = self.conditional()?;
        let Tok::Punct(p) = *self.peek() else {
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

    fn assignment(&mut self, op: &str, lhs: Expr, rhs: Expr, loc: Loc) -> R<Expr> {
        self.check_lvalue(&lhs)?;
        let ty = lhs.ty.clone();
        if op == "=" {
            let rhs = self.convert_for_assign(rhs, &ty)?;
            return Ok(mk(ExprKind::Assign(Box::new(lhs), Box::new(rhs)), ty, loc));
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
            ));
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

    fn check_lvalue(&self, e: &Expr) -> R<()> {
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

    pub(crate) fn conditional(&mut self) -> R<Expr> {
        let cond = self.binary(1)?;
        if !self.is("?") {
            return Ok(cond);
        }
        let loc = self.loc();
        self.advance();
        let cond = self.rvalue(cond);
        let a = self.expr()?;
        self.expect(":")?;
        let b = self.conditional()?;
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
        Ok(mk(
            ExprKind::Cond(Box::new(cond), Box::new(a), Box::new(b)),
            ty,
            loc,
        ))
    }

    fn binary(&mut self, min_prec: u8) -> R<Expr> {
        let mut lhs = self.cast_expr()?;
        loop {
            let Tok::Punct(p) = *self.peek() else {
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

    pub(crate) fn binop(&mut self, p: &str, l: Expr, r: Expr, loc: Loc) -> R<Expr> {
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

    fn ptr_add(&mut self, ptr: Expr, n: Expr, negate: bool, loc: Loc) -> R<Expr> {
        let size = self.elem_size(&ptr.ty, loc)?;
        let mut n = self.cast(n, LONG);
        if negate {
            n = mk(ExprKind::Unary(UnOp::Neg, Box::new(n)), LONG, loc);
        }
        let ty = ptr.ty.clone();
        Ok(mk(
            ExprKind::PtrAdd(Box::new(ptr), Box::new(n), size),
            ty,
            loc,
        ))
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

    fn cast_expr(&mut self) -> R<Expr> {
        if self.is("(") && self.type_follows(1) {
            let loc = self.loc();
            self.advance();
            let ty = self.type_name()?;
            self.expect(")")?;
            if self.is("{") {
                let lit = self.compound_literal(ty, loc)?;
                return self.postfix_ops(lit);
            }
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
            let mut e = self.cast(e, ty);
            e.loc = loc;
            return Ok(e);
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

    fn unary(&mut self) -> R<Expr> {
        let loc = self.loc();
        let Some(word) = (match self.peek() {
            Tok::Punct(p) => Some(p.to_string()),
            Tok::Ident(s) => Some(s.clone()),
            _ => None,
        }) else {
            return self.postfix();
        };
        match word.as_str() {
            "__extension__" => {
                self.advance();
                self.cast_expr()
            }
            "sizeof" | "_Alignof" | "__alignof__" | "__alignof" => {
                self.advance();
                let ty = if self.is("(") && self.type_follows(1) {
                    self.advance();
                    let ty = self.type_name()?;
                    self.expect(")")?;
                    if self.is("{") {
                        self.compound_literal(ty, loc)?.ty
                    } else {
                        ty
                    }
                } else {
                    self.unary()?.ty
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
                    self.align_of(&ty)
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
            "*" => {
                self.advance();
                let e = self.cast_expr()?;
                let e = self.rvalue(e);
                let Some(pointee) = e.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "dereferencing a non-pointer"));
                };
                Ok(mk(ExprKind::Deref(Box::new(e)), pointee, loc))
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

    pub(crate) fn address_of(&mut self, e: Expr, loc: Loc) -> R<Expr> {
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
        let ty = Ty::ptr_to(e.ty.clone());
        Ok(mk(ExprKind::Addr(Box::new(e)), ty, loc))
    }

    /// A local whose address is taken must live in memory.
    fn mark_address_taken(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Local(id) => self.mark_in_memory(*id),
            ExprKind::Member { base, .. } => self.mark_address_taken(base),
            _ => {}
        }
    }

    fn incdec(&mut self, e: Expr, inc: bool, prefix: bool, loc: Loc) -> R<Expr> {
        self.check_lvalue(&e)?;
        if !e.ty.is_scalar() {
            return Err(Error::new(loc, "`++`/`--` needs a scalar"));
        }
        if e.ty.is_ptr() {
            self.elem_size(&e.ty, loc)?;
        }
        let ty = e.ty.clone();
        Ok(mk(
            ExprKind::IncDec {
                lhs: Box::new(e),
                inc,
                prefix,
            },
            ty,
            loc,
        ))
    }

    fn postfix(&mut self) -> R<Expr> {
        let e = self.primary()?;
        self.postfix_ops(e)
    }

    fn postfix_ops(&mut self, mut e: Expr) -> R<Expr> {
        loop {
            let loc = self.loc();
            if self.eat("[") {
                let idx = self.expr()?;
                self.expect("]")?;
                let sum = self.binop("+", e, idx, loc)?;
                let Some(elem) = sum.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "subscript of a non-pointer"));
                };
                e = mk(ExprKind::Deref(Box::new(sum)), elem, loc);
            } else if self.eat("(") {
                e = self.call(e, loc)?;
            } else if self.eat(".") {
                e = self.member(e, loc)?;
            } else if self.eat("->") {
                let p = self.rvalue(e);
                let Some(pointee) = p.ty.pointee().cloned() else {
                    return Err(Error::new(loc, "`->` on a non-pointer"));
                };
                e = mk(ExprKind::Deref(Box::new(p)), pointee, loc);
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

    fn member(&mut self, base: Expr, loc: Loc) -> R<Expr> {
        let name = self.ident()?;
        let Ty::Struct(id) = base.ty else {
            return Err(Error::new(loc, format!("`.{name}` on a non-struct")));
        };
        let def = &self.structs[id];
        if !def.complete {
            return Err(Error::new(loc, "member of an incomplete struct"));
        }
        let Some((offset, m)) = def.find(&name, &self.structs) else {
            return Err(Error::new(loc, format!("no member `{name}`")));
        };
        Ok(mk(
            ExprKind::Member {
                base: Box::new(base),
                offset,
                bits: m.bits,
            },
            m.ty,
            loc,
        ))
    }

    fn call(&mut self, callee: Expr, loc: Loc) -> R<Expr> {
        let mut args = Vec::new();
        while !self.eat(")") {
            if !args.is_empty() {
                self.expect(",")?;
            }
            args.push(self.assign()?);
        }
        self.make_call(callee, args, loc)
    }

    fn make_call(&mut self, callee: Expr, raw_args: Vec<Expr>, loc: Loc) -> R<Expr> {
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
        Ok(mk(ExprKind::Call(Box::new(f), args), fty.ret.clone(), loc))
    }

    /// Promotions for arguments without a parameter type.
    fn default_promote(&mut self, e: Expr) -> Expr {
        let e = self.rvalue(e);
        if e.ty == Ty::Float {
            return self.cast(e, Ty::Double);
        }
        self.promote(e)
    }

    fn primary(&mut self) -> R<Expr> {
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
            Tok::Str { mut bytes, wide } => {
                let mut wide = wide;
                while let Tok::Str {
                    bytes: more,
                    wide: w,
                } = self.peek().clone()
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
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Ident(name) => {
                if name == "__func__" || name == "__FUNCTION__" || name == "__PRETTY_FUNCTION__" {
                    let func = self.fs.as_ref().map_or("", |f| f.name.as_str());
                    let bytes = func.as_bytes().to_vec();
                    return Ok(self.string_expr(bytes, false, loc));
                }
                if name == "_Generic" {
                    return Err(Error::new(loc, "`_Generic` is not supported"));
                }
                if name.starts_with("__builtin_") {
                    return self.builtin(&name, loc);
                }
                if name == "__atomic_load_n" || name == "__atomic_store_n" {
                    return self.atomic(&name, loc);
                }
                self.lookup_value(&name, loc)
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn string_expr(&mut self, mut bytes: Vec<u8>, wide: bool, loc: Loc) -> Expr {
        let unit = if wide { 4 } else { 1 };
        bytes.extend(std::iter::repeat_n(0, unit));
        let len = (bytes.len() / unit) as u64;
        let i = self.add_string(bytes, wide);
        let elem = if wide { INT } else { CHAR };
        mk(ExprKind::Str(i), Ty::Array(Rc::new(elem), Some(len)), loc)
    }

    fn builtin(&mut self, name: &str, loc: Loc) -> R<Expr> {
        let base = &name["__builtin_".len()..];
        match base {
            "va_start" | "va_arg" | "va_end" | "va_copy" => {
                self.expect("(")?;
                let ap = self.assign()?;
                let e = match base {
                    "va_start" => {
                        self.expect(",")?;
                        self.assign()?;
                        self.mark_address_taken(&ap);
                        mk(ExprKind::VaStart(Box::new(ap)), Ty::Void, loc)
                    }
                    "va_arg" => {
                        self.expect(",")?;
                        let ty = self.type_name()?;
                        if !ty.is_scalar() {
                            return Err(Error::new(loc, "`va_arg` of a non-scalar"));
                        }
                        self.mark_address_taken(&ap);
                        mk(ExprKind::VaArg(Box::new(ap)), ty, loc)
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
                    let Some((off, m)) = self.structs[id].find(&name, &self.structs) else {
                        return Err(Error::new(loc, format!("no member `{name}`")));
                    };
                    offset += off;
                    ty = m.ty;
                }
                self.expect(")")?;
                Ok(mk(ExprKind::Int(offset as i64), ULONG, loc))
            }
            "expect" => {
                let args = self.builtin_args(2, loc)?;
                let e = args.into_iter().next().unwrap();
                Ok(self.cast(e, LONG))
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
            "alloca" => self.builtin_call(Builtin::Alloca, &[ULONG], Ty::ptr_to(Ty::Void), loc),
            "trap" | "unreachable" => self.builtin_call(Builtin::Trap, &[], Ty::Void, loc),
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
    fn atomic(&mut self, name: &str, loc: Loc) -> R<Expr> {
        let store = name == "__atomic_store_n";
        let mut args = self.builtin_args(if store { 3 } else { 2 }, loc)?;
        let Some(ty) = args[0].ty.pointee().cloned() else {
            return Err(Error::new(loc, format!("`{name}` needs a pointer")));
        };
        let order = args.last().unwrap();
        if !matches!(eval_int(order), Some(0..=5)) || !(ty.is_integer() || ty.is_ptr()) {
            return Err(Error::new(loc, format!("unsupported `{name}`")));
        }
        if store {
            let value = args.remove(1);
            let value = self.convert_for_assign(value, &ty)?;
            args.insert(1, value);
            return Ok(mk(
                ExprKind::Builtin(Builtin::AtomicStore, args),
                Ty::Void,
                loc,
            ));
        }
        Ok(mk(ExprKind::Builtin(Builtin::AtomicLoad, args), ty, loc))
    }

    fn builtin_args(&mut self, n: usize, loc: Loc) -> R<Vec<Expr>> {
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

    fn builtin_call(&mut self, op: Builtin, params: &[Ty], ret: Ty, loc: Loc) -> R<Expr> {
        let args = self.builtin_args(params.len(), loc)?;
        let mut converted = Vec::new();
        for (arg, ty) in args.into_iter().zip(params) {
            converted.push(self.convert_for_assign(arg, ty)?);
        }
        Ok(mk(ExprKind::Builtin(op, converted), ret, loc))
    }

    /// `(type){...}`: an unnamed object, local in a function and static
    /// at file scope.
    fn compound_literal(&mut self, ty: Ty, loc: Loc) -> R<Expr> {
        let (init, ty) = self.initializer(&ty)?;
        if self.fs.is_none() {
            let name = self.unique_name("__compound_literal");
            let g = self.add_static(name, ty.clone(), init, false);
            return Ok(mk(ExprKind::Global(g), ty, loc));
        }
        // The name cannot clash: it is not an identifier.
        let name = self.unique_name("compound literal");
        let id = self.new_local(name, ty.clone(), Sugar::Plain, true);
        Ok(mk(ExprKind::CompoundLit(id, Box::new(init)), ty, loc))
    }

    // ---- conversions ----

    /// Array-to-pointer and function-to-pointer decay.
    pub(crate) fn rvalue(&mut self, e: Expr) -> Expr {
        match &e.ty {
            Ty::Array(elem, _) => {
                let ty = Ty::Ptr(elem.clone());
                let loc = e.loc;
                self.mark_address_taken(&e);
                mk(ExprKind::Addr(Box::new(e)), ty, loc)
            }
            Ty::Func(_) => {
                let ty = Ty::ptr_to(e.ty.clone());
                let loc = e.loc;
                mk(ExprKind::Addr(Box::new(e)), ty, loc)
            }
            _ => e,
        }
    }

    /// Converts `e` to `ty`; integer constants are converted right away.
    pub(crate) fn cast(&mut self, e: Expr, ty: Ty) -> Expr {
        if e.ty == ty {
            return e;
        }
        if let ExprKind::Int(v) = e.kind
            && let Ty::Int(_) = ty
            && e.ty.is_integer()
        {
            return mk(ExprKind::Int(normalize(v, &ty)), ty, e.loc);
        }
        let loc = e.loc;
        mk(ExprKind::Cast(Box::new(e)), ty, loc)
    }

    /// The integer promotions.
    pub(crate) fn promote(&mut self, e: Expr) -> Expr {
        let e = self.rvalue(e);
        let ty = promoted(&e.ty);
        self.cast(e, ty)
    }

    /// Conversion as if by assignment, to parameters and returns too.
    pub(crate) fn convert_for_assign(&mut self, e: Expr, ty: &Ty) -> R<Expr> {
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

fn offset_member_follows(p: &mut Parser, first: bool) -> bool {
    if first {
        return matches!(p.peek(), Tok::Ident(_));
    }
    p.eat(".")
}

fn is_null(e: &Expr) -> bool {
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
