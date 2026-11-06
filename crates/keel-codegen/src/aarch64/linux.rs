//! Base AAPCS64 Linux variadic state and initial-exec TLS.
use super::*;

impl FunctionCompiler<'_> {
    pub(super) fn symbol(&self, name: &str) -> String {
        if self.linux {
            name.to_owned()
        } else {
            Format::MachO.symbol(name)
        }
    }
    pub(super) fn linux_va_start(&mut self, list: Value) {
        let a = self.use_reg(list, S2);
        let area = self.variadic_area.expect("variadic save area");
        let (gr, vr) = abi::named_registers(&param_kinds(&self.func.sig.params));
        // __stack, __gr_top and __vr_top.
        self.address(Loc::Reg(S0), "x29", self.variadic_offset);
        self.mach("str", [Reg::x(S0).into(), Address::base(Reg::x(a)).into()]);
        for (field, offset) in [(8, area + 64), (16, area + 192)] {
            self.address(Loc::Reg(S0), self.base, offset);
            self.mach(
                "str",
                [Reg::x(S0).into(), Address::offset(Reg::x(a), field).into()],
            );
        }
        for (field, value) in [(24, -(8 - gr as i64) * 8), (28, -(8 - vr as i64) * 16)] {
            self.materialize(S0, value as u64, false);
            self.mach(
                "str",
                [Reg::w(S0).into(), Address::offset(Reg::x(a), field).into()],
            );
        }
    }
    pub(super) fn linux_va_arg(
        &mut self,
        ty: Type,
        list: Value,
        result: Option<Value>,
        inst: Inst,
    ) {
        let a = self.use_reg(list, S2);
        let floating = ty.is_float() || ty.is_vector();
        let (offs, top, step) = if floating {
            (28, 16, 16)
        } else {
            (24, 8, if ty == Type::I128 { 16 } else { 8 })
        };
        let stack = format!(".L{}_{}_va_stack", self.func.name, inst);
        let ready = format!(".L{}_{}_va_ready", self.func.name, inst);
        self.mach(
            "ldr",
            [Reg::w(S0).into(), Address::offset(Reg::x(a), offs).into()],
        );
        self.mach("cmp", [Reg::w(S0).into(), Arg::imm(0)]);
        self.out.symbolic(Symbolic::Conditional {
            condition: 10,
            target: stack.clone(),
        }); // ge
        if ty == Type::I128 {
            self.mach("add", [Reg::w(S0).into(), Reg::w(S0).into(), Arg::imm(15)]);
            self.mach("and", [Reg::w(S0).into(), Reg::w(S0).into(), Arg::imm(-16)]);
        }
        self.mach(
            "add",
            [Reg::w(S1).into(), Reg::w(S0).into(), Arg::imm(step)],
        );
        self.mach(
            "str",
            [Reg::w(S1).into(), Address::offset(Reg::x(a), offs).into()],
        );
        self.mach("cmp", [Reg::w(S1).into(), Arg::imm(0)]);
        self.out.symbolic(Symbolic::Conditional {
            condition: 12,
            target: stack.clone(),
        }); // gt
        self.mach(
            "ldr",
            [Reg::x(S1).into(), Address::offset(Reg::x(a), top).into()],
        );
        self.mach("sxtw", [Reg::x(S0).into(), Reg::w(S0).into()]);
        self.mach(
            "add",
            [Reg::x(S0).into(), Reg::x(S1).into(), Reg::x(S0).into()],
        );
        self.out.branch(false, ready.clone());
        self.out.label(stack);
        self.mach("ldr", [Reg::x(S0).into(), Address::base(Reg::x(a)).into()]);
        let size = ty.bytes().unwrap_or(8) as usize;
        if size > 8 {
            self.mach("add", [Reg::x(S0).into(), Reg::x(S0).into(), Arg::imm(15)]);
            self.mach("and", [Reg::x(S0).into(), Reg::x(S0).into(), Arg::imm(-16)]);
        }
        self.mach(
            "add",
            [
                Reg::x(S1).into(),
                Reg::x(S0).into(),
                Arg::imm(size.next_multiple_of(8)),
            ],
        );
        self.mach("str", [Reg::x(S1).into(), Address::base(Reg::x(a)).into()]);
        self.out.label(ready);
        if ty == Type::I128 {
            // Load both halves before an allocated destination can overwrite
            // the pointer, then commit both allocation homes.
            self.mach(
                "ldr",
                [Reg::x(S1).into(), Address::offset(Reg::x(S0), 8).into()],
            );
            let d = self.def_reg(result);
            self.mach("ldr", [Reg::x(d).into(), Address::base(Reg::x(S0)).into()]);
            self.def_done(result, d);
            if let Some(value) = result {
                self.emit_move(self.hi(value), Loc::Reg(S1));
            }
        } else if floating {
            let d = self.def_freg(result);
            self.mach(
                "ldr",
                [freg(d, ty).into(), Address::base(Reg::x(S0)).into()],
            );
            self.def_done(result, d);
        } else {
            let d = self.def_reg(result);
            self.mach(
                match ty {
                    Type::I8 => "ldrb",
                    Type::I16 => "ldrh",
                    _ => "ldr",
                },
                [reg(d, ty).into(), Address::base(Reg::x(S0)).into()],
            );
            self.def_done(result, d);
        }
    }
    pub(super) fn linux_tls_address(&mut self, d: u8, name: &str) {
        // A spilled destination uses S0; keep the TLS offset intact while
        // reading the thread pointer into a different reserved register.
        let thread = if d == S0 { S1 } else { S0 };
        self.out.symbolic(Symbolic::PageAddress {
            register: d,
            kind: PageKind::ThreadLocal,
            target: name.into(),
        });
        self.out.symbolic(Symbolic::PageOffset {
            register: d,
            base: d,
            load: true,
            kind: PageKind::ThreadLocal,
            target: name.into(),
        });
        self.native(
            Opcode::MRS_RS_systemmove,
            [
                (FieldId::o0, 1),
                (FieldId::op1, 3),
                (FieldId::CRn, 13),
                (FieldId::CRm, 0),
                (FieldId::op2, 2),
                (FieldId::Rt, thread as u32),
            ],
            |out| write!(out, "mrs\tx{thread}, TPIDR_EL0"),
        );
        self.mach(
            "add",
            [Reg::x(d).into(), Reg::x(d).into(), Reg::x(thread).into()],
        );
    }
}

