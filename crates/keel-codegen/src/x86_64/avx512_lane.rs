//! Foundation shifts and lane transfers use their exact source/destination widths.
use super::*;

pub(super) fn validate(
    func: &Function,
    id: Intrinsic,
    args: &[Value],
    features: keel_asm_ir::x86::Features,
) -> Result<(), &'static str> {
    if !features.avx512f {
        return Err("AVX512 lane intrinsic requires AVX512F");
    }
    if args.len() != id.info().params.len()
        || args
            .iter()
            .zip(id.info().params)
            .any(|(&value, &ty)| func.value_type(func.resolve(value)) != ty)
    {
        return Err("AVX512 lane intrinsic signature mismatch");
    }
    if let Some((index, max)) = id.x86_avx512_lane_control()
        && !iconst(func, args[index]).is_some_and(|control| (0..=max).contains(&control))
    {
        return Err("AVX512 lane control is not a constant in its instruction range");
    }
    Ok(())
}
impl FunctionCompiler<'_> {
    fn lane_stage(&mut self, value: Value, scratch: u8) {
        let source = self.use_freg(value, scratch);
        if source != scratch {
            let ty = self.ty(value);
            self.emit(
                movs(ty),
                [asm_arg(simd(scratch, ty)), asm_arg(simd(source, ty))],
            );
        }
    }
    pub(super) fn native_avx512_lane(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let opcode = avx512_mask::opcode(id.x86_avx512_lane_opcode().expect("lane opcode"), true);
        let control = id.x86_avx512_lane_control();
        if id.is_x86_avx512_lane_store() {
            self.lane_stage(args[1], FS1);
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let width = id.x86_avx512_lane_memory_bytes().expect("short lane store") * 8;
            self.emit(
                opcode,
                [
                    asm_arg(sized(memory(r64(base), 0), width)),
                    asm_arg(simd_width(FS1, 512)),
                    asm_arg(iconst(self.func, args[3]).expect("extract lane")),
                ],
            );
            return;
        }
        if id.is_x86_avx512_lane_load() {
            self.lane_stage(args[1], FS0);
            if args.len() == 5 {
                self.lane_stage(args[3], FS1);
            }
            self.avx512_stage_mask(args[2]);
            let base = self.use_reg(args[0], S1);
            let width = id.x86_avx512_lane_memory_bytes().expect("short lane load") * 8;
            let operand = sized(memory(r64(base), 0), width);
            if args.len() == 5 {
                self.emit(
                    opcode,
                    [
                        asm_arg(simd_width(FS0, 512)),
                        asm_arg(simd_width(FS1, 512)),
                        asm_arg(operand),
                        asm_arg(iconst(self.func, args[4]).expect("insert lane")),
                    ],
                );
            } else {
                self.emit(opcode, [asm_arg(simd_width(FS0, 512)), asm_arg(operand)]);
            }
        } else {
            let target = self.ty(args[0]);
            self.lane_stage(args[0], FS0);
            match control {
                Some((4, _)) => {
                    self.lane_stage(args[2], FS1);
                    self.lane_stage(args[3], FS2);
                    self.avx512_stage_mask(args[1]);
                    self.emit(
                        opcode,
                        [
                            asm_arg(simd_width(FS0, 512)),
                            asm_arg(simd_width(FS1, 512)),
                            asm_arg(simd(FS2, self.ty(args[3]))),
                            asm_arg(iconst(self.func, args[4]).expect("insert lane")),
                        ],
                    );
                }
                Some((3, _)) => {
                    self.lane_stage(args[2], FS1);
                    self.avx512_stage_mask(args[1]);
                    self.emit(
                        opcode,
                        [
                            asm_arg(simd(FS0, target)),
                            asm_arg(simd(FS1, self.ty(args[2]))),
                            asm_arg(iconst(self.func, args[3]).expect("shift/extract control")),
                        ],
                    );
                }
                None if args.len() == 4 => {
                    self.lane_stage(args[2], FS1);
                    self.lane_stage(args[3], FS2);
                    self.avx512_stage_mask(args[1]);
                    self.emit(
                        opcode,
                        [
                            asm_arg(simd_width(FS0, 512)),
                            asm_arg(simd_width(FS1, 512)),
                            asm_arg(xmm(FS2)),
                        ],
                    );
                }
                None => {
                    let source = self.ty(args[2]);
                    if source.is_vector() {
                        // Block broadcasts have memory-only foundation forms.
                        // Stage the SSA block once at its exact 16/32-byte width.
                        self.lane_stage(args[2], FS1);
                        let home = sized(
                            self.copy_slot(self.copy_temp.expect("block broadcast staging")),
                            ptr(source),
                        );
                        self.emit(
                            movs(source),
                            [asm_arg(home.clone()), asm_arg(simd(FS1, source))],
                        );
                        self.avx512_stage_mask(args[1]);
                        self.emit(opcode, [asm_arg(simd_width(FS0, 512)), asm_arg(home)]);
                    } else if source.is_int() {
                        self.avx512_stage_mask(args[1]);
                        let scalar = self.use_reg(args[2], S1);
                        self.emit(
                            opcode,
                            [asm_arg(simd_width(FS0, 512)), asm_arg(reg(scalar, source))],
                        );
                    } else {
                        self.lane_stage(args[2], FS1);
                        self.avx512_stage_mask(args[1]);
                        self.emit(opcode, [asm_arg(simd_width(FS0, 512)), asm_arg(xmm(FS1))]);
                    }
                }
                _ => unreachable!("lane control metadata"),
            }
        }
        let value = result.expect("lane result");
        let target = self.ty(value);
        let destination = self.def_freg(result);
        if destination != FS0 {
            self.emit(
                movs(target),
                [
                    asm_arg(simd(destination, target)),
                    asm_arg(simd(FS0, target)),
                ],
            );
        }
        self.def_done(result, destination);
    }
}
