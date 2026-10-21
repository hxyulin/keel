//! Initializers, flattened into (offset, value) items.
//!
//! The rules, from C11 6.7.9: a braced list initializes an aggregate's
//! elements or members in order; a designator (`[3] =`, `.x =`) moves to
//! another one, and the next undesignated item continues after it. A
//! nested aggregate without its own braces takes as many items from the
//! enclosing list as it needs ("brace elision"), unless the item is an
//! expression of its own struct type, which initializes it whole. Anything
//! not mentioned is zero.

use crate::Error;
use crate::ast::*;
use crate::lex::Tok;
use crate::parse::{Parser, R};
use crate::types::*;

/// Where a sub-object is.
#[derive(Copy, Clone)]
struct Slot<'a> {
    ty: Ty<'a>,
    offset: u64,
    bits: Option<BitField>,
}

impl<'a> Parser<'a> {
    /// The initializer after `=` for an object of type `ty`. Returns the
    /// items and the type, completed if it was an array of unknown length.
    pub(crate) fn initializer(&mut self, ty: &Ty<'a>) -> R<(Init<'a>, Ty<'a>)> {
        let mut items = Vec::new();
        let slot = Slot {
            ty: *ty.unatomic(),
            offset: 0,
            bits: None,
        };
        let count = self.init_braced_or_expr(&slot, &mut items)?;
        let ty = match ty {
            Ty::Array(ArrayTy { elem, len: None }) => self.array_type(*elem, Some(count)),
            _ => *ty,
        };
        Ok((
            Init {
                items: self.slice(&items),
            },
            ty,
        ))
    }

    /// One initializer for `slot`: a braced list, a string for a character
    /// array, or an expression. Returns how many array elements it gave.
    fn init_braced_or_expr(&mut self, slot: &Slot<'a>, items: &mut Vec<InitItem<'a>>) -> R<u64> {
        if let Some(n) = self.init_string(slot, items)? {
            return Ok(n);
        }
        if self.eat("{") {
            if let Some(n) = self.init_string(slot, items)? {
                self.eat(",");
                self.expect("}")?;
                return Ok(n);
            }
            return self.init_list(slot, items);
        }
        let e = self.assign()?;
        self.init_with(slot, e, items)?;
        Ok(1)
    }

    /// A string literal initializing a character array, if that is what
    /// comes next. Returns the number of characters with the terminator.
    fn init_string(&mut self, slot: &Slot<'a>, items: &mut Vec<InitItem<'a>>) -> R<Option<u64>> {
        let Ty::Array(&ArrayTy { elem, len }) = slot.ty else {
            return Ok(None);
        };
        let elem_size = match elem {
            Ty::Int(k) if k.size() == 1 => 1,
            Ty::Int(k) if k.size() == 4 => 4,
            _ => return Ok(None),
        };
        if !matches!(self.peek(), Tok::Str { .. }) {
            return Ok(None);
        }
        let e = self.assign()?;
        let ExprKind::Str(i) = e.kind else {
            return Err(Error::new(e.loc, "expected a string literal"));
        };
        let mut bytes = self.strings[i].bytes;
        let unit = if self.strings[i].wide { 4 } else { 1 };
        if unit != elem_size {
            return Err(Error::new(e.loc, "string literal of the wrong width"));
        }
        let count = (bytes.len() / unit) as u64;
        if let Some(n) = len {
            bytes = &bytes[..bytes.len().min((n * elem_size as u64) as usize)];
        }
        items.push(InitItem::Bytes {
            offset: slot.offset,
            bytes,
        });
        Ok(Some(count))
    }

