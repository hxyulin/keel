//! Lowering of calls, exception handling, branches, traps and returns.
use super::*;

impl FunctionCompiler<'_> {
    /// Calls, invokes and landing pads, tail calls, jumps, branches, switches,
    /// traps and returns.
    pub(super) fn control_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::Call {
                func: callee,
                ref args,
            } => {
                let decl = func.callee(callee);
                let sret = self.move_args(args, &decl.sig.params);
                self.out.branch_name(
                    true,
                    if self.linux {
                        Name::Text(&decl.name)
                    } else {
                        Name::Symbol(&decl.name)
                    },
                );
                self.call_results(inst, &decl.sig, result, sret);
            }
            // Only the call itself can throw, so the call-site entry
            // covers just the `bl`.
            InstData::Invoke {
                func: callee,
                ref args,
                ref dests,
            } => {
                let decl = func.callee(callee);
                let sret = self.move_args(args, &decl.sig.params);
                let site = format!(
                    "L{}_{inst_name}",
                    func.name,
                    inst_name = func.inst_name(inst)
                );
                self.out.label(format!("{site}_begin"));
                self.out.branch_name(
                    true,
                    if self.linux {
                        Name::Text(&decl.name)
                    } else {
                        Name::Symbol(&decl.name)
                    },
                );
                self.out.label(format!("{site}_end"));
                let pad = self.label(dests[1].block);
                self.eh.sites.push((site, pad, dests[1].block));
                self.call_results(inst, &decl.sig, result, sret);
                self.block_args(&dests[0], 0);
                self.branch_unless_next(dests[0].block);
            }
            // The unwinder enters with the exception in x0 and the selector
            // in x1, and the callee-saved registers restored.
            InstData::LandingPad { .. } => {
                let results = func.inst_results(inst);
                let (exception, selector) = (results[0], results[1]);
                if let Some(loc) = self.defined(exception) {
                    self.emit_move(loc, Loc::Reg(0));
                }
                if let Some(loc) = self.defined(selector) {
                    self.emit_move(loc, Loc::Reg(1));
                }
            }
            InstData::EhTypeid { typeinfo } => {
                let d = self.def_reg(result);
                let index = self.eh.type_index(Some(typeinfo));
                self.materialize(d, index, false);
                self.def_done(result, d);
            }
            // Arguments into registers, then this function's epilogue, then
            // a jump: the callee returns straight to our caller.
            InstData::ReturnCall {
                func: callee,
                ref args,
            } => {
                let decl = func.callee(callee);
                self.move_args(args, &decl.sig.params);
                self.epilogue();
                self.out.branch_name(
                    false,
                    if self.linux {
                        Name::Text(&decl.name)
                    } else {
                        Name::Symbol(&decl.name)
                    },
                );
            }
            InstData::ReturnCallIndirect {
                sig,
                callee,
                ref args,
            } => {
                self.move_args(args, &func.signature(sig).params);
                // The epilogue restores callee-saved registers, which may
                // hold the target, so it moves to x16 first.
                let target = self.use_reg(callee, S0);
                self.move_gp(S0, target, true);
                self.epilogue();
                self.mach("br", [(Reg::x(S0)).into()]);
            }
            InstData::CallIndirect {
                sig,
                callee,
                ref args,
            } => {
                let sret = self.move_args(args, &func.signature(sig).params);
                // The callee is in an allocated register, which the argument
                // moves leave alone (they write x0–x8, x16 and x17), or in a
                // spill slot, reloaded after the moves so x16 is free.
                let target = self.use_reg(callee, S0);
                self.mach("blr", [(Reg::x(target)).into()]);
                self.call_results(inst, func.signature(sig), result, sret);
            }
            InstData::Jump { ref dest } => {
                self.block_args(dest, 0);
                #[cfg(feature = "experimental-dispatch")]
                if !self.emit_dispatch_tail(inst) {
                    self.branch_unless_next(dest.block);
                }
                #[cfg(not(feature = "experimental-dispatch"))]
                self.branch_unless_next(dest.block);
            }
            InstData::Brif { cond, ref dests } => {
                let [then_dest, else_dest] = &**dests;
                // Take the same false edge, with the same landing pads and
                // edge copies, whether testing a register or a comparison.
                let test = if let Some(&(producer, inverted)) =
                    self.selection.overflow_branches.get(&inst)
                {
                    let InstData::Overflow { op, .. } = *func.inst_data(producer) else {
                        unreachable!("overflow branch producer")
                    };
                    let code = match op {
                        OverflowOp::Uadd => "hs",
                        OverflowOp::Usub => "lo",
                        OverflowOp::Sadd | OverflowOp::Ssub => "vs",
                        _ => unreachable!("selected add/sub overflow"),
                    };
                    BrifTest::Flags(if inverted { code } else { invert_cond(code) })
                } else {
                    match func.value_def(func.resolve(cond)) {
                        // A fused compare-and-swap left eq when it stored.
                        ValueDef::Result(cas, _)
                            if self.fused_compares.contains(&func.resolve(cond))
                                && matches!(func.inst_data(cas), InstData::AtomicCas { .. }) =>
                        {
                            BrifTest::Flags("ne")
                        }
                        ValueDef::Result(compare, _)
                            if self.fused_compares.contains(&func.resolve(cond)) =>
                        {
                            BrifTest::Flags(invert_cond(self.compare(compare)))
                        }
                        _ if !self.has_inline_asm
                            && matches!(self.ty(cond), Type::I32 | Type::I64) =>
                        {
                            let c = self.use_reg(cond, S0);
                            BrifTest::Zero(reg(c, self.ty(cond)))
                        }
                        _ => {
                            self.test_value(cond);
                            BrifTest::Flags("eq")
                        }
                    }
                };
                // Copy block arguments only on their own edge. A fallthrough
                // edge, or the only edge with moves, can copy inline;
                // otherwise the false edge needs a landing pad.
                let then_noop = self.block_args_noop(then_dest, 0);
                if then_noop
                    && (self.next_block == Some(else_dest.block)
                        || !self.block_args_noop(else_dest, 1))
                {
                    match &test {
                        BrifTest::Flags(condition) => {
                            self.conditional(
                                invert_cond(condition),
                                self.block_name(then_dest.block),
                            );
                        }
                        BrifTest::Zero(register) => {
                            self.compare_branch(*register, true, self.block_name(then_dest.block));
                        }
                    }
                    self.block_args(else_dest, 1);
                    self.branch_unless_next(else_dest.block);
                } else if self.block_args_noop(else_dest, 1) {
                    self.brif_false(&test, self.block_name(else_dest.block));
                    self.block_args(then_dest, 0);
                    self.branch_unless_next(then_dest.block);
                } else {
                    let pad = format!(
                        "L{}_{inst_name}_else",
                        func.name,
                        inst_name = func.inst_name(inst)
                    );
                    self.brif_false(&test, Name::Text(&pad));
                    self.block_args(then_dest, 0);
                    self.out
                        .branch_name(false, self.block_name(then_dest.block));
                    self.out.label(pad.clone());
                    self.block_args(else_dest, 1);
                    self.branch_unless_next(else_dest.block);
                }
            }
            InstData::Switch {
                arg,
                ref dests,
                ref cases,
            } => self.switch(inst, arg, dests, cases, None),
            InstData::Trap => self.mach("brk", [Arg::imm(1)]),
            InstData::Return { ref args } => {
                // A struct result in registers is loaded from where the
                // function wrote it.
                if let Some(Purpose::Sret(layout)) = func.sig.params.first().map(|p| &p.purpose)
                    && let Some(area) = self.param_areas[0]
                {
                    self.load_struct_regs(layout, area);
                }
                if args.len() > 1 {
                    let locs = abi::multi_result_locs(&func.sig).expect("preflight checked it");
                    let moves: Vec<_> = args
                        .iter()
                        .zip(locs)
                        .map(|(&v, dst)| (dst, self.loc(v)))
                        .collect();
                    self.parallel_copy(&moves);
                } else if let Some(&value) = args.first() {
                    let src = self.loc(value);
                    self.emit_move(return_loc(self.ty(value)), src);
                    if self.ty(value) == Type::I128 {
                        let hi = self.hi(value);
                        self.emit_move(Loc::Reg(1), hi);
                    }
                    let ret = &func.sig.returns[0];
                    self.extend_in_place(0, ret.ty, ret.ext);
                }
                self.return_epilogue();
            }
            _ => unreachable!("not control flow"),
        }
    }

    fn return_epilogue(&mut self) {
        // Return values already occupy the ABI registers. Every ordinary
        // return has the same frame restoration, independent of its value.
        if self.share_returns && !self.debug {
            let label = format!("L{}_return_epilogue", self.func.name);
            if self.return_epilogue_emitted {
                self.out.branch_name(false, Name::Text(&label));
                return;
            }
            self.out.label(label);
            self.return_epilogue_emitted = true;
        }
        self.epilogue();
        self.native(Opcode::RET_64R_branch_reg, [(FieldId::Rn, 30)], |out| {
            out.write_str("ret")
        });
    }
}
