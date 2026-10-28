//! Fixed-width SIMD value shapes and lane-wise arithmetic.
use crate::Type;
use std::{fmt, str::FromStr};

/// Fixed 64-, 128- or 256-bit shapes. Lane zero occupies the lowest-addressed bytes in memory.
/// Signedness belongs to operations, as it does for scalar integer types.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum VectorType {
    I8x8,
    I8x16,
    I8x32,
    I16x4,
    I16x8,
    I16x16,
    I32x2,
    I32x4,
    I32x8,
    I64x1,
    I64x2,
    I64x4,
    F32x2,
    F32x4,
    F32x8,
    F64x1,
    F64x2,
    F64x4,
}
impl VectorType {
    pub const ALL_64: [Self; 6] = [
        Self::I8x8,
        Self::I16x4,
        Self::I32x2,
        Self::I64x1,
        Self::F32x2,
        Self::F64x1,
    ];
    pub const ALL_128: [Self; 6] = [
        Self::I8x16,
        Self::I16x8,
        Self::I32x4,
        Self::I64x2,
        Self::F32x4,
        Self::F64x2,
    ];
    pub const ALL_256: [Self; 6] = [
        Self::I8x32,
        Self::I16x16,
        Self::I32x8,
        Self::I64x4,
        Self::F32x8,
        Self::F64x4,
    ];
    pub const ALL: [Self; 18] = [
        Self::I8x8,
        Self::I16x4,
        Self::I32x2,
        Self::I64x1,
        Self::F32x2,
        Self::F64x1,
        Self::I8x16,
        Self::I16x8,
        Self::I32x4,
        Self::I64x2,
        Self::F32x4,
        Self::F64x2,
        Self::I8x32,
        Self::I16x16,
        Self::I32x8,
        Self::I64x4,
        Self::F32x8,
        Self::F64x4,
    ];
    pub const fn bytes(self) -> u32 {
        match self {
            Self::I8x8 | Self::I16x4 | Self::I32x2 | Self::I64x1 | Self::F32x2 | Self::F64x1 => 8,
            Self::I8x32 | Self::I16x16 | Self::I32x8 | Self::I64x4 | Self::F32x8 | Self::F64x4 => {
                32
            }
            _ => 16,
        }
    }
    pub fn lane_type(self) -> Type {
        match self {
            Self::I8x8 | Self::I8x16 | Self::I8x32 => Type::I8,
            Self::I16x4 | Self::I16x8 | Self::I16x16 => Type::I16,
            Self::I32x2 | Self::I32x4 | Self::I32x8 => Type::I32,
            Self::I64x1 | Self::I64x2 | Self::I64x4 => Type::I64,
            Self::F32x2 | Self::F32x4 | Self::F32x8 => Type::F32,
            Self::F64x1 | Self::F64x2 | Self::F64x4 => Type::F64,
        }
    }
    pub fn lanes(self) -> u8 {
        (self.bytes() / self.lane_type().bytes().unwrap()) as u8
    }
    /// Integer lanes of the same width, used for comparison masks.
    pub fn mask(self) -> Self {
        match self {
            Self::F32x2 => Self::I32x2,
            Self::F32x4 => Self::I32x4,
            Self::F32x8 => Self::I32x8,
            Self::F64x1 => Self::I64x1,
            Self::F64x2 => Self::I64x2,
            Self::F64x4 => Self::I64x4,
            other => other,
        }
    }
}
impl fmt::Display for VectorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I8x8 => "i8x8",
            Self::I8x16 => "i8x16",
            Self::I8x32 => "i8x32",
            Self::I16x4 => "i16x4",
            Self::I16x8 => "i16x8",
            Self::I16x16 => "i16x16",
            Self::I32x2 => "i32x2",
            Self::I32x4 => "i32x4",
            Self::I32x8 => "i32x8",
            Self::I64x1 => "i64x1",
            Self::I64x2 => "i64x2",
            Self::I64x4 => "i64x4",
            Self::F32x2 => "f32x2",
            Self::F32x4 => "f32x4",
            Self::F32x8 => "f32x8",
            Self::F64x1 => "f64x1",
            Self::F64x2 => "f64x2",
            Self::F64x4 => "f64x4",
        })
    }
}
impl FromStr for VectorType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "i8x8" => Self::I8x8,
            "i8x16" => Self::I8x16,
            "i8x32" => Self::I8x32,
            "i16x4" => Self::I16x4,
            "i16x8" => Self::I16x8,
            "i16x16" => Self::I16x16,
            "i32x2" => Self::I32x2,
            "i32x4" => Self::I32x4,
            "i32x8" => Self::I32x8,
            "i64x1" => Self::I64x1,
            "i64x2" => Self::I64x2,
            "i64x4" => Self::I64x4,
            "f32x2" => Self::F32x2,
            "f32x4" => Self::F32x4,
            "f32x8" => Self::F32x8,
            "f64x1" => Self::F64x1,
            "f64x2" => Self::F64x2,
            "f64x4" => Self::F64x4,
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
    Padd => "vpadd", Psmin => "vpsmin", Psmax => "vpsmax", Pumin => "vpumin", Pumax => "vpumax",
    Fpadd => "vfpadd", Fpmin => "vfpmin", Fpmax => "vfpmax",
    Smin => "vsmin", Smax => "vsmax", Umin => "vumin", Umax => "vumax",
    Sabd => "vsabd", Uabd => "vuabd",
    Shadd => "vshadd", Uhadd => "vuhadd", Srhadd => "vsrhadd", Urhadd => "vurhadd",
    Shsub => "vshsub", Uhsub => "vuhsub",
    SaddSat => "vsadd_sat", UaddSat => "vuadd_sat", SsubSat => "vssub_sat", UsubSat => "vusub_sat",
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
                | Self::Fpadd
                | Self::Fpmin
                | Self::Fpmax
        )
    }
}

