//! Runtime values.

use crate::Stop;
use keel_ir::{Type, VectorType};

/// A runtime value. Integers hold their bit pattern; signedness belongs to
/// operations, as in the IR. Floats hold their raw IEEE bits, so NaN payloads
/// and signed zeros compare exactly. A pointer is a plain 64-bit address.
/// A vector holds its bytes in memory order: lane zero first, little-endian.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Val {
    I8(u8),
    I16(u16),
    I32(u32),
    I64(u64),
    I128(u128),
    /// Raw `f32` bits.
    F32(u32),
    /// Raw `f64` bits.
    F64(u64),
    Ptr(u64),
    Vector(VectorType, Vec<u8>),
}

impl Val {
    pub fn f32(x: f32) -> Self {
        Self::F32(x.to_bits())
    }

    pub fn f64(x: f64) -> Self {
        Self::F64(x.to_bits())
    }

    /// An integer of type `ty`, truncated to its width.
    ///
    /// # Panics
    ///
    /// If `ty` is not an integer type.
    pub fn int(ty: Type, value: i128) -> Self {
        assert!(ty.is_int(), "Val::int needs an integer type, not {ty}");
        Self::from_int(ty, value as u128)
    }

    /// A vector of `ty` from lane bit patterns, each truncated to the lane width.
    ///
    /// # Panics
    ///
    /// If the number of lanes does not match `ty`.
    pub fn vector(ty: VectorType, lanes: &[u64]) -> Self {
        assert_eq!(lanes.len(), usize::from(ty.lanes()), "lane count of {ty}");
        from_lanes(ty, lanes)
    }

    pub fn ty(&self) -> Type {
        match self {
            Self::I8(_) => Type::I8,
            Self::I16(_) => Type::I16,
            Self::I32(_) => Type::I32,
            Self::I64(_) => Type::I64,
            Self::I128(_) => Type::I128,
            Self::F32(_) => Type::F32,
            Self::F64(_) => Type::F64,
            Self::Ptr(_) => Type::Ptr,
            Self::Vector(ty, _) => Type::Vector(*ty),
        }
    }

    /// The value as an `f32` or `f64`, widened to `f64`.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::F32(bits) => Some(f64::from(f32::from_bits(bits))),
            Self::F64(bits) => Some(f64::from_bits(bits)),
            _ => None,
        }
    }

    /// The bit pattern of an integer or pointer, zero-extended.
    pub fn as_u128(&self) -> Option<u128> {
        match *self {
            Self::I8(x) => Some(x.into()),
            Self::I16(x) => Some(x.into()),
            Self::I32(x) => Some(x.into()),
            Self::I64(x) | Self::Ptr(x) => Some(x.into()),
            Self::I128(x) => Some(x),
            _ => None,
        }
    }

    /// The lanes of a vector, each zero-extended to 64 bits.
    pub fn lanes(&self) -> Option<Vec<u64>> {
        match self {
            Self::Vector(ty, bytes) => Some(lanes(*ty, bytes)),
            _ => None,
        }
    }

    pub(crate) fn from_int(ty: Type, bits: u128) -> Self {
        match ty {
            Type::I8 => Self::I8(bits as u8),
            Type::I16 => Self::I16(bits as u16),
            Type::I32 => Self::I32(bits as u32),
            Type::I64 => Self::I64(bits as u64),
            Type::I128 => Self::I128(bits),
            _ => unreachable!("from_int on {ty}"),
        }
    }

    /// Integer type and bits, for integer values only.
    pub(crate) fn int_parts(&self) -> Option<(Type, u128)> {
        match self {
            Self::Ptr(_) => None,
            _ => self.as_u128().map(|bits| (self.ty(), bits)),
        }
    }

    pub(crate) fn from_float(ty: Type, bits: u64) -> Self {
        match ty {
            Type::F32 => Self::F32(bits as u32),
            Type::F64 => Self::F64(bits),
            _ => unreachable!("from_float on {ty}"),
        }
    }

    pub(crate) fn float_parts(&self) -> Option<(Type, u64)> {
        match *self {
            Self::F32(bits) => Some((Type::F32, bits.into())),
            Self::F64(bits) => Some((Type::F64, bits)),
            _ => None,
        }
    }

    /// The value's bytes in memory order.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Vector(_, bytes) => bytes.clone(),
            Self::F32(bits) => bits.to_le_bytes().to_vec(),
            Self::F64(bits) => bits.to_le_bytes().to_vec(),
            other => {
                let bits = other.as_u128().unwrap();
                let size = byte_size(other.ty()).unwrap() as usize;
                bits.to_le_bytes()[..size].to_vec()
            }
        }
    }

    /// Reads a value of `ty` from exactly `byte_size(ty)` bytes.
    pub(crate) fn from_bytes(ty: Type, bytes: &[u8]) -> Self {
        let mut buf = [0u8; 16];
        if let Type::Vector(vt) = ty {
            return Self::Vector(vt, bytes.to_vec());
        }
        buf[..bytes.len()].copy_from_slice(bytes);
        let bits = u128::from_le_bytes(buf);
        match ty {
            Type::F32 | Type::F64 => Self::from_float(ty, bits as u64),
            Type::Ptr => Self::Ptr(bits as u64),
            _ => Self::from_int(ty, bits),
        }
    }
}

/// Bit width of an integer or float scalar type; pointers are 64 bits.
pub(crate) fn bit_width(ty: Type) -> u32 {
    match ty {
        Type::Ptr => 64,
        _ => ty.bytes().expect("sized type") * 8,
    }
}

/// Storage size of a value of `ty`. The interpreter has no `f80`.
pub(crate) fn byte_size(ty: Type) -> Result<u64, Stop> {
    match ty {
        Type::Ptr => Ok(8),
        Type::F80 => Err(Stop::unsupported("f80 values")),
        _ => Ok(u64::from(ty.bytes().unwrap())),
    }
}

pub(crate) fn mask(width: u32) -> u128 {
    if width >= 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}

/// `bits`, `width` bits wide, as a signed number.
pub(crate) fn signed(bits: u128, width: u32) -> i128 {
    let shift = 128 - width;
    ((bits << shift) as i128) >> shift
}

pub(crate) fn lanes(ty: VectorType, bytes: &[u8]) -> Vec<u64> {
    let size = (ty.bytes() / u32::from(ty.lanes())) as usize;
    bytes
        .chunks_exact(size)
        .map(|chunk| {
            let mut buf = [0u8; 8];
            buf[..size].copy_from_slice(chunk);
            u64::from_le_bytes(buf)
        })
        .collect()
}

pub(crate) fn from_lanes(ty: VectorType, lanes: &[u64]) -> Val {
    let size = (ty.bytes() / u32::from(ty.lanes())) as usize;
    let mut bytes = Vec::with_capacity(ty.bytes() as usize);
    for lane in lanes {
        bytes.extend_from_slice(&lane.to_le_bytes()[..size]);
    }
    Val::Vector(ty, bytes)
}
