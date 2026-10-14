//! Instruction formats.
//!
//! Each format is a variant of [`InstData`], so a malformed instruction (a
//! jump without a target, a load without a type) cannot be built. Generic
//! passes do not match on formats; they use [`InstData::for_each_operand`]
//! and friends.

use crate::{Block, DataRef, FuncRef, InlineAsm, Intrinsic, SigRef, StackSlot, Type, Value};
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum InstData {
    /// Integer constant, sign-extended (or truncated) to `ty`.
    Iconst {
        ty: Type,
        imm: i64,
    },
    /// Float constant, as the bits of `ty` (`f32` uses the low 32), so the
    /// instruction stays hashable and NaN payloads survive.
    Fconst {
        ty: Type,
        bits: u64,
    },
    Unary {
        op: UnaryOp,
        arg: Value,
    },
    Binary {
        op: BinaryOp,
        args: [Value; 2],
    },
    /// Integer arithmetic that also reports overflow: the wrapped result,
    /// and an `i8` that is 1 if the exact result does not fit the type
    /// (taken as unsigned or signed, by the operation).
    Overflow {
        op: OverflowOp,
        args: [Value; 2],
    },
    /// `args[0] * args[1] + args[2]` with a single rounding.
    Fma {
        args: [Value; 3],
    },
    /// `args[1]` if `args[0]` is non-zero, else `args[2]`. Does not branch.
    Select {
        args: [Value; 3],
    },
    /// Conversion between types: integer widths, pointers and integers,
    /// floats and integers.
    Convert {
        op: ConvertOp,
        ty: Type,
        arg: Value,
    },
    /// Produces `i8` 0 or 1.
    Icmp {
        cond: IntCC,
        args: [Value; 2],
    },
    /// Produces `i8` 0 or 1.
    Fcmp {
        cond: FloatCC,
        args: [Value; 2],
    },
    Load {
        ty: Type,
        addr: Value,
    },
    Store {
        value: Value,
        addr: Value,
    },
    /// Atomic accesses, each to naturally aligned memory, ordered as C11's
    /// `memory_order`s. Integers up to `i64` and `ptr`.
    AtomicLoad {
        ty: Type,
        ordering: Ordering,
        addr: Value,
    },
    AtomicStore {
        ordering: Ordering,
        value: Value,
        addr: Value,
    },
    /// Applies `op` to the memory at `args[0]` with `args[1]`, and gives
    /// the old value.
    AtomicRmw {
        op: RmwOp,
        ordering: Ordering,
        args: [Value; 2],
    },
    /// Compare and swap: if the memory at `args[0]` holds `args[1]`,
    /// replaces it with `args[2]`. Gives the old value either way.
    AtomicCas {
        ordering: Ordering,
        args: [Value; 3],
    },
    Fence {
        ordering: Ordering,
    },
    /// Reads the next variadic argument, of type `ty` (`i32`, `i64`, `ptr`
    /// or `f64`), from the `va_list` at `list`, and advances it.
    VaArg {
        ty: Type,
        list: Value,
    },
    /// Calls a function; results follow its signature.
    Call {
        func: FuncRef,
        args: Vec<Value>,
    },
    /// Calls the function at address `callee`, which must have signature
    /// `sig`.
    CallIndirect {
        sig: SigRef,
        callee: Value,
        args: Vec<Value>,
    },
    /// A call-like operation with known meaning; see [`Intrinsic`].
    Intrinsic {
        id: Intrinsic,
        args: Vec<Value>,
    },
    /// Inline assembly: one result per output, one argument per input; see
    /// [`InlineAsm`]. Pinned and never removed, as the template may do
    /// anything.
    InlineAsm {
        asm: Box<InlineAsm>,
        args: Vec<Value>,
    },
    /// Address of a stack slot.
    StackAddr {
        slot: StackSlot,
    },
    /// Address of a function or data object.
    SymbolAddr {
        symbol: Symbol,
    },
    Jump {
        dest: BlockCall,
    },
    /// Branch to `dests[0]` if `cond` is non-zero, else to `dests[1]`.
    Brif {
        cond: Value,
        dests: [BlockCall; 2],
    },
    /// Branch to `dests[i + 1]` if `arg` equals `cases[i]`, else to
    /// `dests[0]`. Case values are bit patterns of `arg`'s type, written
    /// signed or unsigned, and distinct.
    Switch {
        arg: Value,
        dests: Vec<BlockCall>,
        cases: Vec<i64>,
    },
    Return {
        args: Vec<Value>,
    },
    /// Calls a function and returns what it returns, in place of this
    /// function's frame: a guaranteed tail call. Its signature returns what
    /// this function's does.
    ReturnCall {
        func: FuncRef,
        args: Vec<Value>,
    },
    /// Calls a function that may throw: on return, continues to `dests[0]`
    /// with the results defined; on an exception, to the landing pad
    /// `dests[1]`. Neither destination takes arguments. Needs a function
    /// with a personality.
    Invoke {
        func: FuncRef,
        args: Vec<Value>,
        dests: [BlockCall; 2],
    },
    /// The first instruction of a block an `invoke` unwinds to: gives the
    /// exception object (`ptr`) and the selector (`i32`), which says which
    /// clause matched: a catch clause's `eh_typeid`, or 0 for `cleanup`.
    LandingPad {
        clauses: Vec<Clause>,
    },
    /// The selector a landing pad gives when `typeinfo`'s catch clause
    /// matches.
    EhTypeid {
        typeinfo: DataRef,
    },
    /// [`InstData::ReturnCall`] through a pointer.
    ReturnCallIndirect {
        sig: SigRef,
        callee: Value,
        args: Vec<Value>,
    },
    /// Stops the program. A terminator, since nothing after it runs.
    Trap,
}

