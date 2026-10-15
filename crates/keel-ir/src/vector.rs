//! Fixed-width SIMD value shapes and lane-wise arithmetic.
use crate::Type;
use std::{fmt, str::FromStr};

/// Exactly 128 bits. Lane zero occupies the lowest-addressed bytes in memory.
/// Signedness belongs to operations, as it does for scalar integer types.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum VectorType {
    I8x16,
    I16x8,
    I32x4,
    I64x2,
    F32x4,
    F64x2,
}
impl VectorType {
    pub const ALL: [Self; 6] = [
        Self::I8x16,
        Self::I16x8,
        Self::I32x4,
        Self::I64x2,
        Self::F32x4,
        Self::F64x2,
    ];
    pub fn lane_type(self) -> Type {
        match self {
            Self::I8x16 => Type::I8,
            Self::I16x8 => Type::I16,
            Self::I32x4 => Type::I32,
            Self::I64x2 => Type::I64,
            Self::F32x4 => Type::F32,
            Self::F64x2 => Type::F64,
        }
    }
    pub fn lanes(self) -> u8 {
        (16 / self.lane_type().bytes().unwrap()) as u8
    }
    /// Integer lanes of the same width, used for comparison masks.
    pub fn mask(self) -> Self {
        match self {
            Self::F32x4 => Self::I32x4,
            Self::F64x2 => Self::I64x2,
            other => other,
        }
    }
}
impl fmt::Display for VectorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I8x16 => "i8x16",
            Self::I16x8 => "i16x8",
            Self::I32x4 => "i32x4",
            Self::I64x2 => "i64x2",
            Self::F32x4 => "f32x4",
            Self::F64x2 => "f64x2",
        })
    }
}
impl FromStr for VectorType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "i8x16" => Self::I8x16,
            "i16x8" => Self::I16x8,
            "i32x4" => Self::I32x4,
            "i64x2" => Self::I64x2,
            "f32x4" => Self::F32x4,
            "f64x2" => Self::F64x2,
            _ => return Err(()),
        })
    }
}

macro_rules! vector_binary_ops {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Pure, lane-wise operations. Integer arithmetic wraps at lane width;
        /// shift counts are reduced modulo lane width independently per lane.
        /// Float operations have the corresponding scalar IEEE semantics.
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum VectorBinaryOp { $($variant),* }
        impl fmt::Display for VectorBinaryOp {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(match self { $(Self::$variant => $name),* }) }
        }
        impl FromStr for VectorBinaryOp {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, ()> { Ok(match s { $($name => Self::$variant,)* _ => return Err(()) }) }
        }
    };
}
vector_binary_ops! {
    Iadd => "viadd", Isub => "visub", Imul => "vimul",
    And => "vand", Or => "vor", Xor => "vxor",
    Shl => "vshl", Ushr => "vushr", Sshr => "vsshr",
    Fadd => "vfadd", Fsub => "vfsub", Fmul => "vfmul", Fdiv => "vfdiv",
    Fmin => "vfmin", Fmax => "vfmax", Fcopysign => "vfcopysign",
}
impl VectorBinaryOp {
    pub fn is_float(self) -> bool {
        matches!(
            self,
            Self::Fadd
                | Self::Fsub
                | Self::Fmul
                | Self::Fdiv
                | Self::Fmin
                | Self::Fmax
                | Self::Fcopysign
        )
    }
}
