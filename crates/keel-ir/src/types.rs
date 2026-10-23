//! Types of SSA values. Memory is untyped bytes; these describe values only.

use crate::VectorType;
use std::fmt;

/// Signedness lives on operations (`sdiv`/`udiv`), never on types.
/// Scalar comparisons produce `I8` holding 0 or 1; vector comparisons use
/// integer lane masks. There is no boolean type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    /// x87 extended: 64-bit significand and 15-bit exponent plus sign.
    /// The value payload is 10 bytes; SysV object storage is 16 bytes.
    F80,
    /// A fixed-width SIMD value, distinct from scalar integers/floats.
    Vector(VectorType),
    /// Opaque address. Pointer width is a target property.
    Ptr,
}

impl Type {
    #[inline]
    pub fn is_int(self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128
        )
    }

    #[inline]
    pub fn is_float(self) -> bool {
        matches!(self, Type::F32 | Type::F64 | Type::F80)
    }

    #[inline]
    pub fn is_vector(self) -> bool {
        matches!(self, Self::Vector(_))
    }

    #[inline]
    pub fn vector(self) -> Option<VectorType> {
        match self {
            Self::Vector(v) => Some(v),
            _ => None,
        }
    }

    /// Size in bytes, or `None` for `Ptr` whose size depends on the target.
    #[inline]
    pub fn bytes(self) -> Option<u32> {
        match self {
            Type::I8 => Some(1),
            Type::I16 => Some(2),
            Type::I32 | Type::F32 => Some(4),
            Type::I64 | Type::F64 => Some(8),
            Type::F80 => Some(10),
            Type::I128 => Some(16),
            Type::Vector(v) => Some(v.bytes()),
            Type::Ptr => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Vector(v) = self {
            return v.fmt(f);
        }
        f.write_str(match self {
            Type::I8 => "i8",
            Type::I16 => "i16",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::I128 => "i128",
            Type::F32 => "f32",
            Type::F64 => "f64",
            Type::F80 => "f80",
            Type::Ptr => "ptr",
            Type::Vector(_) => unreachable!(),
        })
    }
}
