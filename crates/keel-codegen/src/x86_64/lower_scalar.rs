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
                self.materialize(d, imm, ty);
                self.def_done(result, d);
            }
            InstData::F80const {
                significand,
                sign_exp,
            } => self.f80_constant(result, significand, sign_exp),
            InstData::Fconst { bits, .. } => {
                let d = self.def_freg(result);
                if bits == 0 {
                    self.emit(Opcode::Xorps, [asm_arg(xmm(d)), asm_arg(xmm(d))]);
                } else {
                    self.emit(Opcode::Movabs, [asm_arg(r64(0)), asm_arg(bits)]);
                    self.emit(Opcode::Movq, [asm_arg(xmm(d)), asm_arg(r64(0))]);
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
                // A thread's copies of the thread-locals are at fixed
                // offsets from its thread pointer, the base of fs, whose
                // first word points to itself. An executable knows the
                // offsets of its own at link time (local-exec); others come
                // from the GOT (initial-exec).
                let defined = self.symbols.defines(name);
                // Intel operand syntax reserves names such as `offset`, `ptr`
                // and register names. Quoting keeps C symbols unambiguous.
                let name = name.to_owned();
                if !thread_local && self.code_model != CodeModel::PositionIndependent {
                    let destination = match self.code_model {
                        CodeModel::Small => r32(d),
                        CodeModel::Kernel => r64(d),
                        CodeModel::PositionIndependent => unreachable!("static address branch"),
                    };
                    self.emit(
                        Opcode::Mov,
                        [
                            asm_arg(destination),
                            asm_arg(symbol_address(&name, Modifier::None)),
                        ],
                    );
                } else if thread_local && defined {
                    self.emit(
                        Opcode::Mov,
                        [asm_arg(r64(d)), asm_arg(sized(fs_zero(), 64))],
                    );
                    self.emit(
                        Opcode::Lea,
                        [
                            asm_arg(r64(d)),
                            asm_arg(symbol_memory(r64(d), &name, Modifier::TpOff)),
                        ],
                    );
                } else if thread_local {
                    self.emit(
                        Opcode::Mov,
                        [
                            asm_arg(r64(d)),
                            asm_arg(sized(symbol_memory(rip(), &name, Modifier::GotTpOff), 64)),
                        ],
                    );
                    self.emit(
                        Opcode::Add,
                        [asm_arg(r64(d)), asm_arg(sized(fs_zero(), 64))],
                    );
                } else if defined {
                    self.emit(
                        Opcode::Lea,
                        [
                            asm_arg(r64(d)),
                            asm_arg(symbol_memory(rip(), &name, Modifier::None)),
                        ],
                    );
                } else {
                    self.emit(
                        Opcode::Mov,
                        [
                            asm_arg(r64(d)),
                            asm_arg(sized(symbol_memory(rip(), &name, Modifier::GotPcRel), 64)),
                        ],
                    );
                }
                self.def_done(result, d);
            }
            InstData::StackAddr { slot } => {
                let d = self.def_reg(result);
                let at = self.frame(self.stack_slot_offsets[slot.index()]);
                self.emit(Opcode::Lea, [asm_arg(r64(d)), asm_arg(at)]);
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
                let d = self.def_freg(result);
                self.float_unary(inst, op, arg, d);
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } => {
                let d = self.def_reg(result);
                self.unary(op, arg, d);
                self.def_done(result, d);
            }
            InstData::Fma { args } if native_fma_legal(self.ty(args[0]), self.features) => {
                self.native_fma(args, result)
            }
            InstData::Fma { args } => {
                let ty = self.ty(args[0]);
                let moves: Vec<(Loc, Loc)> = (0..3)
                    .map(|i| (Loc::FReg(i as u8), self.loc(args[i])))
                    .collect();
                self.parallel_copy(&moves);
                let name = if ty == Type::F32 { "fmaf" } else { "fma" };
                self.emit(Opcode::Call, [asm_arg(symbol_address(name, Modifier::Plt))]);
                self.call_result(result, None);
            }
            InstData::Overflow { op, args } => self.overflow(inst, op, args),
            InstData::Binary { op, args } if op.is_float() => {
                let d = self.def_freg(result);
                self.float_binary(inst, op, args, d);
                self.def_done(result, d);
            }
            InstData::Binary { .. } if self.selection.division_remainders.contains_key(&inst) => {
                let d = self.def_reg(result);
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S2))]);
                self.def_done(result, d);
            }
            InstData::Binary { op, args } => {
                let d = self.def_reg(result);
                self.binary(inst, op, args, d);
                self.def_done(result, d);
            }
            InstData::Select { args }
                if self.ty(args[1]).is_float() || self.ty(args[1]).is_vector() =>
            {
                let d = self.def_freg(result);
                let skip = format!(".L{}_{inst}_select", func.name);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                self.emit_move(Loc::FReg(FS0), Loc::FReg(b));
                self.emit(Opcode::Jcc(4), [asm_arg(&skip)]);
                self.emit_move(Loc::FReg(FS0), Loc::FReg(a));
                self.out.label(skip);
                self.emit_move(Loc::FReg(d), Loc::FReg(FS0));
                self.def_done(result, d);
            }
            // The false value in rax, replaced by the true one if the
            // condition is non-zero; the moves leave the flags alone.
            InstData::Select { args } => {
                let d = self.def_reg(result);
                if let Some(&compare) = self.selection.select_compares.get(&inst) {
                    // Stage arms outside the compare's rax/rcx scratch. It
                    // preserves these
                    // two reserved registers even when every operand spills.
                    self.emit_move(Loc::Reg(S2), self.loc(args[2]));
                    self.emit_move(Loc::Reg(S3), self.loc(args[1]));
                    let cc = self.compare(compare);
                    self.emit(
                        Opcode::Cmovcc(condition(cc)),
                        [asm_arg(r64(S2)), asm_arg(r64(S3))],
                    );
                    self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(S2))]);
                    self.def_done(result, d);
                    return;
                }
                self.test_value(args[0]);
                let a = self.use_reg(args[1], S1);
                let b = self.use_reg(args[2], S2);
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(b))]);
                self.emit(Opcode::Cmovcc(5), [asm_arg(r64(0)), asm_arg(r64(a))]);
                self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(0))]);
                self.def_done(result, d);
            }
            InstData::Convert { op, ty, arg } if ty.is_vector() && self.ty(arg).is_vector() => {
                self.vector_convert(op, ty, arg, result);
            }
            InstData::Convert { op, ty, arg } if ty.is_float() || self.ty(arg).is_float() => {
                self.float_convert(inst, op, ty, arg, result);
            }
            InstData::Convert { op, ty, arg } => {
                let d = self.def_reg(result);
                let a = self.use_reg(arg, S0);
                let from = self.ty(arg);
                match op {
                    ConvertOp::Sext => match from {
                        Type::I32 => self.emit(Opcode::Movsxd, [asm_arg(r64(d)), asm_arg(r32(a))]),
                        _ => {
                            self.emit(Opcode::Movsx, [asm_arg(wide(d, ty)), asm_arg(reg(a, from))])
                        }
                    },
                    // Writing a 32-bit register clears the upper 32 bits.
                    ConvertOp::Uext
                        if self.zero_extended_narrow(arg) || self.zero_extended_i32(arg) =>
                    {
                        if d != a {
                            self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(a))]);
                        }
                    }
                    ConvertOp::Uext => match from {
                        Type::I32 => self.emit(Opcode::Mov, [asm_arg(r32(d)), asm_arg(r32(a))]),
                        _ => self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(reg(a, from))]),
                    },
                    // The low bits are already right; upper bits are
                    // unspecified. A pointer is a 64-bit address already.
                    ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                        if a != d {
                            self.emit(Opcode::Mov, [asm_arg(r64(d)), asm_arg(r64(a))]);
                        }
                    }
                    _ => unreachable!("float conversions are lowered by float_convert"),
                }
                self.def_done(result, d);
            }
            InstData::Icmp { .. } | InstData::Fcmp { .. } => {
                let d = self.def_reg(result);
                let cc = self.compare(inst);
                if matches!(*func.inst_data(inst), InstData::Fcmp { cond: FloatCC::Eq | FloatCC::Ne, args } if func.resolve(args[0]) != func.resolve(args[1]))
                {
                    // Eq/Ne already combined the two flags into canonical al.
                    if d != S0 {
                        self.emit(Opcode::Mov, [asm_arg(r8(d)), asm_arg(r8(S0))]);
                    }
                } else {
                    self.emit(Opcode::Setcc(condition(cc)), [asm_arg(r8(d))]);
                }
                self.def_done(result, d);
            }
            _ => unreachable!("not a scalar operation"),
        }
    }
}
