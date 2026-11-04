//! Unmasked packed floating arithmetic, with source order matching scalar IR.
use super::*;
use keel_ir::{FloatCC, VectorBinaryOp as V, VectorType};
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
                && matches!(
                    op,
                    V::Fadd | V::Fsub | V::Fmul | V::Fdiv | V::Fcopysign | V::Fmin | V::Fmax
                )
        }
        InstData::VectorFcmp { args, .. } => floating(ty(args[0])),
        InstData::VectorSelect { args } => floating(ty(args[1])),
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
fn k1() -> Register {
    Register {
        class: RegisterClass::Mask,
        number: 1,
        high_byte: false,
        width: 64,
    }
}
fn masked_move(double: bool) -> Opcode {
    let mut op = EvexOpcode::new(if double {
        EvexMnemonic::Vmovdqu64
    } else {
        EvexMnemonic::Vmovdqu32
    });
    op.options.mask = Some(1);
    Opcode::Evex(op)
}
impl FunctionCompiler<'_> {
    fn float512_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                movs(self.ty(value)),
                [
                    asm_arg(simd_width(scratch, 512)),
                    asm_arg(simd_width(source, 512)),
                ],
            );
        }
    }
    fn float512_compare(&mut self, mnemonic: EvexMnemonic, a: u8, b: u8, predicate: i32) {
        self.emit(
            Opcode::Evex(EvexOpcode::new(mnemonic)),
            [
                asm_arg(k1()),
                asm_arg(simd_width(a, 512)),
                asm_arg(simd_width(b, 512)),
                asm_arg(predicate),
            ],
        );
    }
    fn float512_finish(&mut self, result: Option<Value>, source: u8) {
        let d = self.def_freg(result);
        if d != source {
            self.emit(
                Opcode::Vmovups,
                [
                    asm_arg(simd_width(d, 512)),
                    asm_arg(simd_width(source, 512)),
                ],
            );
        }
        self.def_done(result, d);
    }

    fn float512_constant(&mut self, double: bool, bits: u64) {
        self.emit(Opcode::Movabs, [asm_arg(r64(S0)), asm_arg(bits)]);
        self.emit(Opcode::Movq, [asm_arg(xmm(FS2)), asm_arg(r64(S0))]);
        self.emit(
            if double {
                op::VPBROADCASTQ
            } else {
                op::VPBROADCASTD
            },
            [asm_arg(simd_width(FS2, 512)), asm_arg(xmm(FS2))],
        );
    }
    fn float512_bits(&mut self, opcode: Opcode, d: u8, a: u8, b: u8) {
        self.emit(
            opcode,
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
        if let InstData::VectorSelect { args } = *data {
            let double = self.ty(args[1]) == Type::Vector(VectorType::F64x8);
            self.float512_stage(args[0], FS0);
            self.float512_bits(op::VPXORD, FS1, FS1, FS1);
            self.float512_compare(
                if double {
                    EvexMnemonic::Vpcmpq
                } else {
                    EvexMnemonic::Vpcmpd
                },
                FS0,
                FS1,
                4,
            );
            self.float512_stage(args[1], FS1);
            self.float512_stage(args[2], FS2);
            self.emit(
                masked_move(double),
                [asm_arg(simd_width(FS2, 512)), asm_arg(simd_width(FS1, 512))],
            );
            self.float512_finish(result, FS2);
            return true;
        }
        if let InstData::VectorFcmp { cond, args } = *data {
            let double = self.ty(args[0]) == Type::Vector(VectorType::F64x8);
            self.float512_stage(args[0], FS0);
            self.float512_stage(args[1], FS1);
            let predicate = match cond {
                FloatCC::Eq => 0,
                FloatCC::Ne => 4,
                FloatCC::Lt => 1,
                FloatCC::Le => 2,
                FloatCC::Gt => 14,
                FloatCC::Ge => 13,
                FloatCC::Ord => 7,
                FloatCC::Uno => 3,
            };
            self.float512_compare(
                if double {
                    EvexMnemonic::Vcmppd
                } else {
                    EvexMnemonic::Vcmpps
                },
                FS0,
                FS1,
                predicate,
            );
            self.float512_constant(double, u64::MAX);
            self.float512_bits(op::VPXORD, FS0, FS0, FS0);
            self.emit(
                masked_move(double),
                [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS2, 512))],
            );
            self.float512_finish(result, FS0);
            return true;
        }
        let (a, b) = match *data {
            InstData::VectorBinary { args, .. } => (args[0], Some(args[1])),
            InstData::VectorUnary { arg, .. } => (arg, None),
            _ => unreachable!("retained floating operation"),
        };
        let ty = self.ty(a);
        let double = ty == Type::Vector(VectorType::F64x8);
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
                op: V::Fmin | V::Fmax,
                ..
            } => {
                let minimum = matches!(data, InstData::VectorBinary { op: V::Fmin, .. });
                let compare = if double {
                    EvexMnemonic::Vcmppd
                } else {
                    EvexMnemonic::Vcmpps
                };
                self.float512_compare(compare, FS0, FS1, 3);
                let opcode = match (minimum, double) {
                    (true, false) => op::VMINPS,
                    (true, true) => op::VMINPD,
                    (false, false) => op::VMAXPS,
                    (false, true) => op::VMAXPD,
                };
                self.float512_bits(opcode, FS2, FS0, FS1);
                self.float512_bits(opcode, FS0, FS1, FS0);
                // Both source orders choose opposite zero signs on ties. OR
                // selects -0 for minimum; AND selects +0 for maximum.
                self.float512_bits(if minimum { op::VPORD } else { op::VPANDD }, FS0, FS0, FS2);
                self.float512_constant(
                    double,
                    if double {
                        0x7ff8000000000000
                    } else {
                        0x7fc00000
                    },
                );
                self.emit(
                    masked_move(double),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS2, 512))],
                );
            }
            InstData::VectorBinary {
                op: V::Fcopysign, ..
            } => {
                self.float512_constant(double, sign);
                self.float512_bits(op::VPANDD, FS1, FS1, FS2);
                self.float512_bits(op::VPANDND, FS0, FS2, FS0);
                self.float512_bits(op::VPORD, FS0, FS0, FS1);
            }
            InstData::VectorBinary { op, .. } => {
                let opcode = match (op, double) {
                    (V::Fadd, false) => Opcode::Vaddps,
                    (V::Fadd, true) => Opcode::Vaddpd,
                    (V::Fsub, false) => Opcode::Vsubps,
                    (V::Fsub, true) => Opcode::Vsubpd,
                    (V::Fmul, false) => Opcode::Vmulps,
                    (V::Fmul, true) => Opcode::Vmulpd,
                    (V::Fdiv, false) => Opcode::Vdivps,
                    (V::Fdiv, true) => Opcode::Vdivpd,
                    _ => unreachable!("retained binary"),
                };
                self.float512_bits(opcode, FS0, FS0, FS1);
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
                    self.float512_bits(op::VPXORD, FS0, FS0, FS2);
                } else {
                    self.float512_bits(op::VPANDND, FS0, FS2, FS0);
                }
            }
            InstData::VectorUnary {
                op: UnaryOp::Fsqrt, ..
            } => {
                self.emit(
                    if double { op::VSQRTPD } else { op::VSQRTPS },
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
                    if double {
                        op::VRNDSCALEPD
                    } else {
                        op::VRNDSCALEPS
                    },
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
