//! Packed NEON selection and shared scalar-lane legalization.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};
use std::borrow::Cow;

struct Packed {
    opcode: Opcode,
    mnemonic: &'static str,
    arrangement: &'static str,
    field: Option<(FieldId, u32)>,
}
fn arrangement(shape: VectorType) -> &'static str {
    use VectorType::*;
    match shape {
        I8x16 => "16b",
        I16x8 => "8h",
        I32x4 | F32x4 => "4s",
        I64x2 | F64x2 => "2d",
    }
}
fn binary(op: V, shape: VectorType) -> Option<Packed> {
    let (opcode, mnemonic) = match op {
        V::Iadd => (Opcode::ADD_asimdsame_only, "add"),
        V::Isub => (Opcode::SUB_asimdsame_only, "sub"),
        V::Imul if shape != VectorType::I64x2 => (Opcode::MUL_asimdsame_only, "mul"),
        V::And => (Opcode::AND_asimdsame_only, "and"),
        V::Or => (Opcode::ORR_asimdsame_only, "orr"),
        V::Xor => (Opcode::EOR_asimdsame_only, "eor"),
        V::Fadd => (Opcode::FADD_asimdsame_only, "fadd"),
        V::Fsub => (Opcode::FSUB_asimdsame_only, "fsub"),
        V::Fmul => (Opcode::FMUL_asimdsame_only, "fmul"),
        V::Fdiv => (Opcode::FDIV_asimdsame_only, "fdiv"),
        V::Fmin => (Opcode::FMIN_asimdsame_only, "fmin"),
        V::Fmax => (Opcode::FMAX_asimdsame_only, "fmax"),
        _ => return None,
    };
    let bitwise = matches!(op, V::And | V::Or | V::Xor);
    Some(Packed {
        opcode,
        mnemonic,
        arrangement: if bitwise { "16b" } else { arrangement(shape) },
        field: if bitwise {
            None
        } else if op.is_float() {
            Some((FieldId::sz, u32::from(shape == VectorType::F64x2)))
        } else {
            Some((
                FieldId::size,
                shape.lane_type().bytes().unwrap().trailing_zeros(),
            ))
        },
    })
}
fn unary(op: UnaryOp, shape: VectorType) -> Option<Packed> {
    let size = shape.lane_type().bytes().unwrap().trailing_zeros();
    let (opcode, mnemonic) = match op {
        UnaryOp::Ineg => (Opcode::NEG_asimdmisc_R, "neg"),
        UnaryOp::Bnot => (Opcode::NOT_asimdmisc_R, "mvn"),
        UnaryOp::Clz if shape != VectorType::I64x2 => (Opcode::CLZ_asimdmisc_R, "clz"),
        UnaryOp::Popcnt if shape == VectorType::I8x16 => (Opcode::CNT_asimdmisc_R, "cnt"),
        UnaryOp::Bswap => match size {
            1 => (Opcode::REV16_asimdmisc_R, "rev16"),
            2 => (Opcode::REV32_asimdmisc_R, "rev32"),
            3 => (Opcode::REV64_asimdmisc_R, "rev64"),
            _ => return None,
        },
        UnaryOp::Fneg => (Opcode::FNEG_asimdmisc_R, "fneg"),
        UnaryOp::Fabs => (Opcode::FABS_asimdmisc_R, "fabs"),
        UnaryOp::Fsqrt => (Opcode::FSQRT_asimdmisc_R, "fsqrt"),
        UnaryOp::Fceil => (Opcode::FRINTP_asimdmisc_R, "frintp"),
        UnaryOp::Ffloor => (Opcode::FRINTM_asimdmisc_R, "frintm"),
        UnaryOp::Ftrunc => (Opcode::FRINTZ_asimdmisc_R, "frintz"),
        UnaryOp::Fnearest => (Opcode::FRINTN_asimdmisc_R, "frintn"),
        _ => return None,
    };
    let bytes = matches!(op, UnaryOp::Bnot | UnaryOp::Bswap);
    let field = if op == UnaryOp::Bnot {
        None
    } else if op.is_float() {
        Some((FieldId::sz, u32::from(shape == VectorType::F64x2)))
    } else {
        Some((FieldId::size, if bytes { 0 } else { size }))
    };
    Some(Packed {
        opcode,
        mnemonic,
        arrangement: if bytes { "16b" } else { arrangement(shape) },
        field,
    })
}
pub(super) fn lower(func: &Function) -> Cow<'_, Function> {
    crate::vector::lower(func, |f, data| match data {
        InstData::VectorBinary { op, args } => {
            binary(*op, f.value_type(args[0]).vector().unwrap()).is_some()
        }
        InstData::VectorUnary { op, arg } => {
            unary(*op, f.value_type(*arg).vector().unwrap()).is_some()
        }
        _ => false,
    })
}
impl FunctionCompiler<'_> {
    pub(super) fn vector_binary(&mut self, op: V, args: [Value; 2], result: Option<Value>) {
        let spec = binary(op, self.ty(args[0]).vector().unwrap()).expect("legalized NEON binary");
        let d = self.def_freg(result);
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Rm, u32::from(b)),
            (FieldId::Q, 1),
        ];
        let text = || {
            format!(
                "{}\tv{d}.{}, v{a}.{}, v{b}.{}",
                spec.mnemonic, spec.arrangement, spec.arrangement, spec.arrangement
            )
        };
        if let Some(field) = spec.field {
            self.native(
                spec.opcode,
                [base[0], base[1], base[2], base[3], field],
                text,
            );
        } else {
            self.native(spec.opcode, base, text);
        }
        self.def_done(result, d);
    }
    pub(super) fn vector_unary(&mut self, op: UnaryOp, arg: Value, result: Option<Value>) {
        let spec = unary(op, self.ty(arg).vector().unwrap()).expect("legalized NEON unary");
        let d = self.def_freg(result);
        let a = self.use_freg(arg, FS0);
        let base = [
            (FieldId::Rd, u32::from(d)),
            (FieldId::Rn, u32::from(a)),
            (FieldId::Q, 1),
        ];
        let text = || {
            format!(
                "{}\tv{d}.{}, v{a}.{}",
                spec.mnemonic, spec.arrangement, spec.arrangement
            )
        };
        if let Some(field) = spec.field {
            self.native(spec.opcode, [base[0], base[1], base[2], field], text);
        } else {
            self.native(spec.opcode, base, text);
        }
        self.def_done(result, d);
    }
    pub(super) fn move_vector(&mut self, d: u8, s: u8) {
        if d == s {
            return;
        }
        self.native(
            Opcode::ORR_asimdsame_only,
            [
                (FieldId::Q, 1),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, s.into()),
                (FieldId::Rm, s.into()),
            ],
            || format!("mov\tv{d}.16b, v{s}.16b"),
        );
    }
    pub(super) fn vector_spill(&mut self, store: bool, register: u8, slot: u32) {
        let offset = self.spill_base + 8 * slot as usize;
        // Unaligned Q spills use unscaled addressing when possible; large
        // offsets need an address in x17. Scalar slots retain their old layout.
        let at = if !offset.is_multiple_of(16) && offset > 255 {
            self.address(Loc::Reg(S1), self.base, offset);
            Address::base(Reg::x(S1))
        } else {
            self.slot(slot)
        };
        self.mach(
            if store { "str" } else { "ldr" },
            [Reg::q(register).into(), at.into()],
        );
    }
}
