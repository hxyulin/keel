//! Scalar EVEX operations retain independent low-lane merge and upper passthrough.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("scalar EVEX operation requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("scalar EVEX signature mismatch");
    }
    let (_, immediate, control) = id.x86_avx512_scalar().expect("scalar EVEX intrinsic");
    for index in immediate.into_iter().chain(control) {
        if !iconst(func, args[index])
            .is_some_and(|value| id.valid_x86_avx512_scalar_control(index, value))
        {
            return Err("invalid scalar EVEX immediate or SAE/rounding control");
        }
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    fn scalar_evex_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            self.emit(
                Opcode::Movups,
                [asm_arg(xmm(scratch)), asm_arg(xmm(source))],
            );
        }
    }
    pub(super) fn native_avx512_scalar(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, immediate, control) = id.x86_avx512_scalar().expect("scalar EVEX intrinsic");
        let compare = name.starts_with("vcmp");
        if compare {
            self.scalar_evex_stage(args[0], FS1);
            self.scalar_evex_stage(args[1], FS2);
            self.avx512_stage_mask(args[2]);
        } else {
            self.scalar_evex_stage(args[0], FS0);
            self.scalar_evex_stage(args[2], FS1);
            self.scalar_evex_stage(args[3], FS2);
            self.avx512_stage_mask(args[1]);
        }
        let mut op = EvexOpcode::parse(name).expect("scalar EVEX opcode");
        op.options.mask = Some(1);
        if let Some(index) = control {
            let value = iconst(self.func, args[index]).expect("validated control");
            if value != 4 {
                op.options.rounding = Some(if id.x86_avx512_scalar_embedded_rounding() {
                    match value {
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
        }
        if compare {
            self.emit(
                Opcode::Evex(op),
                [
                    asm_arg(avx512_mask::k1()),
                    asm_arg(xmm(FS1)),
                    asm_arg(xmm(FS2)),
                    asm_arg(iconst(self.func, args[3]).expect("validated predicate")),
                ],
            );
            let dest = self.def_reg(result);
            self.emit(
                Opcode::parse("kmovw").expect("AVX512F mask move"),
                [asm_arg(r32(dest)), asm_arg(avx512_mask::k1())],
            );
            self.def_done(result, dest);
            return;
        }
        if let Some(index) = immediate {
            self.emit(
                Opcode::Evex(op),
                [
                    asm_arg(xmm(FS0)),
                    asm_arg(xmm(FS1)),
                    asm_arg(xmm(FS2)),
                    asm_arg(iconst(self.func, args[index]).expect("validated immediate")),
                ],
            );
        } else {
            self.emit(
                Opcode::Evex(op),
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
            );
        }
        let dest = self.def_freg(result);
        if dest != FS0 {
            self.emit(Opcode::Movups, [asm_arg(xmm(dest)), asm_arg(xmm(FS0))]);
        }
        self.def_done(result, dest);
    }
}
