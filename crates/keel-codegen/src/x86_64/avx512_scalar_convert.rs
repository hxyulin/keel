//! Scalar AVX512F conversions and comparisons expose rounding, SAE and raw flags.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("scalar EVEX conversion/flags require AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("scalar EVEX conversion/flag signature mismatch");
    }
    let (_, index) = id.x86_avx512_scalar_convert().expect("scalar conversion");
    if !iconst(func, args[index])
        .is_some_and(|value| id.valid_x86_avx512_scalar_convert_control(value))
    {
        return Err("invalid scalar EVEX conversion/flag rounding or SAE control");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    fn scalar_convert_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                Opcode::Movups,
                [asm_arg(xmm(scratch)), asm_arg(xmm(source))],
            );
        }
    }
    pub(super) fn native_avx512_scalar_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, index) = id.x86_avx512_scalar_convert().expect("scalar conversion");
        let mut op = EvexOpcode::parse(name).expect("scalar EVEX opcode");
        let control = iconst(self.func, args[index]).expect("validated control");
        if control != 4 {
            op.options.rounding = Some(if id.x86_avx512_scalar_convert_embedded_rounding() {
                match control {
                    8 => Rounding::Nearest,
                    9 => Rounding::Down,
                    10 => Rounding::Up,
                    11 => Rounding::Zero,
                    _ => unreachable!("validated rounding"),
                }
            } else {
                Rounding::Sae
            });
        }
        if name.contains("comis") {
            self.scalar_convert_stage(args[0], FS1);
            self.scalar_convert_stage(args[1], FS2);
            self.emit(Opcode::Evex(op), [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
            // Snapshot all flags before integer packing changes EFLAGS.
            for (cc, r) in [(2, S0), (10, S1), (4, S2)] {
                self.emit(Opcode::Setcc(cc), [asm_arg(r8(r))]);
            }
            for r in [S0, S1, S2] {
                self.emit(Opcode::Movzx, [asm_arg(r32(r)), asm_arg(r8(r))]);
            }
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(2)]);
            self.emit(Opcode::Shl, [asm_arg(r32(S2)), asm_arg(6)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S2))]);
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
            }
            self.def_done(result, d);
            return;
        }
        let target = id.info().returns[0];
        if target.is_int() {
            self.scalar_convert_stage(args[0], FS1);
            let d = self.def_reg(result);
            self.emit(
                Opcode::Evex(op),
                [asm_arg(reg(d, target)), asm_arg(xmm(FS1))],
            );
            self.def_done(result, d);
            return;
        }
        if index == 2 {
            self.scalar_convert_stage(args[0], FS1);
            let a = self.use_reg(args[1], S0);
            let d = self.def_freg(result);
            self.emit(
                Opcode::Evex(op),
                [
                    asm_arg(xmm(d)),
                    asm_arg(xmm(FS1)),
                    asm_arg(reg(a, self.ty(args[1]))),
                ],
            );
            self.def_done(result, d);
            return;
        }
        self.scalar_convert_stage(args[0], FS0);
        self.scalar_convert_stage(args[2], FS1);
        self.scalar_convert_stage(args[3], FS2);
        self.avx512_stage_mask(args[1]);
        op.options.mask = Some(1);
        self.emit(
            Opcode::Evex(op),
            [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
        );
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(Opcode::Movups, [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
        }
        self.def_done(result, d);
    }
}
