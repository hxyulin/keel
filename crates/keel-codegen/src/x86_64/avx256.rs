//! Packed 256-bit selection. Inputs are captured in reserved physical SIMD
//! indices before any result write. SIMD indices12..15 are reserved in wide
//! functions, and alias the XMM views used for scalar/lane transfers.
use super::*;
use keel_ir::{FloatCC, VectorBinaryOp as V, VectorType};
const W: u8 = 12;
fn op(s: &str) -> Opcode {
    Opcode::parse(s).expect("inventoried AVX opcode")
}
fn y(n: u8) -> Register {
    simd_width(n, 256)
}
fn suffix(ty: Type) -> &'static str {
    match ty.vector().unwrap().lane_type() {
        Type::I8 => "b",
        Type::I16 => "w",
        Type::I32 => "d",
        Type::I64 => "q",
        Type::F32 => "ps",
        Type::F64 => "pd",
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
    fn w3(&mut self, name: &str, d: u8, a: u8, b: u8) {
        self.emit(op(name), [asm_arg(y(d)), asm_arg(y(a)), asm_arg(y(b))]);
    }
    fn wi(&mut self, name: &str, d: u8, a: u8, imm: i64) {
        self.emit(op(name), [asm_arg(y(d)), asm_arg(y(a)), asm_arg(imm)]);
    }
    /// Parallel-copy temporary is dead between IR instructions and has32 bytes.
    /// Constants use memory operands, leaving all four SIMD scratch views usable.
    fn wc(&mut self, bytes: [u8; 32]) -> Memory {
        let at = self.copy_slot(self.copy_temp.expect("wide SIMD temporary"));
        for (i, b) in bytes.chunks_exact(8).enumerate() {
            let n = u64::from_le_bytes(b.try_into().unwrap());
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
    fn wk3(&mut self, name: &str, d: u8, a: u8, ty: Type, k: u64) {
        let m = self.wk(ty, k);
        self.emit(op(name), [asm_arg(y(d)), asm_arg(y(a)), asm_arg(m)]);
    }
    fn wshift_imm(&mut self, op: V, ty: Type, d: u8, a: u8, n: u8) {
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        let s = if bits == 8 { "w" } else { suffix(ty) };
        self.wi(
            &format!(
                "vp{}{}",
                match op {
                    V::Shl => "sll",
                    _ => "srl",
                },
                s
            ),
            d,
            a,
            n.into(),
        );
        if bits == 8 {
            self.wk3(
                "vpand",
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
            self.emit(op("vpxor"), [asm_arg(y(FS0)), asm_arg(y(FS0)), asm_arg(&m)]);
            self.emit(op("vpxor"), [asm_arg(y(FS1)), asm_arg(y(FS1)), asm_arg(m)]);
        }
        self.w3(
            &format!(
                "vpcmp{}{}",
                if matches!(cond, IntCC::Eq | IntCC::Ne) {
                    "eq"
                } else {
                    "gt"
                },
                suffix(ty)
            ),
            FS0,
            if swap { FS1 } else { FS0 },
            if swap { FS0 } else { FS1 },
        );
        if invert {
            self.w3("vpcmpeqd", W, W, W);
            self.w3("vpxor", FS0, FS0, W);
        }
    }
    fn wmul(&mut self, ty: Type) {
        match ty.vector().unwrap().lane_type() {
            Type::I16 => self.w3("vpmullw", FS0, FS0, FS1),
            Type::I32 => self.w3("vpmulld", FS0, FS0, FS1),
            Type::I8 => {
                self.w3("vpmullw", FS2, FS0, FS1);
                self.wi("vpsrlw", FS0, FS0, 8);
                self.wi("vpsrlw", FS1, FS1, 8);
                self.w3("vpmullw", FS0, FS0, FS1);
                self.wi("vpsllw", FS0, FS0, 8);
                self.wk3("vpand", FS2, FS2, Type::Vector(VectorType::I16x16), 255);
                self.w3("vpor", FS0, FS0, FS2);
            }
            Type::I64 => {
                self.w3("vpmuludq", FS2, FS0, FS1);
                self.wi("vpshufd", FS0, FS0, 0xb1);
                self.w3("vpmulld", FS0, FS0, FS1);
                self.wi("vpshufd", FS1, FS0, 0xb1);
                self.w3("vpaddd", FS0, FS0, FS1);
                self.wi("vpsllq", FS0, FS0, 32);
                self.w3("vpaddq", FS0, FS0, FS2);
            }
            _ => unreachable!(),
        }
    }
    fn wshift(&mut self, operation: V, ty: Type, counts: Value) {
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        if bits >= 32 {
            self.wk3("vpand", FS1, FS1, ty, (bits - 1) as u64);
            if bits == 64 && operation == V::Sshr {
                self.w3("vpxor", FS2, FS2, FS2);
                self.w3("vpcmpgtq", FS2, FS2, FS0);
                self.w3("vpxor", FS0, FS0, FS2);
                self.w3("vpsrlvq", FS0, FS0, FS1);
                self.w3("vpxor", FS0, FS0, FS2);
            } else {
                self.w3(
                    &format!(
                        "vp{}v{}",
                        match operation {
                            V::Shl => "sll",
                            V::Ushr => "srl",
                            _ => "sra",
                        },
                        suffix(ty)
                    ),
                    FS0,
                    FS0,
                    FS1,
                );
            }
            return;
        }
        for bit in 0..bits.trailing_zeros() {
            let shift = 1u8 << bit;
            self.wstage(counts, FS2);
            let m = self.wk(ty, 1 << bit);
            self.emit(op("vpand"), [asm_arg(y(FS2)), asm_arg(y(FS2)), asm_arg(&m)]);
            self.emit(
                op(&format!("vpcmpeq{}", suffix(ty))),
                [asm_arg(y(FS2)), asm_arg(y(FS2)), asm_arg(m)],
            );
            if operation == V::Sshr && bits == 16 {
                self.wi("vpsraw", W, FS0, shift.into());
            } else {
                self.wshift_imm(
                    if operation == V::Shl { V::Shl } else { V::Ushr },
                    ty,
                    W,
                    FS0,
                    shift,
                );
                if operation == V::Sshr {
                    self.w3("vpxor", FS1, FS1, FS1);
                    self.w3("vpcmpgtb", FS1, FS1, FS0);
                    self.wk3("vpand", FS1, FS1, ty, (255u64 << (8 - shift)) & 255);
                    self.w3("vpor", W, W, FS1);
                }
            }
            self.w3("vpxor", W, W, FS0);
            self.w3("vpand", W, W, FS2);
            self.w3("vpxor", FS0, FS0, W);
        }
    }
    fn wpopcount(&mut self, ty: Type) {
        let byte = Type::Vector(VectorType::I8x32);
        self.wk3("vpand", FS1, FS0, byte, 15);
        self.wi("vpsrlw", FS2, FS0, 4);
        self.wk3("vpand", FS2, FS2, byte, 15);
        let lookup = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4];
        let m = self.wc(std::array::from_fn(|i| lookup[i % 16]));
        self.emit(Opcode::Vmovups, [asm_arg(y(W)), asm_arg(m)]);
        self.w3("vpshufb", FS1, W, FS1);
        self.w3("vpshufb", FS2, W, FS2);
        self.w3("vpaddb", FS0, FS1, FS2);
        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
        if bits >= 16 {
            self.wk3("vpmaddubsw", FS0, FS0, byte, 1);
        }
        if bits >= 32 {
            self.wk3("vpmaddwd", FS0, FS0, Type::Vector(VectorType::I16x16), 1);
        }
        if bits == 64 {
            self.wi("vpsrlq", FS1, FS0, 32);
            self.w3("vpaddq", FS0, FS0, FS1);
            self.wk3("vpand", FS0, FS0, ty, u32::MAX as u64);
        }
    }
    fn wshuffle(&mut self, ty: Type, args: [Value; 2], lanes: &[u8]) {
        self.wstage(args[0], FS1);
        self.wstage(args[1], FS2);
        self.w3("vxorps", FS0, FS0, FS0);
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
                    op("vperm2i128"),
                    [
                        asm_arg(y(W)),
                        asm_arg(y(if input == 0 { FS1 } else { FS2 })),
                        asm_arg(y(if input == 0 { FS1 } else { FS2 })),
                        asm_arg(if half == 0 { 0 } else { 0x11 }),
                    ],
                );
                let m = self.wc(mask);
                self.emit(op("vpshufb"), [asm_arg(y(W)), asm_arg(y(W)), asm_arg(m)]);
                self.w3("vpor", FS0, FS0, W);
            }
        }
    }
    fn wreduce_pair(&mut self, operation: keel_ir::VectorReduceOp, ty: Type) {
        use keel_ir::VectorReduceOp as R;
        let lane = ty.vector().unwrap().lane_type();
        let name = match operation {
            R::Add => format!("vpadd{}", suffix(ty)),
            R::And => "vpand".into(),
            R::Or => "vpor".into(),
            R::Xor => "vpxor".into(),
            _ => String::new(),
        };
        if !name.is_empty() {
            self.w3(&name, FS0, FS0, FS1);
            return;
        }
        let signed = matches!(operation, R::Smin | R::Smax);
        let min = matches!(operation, R::Smin | R::Umin);
        if lane != Type::I64 {
            self.w3(
                &format!(
                    "vp{}{}{}",
                    if min { "min" } else { "max" },
                    if signed { "s" } else { "u" },
                    suffix(ty)
                ),
                FS0,
                FS0,
                FS1,
            );
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
            self.w3("vpand", FS2, FS2, FS0);
            self.w3("vpandn", FS0, FS0, W);
            self.w3("vpor", FS0, FS0, FS2);
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
                self.wstage(args[0], FS0);
                self.wstage(args[1], FS1);
                match operation {
                    V::And | V::Or | V::Xor => self.w3(
                        match operation {
                            V::And => "vandps",
                            V::Or => "vorps",
                            _ => "vxorps",
                        },
                        FS0,
                        FS0,
                        FS1,
                    ),
                    V::Iadd | V::Isub => self.w3(
                        &format!(
                            "vp{}{}",
                            if *operation == V::Iadd { "add" } else { "sub" },
                            suffix(ty)
                        ),
                        FS0,
                        FS0,
                        FS1,
                    ),
                    V::Imul => self.wmul(ty),
                    V::Shl | V::Ushr | V::Sshr => self.wshift(*operation, ty, args[1]),
                    V::Fmin | V::Fmax => {
                        let s = suffix(ty);
                        {
                            self.emit(
                                op(&format!("vcmp{s}")),
                                [asm_arg(y(W)), asm_arg(y(FS0)), asm_arg(y(FS1)), asm_arg(3)],
                            );
                        }
                        self.w3(
                            &format!(
                                "v{}{}",
                                if *operation == V::Fmin { "min" } else { "max" },
                                s
                            ),
                            FS2,
                            FS0,
                            FS1,
                        );
                        self.w3(
                            &format!(
                                "v{}{}",
                                if *operation == V::Fmin { "min" } else { "max" },
                                s
                            ),
                            FS0,
                            FS1,
                            FS0,
                        );
                        self.w3(
                            if *operation == V::Fmin {
                                "vorps"
                            } else {
                                "vandps"
                            },
                            FS0,
                            FS0,
                            FS2,
                        );
                        if *operation == V::Fmin {
                            let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                            self.wk3("vandps", W, W, ty, 1u64 << (bits - 2));
                        }
                        self.w3("vorps", FS0, FS0, W);
                    }
                    V::Fcopysign => {
                        let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                        let sign = 1u64 << (bits - 1);
                        self.wk3("vandps", FS0, FS0, ty, !sign);
                        self.wk3("vandps", FS1, FS1, ty, sign);
                        self.w3("vorps", FS0, FS0, FS1);
                    }
                    _ => self.w3(
                        &format!(
                            "v{}{}",
                            match operation {
                                V::Fadd => "add",
                                V::Fsub => "sub",
                                V::Fmul => "mul",
                                V::Fdiv => "div",
                                _ => unreachable!(),
                            },
                            suffix(ty)
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
                    op(&format!("vcmp{}", suffix(ty))),
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
                self.w3("vpxor", W, W, W);
                self.w3(&format!("vpcmpeq{}", suffix(ty)), FS0, FS0, W);
                self.w3("vpxor", FS1, FS1, FS2);
                self.w3("vpandn", FS0, FS0, FS1);
                self.w3("vpxor", FS0, FS0, FS2);
            }
            InstData::VectorUnary { op: operation, arg } => {
                let ty = self.ty(*arg);
                self.wstage(*arg, FS0);
                let bits = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
                match operation {
                    UnaryOp::Ineg => {
                        self.w3("vpxor", FS1, FS1, FS1);
                        self.w3(&format!("vpsub{}", suffix(ty)), FS0, FS1, FS0);
                    }
                    UnaryOp::Bnot => {
                        self.w3("vpcmpeqd", FS1, FS1, FS1);
                        self.w3("vpxor", FS0, FS0, FS1);
                    }
                    UnaryOp::Bswap => {
                        let width = (bits / 8) as usize;
                        let m = self.wc(std::array::from_fn(|i| {
                            (i / width * width + width - 1 - i % width) % 16
                        })
                        .map(|b| b as u8));
                        self.emit(
                            op("vpshufb"),
                            [asm_arg(y(FS0)), asm_arg(y(FS0)), asm_arg(m)],
                        );
                    }
                    UnaryOp::Popcnt => self.wpopcount(ty),
                    UnaryOp::Clz => {
                        for bit in 0..bits.trailing_zeros() {
                            self.wshift_imm(V::Ushr, ty, W, FS0, 1 << bit);
                            self.w3("vpor", FS0, FS0, W);
                        }
                        self.wpopcount(ty);
                        let m = self.wk(ty, bits.into());
                        self.emit(Opcode::Vmovups, [asm_arg(y(FS1)), asm_arg(m)]);
                        self.w3(&format!("vpsub{}", suffix(ty)), FS0, FS1, FS0);
                    }
                    UnaryOp::Ctz => {
                        let one = self.wk(ty, 1);
                        self.emit(
                            op(&format!("vpsub{}", suffix(ty))),
                            [asm_arg(y(FS1)), asm_arg(y(FS0)), asm_arg(one)],
                        );
                        self.w3("vpcmpeqd", FS2, FS2, FS2);
                        self.w3("vpxor", FS0, FS0, FS2);
                        self.w3("vpand", FS0, FS0, FS1);
                        self.wpopcount(ty);
                    }
                    UnaryOp::Fneg | UnaryOp::Fabs => self.wk3(
                        if *operation == UnaryOp::Fneg {
                            "vxorps"
                        } else {
                            "vandps"
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
                        op(&format!("vsqrt{}", suffix(ty))),
                        [asm_arg(y(FS0)), asm_arg(y(FS0))],
                    ),
                    _ => self.wi(
                        &format!("vround{}", suffix(ty)),
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
                        op(&format!(
                            "vpbroadcast{}",
                            match lane.bytes().unwrap() {
                                1 => "b",
                                2 => "w",
                                4 => "d",
                                _ => "q",
                            }
                        )),
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
                        op("vinsertf128"),
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
                    op("vextractf128"),
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(lane / n)],
                );
                self.emit(
                    op(&format!(
                        "vpextr{}",
                        match bytes {
                            1 => "b",
                            2 => "w",
                            4 => "d",
                            _ => "q",
                        }
                    )),
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
                    op("vextractf128"),
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(lane / n)],
                );
                self.emit(
                    op(&format!(
                        "vpinsr{}",
                        match bytes {
                            1 => "b",
                            2 => "w",
                            4 => "d",
                            _ => "q",
                        }
                    )),
                    [
                        asm_arg(xmm(FS1)),
                        asm_arg(xmm(FS1)),
                        asm_arg(if bytes == 8 { r64(r) } else { r32(r) }),
                        asm_arg(lane % n),
                    ],
                );
                self.emit(
                    op("vinsertf128"),
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
                    op("vperm2i128"),
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
                    self.wi("vpsrldq", FS1, FS0, shift.into());
                    self.wreduce_pair(*operation, ty);
                    shift /= 2;
                }
                let d = self.def_reg(result);
                self.emit(
                    op(&format!(
                        "vpextr{}",
                        match bytes {
                            1 => "b",
                            2 => "w",
                            4 => "d",
                            _ => "q",
                        }
                    )),
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
                self.w3("vpshufb", FS0, FS0, W);
                self.emit(
                    op("vextracti128"),
                    [asm_arg(xmm(FS1)), asm_arg(y(FS0)), asm_arg(1)],
                );
                self.emit(
                    op("vpslldq"),
                    [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(half)],
                );
                self.emit(
                    op("vpor"),
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
                        op("vextracti128"),
                        [asm_arg(xmm(FS1)), asm_arg(y(r)), asm_arg(offset / 16)],
                    );
                } else {
                    self.emit(Opcode::Vmovups, [asm_arg(xmm(FS1)), asm_arg(xmm(r))]);
                }
                if offset % 16 != 0 {
                    self.emit(
                        op("vpsrldq"),
                        [asm_arg(xmm(FS1)), asm_arg(xmm(FS1)), asm_arg(offset % 16)],
                    );
                }
                self.emit(
                    op(&format!(
                        "vpmov{}{}{}",
                        if matches!(operation, ConvertOp::SwidenLow | ConvertOp::SwidenHigh) {
                            "sx"
                        } else {
                            "zx"
                        },
                        match a {
                            1 => "b",
                            2 => "w",
                            _ => "d",
                        },
                        match b {
                            2 => "w",
                            4 => "d",
                            _ => "q",
                        }
                    )),
                    [asm_arg(simd(FS0, *ty)), asm_arg(xmm(FS1))],
                );
            }
            _ => return false,
        }
        self.wdone(result);
        true
    }
}
