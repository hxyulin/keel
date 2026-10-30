//! VSIB operations use reserved SIMD views and K1; destructive gather masks never
//! modify the immutable scalar SSA mask or a live index/source allocation.
use super::avx512_mask::opcode;
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("AVX512 indexed memory requires AVX512F");
    }
    if args.len() != 5
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(v) != ty)
    {
        return Err("AVX512 indexed memory signature mismatch");
    }
    if !iconst(func, args[4]).is_some_and(|scale| matches!(scale, 1 | 2 | 4 | 8)) {
        return Err("indexed memory scale must be iconst.i32 in1,2,4,8");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_indexed(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        for (value, scratch) in [(args[1], FS1), (args[2], FS0)] {
            let ty = self.ty(value);
            let source = self.use_freg(value, scratch);
            if source != scratch {
                self.emit(
                    movs(ty),
                    [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
                );
            }
        }
        self.avx512_stage_mask(args[3]);
        let base = self.use_reg(args[0], S1);
        let mut at = memory(r64(base), 0);
        at.index = Some(simd(FS1, self.ty(args[1])));
        at.scale = iconst(self.func, args[4]).expect("validated VSIB scale") as u8;
        let ty = self.ty(args[2]);
        let width = ty
            .vector()
            .expect("indexed data vector")
            .lane_type()
            .bytes()
            .expect("lane width") as u16
            * 8;
        let name = id
            .info()
            .name
            .strip_prefix("x86_avx512_")
            .expect("indexed intrinsic name");
        if id.is_x86_avx512_scatter() {
            self.emit(
                opcode(name, true),
                [asm_arg(sized(at, width)), asm_arg(simd(FS0, ty))],
            );
        } else {
            self.emit(
                opcode(name, true),
                [asm_arg(simd(FS0, ty)), asm_arg(sized(at, width))],
            );
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
            }
            self.def_done(result, d);
        }
    }
}
