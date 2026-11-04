//! Lowering of the legalized vector instructions that no cover claimed.
use super::*;

impl FunctionCompiler<'_> {
    /// Vector compares, selects, arithmetic, shuffles, lanes and reductions.
    pub(super) fn vector_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::VectorIcmp { cond, args } => self.vector_compare(cond, args, result),
            InstData::VectorSelect { args } => self.vector_select(args, result),
            InstData::VectorBinary { op, args } => {
                if op == keel_ir::VectorBinaryOp::Imul
                    && self.ty(args[0]) == Type::Vector(keel_ir::VectorType::I32x4)
                    && !self.features.sse41
                {
                    self.vector_multiply_dwords_sse2(args, result);
                    return;
                }
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                if let Some(opcode) = self
                    .features
                    .avx
                    .then(|| vector::packed_avx(op, self.ty(args[0]).vector().unwrap()))
                    .flatten()
                {
                    self.emit(opcode, [asm_arg(xmm(d)), asm_arg(xmm(a)), asm_arg(xmm(b))]);
                    self.def_done(result, d);
                    return;
                }
                // Preserve the second source if writing d would overwrite it.
                let dst = if d == b && a != d { FS2 } else { d };
                if dst != a {
                    self.emit(Opcode::Movaps, [asm_arg(xmm(dst)), asm_arg(xmm(a))]);
                }
                self.emit(
                    vector::packed(op, self.ty(args[0]).vector().unwrap(), self.features)
                        .expect("legalized packed op"),
                    [asm_arg(xmm(dst)), asm_arg(xmm(b))],
                );
                if dst != d {
                    self.emit(Opcode::Movaps, [asm_arg(xmm(d)), asm_arg(xmm(dst))]);
                }
                self.def_done(result, d);
            }
            InstData::VectorUnary {
                op: UnaryOp::Fsqrt,
                arg,
            } => {
                let d = self.def_freg(result);
                let a = self.use_freg(arg, FS1);
                let op = if self.ty(arg).vector().unwrap().lane_type() == Type::F32 {
                    Opcode::Sqrtps
                } else {
                    Opcode::Sqrtpd
                };
                self.emit(op, [asm_arg(xmm(d)), asm_arg(xmm(a))]);
                self.def_done(result, d);
            }
            InstData::VectorUnary {
                op: UnaryOp::Ineg,
                arg,
            } => self.vector_ineg(arg, result),
            InstData::VectorShuffle { args, ref lanes } => self.vector_shuffle(args, lanes, result),
            InstData::VectorExtract { arg, lane } => {
                let source = self.use_freg(arg, FS0);
                let d = self.def_reg(result);
                let source = if lane == 0 {
                    source
                } else {
                    // Use reserved scratch; the input may still be live.
                    self.emit(Opcode::Movhlps, [asm_arg(xmm(FS1)), asm_arg(xmm(source))]);
                    FS1
                };
                self.emit(Opcode::Movq, [asm_arg(r64(d)), asm_arg(xmm(source))]);
                self.def_done(result, d);
            }
            InstData::VectorSplat { ty, arg } => self.vector_splat(ty, arg, result),
            InstData::VectorReduce {
                op: keel_ir::VectorReduceOp::Add,
                arg,
            } => self.vector_reduce_add(arg, result),
            InstData::VectorUnary { .. }
            | InstData::VectorReduce { .. }
            | InstData::VectorInsert { .. }
            | InstData::VectorFcmp { .. } => unreachable!("legalized vector operation"),
            _ => unreachable!("not a vector instruction"),
        }
    }
}
