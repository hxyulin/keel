//! Lowering of variadic arguments, loads, stores, atomics and fences.
use super::*;

impl FunctionCompiler<'_> {
    /// Variadic arguments, loads, stores, atomic operations and fences.
    pub(super) fn memory_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            // Load the pointer, store it advanced by a slot, then read the
            // argument: the result may share the list's register.
            InstData::AggregateVaArg { .. } => {
                unreachable!("aggregate va_arg rejected before AArch64 emission")
            }
            InstData::VaArg { ty, list, .. } if self.linux => {
                self.linux_va_arg(ty, list, result, inst)
            }
            InstData::VaArg { ty, list, .. } => {
                let a = self.use_reg(list, S2);
                self.mach(
                    "ldr",
                    [(Reg::x(S0)).into(), Address::base(Reg::x(a)).into()],
                );
                let bytes = if ty.is_vector() {
                    ty.bytes().unwrap()
                } else {
                    8
                };
                if bytes == 16 {
                    self.mach("add", [Reg::x(S0).into(), Reg::x(S0).into(), Arg::imm(15)]);
                    self.mach("and", [Reg::x(S0).into(), Reg::x(S0).into(), Arg::imm(-16)]);
                }
                self.mach(
                    "add",
                    [
                        (Reg::x(S1)).into(),
                        (Reg::x(S0)).into(),
                        Arg::imm(i64::from(bytes)),
                    ],
                );
                self.mach(
                    "str",
                    [(Reg::x(S1)).into(), Address::base(Reg::x(a)).into()],
                );
                if ty.is_float() || ty.is_vector() {
                    let d = self.def_freg(result);
                    self.mach(
                        "ldr",
                        [(freg(d, ty)).into(), Address::base(Reg::x(S0)).into()],
                    );
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    self.mach(
                        "ldr",
                        [(reg(d, ty)).into(), Address::base(Reg::x(S0)).into()],
                    );
                    self.def_done(result, d);
                }
            }
            InstData::Load { ty, addr, .. } if ty.is_float() || ty.is_vector() => {
                let d = self.def_freg(result);
                let a = self.mem(addr, S0, ty);
                self.mach("ldr", [(freg(d, ty)).into(), (a).into()]);
                self.def_done(result, d);
            }
            InstData::Load { ty, addr, .. } => {
                let d = self.def_reg(result);
                let a = self.mem(addr, S0, ty);
                let op = match ty {
                    Type::I8 => "ldrb",
                    Type::I16 => "ldrh",
                    _ => "ldr",
                };
                self.mach(op, [(reg(d, ty)).into(), (a).into()]);
                self.def_done(result, d);
            }
            InstData::Store { value, addr, .. }
                if self.ty(value).is_float() || self.ty(value).is_vector() =>
            {
                let ty = self.ty(value);
                let v = self.use_freg(value, FS0);
                let a = self.mem(addr, S1, ty);
                self.mach("str", [(freg(v, ty)).into(), (a).into()]);
            }
            InstData::Store { value, addr, .. } => {
                let ty = self.ty(value);
                let v = match folded_imm(func, func.inst_data(inst)) {
                    Some(_) if ty == Type::I64 => Reg::XZR,
                    Some(_) => Reg::WZR,
                    None => reg(self.use_reg(value, S0), ty),
                };
                let a = self.mem(addr, S1, ty);
                let op = match ty {
                    Type::I8 => "strb",
                    Type::I16 => "strh",
                    _ => "str",
                };
                self.mach(op, [(v).into(), (a).into()]);
            }
            InstData::AtomicLoad { ty, ordering, addr } => {
                let d = self.def_reg(result);
                let a = self.use_reg(addr, S0);
                let op = if ordering == Ordering::Relaxed {
                    "ldr"
                } else {
                    "ldar"
                };
                self.atomic(
                    op,
                    "",
                    ty,
                    [reg(d, ty).into(), Address::base(Reg::x(a)).into()],
                );
                self.def_done(result, d);
            }
            InstData::AtomicStore {
                ordering,
                value,
                addr,
            } => {
                let ty = self.ty(value);
                let v = self.use_reg(value, S0);
                let a = self.use_reg(addr, S1);
                let op = if ordering == Ordering::Relaxed {
                    "str"
                } else {
                    "stlr"
                };
                self.atomic(
                    op,
                    "",
                    ty,
                    [reg(v, ty).into(), Address::base(Reg::x(a)).into()],
                );
            }
            InstData::AtomicRmw { op, ordering, args } if self.linux => {
                self.linux_atomic(op, &args, result, inst, ordering, None)
            }
            InstData::AtomicCas {
                success,
                failure,
                weak,
                args,
            } if self.linux => self.linux_atomic(
                RmwOp::Xchg,
                &args,
                result,
                inst,
                Ordering::merge(success, failure),
                Some(weak),
            ),
            InstData::AtomicRmw { op, ordering, args } => {
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S0);
                let mut v = self.use_reg(args[1], S1);
                // LSE has no subtract or and: add the negation, clear the
                // complement.
                let mnemonic = match op {
                    RmwOp::Sub => {
                        self.mach("neg", [(reg(S1, ty)).into(), (reg(v, ty)).into()]);
                        v = S1;
                        "ldadd"
                    }
                    RmwOp::And => {
                        self.mach("mvn", [(reg(S1, ty)).into(), (reg(v, ty)).into()]);
                        v = S1;
                        "ldclr"
                    }
                    RmwOp::Add => "ldadd",
                    RmwOp::Or => "ldset",
                    RmwOp::Xor => "ldeor",
                    RmwOp::Xchg => "swp",
                    RmwOp::Umin => "ldumin",
                    RmwOp::Umax => "ldumax",
                    RmwOp::Smin => "ldsmin",
                    RmwOp::Smax => "ldsmax",
                };
                // The old value goes to a scratch register, so it never
                // shares one with the operands.
                let order = ordering_suffix(ordering);
                let (rv, rd) = (reg(v, ty), reg(S2, ty));
                self.atomic(
                    mnemonic,
                    order,
                    ty,
                    [rv.into(), rd.into(), Address::base(Reg::x(a)).into()],
                );
                if let Some(loc) = result.and_then(|r| self.defined(r)) {
                    self.emit_move(loc, Loc::Reg(S2));
                }
            }
            // LSE `cas` never fails spuriously, so a weak one is the same.
            InstData::AtomicCas {
                success,
                failure,
                args,
                ..
            } => {
                // `cas` compares with its first register and overwrites it
                // with the old value, so the expected value goes in a
                // scratch register.
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S0);
                let expected = self.use_reg(args[1], S1);
                self.move_gp(S2, expected, true);
                let new = self.use_reg(args[2], S1);
                let order = ordering_suffix(Ordering::merge(success, failure));
                let (rs, rt) = (reg(S2, ty), reg(new, ty));
                self.atomic(
                    "cas",
                    order,
                    ty,
                    [rs.into(), rt.into(), Address::base(Reg::x(a)).into()],
                );
                // An unused or fused flag has no location. A fused one
                // leaves the compare's flags for the next `brif`.
                let flag = self.func.inst_results(inst)[1];
                let fused = self.fused_compares.contains(&flag);
                let flag = self.defined(flag);
                // The flag compares before the old value moves out, which
                // may overwrite the expected value's register. `cas` zero
                // extends a narrow old value; the expected value's upper
                // bits are unspecified.
                if flag.is_some() || fused {
                    let expected = self.use_reg(args[1], S1);
                    let expected = self.extend(S0, expected, ty, false);
                    self.mach("cmp", [reg(S2, ty).into(), reg(expected, ty).into()]);
                }
                if flag.is_some() {
                    self.mach("cset", [Reg::w(S0).into(), "eq".into()]);
                }
                if let Some(loc) = result.and_then(|r| self.defined(r)) {
                    self.emit_move(loc, Loc::Reg(S2));
                }
                if let Some(loc) = flag {
                    self.emit_move(loc, Loc::Reg(S0));
                }
            }
            InstData::Fence { ordering } => {
                let barrier = if ordering == Ordering::Acquire {
                    Arg::Barrier(9)
                } else {
                    Arg::Barrier(11)
                };
                self.mach("dmb", [barrier]);
            }
            _ => unreachable!("not a memory access"),
        }
    }
}
