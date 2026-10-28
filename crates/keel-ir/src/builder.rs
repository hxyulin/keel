//! Typed constructors for instructions, so passes and frontends write
//! `func.ins(at).iadd(a, b)` instead of assembling an `InstData` by hand.
//!
//! An `InstBuilder` inserts one instruction and is consumed by it; methods
//! for instructions with one result return that `Value`, the others return
//! the `Inst`. `func.ins(at).loc(loc)` gives the instruction a source
//! location.

use crate::function::InsertPoint;
use crate::{
    BinaryOp, Block, BlockCall, ConvertOp, FloatCC, FuncRef, Function, InlineAsm, Inst, InstData,
    IntCC, Intrinsic, Ordering, OverflowOp, RmwOp, SigRef, SourceLoc, StackSlot, Symbol, Type,
    UnaryOp, Value, VectorBinaryOp,
};

#[derive(Debug)]
pub struct InstBuilder<'f> {
    func: &'f mut Function,
    at: InsertPoint,
    loc: Option<SourceLoc>,
    result_tys: Option<&'f mut Vec<Type>>,
}

impl Function {
    /// A builder that inserts one instruction at `at`.
    pub fn ins(&mut self, at: InsertPoint) -> InstBuilder<'_> {
        InstBuilder {
            func: self,
            at,
            loc: None,
            result_tys: None,
        }
    }

    /// A one-instruction builder borrowing reusable result-type scratch.
    pub fn ins_with_scratch<'f>(
        &'f mut self,
        at: InsertPoint,
        result_tys: &'f mut Vec<Type>,
    ) -> InstBuilder<'f> {
        InstBuilder {
            func: self,
            at,
            loc: None,
            result_tys: Some(result_tys),
        }
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
    /// Gives the instruction a source location.
    pub fn loc(mut self, loc: Option<SourceLoc>) -> Self {
        self.loc = loc;
        self
    }

    /// Inserts any instruction.
    pub fn build(self, data: InstData) -> Inst {
        let inst = match self.result_tys {
            Some(scratch) => self.func.insert_inst_with_scratch(self.at, data, scratch),
            None => self.func.insert_inst(self.at, data),
        };
        self.func.set_inst_loc(inst, self.loc);
        inst
    }

    fn value(self, data: InstData) -> Value {
        let func = &mut *self.func;
        let at = self.at;
        let loc = self.loc;
        let inst = match self.result_tys {
            Some(scratch) => func.insert_inst_with_scratch(at, data, scratch),
            None => func.insert_inst(at, data),
        };
        func.set_inst_loc(inst, loc);
        func.first_result(inst)
    }

    pub fn vsplat(self, ty: Type, arg: Value) -> Value {
        self.value(InstData::VectorSplat { ty, arg })
    }
    pub fn vector_unary(self, op: UnaryOp, arg: Value) -> Value {
        self.value(InstData::VectorUnary { op, arg })
    }
    pub fn neon(self, op: crate::NeonIntrinsic, ty: Type, args: &[Value]) -> Value {
        self.value(InstData::Neon {
            op,
            ty,
            args: args.to_vec(),
        })
    }
    /// Structured loads return 1–4 vectors; stores return no values.
    /// Query the returned instruction with `Function::inst_results`.
    pub fn neon_memory(self, op: crate::NeonIntrinsic, ty: Type, args: &[Value]) -> Inst {
        assert!(op.memory().is_some());
        self.build(InstData::Neon {
            op,
            ty,
            args: args.to_vec(),
        })
    }
    pub fn vector_reduce(self, op: crate::VectorReduceOp, arg: Value) -> Value {
        self.value(InstData::VectorReduce { op, arg })
    }
    pub fn vector_binary(self, op: VectorBinaryOp, a: Value, b: Value) -> Value {
        self.value(InstData::VectorBinary { op, args: [a, b] })
    }
    pub fn vextract(self, arg: Value, lane: u8) -> Value {
        self.value(InstData::VectorExtract { arg, lane })
    }
    pub fn vinsert(self, vector: Value, value: Value, lane: u8) -> Value {
        self.value(InstData::VectorInsert {
            args: [vector, value],
            lane,
        })
    }
    pub fn vshuffle(self, a: Value, b: Value, lanes: Vec<u8>) -> Value {
        self.value(InstData::VectorShuffle {
            args: [a, b],
            lanes,
        })
    }
    pub fn vicmp(self, cond: IntCC, a: Value, b: Value) -> Value {
        self.value(InstData::VectorIcmp { cond, args: [a, b] })
    }
    pub fn vfcmp(self, cond: FloatCC, a: Value, b: Value) -> Value {
        self.value(InstData::VectorFcmp { cond, args: [a, b] })
    }
    pub fn vselect(self, mask: Value, a: Value, b: Value) -> Value {
        self.value(InstData::VectorSelect { args: [mask, a, b] })
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
        fcopysign => Fcopysign,
    }

    /// The wrapped result and the `i8` overflow flag.
    pub fn overflow(self, op: OverflowOp, a: Value, b: Value) -> (Value, Value) {
        let func = &mut *self.func;
        let data = InstData::Overflow { op, args: [a, b] };
        let inst = match self.result_tys {
            Some(scratch) => func.insert_inst_with_scratch(self.at, data, scratch),
            None => func.insert_inst(self.at, data),
        };
        func.set_inst_loc(inst, self.loc);
        let results = func.inst_results(inst);
        (results[0], results[1])
    }

    pub fn fma(self, a: Value, b: Value, c: Value) -> Value {
        self.value(InstData::Fma { args: [a, b, c] })
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

    pub fn f80const(self, significand: u64, sign_exp: u16) -> Value {
        self.value(InstData::F80const {
            significand,
            sign_exp,
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
        self.load_with_flags(ty, addr, crate::MemoryFlags::default())
    }

    pub fn load_with_flags(self, ty: Type, addr: Value, flags: crate::MemoryFlags) -> Value {
        self.value(InstData::Load { ty, addr, flags })
    }

    pub fn store_with_flags(self, value: Value, addr: Value, flags: crate::MemoryFlags) -> Inst {
        self.build(InstData::Store { value, addr, flags })
    }

    pub fn aggregate_va_arg(self, layout: crate::Layout, list: Value) -> Value {
        let home = self.func.create_stack_slot(
            layout
                .size()
                .try_into()
                .expect("aggregate home size fits u32"),
            layout
                .align()
                .try_into()
                .expect("aggregate home alignment fits u32"),
        );
        self.value(InstData::AggregateVaArg {
            layout,
            list,
            home,
            x86_vector_abi: None,
        })
    }

    pub fn call_abi(self, func: FuncRef, args: crate::CallArgs) -> Inst {
        self.build(InstData::Call { func, args })
    }

    pub fn call_indirect_abi(self, sig: SigRef, callee: Value, args: crate::CallArgs) -> Inst {
        self.build(InstData::CallIndirect { sig, callee, args })
    }

    pub fn va_arg(self, ty: Type, list: Value) -> Value {
        self.value(InstData::VaArg {
            ty,
            list,
            x86_vector_abi: None,
        })
    }

    pub fn va_arg_with_x86_vector_abi(
        self,
        ty: Type,
        list: Value,
        abi: crate::X86VectorAbi,
    ) -> Value {
        self.value(InstData::VaArg {
            ty,
            list,
            x86_vector_abi: Some(abi),
        })
    }

    pub fn atomic_load(self, ty: Type, ordering: Ordering, addr: Value) -> Value {
        self.value(InstData::AtomicLoad { ty, ordering, addr })
    }

    pub fn atomic_store(self, ordering: Ordering, value: Value, addr: Value) -> Inst {
        self.build(InstData::AtomicStore {
            ordering,
            value,
            addr,
        })
    }

    pub fn atomic_rmw(self, op: RmwOp, ordering: Ordering, addr: Value, value: Value) -> Value {
        self.value(InstData::AtomicRmw {
            op,
            ordering,
            args: [addr, value],
        })
    }

    pub fn atomic_cas(self, ordering: Ordering, addr: Value, expected: Value, new: Value) -> Value {
        self.value(InstData::AtomicCas {
            ordering,
            args: [addr, expected, new],
        })
    }

    pub fn fence(self, ordering: Ordering) -> Inst {
        self.build(InstData::Fence { ordering })
    }

    pub fn store(self, value: Value, addr: Value) -> Inst {
        self.store_with_flags(value, addr, crate::MemoryFlags::default())
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
            args: args.to_vec().into(),
        })
    }

    /// Inline assembly; its results are the outputs, in order.
    pub fn inline_asm(self, asm: InlineAsm, args: &[Value]) -> Inst {
        self.build(InstData::InlineAsm {
            asm: Box::new(asm),
            args: args.to_vec(),
        })
    }

    pub fn call_indirect(self, sig: SigRef, callee: Value, args: &[Value]) -> Inst {
        self.build(InstData::CallIndirect {
            sig,
            callee,
            args: args.to_vec().into(),
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