/// Formats an `fconst` operand: the shortest decimal that reads back as the
/// same value, or the raw bits in hex for NaN, whose payload decimal cannot
/// show.
pub(crate) fn float_text(ty: Type, bits: u64) -> String {
    match ty {
        Type::F32 => {
            let x = f32::from_bits(bits as u32);
            // Bits above 32 are invalid; show them rather than drop them.
            if x.is_nan() || bits > u64::from(u32::MAX) {
                format!("{bits:#010x}")
            } else {
                format!("{x:?}")
            }
        }
        _ => {
            let x = f64::from_bits(bits);
            if x.is_nan() {
                format!("{bits:#018x}")
            } else {
                format!("{x:?}")
            }
        }
    }
}

/// A linker symbol an instruction refers to, through one of the function's
/// tables.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Symbol {
    Func(FuncRef),
    Data(DataRef),
}

/// A branch target together with the arguments for its block parameters.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct BlockCall {
    pub block: Block,
    pub args: Vec<Value>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum BinaryOp {
    Iadd,
    Isub,
    Imul,
    Udiv,
    Sdiv,
    Urem,
    Srem,
    And,
    Or,
    Xor,
    Shl,
    Ushr,
    Sshr,
    /// Rotates; the amount is taken modulo the bit width, like shifts.
    Rotl,
    Rotr,
    /// High half of the double-width product.
    Umulhi,
    Smulhi,
    /// `ptr + i64` byte offset.
    Ptradd,
    /// Float arithmetic, IEEE 754 with round-to-nearest-even.
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    /// IEEE 754-2019 `minimum`/`maximum`: a NaN operand gives NaN, and
    /// -0 is less than +0.
    Fmin,
    Fmax,
    /// The magnitude of `args[0]` with the sign of `args[1]`.
    Fcopysign,
}

/// A landing pad clause: what kind of exception it lands for. The
/// personality tries them in order.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Clause {
    /// Exceptions matching a type, by its typeinfo object; `None` matches
    /// any exception.
    Catch(Option<DataRef>),
    /// Any exception, to run cleanups and resume unwinding. Only last.
    Cleanup,
}

/// The memory ordering of an atomic operation, as in C11.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Ordering {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

