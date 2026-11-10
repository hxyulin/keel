//! Instruction formats.
//!
//! Each format is a variant of [`InstData`], so a malformed instruction (a
//! jump without a target, a load without a type) cannot be built. Generic
//! passes do not match on formats; they use [`InstData::for_each_operand`]
//! and friends.

use crate::{
    Block, DataRef, FuncRef, InlineAsm, Intrinsic, MemoryEffects, MemoryFlags, SigRef, StackSlot,
    Type, Value, VectorBinaryOp,
};
use std::fmt;

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum InstData {
    /// Integer constant. `imm` holds the value sign-extended from `ty`'s
    /// width ([`Type::canonical_imm`]), so `iconst.i8 255` is stored as -1;
    /// the verifier rejects any other form. Build one with
    /// [`InstData::iconst`] to get the canonical form.
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
    /// Raw x87 extended payload, without the six bytes of ABI padding.
    F80const {
        significand: u64,
        sign_exp: u16,
    },
    Unary {
        op: UnaryOp,
        arg: Value,
    },
    Binary {
        op: BinaryOp,
        args: [Value; 2],
    },
    /// Broadcast one scalar into every lane of `ty`.
    VectorSplat {
        ty: Type,
        arg: Value,
    },
    VectorUnary {
        op: UnaryOp,
        arg: Value,
    },
    /// Explicit ARM semantics with observable FPCR/FPSR effects. Pinned barrier.
    Neon {
        op: crate::NeonIntrinsic,
        ty: Type,
        args: Vec<Value>,
    },
    VectorReduce {
        op: crate::VectorReduceOp,
        arg: Value,
    },
    VectorBinary {
        op: VectorBinaryOp,
        args: [Value; 2],
    },
    /// Extract a scalar lane. Index is a checked compile-time immediate.
    VectorExtract {
        arg: Value,
        lane: u8,
    },
    /// Replace one lane; args are (vector, scalar).
    VectorInsert {
        args: [Value; 2],
        lane: u8,
    },
    /// Concatenate two equal-shaped vectors, then choose one source lane per
    /// output lane. Every index must be in 0..2*lane_count; no undef lanes.
    VectorShuffle {
        args: [Value; 2],
        lanes: Vec<u8>,
    },
    /// Comparison masks contain all ones for true, zero for false per lane.
    VectorIcmp {
        cond: IntCC,
        args: [Value; 2],
    },
    VectorFcmp {
        cond: FloatCC,
        args: [Value; 2],
    },
    /// Select an entire lane from `args[1]` if its integer mask lane is nonzero,
    /// otherwise `args[2]`. Mask has the same lane count and width as the arms.
    VectorSelect {
        args: [Value; 3],
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
        flags: MemoryFlags,
        ty: Type,
        addr: Value,
    },
    Store {
        flags: MemoryFlags,
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
    /// replaces it with `args[2]`. Gives the old value and an `i8` flag, 1 if
    /// it stored. `success` orders the access when it stores and `failure`
    /// when it does not. A weak one may fail while the memory holds
    /// `args[1]`.
    AtomicCas {
        success: Ordering,
        failure: Ordering,
        weak: bool,
        args: [Value; 3],
    },
    Fence {
        ordering: Ordering,
    },
    /// Reads the next variadic argument, of type `ty` (`i32`, `i64`, `ptr`
    /// or `f64`; AArch64 Linux also supports `i128`), from the `va_list` at `list`, and advances it.
    VaArg {
        ty: Type,
        list: Value,
        /// Explicit producer of this va_list; never inferred from the host.
        x86_vector_abi: Option<crate::X86VectorAbi>,
    },
    /// Materializes the next aggregate in an independent private frame home,
    /// returning its address. Overflow transport reads exactly layout.size() bytes.
    /// Register transport reads occupied ABI eightbytes at their object offsets;
    /// padding-only NO_CLASS eightbytes have no source and remain unspecified.
    /// Reads finish before advancing the va_list. The snapshot is writable and
    /// distinct from the register save area, overflow area, and other homes.
    /// The home is an explicit stack slot in this function activation. Re-executing
    /// this instruction, or a clone sharing its home, overwrites that storage;
    /// a pointer carried through a phi still aliases it. To retain an earlier
    /// value across re-execution, copy the object to explicit storage first.
    /// The address is valid until function return; contents are mutable.
    /// As a memory-access precondition, [home, home + layout.size()) must be
    /// disjoint from the va_list object and every register-save/overflow byte
    /// read by this extraction. Unknown external pointers carry this contract;
    /// the verifier rejects provable list/home overlap. Different extractions
    /// may intentionally share a home (for example, clones after loop rotation).
    AggregateVaArg {
        layout: crate::Layout,
        list: Value,
        /// Explicit activation-local storage identity. Cloning an instruction
        /// preserves it; inlining remaps it like every other stack slot.
        home: StackSlot,
        x86_vector_abi: Option<crate::X86VectorAbi>,
    },
    /// Calls a function; results follow its signature.
    Call {
        func: FuncRef,
        args: crate::CallArgs,
    },
    /// Calls the function at address `callee`, which must have signature
    /// `sig`.
    CallIndirect {
        sig: SigRef,
        callee: Value,
        args: crate::CallArgs,
    },
    /// A call-like operation with known meaning; see [`Intrinsic`].
    Intrinsic {
        id: Intrinsic,
        args: Vec<Value>,
    },
    /// Owned full-width FLAGS raw CFG under the explicit terminal external
    /// monitor contract. Inputs are I64 images captured before ownership; all
    /// results are I64 observations published after normal image reapplication.
    /// Faults/traps produce no result, restoration, unwind or resumable ABI edge.
    X86RflagsRegion {
        plan: Box<crate::rflags_region::Plan>,
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
        /// Boxed, like `invoke`'s, to keep [`InstData`] at 48 bytes.
        dests: Box<[BlockCall; 2]>,
    },
    /// Branch to `dests[i + 1]` if `arg` equals `cases[i]`, else to
    /// `dests[0]`. Case values are bit patterns of `arg`'s type, written
    /// signed or unsigned, and distinct.
    Switch {
        arg: Value,
        dests: Vec<BlockCall>,
        /// Boxed to keep [`InstData`] at 48 bytes.
        cases: Box<Vec<i64>>,
    },
    Return {
        args: Vec<Value>,
    },
    /// Calls a function and returns what it returns, in place of this
    /// function's frame: a guaranteed tail call. Its signature returns what
    /// this function's does.
    ReturnCall {
        func: FuncRef,
        args: crate::CallArgs,
    },
    /// Calls a function that may throw: on return, continues to `dests[0]`
    /// with the results defined; on an exception, to the landing pad
    /// `dests[1]`. Neither destination takes arguments. Needs a function
    /// with a personality.
    Invoke {
        func: FuncRef,
        args: crate::CallArgs,
        dests: Box<[BlockCall; 2]>,
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
        args: crate::CallArgs,
    },
    /// Stops the program. A terminator, since nothing after it runs.
    Trap,
}

// One per instruction ever created; see `InstNode`.
const _: () = assert!(std::mem::size_of::<InstData>() == 48);

// Fails to compile if a format that `clone` copies bitwise gains a field
// that is not `Copy`. Patterns list every field, so a new field also fails.
const _: () = {
    fn copy<T: Copy>(_: &T) {}
    #[allow(dead_code)]
    fn fields_are_copy(data: &InstData) {
        match data {
            InstData::Iconst { ty, imm } => {
                copy(ty);
                copy(imm);
            }
            InstData::Fconst { ty, bits } => {
                copy(ty);
                copy(bits);
            }
            InstData::F80const {
                significand,
                sign_exp,
            } => {
                copy(significand);
                copy(sign_exp);
            }
            InstData::Unary { op, arg } => {
                copy(op);
                copy(arg);
            }
            InstData::Binary { op, args } => {
                copy(op);
                copy(args);
            }
            InstData::VectorSplat { ty, arg } => {
                copy(ty);
                copy(arg);
            }
            InstData::VectorUnary { op, arg } => {
                copy(op);
                copy(arg);
            }
            InstData::VectorReduce { op, arg } => {
                copy(op);
                copy(arg);
            }
            InstData::VectorBinary { op, args } => {
                copy(op);
                copy(args);
            }
            InstData::VectorExtract { arg, lane } => {
                copy(arg);
                copy(lane);
            }
            InstData::VectorInsert { args, lane } => {
                copy(args);
                copy(lane);
            }
            InstData::VectorIcmp { cond, args } => {
                copy(cond);
                copy(args);
            }
            InstData::VectorFcmp { cond, args } => {
                copy(cond);
                copy(args);
            }
            InstData::VectorSelect { args } => {
                copy(args);
            }
            InstData::Overflow { op, args } => {
                copy(op);
                copy(args);
            }
            InstData::Fma { args } => {
                copy(args);
            }
            InstData::Select { args } => {
                copy(args);
            }
            InstData::Convert { op, ty, arg } => {
                copy(op);
                copy(ty);
                copy(arg);
            }
            InstData::Icmp { cond, args } => {
                copy(cond);
                copy(args);
            }
            InstData::Fcmp { cond, args } => {
                copy(cond);
                copy(args);
            }
            InstData::Load { flags, ty, addr } => {
                copy(flags);
                copy(ty);
                copy(addr);
            }
            InstData::Store { flags, value, addr } => {
                copy(flags);
                copy(value);
                copy(addr);
            }
            InstData::AtomicLoad { ty, ordering, addr } => {
                copy(ty);
                copy(ordering);
                copy(addr);
            }
            InstData::AtomicStore {
                ordering,
                value,
                addr,
            } => {
                copy(ordering);
                copy(value);
                copy(addr);
            }
            InstData::AtomicRmw { op, ordering, args } => {
                copy(op);
                copy(ordering);
                copy(args);
            }
            InstData::AtomicCas {
                success,
                failure,
                weak,
                args,
            } => {
                copy(success);
                copy(failure);
                copy(weak);
                copy(args);
            }
            InstData::Fence { ordering } => {
                copy(ordering);
            }
            InstData::VaArg {
                ty,
                list,
                x86_vector_abi,
            } => {
                copy(ty);
                copy(list);
                copy(x86_vector_abi);
            }
            InstData::StackAddr { slot } => {
                copy(slot);
            }
            InstData::SymbolAddr { symbol } => {
                copy(symbol);
            }
            InstData::EhTypeid { typeinfo } => {
                copy(typeinfo);
            }
            InstData::Trap => {}
            _ => {}
        }
    }
};

// Passes clone instructions to rewrite them; most formats own no heap data
// and copy as plain bytes, without a per-format jump.
impl Clone for InstData {
    #[inline]
    fn clone(&self) -> Self {
        match self {
            Self::Iconst { .. }
            | Self::Fconst { .. }
            | Self::F80const { .. }
            | Self::Unary { .. }
            | Self::Binary { .. }
            | Self::VectorSplat { .. }
            | Self::VectorUnary { .. }
            | Self::VectorReduce { .. }
            | Self::VectorBinary { .. }
            | Self::VectorExtract { .. }
            | Self::VectorInsert { .. }
            | Self::VectorIcmp { .. }
            | Self::VectorFcmp { .. }
            | Self::VectorSelect { .. }
            | Self::Overflow { .. }
            | Self::Fma { .. }
            | Self::Select { .. }
            | Self::Convert { .. }
            | Self::Icmp { .. }
            | Self::Fcmp { .. }
            | Self::Load { .. }
            | Self::Store { .. }
            | Self::AtomicLoad { .. }
            | Self::AtomicStore { .. }
            | Self::AtomicRmw { .. }
            | Self::AtomicCas { .. }
            | Self::Fence { .. }
            | Self::VaArg { .. }
            | Self::StackAddr { .. }
            | Self::SymbolAddr { .. }
            | Self::EhTypeid { .. }
            | Self::Trap => {
                // SAFETY: these formats hold only `Copy` fields, so a bitwise
                // copy is an independent, equal value.
                unsafe { std::ptr::read(self) }
            }
            Self::AggregateVaArg {
                layout,
                list,
                home,
                x86_vector_abi,
            } => Self::AggregateVaArg {
                layout: layout.clone(),
                list: *list,
                home: *home,
                x86_vector_abi: *x86_vector_abi,
            },
            Self::Neon { op, ty, args } => Self::Neon {
                op: *op,
                ty: *ty,
                args: args.clone(),
            },
            Self::VectorShuffle { args, lanes } => Self::VectorShuffle {
                args: *args,
                lanes: lanes.clone(),
            },
            Self::Call { func, args } => Self::Call {
                func: *func,
                args: args.clone(),
            },
            Self::CallIndirect { sig, callee, args } => Self::CallIndirect {
                sig: *sig,
                callee: *callee,
                args: args.clone(),
            },
            Self::Intrinsic { id, args } => Self::Intrinsic {
                id: *id,
                args: args.clone(),
            },
            Self::X86RflagsRegion { plan, args } => Self::X86RflagsRegion {
                plan: plan.clone(),
                args: args.clone(),
            },
            Self::InlineAsm { asm, args } => Self::InlineAsm {
                asm: asm.clone(),
                args: args.clone(),
            },
            Self::Jump { dest } => Self::Jump { dest: dest.clone() },
            Self::Brif { cond, dests } => Self::Brif {
                cond: *cond,
                dests: dests.clone(),
            },
            Self::Switch { arg, dests, cases } => Self::Switch {
                arg: *arg,
                dests: dests.clone(),
                cases: cases.clone(),
            },
            Self::Return { args } => Self::Return { args: args.clone() },
            Self::ReturnCall { func, args } => Self::ReturnCall {
                func: *func,
                args: args.clone(),
            },
            Self::Invoke { func, args, dests } => Self::Invoke {
                func: *func,
                args: args.clone(),
                dests: dests.clone(),
            },
            Self::LandingPad { clauses } => Self::LandingPad {
                clauses: clauses.clone(),
            },
            Self::ReturnCallIndirect { sig, callee, args } => Self::ReturnCallIndirect {
                sig: *sig,
                callee: *callee,
                args: args.clone(),
            },
        }
    }
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

impl Ordering {
    /// One ordering for a compare-and-swap's two: the weakest that is at
    /// least as strong as either, as LLVM merges them. A lowering whose
    /// failure path runs only the load uses this for both paths.
    pub fn merge(success: Ordering, failure: Ordering) -> Ordering {
        use Ordering::*;
        if success == SeqCst || failure == SeqCst {
            return SeqCst;
        }
        let acquires = matches!(success, Acquire | AcqRel) || failure == Acquire;
        let releases = matches!(success, Release | AcqRel);
        match (acquires, releases) {
            (true, true) => AcqRel,
            (true, false) => Acquire,
            (false, true) => Release,
            (false, false) => Relaxed,
        }
    }

    /// The strongest failure ordering C allows with this success ordering:
    /// the success ordering without its release half.
    pub fn cas_failure(self) -> Ordering {
        match self {
            Ordering::Release => Ordering::Relaxed,
            Ordering::AcqRel => Ordering::Acquire,
            other => other,
        }
    }
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
    /// Extend the lowest destination-lane-count integer lanes of a 128-bit
    /// vector into wider integer lanes. Upper source lanes are ignored.
    /// Unlike scalar sext/uext, the result always remains 128 bits.
    SwidenLow,
    UwidenLow,
    /// Extend the highest destination-lane-count input lanes.
    SwidenHigh,
    UwidenHigh,
    /// Truncate every input lane into the low output lanes; remaining lanes zero.
    NarrowLow,
    /// Add adjacent signed/unsigned input lanes in double-width output lanes.
    SpairwiseAddLong,
    UpairwiseAddLong,
    /// Clamp each input lane to the signed/unsigned destination range, then
    /// narrow into low lanes, with all remaining lanes zero. No FPSR.QC effect.
    SnarrowSatLow,
    UnarrowSatLow,
    SunarrowSatLow,
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
    /// Reinterprets equal-size bits between vectors (or vector/i128),
    /// or between a scalar integer and float.
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
    #[inline]
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
    #[inline]
    pub fn can_trap(self) -> bool {
        matches!(
            self,
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem
        )
    }
}

impl UnaryOp {
    #[inline]
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
    /// An `iconst` of `ty` holding `imm` in canonical form: `imm` is taken
    /// as bits of `ty` and sign-extended.
    #[inline]
    pub fn iconst(ty: Type, imm: i64) -> Self {
        Self::Iconst {
            ty,
            imm: ty.canonical_imm(imm),
        }
    }

    /// A compile-time object-lifetime boundary with no machine operand uses.
    #[inline]
    pub fn is_lifetime_marker(&self) -> bool {
        matches!(
            self,
            Self::Intrinsic {
                id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                ..
            }
        )
    }

    #[inline]
    pub fn is_vector_instruction(&self) -> bool {
        matches!(
            self,
            Self::Convert {
                op: ConvertOp::SwidenLow
                    | ConvertOp::UwidenLow
                    | ConvertOp::SwidenHigh
                    | ConvertOp::UwidenHigh
                    | ConvertOp::NarrowLow
                    | ConvertOp::SpairwiseAddLong
                    | ConvertOp::UpairwiseAddLong
                    | ConvertOp::SnarrowSatLow
                    | ConvertOp::UnarrowSatLow
                    | ConvertOp::SunarrowSatLow,
                ..
            } | Self::VectorSplat { .. }
                | Self::VectorUnary { .. }
                | Self::VectorReduce { .. }
                | Self::VectorBinary { .. }
                | Self::VectorExtract { .. }
                | Self::VectorInsert { .. }
                | Self::VectorShuffle { .. }
                | Self::VectorIcmp { .. }
                | Self::VectorFcmp { .. }
                | Self::VectorSelect { .. }
        )
    }

    /// Pinned instructions keep their position; the rest are pure and float,
    /// placed only by the scheduler.
    #[inline]
    pub fn is_pinned(&self) -> bool {
        match self {
            InstData::Binary { op, .. } => op.can_trap(),
            InstData::Neon { .. } => true,
            InstData::Intrinsic { id, .. } => !id.info().pure,
            InstData::Iconst { .. }
            | InstData::VectorSplat { .. }
            | InstData::VectorUnary { .. }
            | InstData::VectorReduce { .. }
            | InstData::VectorBinary { .. }
            | InstData::VectorExtract { .. }
            | InstData::VectorInsert { .. }
            | InstData::VectorShuffle { .. }
            | InstData::VectorIcmp { .. }
            | InstData::VectorFcmp { .. }
            | InstData::VectorSelect { .. }
            | InstData::Fconst { .. }
            | InstData::F80const { .. }
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
            | InstData::AggregateVaArg { .. }
            | InstData::Call { .. }
            | InstData::CallIndirect { .. }
            | InstData::X86RflagsRegion { .. }
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

    /// Memory effects are distinct from scheduling: even a non-trapping,
    /// ordinary load remains pinned until a memory-aware pass proves a move.
    /// Keep classification exhaustive: new opcodes must declare their effects.
    #[inline]
    pub fn memory_effects(&self) -> MemoryEffects {
        use MemoryEffects::*;
        match self {
            Self::Load { flags, .. } if !flags.volatile => Read,
            Self::Store { flags, .. } if !flags.volatile => Write,
            Self::Load { .. }
            | Self::Store { .. }
            | Self::AtomicLoad { .. }
            | Self::AtomicStore { .. }
            | Self::AtomicRmw { .. }
            | Self::AtomicCas { .. }
            | Self::Fence { .. }
            | Self::Call { .. }
            | Self::CallIndirect { .. }
            | Self::Invoke { .. }
            | Self::ReturnCall { .. }
            | Self::ReturnCallIndirect { .. }
            | Self::X86RflagsRegion { .. }
            | Self::Neon { .. } => Barrier,
            Self::InlineAsm { asm, .. } => {
                use crate::{AsmClobber, AsmOperand};
                if asm.clobbers.contains(&AsmClobber::Memory)
                    || asm.inputs.contains(&AsmOperand::StackPointer)
                {
                    Barrier
                } else {
                    let read = asm
                        .inputs
                        .iter()
                        .any(|op| matches!(op, AsmOperand::Mem | AsmOperand::MemReadWrite));
                    let write = asm
                        .inputs
                        .iter()
                        .any(|op| matches!(op, AsmOperand::MemWrite | AsmOperand::MemReadWrite));
                    match (read, write) {
                        (false, false) => None,
                        (true, false) => Read,
                        (false, true) => Write,
                        (true, true) => ReadWrite,
                    }
                }
            }
            Self::VaArg { .. } | Self::AggregateVaArg { .. } => ReadWrite,
            Self::Intrinsic { id, .. } => id.memory_effects(),
            Self::Iconst { .. }
            | Self::VectorSplat { .. }
            | Self::VectorUnary { .. }
            | Self::VectorReduce { .. }
            | Self::VectorBinary { .. }
            | Self::VectorExtract { .. }
            | Self::VectorInsert { .. }
            | Self::VectorShuffle { .. }
            | Self::VectorIcmp { .. }
            | Self::VectorFcmp { .. }
            | Self::VectorSelect { .. }
            | Self::Fconst { .. }
            | Self::F80const { .. }
            | Self::Unary { .. }
            | Self::Overflow { .. }
            | Self::Fma { .. }
            | Self::Select { .. }
            | Self::Convert { .. }
            | Self::Icmp { .. }
            | Self::Fcmp { .. }
            | Self::StackAddr { .. }
            | Self::SymbolAddr { .. }
            | Self::EhTypeid { .. }
            | Self::Jump { .. }
            | Self::Brif { .. }
            | Self::Switch { .. }
            | Self::Return { .. }
            | Self::LandingPad { .. }
            | Self::Binary { .. }
            | Self::Trap => None,
        }
    }

    /// Conservative possibility of a fault, trap, or exceptional exit.
    /// `false` alone permits neither deletion of side effects nor motion
    /// across memory writes, barriers, or lifetime changes.
    #[inline]
    pub fn may_trap(&self) -> bool {
        match self {
            Self::Binary { op, .. } => op.can_trap(),
            Self::Load { flags, .. } | Self::Store { flags, .. } => !flags.notrap,
            Self::AtomicLoad { .. }
            | Self::AtomicStore { .. }
            | Self::AtomicRmw { .. }
            | Self::AtomicCas { .. }
            | Self::VaArg { .. }
            | Self::AggregateVaArg { .. }
            | Self::Call { .. }
            | Self::CallIndirect { .. }
            | Self::Invoke { .. }
            | Self::ReturnCall { .. }
            | Self::ReturnCallIndirect { .. }
            | Self::X86RflagsRegion { .. }
            | Self::InlineAsm { .. }
            | Self::Trap => true,
            Self::Neon { .. } => true,
            Self::Intrinsic { id, .. } => id.may_trap(),
            Self::Iconst { .. }
            | Self::VectorSplat { .. }
            | Self::VectorUnary { .. }
            | Self::VectorReduce { .. }
            | Self::VectorBinary { .. }
            | Self::VectorExtract { .. }
            | Self::VectorInsert { .. }
            | Self::VectorShuffle { .. }
            | Self::VectorIcmp { .. }
            | Self::VectorFcmp { .. }
            | Self::VectorSelect { .. }
            | Self::Fconst { .. }
            | Self::F80const { .. }
            | Self::Unary { .. }
            | Self::Overflow { .. }
            | Self::Fma { .. }
            | Self::Select { .. }
            | Self::Convert { .. }
            | Self::Icmp { .. }
            | Self::Fcmp { .. }
            | Self::StackAddr { .. }
            | Self::SymbolAddr { .. }
            | Self::EhTypeid { .. }
            | Self::Jump { .. }
            | Self::Brif { .. }
            | Self::Switch { .. }
            | Self::Return { .. }
            | Self::LandingPad { .. }
            | Self::Fence { .. } => false,
        }
    }

    #[inline]
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

    #[inline]
    pub fn branch_dests(&self) -> &[BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_ref(dest),
            InstData::Brif { dests, .. } | InstData::Invoke { dests, .. } => &dests[..],
            InstData::Switch { dests, .. } => dests,
            _ => &[],
        }
    }

    #[inline]
    pub fn branch_dests_mut(&mut self) -> &mut [BlockCall] {
        match self {
            InstData::Jump { dest } => std::slice::from_mut(dest),
            InstData::Brif { dests, .. } | InstData::Invoke { dests, .. } => &mut dests[..],
            InstData::Switch { dests, .. } => dests,
            _ => &mut [],
        }
    }

    /// Types hidden behind ABI pointers, in addition to ordinary SSA types.
    /// A type may be visited more than once, and an array's element types
    /// are visited once rather than once per element.
    /// Capability checks must visit these even when every SSA value is ptr.
    pub fn for_each_abi_type(&self, visit: impl FnMut(Type)) {
        self.for_each_abi_type_filtered(visit, |_, _| true);
    }

    /// Like for_each_abi_type, with a target-proved filter for layouts that use
    /// byte copies rather than SIMD values. Call operand indices remain exact;
    /// None identifies the AggregateVaArg layout. Ordinary SSA types are always
    /// visited. The default visitor above includes every layout unchanged.
    pub fn for_each_abi_type_filtered(
        &self,
        mut visit: impl FnMut(Type),
        mut include: impl FnMut(&crate::Layout, Option<usize>) -> bool,
    ) {
        if let Some(args) = self.call_args() {
            for i in 0..args.len() {
                if let Some(p) = args.abi(i) {
                    visit(p.ty);
                    if let Some(layout) = p.layout()
                        && include(layout, Some(i))
                    {
                        layout.for_each_leaf_type(&mut visit);
                    }
                }
            }
        }
        if let Self::AggregateVaArg { layout, .. } = self
            && include(layout, None)
        {
            layout.for_each_leaf_type(&mut visit);
        }
    }

    /// ABI-bearing operands for all call forms, including guaranteed tails.
    pub fn call_args(&self) -> Option<&crate::CallArgs> {
        match self {
            Self::Call { args, .. }
            | Self::CallIndirect { args, .. }
            | Self::Invoke { args, .. }
            | Self::ReturnCall { args, .. }
            | Self::ReturnCallIndirect { args, .. } => Some(args),
            _ => None,
        }
    }

    /// Visits every value operand, including block-call arguments.
    pub fn for_each_operand(&self, mut f: impl FnMut(Value)) {
        match self {
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::F80const { .. }
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
            InstData::VectorBinary { args, .. }
            | InstData::VectorInsert { args, .. }
            | InstData::VectorShuffle { args, .. }
            | InstData::VectorIcmp { args, .. }
            | InstData::VectorFcmp { args, .. }
            | InstData::Binary { args, .. }
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
            InstData::VectorSelect { args }
            | InstData::Select { args }
            | InstData::Fma { args } => args.iter().copied().for_each(f),
            InstData::VectorSplat { arg, .. }
            | InstData::VectorUnary { arg, .. }
            | InstData::VectorReduce { arg, .. }
            | InstData::VectorExtract { arg, .. }
            | InstData::Unary { arg, .. } => f(*arg),
            InstData::Convert { arg, .. } => f(*arg),
            InstData::Load { addr, .. } => f(*addr),
            InstData::VaArg { list, .. } | InstData::AggregateVaArg { list, .. } => f(*list),
            InstData::Store { value, addr, .. } => {
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
            InstData::Call { args, .. } | InstData::ReturnCall { args, .. } => {
                args.iter().copied().for_each(f)
            }
            InstData::Return { args }
            | InstData::X86RflagsRegion { args, .. }
            | InstData::InlineAsm { args, .. }
            | InstData::Neon { args, .. }
            | InstData::Intrinsic { args, .. } => args.iter().copied().for_each(f),
        }
    }

    pub fn for_each_operand_mut(&mut self, mut f: impl FnMut(&mut Value)) {
        match self {
            InstData::Iconst { .. }
            | InstData::Fconst { .. }
            | InstData::F80const { .. }
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
            InstData::VectorBinary { args, .. }
            | InstData::VectorInsert { args, .. }
            | InstData::VectorShuffle { args, .. }
            | InstData::VectorIcmp { args, .. }
            | InstData::VectorFcmp { args, .. }
            | InstData::Binary { args, .. }
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
            InstData::VectorSelect { args }
            | InstData::Select { args }
            | InstData::Fma { args } => args.iter_mut().for_each(f),
            InstData::VectorSplat { arg, .. }
            | InstData::VectorUnary { arg, .. }
            | InstData::VectorReduce { arg, .. }
            | InstData::VectorExtract { arg, .. }
            | InstData::Unary { arg, .. } => f(arg),
            InstData::Convert { arg, .. } => f(arg),
            InstData::Load { addr, .. } => f(addr),
            InstData::VaArg { list, .. } | InstData::AggregateVaArg { list, .. } => f(list),
            InstData::Store { value, addr, .. } => {
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
            InstData::Call { args, .. } | InstData::ReturnCall { args, .. } => {
                args.iter_mut().for_each(f)
            }
            InstData::Return { args }
            | InstData::X86RflagsRegion { args, .. }
            | InstData::InlineAsm { args, .. }
            | InstData::Neon { args, .. }
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
            ConvertOp::SwidenLow => "swiden_low",
            ConvertOp::UwidenLow => "uwiden_low",
            ConvertOp::SwidenHigh => "swiden_high",
            ConvertOp::UwidenHigh => "uwiden_high",
            ConvertOp::NarrowLow => "narrow_low",
            ConvertOp::SpairwiseAddLong => "spairwise_add_long",
            ConvertOp::UpairwiseAddLong => "upairwise_add_long",
            ConvertOp::SnarrowSatLow => "snarrow_sat_low",
            ConvertOp::UnarrowSatLow => "unarrow_sat_low",
            ConvertOp::SunarrowSatLow => "sunarrow_sat_low",
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
