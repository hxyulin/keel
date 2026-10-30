//! Unmasked AVX512F integer primitives. Byte and word arithmetic remains portable.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};

fn integer(ty: Type) -> bool {
    matches!(ty, Type::Vector(VectorType::I32x16 | VectorType::I64x8))
}

pub(super) fn keep(func: &Function, data: &InstData, features: keel_asm_ir::x86::Features) -> bool {
    if !features.avx512f {
        return false;
    }
    let ty = |value| func.value_type(func.resolve(value));
    match data {
        InstData::VectorBinary { op, args } => {
            integer(ty(args[0]))
                && (matches!(
                    op,
                    V::Iadd
                        | V::Isub
                        | V::And
                        | V::Or
                        | V::Xor
                        | V::Smin
                        | V::Smax
                        | V::Umin
                        | V::Umax
                        | V::Shl
                        | V::Ushr
                        | V::Sshr
                ) || *op == V::Imul && ty(args[0]) == Type::Vector(VectorType::I32x16))
        }
        InstData::VectorSplat { ty, .. } => integer(*ty),
        InstData::Convert {
            op: ConvertOp::Bitcast,
            ty: result,
            arg,
        } => {
            result.vector().is_some_and(|shape| shape.bytes() == 64)
                && ty(*arg).vector().is_some_and(|shape| shape.bytes() == 64)
        }
        _ => false,
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn avx512(&mut self, data: &InstData, result: Option<Value>) -> bool {
        if !keep(self.func, data, self.features) {
            return false;
        }
        match *data {
            InstData::VectorBinary { op, args } => {
                // Three-operand forms read both sources before writing. Separate
                // reload scratches cover spilled sources and a spilled result.
                let a = self.use_freg(args[0], FS0);
                let mut b = self.use_freg(args[1], FS1);
                let d = self.def_freg(result);
                let qword = self.ty(args[0]) == Type::Vector(VectorType::I64x8);
                if matches!(op, V::Shl | V::Ushr | V::Sshr) {
                    // IR shifts reduce each count modulo lane width. Hardware
                    // variable shifts instead saturate, so normalize a copy.
                    self.emit(
                        Opcode::Mov,
                        [asm_arg(r32(S0)), asm_arg(if qword { 63 } else { 31 })],
                    );
                    self.emit(Opcode::Movd, [asm_arg(xmm(FS2)), asm_arg(r32(S0))]);
                    self.emit(
                        Opcode::parse(if qword {
                            "vpbroadcastq"
                        } else {
                            "vpbroadcastd"
                        })
                        .expect("AVX512F broadcast"),
                        [asm_arg(simd_width(FS2, 512)), asm_arg(xmm(FS2))],
                    );
                    self.emit(
                        Opcode::parse("vpandd").expect("AVX512F AND"),
                        [
                            asm_arg(simd_width(FS1, 512)),
                            asm_arg(simd_width(b, 512)),
                            asm_arg(simd_width(FS2, 512)),
                        ],
                    );
                    b = FS1;
                }
                let name = match (op, qword) {
                    (V::Iadd, false) => "vpaddd",
                    (V::Iadd, true) => "vpaddq",
                    (V::Isub, false) => "vpsubd",
                    (V::Isub, true) => "vpsubq",
                    (V::Imul, false) => "vpmulld",
                    (V::Smin, false) => "vpminsd",
                    (V::Smin, true) => "vpminsq",
                    (V::Smax, false) => "vpmaxsd",
                    (V::Smax, true) => "vpmaxsq",
                    (V::Umin, false) => "vpminud",
                    (V::Umin, true) => "vpminuq",
                    (V::Umax, false) => "vpmaxud",
                    (V::Umax, true) => "vpmaxuq",
                    (V::Shl, false) => "vpsllvd",
                    (V::Shl, true) => "vpsllvq",
                    (V::Ushr, false) => "vpsrlvd",
                    (V::Ushr, true) => "vpsrlvq",
                    (V::Sshr, false) => "vpsravd",
                    (V::Sshr, true) => "vpsravq",
                    (V::And, _) => "vpandd",
                    (V::Or, _) => "vpord",
                    (V::Xor, _) => "vpxord",
                    _ => unreachable!("retained AVX512F binary operation"),
                };
                self.emit(
                    Opcode::parse(name).expect("AVX512F opcode"),
                    [
                        asm_arg(simd_width(d, 512)),
                        asm_arg(simd_width(a, 512)),
                        asm_arg(simd_width(b, 512)),
                    ],
                );
                self.def_done(result, d);
            }
            InstData::VectorSplat { ty, arg } => {
                let a = self.use_reg(arg, S0);
                let d = self.def_freg(result);
                let qword = ty == Type::Vector(VectorType::I64x8);
                self.emit(
                    if qword { Opcode::Movq } else { Opcode::Movd },
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(if qword { r64(a) } else { r32(a) }),
                    ],
                );
                let name = if qword {
                    "vpbroadcastq"
                } else {
                    "vpbroadcastd"
                };
                self.emit(
                    Opcode::parse(name).expect("AVX512F broadcast"),
                    [asm_arg(simd_width(d, 512)), asm_arg(xmm(FS1))],
                );
                self.def_done(result, d);
            }
            InstData::Convert { arg, .. } => {
                let a = self.use_freg(arg, FS1);
                let d = self.def_freg(result);
                if d != a {
                    self.emit(
                        Opcode::Vmovups,
                        [asm_arg(simd_width(d, 512)), asm_arg(simd_width(a, 512))],
                    );
                }
                self.def_done(result, d);
            }
            _ => unreachable!("retained AVX512F operation"),
        }
        true
    }
}
