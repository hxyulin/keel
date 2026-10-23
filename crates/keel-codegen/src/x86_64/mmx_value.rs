//! Immutable MMX-sized numeric values, lowered with strict-v1 SSE2 only.
//!
//! I64 holds raw little-endian two-i32 bits. MXCSR rounding, DAZ, sticky
//! exceptions and masked-invalid integer indefinite results are architectural;
//! these operations remain pinned even when unused. There is NO physical MMX
//! or x87 register/tag-state side effect. CVTPI2PS's first argument supplies
//! raw upper64 merge bits, never floating-point observations of those lanes.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn native_mmx_value_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        let to_ps = id == X86SseMmxvalueCvtpi2ps;
        let from_pi = to_ps || id == X86SseMmxvalueCvtpi2pd;
        if from_pi {
            // Capture both inputs before any result publication. Reserved SIMD
            // scratch cannot alias allocated inputs, including spilled inputs.
            if to_ps {
                let r = self.use_freg(args[0], FS1);
                self.emit(Opcode::Movups, [asm_arg(xmm(FS1)), asm_arg(xmm(r))]);
            }
            let r = self.use_reg(args[usize::from(to_ps)], S0);
            // MOVQ from GPR zeroes high64: extra CVTDQ2PS lanes are exact zero.
            self.emit(Opcode::Movq, [asm_arg(xmm(FS0)), asm_arg(r64(r))]);
            self.emit(
                Opcode::parse(if to_ps { "cvtdq2ps" } else { "cvtdq2pd" }).unwrap(),
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))],
            );
            if to_ps {
                // MOVLHPS copies bits only; merge.high64 moves via high -> low
                // -> high, preserving even signaling NaN payloads unchanged.
                self.emit(Opcode::Movhlps, [asm_arg(xmm(FS1)), asm_arg(xmm(FS1))]);
                self.emit(Opcode::Movlhps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
            }
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(Opcode::Movups, [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
            }
            self.def_done(result, d);
        } else {
            let r = self.use_freg(args[0], FS1);
            let ps = matches!(id, X86SseMmxvalueCvtps2pi | X86SseMmxvalueCvttps2pi);
            // XMM MOVQ zero padding excludes upper source poison from FP
            // conversion. PD consumes both original lanes, with no padding.
            self.emit(
                if ps { Opcode::Movq } else { Opcode::Movups },
                [asm_arg(xmm(FS0)), asm_arg(xmm(r))],
            );
            let op = match id {
                X86SseMmxvalueCvtps2pi => "cvtps2dq",
                X86SseMmxvalueCvttps2pi => "cvttps2dq",
                X86SseMmxvalueCvtpd2pi => "cvtpd2dq",
                X86SseMmxvalueCvttpd2pi => "cvttpd2dq",
                _ => unreachable!("verified MMX-value conversion"),
            };
            self.emit(
                Opcode::parse(op).unwrap(),
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))],
            );
            let d = self.def_reg(result);
            self.emit(Opcode::Movq, [asm_arg(r64(d)), asm_arg(xmm(FS0))]);
            self.def_done(result, d);
        }
    }
}
