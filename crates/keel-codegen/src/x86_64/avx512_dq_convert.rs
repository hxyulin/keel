//! Raw DQ qword/FP conversions preserve hardware flags and indefinite results.
use super::*;
use keel_asm_ir::x86::evex::{EvexOpcode, Rounding};

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let (_, width, _, _) = id
        .x86_avx512_dq_convert()
        .expect("DQ conversion descriptor");
    if !features.avx512f || !features.avx512dq || (width < 512 && !features.avx512vl) {
        return Err("qword/FP conversion requires AVX512F+DQ and narrow forms require AVX512VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("DQ conversion signature mismatch");
    }
    if width == 512
        && !iconst(func, args[3]).is_some_and(|n| id.valid_x86_avx512_dq_convert_control(n))
    {
        return Err("invalid DQ conversion rounding/SAE control");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_dq_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, width, input, output) = id
            .x86_avx512_dq_convert()
            .expect("DQ conversion descriptor");
        let input = Type::Vector(input);
        let output = Type::Vector(output);
        debug_assert_eq!(
            u32::from(width),
            input.bytes().unwrap().max(output.bytes().unwrap()) * 8
        );
        // Capture source and independent merge before touching an overlapping result.
        self.avx512_stage_vector(args[2], FS1);
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_mask(args[1]);
        let mut opcode = EvexOpcode::parse(name).expect("imported DQ conversion");
        opcode.options.mask = Some(1);
        if width == 512 {
            let control = iconst(self.func, args[3]).expect("validated DQ control");
            if control != 4 {
                opcode.options.rounding = Some(if name.starts_with("vcvtt") {
                    Rounding::Sae
                } else {
                    match control & 3 {
                        0 => Rounding::Nearest,
                        1 => Rounding::Down,
                        2 => Rounding::Up,
                        _ => Rounding::Zero,
                    }
                });
            }
        }
        // A 128-bit QQ->PS conversion zeros its upper two dwords even with mask0.
        self.emit(
            Opcode::Evex(opcode),
            [asm_arg(simd(FS0, output)), asm_arg(simd(FS1, input))],
        );
        let dest = self.def_freg(result);
        if dest != FS0 {
            self.emit(
                movs(output),
                [asm_arg(simd(dest, output)), asm_arg(simd(FS0, output))],
            );
        }
        self.def_done(result, dest);
    }
}