impl FunctionCompiler<'_> {
    /// ARMv8.0 LL/SC loops; do not assume the optional LSE extension. The
    /// load-exclusive acquires when `ordering` does and the store-exclusive
    /// releases when it does, as Clang maps them. `cas` is `Some(weak)` for
    /// a compare-and-swap: a weak one makes one attempt instead of looping.
    pub(super) fn linux_atomic(
        &mut self,
        op: RmwOp,
        args: &[Value],
        result: Option<Value>,
        inst: Inst,
        ordering: Ordering,
        cas: Option<bool>,
    ) {
        let weak = cas == Some(true);
        let cas = cas.is_some();
        let acquire = matches!(
            ordering,
            Ordering::Acquire | Ordering::AcqRel | Ordering::SeqCst
        );
        let release = matches!(
            ordering,
            Ordering::Release | Ordering::AcqRel | Ordering::SeqCst
        );
        let ty = self.ty(args[1]);
        for (r, &arg) in args.iter().enumerate() {
            self.emit_move(Loc::Reg(r as u8), self.loc(arg));
        }
        // x0 address; x1 value/expected; x2 CAS replacement; x3 old;
        // x4 RMW replacement; w5 store-exclusive status. These registers
        // are excluded from Linux allocation, including private leaves.
        self.extend_in_place(
            1,
            ty,
            if matches!(op, RmwOp::Smin | RmwOp::Smax) {
                ArgExt::Sext
            } else {
                ArgExt::Uext
            },
        );
        let retry = format!(".L{}_{}_atomic_retry", self.func.name, inst);
        let done = format!(".L{}_{}_atomic_done", self.func.name, inst);
        if !weak {
            self.out.label(retry.clone());
        }
        let load = match (ty, acquire) {
            (Type::I8, true) => Opcode::LDAXRB_LR32_ldstexclr,
            (Type::I16, true) => Opcode::LDAXRH_LR32_ldstexclr,
            (Type::I32, true) => Opcode::LDAXR_LR32_ldstexclr,
            (_, true) => Opcode::LDAXR_LR64_ldstexclr,
            (Type::I8, false) => Opcode::LDXRB_LR32_ldstexclr,
            (Type::I16, false) => Opcode::LDXRH_LR32_ldstexclr,
            (Type::I32, false) => Opcode::LDXR_LR32_ldstexclr,
            (_, false) => Opcode::LDXR_LR64_ldstexclr,
        };
        let suffix = match ty {
            Type::I8 => "b",
            Type::I16 => "h",
            _ => "",
        };
        let a = if acquire { "a" } else { "" };
        self.native(load, [(FieldId::Rt, 3), (FieldId::Rn, 0)], |out| {
            write!(out, "ld{a}xr{suffix}\t{}, [x0]", reg(3, ty))
        });
        if cas {
            self.mach("cmp", [reg(3, ty).into(), reg(1, ty).into()]);
            self.out.symbolic(Symbolic::Conditional {
                condition: 1,
                target: done.clone(),
            });
            self.move_gp(4, 2, matches!(ty, Type::I64 | Type::Ptr));
        } else {
            match op {
                RmwOp::Xchg => self.move_gp(4, 1, matches!(ty, Type::I64 | Type::Ptr)),
                RmwOp::Add | RmwOp::Sub | RmwOp::And | RmwOp::Or | RmwOp::Xor => {
                    let mnemonic = match op {
                        RmwOp::Add => "add",
                        RmwOp::Sub => "sub",
                        RmwOp::And => "and",
                        RmwOp::Or => "orr",
                        _ => "eor",
                    };
                    self.mach(
                        mnemonic,
                        [reg(4, ty).into(), reg(3, ty).into(), reg(1, ty).into()],
                    );
                }
                _ => {
                    if matches!(op, RmwOp::Smin | RmwOp::Smax) {
                        self.extend_in_place(3, ty, ArgExt::Sext);
                    }
                    self.mach("cmp", [reg(3, ty).into(), reg(1, ty).into()]);
                    let cond = match op {
                        RmwOp::Smin => "lt",
                        RmwOp::Smax => "gt",
                        RmwOp::Umin => "lo",
                        _ => "hi",
                    };
                    self.mach(
                        "csel",
                        [
                            reg(4, ty).into(),
                            reg(3, ty).into(),
                            reg(1, ty).into(),
                            cond.into(),
                        ],
                    );
                }
            }
        }
        let store = match (ty, release) {
            (Type::I8, true) => Opcode::STLXRB_SR32_ldstexclr,
            (Type::I16, true) => Opcode::STLXRH_SR32_ldstexclr,
            (Type::I32, true) => Opcode::STLXR_SR32_ldstexclr,
            (_, true) => Opcode::STLXR_SR64_ldstexclr,
            (Type::I8, false) => Opcode::STXRB_SR32_ldstexclr,
            (Type::I16, false) => Opcode::STXRH_SR32_ldstexclr,
            (Type::I32, false) => Opcode::STXR_SR32_ldstexclr,
            (_, false) => Opcode::STXR_SR64_ldstexclr,
        };
        let l = if release { "l" } else { "" };
        self.native(
            store,
            [(FieldId::Rs, 5), (FieldId::Rt, 4), (FieldId::Rn, 0)],
            |out| write!(out, "st{l}xr{suffix}\tw5, {}, [x0]", reg(4, ty)),
        );
        if weak {
            // eq exactly when the store-exclusive succeeded.
            self.mach("cmp", [Reg::w(5).into(), Arg::imm(0)]);
        } else {
            self.out.symbolic(Symbolic::Compare {
                register: 5,
                is64: false,
                nonzero: true,
                target: retry,
            });
        }
        self.out.label(done);
        self.native(Opcode::CLREX_BN_barriers, [(FieldId::CRm, 15)], |out| {
            out.write_str("clrex")
        });
        // Neither the store-exclusive, its branch nor `clrex` writes the
        // flags, so they still hold the last compare: eq exactly when it
        // stored.
        let flag = cas
            .then(|| self.func.inst_results(inst)[1])
            .and_then(|flag| self.defined(flag));
        if flag.is_some() {
            self.mach("cset", [Reg::w(S0).into(), "eq".into()]);
        }
        if let Some(dst) = result.and_then(|v| self.defined(v)) {
            self.emit_move(dst, Loc::Reg(3));
        }
        if let Some(loc) = flag {
            self.emit_move(loc, Loc::Reg(S0));
        }
    }
}
