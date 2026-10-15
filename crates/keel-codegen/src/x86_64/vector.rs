//! Packed x86 instruction selection and shared lane legalization.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};
use std::borrow::Cow;

pub(super) fn packed(op: V, shape: VectorType) -> Option<Opcode> {
    use VectorType::*;
    Some(match op {
        V::Iadd => match shape {
            I8x16 => Opcode::Paddb,
            I16x8 => Opcode::Paddw,
            I32x4 => Opcode::Paddd,
            I64x2 => Opcode::Paddq,
            _ => return None,
        },
        V::Isub => match shape {
            I8x16 => Opcode::Psubb,
            I16x8 => Opcode::Psubw,
            I32x4 => Opcode::Psubd,
            I64x2 => Opcode::Psubq,
            _ => return None,
        },
        V::Imul if shape == I16x8 => Opcode::Pmullw,
        V::And => Opcode::Andps,
        V::Or => Opcode::Orps,
        V::Xor => Opcode::Xorps,
        V::Fadd => {
            if shape == F32x4 {
                Opcode::Addps
            } else {
                Opcode::Addpd
            }
        }
        V::Fsub => {
            if shape == F32x4 {
                Opcode::Subps
            } else {
                Opcode::Subpd
            }
        }
        V::Fmul => {
            if shape == F32x4 {
                Opcode::Mulps
            } else {
                Opcode::Mulpd
            }
        }
        V::Fdiv => {
            if shape == F32x4 {
                Opcode::Divps
            } else {
                Opcode::Divpd
            }
        }
        _ => return None,
    })
}
pub(super) fn lower(func: &Function) -> Cow<'_, Function> {
    crate::vector::lower(func, |func, data| match data {
        InstData::VectorExtract { arg, .. } => {
            func.value_type(*arg) == Type::Vector(VectorType::I64x2)
        }
        InstData::VectorBinary { op, args } => {
            packed(*op, func.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary {
            op: UnaryOp::Fsqrt, ..
        } => true,
        _ => false,
    })
}