/// Integer-family semantics beyond ordinary wrapping arithmetic:
/// - min/max compare lanes with the named signedness;
/// - ABD computes the mathematical absolute difference and returns its low W bits;
/// - HADD/HSUB compute floor((a +/- b)/2) in W+1 bits, then keep W bits;
/// - RHADD computes floor((a+b+1)/2) in W+1 bits, then keeps W bits;
/// - *_sat clamps the exact mathematical result to the named W-bit range.
///
/// These portable operations do not read or update architectural FPSR.QC.
impl VectorBinaryOp {
    pub fn is_saturating(self) -> bool {
        matches!(
            self,
            Self::SaddSat | Self::UaddSat | Self::SsubSat | Self::UsubSat
        )
    }
}

/// Horizontal integer reductions. Add wraps at the input lane width; bitwise
/// operations combine all lanes. Min/max use the named lane signedness.
/// The scalar result has the input lane type; no reassociation of FP is implied.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum VectorReduceOp {
    Add,
    And,
    Or,
    Xor,
    Smin,
    Smax,
    Umin,
    Umax,
}
impl VectorReduceOp {
    pub const ALL: [Self; 8] = [
        Self::Add,
        Self::And,
        Self::Or,
        Self::Xor,
        Self::Smin,
        Self::Smax,
        Self::Umin,
        Self::Umax,
    ];
    pub fn binary(self) -> VectorBinaryOp {
        match self {
            Self::Add => VectorBinaryOp::Iadd,
            Self::And => VectorBinaryOp::And,
            Self::Or => VectorBinaryOp::Or,
            Self::Xor => VectorBinaryOp::Xor,
            Self::Smin => VectorBinaryOp::Smin,
            Self::Smax => VectorBinaryOp::Smax,
            Self::Umin => VectorBinaryOp::Umin,
            Self::Umax => VectorBinaryOp::Umax,
        }
    }
}
impl fmt::Display for VectorReduceOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Add => "vreduce_add",
            Self::And => "vreduce_and",
            Self::Or => "vreduce_or",
            Self::Xor => "vreduce_xor",
            Self::Smin => "vreduce_smin",
            Self::Smax => "vreduce_smax",
            Self::Umin => "vreduce_umin",
            Self::Umax => "vreduce_umax",
        })
    }
}
impl FromStr for VectorReduceOp {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "vreduce_add" => Self::Add,
            "vreduce_and" => Self::And,
            "vreduce_or" => Self::Or,
            "vreduce_xor" => Self::Xor,
            "vreduce_smin" => Self::Smin,
            "vreduce_smax" => Self::Smax,
            "vreduce_umin" => Self::Umin,
            "vreduce_umax" => Self::Umax,
            _ => return Err(()),
        })
    }
}

impl VectorBinaryOp {
    /// Pair adjacent lanes of the concatenation a,b: result[i] =
    /// binary(concat[2*i], concat[2*i+1]). Floating operations preserve that
    /// operand order and scalar IEEE NaN/signed-zero behavior.
    pub fn pairwise_binary(self) -> Option<Self> {
        Some(match self {
            Self::Padd => Self::Iadd,
            Self::Psmin => Self::Smin,
            Self::Psmax => Self::Smax,
            Self::Pumin => Self::Umin,
            Self::Pumax => Self::Umax,
            Self::Fpadd => Self::Fadd,
            Self::Fpmin => Self::Fmin,
            Self::Fpmax => Self::Fmax,
            _ => return None,
        })
    }
}