/// The operation of an `atomic_rmw`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum RmwOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
    /// Stores the operand.
    Xchg,
    Umin,
    Umax,
    Smin,
    Smax,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OverflowOp {
    Uadd,
    Sadd,
    Usub,
    Ssub,
    Umul,
    Smul,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum UnaryOp {
    Ineg,
    Bnot,
    /// Leading zeros; the bit width for 0.
    Clz,
    /// Trailing zeros; the bit width for 0.
    Ctz,
    Popcnt,
    /// Byte swap. Needs at least `i16`.
    Bswap,
    Fneg,
    Fabs,
    Fsqrt,
    /// Rounding to an integral float: up, down, toward zero, and to the
    /// nearest with ties to even.
    Fceil,
    Ffloor,
    Ftrunc,
    Fnearest,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ConvertOp {
    Sext,
    Uext,
    Trunc,
    /// `ptr` to `i64`: the address as a number.
    PtrToInt,
    /// `i64` to `ptr`.
    IntToPtr,
    /// `f32` to `f64`, exactly.
    Fpext,
    /// `f64` to `f32`, rounding to nearest.
    Fptrunc,
    /// Float to `i32`/`i64`, rounding toward zero. Out-of-range values
    /// saturate and NaN gives 0, as on aarch64 (and Rust's `as`).
    Fptosi,
    Fptoui,
    /// Integer to float, rounding to nearest.
    Sitofp,
    Uitofp,
    /// Reinterprets the bits between an integer and a float of the same
    /// size.
    Bitcast,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum IntCC {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
}

/// Float comparisons. All but `ne` and `uno` are false if either operand is
/// NaN; `ne` is true then, like C's `!=`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum FloatCC {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Neither operand is NaN.
    Ord,
    /// At least one operand is NaN.
    Uno,
}

impl BinaryOp {
    pub fn is_float(self) -> bool {
        matches!(
            self,
            BinaryOp::Fadd
                | BinaryOp::Fsub
                | BinaryOp::Fmul
                | BinaryOp::Fdiv
                | BinaryOp::Fmin
                | BinaryOp::Fmax
                | BinaryOp::Fcopysign
        )
    }

    /// Division and remainder trap on a zero divisor (and `sdiv` on overflow).
    pub fn can_trap(self) -> bool {
        matches!(
            self,
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem
        )
    }
}

impl UnaryOp {
    pub fn is_float(self) -> bool {
        matches!(
            self,
            UnaryOp::Fneg
                | UnaryOp::Fabs
                | UnaryOp::Fsqrt
                | UnaryOp::Fceil
                | UnaryOp::Ffloor
                | UnaryOp::Ftrunc
                | UnaryOp::Fnearest
        )
    }
}

impl InstData {
    /// Pinned instructions keep their position; the rest are pure and float,
    /// placed only by the scheduler.
    pub fn is_pinned(&self) -> bool {
        match self {
            InstData::Binary { op, .. } => op.can_trap(),
            InstData::Intrinsic { id, .. } => !id.info().pure,
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::Unary { .. }
            | InstData::Overflow { .. }
            | InstData::Fma { .. }
            | InstData::Select { .. }
            | InstData::Convert { .. }
            | InstData::Icmp { .. }
            | InstData::Fcmp { .. }
            | InstData::StackAddr { .. }
            | InstData::EhTypeid { .. }
            | InstData::SymbolAddr { .. } => false,
            InstData::Load { .. }
            | InstData::Store { .. }
            | InstData::AtomicLoad { .. }
            | InstData::AtomicStore { .. }
            | InstData::AtomicRmw { .. }
            | InstData::AtomicCas { .. }
            | InstData::Fence { .. }
            | InstData::VaArg { .. }
            | InstData::Call { .. }
            | InstData::CallIndirect { .. }
            | InstData::InlineAsm { .. }
            | InstData::Trap
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. }
            | InstData::Return { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::Invoke { .. }
            | InstData::LandingPad { .. } => true,
        }
    }

    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            InstData::Jump { .. }
                | InstData::Brif { .. }
                | InstData::Switch { .. }
                | InstData::Return { .. }
                | InstData::ReturnCall { .. }
                | InstData::ReturnCallIndirect { .. }
                | InstData::Invoke { .. }
                | InstData::Trap
        )
    }

    pub fn branch_dests(&self) -> &[BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_ref(dest),
            InstData::Brif { dests, .. } => dests,
            InstData::Switch { dests, .. } => dests,
            InstData::Invoke { dests, .. } => dests,
            _ => &[],
        }
    }

    pub fn branch_dests_mut(&mut self) -> &mut [BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_mut(dest),
            InstData::Brif { dests, .. } => dests,
            InstData::Switch { dests, .. } => dests,
            InstData::Invoke { dests, .. } => dests,
            _ => &mut [],
        }
    }

    /// Visits every value operand, including block-call arguments.
    pub fn for_each_operand(&self, mut f: impl FnMut(Value)) {
        match self {
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::StackAddr { .. }
            | InstData::SymbolAddr { .. }
            | InstData::Fence { .. }
            | InstData::LandingPad { .. }
            | InstData::EhTypeid { .. }
            | InstData::Trap => {}
            InstData::Invoke { args, dests, .. } => {
                args.iter().copied().for_each(&mut f);
                dests.iter().flat_map(|d| &d.args).copied().for_each(f);
            }
            InstData::Binary { args, .. }
            | InstData::Overflow { args, .. }
            | InstData::AtomicRmw { args, .. }
            | InstData::Icmp { args, .. }
            | InstData::Fcmp { args, .. } => args.iter().copied().for_each(f),
            InstData::AtomicCas { args, .. } => args.iter().copied().for_each(f),
            InstData::AtomicLoad { addr, .. } => f(*addr),
            InstData::AtomicStore { value, addr, .. } => {
                f(*value);
                f(*addr);
            }
            InstData::CallIndirect { callee, args, .. }
            | InstData::ReturnCallIndirect { callee, args, .. } => {
                f(*callee);
                args.iter().copied().for_each(f);
            }
            InstData::Select { args } | InstData::Fma { args } => args.iter().copied().for_each(f),
            InstData::Unary { arg, .. } => f(*arg),
            InstData::Convert { arg, .. } => f(*arg),
            InstData::Load { addr, .. } => f(*addr),
            InstData::VaArg { list, .. } => f(*list),
            InstData::Store { value, addr } => {
                f(*value);
                f(*addr);
            }
            InstData::Jump { dest } => dest.args.iter().copied().for_each(f),
            InstData::Brif { cond, dests } => {
                f(*cond);
                dests.iter().flat_map(|d| &d.args).copied().for_each(f);
            }
            InstData::Switch { arg, dests, .. } => {
                f(*arg);
                dests.iter().flat_map(|d| &d.args).copied().for_each(f);
            }
            InstData::Return { args }
            | InstData::Call { args, .. }
            | InstData::ReturnCall { args, .. }
            | InstData::InlineAsm { args, .. }
            | InstData::Intrinsic { args, .. } => args.iter().copied().for_each(f),
        }
    }

    pub fn for_each_operand_mut(&mut self, mut f: impl FnMut(&mut Value)) {
        match self {
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::StackAddr { .. }
            | InstData::SymbolAddr { .. }
            | InstData::Fence { .. }
            | InstData::LandingPad { .. }
            | InstData::EhTypeid { .. }
            | InstData::Trap => {}
            InstData::Invoke { args, dests, .. } => {
                args.iter_mut().for_each(&mut f);
                dests.iter_mut().flat_map(|d| &mut d.args).for_each(f);
            }
            InstData::Binary { args, .. }
            | InstData::Overflow { args, .. }
            | InstData::AtomicRmw { args, .. }
            | InstData::Icmp { args, .. }
            | InstData::Fcmp { args, .. } => args.iter_mut().for_each(f),
            InstData::AtomicCas { args, .. } => args.iter_mut().for_each(f),
            InstData::AtomicLoad { addr, .. } => f(addr),
            InstData::AtomicStore { value, addr, .. } => {
                f(value);
                f(addr);
            }
            InstData::CallIndirect { callee, args, .. }
            | InstData::ReturnCallIndirect { callee, args, .. } => {
                f(callee);
                args.iter_mut().for_each(f);
            }
            InstData::Select { args } | InstData::Fma { args } => args.iter_mut().for_each(f),
            InstData::Unary { arg, .. } => f(arg),
            InstData::Convert { arg, .. } => f(arg),
            InstData::Load { addr, .. } => f(addr),
            InstData::VaArg { list, .. } => f(list),
            InstData::Store { value, addr } => {
                f(value);
                f(addr);
            }
            InstData::Jump { dest } => dest.args.iter_mut().for_each(f),
            InstData::Brif { cond, dests } => {
                f(cond);
                dests.iter_mut().flat_map(|d| &mut d.args).for_each(f);
            }
            InstData::Switch { arg, dests, .. } => {
                f(arg);
                dests.iter_mut().flat_map(|d| &mut d.args).for_each(f);
            }
            InstData::Return { args }
            | InstData::Call { args, .. }
            | InstData::ReturnCall { args, .. }
            | InstData::InlineAsm { args, .. }
            | InstData::Intrinsic { args, .. } => args.iter_mut().for_each(f),
        }
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BinaryOp::Iadd => "iadd",
            BinaryOp::Isub => "isub",
            BinaryOp::Imul => "imul",
            BinaryOp::Udiv => "udiv",
            BinaryOp::Sdiv => "sdiv",
            BinaryOp::Urem => "urem",
            BinaryOp::Srem => "srem",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::Xor => "xor",
            BinaryOp::Shl => "shl",
            BinaryOp::Ushr => "ushr",
            BinaryOp::Sshr => "sshr",
            BinaryOp::Rotl => "rotl",
            BinaryOp::Rotr => "rotr",
            BinaryOp::Umulhi => "umulhi",
            BinaryOp::Smulhi => "smulhi",
            BinaryOp::Ptradd => "ptradd",
            BinaryOp::Fadd => "fadd",
            BinaryOp::Fsub => "fsub",
            BinaryOp::Fmul => "fmul",
            BinaryOp::Fdiv => "fdiv",
            BinaryOp::Fmin => "fmin",
            BinaryOp::Fmax => "fmax",
            BinaryOp::Fcopysign => "fcopysign",
        })
    }
}

