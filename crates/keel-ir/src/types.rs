//! Types of SSA values. Memory is untyped bytes; these describe values only.

use std::fmt;

/// Signedness lives on operations (`sdiv`/`udiv`), never on types.
/// Comparisons produce `I8` holding 0 or 1; there is no boolean type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    /// Opaque address. Pointer width is a target property.
    Ptr,
}

impl Type {
    pub fn is_int(self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128
        )
    }

    pub fn is_float(self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }

    /// Size in bytes, or `None` for `Ptr` whose size depends on the target.
    pub fn bytes(self) -> Option<u32> {
        match self {
            Type::I8 => Some(1),
            Type::I16 => Some(2),
            Type::I32 | Type::F32 => Some(4),
            Type::I64 | Type::F64 => Some(8),
            Type::I128 => Some(16),
            Type::Ptr => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::I8 => "i8",
            Type::I16 => "i16",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::I128 => "i128",
            Type::F32 => "f32",
            Type::F64 => "f64",
            Type::Ptr => "ptr",
        })
    }
}
