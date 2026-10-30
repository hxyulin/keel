//! DQ lane values preserve payload bits and use the instruction's encoded width.
use super::*;
use keel_ir::intrinsic::X86Avx512DqLaneKind;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_dq_lane().expect("DQ lane descriptor");
    if !features.avx512f || !features.avx512dq || (d.width < 512 && !features.avx512vl) {
        return Err("DQ lane operation requires AVX512F+DQ and narrow encodings require AVX512VL");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("DQ lane intrinsic signature mismatch");
    }
    if d.control.is_some_and(|(index, max)| {
        !iconst(func, args[index]).is_some_and(|n| (0..=max).contains(&n))
    }) {
        return Err("DQ lane immediate must be a constant in its instruction range");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_dq_lane(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_dq_lane().expect("DQ lane descriptor");
        let input = Type::Vector(d.input);
        let output = Type::Vector(d.output);
        self.avx512_stage_vector(args[0], FS0);
        self.avx512_stage_vector(args[2], FS1);
        let opcode = avx512_mask::opcode(d.opcode, true);
        match d.kind {
            X86Avx512DqLaneKind::Register => {
                self.avx512_stage_mask(args[1]);
                self.emit(
                    opcode,
                    [asm_arg(simd(FS0, output)), asm_arg(simd(FS1, input))],
                );
            }
            X86Avx512DqLaneKind::Memory => {
                // These broadcasts lack register-source encodings. The SSA
                // source is captured once in compiler-owned exact-width storage.
                let home = sized(
                    self.copy_slot(self.copy_temp.expect("DQ block broadcast storage")),
                    ptr(input),
                );
                self.emit(
                    movs(input),
                    [asm_arg(home.clone()), asm_arg(simd(FS1, input))],
                );
                self.avx512_stage_mask(args[1]);
                self.emit(opcode, [asm_arg(simd(FS0, output)), asm_arg(home)]);
            }
            X86Avx512DqLaneKind::Extract => {
                self.avx512_stage_mask(args[1]);
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, output)),
                        asm_arg(simd(FS1, input)),
                        asm_arg(iconst(self.func, args[3]).expect("validated extraction index")),
                    ],
                );
            }
            X86Avx512DqLaneKind::Insert => {
                self.avx512_stage_vector(args[3], FS2);
                self.avx512_stage_mask(args[1]);
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, output)),
                        asm_arg(simd(FS1, output)),
                        asm_arg(simd(FS2, input)),
                        asm_arg(iconst(self.func, args[4]).expect("validated insertion index")),
                    ],
                );
            }
        }
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