impl fmt::Display for Ordering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Ordering::Relaxed => "relaxed",
            Ordering::Acquire => "acquire",
            Ordering::Release => "release",
            Ordering::AcqRel => "acq_rel",
            Ordering::SeqCst => "seq_cst",
        })
    }
}

impl fmt::Display for RmwOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RmwOp::Add => "add",
            RmwOp::Sub => "sub",
            RmwOp::And => "and",
            RmwOp::Or => "or",
            RmwOp::Xor => "xor",
            RmwOp::Xchg => "xchg",
            RmwOp::Umin => "umin",
            RmwOp::Umax => "umax",
            RmwOp::Smin => "smin",
            RmwOp::Smax => "smax",
        })
    }
}

impl fmt::Display for OverflowOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OverflowOp::Uadd => "uadd_overflow",
            OverflowOp::Sadd => "sadd_overflow",
            OverflowOp::Usub => "usub_overflow",
            OverflowOp::Ssub => "ssub_overflow",
            OverflowOp::Umul => "umul_overflow",
            OverflowOp::Smul => "smul_overflow",
        })
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnaryOp::Ineg => "ineg",
            UnaryOp::Bnot => "bnot",
            UnaryOp::Clz => "clz",
            UnaryOp::Ctz => "ctz",
            UnaryOp::Popcnt => "popcnt",
            UnaryOp::Bswap => "bswap",
            UnaryOp::Fneg => "fneg",
            UnaryOp::Fabs => "fabs",
            UnaryOp::Fsqrt => "fsqrt",
            UnaryOp::Fceil => "fceil",
            UnaryOp::Ffloor => "ffloor",
            UnaryOp::Ftrunc => "ftrunc",
            UnaryOp::Fnearest => "fnearest",
        })
    }
}

