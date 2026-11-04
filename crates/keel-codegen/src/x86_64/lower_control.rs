//! Lowering of calls, branches, traps and returns.
use super::*;

impl FunctionCompiler<'_> {
    /// Calls, tail calls, jumps, branches, switches, traps and returns.
    pub(super) fn control_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::Call {
                func: callee,
                ref args,
            } => {
                let decl = func.callee(callee);
                let sret = self.move_args(args, &decl.sig, None, false);
                self.call_function(Opcode::Call, &decl.name);
                self.call_result(result, sret);
            }
            InstData::CallIndirect {
                sig,
                callee,
                ref args,
            } => {
                let sret = self.move_args(args, func.signature(sig), Some(callee), false);
                self.emit(Opcode::Call, [asm_arg(r64(R10))]);
                self.call_result(result, sret);
            }
            // Arguments into registers, then this function's epilogue, then
            // a jump: the callee returns straight to our caller.
            InstData::ReturnCall {
                func: callee,
                ref args,
            } => {
                let decl = func.callee(callee);
                self.move_args(args, &decl.sig, None, true);
                self.epilogue();
                self.call_function(Opcode::Jmp, &decl.name);
            }
            // The epilogue only restores callee-saved registers and rbp,
            // so the target stays in r10.
            InstData::ReturnCallIndirect {
                sig,
                callee,
                ref args,
            } => {
                self.move_args(args, func.signature(sig), Some(callee), true);
                self.epilogue();
                self.emit(Opcode::Jmp, [asm_arg(r64(R10))]);
            }
            InstData::Jump { ref dest } => {
                self.block_args(dest, 0);
                self.branch_unless_next(dest.block);
            }
            InstData::Brif {
                cond,
                dests: [ref then_dest, ref else_dest],
            } => {
                // To the else-branch when the condition is zero, or when a
                // fused compare's condition does not hold.
                let not = match func.value_def(func.resolve(cond)) {
                    ValueDef::Result(compare, _)
                        if self.fused_compares.contains(&func.resolve(cond)) =>
                    {
                        invert_cond(self.compare(compare))
                    }
                    _ => {
                        self.test_value(cond);
                        "e"
                    }
                };
                // Branch directly to a copy-free true edge when the false
                // successor is next or the false edge is the only one with
                // copies, copying its arguments inline. Otherwise keep copies
                // on their own edges with landing pads.
                let then_noop = self.block_args_noop(then_dest, 0);
                if then_noop
                    && (self.next_block == Some(else_dest.block)
                        || !self.block_args_noop(else_dest, 1))
                {
                    self.jump_to_block(Opcode::Jcc(condition(invert_cond(not))), then_dest.block);
                    self.block_args(else_dest, 1);
                    self.branch_unless_next(else_dest.block);
                } else if self.block_args_noop(else_dest, 1) {
                    self.jump_to_block(Opcode::Jcc(condition(not)), else_dest.block);
                    self.block_args(then_dest, 0);
                    self.branch_unless_next(then_dest.block);
                } else {
                    let pad = format!(".L{}_{inst}_else", func.name);
                    self.emit(Opcode::Jcc(condition(not)), [asm_arg(&pad)]);
                    self.block_args(then_dest, 0);
                    self.jump_to_block(Opcode::Jmp, then_dest.block);
                    self.out.label(pad);
                    self.block_args(else_dest, 1);
                    self.branch_unless_next(else_dest.block);
                }
            }
            InstData::Switch {
                arg,
                ref dests,
                ref cases,
            } => self.switch(inst, arg, dests, cases),
            InstData::Trap => self.emit(Opcode::Ud2, []),
            InstData::Return { ref args } => {
                if self.named.locs.first() == Some(&ArgLoc::SretX87) {
                    let area = self.param_areas[0].expect("planned x87 result object");
                    self.emit(Opcode::Fld, [asm_arg(sized(self.frame(area), 80))]);
                }
                // A struct result in registers is loaded from where the
                // function wrote it; one in memory returns its address.
                if let Some(ArgLoc::SretRegs(regs)) = self.named.locs.first().cloned() {
                    let area = self.param_areas[0].expect("planned");
                    if let Some(ty) = func.sig.params[0].layout().and_then(sysv::register_vector) {
                        self.emit(
                            movs(ty),
                            [
                                asm_arg(simd(0, ty)),
                                asm_arg(sized(self.frame(area), ptr(ty))),
                            ],
                        );
                    } else {
                        for sysv::RegPiece { offset, loc: r } in regs {
                            let at = sized(self.frame(area + offset as usize), 64);
                            match r {
                                Loc::Reg(r) => {
                                    self.emit(Opcode::Mov, [asm_arg(r64(r)), asm_arg(at)])
                                }
                                _ => self
                                    .emit(Opcode::Movsd, [asm_arg(xmm(loc_reg(r))), asm_arg(at)]),
                            }
                        }
                    }
                }
                if let Some(slot) = self.sret_ptr {
                    let at = self.frame(slot);
                    self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(sized(at, 64))]);
                }
                if let Some(&value) = args.first() {
                    let ty = self.ty(value);
                    if ty == Type::F80 {
                        self.f80_push(value);
                    } else if sysv::scalar_memory_result(&func.sig) {
                        let r = self.use_freg(value, FS0);
                        self.emit(
                            Opcode::Movsd,
                            [asm_arg(sized(memory(r64(RAX), 0), 64)), asm_arg(xmm(r))],
                        );
                    } else {
                        self.emit_move(sysv::return_loc(ty), self.loc(value));
                    }
                    if ty == Type::I128 {
                        self.emit_move(Loc::Reg(RDX), self.hi(value));
                    }
                    let ret = &func.sig.returns[0];
                    self.extend_in_place(RAX, ret.ty, ret.ext);
                }
                let wide_wrapper_return = func.sig.params.first().is_some_and(|p| {
                    matches!(p.purpose, Purpose::Sret(_))
                        && p.layout()
                            .and_then(sysv::register_vector)
                            .is_some_and(|t| t.bytes().is_some_and(|bytes| bytes >= 32))
                });
                if self.wide_vectors
                    && !wide_wrapper_return
                    && !args
                        .first()
                        .is_some_and(|&v| self.ty(v).bytes().is_some_and(|bytes| bytes >= 32))
                {
                    self.emit(Opcode::Vzeroupper, []);
                }
                if let Some(shared) = self.shared_return {
                    let label = format!(".L{}_return", self.func.name);
                    if shared == inst {
                        self.out.label(label);
                        self.epilogue();
                        self.emit(Opcode::Ret, []);
                    } else {
                        self.emit(Opcode::Jmp, [asm_arg(label)]);
                    }
                } else {
                    self.epilogue();
                    self.emit(Opcode::Ret, []);
                }
            }
            _ => unreachable!("not control flow"),
        }
    }
}
