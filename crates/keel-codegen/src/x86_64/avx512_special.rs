//! Raw packed FP special operations retain MXCSR ordering and merge masking.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f || (id.x86_avx512_vl_special().is_some() && !features.avx512vl) {
        return Err("AVX512 floating special requires AVX512F and narrow forms require AVX512VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 floating special signature mismatch");
    }
    let (_, immediate, control) = id.x86_avx512_special().expect("special intrinsic");
    for index in immediate.into_iter().chain(control) {
        if !iconst(func, args[index])
            .is_some_and(|value| id.valid_x86_avx512_special_control(index, value))
        {
            return Err("invalid AVX512 floating special immediate or SAE/rounding control");
        }
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_special(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, immediate, control) = id.x86_avx512_special().expect("special intrinsic");
        let width = (self.ty(args[0]).bytes().expect("floating vector width") * 8) as u16;
        let independent_fixup =
            id.x86_avx512_vl_special().is_some() && name.starts_with("vfixupimm");
        let binary = name.starts_with("vscalef") || name.starts_with("vfixupimm");
        if independent_fixup {
            self.avx512_stage_vector(args[2], FS0);
            self.avx512_stage_vector(args[3], FS1);
            self.avx512_stage_vector(args[4], FS2);
        } else {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            if binary {
                self.avx512_stage_vector(args[3], FS2);
            }
        }
        self.avx512_stage_mask(args[1]);
        let mut op = EvexOpcode::parse(name).expect("AVX512F special opcode");
        op.options.mask = Some(1);
        if let Some(index) = control {
            op.options.rounding = match iconst(self.func, args[index]).expect("validated control") {
                4 => None,
                value if name.starts_with("vscalef") => Some(match value {
                    8 => Rounding::Nearest,
                    9 => Rounding::Down,
                    10 => Rounding::Up,
                    11 => Rounding::Zero,
                    _ => unreachable!("validated rounding"),
                }),
                8 => Some(Rounding::Sae),
                _ => unreachable!("validated SAE"),
            };
        }
        let d = asm_arg(simd_width(FS0, width));
        let a = asm_arg(simd_width(FS1, width));
        let b = asm_arg(simd_width(FS2, width));
        match (binary, immediate) {
            (false, None) => self.emit(Opcode::Evex(op), [d, a]),
            (false, Some(index)) => self.emit(
                Opcode::Evex(op),
                [
                    d,
                    a,
                    asm_arg(iconst(self.func, args[index]).expect("validated immediate")),
                ],
            ),
            (true, None) => self.emit(Opcode::Evex(op), [d, a, b]),
            (true, Some(index)) => self.emit(
                Opcode::Evex(op),
                [
                    d,
                    a,
                    b,
                    asm_arg(iconst(self.func, args[index]).expect("validated immediate")),
                ],
            ),
        }
        let result_scratch = if independent_fixup {
            // Original SSA operands remain in their allocated registers or spills.
            // Load merge after the instruction so only reserved scratch registers
            // are used, even in a function containing only 128-bit vectors.
            self.avx512_stage_vector(args[0], FS1);
            let move_opcode = if matches!(
                self.ty(args[0]),
                Type::Vector(keel_ir::VectorType::F64x2 | keel_ir::VectorType::F64x4)
            ) {
                "vmovupd"
            } else {
                "vmovups"
            };
            self.emit(
                avx512_mask::opcode(move_opcode, true),
                [
                    asm_arg(simd_width(FS1, width)),
                    asm_arg(simd_width(FS0, width)),
                ],
            );
            FS1
        } else {
            FS0
        };
        let dest = self.def_freg(result);
        if dest != result_scratch {
            self.emit(
                movs(self.ty(args[0])),
                [
                    asm_arg(simd_width(dest, width)),
                    asm_arg(simd_width(result_scratch, width)),
                ],
            );
        }
        self.def_done(result, dest);
    }
}
