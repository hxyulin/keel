//! Wrapping integer reductions use full-width shuffle trees with AVX512F only.
use super::*;
use keel_ir::{VectorReduceOp as R, VectorType};

pub(super) fn keep(func: &Function, data: &InstData, features: keel_asm_ir::x86::Features) -> bool {
    features.avx512f
        && matches!(data, InstData::VectorReduce {arg,..} if matches!(func.value_type(func.resolve(*arg)),Type::Vector(VectorType::I32x16|VectorType::I64x8)))
}
fn opcode(mnemonic: EvexMnemonic) -> Opcode {
    Opcode::Evex(EvexOpcode::new(mnemonic))
}
impl FunctionCompiler<'_> {
    pub(super) fn avx512_reduce(&mut self, data: &InstData, result: Option<Value>) -> bool {
        if !keep(self.func, data, self.features) {
            return false;
        }
        let InstData::VectorReduce { op, arg } = *data else {
            unreachable!("retained AVX512 reduction")
        };
        let q = self.ty(arg) == Type::Vector(VectorType::I64x8);
        let name = match (op, q) {
            (R::Add, false) => EvexMnemonic::Vpaddd,
            (R::Add, true) => EvexMnemonic::Vpaddq,
            (R::And, _) => EvexMnemonic::Vpandd,
            (R::Or, _) => EvexMnemonic::Vpord,
            (R::Xor, _) => EvexMnemonic::Vpxord,
            (R::Smin, false) => EvexMnemonic::Vpminsd,
            (R::Smin, true) => EvexMnemonic::Vpminsq,
            (R::Smax, false) => EvexMnemonic::Vpmaxsd,
            (R::Smax, true) => EvexMnemonic::Vpmaxsq,
            (R::Umin, false) => EvexMnemonic::Vpminud,
            (R::Umin, true) => EvexMnemonic::Vpminuq,
            (R::Umax, false) => EvexMnemonic::Vpmaxud,
            (R::Umax, true) => EvexMnemonic::Vpmaxuq,
        };
        let a = self.use_freg(arg, FS0);
        if a != FS0 {
            self.emit(
                opcode(EvexMnemonic::Vmovups),
                [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(a, 512))],
            );
        }
        // Fold opposite 256-bit halves, then adjacent 128-bit quarters. Every
        // quarter now contains the corresponding lanes from the whole input.
        for control in [0x4e, 0xb1] {
            self.emit(
                opcode(EvexMnemonic::Vshufi32x4),
                [
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(control),
                ],
            );
            self.emit(
                opcode(name),
                [
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                ],
            );
        }
        // Within each quarter, swap qwords, then dwords when needed. Packed
        // addition truncates at each step, preserving the IR's wrapping sum.
        for control in [0x4e, 0xb1].into_iter().take(if q { 1 } else { 2 }) {
            self.emit(
                opcode(EvexMnemonic::Vpshufd),
                [
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(control),
                ],
            );
            self.emit(
                opcode(name),
                [
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS0, 512)),
                    asm_arg(simd_width(FS1, 512)),
                ],
            );
        }
        let d = self.def_reg(result);
        self.emit(
            if q { Opcode::Movq } else { Opcode::Movd },
            [asm_arg(if q { r64(d) } else { r32(d) }), asm_arg(xmm(FS0))],
        );
        self.def_done(result, d);
        true
    }
}
