//! Memory layouts of C aggregates, for passing structs by value: written
//! `{i32, [4 x f32], {i8, ptr}}`.
//!
//! The IR never has an aggregate *value*. A struct lives in memory and is
//! handled through a `ptr`; a layout only appears on a signature
//! (`ptr byval {..}`, `ptr sret {..}`), where the calling convention needs
//! its shape to decide which registers carry it.

use crate::Type;
use crate::data::PTR_BYTES;
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
    pub fn size(&self) -> u64 {
        match self {
            Layout::Scalar(ty) => scalar_bytes(*ty),
            Layout::Struct(fields) => {
                let mut offset: u64 = 0;
                for field in fields {
                    offset = offset.next_multiple_of(field.align()) + field.size();
                }
                offset.next_multiple_of(self.align())
            }
            Layout::Array(elem, n) => elem.size() * u64::from(*n),
        }
    }

    pub fn align(&self) -> u64 {
        match self {
            Layout::Scalar(ty) => scalar_bytes(*ty),
            Layout::Struct(fields) => fields.iter().map(Layout::align).max().unwrap_or(1),
            Layout::Array(elem, _) => elem.align(),
        }
    }

    /// Every scalar inside, with its byte offset, in memory order.
    pub fn leaves(&self) -> Vec<(u64, Type)> {
        let mut out = Vec::new();
        self.collect_leaves(0, &mut out);
        out
    }

    fn collect_leaves(&self, base: u64, out: &mut Vec<(u64, Type)>) {
        match self {
            Layout::Scalar(ty) => out.push((base, *ty)),
            Layout::Struct(fields) => {
                let mut offset: u64 = 0;
                for field in fields {
                    offset = offset.next_multiple_of(field.align());
                    field.collect_leaves(base + offset, out);
                    offset += field.size();
                }
            }
            Layout::Array(elem, n) => {
                for i in 0..u64::from(*n) {
                    elem.collect_leaves(base + i * elem.size(), out);
                }
            }
        }
    }
}

fn scalar_bytes(ty: Type) -> u64 {
    ty.bytes().map_or(PTR_BYTES, u64::from)
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
