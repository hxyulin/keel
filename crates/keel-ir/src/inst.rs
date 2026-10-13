//! Instruction formats.
//!
//! Each format is a variant of [`InstData`], so a malformed instruction (a
//! jump without a target, a load without a type) cannot be built. Generic
//! passes do not match on formats; they use [`InstData::for_each_operand`]
//! and friends.

use crate::{Block, DataRef, FuncRef, Intrinsic, SigRef, StackSlot, Type, Value};
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
            | InstData::Select { .. }
            | InstData::Convert { .. }
            | InstData::Icmp { .. }
            | InstData::Fcmp { .. }
            | InstData::StackAddr { .. }
            | InstData::SymbolAddr { .. } => false,
            InstData::Load { .. }
            | InstData::Store { .. }
            | InstData::Call { .. }
            | InstData::CallIndirect { .. }
            | InstData::Trap
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. }
            | InstData::Return { .. } => true,
        }
    }

    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            InstData::Jump { .. }
                | InstData::Brif { .. }
                | InstData::Switch { .. }
                | InstData::Return { .. }
                | InstData::Trap
        )
    }

    pub fn branch_dests(&self) -> &[BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_ref(dest),
            InstData::Brif { dests, .. } => dests,
            InstData::Switch { dests, .. } => dests,
            _ => &[],
        }
    }

    pub fn branch_dests_mut(&mut self) -> &mut [BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_mut(dest),
            InstData::Brif { dests, .. } => dests,
            InstData::Switch { dests, .. } => dests,
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
            | InstData::Trap => {}
            InstData::Binary { args, .. }
            | InstData::Icmp { args, .. }
            | InstData::Fcmp { args, .. } => args.iter().copied().for_each(f),
            InstData::CallIndirect { callee, args, .. } => {
                f(*callee);
                args.iter().copied().for_each(f);
            }
            InstData::Select { args } => args.iter().copied().for_each(f),
            InstData::Unary { arg, .. } => f(*arg),
            InstData::Convert { arg, .. } => f(*arg),
            InstData::Load { addr, .. } => f(*addr),
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
            | InstData::Intrinsic { args, .. } => args.iter().copied().for_each(f),
        }
    }

    pub fn for_each_operand_mut(&mut self, mut f: impl FnMut(&mut Value)) {
        match self {
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::StackAddr { .. }
            | InstData::SymbolAddr { .. }
            | InstData::Trap => {}
            InstData::Binary { args, .. }
            | InstData::Icmp { args, .. }
            | InstData::Fcmp { args, .. } => args.iter_mut().for_each(f),
            InstData::CallIndirect { callee, args, .. } => {
                f(callee);
                args.iter_mut().for_each(f);
            }
            InstData::Select { args } => args.iter_mut().for_each(f),
            InstData::Unary { arg, .. } => f(arg),
            InstData::Convert { arg, .. } => f(arg),
            InstData::Load { addr, .. } => f(addr),
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
