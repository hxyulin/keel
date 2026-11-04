//! Packed 256-bit selection. Inputs are captured in reserved physical SIMD
//! indices before any result write, except direct three-operand integer adds.
//! SIMD indices12..15 are reserved in wide
//! functions, and alias the XMM views used for scalar/lane transfers.
use super::*;
use keel_ir::{FloatCC, VectorBinaryOp as V, VectorType};
const W: u8 = 12;
/// Integer opcode families, indexed by `int_lane`: byte, word, dword, qword.
const VPADD: [Opcode; 4] = [op::VPADDB, op::VPADDW, op::VPADDD, op::VPADDQ];
const VPSUB: [Opcode; 4] = [op::VPSUBB, op::VPSUBW, op::VPSUBD, op::VPSUBQ];
const VPCMPEQ: [Opcode; 4] = [op::VPCMPEQB, op::VPCMPEQW, op::VPCMPEQD, op::VPCMPEQQ];
const VPCMPGT: [Opcode; 4] = [op::VPCMPGTB, op::VPCMPGTW, op::VPCMPGTD, op::VPCMPGTQ];
/// Byte lanes shift as words; `wshift_imm` masks the bits that cross lanes.
const VPSLL: [Opcode; 4] = [op::VPSLLW, op::VPSLLW, op::VPSLLD, op::VPSLLQ];
const VPSRL: [Opcode; 4] = [op::VPSRLW, op::VPSRLW, op::VPSRLD, op::VPSRLQ];
const VPMINS: [Opcode; 3] = [op::VPMINSB, op::VPMINSW, op::VPMINSD];
const VPMINU: [Opcode; 3] = [op::VPMINUB, op::VPMINUW, op::VPMINUD];
const VPMAXS: [Opcode; 3] = [op::VPMAXSB, op::VPMAXSW, op::VPMAXSD];
const VPMAXU: [Opcode; 3] = [op::VPMAXUB, op::VPMAXUW, op::VPMAXUD];
const VPBROADCAST: [Opcode; 4] = [
    op::VPBROADCASTB,
    op::VPBROADCASTW,
    op::VPBROADCASTD,
    op::VPBROADCASTQ,
];
const VPEXTR: [Opcode; 4] = [op::VPEXTRB, op::VPEXTRW, op::VPEXTRD, op::VPEXTRQ];
const VPINSR: [Opcode; 4] = [op::VPINSRB, op::VPINSRW, op::VPINSRD, op::VPINSRQ];
fn y(n: u8) -> Register {
    simd_width(n, 256)
}
/// Index of the integer lane form: 0 byte, 1 word, 2 dword, 3 qword.
fn int_lane(ty: Type) -> usize {
    match ty.vector().unwrap().lane_type() {
        Type::I8 => 0,
        Type::I16 => 1,
        Type::I32 => 2,
        Type::I64 => 3,
        _ => unreachable!(),
    }
}
/// Index of a form by lane size in bytes, with sizes above four taking qword.
fn size_lane(bytes: u32) -> usize {
    match bytes {
        1 => 0,
        2 => 1,
        4 => 2,
        _ => 3,
    }
}
/// The packed-single or packed-double form for a float lane type.
fn float_lane(ty: Type, [ps, pd]: [Opcode; 2]) -> Opcode {
    match ty.vector().unwrap().lane_type() {
        Type::F32 => ps,
        Type::F64 => pd,
        _ => unreachable!(),
    }
}
fn wide(ty: Type) -> bool {
    ty.vector().is_some_and(|v| v.bytes() == 32)
}
pub(super) fn keep(f: &Function, d: &InstData, features: keel_asm_ir::x86::Features) -> bool {
    let t = |v| f.value_type(f.resolve(v));
    match d {
        InstData::VectorBinary { op, args } if wide(t(args[0])) => match op {
            V::Fadd
            | V::Fsub
            | V::Fmul
            | V::Fdiv
            | V::Fmin
            | V::Fmax
            | V::Fcopysign
            | V::And
            | V::Or
            | V::Xor => true,
            V::Iadd | V::Isub | V::Imul | V::Shl | V::Ushr | V::Sshr => features.avx2,
            _ => false, // portable compound operations expand into these packed primitives
        },
        InstData::VectorIcmp { args, .. } => wide(t(args[0])) && features.avx2,
        InstData::VectorFcmp { args, .. } => wide(t(args[0])),
        InstData::VectorSelect { args } => wide(t(args[1])) && features.avx2,
        InstData::VectorSplat { ty, .. } => wide(*ty),
        InstData::VectorShuffle { args, .. } => wide(t(args[0])) && features.avx2,
        InstData::VectorExtract { arg, .. } => wide(t(*arg)),
        InstData::VectorReduce { arg, .. } => wide(t(*arg)) && features.avx2,
        InstData::VectorInsert { args, .. } => wide(t(args[0])),
        InstData::VectorUnary { op, arg } if wide(t(*arg)) => {
            features.avx2
                || matches!(
                    op,
                    UnaryOp::Fneg
                        | UnaryOp::Fabs
                        | UnaryOp::Fsqrt
                        | UnaryOp::Fceil
                        | UnaryOp::Ffloor
                        | UnaryOp::Ftrunc
                        | UnaryOp::Fnearest
                )
        }
        InstData::Convert {
            op: ConvertOp::Bitcast,
            ty,
            arg,
        } => wide(*ty) && wide(t(*arg)),
        InstData::Convert {
            op:
                ConvertOp::SwidenLow
                | ConvertOp::UwidenLow
                | ConvertOp::SwidenHigh
                | ConvertOp::UwidenHigh,
            ty,
            arg,
        } => (wide(*ty) || wide(t(*arg))) && features.avx2 && t(*arg).vector().is_some(),
        InstData::Convert {
            op: ConvertOp::NarrowLow,
            ty,
            arg,
        } => features.avx2 && (wide(*ty) || wide(t(*arg))) && ty.is_vector() && t(*arg).is_vector(),
        _ => false,
    }
}
/// VPSHUFD repeats one four-dword control independently in each 128-bit half.
/// Equal resolved sources permit either spelling of each concatenated index.
fn dword_shuffle_control(ty: Type, same: bool, lanes: &[u8]) -> Option<u8> {
    if !same
        || !matches!(ty, Type::Vector(VectorType::I32x8 | VectorType::F32x8))
        || lanes.len() != 8
        || lanes.iter().any(|&lane| lane >= 16)
    {
        return None;
    }
    let mut imm = 0;
    for i in 0..4 {
        let low = lanes[i] % 8;
        if low >= 4 || lanes[i + 4] % 8 != low + 4 {
            return None;
        }
        imm |= low << (2 * i);
    }
    Some(imm)
}
impl FunctionCompiler<'_> {
    fn wstage(&mut self, v: Value, d: u8) {
        let r = self.use_freg(v, d);
        if r != d {
            self.emit(Opcode::Vmovups, [asm_arg(y(d)), asm_arg(y(r))]);
        }
    }
    fn wdone(&mut self, r: Option<Value>) {
        let d = self.def_freg(r);
        if d != FS0 {
            self.emit(Opcode::Vmovups, [asm_arg(y(d)), asm_arg(y(FS0))]);
        }
        self.def_done(r, d);
    }
    fn w3(&mut self, opcode: Opcode, d: u8, a: u8, b: u8) {
        self.emit(opcode, [asm_arg(y(d)), asm_arg(y(a)), asm_arg(y(b))]);
    }
    fn wi(&mut self, opcode: Opcode, d: u8, a: u8, imm: i64) {
        self.emit(opcode, [asm_arg(y(d)), asm_arg(y(a)), asm_arg(imm)]);
    }
    /// Parallel-copy temporary is dead between IR instructions and has32 bytes.
    /// Constants use memory operands, leaving all four SIMD scratch views usable.
    fn wc(&mut self, bytes: [u8; 32]) -> Memory {
        let at = self.copy_slot(self.copy_temp.expect("wide SIMD temporary"));
        for (i, b) in bytes.as_chunks::<8>().0.iter().enumerate() {
            let n = u64::from_le_bytes(*b);
            self.emit(Opcode::Movabs, [asm_arg(r64(S0)), asm_arg(n)]);
            self.emit(
                Opcode::Mov,
                [
                    asm_arg(sized(offset_memory(&at, (i * 8) as i128), 64)),
                    asm_arg(r64(S0)),
                ],
            );
        }
        at
    }
    fn wk(&mut self, ty: Type, k: u64) -> Memory {
        let n = ty.vector().unwrap().lane_type().bytes().unwrap() as usize;
        let b = k.to_le_bytes();
        self.wc(std::array::from_fn(|i| b[i % n]))
    }
    fn wk3(&mut self, opcode: Opcode, d: u8, a: u8, ty: Type, k: u64) {
        let m = self.wk(ty, k);
        self.emit(opcode, [asm_arg(y(d)), asm_arg(y(a)), asm_arg(m)]);
    }
    fn wshift_imm(&mut self, op: V, ty: Type, d: u8, a: u8, n: u8) {
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        let forms = match op {
            V::Shl => VPSLL,
            _ => VPSRL,
        };
        self.wi(forms[int_lane(ty)], d, a, n.into());
        if bits == 8 {
            self.wk3(
                op::VPAND,
                d,
                d,
                ty,
                if op == V::Shl {
                    (255u64 << n) & 255
                } else {
                    255 >> n
                },
            );
        }
    }
    fn wcompare(&mut self, cond: IntCC, ty: Type) {
        let invert = matches!(
            cond,
            IntCC::Ne | IntCC::Sle | IntCC::Sge | IntCC::Ule | IntCC::Uge
        );
        let swap = matches!(cond, IntCC::Slt | IntCC::Sge | IntCC::Ult | IntCC::Uge);
        if matches!(cond, IntCC::Ult | IntCC::Ule | IntCC::Ugt | IntCC::Uge) {
            let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
            let m = self.wk(ty, 1u64 << (bits - 1));
            self.emit(op::VPXOR, [asm_arg(y(FS0)), asm_arg(y(FS0)), asm_arg(&m)]);
            self.emit(op::VPXOR, [asm_arg(y(FS1)), asm_arg(y(FS1)), asm_arg(m)]);
        }
        let forms = if matches!(cond, IntCC::Eq | IntCC::Ne) {
            VPCMPEQ
        } else {
            VPCMPGT
        };
        self.w3(
            forms[int_lane(ty)],
            FS0,
            if swap { FS1 } else { FS0 },
            if swap { FS0 } else { FS1 },
        );
        if invert {
            self.w3(op::VPCMPEQD, W, W, W);
            self.w3(op::VPXOR, FS0, FS0, W);
        }
    }
    fn wmul(&mut self, ty: Type) {
        match ty.vector().unwrap().lane_type() {
            Type::I16 => self.w3(op::VPMULLW, FS0, FS0, FS1),
            Type::I32 => self.w3(op::VPMULLD, FS0, FS0, FS1),
            Type::I8 => {
                self.w3(op::VPMULLW, FS2, FS0, FS1);
                self.wi(op::VPSRLW, FS0, FS0, 8);
                self.wi(op::VPSRLW, FS1, FS1, 8);
                self.w3(op::VPMULLW, FS0, FS0, FS1);
                self.wi(op::VPSLLW, FS0, FS0, 8);
                self.wk3(op::VPAND, FS2, FS2, Type::Vector(VectorType::I16x16), 255);
                self.w3(op::VPOR, FS0, FS0, FS2);
            }
            Type::I64 => {
                self.w3(op::VPMULUDQ, FS2, FS0, FS1);
                self.wi(op::VPSHUFD, FS0, FS0, 0xb1);
                self.w3(op::VPMULLD, FS0, FS0, FS1);
                self.wi(op::VPSHUFD, FS1, FS0, 0xb1);
                self.w3(op::VPADDD, FS0, FS0, FS1);
                self.wi(op::VPSLLQ, FS0, FS0, 32);
                self.w3(op::VPADDQ, FS0, FS0, FS2);
            }
            _ => unreachable!(),
        }
    }
    fn wshift(&mut self, operation: V, ty: Type, counts: Value) {
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        if bits >= 32 {
            self.wk3(op::VPAND, FS1, FS1, ty, (bits - 1) as u64);
            if bits == 64 && operation == V::Sshr {
                self.w3(op::VPXOR, FS2, FS2, FS2);
                self.w3(op::VPCMPGTQ, FS2, FS2, FS0);
                self.w3(op::VPXOR, FS0, FS0, FS2);
                self.w3(op::VPSRLVQ, FS0, FS0, FS1);
                self.w3(op::VPXOR, FS0, FS0, FS2);
            } else {
                // Dword and qword forms. AVX2 has no VPSRAVQ; qword Sshr
                // takes the sign-flip sequence above.
                let forms: &[Opcode] = match operation {
                    V::Shl => &[op::VPSLLVD, op::VPSLLVQ],
                    V::Ushr => &[op::VPSRLVD, op::VPSRLVQ],
                    _ => &[op::VPSRAVD],
                };
                self.w3(forms[int_lane(ty) - 2], FS0, FS0, FS1);
            }
            return;
        }
        for bit in 0..bits.trailing_zeros() {
            let shift = 1u8 << bit;
            self.wstage(counts, FS2);
            let m = self.wk(ty, 1 << bit);
            self.emit(op::VPAND, [asm_arg(y(FS2)), asm_arg(y(FS2)), asm_arg(&m)]);
            self.emit(
                VPCMPEQ[int_lane(ty)],
                [asm_arg(y(FS2)), asm_arg(y(FS2)), asm_arg(m)],
            );
            if operation == V::Sshr && bits == 16 {
                self.wi(op::VPSRAW, W, FS0, shift.into());
            } else {
                self.wshift_imm(
                    if operation == V::Shl { V::Shl } else { V::Ushr },
                    ty,
                    W,
                    FS0,
                    shift,
                );
                if operation == V::Sshr {
                    self.w3(op::VPXOR, FS1, FS1, FS1);
                    self.w3(op::VPCMPGTB, FS1, FS1, FS0);
                    self.wk3(op::VPAND, FS1, FS1, ty, (255u64 << (8 - shift)) & 255);
                    self.w3(op::VPOR, W, W, FS1);
                }
            }
            self.w3(op::VPXOR, W, W, FS0);
            self.w3(op::VPAND, W, W, FS2);
            self.w3(op::VPXOR, FS0, FS0, W);
        }
    }
    fn wpopcount(&mut self, ty: Type) {
        let byte = Type::Vector(VectorType::I8x32);
        self.wk3(op::VPAND, FS1, FS0, byte, 15);
        self.wi(op::VPSRLW, FS2, FS0, 4);
        self.wk3(op::VPAND, FS2, FS2, byte, 15);
        let lookup = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4];
        let m = self.wc(std::array::from_fn(|i| lookup[i % 16]));
        self.emit(Opcode::Vmovups, [asm_arg(y(W)), asm_arg(m)]);
        self.w3(op::VPSHUFB, FS1, W, FS1);
        self.w3(op::VPSHUFB, FS2, W, FS2);
        self.w3(op::VPADDB, FS0, FS1, FS2);
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        if bits >= 16 {
            self.wk3(op::VPMADDUBSW, FS0, FS0, byte, 1);
        }
        if bits >= 32 {
            self.wk3(op::VPMADDWD, FS0, FS0, Type::Vector(VectorType::I16x16), 1);
        }
        if bits == 64 {
            self.wi(op::VPSRLQ, FS1, FS0, 32);
            self.w3(op::VPADDQ, FS0, FS0, FS1);
            self.wk3(op::VPAND, FS0, FS0, ty, u32::MAX as u64);
        }
    }
    fn wshuffle(&mut self, ty: Type, args: [Value; 2], lanes: &[u8]) {
        if self.features.avx2
            && let Some(imm) = dword_shuffle_control(
                ty,
                self.func.resolve(args[0]) == self.func.resolve(args[1]),
                lanes,
            )
        {
            self.wstage(args[0], FS1);
            self.wi(op::VPSHUFD, FS0, FS1, i64::from(imm));
            return;
        }
        self.wstage(args[0], FS1);
        self.wstage(args[1], FS2);
        self.w3(Opcode::Vxorps, FS0, FS0, FS0);
        let width = ty.vector().unwrap().lane_type().bytes().unwrap() as usize;
        for input in 0..2 {
            for half in 0..2 {
                let mask = std::array::from_fn(|byte| {
                    let src = usize::from(lanes[byte / width]) * width + byte % width;
                    if src / 32 == input && src % 32 / 16 == half {
                        (src % 16) as u8
                    } else {
                        0x80
                    }
                });
                if mask.iter().all(|&b| b == 0x80) {
                    continue;
                }
                self.emit(
                    op::VPERM2I128,
                    [
                        asm_arg(y(W)),
                        asm_arg(y(if input == 0 { FS1 } else { FS2 })),
                        asm_arg(y(if input == 0 { FS1 } else { FS2 })),
                        asm_arg(if half == 0 { 0 } else { 0x11 }),
                    ],
                );
                let m = self.wc(mask);
                self.emit(op::VPSHUFB, [asm_arg(y(W)), asm_arg(y(W)), asm_arg(m)]);
                self.w3(op::VPOR, FS0, FS0, W);
            }
        }
    }
    fn wreduce_pair(&mut self, operation: keel_ir::VectorReduceOp, ty: Type) {
        use keel_ir::VectorReduceOp as R;
        let lane = ty.vector().unwrap().lane_type();
        let bitwise = match operation {
            R::Add => Some(VPADD[int_lane(ty)]),
            R::And => Some(op::VPAND),
            R::Or => Some(op::VPOR),
            R::Xor => Some(op::VPXOR),
            _ => None,
        };
        if let Some(opcode) = bitwise {
            self.w3(opcode, FS0, FS0, FS1);
            return;
        }
        let signed = matches!(operation, R::Smin | R::Smax);
        let min = matches!(operation, R::Smin | R::Umin);
        if lane != Type::I64 {
            let forms = match (min, signed) {
                (true, true) => VPMINS,
                (true, false) => VPMINU,
                (false, true) => VPMAXS,
                (false, false) => VPMAXU,
            };
            self.w3(forms[int_lane(ty)], FS0, FS0, FS1);
        } else {
            self.emit(Opcode::Vmovups, [asm_arg(y(FS2)), asm_arg(y(FS0))]);
            self.emit(Opcode::Vmovups, [asm_arg(y(W)), asm_arg(y(FS1))]);
            self.wcompare(
                match (signed, min) {
                    (true, true) => IntCC::Slt,
                    (true, false) => IntCC::Sgt,
                    (false, true) => IntCC::Ult,
                    (false, false) => IntCC::Ugt,
                },
                ty,
            );
            self.w3(op::VPAND, FS2, FS2, FS0);
            self.w3(op::VPANDN, FS0, FS0, W);
            self.w3(op::VPOR, FS0, FS0, FS2);
        }
    }
    pub(super) fn avx256(&mut self, data: &InstData, result: Option<Value>) -> bool {
        if !keep(self.func, data, self.features) {
            return false;
        }
        match data {
            InstData::VectorBinary {
                op: operation,
                args,
            } => {
                let ty = self.ty(args[0]);
                if *operation == V::Iadd {
                    // Wide allocation excludes FS0/FS1 (including their XMM
                    // views). use_freg resolves aliases and reloads only into
                    // its requested scratch, at the original 32-byte width.
                    // Capture both operands before writing: VEX integer adds
                    // permit d == a or d == b and read the full YMM payloads.
                    // Distinct reload scratches also cover
                    // two spilled inputs and a spilled destination (FS0).
                    let a = self.use_freg(args[0], FS0);
                    let b = self.use_freg(args[1], FS1);
                    let d = self.def_freg(result);
                    self.w3(VPADD[int_lane(ty)], d, a, b);
                    self.def_done(result, d);
                    return true;
                }
                self.wstage(args[0], FS0);
                self.wstage(args[1], FS1);
                match operation {
                    V::And | V::Or | V::Xor => self.w3(
                        match operation {
                            V::And => Opcode::Vandps,
                            V::Or => Opcode::Vorps,
                            _ => Opcode::Vxorps,
                        },
                        FS0,
                        FS0,
                        FS1,
                    ),
                    V::Iadd | V::Isub => self.w3(
                        if *operation == V::Iadd { VPADD } else { VPSUB }[int_lane(ty)],
                        FS0,
                        FS0,
                        FS1,
                    ),
                    V::Imul => self.wmul(ty),
                    V::Shl | V::Ushr | V::Sshr => self.wshift(*operation, ty, args[1]),
                    V::Fmin | V::Fmax => {
                        self.emit(
                            float_lane(ty, [op::VCMPPS, op::VCMPPD]),
                            [asm_arg(y(W)), asm_arg(y(FS0)), asm_arg(y(FS1)), asm_arg(3)],
                        );
                        let minmax = float_lane(
                            ty,
                            if *operation == V::Fmin {
                                [op::VMINPS, op::VMINPD]
                            } else {
                                [op::VMAXPS, op::VMAXPD]
                            },
                        );
                        self.w3(minmax, FS2, FS0, FS1);
                        self.w3(minmax, FS0, FS1, FS0);
                        self.w3(
                            if *operation == V::Fmin {
                                Opcode::Vorps
                            } else {
                                Opcode::Vandps
                            },
                            FS0,
                            FS0,
                            FS2,
                        );
                        if *operation == V::Fmin {
                            let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                            self.wk3(Opcode::Vandps, W, W, ty, 1u64 << (bits - 2));
                        }
                        self.w3(Opcode::Vorps, FS0, FS0, W);
                    }
                    V::Fcopysign => {
                        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                        let sign = 1u64 << (bits - 1);
                        self.wk3(Opcode::Vandps, FS0, FS0, ty, !sign);
                        self.wk3(Opcode::Vandps, FS1, FS1, ty, sign);
                        self.w3(Opcode::Vorps, FS0, FS0, FS1);
                    }
                    _ => self.w3(
                        float_lane(
                            ty,
                            match operation {
                                V::Fadd => [Opcode::Vaddps, Opcode::Vaddpd],
                                V::Fsub => [Opcode::Vsubps, Opcode::Vsubpd],
                                V::Fmul => [Opcode::Vmulps, Opcode::Vmulpd],
                                V::Fdiv => [Opcode::Vdivps, Opcode::Vdivpd],
                                _ => unreachable!(),
                            },
                        ),
                        FS0,
                        FS0,
                        FS1,
                    ),
                }
            }
            InstData::VectorIcmp { cond, args } => {
                let ty = self.ty(args[0]);
                self.wstage(args[0], FS0);
                self.wstage(args[1], FS1);
                self.wcompare(*cond, ty);
            }
            InstData::VectorFcmp { cond, args } => {
                let ty = self.ty(args[0]);
                self.wstage(args[0], FS0);
                self.wstage(args[1], FS1);
                let imm = match cond {
                    FloatCC::Eq => 0,
                    FloatCC::Ne => 4,
                    FloatCC::Lt => 1,
                    FloatCC::Le => 2,
                    FloatCC::Gt => 14,
                    FloatCC::Ge => 13,
                    FloatCC::Ord => 7,
                    FloatCC::Uno => 3,
                };
                self.emit(
                    float_lane(ty, [op::VCMPPS, op::VCMPPD]),
                    [
                        asm_arg(y(FS0)),
                        asm_arg(y(FS0)),
                        asm_arg(y(FS1)),
                        asm_arg(imm),
                    ],
                );
            }
            InstData::VectorSelect { args } => {
                let ty = self.ty(args[0]);
                self.wstage(args[0], FS0);
                self.wstage(args[1], FS1);
                self.wstage(args[2], FS2);
                self.w3(op::VPXOR, W, W, W);
                self.w3(VPCMPEQ[int_lane(ty)], FS0, FS0, W);
                self.w3(op::VPXOR, FS1, FS1, FS2);
                self.w3(op::VPANDN, FS0, FS0, FS1);
                self.w3(op::VPXOR, FS0, FS0, FS2);
            }
            InstData::VectorUnary { op: operation, arg } => {
                let ty = self.ty(*arg);
                self.wstage(*arg, FS0);
                let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                match operation {
                    UnaryOp::Ineg => {
                        self.w3(op::VPXOR, FS1, FS1, FS1);
                        self.w3(VPSUB[int_lane(ty)], FS0, FS1, FS0);
                    }
                    UnaryOp::Bnot => {
                        self.w3(op::VPCMPEQD, FS1, FS1, FS1);
                        self.w3(op::VPXOR, FS0, FS0, FS1);
                    }
                    UnaryOp::Bswap => {
                        let width = (bits / 8) as usize;
                        let m = self.wc(std::array::from_fn(|i| {
                            (i / width * width + width - 1 - i % width) % 16
                        })
                        .map(|b| b as u8));
                        self.emit(op::VPSHUFB, [asm_arg(y(FS0)), asm_arg(y(FS0)), asm_arg(m)]);
                    }
                    UnaryOp::Popcnt => self.wpopcount(ty),
                    UnaryOp::Clz => {
                        for bit in 0..bits.trailing_zeros() {
                            self.wshift_imm(V::Ushr, ty, W, FS0, 1 << bit);
                            self.w3(op::VPOR, FS0, FS0, W);
                        }
                        self.wpopcount(ty);
                        let m = self.wk(ty, bits.into());
                        self.emit(Opcode::Vmovups, [asm_arg(y(FS1)), asm_arg(m)]);
                        self.w3(VPSUB[int_lane(ty)], FS0, FS1, FS0);
                    }
                    UnaryOp::Ctz => {
                        let one = self.wk(ty, 1);
                        self.emit(
                            VPSUB[int_lane(ty)],
                            [asm_arg(y(FS1)), asm_arg(y(FS0)), asm_arg(one)],
                        );
                        self.w3(op::VPCMPEQD, FS2, FS2, FS2);
                        self.w3(op::VPXOR, FS0, FS0, FS2);
                        self.w3(op::VPAND, FS0, FS0, FS1);
                        self.wpopcount(ty);
                    }
                    UnaryOp::Fneg | UnaryOp::Fabs => self.wk3(
                        if *operation == UnaryOp::Fneg {
                            Opcode::Vxorps
                        } else {
                            Opcode::Vandps
                        },
                        FS0,
                        FS0,
                        ty,
                        if *operation == UnaryOp::Fneg {
                            1u64 << (bits - 1)
                        } else {
                            !(1u64 << (bits - 1))
                        },
                    ),
                    UnaryOp::Fsqrt => self.emit(
                        float_lane(ty, [op::VSQRTPS, op::VSQRTPD]),
                        [asm_arg(y(FS0)), asm_arg(y(FS0))],
                    ),
                    _ => self.wi(
                        float_lane(ty, [op::VROUNDPS, op::VROUNDPD]),
                        FS0,
                        FS0,
                        match operation {
                            UnaryOp::Fnearest => 8,
                            UnaryOp::Ffloor => 9,
                            UnaryOp::Fceil => 10,
                            UnaryOp::Ftrunc => 11,
                            _ => unreachable!(),
                        },
                    ),
                }
            }
            InstData::VectorShuffle { args, lanes } => {
                self.wshuffle(self.ty(args[0]), *args, lanes)
            }
            InstData::VectorSplat { ty, arg } => {
                let lane = ty.vector().unwrap().lane_type();
                let r = if lane.is_float() {
                    let r = self.use_freg(*arg, FS1);
                    self.emit(
                        if lane == Type::F32 {
                            Opcode::Movd
                        } else {
                            Opcode::Movq
                        },
                        [
                            asm_arg(if lane == Type::F32 { r32(S0) } else { r64(S0) }),
                            asm_arg(xmm(r)),
                        ],
                    );
                    S0
                } else {
                    self.use_reg(*arg, S0)
                };
                self.emit(
                    if lane.bytes() == Some(8) {
                        Opcode::Movq
                    } else {
                        Opcode::Movd
                    },
                    [
                        asm_arg(xmm(FS0)),
                        asm_arg(if lane.bytes() == Some(8) {
                            r64(r)
                        } else {
                            r32(r)
                        }),
                    ],
                );
                if self.features.avx2 {
                    self.emit(
                        VPBROADCAST[size_lane(lane.bytes().unwrap())],
                        [asm_arg(y(FS0)), asm_arg(xmm(FS0))],
                    );
                } else {
                    match lane.bytes().unwrap() {
                        1 => {
                            self.emit(Opcode::Punpcklbw, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]);
                            self.emit(Opcode::Punpcklwd, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]);
                        }
                        2 => self.emit(Opcode::Punpcklwd, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]),
                        _ => {}
                    }
                    self.emit(
                        Opcode::Pshufd,
                        [
                            asm_arg(xmm(FS0)),
                            asm_arg(xmm(FS0)),
                            asm_arg(if lane.bytes() == Some(8) { 0x44 } else { 0 }),
                        ],
                    );
                    self.emit(
                        op::VINSERTF128,
                        [
                            asm_arg(y(FS0)),
                            asm_arg(y(FS0)),
                            asm_arg(xmm(FS0)),
                            asm_arg(1),
                        ],
                    );
                }
            }
            InstData::VectorExtract { arg, lane } => {
                let ty = self.ty(*arg);
                self.wstage(*arg, FS0);
                let bytes = ty.vector().unwrap().lane_type().bytes().unwrap() as u8;
                let n = 16 / bytes;
                self.emit(
                    op::VEXTRACTF128,
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(lane / n)],
                );
                self.emit(
                    VPEXTR[size_lane(bytes.into())],
                    [
                        asm_arg(if bytes == 8 { r64(S0) } else { r32(S0) }),
                        asm_arg(xmm(FS1)),
                        asm_arg(lane % n),
                    ],
                );
                let scalar = ty.vector().unwrap().lane_type();
                if scalar.is_float() {
                    let d = self.def_freg(result);
                    self.emit(
                        if bytes == 8 {
                            Opcode::Movq
                        } else {
                            Opcode::Movd
                        },
                        [
                            asm_arg(xmm(d)),
                            asm_arg(if bytes == 8 { r64(S0) } else { r32(S0) }),
                        ],
                    );
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    self.emit(
                        Opcode::Mov,
                        [asm_arg(reg(d, scalar)), asm_arg(reg(S0, scalar))],
                    );
                    self.def_done(result, d);
                }
                return true;
            }
            InstData::VectorInsert { args, lane } => {
                let ty = self.ty(args[0]);
                self.wstage(args[0], FS0);
                let scalar = ty.vector().unwrap().lane_type();
                let bytes = scalar.bytes().unwrap() as u8;
                let n = 16 / bytes;
                let r = if scalar.is_float() {
                    let r = self.use_freg(args[1], FS1);
                    self.emit(
                        if bytes == 8 {
                            Opcode::Movq
                        } else {
                            Opcode::Movd
                        },
                        [
                            asm_arg(if bytes == 8 { r64(S0) } else { r32(S0) }),
                            asm_arg(xmm(r)),
                        ],
                    );
                    S0
                } else {
                    self.use_reg(args[1], S0)
                };
                self.emit(
                    op::VEXTRACTF128,
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(lane / n)],
                );
                self.emit(
                    VPINSR[size_lane(bytes.into())],
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(xmm(FS1)),
                        asm_arg(if bytes == 8 { r64(r) } else { r32(r) }),
                        asm_arg(lane % n),
                    ],
                );
                self.emit(
                    op::VINSERTF128,
                    [
                        asm_arg(y(FS0)),
                        asm_arg(y(FS0)),
                        asm_arg(xmm(FS1)),
                        asm_arg(lane / n),
                    ],
                );
            }
            InstData::Convert {
                op: ConvertOp::Bitcast,
                arg,
                ..
            } => self.wstage(*arg, FS0),
            InstData::VectorReduce { op: operation, arg } => {
                let ty = self.ty(*arg);
                self.wstage(*arg, FS0);
                self.emit(
                    op::VPERM2I128,
                    [
                        asm_arg(y(FS1)),
                        asm_arg(y(FS0)),
                        asm_arg(y(FS0)),
                        asm_arg(1),
                    ],
                );
                self.wreduce_pair(*operation, ty);
                let bytes = ty.vector().unwrap().lane_type().bytes().unwrap();
                let mut shift = 8;
                while shift >= bytes {
                    self.wi(op::VPSRLDQ, FS1, FS0, shift.into());
                    self.wreduce_pair(*operation, ty);
                    shift /= 2;
                }
                let d = self.def_reg(result);
                self.emit(
                    VPEXTR[size_lane(bytes)],
                    [
                        asm_arg(if bytes == 8 { r64(d) } else { r32(d) }),
                        asm_arg(xmm(FS0)),
                        asm_arg(0),
                    ],
                );
                self.def_done(result, d);
                return true;
            }
            InstData::Convert {
                op: ConvertOp::NarrowLow,
                ty,
                arg,
            } => {
                let source = self.ty(*arg).vector().unwrap();
                let target = ty.vector().unwrap();
                let r = self.use_freg(*arg, FS0);
                // VEX128 copy clears the unavailable upper half for a Q input.
                self.emit(
                    Opcode::Vmovups,
                    [
                        asm_arg(simd_width(FS0, (source.bytes() * 8) as u16)),
                        asm_arg(simd_width(r, (source.bytes() * 8) as u16)),
                    ],
                );
                let a = source.lane_type().bytes().unwrap() as usize;
                let b = target.lane_type().bytes().unwrap() as usize;
                let half = 16 / a * b;
                let mask = self.wc(std::array::from_fn(|i| {
                    if i % 16 < half {
                        ((i % 16 / b) * a + i % b) as u8
                    } else {
                        128
                    }
                }));
                self.emit(Opcode::Vmovups, [asm_arg(y(W)), asm_arg(mask)]);
                self.w3(op::VPSHUFB, FS0, FS0, W);
                self.emit(
                    op::VEXTRACTI128,
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(1)],
                );
                self.emit(
                    op::VPSLLDQ,
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(half)],
                );
                self.emit(
                    op::VPOR,
                    [asm_arg(xmm(FS0)), asm_arg(xmm(FS0)), asm_arg(xmm(FS1))],
                );
            }
            InstData::Convert {
                op: operation,
                ty,
                arg,
            } => {
                let source = self.ty(*arg).vector().unwrap();
                let target = ty.vector().unwrap();
                let a = source.lane_type().bytes().unwrap();
                let b = target.lane_type().bytes().unwrap();
                let r = self.use_freg(*arg, FS0);
                let high = matches!(operation, ConvertOp::SwidenHigh | ConvertOp::UwidenHigh);
                let offset = if high {
                    source.bytes() - u32::from(target.lanes()) * a
                } else {
                    0
                };
                if source.bytes() == 32 {
                    self.emit(
                        op::VEXTRACTI128,
                        [asm_arg(xmm(FS1)), asm_arg(y(r)), asm_arg(offset / 16)],
                    );
                } else {
                    self.emit(Opcode::Vmovups, [asm_arg(xmm(FS1)), asm_arg(xmm(r))]);
                }
                if offset % 16 != 0 {
                    self.emit(
                        op::VPSRLDQ,
                        [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(offset % 16)],
                    );
                }
                let signed = matches!(operation, ConvertOp::SwidenLow | ConvertOp::SwidenHigh);
                let opcode = match (signed, a, b) {
                    (true, 1, 2) => op::VPMOVSXBW,
                    (true, 1, 4) => op::VPMOVSXBD,
                    (true, 1, 8) => op::VPMOVSXBQ,
                    (true, 2, 4) => op::VPMOVSXWD,
                    (true, 2, 8) => op::VPMOVSXWQ,
                    (true, 4, 8) => op::VPMOVSXDQ,
                    (false, 1, 2) => op::VPMOVZXBW,
                    (false, 1, 4) => op::VPMOVZXBD,
                    (false, 1, 8) => op::VPMOVZXBQ,
                    (false, 2, 4) => op::VPMOVZXWD,
                    (false, 2, 8) => op::VPMOVZXWQ,
                    (false, 4, 8) => op::VPMOVZXDQ,
                    _ => unreachable!("widening needs a wider target lane"),
                };
                self.emit(opcode, [asm_arg(simd(FS0, *ty)), asm_arg(xmm(FS1))]);
            }
            _ => return false,
        }
        self.wdone(result);
        true
    }
}

