//! Destructive hardware destination/mask are reserved scratch views; the IR
//! passthrough, indices and mask remain normal immutable SSA inputs.
use super::*;
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
        let name = id.info().name.strip_prefix("x86_avx2_").unwrap();
        let name = name
            .strip_suffix("128")
            .or_else(|| name.strip_suffix("256"))
            .unwrap();
        self.emit(
            Opcode::parse(name).unwrap(),
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(sized(at, bits)),
                asm_arg(simd(FS2, self.ty(args[3]))),
            ],
        );
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
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
        let name = id.info().name;
        let op_name = name
            .strip_prefix("x86_avx_")
            .or_else(|| name.strip_prefix("x86_avx2_"))
            .unwrap()
            .split('_')
            .next()
            .unwrap();
        let opcode = Opcode::parse(op_name).unwrap();
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
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
            }
            self.def_done(result, d);
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
            Opcode::parse(if id == Intrinsic::X86SseMaskmovdqu {
                "maskmovdqu"
            } else {
                "vmaskmovdqu"
            })
            .unwrap(),
            [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))],
        );
        self.emit(Opcode::Sfence, []);
        self.emit(Opcode::Mov, [asm_arg(r64(RDI)), asm_arg(r64(S2))]);
    }
}
