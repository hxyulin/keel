//! Lowering of scalar constants, addresses, arithmetic, selects,
//! conversions and compares.
use super::*;

impl FunctionCompiler<'_> {
    /// Constants, and the addresses of symbols and stack slots.
    pub(super) fn constant_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::StackAddr { .. } | InstData::Iconst { .. }
                if result.is_some_and(|r| {
                    self.folded_values.contains(&r)
                        || matches!(self.defined(r), Some(Loc::Remat(_)))
                }) => {}
            InstData::Iconst { ty, imm } => {
                let d = self.def_reg(result);
                self.materialize(d, imm as u64, !fits_w(ty));
                self.def_done(result, d);
            }
            InstData::F80const { .. } => unreachable!("f80 rejected before lowering"),
            InstData::Fconst { ty, bits } => {
                let d = self.def_freg(result);
                let fd = freg(d, ty);
                let value = match ty {
                    Type::F32 => Some(f64::from(f32::from_bits(bits as u32))),
                    Type::F64 => Some(f64::from_bits(bits)),
                    _ => None,
                };
                if bits == 0 {
                    let zero = if ty == Type::F32 { "wzr" } else { "xzr" };
                    self.mach("fmov", [(fd).into(), (zero).into()]);
                } else if let Some(value) = value
                    && let Some(imm8) = fmov_imm8(value)
                {
                    self.native(
                        if ty == Type::F32 {
                            Opcode::FMOV_S_floatimm
                        } else {
                            Opcode::FMOV_D_floatimm
                        },
                        [(FieldId::Rd, d.into()), (FieldId::imm8, imm8)],
                        |out| write!(out, "fmov\t{fd}, #{value:?}"),
                    );
                } else {
                    let is64 = ty == Type::F64;
                    self.materialize(S0, bits, is64);
                    let src = if is64 { Reg::x(S0) } else { Reg::w(S0) };
                    self.mach("fmov", [(fd).into(), (src).into()]);
                }
                self.def_done(result, d);
            }
            InstData::SymbolAddr { symbol } => {
                let d = self.def_reg(result);
                let name = func.symbol_name(symbol);
                let thread_local = self
                    .symbols
                    .data_object(name)
                    .is_some_and(|d| d.thread_local);
                if thread_local && self.linux {
                    self.linux_tls_address(d, name);
                } else if thread_local {
                    // The symbol is the variable's descriptor; calling its
                    // getter with the descriptor in x0 returns the address
                    // in x0. The getter preserves every register but x0,
                    // x16, x17 and x30, none of which hold values.
                    self.out.symbolic(Symbolic::PageAddress {
                        register: 0,
                        kind: PageKind::ThreadLocal,
                        target: self.symbol(name),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: 0,
                        base: 0,
                        load: true,
                        kind: PageKind::ThreadLocal,
                        target: self.symbol(name),
                    });
                    self.mach(
                        "ldr",
                        [(Reg::x(S0)).into(), Address::base(Reg::x(0)).into()],
                    );
                    self.mach("blr", [(Reg::x(S0)).into()]);
                    self.mach("mov", [(Reg::x(d)).into(), (Reg::x(0)).into()]);
                } else if self.symbols.defines(name) && !self.symbols.exports_weak_odr(name) {
                    // dyld may coalesce an exported weak_odr definition with
                    // another image's, so code reads its address from the
                    // GOT, as an import's.
                    self.out.symbolic(Symbolic::PageAddress {
                        register: d,
                        kind: PageKind::Direct,
                        target: self.symbol(name),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: d,
                        base: d,
                        load: false,
                        kind: PageKind::Direct,
                        target: self.symbol(name),
                    });
                } else {
                    self.out.symbolic(Symbolic::PageAddress {
                        register: d,
                        kind: PageKind::Got,
                        target: self.symbol(name),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: d,
                        base: d,
                        load: true,
                        kind: PageKind::Got,
                        target: self.symbol(name),
                    });
                }
                self.def_done(result, d);
            }
            InstData::StackAddr { slot } => {
                let d = self.def_reg(result);
                let offset = self.stack_slot_offsets[slot.index()];
                // `add` takes a 12-bit immediate, optionally shifted by 12.
                self.mach(
                    "add",
                    [
                        (Reg::x(d)).into(),
                        (self.base).into(),
                        Arg::imm(offset & 0xfff),
                    ],
                );
                if offset >> 12 != 0 {
                    self.mach(
                        "add",
                        [
                            (Reg::x(d)).into(),
                            (Reg::x(d)).into(),
                            Arg::imm(offset >> 12),
                            Arg::shift("lsl", 12),
                        ],
                    );
                }
                self.def_done(result, d);
            }
            _ => unreachable!("not a constant or address"),
        }
    }

    /// Scalar arithmetic, selects, conversions and compares.
    pub(super) fn scalar_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::Convert {
                op: ConvertOp::IntToPtr,
                ty: Type::Ptr,
                ..
            } if result.is_some_and(|r| {
                matches!(self.defined(r), Some(Loc::Remat(v)) if v == r)
                    && matches!(
                        crate::regalloc::remat_recipe(self.func, r),
                        Some(crate::regalloc::RematRecipe::NullPtr)
                    )
            }) => {}
            InstData::Icmp { .. } | InstData::Fcmp { .. }
                if result.is_some_and(|r| self.fused_compares.contains(&r)) => {}
            InstData::Unary { op, arg } if op.is_float() => {
                let ty = self.ty(arg);
                let d = self.def_freg(result);
                let a = self.use_freg(arg, FS0);
                let mnemonic = match op {
                    UnaryOp::Fneg => "fneg",
                    UnaryOp::Fabs => "fabs",
                    UnaryOp::Fsqrt => "fsqrt",
                    UnaryOp::Fceil => "frintp",
                    UnaryOp::Ffloor => "frintm",
                    UnaryOp::Ftrunc => "frintz",
                    UnaryOp::Fnearest => "frintn",
                    _ => unreachable!("integer operation"),
                };
                self.mach(mnemonic, [(freg(d, ty)).into(), (freg(a, ty)).into()]);
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } => {
                let d = self.def_reg(result);
                self.unary(op, arg, d);
                self.def_done(result, d);
            }
            InstData::Binary {
                op: BinaryOp::Fcopysign,
                args,
            } => {
                // The sign bit of `args[1]` with the rest of `args[0]`,
                // spliced in general registers.
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let (bits, ra, rb) = if ty == Type::F32 {
                    (31, Reg::w(S0), Reg::w(S1))
                } else {
                    (63, Reg::x(S0), Reg::x(S1))
                };
                self.mach("fmov", [(ra).into(), (freg(a, ty)).into()]);
                self.mach("fmov", [(rb).into(), (freg(b, ty)).into()]);
                self.mach(
                    "bfxil",
                    [(rb).into(), (ra).into(), Arg::imm(0), Arg::imm(bits)],
                );
                self.mach("fmov", [(freg(d, ty)).into(), (rb).into()]);
                self.def_done(result, d);
            }
            InstData::Fma { args } => {
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let c = self.use_freg(args[2], FS2);
                let [fa, fb, fc, fd] = [a, b, c, d].map(|r| freg(r, ty));
                self.mach(
                    "fmadd",
                    [(fd).into(), (fa).into(), (fb).into(), (fc).into()],
                );
                self.def_done(result, d);
            }
            InstData::Overflow { op, args } => self.overflow(inst, op, args),
            InstData::Binary { op, args } if op.is_float() => {
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let mnemonic = match op {
                    BinaryOp::Fadd => "fadd",
                    BinaryOp::Fsub => "fsub",
                    BinaryOp::Fmul => "fmul",
                    BinaryOp::Fdiv => "fdiv",
                    BinaryOp::Fmin => "fmin",
                    BinaryOp::Fmax => "fmax",
                    _ => unreachable!("integer operation"),
                };
                let (fa, fb, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.mach(mnemonic, [(fd).into(), (fa).into(), (fb).into()]);
                self.def_done(result, d);
            }
            InstData::Binary { op, args } => {
                let d = self.def_reg(result);
                self.binary(op, args, d);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_vector() => {
                let d = self.def_freg(result);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let skip = format!("L{}_{inst}_vselect", func.name);
                self.move_vector(FS0, b);
                self.conditional("eq", Name::Text(&skip));
                self.move_vector(FS0, a);
                self.out.label(skip);
                self.move_vector(d, FS0);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_float() => {
                let ty = self.ty(args[1]);
                let d = self.def_freg(result);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let (a, b, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.mach(
                    "fcsel",
                    [(fd).into(), (a).into(), (b).into(), Arg::from("ne")],
                );
                self.def_done(result, d);
            }
            InstData::Select { args } => {
                let d = self.def_reg(result);
                let ty = self.ty(args[1]);
                let cc = if let Some(&compare) = self.selection.select_compares.get(&inst) {
                    self.compare(compare)
                } else {
                    self.test_value(args[0]);
                    "ne"
                };
                // CMP uses S0/S1 before either arm is staged. use_reg only
                // loads, moves wide constants, or computes a stack address;
                // all preserve NZCV and the other arm's reserved scratch.
                let a = self.use_reg(args[1], S1);
                let b = self.use_reg(args[2], S2);
                let (a, b, rd) = (reg(a, ty), reg(b, ty), reg(d, ty));
                self.mach("csel", [(rd).into(), (a).into(), (b).into(), Arg::from(cc)]);
                self.def_done(result, d);
            }
            InstData::Convert {
                op: op @ (ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong),
                ty,
                arg,
            } => self.vector_pairwise_long(op, ty, arg, result),
            InstData::Convert {
                op: ConvertOp::NarrowLow,
                ty,
                arg,
            } => self.vector_narrow(ty, arg, result),
            InstData::Convert {
                op:
                    op @ (ConvertOp::SwidenLow
                    | ConvertOp::UwidenLow
                    | ConvertOp::SwidenHigh
                    | ConvertOp::UwidenHigh),
                ty,
                arg,
            } => {
                self.vector_widen(op, ty, arg, result);
            }
            InstData::Convert {
                op: ConvertOp::Bitcast,
                ty,
                arg,
            } if ty.is_vector() && self.ty(arg).is_vector() => {
                self.vector_bitcast(arg, result);
            }
            InstData::Convert { op, ty, arg } if ty.is_float() || self.ty(arg).is_float() => {
                self.float_convert(op, ty, arg, result);
            }
            InstData::Convert { op, ty, arg } => {
                let d = self.def_reg(result);
                let a = self.use_reg(arg, S0);
                let from = self.ty(arg);
                match op {
                    ConvertOp::Sext => match from {
                        Type::I8 => self.mach("sxtb", [(reg(d, ty)).into(), (Reg::w(a)).into()]),
                        Type::I16 => self.mach("sxth", [(reg(d, ty)).into(), (Reg::w(a)).into()]),
                        _ => self.mach("sxtw", [(Reg::x(d)).into(), (Reg::w(a)).into()]),
                    },
                    // Writing a w register clears the upper 32 bits.
                    ConvertOp::Uext if self.zero_extended_narrow(arg) => {
                        if d != a {
                            self.move_gp(d, a, false);
                        }
                    }
                    ConvertOp::Uext => match from {
                        Type::I8 => self.mach("uxtb", [(Reg::w(d)).into(), (Reg::w(a)).into()]),
                        Type::I16 => self.mach("uxth", [(Reg::w(d)).into(), (Reg::w(a)).into()]),
                        _ => self.move_gp(d, a, false),
                    },
                    // The low bits are already right; upper bits are
                    // unspecified. A pointer is a 64-bit address already.
                    // Either way the bits just move.
                    ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                        if a != d {
                            self.move_gp(d, a, true);
                        }
                    }
                    _ => unreachable!("float conversions are lowered by float_convert"),
                }
                self.def_done(result, d);
            }
            InstData::Icmp { .. } | InstData::Fcmp { .. } => {
                let d = self.def_reg(result);
                let cc = self.compare(inst);
                self.mach("cset", [(Reg::w(d)).into(), (cc).into()]);
                self.def_done(result, d);
            }
            _ => unreachable!("not a scalar operation"),
        }
    }
}
