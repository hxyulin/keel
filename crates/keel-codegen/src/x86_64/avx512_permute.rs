//! Pure register shuffles compute into scratch and merge immutable scalar-mask lanes.
use super::*;
use keel_asm_ir::x86::evex::EvexOpcode;
use keel_ir::X86Avx512PermuteKind as K;
pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f || (id.x86_avx512_vl_permute().is_some() && !features.avx512vl) {
        return Err("AVX512 permutation requires AVX512F and narrow forms require AVX512VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("AVX512 permutation signature mismatch");
    }
    let (_, kind) = id.x86_avx512_permute().expect("permutation intrinsic");
    if let Some(index) = kind.control_index()
        && !iconst(func, args[index]).is_some_and(|control| (0..=255).contains(&control))
    {
        return Err("AVX512 permutation control must be iconst.i32 in 0..=255");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_permute(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (name, kind) = id.x86_avx512_permute().expect("permutation intrinsic");
        let width = (self.ty(args[0]).bytes().expect("permutation width") * 8) as u16;
        self.avx512_stage_vector(args[2], FS0);
        if matches!(
            kind,
            K::Full | K::Local | K::TwoIndex | K::TwoTable | K::Pair | K::PairImmediate
        ) {
            self.avx512_stage_vector(args[3], FS1);
        }
        if matches!(kind, K::TwoIndex | K::TwoTable) {
            self.avx512_stage_vector(args[4], FS2);
        }
        let op = Opcode::Evex(EvexOpcode::parse(name).expect("AVX512F permutation opcode"));
        let z = |r| asm_arg(simd_width(r, width));
        let computed = match kind {
            K::Full => {
                self.emit(op, [z(FS2), z(FS1), z(FS0)]);
                FS2
            }
            K::Local => {
                self.emit(op, [z(FS2), z(FS0), z(FS1)]);
                FS2
            }
            K::TwoIndex => {
                self.emit(op, [z(FS1), z(FS0), z(FS2)]);
                FS1
            }
            K::TwoTable => {
                self.emit(op, [z(FS0), z(FS1), z(FS2)]);
                FS0
            }
            K::Immediate => {
                self.emit(
                    op,
                    [
                        z(FS2),
                        z(FS0),
                        asm_arg(iconst(self.func, args[3]).expect("validated immediate")),
                    ],
                );
                FS2
            }
            K::PairImmediate => {
                self.emit(
                    op,
                    [
                        z(FS2),
                        z(FS0),
                        z(FS1),
                        asm_arg(iconst(self.func, args[4]).expect("validated immediate")),
                    ],
                );
                FS2
            }
            K::Pair => {
                self.emit(op, [z(FS2), z(FS0), z(FS1)]);
                FS2
            }
            K::Unary => {
                self.emit(op, [z(FS2), z(FS0)]);
                FS2
            }
        };
        // Read the original merge only after the tied computation. Every source
        // remains in an allocated register or spill, and scratch12 is avoided.
        let merge = if computed == FS0 { FS1 } else { FS0 };
        self.avx512_stage_vector(args[0], merge);
        self.avx512_stage_mask(args[1]);
        let shape = self.ty(args[0]).vector().expect("permutation vector");
        let name = if shape.lane_type().bytes() == Some(8) {
            "vmovupd"
        } else {
            "vmovups"
        };
        self.emit(avx512_mask::opcode(name, true), [z(merge), z(computed)]);
        let d = self.def_freg(result);
        if d != merge {
            self.emit(movs(self.ty(args[0])), [z(d), z(merge)]);
        }
        self.def_done(result, d);
    }
}
