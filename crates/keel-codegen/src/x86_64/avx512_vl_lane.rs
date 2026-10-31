//! F+VL broadcasts and lane transfers preserve all source homes.
use super::*;
use keel_ir::intrinsic::X86Avx512VlLaneKind as Kind;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    let d = id.x86_avx512_vl_lane().expect("VL lane descriptor");
    if !features.avx512f || !features.avx512vl {
        return Err("VL lane operation requires AVX512F plus AVX512VL");
    }
    if args.len() != d.params.len()
        || args
            .iter()
            .zip(d.params)
            .any(|(&v, &ty)| func.value_type(func.resolve(v)) != ty)
    {
        return Err("VL lane signature mismatch");
    }
    if d.control
        .is_some_and(|index| !iconst(func, args[index]).is_some_and(|n| (0..=1).contains(&n)))
    {
        return Err("VL lane index must be a constant zero or one");
    }
    Ok(())
}

impl FunctionCompiler<'_> {
    pub(super) fn native_avx512_vl_lane(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let d = id.x86_avx512_vl_lane().expect("VL lane descriptor");
        let output = Type::Vector(d.output);
        let opcode = avx512_mask::opcode(d.opcode, true);
        self.avx512_stage_vector(args[0], FS0);
        match d.kind {
            Kind::ScalarInt => {
                // Mask staging may reload through the same GP scratch.
                self.avx512_stage_mask(args[1]);
                let scalar = self.use_reg(args[2], S0);
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, output)),
                        asm_arg(if d.input == Type::I64 {
                            r64(scalar)
                        } else {
                            r32(scalar)
                        }),
                    ],
                );
            }
            Kind::ScalarFloat => {
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_stage_mask(args[1]);
                self.emit(opcode, [asm_arg(simd(FS0, output)), asm_arg(xmm(FS1))]);
            }
            Kind::BroadcastBlock => {
                self.avx512_stage_vector(args[2], FS1);
                // These encodings accept memory sources only. Capture exactly
                // the 16-byte SSA source in the function's owned staging home.
                let home = sized(
                    self.copy_slot(self.copy_temp.expect("VL broadcast storage")),
                    ptr(d.input),
                );
                self.emit(
                    movs(d.input),
                    [asm_arg(home.clone()), asm_arg(simd(FS1, d.input))],
                );
                self.avx512_stage_mask(args[1]);
                self.emit(opcode, [asm_arg(simd(FS0, output)), asm_arg(home)]);
            }
            Kind::ExtractBlock => {
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_stage_mask(args[1]);
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, output)),
                        asm_arg(simd(FS1, d.input)),
                        asm_arg(iconst(self.func, args[3]).expect("validated lane")),
                    ],
                );
            }
            Kind::InsertBlock => {
                self.avx512_stage_vector(args[2], FS1);
                self.avx512_stage_vector(args[3], FS2);
                self.avx512_stage_mask(args[1]);
                self.emit(
                    opcode,
                    [
                        asm_arg(simd(FS0, output)),
                        asm_arg(simd(FS1, output)),
                        asm_arg(simd(FS2, d.input)),
                        asm_arg(iconst(self.func, args[4]).expect("validated lane")),
                    ],
                );
            }
        }
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(output),
                [
                    asm_arg(simd(destination, output)),
                    asm_arg(simd(FS0, output)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}