    /// An expression initializing `slot`: the whole of it if it is a
    /// scalar or the expression has its struct type, else its first scalar
    /// with the rest taken from the enclosing list.
    fn init_with(&mut self, slot: &Slot<'a>, e: Expr<'a>, items: &mut Vec<InitItem<'a>>) -> R<()> {
        if slot.ty.is_scalar() || (slot.ty.is_struct() && e.ty == slot.ty) {
            let expr = self.convert_for_assign(e, &slot.ty)?;
            items.push(InitItem::Expr {
                offset: slot.offset,
                expr: self.node(expr),
                bits: slot.bits,
            });
            return Ok(());
        }
        if !matches!(slot.ty, Ty::Array(..) | Ty::Struct(_)) {
            return Err(Error::new(e.loc, "cannot initialize this type"));
        }
        self.init_elided(slot, Some(e), items)
    }

    /// The sub-objects of an aggregate, in initialization order.
    fn sub_slots(&self, slot: &Slot<'a>) -> Vec<Slot<'a>> {
        match slot.ty {
            Ty::Array(&ArrayTy { elem, len: Some(n) }) => {
                let size = self.size_of(&elem);
                (0..n)
                    .map(|i| Slot {
                        ty: *elem.unatomic(),
                        offset: slot.offset + i * size,
                        bits: None,
                    })
                    .collect()
            }
            Ty::Struct(id) => {
                let def = &self.structs[id];
                let named = def
                    .members
                    .iter()
                    .filter(|m| m.name.is_some() || m.bits.is_none())
                    .map(|m| Slot {
                        ty: *m.ty.unatomic(),
                        offset: slot.offset + m.offset,
                        bits: m.bits,
                    });
                if def.is_union {
                    named.take(1).collect()
                } else {
                    named.collect()
                }
            }
            _ => Vec::new(),
        }
    }

    /// An aggregate without braces, filled from the enclosing list, `first`
    /// already parsed.
    fn init_elided(
        &mut self,
        slot: &Slot<'a>,
        first: Option<Expr<'a>>,
        items: &mut Vec<InitItem<'a>>,
    ) -> R<()> {
        let mut first = first;
        for (i, sub) in self.sub_slots(slot).into_iter().enumerate() {
            if i > 0 {
                // Continue only with another undesignated item of this list.
                let more = self.is(",")
                    && !matches!(
                        self.peek_at(1),
                        Tok::Punct("}") | Tok::Punct(".") | Tok::Punct("[")
                    );
                if !more {
                    break;
                }
                self.advance();
            }
            self.init_item(&sub, first.take(), items)?;
        }
        Ok(())
    }

    /// An undesignated item of a list for `slot`.
    fn init_item(
        &mut self,
        slot: &Slot<'a>,
        pre: Option<Expr<'a>>,
        items: &mut Vec<InitItem<'a>>,
    ) -> R<()> {
        if let Some(e) = pre {
            return self.init_with(slot, e, items);
        }
        if self.is("{") || slot.ty.is_scalar() {
            self.init_braced_or_expr(slot, items)?;
            return Ok(());
        }
        if self.init_string(slot, items)?.is_some() {
            return Ok(());
        }
        let e = self.assign()?;
        self.init_with(slot, e, items)
    }

    /// A braced list for `slot`, after the `{`. Returns the number of
    /// array elements initialized.
    fn init_list(&mut self, slot: &Slot<'a>, items: &mut Vec<InitItem<'a>>) -> R<u64> {
        if slot.ty.is_scalar() {
            if !self.eat("}") {
                self.init_braced_or_expr(slot, items)?;
                self.eat(",");
                self.expect("}")?;
            }
            return Ok(1);
        }
        let unbounded = matches!(slot.ty, Ty::Array(ArrayTy { len: None, .. }));
        let subs = if unbounded {
            Vec::new()
        } else {
            self.sub_slots(slot)
        };
        let mut next = 0usize;
        let mut count = 0u64;
        while !self.eat("}") {
            let loc = self.loc();
            // GNU's `[first ... last] =` initializes each of a range.
            let mut range = (1, 0);
            let sub = if self.is("[") || self.is(".") {
                let (index, sub, n) = self.designation(slot)?;
                next = index + n - 1;
                range = (n, self.size_of(&sub.ty));
                sub
            } else if unbounded {
                let Ty::Array(&ArrayTy { elem, .. }) = slot.ty else {
                    unreachable!()
                };
                Slot {
                    ty: *elem.unatomic(),
                    offset: slot.offset + next as u64 * self.size_of(&elem),
                    bits: None,
                }
            } else {
                match subs.get(next) {
                    Some(s) => *s,
                    None => return Err(Error::new(loc, "too many initializers")),
                }
            };
            let first = items.len();
            if self.is("[") || self.is(".") {
                // `.a.b = ` or `[1].x =`: designators into the sub-object.
                self.designated_rest(&sub, items)?;
            } else if self.is("=") {
                self.advance();
                self.init_item(&sub, None, items)?;
            } else {
                self.init_item(&sub, None, items)?;
            }
            let (n, stride) = range;
            // Only a range with extra elements needs a replication snapshot.
            if n > 1 {
                let one: Vec<InitItem<'a>> = items[first..].to_vec();
                for k in 1..n as u64 {
                    items.extend(one.iter().cloned().map(|mut item| {
                        match &mut item {
                            InitItem::Expr { offset, .. } | InitItem::Bytes { offset, .. } => {
                                *offset += k * stride
                            }
                        }
                        item
                    }));
                }
            }
            next += 1;
            count = count.max(next as u64);
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
            self.release_tokens();
        }
        Ok(count)
    }

    /// The first designator of an item: which sub-object of `slot`, its
    /// index in initialization order, and how many elements from it a
    /// range designator (`[2 ... 5]`) covers.
    fn designation(&mut self, slot: &Slot<'a>) -> R<(usize, Slot<'a>, usize)> {
        let loc = self.loc();
        if self.eat("[") {
            let i = self.const_int()?;
            let last = if self.eat("...") {
                self.const_int()?
            } else {
                i
            };
            self.expect("]")?;
            let Ty::Array(&ArrayTy { elem, len }) = slot.ty else {
                return Err(Error::new(loc, "`[]` designator for a non-array"));
            };
            if i < 0 || last < i || len.is_some_and(|n| last as u64 >= n) {
                return Err(Error::new(loc, "array designator out of range"));
            }
            let sub = Slot {
                ty: *elem.unatomic(),
                offset: slot.offset + i as u64 * self.size_of(&elem),
                bits: None,
            };
            return Ok((i as usize, sub, (last - i + 1) as usize));
        }
        self.expect(".")?;
        let name = self.ident()?;
        let Ty::Struct(id) = slot.ty else {
            return Err(Error::new(loc, "`.` designator for a non-struct"));
        };
        let def = &self.structs[id];
        // A member of this struct, or of an anonymous member, which then
        // counts as the item's position.
        let named: Vec<&Member<'a>> = def
            .members
            .iter()
            .filter(|m| m.name.is_some() || m.bits.is_none())
            .collect();
        for (i, m) in named.iter().enumerate() {
            if m.name.as_deref() == Some(name) {
                let sub = Slot {
                    ty: *m.ty.unatomic(),
                    offset: slot.offset + m.offset,
                    bits: m.bits,
                };
                return Ok((i, sub, 1));
            }
            if m.name.is_none()
                && let Ty::Struct(inner) = m.ty
                && let Some((off, found)) =
                    self.structs[inner].find(&name, &self.structs, &self.typedefs, self.arena)
            {
                let sub = Slot {
                    ty: *found.ty.unatomic(),
                    offset: slot.offset + m.offset + off,
                    bits: found.bits,
                };
                return Ok((i, sub, 1));
            }
        }
        Err(Error::new(loc, format!("no member `{name}`")))
    }

    /// Further designators, then `=` and the initializer.
    fn designated_rest(&mut self, slot: &Slot<'a>, items: &mut Vec<InitItem<'a>>) -> R<()> {
        if self.is("[") || self.is(".") {
            let loc = self.loc();
            let (_, sub, n) = self.designation(slot)?;
            if n > 1 {
                return Err(Error::new(loc, "a range designator must come first"));
            }
            return self.designated_rest(&sub, items);
        }
        self.expect("=")?;
        self.init_item(slot, None, items)
    }
}
