//! Exact scalar constant division, using only reserved GPR scratch registers.
use super::*;

impl FunctionCompiler<'_> {
    /// Divisors are normalized at the IR width and exclude zero, one and
    /// signed minus one. None of these operations can trap. Narrow dividends
    /// are extended before the 32-bit recipe; only the low IR bits are returned.
    pub(super) fn constant_division(&mut self, op: BinaryOp, ty: Type, a: u8, divisor: i64, d: u8) {
        let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
        let remainder = matches!(op, BinaryOp::Srem | BinaryOp::Urem);
        let width = wide_ty(ty);
        let bits = if width == Type::I64 { 64 } else { 32 };
        let magnitude = if signed {
            divisor.unsigned_abs()
        } else {
            divisor as u64
        };
        let r = |n| reg(n, width);
        let x = self.extend(S0, a, ty, signed);
        if x != S0 {
            self.emit(Opcode::Mov, [asm_arg(r(S0)), asm_arg(r(x))]);
        }
        if !signed && remainder && magnitude.is_power_of_two() {
            let mask = magnitude - 1;
            if i32::try_from(mask).is_ok() {
                self.emit(Opcode::And, [asm_arg(r(S0)), asm_arg(mask)]);
            } else {
                self.emit(Opcode::Movabs, [asm_arg(r64(S1)), asm_arg(mask)]);
                self.emit(Opcode::And, [asm_arg(r(S0)), asm_arg(r(S1))]);
            }
            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S0))]);
            return;
        }
        // Keep the original dividend for sign correction, reciprocal averaging
        // and remainders. r11 is never allocated, including tiny/spilled pools.
        self.emit(Opcode::Mov, [asm_arg(r(S3)), asm_arg(r(S0))]);
        let quotient;
        if magnitude.is_power_of_two() {
            let shift = magnitude.trailing_zeros();
            if signed {
                self.emit(Opcode::Mov, [asm_arg(r(S1)), asm_arg(r(S0))]);
                self.emit(Opcode::Sar, [asm_arg(r(S1)), asm_arg(bits - 1)]);
                self.emit(Opcode::Shr, [asm_arg(r(S1)), asm_arg(bits - shift)]);
                self.emit(Opcode::Add, [asm_arg(r(S0)), asm_arg(r(S1))]);
                self.emit(Opcode::Sar, [asm_arg(r(S0)), asm_arg(shift)]);
                if divisor < 0 {
                    self.emit(Opcode::Neg, [asm_arg(r(S0))]);
                }
            } else {
                self.emit(Opcode::Shr, [asm_arg(r(S0)), asm_arg(shift)]);
            }
            quotient = S0;
        } else if signed {
            let (multiplier, shift) = signed_reciprocal(bits, divisor);
            // For i32, a signed 32x32 product fits in i64; for i64 IMUL
            // gives the signed high half in rdx. Both use the same correction.
            let signed_multiplier = if bits == 32 {
                multiplier as u32 as i32 as i64 as u64
            } else {
                multiplier
            };
            self.emit(
                Opcode::Movabs,
                [asm_arg(r64(S1)), asm_arg(signed_multiplier)],
            );
            if bits == 32 {
                self.emit(Opcode::Movsxd, [asm_arg(r64(S0)), asm_arg(r32(S0))]);
                self.emit(Opcode::Imul, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
                self.emit(Opcode::Sar, [asm_arg(r64(S0)), asm_arg(32u32)]);
                quotient = S0;
            } else {
                self.emit(Opcode::Imul, [asm_arg(r64(S1))]);
                quotient = S2;
            }
            let negative_multiplier = multiplier >> (bits - 1) != 0;
            let correction = match (divisor > 0, negative_multiplier) {
                (true, true) => Some(Opcode::Add),
                (false, false) => Some(Opcode::Sub),
                _ => None,
            };
            if let Some(op) = correction {
                self.emit(op, [asm_arg(r(quotient)), asm_arg(r(S3))]);
            }
            if shift != 0 {
                self.emit(Opcode::Sar, [asm_arg(r(quotient)), asm_arg(shift)]);
            }
            self.emit(Opcode::Mov, [asm_arg(r(S1)), asm_arg(r(quotient))]);
            self.emit(Opcode::Shr, [asm_arg(r(S1)), asm_arg(bits - 1)]);
            self.emit(Opcode::Add, [asm_arg(r(quotient)), asm_arg(r(S1))]);
        } else {
            let plan = unsigned_reciprocal(bits, magnitude);
            self.emit(Opcode::Movabs, [asm_arg(r64(S1)), asm_arg(plan.multiplier)]);
            if bits == 32 {
                // A prior i32 copy or extension has zeroed rax's upper half.
                self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(r32(S0))]);
                self.emit(Opcode::Imul, [asm_arg(r64(S0)), asm_arg(r64(S1))]);
                self.emit(Opcode::Shr, [asm_arg(r64(S0)), asm_arg(32u32)]);
                quotient = S0;
            } else {
                self.emit(Opcode::Mul, [asm_arg(r64(S1))]);
                quotient = S2;
            }
            if plan.add {
                self.emit(Opcode::Mov, [asm_arg(r(S1)), asm_arg(r(S3))]);
                self.emit(Opcode::Sub, [asm_arg(r(S1)), asm_arg(r(quotient))]);
                self.emit(Opcode::Shr, [asm_arg(r(S1)), asm_arg(1u32)]);
                self.emit(Opcode::Add, [asm_arg(r(quotient)), asm_arg(r(S1))]);
            }
            if plan.shift != 0 {
                self.emit(Opcode::Shr, [asm_arg(r(quotient)), asm_arg(plan.shift)]);
            }
        }
        if remainder {
            self.emit(Opcode::Movabs, [asm_arg(r64(S1)), asm_arg(divisor as u64)]);
            self.emit(Opcode::Imul, [asm_arg(r(quotient)), asm_arg(r(S1))]);
            self.emit(Opcode::Sub, [asm_arg(r(S3)), asm_arg(r(quotient))]);
            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S3))]);
        } else {
            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(quotient))]);
        }
    }
}
