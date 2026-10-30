//! BW shuffles use reserved scratch views and keep merge independent of tied A.
use super::*;
use keel_ir::intrinsic::X86Avx512BwPermuteKind as K;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id
        .x86_avx512_bw_permute()
        .expect("BW permutation descriptor");
    if !features.avx512f || !features.avx512bw || (d.vector.bytes() < 64 && !features.avx512vl) {
        return Err(
            "BW permutations require AVX512F plus AVX512BW, and narrow forms require AVX512VL",
        );
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("BW permutation signature mismatch");
    }
    if let Some(index) = d.kind.control_index()
        && !iconst(func, args[index]).is_some_and(|n| (0..=255).contains(&n))
    {
        return Err("BW permutation control must be iconst.i32 in 0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_bw_permute(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id
            .x86_avx512_bw_permute()
            .expect("BW permutation descriptor");
        let width = (d.vector.bytes() * 8) as u16;
        let v = |r| asm_arg(simd_width(r, width));
        let output = if d.kind.unmasked() {
            self.avx512_stage_vector(args[0], FS0);
            self.emit(
                avx512_mask::opcode(d.opcode, false),
                [
                    v(FS1),
                    v(FS0),
                    asm_arg(iconst(self.func, args[1]).expect("validated byte shift")),
                ],
            );
            FS1
        } else if d.kind == K::TwoTable {
            self.avx512_stage_vector(args[2], FS0);
            self.avx512_stage_vector(args[3], FS1);
            self.avx512_stage_vector(args[4], FS2);
            self.emit(
                avx512_mask::opcode(d.opcode, false),
                [v(FS0), v(FS1), v(FS2)],
            );
            self.avx512_stage_vector(args[0], FS1);
            self.avx512_bw_stage_mask(args[1]);
            self.emit(avx512_mask::opcode("vmovdqu16", true), [v(FS1), v(FS0)]);
            FS1
        } else {
            self.avx512_stage_vector(args[0], FS0);
            self.avx512_stage_vector(args[2], FS1);
            if matches!(d.kind, K::ByteShuffle | K::Align | K::Permute) {
                self.avx512_stage_vector(args[3], FS2);
            }
            self.avx512_bw_stage_mask(args[1]);
            let opcode = avx512_mask::opcode(d.opcode, true);
            match d.kind {
                K::ByteShuffle | K::Permute => self.emit(opcode, [v(FS0), v(FS1), v(FS2)]),
                K::Align => self.emit(
                    opcode,
                    [
                        v(FS0),
                        v(FS1),
                        v(FS2),
                        asm_arg(iconst(self.func, args[4]).expect("validated alignment")),
                    ],
                ),
                K::HalfHigh | K::HalfLow => self.emit(
                    opcode,
                    [
                        v(FS0),
                        v(FS1),
                        asm_arg(iconst(self.func, args[3]).expect("validated word shuffle")),
                    ],
                ),
                _ => unreachable!("BW permutation recipe"),
            };
            FS0
        };
        let destination = self.def_freg(result);
        if destination != output {
            self.emit(movs(Type::Vector(d.vector)), [v(destination), v(output)]);
        }
        self.def_done(result, destination);
    }
}
