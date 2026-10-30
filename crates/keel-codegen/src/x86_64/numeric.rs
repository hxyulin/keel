//! Explicit numerical operations. Ordinary floating multiply/add never enters
//! this module: fused rounding requires an explicit IR `fma`.
use super::*;
impl FunctionCompiler<'_> {
    pub(super) fn native_fma(&mut self, args: [Value; 3], result: Option<Value>) {
        let ty = self.ty(args[0]);
        // Capture every operand before publishing, including allocated aliases
        // and spilled inputs. All three scratch indices are reserved.
        for (arg, scratch) in args.into_iter().zip([FS0, FS1, FS2]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        let suffix = match ty {
            Type::F32 => "ss",
            Type::F64 => "sd",
            Type::Vector(s) if s.lane_type() == Type::F32 => "ps",
            Type::Vector(_) => "pd",
            _ => unreachable!("verified FMA shape"),
        };
        let name = format!("vfmadd213{suffix}");
        let op = if ty.vector().is_some_and(|shape| shape.bytes() == 64) {
            Opcode::Evex(
                keel_asm_ir::x86::evex::EvexOpcode::parse(&name).expect("AVX512F FMA opcode"),
            )
        } else {
            Opcode::parse(&name).expect("FMA opcode")
        };
        self.emit(
            op,
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(simd(FS1, ty)),
                asm_arg(simd(FS2, ty)),
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
    pub(super) fn native_f16c(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        match id {
            Intrinsic::X86F16cPhToPs128 | Intrinsic::X86F16cPhToPs256 => {
                if id == Intrinsic::X86F16cPhToPs128 {
                    let r = self.use_reg(args[0], S0);
                    self.emit(Opcode::Movq, [asm_arg(xmm(FS1)), asm_arg(r64(r))]);
                } else {
                    let r = self.use_freg(args[0], FS1);
                    if r != FS1 {
                        self.emit(Opcode::Movups, [asm_arg(xmm(FS1)), asm_arg(xmm(r))]);
                    }
                }
                let ty = self.func.value_type(result.unwrap());
                let d = self.def_freg(result);
                self.emit(
                    Opcode::parse("vcvtph2ps").unwrap(),
                    [asm_arg(simd(d, ty)), asm_arg(xmm(FS1))],
                );
                self.def_done(result, d);
            }
            Intrinsic::X86F16cPsToPh128 | Intrinsic::X86F16cPsToPh256 => {
                let r = self.use_freg(args[0], FS1);
                let mode = iconst(self.func, args[1]).expect("validated rounding");
                self.emit(
                    Opcode::parse("vcvtps2ph").unwrap(),
                    [
                        asm_arg(xmm(FS0)),
                        asm_arg(simd(r, self.ty(args[0]))),
                        asm_arg(mode),
                    ],
                );
                if id == Intrinsic::X86F16cPsToPh128 {
                    let d = self.def_reg(result);
                    self.emit(Opcode::Movq, [asm_arg(r64(d)), asm_arg(xmm(FS0))]);
                    self.def_done(result, d);
                } else {
                    let d = self.def_freg(result);
                    if d != FS0 {
                        self.emit(Opcode::Movups, [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
                    }
                    self.def_done(result, d);
                }
            }
            _ => unreachable!("F16C operation"),
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_bmi(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[0]);
        for (arg, scratch) in args.iter().copied().zip([S0, S1]) {
            let r = self.use_reg(arg, scratch);
            if r != scratch {
                self.emit(
                    Opcode::Mov,
                    [asm_arg(reg(scratch, ty)), asm_arg(reg(r, ty))],
                );
            }
        }
        let name = id.info().name;
        let name = if name.contains("bextr") {
            "bextr"
        } else if name.contains("pdep") {
            "pdep"
        } else {
            "pext"
        };
        self.emit(
            Opcode::parse(name).unwrap(),
            [
                asm_arg(reg(S0, ty)),
                asm_arg(reg(S0, ty)),
                asm_arg(reg(S1, ty)),
            ],
        );
        let d = self.def_reg(result);
        if d != S0 {
            self.emit(Opcode::Mov, [asm_arg(reg(d, ty)), asm_arg(reg(S0, ty))]);
        }
        self.def_done(result, d);
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_raw_fp_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let source = self.ty(args[0]);
        let target = self.func.value_type(result.unwrap());
        let opcode = Opcode::parse(id.x86_raw_fp_opcode().unwrap()).unwrap();
        if source.is_int() {
            let r = self.use_reg(args[0], S0);
            self.emit(
                Opcode::Vxorps,
                [asm_arg(xmm(FS0)), asm_arg(xmm(FS0)), asm_arg(xmm(FS0))],
            );
            self.emit(
                opcode,
                [
                    asm_arg(xmm(FS0)),
                    asm_arg(xmm(FS0)),
                    asm_arg(reg(r, source)),
                ],
            );
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(target), [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
            }
            self.def_done(result, d);
        } else if target.is_int() {
            let r = self.use_freg(args[0], FS1);
            let d = self.def_reg(result);
            self.emit(opcode, [asm_arg(reg(d, target)), asm_arg(xmm(r))]);
            self.def_done(result, d);
        } else {
            let r = self.use_freg(args[0], FS1);
            let d = self.def_freg(result);
            if source.is_float() {
                self.emit(opcode, [asm_arg(xmm(d)), asm_arg(xmm(r)), asm_arg(xmm(r))]);
            } else {
                self.emit(opcode, [asm_arg(simd(d, target)), asm_arg(simd(r, source))]);
            }
            self.def_done(result, d);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_fp_compare(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let ty = self.ty(args[0]);
        for (arg, scratch) in args[..2].iter().copied().zip([FS1, FS2]) {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        self.emit(
            Opcode::parse(id.x86_fp_compare_opcode().unwrap()).unwrap(),
            [
                asm_arg(simd(FS0, ty)),
                asm_arg(simd(FS1, ty)),
                asm_arg(simd(FS2, ty)),
                asm_arg(iconst(self.func, args[2]).unwrap()),
            ],
        );
        let target = self.func.value_type(result.unwrap());
        if target.is_int() {
            let d = self.def_reg(result);
            self.emit(
                if target == Type::I32 {
                    Opcode::Movd
                } else {
                    Opcode::Movq
                },
                [asm_arg(reg(d, target)), asm_arg(xmm(FS0))],
            );
            self.def_done(result, d);
        } else {
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(
                    movs(target),
                    [asm_arg(simd(d, target)), asm_arg(simd(FS0, target))],
                );
            }
            self.def_done(result, d);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_pure_bits(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let ty = self.ty(args[0]);
        let name = id
            .x86_pure_bits_opcode()
            .or_else(|| id.x86_legacy_sign_mask_opcode())
            .or_else(|| id.is_x86_legacy_ptest().then_some("ptest"))
            .unwrap();
        let opcode = Opcode::parse(name).unwrap();
        for (arg, scratch) in args.iter().copied().zip([FS1, FS2]) {
            let t = self.ty(arg);
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(t), [asm_arg(simd(scratch, t)), asm_arg(simd(r, t))]);
            }
        }
        if name.starts_with("vperm") {
            self.emit(
                opcode,
                [
                    asm_arg(simd(FS0, ty)),
                    asm_arg(simd(
                        if matches!(name, "vpermd" | "vpermps") {
                            FS2
                        } else {
                            FS1
                        },
                        ty,
                    )),
                    asm_arg(simd(
                        if matches!(name, "vpermd" | "vpermps") {
                            FS1
                        } else {
                            FS2
                        },
                        ty,
                    )),
                ],
            );
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
            }
            self.def_done(result, d);
        } else if name.contains("movmsk") {
            let d = self.def_reg(result);
            self.emit(opcode, [asm_arg(r32(d)), asm_arg(simd(FS1, ty))]);
            self.def_done(result, d);
        } else {
            self.emit(opcode, [asm_arg(simd(FS1, ty)), asm_arg(simd(FS2, ty))]);
            // Capture both flags before any ALU instruction changes them.
            self.emit(Opcode::Setcc(2), [asm_arg(r8(S0))]);
            self.emit(Opcode::Setcc(4), [asm_arg(r8(S1))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S0)), asm_arg(r8(S0))]);
            self.emit(Opcode::Movzx, [asm_arg(r32(S1)), asm_arg(r8(S1))]);
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(6)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
            }
            self.def_done(result, d);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_fp_misc(&mut self, id: Intrinsic, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[0]);
        let name = id
            .x86_fp_misc_opcode()
            .or_else(|| id.x86_fp_horizontal_dot_opcode())
            .unwrap();
        let opcode = Opcode::parse(name).unwrap();
        for (arg, scratch) in args
            .iter()
            .copied()
            .filter(|&v| !self.ty(v).is_int())
            .zip([FS1, FS2])
            .collect::<Vec<_>>()
        {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(simd(scratch, ty)), asm_arg(simd(r, ty))]);
            }
        }
        if name.contains("comis") {
            self.emit(opcode, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
            // Preserve all three result flags before an ALU instruction changes them.
            for (cc, r) in [(2, S0), (10, S1), (4, S2)] {
                self.emit(Opcode::Setcc(cc), [asm_arg(r8(r))]);
            }
            for r in [S0, S1, S2] {
                self.emit(Opcode::Movzx, [asm_arg(r32(r)), asm_arg(r8(r))]);
            }
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(2)]);
            self.emit(Opcode::Shl, [asm_arg(r32(S2)), asm_arg(6)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S2))]);
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
            }
            self.def_done(result, d);
            return;
        }
        let mut operands = vec![asm_arg(simd(FS0, ty))];
        if args.len() == 2 && !id.has_x86_round_control() || id.has_x86_dot_control() {
            operands.extend([asm_arg(simd(FS1, ty)), asm_arg(simd(FS2, ty))]);
        } else {
            if ty.is_float() {
                operands.push(asm_arg(xmm(FS1)));
            }
            operands.push(asm_arg(simd(FS1, ty)));
        }
        if id.has_x86_dot_control() {
            operands.push(asm_arg(iconst(self.func, args[2]).unwrap()));
        }
        if id.has_x86_round_control() {
            operands.push(asm_arg(iconst(self.func, args[1]).unwrap()));
        }
        match operands.len() {
            2 => self.emit(opcode, <[AsmOperand; 2]>::try_from(operands).unwrap()),
            3 => self.emit(opcode, <[AsmOperand; 3]>::try_from(operands).unwrap()),
            4 => self.emit(opcode, <[AsmOperand; 4]>::try_from(operands).unwrap()),
            _ => unreachable!(),
        }
        let d = self.def_freg(result);
        if d != FS0 {
            self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(simd(FS0, ty))]);
        }
        self.def_done(result, d);
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_legacy_raw_fp_convert(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let source = self.ty(args[0]);
        let target = self.func.value_type(result.unwrap());
        let opcode = Opcode::parse(id.x86_legacy_raw_fp_opcode().unwrap()).unwrap();
        if source.is_int() {
            let r = self.use_reg(args[0], S0);
            self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]);
            self.emit(opcode, [asm_arg(xmm(FS0)), asm_arg(reg(r, source))]);
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(target), [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
            }
            self.def_done(result, d);
        } else if target.is_int() {
            let r = self.use_freg(args[0], FS1);
            let d = self.def_reg(result);
            self.emit(opcode, [asm_arg(reg(d, target)), asm_arg(xmm(r))]);
            self.def_done(result, d);
        } else {
            let r = self.use_freg(args[0], FS1);
            // Legacy scalar conversion has a destructive merge destination. It
            // is reserved scratch; no live source or allocated destination is
            // overwritten while its input is being read.
            self.emit(Opcode::Xorps, [asm_arg(xmm(FS0)), asm_arg(xmm(FS0))]);
            self.emit(opcode, [asm_arg(xmm(FS0)), asm_arg(xmm(r))]);
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(target), [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
            }
            self.def_done(result, d);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_legacy_fp_misc(
        &mut self,
        id: Intrinsic,
        args: &[Value],
        result: Option<Value>,
    ) {
        let ty = self.ty(args[0]);
        let compare = id.is_x86_legacy_fp_compare();
        let name = id
            .x86_legacy_fp_misc_opcode()
            .or_else(|| id.x86_legacy_fp_compare_opcode())
            .or_else(|| id.x86_legacy_fp_v2_opcode())
            .unwrap();
        let opcode = Opcode::parse(name).unwrap();
        for (arg, scratch) in args
            .iter()
            .copied()
            .take(if compare {
                2
            } else {
                id.x86_legacy_fp_v2_control().map_or(args.len(), |c| c.0)
            })
            .zip([FS1, FS2])
        {
            let r = self.use_freg(arg, scratch);
            if r != scratch {
                self.emit(movs(ty), [asm_arg(xmm(scratch)), asm_arg(xmm(r))]);
            }
        }
        if name.contains("comis") {
            self.emit(opcode, [asm_arg(xmm(FS1)), asm_arg(xmm(FS2))]);
            for (cc, r) in [(2, S0), (10, S1), (4, S2)] {
                self.emit(Opcode::Setcc(cc), [asm_arg(r8(r))]);
            }
            for r in [S0, S1, S2] {
                self.emit(Opcode::Movzx, [asm_arg(r32(r)), asm_arg(r8(r))]);
            }
            self.emit(Opcode::Shl, [asm_arg(r32(S1)), asm_arg(2)]);
            self.emit(Opcode::Shl, [asm_arg(r32(S2)), asm_arg(6)]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S1))]);
            self.emit(Opcode::Or, [asm_arg(r32(S0)), asm_arg(r32(S2))]);
            let d = self.def_reg(result);
            if d != S0 {
                self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
            }
            self.def_done(result, d);
            return;
        }
        // All legacy two-address operations consume only scratch copies, even
        // when source and result allocation or the two SSA inputs alias.
        self.emit(movs(ty), [asm_arg(xmm(FS0)), asm_arg(xmm(FS1))]);
        if let Some((index, _)) = id.x86_legacy_fp_v2_control() {
            self.emit(
                opcode,
                [
                    asm_arg(xmm(FS0)),
                    asm_arg(xmm(if index == 1 { FS1 } else { FS2 })),
                    asm_arg(iconst(self.func, args[index]).unwrap()),
                ],
            );
        } else if compare {
            self.emit(
                opcode,
                [
                    asm_arg(xmm(FS0)),
                    asm_arg(xmm(FS2)),
                    asm_arg(iconst(self.func, args[2]).unwrap()),
                ],
            );
        } else {
            self.emit(
                opcode,
                [
                    asm_arg(xmm(FS0)),
                    asm_arg(xmm(if args.len() == 2 { FS2 } else { FS1 })),
                ],
            );
        }
        let target = self.func.value_type(result.unwrap());
        if target.is_int() {
            let d = self.def_reg(result);
            self.emit(
                if target == Type::I32 {
                    Opcode::Movd
                } else {
                    Opcode::Movq
                },
                [asm_arg(reg(d, target)), asm_arg(xmm(FS0))],
            );
            self.def_done(result, d);
        } else {
            let d = self.def_freg(result);
            if d != FS0 {
                self.emit(movs(target), [asm_arg(xmm(d)), asm_arg(xmm(FS0))]);
            }
            self.def_done(result, d);
        }
    }
}

impl FunctionCompiler<'_> {
    pub(super) fn native_crc32(&mut self, args: &[Value], result: Option<Value>) {
        let ty = self.ty(args[1]);
        let seed = self.use_reg(args[0], S0);
        if seed != S0 {
            self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(r32(seed))]);
        } else {
            self.emit(Opcode::Mov, [asm_arg(r32(S0)), asm_arg(r32(S0))]);
        }
        let source = self.use_reg(args[1], S1);
        if source != S1 {
            self.emit(
                Opcode::Mov,
                [asm_arg(reg(S1, ty)), asm_arg(reg(source, ty))],
            );
        }
        self.emit(
            Opcode::parse("crc32").unwrap(),
            [
                asm_arg(reg(
                    S0,
                    if ty == Type::I64 {
                        Type::I64
                    } else {
                        Type::I32
                    },
                )),
                asm_arg(reg(S1, ty)),
            ],
        );
        let d = self.def_reg(result);
        if d != S0 {
            self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(S0))]);
        }
        self.def_done(result, d);
    }
}
