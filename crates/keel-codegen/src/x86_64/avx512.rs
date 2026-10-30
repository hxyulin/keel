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
        InstData::VectorShuffle { args, .. } | InstData::VectorInsert { args, .. } => {
            lane512(ty(args[0]))
        }
        InstData::VectorExtract { arg, .. } => lane512(ty(*arg)),
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

fn lane512(ty: Type) -> bool {
    ty.vector().is_some_and(|shape| {
        shape.bytes() == 64 && matches!(shape.lane_type().bytes(), Some(4 | 8))
    })
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
            InstData::VectorShuffle { args, ref lanes } => {
                // Capture both live sources before loading the destructive index
                // destination. Physical scratch views alias across all widths.
                for (value, scratch) in [(args[0], FS1), (args[1], FS2)] {
                    let source = self.use_freg(value, scratch);
                    if source != scratch {
                        self.emit(
                            Opcode::Vmovups,
                            [
                                asm_arg(simd_width(scratch, 512)),
                                asm_arg(simd_width(source, 512)),
                            ],
                        );
                    }
                }
                let width = self
                    .ty(args[0])
                    .vector()
                    .unwrap()
                    .lane_type()
                    .bytes()
                    .unwrap() as usize;
                let mut indices = [0u8; 64];
                for (lane, &index) in lanes.iter().enumerate() {
                    indices[lane * width..(lane + 1) * width]
                        .copy_from_slice(&u64::from(index).to_le_bytes()[..width]);
                }
                let home = self.copy_slot(self.copy_temp.expect("512-bit index staging"));
                for (chunk, bytes) in indices.as_chunks::<8>().0.iter().enumerate() {
                    self.emit(
                        Opcode::Movabs,
                        [asm_arg(r64(S0)), asm_arg(u64::from_le_bytes(*bytes))],
                    );
                    self.emit(
                        Opcode::Mov,
                        [
                            asm_arg(sized(offset_memory(&home, (chunk * 8) as i128), 64)),
                            asm_arg(r64(S0)),
                        ],
                    );
                }
                self.emit(
                    Opcode::Vmovups,
                    [asm_arg(simd_width(FS0, 512)), asm_arg(home)],
                );
                self.emit(
                    Opcode::parse(if width == 4 { "vpermi2d" } else { "vpermi2q" })
                        .expect("AVX512F permutation"),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(simd_width(FS2, 512)),
                    ],
                );
                let d = self.def_freg(result);
                if d != FS0 {
                    self.emit(
                        Opcode::Vmovups,
                        [asm_arg(simd_width(d, 512)), asm_arg(simd_width(FS0, 512))],
                    );
                }
                self.def_done(result, d);
            }
            InstData::VectorExtract { arg, lane } => {
                let shape = self.ty(arg).vector().unwrap();
                let width = shape.lane_type().bytes().unwrap();
                let a = self.use_freg(arg, FS0);
                self.emit(
                    Opcode::parse("vextracti32x4").expect("AVX512F quarter extract"),
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(simd_width(a, 512)),
                        asm_arg(i64::from(u32::from(lane) * width / 16)),
                    ],
                );
                let control = if width == 4 {
                    i64::from(lane % 4) * 0x55
                } else if lane % 2 == 0 {
                    0x44
                } else {
                    0xee
                };
                self.emit(
                    Opcode::Pshufd,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(control)],
                );
                if shape.lane_type().is_float() {
                    let d = self.def_freg(result);
                    self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(FS1))]);
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    self.emit(
                        if width == 4 {
                            Opcode::Movd
                        } else {
                            Opcode::Movq
                        },
                        [
                            asm_arg(if width == 4 { r32(d) } else { r64(d) }),
                            asm_arg(xmm(FS1)),
                        ],
                    );
                    self.def_done(result, d);
                }
            }
            InstData::VectorInsert { args, lane } => {
                let scalar = self.ty(args[1]);
                let width = scalar.bytes().unwrap();
                let a = self.use_freg(args[0], FS0);
                let quarter = i64::from(u32::from(lane) * width / 16);
                self.emit(
                    Opcode::parse("vextracti32x4").expect("AVX512F quarter extract"),
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(simd_width(a, 512)),
                        asm_arg(quarter),
                    ],
                );
                // Patch one scalar in a captured quarter, preserving every other
                // bit without requiring SSE4.1 scalar insert instructions.
                let home = self.copy_slot(self.copy_temp.expect("512-bit lane staging"));
                self.emit(
                    Opcode::Movups,
                    [asm_arg(sized(home.clone(), 128)), asm_arg(xmm(FS1))],
                );
                let at = sized(
                    offset_memory(&home, i128::from(u32::from(lane) * width % 16)),
                    (width * 8) as u16,
                );
                if scalar.is_float() {
                    let b = self.use_freg(args[1], FS2);
                    self.emit(movs(scalar), [asm_arg(at), asm_arg(xmm(b))]);
                } else {
                    let b = self.use_reg(args[1], S0);
                    self.emit(
                        Opcode::Mov,
                        [
                            asm_arg(at),
                            asm_arg(if width == 4 { r32(b) } else { r64(b) }),
                        ],
                    );
                }
                self.emit(
                    Opcode::Movups,
                    [asm_arg(xmm(FS1)), asm_arg(sized(home, 128))],
                );
                let d = self.def_freg(result);
                self.emit(
                    Opcode::parse("vinserti32x4").expect("AVX512F quarter insert"),
                    [
                        asm_arg(simd_width(d, 512)),
                        asm_arg(simd_width(a, 512)),
                        asm_arg(xmm(FS1)),
                        asm_arg(quarter),
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
