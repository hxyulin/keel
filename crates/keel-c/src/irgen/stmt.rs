//! Statement lowering.

use super::*;

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    pub(super) fn stmt(&mut self, s: &Stmt<'p>) -> R<()> {
        // A loop's condition and step run again too, statement
        // expressions in them included.
        let looping = matches!(s, Stmt::While(..) | Stmt::DoWhile(..) | Stmt::For { .. });
        self.loop_depth += u32::from(looping);
        let result = self.stmt_kind(s);
        self.loop_depth -= u32::from(looping);
        result
    }

    fn stmt_kind(&mut self, s: &Stmt<'p>) -> R<()> {
        match s {
            Stmt::Asm(asm) => {
                self.ensure_block();
                self.set_loc(asm.loc);
                if !asm.goto_labels.is_empty() && self.has_vm_types() {
                    return Err(Error::new(
                        asm.loc,
                        "asm goto in a function containing VLAs is not supported",
                    ));
                }
                let mut places = Vec::new();
                let mut readwrite = Vec::new();
                let mut output_inputs = Vec::new();
                for ((e, rw), &kind) in asm.outputs.iter().zip(asm.output_kinds) {
                    if kind != AsmOutputKind::Register {
                        // Memory contributes an address; RSP contributes only
                        // an observer dependency, never an SSA result/writeback.
                        // Later operand evaluations may change the live RSP.
                        output_inputs.push(self.val(e)?);
                        continue;
                    }
                    // Capture once, before inputs: postincrement and calls in
                    // an output address must not run again on writeback.
                    let place = self.place(e)?.expect("checked asm output");
                    if *rw {
                        readwrite.push(self.load_place(&place));
                    }
                    places.push(place);
                }
                let mut args = output_inputs;
                for e in asm.inputs {
                    args.push(self.val(e)?);
                }
                args.extend(readwrite);
                let mut types: Vec<_> = asm
                    .outputs
                    .iter()
                    .zip(asm.output_kinds)
                    .filter(|(_, kind)| **kind == AsmOutputKind::Register)
                    .map(|((e, _), _)| e.ty.ir_with_layout(self.g.p.c_layout))
                    .collect();
                if !asm.goto_labels.is_empty() {
                    types.insert(0, Type::I32);
                }
                let contract = keel_ir::InlineAsm::new(asm.template, asm.constraints, &types)
                    .map_err(|e| Error::new(asm.loc, e))?;
                let inst = self.b.ins().inline_asm(contract, &args);
                let values = self.b.function().inst_results(inst).to_vec();
                if asm.goto_labels.is_empty() {
                    for (place, value) in places.iter().zip(values) {
                        self.store_place(place, value);
                    }
                } else {
                    // Every template edge joins the restoration trampoline.
                    // Write back its real outputs before branching to C labels.
                    for (place, &value) in places.iter().zip(&values[1..]) {
                        self.store_place(place, value);
                    }
                    let fallthrough = self.new_block();
                    let call = |block| keel_ir::BlockCall {
                        block,
                        args: Vec::new(),
                    };
                    let cases = asm
                        .goto_labels
                        .iter()
                        .enumerate()
                        .map(|(i, &label)| {
                            self.asm_goto_targets.push((label, asm.loc));
                            (i as i64 + 1, call(self.label_block(label)))
                        })
                        .collect();
                    self.b.ins().switch(values[0], call(fallthrough), cases);
                    self.switch_to(fallthrough);
                }
            }
            Stmt::Expr(e) => {
                self.ensure_block();
                self.set_loc(e.loc);
                self.full_expression(|fg| fg.discard(e))?;
            }
            Stmt::Init(id, init, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                self.full_expression(|fg| fg.init_local(*id, init))?;
                self.register_cleanup(*id);
            }
            // Without this, a variable read on a path that did not assign it
            // in this iteration would read the last iteration's value: a
            // block parameter live around the whole loop. Zero starts a new
            // value here instead. A declaration that runs at most once, in
            // no loop and with no label before it, needs no zero: a read
            // that no assignment reaches already reads as zero.
            Stmt::Declare(id) => {
                let ty = self.def.locals[*id].ty;
                if ty.runtime_sized() {
                    self.ensure_block();
                    let bytes = self.runtime_size(&ty)?;
                    let save = self.b.ins().intrinsic(Intrinsic::StackSave, &[]);
                    let token = self.b.function().inst_results(save)[0];
                    self.exit_actions.push(ExitAction::Restore(token));
                    let inst = self.b.ins().intrinsic(Intrinsic::StackAlloc, &[bytes]);
                    let addr = self.b.function().inst_results(inst)[0];
                    let Storage::Var(var) = self.locals[*id] else {
                        unreachable!()
                    };
                    self.b.def_var(var, addr);
                    self.alignments
                        .insert(addr, self.def.locals[*id].align as u32);
                    self.register_cleanup(*id);
                    return Ok(());
                }
                if let Storage::Var(var) = self.locals[*id]
                    && (self.loop_depth > 0 || self.labels_placed)
                {
                    self.ensure_block();
                    let zero = self.zero(self.def.locals[*id].ty.ir_with_layout(self.g.p.c_layout));
                    self.b.def_var(var, zero);
                }
                self.register_cleanup(*id);
            }
            Stmt::Block(stmts) => {
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
            }
            Stmt::Scoped(scope, stmts) => {
                let depth = self.exit_actions.len();
                let outer = self.scope;
                self.set_scope(self.scopes.get(*scope).copied().flatten());
                for s in stmts.iter() {
                    self.stmt(s)?;
                }
                self.set_scope(outer);
                self.leave_scope(depth)?;
            }
            Stmt::If(cond, then, els) => {
                self.ensure_block();
                self.set_loc(cond.loc);
                let (t, f, end) = (self.new_block(), self.new_block(), self.new_block());
                self.branch_full(cond, t, f)?;
                self.b.seal_block(t);
                self.b.seal_block(f);
                self.switch_to(t);
                self.stmt(then)?;
                self.jump_to(end);
                self.switch_to(f);
                match els {
                    Some(els) => self.stmt(els)?,
                    // The jump past the `then` branch is the condition's,
                    // not the last line of that branch's.
                    None => self.set_loc(cond.loc),
                }
                self.jump_to(end);
                self.b.seal_block(end);
                self.switch_to(end);
            }
            Stmt::While(cond, body) => {
                self.ensure_block();
                let (header, body_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.jump_to(header);
                self.switch_to(header);
                self.set_loc(cond.loc);
                self.branch_full(cond, body_block, exit)?;
                self.b.seal_block(body_block);
                self.loop_body(body, body_block, exit, header)?;
                self.jump_to(header);
                self.b.seal_block(header);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::DoWhile(body, cond) => {
                self.ensure_block();
                let (body_block, cond_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.jump_to(body_block);
                self.loop_body(body, body_block, exit, cond_block)?;
                self.jump_to(cond_block);
                self.b.seal_block(cond_block);
                self.switch_to(cond_block);
                self.set_loc(cond.loc);
                self.branch_full(cond, body_block, exit)?;
                self.b.seal_block(body_block);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(init) = init {
                    self.stmt(init)?;
                }
                self.ensure_block();
                let (header, body_block, step_block, exit) = (
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                    self.new_block(),
                );
                self.jump_to(header);
                self.switch_to(header);
                match cond {
                    Some(cond) => {
                        self.set_loc(cond.loc);
                        self.branch_full(cond, body_block, exit)?;
                    }
                    None => self.jump_to(body_block),
                }
                self.b.seal_block(body_block);
                self.loop_body(body, body_block, exit, step_block)?;
                self.jump_to(step_block);
                self.b.seal_block(step_block);
                self.switch_to(step_block);
                if let Some(step) = step {
                    self.set_loc(step.loc);
                    self.full_expression(|fg| fg.emit(step))?;
                }
                self.jump_to(header);
                self.b.seal_block(header);
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            Stmt::Switch {
                cond,
                body,
                cases,
                default,
                labels,
            } => {
                if self.has_vm_types() {
                    return Err(Error::new(
                        cond.loc,
                        "switch in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                self.set_loc(cond.loc);
                let value = self.full_expression(|fg| fg.val(cond))?;
                let blocks: Vec<Block> = (0..*labels).map(|_| self.new_block()).collect();
                let exit = self.new_block();
                let default_block = default.map_or(exit, |d| blocks[d]);
                let ty = cond.ty.ir_with_layout(self.g.p.c_layout);
                for case in cases.iter().filter(|case| case.first != case.last) {
                    let first = self.b.ins().iconst(ty, case.first);
                    let last = self.b.ins().iconst(ty, case.last);
                    let (ge, le) = if cond.ty.is_signed() {
                        (IntCC::Sge, IntCC::Sle)
                    } else {
                        (IntCC::Uge, IntCC::Ule)
                    };
                    let above = self.b.ins().icmp(ge, value, first);
                    let below = self.b.ins().icmp(le, value, last);
                    let matches = self.b.ins().and(above, below);
                    let next = self.new_block();
                    self.b
                        .ins()
                        .brif(matches, blocks[case.label], &[], next, &[]);
                    self.terminated = true;
                    self.b.seal_block(next);
                    self.switch_to(next);
                }
                let table = cases
                    .iter()
                    .filter(|case| case.first == case.last)
                    .map(|case| {
                        let dest = keel_ir::BlockCall {
                            block: blocks[case.label],
                            args: Vec::new(),
                        };
                        (case.first, dest)
                    })
                    .collect();
                let default_call = keel_ir::BlockCall {
                    block: default_block,
                    args: Vec::new(),
                };
                self.b.ins().switch(value, default_call, table);
                self.terminated = true;
                self.cases.push(blocks.clone());
                self.breaks.push((exit, self.exit_actions.len()));
                self.stmt(body)?;
                self.jump_to(exit);
                self.breaks.pop();
                self.cases.pop();
                for block in blocks {
                    self.b.seal_block(block);
                }
                self.b.seal_block(exit);
                self.switch_to(exit);
            }
            // A case block's branches are the switch's, built already, and
            // the fall-through into it: sealing it now resolves its reads
            // directly instead of through parameters filled in later.
            Stmt::Case(label) => {
                let block = self.cases.last().unwrap()[*label];
                self.jump_to(block);
                self.b.seal_block(block);
                self.switch_to(block);
            }
            Stmt::Break => {
                self.ensure_block();
                let (target, depth) = *self.breaks.last().unwrap();
                self.emit_exit_actions(depth)?;
                self.jump_to(target);
            }
            Stmt::Continue => {
                self.ensure_block();
                let (target, depth) = *self.continues.last().unwrap();
                self.emit_exit_actions(depth)?;
                self.jump_to(target);
            }
            Stmt::Return(value, loc) => {
                self.ensure_block();
                self.set_loc(*loc);
                match value {
                    None => self.return_default()?,
                    // The return ends the temporaries, so they need no end.
                    Some(e) => self.full_expression(|fg| {
                        if matches!(fg.result, ResultMapping::Empty) {
                            fg.val(e)?;
                            fg.emit_exit_actions(0)?;
                            fg.b.ins().return_(&[]);
                        } else if let Some(sret) = fg.sret {
                            let src = fg.val(e)?;
                            let ty = e.ty;
                            fg.copy(sret, src, &ty);
                            fg.emit_exit_actions(0)?;
                            fg.b.ins().return_(&[]);
                        } else {
                            let v = fg.val(e)?;
                            fg.emit_exit_actions(0)?;
                            fg.b.ins().return_(&[v]);
                        }
                        fg.terminated = true;
                        Ok(())
                    })?,
                }
            }
            Stmt::Goto(label) => {
                if self.has_vm_types() {
                    return Err(Error::new(
                        self.def.loc,
                        "goto in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                if self.has_cleanup() {
                    self.emit_exit_actions(self.cleanup_jumps.depth(label))?;
                }
                let block = self.label_block(label);
                self.jump_to(block);
            }
            Stmt::GotoIndirect(target) => {
                if self.has_vm_types() {
                    return Err(Error::new(
                        target.loc,
                        "computed goto in a function containing VLAs is not supported yet",
                    ));
                }
                self.ensure_block();
                self.set_loc(target.loc);
                if self.has_cleanup()
                    && let ExprKind::LabelAddr(label) = target.kind
                {
                    let label = self.def.label_addrs[label - 1];
                    self.emit_exit_actions(self.cleanup_jumps.depth(label))?;
                    let block = self.label_block(label);
                    self.jump_to(block);
                    return Ok(());
                }
                let n = if let Some(table) = &self.dispatch_table {
                    let (global, index) = dispatch_index(target).expect("preflighted dispatch");
                    debug_assert_eq!(global, table.global);
                    // Keep the numeric table available, including in debug info.
                    self.g.use_global(global);
                    // PtrAdd's index is already a long. Its evaluation (including
                    // assignments and pc++) stays at the original goto location.
                    self.full_expression(|fg| fg.val(index))?
                } else {
                    let p = self.full_expression(|fg| fg.val(target))?;
                    self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, p)
                };
                let dispatch = match self.dispatch {
                    Some(block) => block,
                    None => {
                        let block = self.new_block();
                        self.b.append_block_param(block, Type::I64);
                        self.dispatch = Some(block);
                        block
                    }
                };
                self.b.ins().jump(dispatch, &[n]);
                self.terminated = true;
            }
            Stmt::Label(label) => {
                self.labels_placed = true;
                let block = self.label_block(label);
                self.jump_to(block);
                self.switch_to(block);
            }
        }
        Ok(())
    }

    pub(super) fn emit_dispatch(&mut self) {
        let Some(dispatch) = self.dispatch else {
            return;
        };
        self.switch_to(dispatch);
        self.b.set_srcloc(None);
        let n = self.b.function().block_params(dispatch)[0];
        let call = |block| keel_ir::BlockCall {
            block,
            args: Vec::new(),
        };
        // Keep one shared merge and one switch, whether its key is a numeric
        // label token or an index into a proven immutable table.
        let labels: Vec<(i64, &'p str)> = match &self.dispatch_table {
            Some(table) => table
                .labels
                .iter()
                .enumerate()
                .filter(|(_, label)| **label != 0)
                .map(|(i, &label)| (i as i64, self.def.label_addrs[label - 1]))
                .collect(),
            None => self
                .def
                .label_addrs
                .iter()
                .enumerate()
                .map(|(i, &label)| (i as i64 + 1, label))
                .collect(),
        };
        // Zero entries and invalid tokens still trap. An out-of-bounds table
        // access has no defined C behavior; the fused default also traps.
        let nowhere = self.new_block();
        let table = labels
            .into_iter()
            .map(|(key, label)| (key, call(self.label_block(label))))
            .collect();
        self.b.ins().switch(n, call(nowhere), table);
        self.b.seal_block(nowhere);
        self.switch_to(nowhere);
        self.b.ins().trap();
        self.terminated = true;
    }

    fn loop_body(&mut self, body: &Stmt<'p>, block: Block, exit: Block, cont: Block) -> R<()> {
        self.switch_to(block);
        self.breaks.push((exit, self.exit_actions.len()));
        self.continues.push((cont, self.exit_actions.len()));
        self.stmt(body)?;
        self.breaks.pop();
        self.continues.pop();
        Ok(())
    }
}
