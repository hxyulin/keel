//! Typed constructors for instructions, so passes and frontends write
//! `func.ins(at).iadd(a, b)` instead of assembling an `InstData` by hand.
//!
//! An `InstBuilder` inserts one instruction and is consumed by it; methods
//! for instructions with one result return that `Value`, the others return
//! the `Inst`.

use crate::function::InsertPoint;
use crate::{
    BinaryOp, Block, BlockCall, ConvertOp, FloatCC, FuncRef, Function, Inst, InstData, IntCC,
    Intrinsic, SigRef, StackSlot, Symbol, Type, UnaryOp, Value,
};

pub struct InstBuilder<'f> {
    func: &'f mut Function,
    at: InsertPoint,
}

impl Function {
    /// A builder that inserts one instruction at `at`.
    pub fn ins(&mut self, at: InsertPoint) -> InstBuilder<'_> {
        InstBuilder { func: self, at }
    }
}

macro_rules! binary {
    ($($name:ident => $op:ident),* $(,)?) => {
        $(
            pub fn $name(self, a: Value, b: Value) -> Value {
                self.binary(BinaryOp::$op, a, b)
            }
        )*
    };
}

macro_rules! unary {
    ($($name:ident => $op:ident),* $(,)?) => {
        $(
            pub fn $name(self, a: Value) -> Value {
                self.unary(UnaryOp::$op, a)
            }
        )*
    };
}

impl InstBuilder<'_> {
    /// Inserts any instruction.
    pub fn build(self, data: InstData) -> Inst {
        self.func.insert_inst(self.at, data)
    }

    fn value(self, data: InstData) -> Value {
        let func = self.func;
        let inst = func.insert_inst(self.at, data);
        func.first_result(inst)
    }

    pub fn iconst(self, ty: Type, imm: i64) -> Value {
        self.value(InstData::Iconst { ty, imm })
    }

    pub fn binary(self, op: BinaryOp, a: Value, b: Value) -> Value {
        self.value(InstData::Binary { op, args: [a, b] })
    }

    pub fn unary(self, op: UnaryOp, arg: Value) -> Value {
        self.value(InstData::Unary { op, arg })
    }

    binary! {
        iadd => Iadd, isub => Isub, imul => Imul,
        udiv => Udiv, sdiv => Sdiv, urem => Urem, srem => Srem,
        and => And, or => Or, xor => Xor,
        shl => Shl, ushr => Ushr, sshr => Sshr, rotl => Rotl, rotr => Rotr,
        umulhi => Umulhi, smulhi => Smulhi, ptradd => Ptradd,
        fadd => Fadd, fsub => Fsub, fmul => Fmul, fdiv => Fdiv, fmin => Fmin, fmax => Fmax,
    }

    unary! {
        ineg => Ineg, bnot => Bnot, clz => Clz, ctz => Ctz, popcnt => Popcnt, bswap => Bswap,
        fneg => Fneg, fabs => Fabs, fsqrt => Fsqrt,
        fceil => Fceil, ffloor => Ffloor, ftrunc => Ftrunc, fnearest => Fnearest,
    }

    pub fn f32const(self, x: f32) -> Value {
        self.value(InstData::Fconst {
            ty: Type::F32,
            bits: u64::from(x.to_bits()),
        })
    }

    pub fn f64const(self, x: f64) -> Value {
        self.value(InstData::Fconst {
            ty: Type::F64,
            bits: x.to_bits(),
        })
    }

    pub fn fcmp(self, cond: FloatCC, a: Value, b: Value) -> Value {
        self.value(InstData::Fcmp { cond, args: [a, b] })
    }

    pub fn icmp(self, cond: IntCC, a: Value, b: Value) -> Value {
        self.value(InstData::Icmp { cond, args: [a, b] })
    }

    pub fn select(self, cond: Value, if_true: Value, if_false: Value) -> Value {
        self.value(InstData::Select {
            args: [cond, if_true, if_false],
        })
    }

    pub fn convert(self, op: ConvertOp, ty: Type, arg: Value) -> Value {
        self.value(InstData::Convert { op, ty, arg })
    }

    pub fn sext(self, ty: Type, arg: Value) -> Value {
        self.convert(ConvertOp::Sext, ty, arg)
    }

    pub fn uext(self, ty: Type, arg: Value) -> Value {
        self.convert(ConvertOp::Uext, ty, arg)
    }

    pub fn trunc(self, ty: Type, arg: Value) -> Value {
        self.convert(ConvertOp::Trunc, ty, arg)
    }

    pub fn load(self, ty: Type, addr: Value) -> Value {
        self.value(InstData::Load { ty, addr })
    }

    pub fn va_arg(self, ty: Type, list: Value) -> Value {
        self.value(InstData::VaArg { ty, list })
    }

    pub fn store(self, value: Value, addr: Value) -> Inst {
        self.build(InstData::Store { value, addr })
    }

    pub fn stack_addr(self, slot: StackSlot) -> Value {
        self.value(InstData::StackAddr { slot })
    }

    pub fn symbol_addr(self, symbol: Symbol) -> Value {
        self.value(InstData::SymbolAddr { symbol })
    }

    pub fn call(self, func: FuncRef, args: &[Value]) -> Inst {
        self.build(InstData::Call {
            func,
            args: args.to_vec(),
        })
    }

    pub fn call_indirect(self, sig: SigRef, callee: Value, args: &[Value]) -> Inst {
        self.build(InstData::CallIndirect {
            sig,
            callee,
            args: args.to_vec(),
        })
    }

    pub fn intrinsic(self, id: Intrinsic, args: &[Value]) -> Inst {
        self.build(InstData::Intrinsic {
            id,
            args: args.to_vec(),
        })
    }

    pub fn jump(self, block: Block, args: &[Value]) -> Inst {
        self.build(InstData::Jump {
            dest: BlockCall {
                block,
                args: args.to_vec(),
            },
        })
    }

    pub fn brif(
        self,
        cond: Value,
        then_block: Block,
        then_args: &[Value],
        else_block: Block,
        else_args: &[Value],
    ) -> Inst {
        self.build(InstData::Brif {
            cond,
            dests: [
                BlockCall {
                    block: then_block,
                    args: then_args.to_vec(),
                },
                BlockCall {
                    block: else_block,
                    args: else_args.to_vec(),
                },
            ],
        })
    }

    /// `switch arg, default, [case: dest, ...]`.
    pub fn switch(self, arg: Value, default: BlockCall, cases: Vec<(i64, BlockCall)>) -> Inst {
        let (cases, rest): (Vec<i64>, Vec<BlockCall>) = cases.into_iter().unzip();
        let mut dests = vec![default];
        dests.extend(rest);
        self.build(InstData::Switch { arg, dests, cases })
    }

    pub fn return_(self, args: &[Value]) -> Inst {
        self.build(InstData::Return {
            args: args.to_vec(),
        })
    }

    pub fn trap(self) -> Inst {
        self.build(InstData::Trap)
    }
}
