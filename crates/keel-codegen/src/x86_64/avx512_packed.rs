//! Packed AVX512F primitives retain raw arithmetic, merge and memory contracts.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("packed AVX512 operation requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("packed AVX512 signature mismatch");
    }
    if let Some(index) = id.x86_avx512_packed_control()
        && !iconst(func, args[index]).is_some_and(|value| id.valid_x86_avx512_packed_control(value))
    {
        return Err("invalid packed AVX512 SAE/rounding control");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_packed(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let name = id.x86_avx512_packed_opcode().expect("packed AVX512 opcode");
        let info = id.info();
        let load = info.params[0] == Type::Ptr;
        let store = info.returns.is_empty();
        let stream = name.starts_with("vmovnt");
        let mut op = EvexOpcode::parse(name).expect("AVX512F opcode");
        if load || store {
            if !stream {
                self.avx512_stage_vector(args[1], FS0);
                self.avx512_stage_mask(args[2]);
                op.options.mask = Some(1);
            } else if store {
                self.avx512_stage_vector(args[1], FS0);
            }
            let ptr = args[0];
            let base = self.use_reg(ptr, S1);
            let at = asm_arg(sized(memory(r64(base), 0), 512));
            if store {
                self.emit(Opcode::Evex(op), [at, asm_arg(simd_width(FS0, 512))]);
                return;
            }
            self.emit(Opcode::Evex(op), [asm_arg(simd_width(FS0, 512)), at]);
        } else {
            let blend = info.params[0].is_int();
            let moved = !blend && info.params.len() == 3;
            if blend {
                self.avx512_stage_vector(args[1], FS0);
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_stage_mask(args[0]);
            } else {
                self.avx512_stage_vector(args[0], FS0);
                self.avx512_stage_vector(args[2], FS1);
                if !moved && info.params.len() > 3 && info.params[3].is_vector() {
                    self.avx512_stage_vector(args[3], FS2);
                }
                self.avx512_stage_mask(args[1]);
            }
            op.options.mask = Some(1);
            if let Some(index) = id.x86_avx512_packed_control() {
                let value = iconst(self.func, args[index]).expect("validated control");
                if value != 4 {
                    op.options.rounding =
                        Some(if name.starts_with("vmin") || name.starts_with("vmax") {
                            Rounding::Sae
                        } else {
                            match value {
                                8 => Rounding::Nearest,
                                9 => Rounding::Down,
                                10 => Rounding::Up,
                                11 => Rounding::Zero,
                                _ => unreachable!("validated control"),
                            }
                        });
                }
            }
            if moved {
                self.emit(
                    Opcode::Evex(op),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS1, 512))],
                );
            } else if blend {
                self.emit(
                    Opcode::Evex(op),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                    ],
                );
            } else if name.starts_with("vsqrt") {
                self.emit(
                    Opcode::Evex(op),
                    [asm_arg(simd_width(FS0, 512)), asm_arg(simd_width(FS1, 512))],
                );
            } else {
                self.emit(
                    Opcode::Evex(op),
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(simd_width(FS2, 512)),
                    ],
                );
            }
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(
                Opcode::Vmovups,
                [asm_arg(simd_width(d, 512)), asm_arg(simd_width(FS0, 512))],
            );
        }
        self.def_done(result, d);
    }
}
