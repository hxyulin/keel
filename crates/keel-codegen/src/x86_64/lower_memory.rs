//! Lowering of loads, stores, atomics and fences.
use super::*;

impl FunctionCompiler<'_> {
    /// Loads, stores, atomic operations and fences.
    pub(super) fn memory_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::Load { ty, addr, .. } if ty.is_float() || ty.is_vector() => {
                let d = self.def_freg(result);
                let a = self.mem(addr, S0, ty);
                self.emit(movs(ty), [asm_arg(simd(d, ty)), asm_arg(a)]);
                self.def_done(result, d);
            }
            // A narrow load zero-extends to 32 bits, which avoids writing
            // part of a register (and depending on its old value).
            InstData::Load { ty, addr, .. } => {
                let d = self.def_reg(result);
                let a = self.mem(addr, S0, ty);
                match ty {
                    Type::I8 | Type::I16 => self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(a)]),
                    _ => self.emit(Opcode::Mov, [asm_arg(reg(d, ty)), asm_arg(a)]),
                }
                self.def_done(result, d);
            }
            InstData::Store { value, addr, .. }
                if self.ty(value).is_float() || self.ty(value).is_vector() =>
            {
                let ty = self.ty(value);
                let v = self.use_freg(value, FS0);
                let a = self.mem(addr, S1, ty);
                self.emit(movs(ty), [asm_arg(a), asm_arg(simd(v, ty))]);
            }
            InstData::Store { value, addr, .. } => {
                let ty = self.ty(value);
                let v = match folded_imm(func, func.inst_data(inst)) {
                    Some((_, k)) => asm_arg(k),
                    None => asm_arg(reg(self.use_reg(value, S0), ty)),
                };
                let a = self.mem(addr, S1, ty);
                self.emit(Opcode::Mov, [asm_arg(a), asm_arg(v)]);
            }
            // x86 is strongly ordered: every load is an acquire and every
            // store a release. Only a sequentially consistent store needs
            // more, so that later loads cannot pass it: `xchg` with memory
            // is locked, a full barrier.
            InstData::AtomicLoad { ty, addr, .. } => {
                let d = self.def_reg(result);
                let a = self.use_reg(addr, S0);
                let src = sized(memory(r64(a), 0), ptr(ty));
                match ty {
                    Type::I8 | Type::I16 => {
                        self.emit(Opcode::Movzx, [asm_arg(r32(d)), asm_arg(src)])
                    }
                    _ => self.emit(Opcode::Mov, [asm_arg(reg(d, ty)), asm_arg(src)]),
                }
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
                let op = if ordering == Ordering::SeqCst {
                    Opcode::Xchg
                } else {
                    Opcode::Mov
                };
                if op == Opcode::Xchg && v != S0 {
                    // xchg writes its register operand.
                    self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(v))]);
                }
                let v = if op == Opcode::Xchg { S0 } else { v };
                self.emit(
                    op,
                    [
                        asm_arg(sized(memory(r64(a), 0), ptr(ty))),
                        asm_arg(reg(v, ty)),
                    ],
                );
            }
            InstData::AtomicRmw { op, args, .. } => self.atomic_rmw(inst, op, args, result),
            // `cmpxchg` compares memory with rax and writes the old value
            // to rax either way.
            InstData::AtomicCas { args, .. } => {
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S1);
                let expected = self.use_reg(args[1], S0);
                self.emit(Opcode::Mov, [asm_arg(r64(0)), asm_arg(r64(expected))]);
                let new = self.use_reg(args[2], S2);
                self.emit_locked(
                    Opcode::Cmpxchg,
                    [
                        asm_arg(sized(memory(r64(a), 0), ptr(ty))),
                        asm_arg(reg(new, ty)),
                    ],
                );
                if let Some(loc) = result.and_then(|r| self.defined(r)) {
                    self.emit_move(loc, Loc::Reg(S0));
                }
            }
            InstData::Fence { ordering } => {
                if ordering == Ordering::SeqCst {
                    self.emit(Opcode::Mfence, []);
                }
            }
            _ => unreachable!("not a memory access"),
        }
    }
}
