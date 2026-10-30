//! Word masks live in scalar SSA homes; K1..K3 are transient scratch registers.
use super::*;

fn mask(number: u8) -> Register {
    Register {
        class: RegisterClass::Mask,
        number,
        width: 64,
        high_byte: false,
    }
}

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("word mask intrinsic requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("word mask intrinsic signature mismatch");
    }
    if id.has_x86_avx512_mask_shift_control()
        && !iconst(func, args[1]).is_some_and(|control| (0..256).contains(&control))
    {
        return Err("word mask shift control must be iconst.i32 in0..=255");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_mask_alu(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        use Intrinsic::*;
        let a = self.use_reg(args[0], S0);
        self.emit(
            Opcode::parse("kmovw").expect("AVX512F mask transfer"),
            [asm_arg(mask(1)), asm_arg(r32(a))],
        );
        if args.len() == 2 && !id.has_x86_avx512_mask_shift_control() {
            let b = self.use_reg(args[1], S1);
            self.emit(
                Opcode::parse("kmovw").expect("AVX512F mask transfer"),
                [asm_arg(mask(2)), asm_arg(r32(b))],
            );
        }
        if matches!(id, X86Avx512Ktestw | X86Avx512Kortestw) {
            if id == X86Avx512Ktestw {
                // KTESTW itself requires AVX512DQ. Its two zero predicates can
                // instead use foundation word algebra and KORTESTW under F.
                self.emit(
                    Opcode::parse("kandw").expect("word AND"),
                    [asm_arg(mask(3)), asm_arg(mask(1)), asm_arg(mask(2))],
                );
                self.emit(
                    Opcode::parse("kortestw").expect("word OR test"),
                    [asm_arg(mask(3)), asm_arg(mask(3))],
                );
                self.emit(Opcode::Setcc(4), [asm_arg(r8(S0))]);
                self.emit(
                    Opcode::parse("kandnw").expect("word AND NOT"),
                    [asm_arg(mask(3)), asm_arg(mask(1)), asm_arg(mask(2))],
                );
                self.emit(
                    Opcode::parse("kortestw").expect("word OR test"),
                    [asm_arg(mask(3)), asm_arg(mask(3))],
                );
                self.emit(Opcode::Setcc(4), [asm_arg(r8(S1))]);
            } else {
                self.emit(
                    Opcode::parse("kortestw").expect("word OR test"),
                    [asm_arg(mask(1)), asm_arg(mask(2))],
                );
                // Capture both bits before integer instructions modify FLAGS.
                self.emit(Opcode::Setcc(4), [asm_arg(r8(S0))]);
                self.emit(Opcode::Setcc(2), [asm_arg(r8(S1))]);
            }
            self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r8(S0))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S1)), asm_arg(r8(S1))]);
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(1)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            let destination = self.def_reg(result);
            if destination != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(destination)), asm_arg(r32(S0))]);
            }
            self.def_done(result, destination);
            return;
        }
        let name = match id {
            X86Avx512Kandw => "kandw",
            X86Avx512Kandnw => "kandnw",
            X86Avx512Korw => "korw",
            X86Avx512Kxorw => "kxorw",
            X86Avx512Kxnorw => "kxnorw",
            X86Avx512Knotw => "knotw",
            X86Avx512Kshiftlw => "kshiftlw",
            X86Avx512Kshiftrw => "kshiftrw",
            X86Avx512Kunpckbw => "kunpckbw",
            _ => unreachable!("word mask operation"),
        };
        let opcode = Opcode::parse(name).expect("foundation word mask opcode");
        if id.has_x86_avx512_mask_shift_control() {
            self.emit(
                opcode,
                [
                    asm_arg(mask(3)),
                    asm_arg(mask(1)),
                    asm_arg(iconst(self.func, args[1]).expect("verified word shift control")),
                ],
            );
        } else if id == X86Avx512Knotw {
            self.emit(opcode, [asm_arg(mask(3)), asm_arg(mask(1))]);
        } else {
            self.emit(
                opcode,
                [asm_arg(mask(3)), asm_arg(mask(1)), asm_arg(mask(2))],
            );
        }
        let destination = self.def_reg(result);
        self.emit(
            Opcode::parse("kmovw").expect("AVX512F mask transfer"),
            [asm_arg(r32(destination)), asm_arg(mask(3))],
        );
        self.def_done(result, destination);
    }
}
