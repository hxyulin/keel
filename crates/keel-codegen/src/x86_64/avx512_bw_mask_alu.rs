//! Native BW mask algebra uses scalar SSA homes and transient K1..K3.
use super::*;
use keel_ir::intrinsic::X86Avx512BwMaskAluKind as Kind;
fn k(number: u8) -> Register {
    Register {
        class: RegisterClass::Mask,
        number,
        high_byte: false,
        width: 64,
    }
}
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f || !features.avx512bw {
        return Err("BW mask algebra requires AVX512F+BW");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("BW mask algebra signature mismatch");
    }
    if id
        .x86_avx512_bw_mask_alu()
        .is_some_and(|d| d.kind == Kind::Shift)
        && !iconst(func, args[1]).is_some_and(|n| (0..=255).contains(&n))
    {
        return Err("BW mask shift requires an imm8 constant");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_mask_alu(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id
            .x86_avx512_bw_mask_alu()
            .expect("BW mask algebra descriptor");
        let wide = d.ty == Type::I64;
        let transfer =
            Opcode::parse(if wide { "kmovq" } else { "kmovd" }).expect("BW mask transfer");
        let gp = |n| if wide { r64(n) } else { r32(n) };
        let a = self.use_reg(args[0], S0);
        self.emit(transfer, [asm_arg(k(1)), asm_arg(gp(a))]);
        if matches!(d.kind, Kind::Binary | Kind::Test) {
            let b = self.use_reg(args[1], S1);
            self.emit(transfer, [asm_arg(k(2)), asm_arg(gp(b))]);
        }
        let opcode = Opcode::parse(d.opcode).expect("imported BW mask opcode");
        if d.kind == Kind::Test {
            self.emit(opcode, [asm_arg(k(1)), asm_arg(k(2))]);
            // Capture both flags before any integer packing modifies FLAGS.
            self.emit(Opcode::Setcc(4), [asm_arg(r8(S0))]);
            self.emit(Opcode::Setcc(2), [asm_arg(r8(S1))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r8(S0))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S1)), asm_arg(r8(S1))]);
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(1)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            let dest = self.def_reg(result);
            if dest != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(dest)), asm_arg(r32(S0))]);
            }
            self.def_done(result, dest);
            return;
        }
        match d.kind {
            Kind::Unary => self.emit(opcode, [asm_arg(k(3)), asm_arg(k(1))]),
            Kind::Shift => self.emit(
                opcode,
                [
                    asm_arg(k(3)),
                    asm_arg(k(1)),
                    asm_arg(iconst(self.func, args[1]).expect("validated imm8")),
                ],
            ),
            Kind::Binary => self.emit(opcode, [asm_arg(k(3)), asm_arg(k(1)), asm_arg(k(2))]),
            Kind::Test => unreachable!("mask tests return before algebra emission"),
        }
        let dest = self.def_reg(result);
        self.emit(transfer, [asm_arg(gp(dest)), asm_arg(k(3))]);
        self.def_done(result, dest);
    }
}
