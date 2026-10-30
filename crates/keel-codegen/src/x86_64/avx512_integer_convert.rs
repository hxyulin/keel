//! Masked foundation integer conversion with shortened memory operands.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("integer conversion intrinsic requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 integer conversion signature mismatch");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    fn integer_convert_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            let ty = self.ty(value);
            self.emit(
                movs(ty),
                [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
            );
        }
    }
    pub(super) fn native_avx512_integer_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let opcode = avx512_mask::opcode(
            id.x86_avx512_integer_convert_opcode()
                .expect("integer conversion opcode"),
            true,
        );
        if id.is_x86_avx512_integer_convert_load() || id.is_x86_avx512_integer_convert_store() {
            let ty = self.ty(args[1]);
            self.integer_convert_stage(args[1], FS0);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let width = id
                .x86_avx512_integer_convert_memory_bytes()
                .expect("short integer memory operand")
                * 8;
            let memory = sized(memory(r64(base), 0), width);
            if id.is_x86_avx512_integer_convert_store() {
                self.emit(opcode, [asm_arg(memory), asm_arg(simd(FS0, ty))]);
                return;
            }
            self.emit(opcode, [asm_arg(simd(FS0, ty)), asm_arg(memory)]);
        } else {
            // Both immutable operands can remain live, alias or spill. Capture
            // them at their own widths before the destination merge is changed.
            let destination = self.ty(args[0]);
            let source = self.ty(args[2]);
            self.integer_convert_stage(args[0], FS0);
            self.integer_convert_stage(args[2], FS1);
            self.avx512_stage_mask(args[1]);
            self.emit(
                opcode,
                [asm_arg(simd(FS0, destination)), asm_arg(simd(FS1, source))],
            );
        }
        let value = result.expect("integer conversion result");
        let ty = self.ty(value);
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(ty),
                [asm_arg(simd(destination, ty)), asm_arg(simd(FS0, ty))],
            );
        }
        self.def_done(result, destination);
    }
}
