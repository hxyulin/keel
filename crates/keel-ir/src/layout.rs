//! Memory layouts of C aggregates, for passing structs by value: written
//! `{i32, [4 x f32], {i8, ptr}}`.
//!
//! The IR never has an aggregate *value*. A struct lives in memory and is
//! handled through a `ptr`. Signatures, explicit call operand contracts, and
//! aggregate va_arg carry layouts so the ABI can classify and copy the object.

use crate::{DataLayout, Type};
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Layout {
    Scalar(Type),
    /// Fields in order, each at its natural alignment, like a C struct.
    Struct(Vec<Layout>),
    /// `[n x elem]`
    Array(Box<Layout>, u32),
}

impl Layout {
    /// Size in bytes, including trailing padding to the alignment.
    /// ABI entry points validate checked_size before using this convenience API.
    pub fn size(&self) -> u64 {
        self.checked_size()
            .expect("aggregate layout size overflows u64")
    }

    /// Natural size without overflowing, including nested arrays and padding.
    pub fn checked_size(&self) -> Option<u64> {
        self.checked_size_with_layout(DataLayout::NATIVE64)
    }

    /// Natural size under an explicit storage model, including nested arrays
    /// and padding. Returns `None` on addition, rounding, or product overflow.
    pub fn checked_size_with_layout(&self, layout: DataLayout) -> Option<u64> {
        match self {
            Layout::Scalar(ty) => Some(scalar_bytes(*ty, layout)),
            Layout::Struct(fields) => {
                let mut offset = 0;
                for field in fields {
                    offset = align_up(offset, field.align_with_layout(layout))?
                        .checked_add(field.checked_size_with_layout(layout)?)?;
                }
                align_up(offset, self.align_with_layout(layout))
            }
            Layout::Array(elem, n) => elem
                .checked_size_with_layout(layout)?
                .checked_mul(u64::from(*n)),
        }
    }

    pub fn align(&self) -> u64 {
        self.align_with_layout(DataLayout::NATIVE64)
    }

    /// Natural alignment under an explicit storage model, even for empty arrays.
    pub fn align_with_layout(&self, layout: DataLayout) -> u64 {
        match self {
            Layout::Scalar(Type::Ptr) => layout.pointer_align(),
            Layout::Scalar(ty) => scalar_bytes(*ty, layout),
            Layout::Struct(fields) => fields
                .iter()
                .map(|field| field.align_with_layout(layout))
                .max()
                .unwrap_or(1),
            Layout::Array(elem, _) => elem.align_with_layout(layout),
        }
    }

    /// Every scalar inside, with its byte offset, in memory order.
    /// Requires a validated native size and a practical leaf count, as with
    /// `leaves_with_layout`.
    pub fn leaves(&self) -> Vec<(u64, Type)> {
        self.leaves_with_layout(DataLayout::NATIVE64)
    }

    /// Visits the type of every scalar inside, but an array's element only
    /// once, so the walk is linear in the layout's size, not its leaf count.
    pub fn for_each_leaf_type(&self, visit: &mut impl FnMut(Type)) {
        match self {
            Layout::Scalar(ty) => visit(*ty),
            Layout::Struct(fields) => fields.iter().for_each(|f| f.for_each_leaf_type(visit)),
            Layout::Array(_, 0) => {}
            Layout::Array(elem, _) => elem.for_each_leaf_type(visit),
        }
    }

    /// Every scalar and its byte offset under an explicit storage model.
    /// Callers must first validate `checked_size_with_layout` with the same
    /// model and ensure the leaf count is practical to materialize in a Vec.
    pub fn leaves_with_layout(&self, layout: DataLayout) -> Vec<(u64, Type)> {
        let mut out = Vec::new();
        self.collect_leaves(0, layout, &mut out);
        out
    }

    fn collect_leaves(&self, base: u64, layout: DataLayout, out: &mut Vec<(u64, Type)>) {
        match self {
            Layout::Scalar(ty) => out.push((base, *ty)),
            Layout::Struct(fields) => {
                let mut offset: u64 = 0;
                for field in fields {
                    offset = align_up(offset, field.align_with_layout(layout))
                        .expect("aggregate layout offset overflows u64");
                    field.collect_leaves(
                        base.checked_add(offset)
                            .expect("aggregate layout offset overflows u64"),
                        layout,
                        out,
                    );
                    offset = offset
                        .checked_add(
                            field
                                .checked_size_with_layout(layout)
                                .expect("aggregate layout size overflows u64"),
                        )
                        .expect("aggregate layout offset overflows u64");
                }
            }
            Layout::Array(elem, n) => {
                // Empty elements have no leaves regardless of their count.
                // Keep the AST intact: alignment and member presence still
                // participate in structural producer classification.
                let stride = elem
                    .checked_size_with_layout(layout)
                    .expect("aggregate layout size overflows u64");
                if stride == 0 {
                    return;
                }
                for i in 0..u64::from(*n) {
                    let offset = i
                        .checked_mul(stride)
                        .and_then(|offset| base.checked_add(offset))
                        .expect("aggregate layout offset overflows u64");
                    elem.collect_leaves(offset, layout, out);
                }
            }
        }
    }
}

fn align_up(size: u64, align: u64) -> Option<u64> {
    size.checked_add(align - 1).map(|n| n / align * align)
}

fn scalar_bytes(ty: Type, layout: DataLayout) -> u64 {
    if ty == Type::F80 {
        16
    } else {
        ty.bytes().map_or(layout.pointer_bytes(), u64::from)
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Layout::Scalar(ty) => write!(f, "{ty}"),
            Layout::Struct(fields) => {
                f.write_str("{")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}")?;
                }
                f.write_str("}")
            }
            Layout::Array(elem, n) => write!(f, "[{n} x {elem}]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(fields: Vec<Layout>) -> Layout {
        Layout::Struct(fields)
    }
    use Layout::Scalar as t;

    #[test]
    fn c_struct_rules() {
        // struct { char a; int b; char c; } is 12 bytes, align 4.
        let l = s(vec![t(Type::I8), t(Type::I32), t(Type::I8)]);
        assert_eq!((l.size(), l.align()), (12, 4));
        assert_eq!(l.leaves(), [(0, Type::I8), (4, Type::I32), (8, Type::I8)]);
        // Nesting is not flattening: { char; { char; int } } puts the inner
        // struct at 4.
        let l = s(vec![t(Type::I8), s(vec![t(Type::I8), t(Type::I32)])]);
        assert_eq!(l.size(), 12);
        assert_eq!(l.leaves()[1], (4, Type::I8));
        let l = s(vec![t(Type::I16), Layout::Array(Box::new(t(Type::F32)), 3)]);
        assert_eq!((l.size(), l.align()), (16, 4));
        assert_eq!(l.leaves().last(), Some(&(12, Type::F32)));
        let l = s(vec![t(Type::Ptr), t(Type::I8)]);
        assert_eq!(l.size(), 16);
    }
}
