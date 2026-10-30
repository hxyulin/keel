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
            floating(ty(args[0]))
                && matches!(op, V::Fadd | V::Fsub | V::Fmul | V::Fdiv | V::Fcopysign)
        }
        InstData::VectorUnary { op, arg } => {
            floating(ty(*arg))
                && matches!(
                    op,
                    UnaryOp::Fsqrt
                        | UnaryOp::Fneg
                        | UnaryOp::Fabs
                        | UnaryOp::Fceil
                        | UnaryOp::Ffloor
                        | UnaryOp::Ftrunc
                        | UnaryOp::Fnearest
                )
        }
        _ => false,
    }
}
fn opcode(name: &str) -> Opcode {
    Opcode::parse(name).expect("AVX512F floating opcode")
}
impl FunctionCompiler<'_> {
    fn float512_constant(&mut self, double: bool, bits: u64) {
        self.emit(Opcode::Movabs, [asm_arg(r64(S0)), asm_arg(bits)]);
        self.emit(Opcode::Movq, [asm_arg(xmm(FS2)), asm_arg(r64(S0))]);
        self.emit(
            opcode(if double {
                "vpbroadcastq"
            } else {
                "vpbroadcastd"
            }),
            [asm_arg(simd_width(FS2, 512)), asm_arg(xmm(FS2))],
        );
    }
    fn float512_bits(&mut self, name: &str, d: u8, a: u8, b: u8) {
        self.emit(
            opcode(name),
            [
                asm_arg(simd_width(d, 512)),
                asm_arg(simd_width(a, 512)),
                asm_arg(simd_width(b, 512)),
            ],
        );
    }
    pub(super) fn avx512_float(&mut self, data: &InstData, result: Option<Value>) -> bool {
        if !keep(self.func, data, self.features) {
            return false;
        }
        let (a, b) = match *data {
            InstData::VectorBinary { args, .. } => (args[0], Some(args[1])),
            InstData::VectorUnary { arg, .. } => (arg, None),
            _ => unreachable!("retained floating operation"),
        };
        let ty = self.ty(a);
        let double = ty == Type::Vector(VectorType::F64x8);
        let suffix = if double { "pd" } else { "ps" };
        for (value, scratch) in [(Some(a), FS0), (b, FS1)] {
            if let Some(value) = value {
                let source = self.use_freg(value, scratch);
                if source != scratch {
                    self.emit(
                        movs(ty),
                        [
                            asm_arg(simd_width(scratch, 512)),
                            asm_arg(simd_width(source, 512)),
                        ],
                    );
                }
            }
        }
        let sign = 1u64 << if double { 63 } else { 31 };
        match *data {
            InstData::VectorBinary {
                op: V::Fcopysign, ..
            } => {
                self.float512_constant(double, sign);
                self.float512_bits("vpandd", FS1, FS1, FS2);
                self.float512_bits("vpandnd", FS0, FS2, FS0);
                self.float512_bits("vpord", FS0, FS0, FS1);
            }
            InstData::VectorBinary { op, .. } => {
                let stem = match op {
                    V::Fadd => "vadd",
                    V::Fsub => "vsub",
                    V::Fmul => "vmul",
                    V::Fdiv => "vdiv",
                    _ => unreachable!("retained binary"),
                };
                self.float512_bits(&format!("{stem}{suffix}"), FS0, FS0, FS1);
            }
            InstData::VectorUnary {
                op: UnaryOp::Fneg | UnaryOp::Fabs,
                ..
            } => {
                self.float512_constant(double, sign);
                if matches!(
                    data,
                    InstData::VectorUnary {
                        op: UnaryOp::Fneg,
                        ..
                    }
                ) {
                    self.float512_bits("vpxord", FS0, FS0, FS2);
                } else {
                    self.float512_bits("vpandnd", FS0, FS2, FS0);
                }
            }
            InstData::VectorUnary {
                op: UnaryOp::Fsqrt, ..
            } => {
                self.emit(
                    opcode(&format!("vsqrt{suffix}")),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS0, 512))],
                );
            }
            InstData::VectorUnary { op, .. } => {
                let control = match op {
                    UnaryOp::Fnearest => 8,
                    UnaryOp::Ffloor => 9,
                    UnaryOp::Fceil => 10,
                    UnaryOp::Ftrunc => 11,
                    _ => unreachable!("retained rounding"),
                };
                self.emit(
                    opcode(&format!("vrndscale{suffix}")),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(control),
                    ],
                );
            }
            _ => unreachable!("retained floating operation"),
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                movs(ty),
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(FS0, 512))],
            );
        }
        self.def_done(result, d);
        true
    }
}