impl fmt::Display for ConvertOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConvertOp::Sext => "sext",
            ConvertOp::Uext => "uext",
            ConvertOp::Trunc => "trunc",
            ConvertOp::PtrToInt => "ptrtoint",
            ConvertOp::IntToPtr => "inttoptr",
            ConvertOp::Fpext => "fpext",
            ConvertOp::Fptrunc => "fptrunc",
            ConvertOp::Fptosi => "fptosi",
            ConvertOp::Fptoui => "fptoui",
            ConvertOp::Sitofp => "sitofp",
            ConvertOp::Uitofp => "uitofp",
            ConvertOp::Bitcast => "bitcast",
        })
    }
}

impl fmt::Display for IntCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IntCC::Eq => "eq",
            IntCC::Ne => "ne",
            IntCC::Slt => "slt",
            IntCC::Sle => "sle",
            IntCC::Sgt => "sgt",
            IntCC::Sge => "sge",
            IntCC::Ult => "ult",
            IntCC::Ule => "ule",
            IntCC::Ugt => "ugt",
            IntCC::Uge => "uge",
        })
    }
}

impl fmt::Display for FloatCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FloatCC::Eq => "eq",
            FloatCC::Ne => "ne",
            FloatCC::Lt => "lt",
            FloatCC::Le => "le",
            FloatCC::Gt => "gt",
            FloatCC::Ge => "ge",
            FloatCC::Ord => "ord",
            FloatCC::Uno => "uno",
        })
    }
}
