//! Destructive hardware destination/mask are reserved scratch views; the IR
//! passthrough, indices and mask remain normal immutable SSA inputs.
use super::*;

fn opcode(id: Intrinsic) -> Opcode {
    use Intrinsic::*;
    match id {
        X86Avx2Vgatherdps128 | X86Avx2Vgatherdps256 => op::VGATHERDPS,
        X86Avx2Vgatherdpd128 | X86Avx2Vgatherdpd256 => op::VGATHERDPD,
        X86Avx2Vgatherqps128 | X86Avx2Vgatherqps256 => op::VGATHERQPS,
        X86Avx2Vgatherqpd128 | X86Avx2Vgatherqpd256 => op::VGATHERQPD,
        X86Avx2Vpgatherdd128 | X86Avx2Vpgatherdd256 => op::VPGATHERDD,
        X86Avx2Vpgatherdq128 | X86Avx2Vpgatherdq256 => op::VPGATHERDQ,
        X86Avx2Vpgatherqd128 | X86Avx2Vpgatherqd256 => op::VPGATHERQD,
        X86Avx2Vpgatherqq128 | X86Avx2Vpgatherqq256 => op::VPGATHERQQ,
        _ => unreachable!("AVX2 gather intrinsic"),
    }
}

fn masked_opcode(id: Intrinsic) -> Opcode {
    use Intrinsic::*;
    match id {
        X86AvxVmaskmovpsLoad128
        | X86AvxVmaskmovpsStore128
        | X86AvxVmaskmovpsLoad256
        | X86AvxVmaskmovpsStore256 => op::VMASKMOVPS,
        X86AvxVmaskmovpdLoad128
        | X86AvxVmaskmovpdStore128
        | X86AvxVmaskmovpdLoad256
        | X86AvxVmaskmovpdStore256 => op::VMASKMOVPD,
        X86Avx2VpmaskmovdLoad128
        | X86Avx2VpmaskmovdStore128
        | X86Avx2VpmaskmovdLoad256
        | X86Avx2VpmaskmovdStore256 => op::VPMASKMOVD,
        X86Avx2VpmaskmovqLoad128
        | X86Avx2VpmaskmovqStore128
        | X86Avx2VpmaskmovqLoad256
        | X86Avx2VpmaskmovqStore256 => op::VPMASKMOVQ,
        _ => unreachable!("AVX masked memory intrinsic"),
    }
}
impl FunctionCompiler<'_> {
    pub(super) fn native_gather(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        for (arg, scratch) in [args[1], args[2], args[3]].into_iter().zip([FS1, FS0, FS2]) {
            let ty = self.ty(arg);
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        let base = self.use_reg(args[0], S1);
        let mut at = memory(r64(base), 0);
        at.index = Some(simd(FS1, self.ty(args[1])));
        at.scale = iconst(self.func, args[4]).unwrap() as u8;
        let ty = self.ty(args[2]);
        let bits = (ty.vector().unwrap().lane_type().bytes().unwrap() * 8) as u16;
        self.emit(
            opcode(id),
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(sized(at, bits)),
                asm_arg(simd(FS2, self.ty(args[3]))),
            ],
        );
        self.def_vector_from_fs0(result, ty);
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_masked_mem(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let store = id.is_x86_masked_store();
        let mask = args[if store { 2 } else { 1 }];
        let mt = self.ty(mask);
        let r = self.use_freg(mask, FS1);
        if r != FS1 {
            self.emit(movs(mt), [asm_arg(simd(FS1, mt)), asm_arg(simd(r, mt))]);
        }
        let ty = if store {
            self.ty(args[1])
        } else {
            self.func.value_type(result.unwrap())
        };
        if store {
            let r = self.use_freg(args[1], FS0);
            if r != FS0 {
                self.emit(movs(ty), [asm_arg(simd(FS0, ty)), asm_arg(simd(r, ty))]);
            }
        }
        let base = self.use_reg(args[0], S1);
        let at = sized(memory(r64(base), 0), ptr(ty));
        let opcode = masked_opcode(id);
        if store {
            self.emit(
                opcode,
                [asm_arg(at), asm_arg(simd(FS1, mt)), asm_arg(simd(FS0, ty))],
            );
        } else {
            self.emit(
                opcode,
                [asm_arg(simd(FS0, ty)), asm_arg(simd(FS1, mt)), asm_arg(at)],
            );
            self.def_vector_from_fs0(result, ty);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_byte_mask_store(&mut self, id: Intrinsic, args: &[Value]) {
        let ty = self.ty(args[1]);
        for (arg, scratch) in [args[1], args[2]].into_iter().zip([FS1, FS2]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(xmm(scratch)), asm_arg(xmm(r))]);
            }
        }
        let base = self.use_reg(args[0], S0);
        if base != S0 {
            self.emit(Opcode::Mov, [asm_arg(r64(S0)), asm_arg(r64(base))]);
        }
        // RDI is an implicit address input, but may contain an unrelated live SSA
        // value. All explicit inputs are staged before temporarily replacing it.
        self.emit(Opcode::Mov, [asm_arg(r64(S2)), asm_arg(r64(RDI))]);
        self.emit(Opcode::Mov, [asm_arg(r64(RDI)), asm_arg(r64(S0))]);
        self.emit(
            if id == Intrinsic::X86SseMaskmovdqu {
                Opcode::Maskmovdqu
            } else {
                op::VMASKMOVDQU
            },
            [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
        );
        self.emit(Opcode::Sfence, []);
        self.emit(Opcode::Mov, [asm_arg(r64(RDI)), asm_arg(r64(S2))]);
    }
}