#[cfg(test)]
mod shuffle_tests {
    use super::*;

    #[test]
    fn all_dword_controls_and_rejected_boundaries() {
        let i32x8 = Type::Vector(VectorType::I32x8);
        for imm in 0..=255u8 {
            let lanes: [u8; 8] =
                std::array::from_fn(|i| ((imm >> (2 * (i % 4))) & 3) + (i / 4 * 4) as u8);
            for ty in [i32x8, Type::Vector(VectorType::F32x8)] {
                // Every spelling using either equal concatenated source is valid.
                for sources in 0..256 {
                    let mask: [u8; 8] = std::array::from_fn(|i| {
                        lanes[i] + if sources & (1 << i) != 0 { 8 } else { 0 }
                    });
                    assert_eq!(dword_shuffle_control(ty, true, &mask), Some(imm));
                    assert_eq!(dword_shuffle_control(ty, false, &mask), None);
                }
            }
            for i in 0..8 {
                for replacement in 0..=16 {
                    let mut mask = lanes;
                    mask[i] = replacement;
                    if replacement < 16 && replacement % 8 == lanes[i] {
                        continue;
                    }
                    assert_eq!(dword_shuffle_control(i32x8, true, &mask), None);
                }
            }
            assert_eq!(
                dword_shuffle_control(Type::Vector(VectorType::I64x4), true, &lanes),
                None
            );
            assert_eq!(dword_shuffle_control(i32x8, true, &lanes[..7]), None);
        }
    }
}
