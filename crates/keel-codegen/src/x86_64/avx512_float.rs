//! Unmasked packed floating arithmetic, with source order matching scalar IR.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};

fn floating(ty: Type) -> bool {
    matches!(ty, Type::Vector(VectorType::F32x16 | VectorType::F64x8))
}

pub(super) fn keep(func: &Function, data: &InstData, features: keel_asm_ir::x86::Features) -> bool {
    if !features.avx512f {
        return false;
    }
    let ty = |v| func.value_type(func.resolve(v));
    match data {
        InstData::VectorBinary { op, args } => {
            floating(ty(args[0])) && matches!(op, V::Fadd | V::Fsub | V::Fmul | V::Fdiv)
        }
        InstData::VectorUnary {
            op: UnaryOp::Fsqrt,
            arg,
        } => floating(ty(*arg)),
        _ => false,
    }
}
impl FunctionCompiler<'_> {
    pub(super) fn avx512_float(&mut self, data: &InstData, result: Option<Value>) -> bool {
        if !keep(self.func, data, self.features) {
            return false;
        }
        let (stem, a, b) = match *data {
            InstData::VectorBinary { op, args } => (
                match op {
                    V::Fadd => "vadd",
                    V::Fsub => "vsub",
                    V::Fmul => "vmul",
                    V::Fdiv => "vdiv",
                    _ => unreachable!("retained floating binary operation"),
                },
                args[0],
                Some(args[1]),
            ),
            InstData::VectorUnary { arg, .. } => ("vsqrt", arg, None),
            _ => unreachable!("retained floating operation"),
        };
        let suffix = if self.ty(a) == Type::Vector(VectorType::F32x16) {
            "ps"
        } else {
            "pd"
        };
        let opcode = Opcode::parse(&format!("{stem}{suffix}")).expect("AVX512F floating opcode");
        let a = self.use_freg(a, FS0);
        let b = b.map(|value| self.use_freg(value, FS1));
        let d = self.def_freg(result);
        if let Some(b) = b {
            self.emit(
                opcode,
                [
                    asm_arg(simd_width(d, 512)),
                    asm_arg(simd_width(a, 512)),
                    asm_arg(simd_width(b, 512)),
                ],
            );
        } else {
            self.emit(
                opcode,
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(a, 512))],
            );
        }
        self.def_done(result, d);
        true
    }
}
