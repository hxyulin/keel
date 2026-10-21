//! IR → Apple arm64 assembly text, with values in registers chosen by the
//! linear-scan allocator in [`crate::regalloc`].
//!
//! # Registers
//!
//! ```text
//! x0–x7    arguments and results of calls; never allocated
//! x8       scratch (keel does not return structs through it yet)
//! x9–x15   allocatable, caller-saved: for values not live across a call
//! x16–x17  scratch (IP0/IP1, which the ABI already lets veneers clobber)
//! x18      reserved by Apple; never touched
//! x19–x28  allocatable, callee-saved: saved in the prologue if used
//! x29–x30  frame pointer and return address
//!
//! v0–v7    float arguments and results; never allocated
//! v8–v15   allocatable, callee-saved (only the low 64 bits, d8–d15,
//!          which is all a scalar float uses)
//! v16–v18  scratch
//! v19–v31  allocatable, caller-saved
//! ```
//!
//! Floats live in the SIMD/FP registers, named `s<n>` as f32 and `d<n>` as
//! f64; the same rules apply as for integers, with v16–v18 as the scratch
//! registers.
//!
//! Keeping x0–x7 out of the pool means setting up a call never has to move
//! a live value out of the way, and a spilled operand can always be
//! reloaded into a scratch register. Each instruction is lowered with its
//! operands in their allocated registers (or reloaded into x16, x17, x8 if
//! spilled) and computes straight into its result's register (or into x16
//! and stores it, if the result is spilled).
//!
//! A result may share a register with an operand that dies at the same
//! instruction (see the regalloc docs on program points), so **a lowering
//! must read every operand before it writes the destination**. Lowerings
//! that need several steps build intermediate values in scratch registers
//! and write the destination last.
//!
//! Values narrower than 64 bits have unspecified upper bits. Operations
//! whose result depends on those bits (comparisons, division, right shifts,
//! extensions) extend their operands first, into a scratch register.
//!
//! # Frame
//!
//! From the stack pointer up:
//!
//! ```text
//! sp + 0              outgoing stack arguments, for the call that needs most
//! ...                 copies of big structs that calls pass by value
//! ...                 a slot keeping a call's sret address across the call
//! ...                 struct parameters and an sret result held in registers
//! ...                 spill slots, 8 bytes each
//! ...                 IR stack slots, each at its alignment
//! ...                 padding to 16 bytes
//! ...                 callee-saved FP registers this function uses, in pairs
//! x29 - 16 * pairs    callee-saved general registers it uses, in pairs
//! x29                 saved x29 (frame pointer), saved x30 (return address)
//! x29 + 16            incoming stack arguments, in the caller's frame
//! ```
//!
//! Calls store their stack arguments at `sp` directly: the area is part of
//! the fixed frame, so `sp` never moves after the prologue, except in a
//! function that allocates stack dynamically (`stack_alloc`,
//! `stack_restore`). There, x19 is the *base pointer*: it keeps the value
//! `sp` had after the prologue, and everything in the fixed frame but the
//! outgoing arguments is addressed from it. The outgoing arguments stay at
//! the bottom, at whatever `sp` is: `stack_alloc` moves `sp` down by the
//! size, rounded to 16, and returns the memory just above the outgoing
//! area, part of which held the old outgoing area.
//!
//! # Symbols
//!
//! Code may not contain absolute addresses: the executable is loaded at a
//! random address. A symbol's address is computed relative to the program
//! counter: `adrp` gives its 4 KiB page, `add` the offset within the page.
//! Symbols defined in another image (libc, a dylib) are only known at load
//! time, so their addresses come from the global offset table (GOT), a table
//! of pointers the loader fills in: `adrp` + `ldr` reads the GOT entry.
//!
//! # Debug info
//!
//! Every function has call frame information (`.cfi_*`), which unwinders
//! and debuggers need to walk through its frame. The frame is addressed
//! from x29, so the CFA is `x29 + 16` from the end of `mov x29, sp` on,
//! and the saved registers sit at fixed offsets from it.
//!
//! If the module names source files, instructions with a location get a
//! `.loc` directive, from which the assembler builds the DWARF line table.
//! The prologue takes the function's `dloc`, or else the first location
//! in the function, with column 0,
//! and the first located instruction is marked `prologue_end`, where a
//! debugger stops for a breakpoint on the function. An instruction
//! without a location continues the previous one's. `__debug_info` has
//! a compile unit and a subprogram per function, which makes the linker
//! record the object in the executable's debug map, which is how lldb
//! finds the line table; and the module's debug types, globals and
//! variables, which are in stack slots at fixed offsets from x29.

use crate::abi::{self, ArgKind, ArgLoc, Slot, StructClass};
use crate::data;
use crate::dwarf::{self, FrameDebug};
use crate::fold::{self, iconst};
use crate::integer_division::{reducible_divisor, signed_reciprocal, unsigned_reciprocal};
use crate::metadata::{Cfi, Directive, Expression, SectionKind, Sink};
use crate::parallel_copy::sequentialize;
use crate::regalloc::{self, Allocation, Loc, Pools, RematRecipe};
use crate::{CodegenError, Format, loc_reg, pieces};
use keel_ir::fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use keel_ir::{
    AbiParam, ArgExt, AsmClobber, AsmOperand, BinaryOp, Block, BlockCall, CallConv, Clause,
    ConvertOp, DataRef, DebugScope, EntityRef, FloatCC, Function, InlineAsm, Inst, InstData, IntCC,
    Intrinsic, Layout, Module, Ordering, OverflowOp, PhysReg, Purpose, RmwOp, Signature,
    SourceFile, SourceLoc, Type, UnaryOp, Value, ValueDef,
};
use std::fmt::Write;
mod absolute;
mod absolute_diff;
mod constant_plan;
#[cfg(feature = "experimental-dispatch")]
mod dispatch_tail;
mod extended_add;
mod leaf_calls;
mod native;
mod output;
mod packed_load;
mod packed_minmax;
mod vector;
use keel_asm_ir::{FieldId, Instruction as NativeInstruction, Opcode};
use keel_asm_ir::{PageKind, SymbolicInstruction as Symbolic};
use native::Piece::{Class, Hex, Number, Reg as R, Text};
use native::{Address, Arg, Reg};
use output::Output;

/// The registers values are allocated to.
pub const REGISTERS: Pools = Pools {
    caller_saved: &[9, 10, 11, 12, 13, 14, 15],
    callee_saved: &[19, 20, 21, 22, 23, 24, 25, 26, 27, 28],
    float_caller_saved: &[19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31],
    float_callee_saved: &[8, 9, 10, 11, 12, 13, 14, 15],
};

/// Scratch registers, never allocated. Spilled operands are reloaded into
/// them in this order, a spilled result is computed in `S0`, and lowerings
/// use them for intermediate values.
const S0: u8 = 16;
const S1: u8 = 17;
const S2: u8 = 8;

/// The base pointer of functions that move `sp`; see "Frame".
const BASE: u8 = 19;

/// FP scratch registers, used like `S0`–`S2` for floats.
const FS0: u8 = 16;
const FS1: u8 = 17;
const FS2: u8 = 18;

/// The registers inline assembly operands go in: the argument and scratch
/// registers, which are never allocated, so moving operands in and out of
/// them disturbs no value.
const ASM_REGS: [u8; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17];
const ASM_FREGS: [u8; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 16, 17, 18];

/// Compiles a module into one assembly file for `clang`/`as`. Declared
/// functions are left as undefined symbols for the linker to resolve.
pub fn compile(module: &Module) -> Result<String, CodegenError> {
    compile_with(module, &REGISTERS)
}

/// Like [`compile`], allocating only from `pools`, which must be drawn from
/// [`REGISTERS`]. Tests use a tiny set to force spills.
pub fn compile_with(module: &Module, pools: &Pools) -> Result<String, CodegenError> {
    compile_with_options(module, pools, crate::SelectionOptions::default())
}

/// Like [`compile_with`], with optional target selection passes enabled.
pub fn compile_with_options(
    module: &Module,
    pools: &Pools,
    options: crate::SelectionOptions,
) -> Result<String, CodegenError> {
    let selector = crate::OptionsSelector(options);
    compile_with_pipeline(
        module,
        pools,
        &crate::CodegenPipeline {
            selector: &selector,
            allocator: &regalloc::AllocationTier::Full,
        },
    )
}

/// Compile using independently replaceable selection and allocation stages.
pub fn compile_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<String, CodegenError> {
    match compile_output(module, pools, pipeline, false, None)? {
        Output::Text(text) => Ok(text),
        _ => unreachable!(),
    }
}

/// Emit structured assembly, with explicit source fragments for unmigrated
/// instruction/directive paths and user inline assembly.
pub fn compile_program_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
) -> Result<keel_asm_ir::Program, CodegenError> {
    match compile_output(module, pools, pipeline, true, None)? {
        Output::Structured(program) => Ok(program),
        _ => unreachable!(),
    }
}

/// Hand the program to `sink` in order, at least once per function.
pub fn compile_program_chunks_with_pipeline(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
    sink: &mut dyn FnMut(keel_asm_ir::Program),
) -> Result<(), CodegenError> {
    match compile_output(module, pools, pipeline, true, Some(&mut *sink))? {
        Output::Structured(program) => sink(program),
        _ => unreachable!(),
    }
    Ok(())
}

fn compile_output(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
    structured: bool,
    mut sink: Option<&mut dyn FnMut(keel_asm_ir::Program)>,
) -> Result<Output, CodegenError> {
    crate::validate_odr(module, crate::Target::Aarch64AppleDarwin)?;
    backend_scope!(Module);
    pipeline.validate_target(crate::Target::Aarch64AppleDarwin)?;
    let subset = |pool: &[u8], of: &[u8]| pool.iter().all(|r| of.contains(r));
    assert!(
        subset(pools.caller_saved, REGISTERS.caller_saved)
            && subset(pools.callee_saved, REGISTERS.callee_saved)
            && subset(pools.float_caller_saved, REGISTERS.float_caller_saved)
            && subset(pools.float_callee_saved, REGISTERS.float_callee_saved),
        "register pools must be subsets of REGISTERS"
    );
    let mut out = Output::new(structured);
    out.section("__TEXT", "__text", SectionKind::Data);
    let debug = !module.files.is_empty() && !module.functions.is_empty();
    if debug {
        for (i, path) in module.files.iter().enumerate() {
            out.directive(Directive::File {
                index: (i + 1) as u32,
                path: path.as_bytes().to_vec(),
            });
        }
    }
    let symbols = crate::symbols::Symbols::new(module);
    let (leaf_summaries, mut prepared_leaves) =
        leaf_calls::prepare(&symbols, pools, pipeline, structured)?;
    let mut frames = Vec::new();
    for (index, original) in module.functions.iter().enumerate() {
        if let Some((body, scope_marks)) = prepared_leaves.remove(&index) {
            out.append(body);
            frames.push(FrameDebug {
                slots: Vec::new(),
                scope_marks,
            });
            out.flush(&mut sink);
            continue;
        }
        #[cfg(feature = "backend-profile")]
        let _function_profile = crate::backend_profile::function(&original.name);
        let func = vector::lower(original);
        let mut compiler = FunctionCompiler::new(
            &func,
            &symbols,
            pools,
            pipeline,
            structured,
            &leaf_summaries,
        )?;
        compiler.debug = debug;
        let slots = if debug {
            compiler.slot_frame_offsets()
        } else {
            Vec::new()
        };
        let scope_marks = compiler.compile(&mut out);
        frames.push(FrameDebug { slots, scope_marks });
        out.flush(&mut sink);
    }
    {
        backend_scope!(Data);
        for data in &module.data {
            if let Some(init) = &data.init {
                data::emit_macho(&mut out, data, init);
            }
        }
    }
    if debug {
        backend_scope!(Debug);
        // x29 is DWARF register 29.
        dwarf::emit_debug_info(&mut out, Format::MachO, 29, module, &frames);
    }
    out.directive(Directive::SubsectionsViaSymbols);
    Ok(out)
}

/// How far a conditional branch (`b.cond`, `cbz`, `cbnz`) reaches: a
/// signed 19-bit count of instructions, ±1 MiB.
const COND_BRANCH_RANGE: usize = 1 << 20;

/// Makes the conditional branches of a function too big for them to be
/// sure to reach their targets into a branch on the opposite condition
/// over a `b`, which reaches ±128 MiB. Interpreter loops get that big.
/// Relaxes the function whose text is `out[start..]`.
fn relax_branches(out: &mut String, start: usize, name: &str) {
    // An upper bound on the size: every line that is not a label, a
    // comment or a directive is one instruction; data in the code is
    // jump table entries, four bytes each. Each counted line takes at least
    // one byte of text, so short functions need no line scan at all.
    if (out.len() - start).saturating_mul(4) < COND_BRANCH_RANGE {
        return;
    }
    let size: usize = out[start..]
        .lines()
        .filter(|l| l.starts_with('\t') && !l.starts_with("\t;"))
        .filter(|l| !l.starts_with("\t.") || l.starts_with("\t.long"))
        .count()
        * 4;
    if size < COND_BRANCH_RANGE {
        return;
    }
    let mut text = out.split_off(start);
    relax_text(&mut text, name, &mut 0);
    out.push_str(&text);
}
fn relax_text(out: &mut String, name: &str, n: &mut usize) {
    let mut relaxed = String::with_capacity(out.len() + out.len() / 8);
    for line in out.lines() {
        // The inverted branch up to its target, which skips the `b`.
        let far = |inverse: String, target: &str| {
            format!("\t{inverse}L{name}_near{n}\n\tb\t{target}\nL{name}_near{n}:\n")
        };
        let rewritten = if let Some((cond, target)) = line
            .strip_prefix("\tb.")
            .and_then(|rest| rest.split_once('\t'))
        {
            Some(far(format!("b.{}\t", invert_cond(cond)), target))
        } else if let Some((op, rest)) = line
            .strip_prefix("\tcbz\t")
            .map(|r| ("cbnz", r))
            .or_else(|| line.strip_prefix("\tcbnz\t").map(|r| ("cbz", r)))
        {
            let (reg, target) = rest.split_once(", ").expect("cbz reg, label");
            Some(far(format!("{op}\t{reg}, "), target))
        } else {
            None
        };
        match rewritten {
            Some(code) => {
                relaxed.push_str(&code);
                *n += 1;
            }
            None => {
                relaxed.push_str(line);
                relaxed.push('\n');
            }
        }
    }
    *out = relaxed;
}

/// The condition code that holds when `cond` does not.
fn invert_cond(cond: &str) -> &'static str {
    match cond {
        "eq" => "ne",
        "ne" => "eq",
        "hs" | "cs" => "lo",
        "lo" | "cc" => "hs",
        "mi" => "pl",
        "pl" => "mi",
        "vs" => "vc",
        "vc" => "vs",
        "hi" => "ls",
        "ls" => "hi",
        "ge" => "lt",
        "lt" => "ge",
        "gt" => "le",
        "le" => "gt",
        _ => panic!("unknown condition `{cond}`"),
    }
}

/// The offset of the stack slot whose address `addr` is, if an access of
/// type `ty` can address it directly: `ldr`/`str`'s immediate is 12 bits
/// unsigned, scaled by the access size.
fn folded_offset(
    func: &Function,
    offsets: &crate::stack_frame::SlotOffsets,
    addr: Value,
    ty: Type,
) -> Option<usize> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(addr)) else {
        return None;
    };
    let InstData::StackAddr { slot } = *func.inst_data(inst) else {
        return None;
    };
    let offset = offsets[slot.index()];
    let size = ty.bytes()? as usize;
    ((size <= 8 || ty.is_vector()) && offset.is_multiple_of(size) && offset / size < 4096)
        .then_some(offset)
}

/// One checked plan shared by preallocation folding and emission. Only exact
/// execution-width integers participate; pointer/narrow lowering stays unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AddSubImmediate {
    constant: Value,
    input: Value,
    imm12: u32,
    shift: u8,
    subtract: bool,
}

fn addsub_immediate(func: &Function, data: &InstData) -> Option<AddSubImmediate> {
    let (args, subtract, commute, compare) = match *data {
        InstData::Binary {
            op: BinaryOp::Iadd,
            args,
        } => (args, false, true, false),
        InstData::Binary {
            op: BinaryOp::Isub,
            args,
        } => (args, true, false, false),
        InstData::Icmp { cond, args } => {
            // Replacing subtraction by addition preserves the result and Z,
            // but not every NZCV bit. Only EQ/NE can use a negative constant.
            (args, true, false, !matches!(cond, IntCC::Eq | IntCC::Ne))
        }
        _ => return None,
    };
    let ty = func.value_type(func.resolve(args[0]));
    if !matches!(ty, Type::I32 | Type::I64) {
        return None;
    }
    for side in if commute { &[1, 0][..] } else { &[1][..] } {
        let constant = func.resolve(args[*side]);
        let Some(raw) = iconst(func, constant) else {
            continue;
        };
        let signed = if ty == Type::I32 {
            raw as i32 as i64
        } else {
            raw
        };
        if compare && signed < 0 {
            continue;
        }
        let magnitude = signed.unsigned_abs();
        let (imm12, shift) = if magnitude < 4096 {
            (magnitude as u32, 0)
        } else if magnitude & 4095 == 0 && magnitude >> 12 < 4096 {
            ((magnitude >> 12) as u32, 12)
        } else {
            continue;
        };
        return Some(AddSubImmediate {
            constant,
            input: func.resolve(args[1 - *side]),
            imm12,
            shift,
            subtract: subtract ^ (signed < 0),
        });
    }
    None
}

/// The operand of `data` that is a constant encoded in the instruction
/// itself, and that immediate: typed add/sub/compare plans can reverse the
/// operation and shift a 12-bit magnitude; legacy narrow/pointer forms stay
/// unsigned and unshifted. I32/I64 logical masks use the execution width, a shift amount
/// is taken modulo the width, and a stored zero
/// is the zero register.
fn folded_imm(func: &Function, data: &InstData) -> Option<(Value, i64)> {
    if let Some(plan) = addsub_immediate(func, data) {
        return Some((plan.constant, i64::from(plan.imm12) << plan.shift));
    }
    let fits = |v| iconst(func, v).filter(|k| (0..4096).contains(k));
    match *data {
        InstData::Binary { op, args } if func.value_type(func.resolve(args[0])) != Type::I128 => {
            let ty = func.value_type(func.resolve(args[0]));
            match op {
                BinaryOp::Iadd | BinaryOp::Ptradd | BinaryOp::Isub => {
                    fits(args[1]).map(|k| (args[1], k))
                }
                BinaryOp::And | BinaryOp::Or | BinaryOp::Xor
                    if matches!(ty, Type::I32 | Type::I64) =>
                {
                    // These operations commute. Prefer the RHS, but also fold
                    // raw IR with a constant on the LHS, without an IR rewrite.
                    [args[1], args[0]].into_iter().find_map(|v| {
                        let k = iconst(func, v)?;
                        native::logical_immediate(k as u64, if ty == Type::I32 { 32 } else { 64 })
                            .map(|plan| (func.resolve(v), plan.value as i64))
                    })
                }
                BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                    let bits = 8 * ty.bytes()? as i64;
                    iconst(func, args[1]).map(|k| (args[1], k & (bits - 1)))
                }
                _ => None,
            }
        }
        // A narrow operand is extended before the compare, so the constant
        // must read the same zero- or sign-extended.
        InstData::Icmp { args, .. } => {
            let ty = func.value_type(func.resolve(args[0]));
            let k = fits(args[1])?;
            let narrow = narrow_bits(ty).is_some_and(|bits| k >= 1 << (bits - 1));
            (ty != Type::I128 && !narrow).then_some((args[1], k))
        }
        InstData::Store { value, .. } => (iconst(func, value) == Some(0)
            && !func.value_type(func.resolve(value)).is_float())
        .then_some((value, 0)),
        _ => None,
    }
}

/// Appends the operands of `data` folded into it: a stack address
/// [`folded_offset`] addresses directly, and a [`folded_imm`].
fn folds(
    func: &Function,
    offsets: &crate::stack_frame::SlotOffsets,
    data: &InstData,
    out: &mut Vec<Value>,
) {
    let addr = match *data {
        InstData::Load { ty, addr, .. } => Some((addr, ty)),
        InstData::Store { value, addr, .. } => Some((addr, func.value_type(func.resolve(value)))),
        _ => None,
    }
    .filter(|&(addr, ty)| folded_offset(func, offsets, addr, ty).is_some())
    .map(|(addr, _)| addr);
    out.extend(addr);
    out.extend(folded_imm(func, data).map(|(v, _)| v));
    out.extend(folded_divisor(func, data));
}

/// A quotient by a [`reducible_divisor`] never reads the divisor's register.
fn folded_divisor(func: &Function, data: &InstData) -> Option<Value> {
    match *data {
        InstData::Binary {
            op: BinaryOp::Sdiv | BinaryOp::Udiv,
            args,
        } if reducible_divisor(func, data).is_some() => Some(func.resolve(args[1])),
        _ => None,
    }
}

/// Instructions that clobber the caller-saved registers: calls, the memory
/// intrinsics, which are calls to libc, `i128` division, a call to
/// compiler-rt, and inline assembly that says so.
fn is_call(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. } => true,
        // Inline assembly that clobbers allocatable caller-saved registers
        // is treated like a call, which keeps values out of all of them.
        InstData::InlineAsm { asm, .. } => asm.clobbers.iter().any(|c| match c {
            AsmClobber::Reg(PhysReg { float: false, num }) => REGISTERS.caller_saved.contains(num),
            AsmClobber::Reg(PhysReg { float: true, num }) => {
                REGISTERS.float_caller_saved.contains(num)
            }
            AsmClobber::Memory | AsmClobber::Flags => false,
        }),
        InstData::Binary { op, args } => {
            op.can_trap() && func.value_type(func.resolve(args[0])) == Type::I128
        }
        InstData::Intrinsic { id, .. } => match id {
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp => true,
            Intrinsic::Debugtrap
            | Intrinsic::VaStart
            | Intrinsic::StackAlloc
            | Intrinsic::StackSave
            | Intrinsic::StackRestore
            | Intrinsic::FrameAddress => false,
        },
        _ => false,
    }
}

/// Calls that may return a second time, from a `longjmp`.
fn returns_twice(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { func: callee, .. } | InstData::Invoke { func: callee, .. } => {
            func.callee(*callee).sig.returns_twice
        }
        InstData::CallIndirect { sig, .. } => func.signature(*sig).returns_twice,
        _ => false,
    }
}

/// The largest offset `ldr x, [sp, #imm]` can encode (12 bits, scaled by 8).
/// Spill slots are addressed this way, so they must fit below it.
const MAX_SLOT_OFFSET: usize = 4095 * 8;

/// The largest frame `sub sp` can allocate in two instructions (24 bits).
const MAX_FRAME_SIZE: usize = (1 << 24) - 16;

/// The most bytes of stack arguments a call may pass or a function take.
/// Stack arguments are addressed with scaled 12-bit offsets (from `sp` by
/// the caller, from `x29 + 16` by the callee), which a byte-sized argument
/// beyond this could not reach.
const MAX_STACK_ARGS: u32 = 4080;

/// The predicate for taking a Brif's false edge. Keep fused comparisons on
/// NZCV; ordinary full-width integer truth tests can branch on the register.
enum BrifTest {
    Flags(&'static str),
    Zero(Reg),
}

struct RegisterSret<'a> {
    regs: Vec<Loc>,
    layout: &'a Layout,
    /// A fixed object can be addressed again after the call without keeping
    /// its pointer in the call's private save slot.
    stack_offset: Option<usize>,
}

struct FunctionCompiler<'a> {
    func: &'a Function,
    symbols: &'a crate::symbols::Symbols<'a>,
    alloc: Allocation,
    #[cfg(feature = "experimental-dispatch")]
    dispatch_tails: HashMap<Inst, Inst>,
    current_inst: Option<Inst>,
    allocation_index: Option<usize>,
    selection: crate::select::Selection,
    replicate_loads: HashMap<Inst, Value>,
    packed_absolute_values: HashMap<Inst, Value>,
    packed_low_loads: HashMap<Inst, Value>,
    packed_minmax: HashMap<Inst, packed_minmax::MinMax>,
    extended_adds: HashMap<Inst, extended_add::ExtendedAdd>,
    absolute_values: HashMap<Inst, Value>,
    widening_adds: HashMap<Inst, vector::WideningAdd>,
    absolute_differences: HashMap<Inst, absolute_diff::ExactSabal>,
    /// Offset from `sp` of each IR stack slot.
    stack_slot_offsets: crate::stack_frame::SlotOffsets,
    /// `stack_addr` results only used as the address of loads and stores
    /// that address the slot directly (`[sp, #offset]`): the address
    /// itself is never computed.
    folded_values: fold::ValueSet,
    fused_compares: HashSet<Value>,
    /// Inline assembly has no declared flag inputs. Preserve the old NZCV
    /// definitions throughout such functions, including across block edges.
    has_inline_asm: bool,
    /// Offset from `sp` of the area where calls put copies of big structs
    /// passed by value.
    copy_base: usize,
    /// Offset from `sp` of an 8-byte slot keeping a call's `sret` address
    /// across the call, for results that come back in registers.
    sret_save: Option<usize>,
    /// Offset from `sp` of the memory behind each `byval` parameter that
    /// arrived in registers, and behind an `sret` result returned in them.
    param_areas: Vec<Option<usize>>,
    /// Offset from `sp` of spill slot 0, above the areas for calls.
    spill_base: usize,
    vector_spills: HashSet<u32>,
    copy_temp: Option<u32>,
    /// The register the fixed frame is addressed from: `sp`, or x19 in a
    /// function that moves `sp`.
    base: &'static str,
    /// Offset from x29 of the first variadic argument: the next 8-byte slot
    /// after the named stack parameters.
    variadic_offset: u32,
    /// Outgoing arguments, spill slots and IR stack slots; the
    /// callee-saved area is separate.
    frame_size: usize,
    /// A private leaf that neither needs nor observes a stack frame.
    frameless: bool,
    /// The block emitted after the current one, which a branch can fall
    /// through to.
    next_block: Option<Block>,
    out: Output,
    uses_trap: bool,
    /// Exception handling, for a function with a personality.
    eh: ExceptionTable,
    /// Whether to emit `.loc` directives.
    debug: bool,
    /// The location of the last `.loc`.
    srcloc: Option<SourceLoc>,
    /// Whether the next `.loc` ends the prologue.
    prologue_end: bool,
    /// The lexical scope of the code being emitted, and where it changed.
    scope: Option<DebugScope>,
    scope_marks: Vec<(String, Option<DebugScope>)>,
}

impl<'a> FunctionCompiler<'a> {
    /// The offset of each stack slot from `x29`, the frame pointer. The
    /// prologue sets `x29` to `sp` on entry, less the 16 bytes of its own
    /// pair, and then lowers `sp` by 16 per pair of saved registers and
    /// by the frame size; slots are at fixed offsets from that `sp`.
    /// Dynamic allocations move `sp` further, but `x29` stays put, which
    /// is why debuggers address variables from it.
    fn slot_frame_offsets(&self) -> Vec<i64> {
        let pairs = self.alloc.callee_saved_used.len().div_ceil(2)
            + self.alloc.float_callee_saved_used.len().div_ceil(2);
        let sp = -((16 * pairs + self.frame_size) as i64);
        self.stack_slot_offsets
            .debug_offsets()
            .map(|offset| sp + offset as i64)
            .collect()
    }

    fn new(
        func: &'a Function,
        symbols: &'a crate::symbols::Symbols<'a>,
        pools: &Pools,
        pipeline: &crate::CodegenPipeline<'_>,
        structured: bool,
        leaf_summaries: &leaf_calls::Summaries,
    ) -> Result<Self, CodegenError> {
        let module = symbols.module;
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        let (CallConv::C | CallConv::Fast) = func.sig.cc;
        let param_kinds = param_kinds(&func.sig.params);
        let (param_locs, incoming) = abi::arg_locs(&param_kinds, param_kinds.len());
        if incoming > MAX_STACK_ARGS {
            return Err(err(format!(
                "{incoming} bytes of stack parameters is more than the {MAX_STACK_ARGS} supported"
            )));
        }
        match func.sig.returns.as_slice() {
            [] => {}
            // The C ABI has the callee extend these to 32 bits. Private
            // cc(fast) edges only require the declared narrow value.
            [
                AbiParam {
                    ty: Type::I8 | Type::I16,
                    ext: ArgExt::None,
                    ..
                },
            ] if func.sig.cc == CallConv::C => {
                return Err(err(
                    "an i8/i16 return value needs `sext` or `uext`: C callers expect 32 bits"
                        .into(),
                ));
            }
            [_] => {}
            _ => {
                return Err(err(
                    "multiple return values are not supported by the C convention".into(),
                ));
            }
        }
        for (_, callee) in func.callees() {
            let what = || format!("call to @{}", callee.name);
            check_call_signature(what, &callee.sig, symbols.is_external(&callee.name))
                .map_err(err)?;
        }
        // The target of an indirect call may be an external function.
        for (_, sig) in func.signatures() {
            check_call_signature(|| format!("indirect call {sig}"), sig, true).map_err(err)?;
        }
        // One walk finds which (mostly rare) instruction kinds occur, so the
        // checks and covers below only scan functions that can match.
        let present = crate::kinds::InstKinds::of(func);
        for (_, inst) in func.layout_insts().filter(|_| present.high_multiply) {
            if let InstData::Binary {
                op: op @ (BinaryOp::Umulhi | BinaryOp::Smulhi),
                args,
            } = func.inst_data(inst)
                && func.value_type(func.resolve(args[0])) == Type::I128
            {
                return Err(err(format!("{op} on i128 is not supported yet")));
            }
            if let InstData::Overflow {
                op: op @ (OverflowOp::Umul | OverflowOp::Smul),
                args,
            } = func.inst_data(inst)
                && func.value_type(func.resolve(args[0])) == Type::I128
            {
                return Err(err(format!("{op} on i128 is not supported yet")));
            }
        }

        let eh = if present.landing_pad {
            ExceptionTable::new(func).map_err(err)?
        } else {
            ExceptionTable::default()
        };

        // A tail call's arguments must all go in registers: the caller's
        // frame, where stack arguments would go, is gone by the jump.
        for (_, inst) in func.layout_insts().filter(|_| present.return_call) {
            let (args, sig) = match func.inst_data(inst) {
                InstData::ReturnCall { func: callee, args } => (args, &func.callee(*callee).sig),
                InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
                _ => continue,
            };
            let kinds = arg_kinds(func, &sig.params, args);
            let (locs, stack) = abi::arg_locs(&kinds, sig.params.len());
            let in_regs = locs
                .iter()
                .all(|loc| matches!(loc, ArgLoc::Value(Slot::Reg(_))));
            if stack > 0 || !in_regs {
                return Err(err(format!(
                    "`{}` needs stack or struct arguments, which tail calls do not support",
                    func.display_inst(inst)
                )));
            }
            // An sret result may have to be loaded into registers on return,
            // which the callee would not do.
            if func
                .sig
                .params
                .iter()
                .any(|p| matches!(p.purpose, Purpose::Sret(_)))
            {
                return Err(err(
                    "a function returning a struct cannot make tail calls".into()
                ));
            }
        }

        // The bottom of the frame holds what calls need: their stack
        // arguments, copies of big structs passed by value, and a slot for
        // an `sret` address.
        let (mut outgoing, mut copies, mut needs_sret_save) = (0, 0u64, false);
        for (_, inst) in func.layout_insts().filter(|_| present.call) {
            let (args, params) = match func.inst_data(inst) {
                InstData::Call { func: callee, args } => (args, &func.callee(*callee).sig.params),
                InstData::CallIndirect { sig, args, .. } => (args, &func.signature(*sig).params),
                _ => continue,
            };
            let kinds = arg_kinds(func, params, args);
            let (locs, stack) = abi::arg_locs(&kinds, params.len());
            outgoing = outgoing.max(stack);
            let mut bytes: u64 = 0;
            for (loc, kind) in locs.iter().zip(&kinds) {
                match (loc, kind) {
                    (ArgLoc::Copy(_), ArgKind::ByVal(layout)) => {
                        bytes = bytes.next_multiple_of(16) + layout.size();
                    }
                    (ArgLoc::SretRegs(_), _) => needs_sret_save = true,
                    _ => {}
                }
            }
            copies = copies.max(bytes);
        }
        if outgoing > MAX_STACK_ARGS {
            return Err(err(format!(
                "a call passes {outgoing} bytes of stack arguments, more than the {MAX_STACK_ARGS} supported"
            )));
        }
        let copy_base = (outgoing as usize).next_multiple_of(16);
        let mut offset = copy_base + copies as usize;
        let sret_save = needs_sret_save.then(|| {
            offset = offset.next_multiple_of(8) + 8;
            offset - 8
        });
        let mut param_areas = Vec::new();
        for (loc, param) in param_locs.iter().zip(&func.sig.params) {
            param_areas.push(match loc {
                ArgLoc::InRegs(_) | ArgLoc::SretRegs(_) => {
                    let layout = param.layout().expect("a struct");
                    offset = offset.next_multiple_of(layout.align().max(8) as usize);
                    let area = offset;
                    offset += layout.size().next_multiple_of(8) as usize;
                    Some(area)
                }
                _ => None,
            });
        }
        let spill_base = offset.next_multiple_of(8);

        let moves_sp = present.intrinsic
            && func.layout_insts().any(|(_, inst)| {
                matches!(
                    func.inst_data(inst),
                    InstData::Intrinsic {
                        id: Intrinsic::StackAlloc | Intrinsic::StackRestore,
                        ..
                    }
                )
            });
        // Callee-saved registers inline assembly clobbers: saved by the
        // prologue, and never allocated, so no value lives in one across
        // the assembly.
        let (mut asm_saved, mut asm_float_saved) = (Vec::new(), Vec::new());
        let mut has_inline_asm = false;
        for (_, inst) in func.layout_insts().filter(|_| present.inline_asm) {
            let InstData::InlineAsm { asm, args } = func.inst_data(inst) else {
                continue;
            };
            has_inline_asm = true;
            let at = |message: String| err(format!("`{}`: {message}", func.display_inst(inst)));
            let regs = asm_regs(asm).map_err(at)?;
            asm_text(func, asm, args, &regs).map_err(at)?;
            for clobber in &asm.clobbers {
                let AsmClobber::Reg(r) = *clobber else {
                    continue;
                };
                match (r.float, r.num) {
                    (false, 18) => return Err(at("x18 is reserved by the platform".into())),
                    (false, 29) => return Err(at("x29 is the frame pointer".into())),
                    (false, BASE) if moves_sp => {
                        return Err(at(
                            "x19 is the base pointer of a function that allocates stack".into(),
                        ));
                    }
                    (false, n) if REGISTERS.callee_saved.contains(&n) => asm_saved.push(n),
                    (true, n) if REGISTERS.float_callee_saved.contains(&n) => {
                        asm_float_saved.push(n)
                    }
                    _ => {}
                }
            }
        }
        let has_tls_getter = present.symbol_addr
            && func.layout_insts().any(|(_, i)| match func.inst_data(i) {
                InstData::SymbolAddr { symbol } => symbols
                    .data_object(func.symbol_name(*symbol))
                    .is_some_and(|data| data.thread_local),
                _ => false,
            });
        // A scalar integer private leaf never runs the wide-ABI staging or
        // call helpers that reserve x0-x7. Let it compute in its argument/result
        // bank directly. Respect custom pools, including forced-spill tests.
        let argument_bank = func.sig.cc == CallConv::Fast
            && !has_tls_getter
            && module.files.is_empty()
            && func.personality.is_none()
            && incoming == 0
            && func.num_stack_slots() == 0
            && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
            && pools.caller_saved == REGISTERS.caller_saved
            && pools.callee_saved == REGISTERS.callee_saved
            && pools.float_caller_saved == REGISTERS.float_caller_saved
            && pools.float_callee_saved == REGISTERS.float_callee_saved
            && (0..func.num_values()).all(|i| {
                matches!(
                    func.value_type(Value::new(i)),
                    Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                )
            })
            && !func.layout_insts().any(|(_, i)| {
                let d = func.inst_data(i);
                is_call(func, d)
                    || matches!(
                        d,
                        InstData::Intrinsic { .. }
                            | InstData::InlineAsm { .. }
                            | InstData::ReturnCall { .. }
                            | InstData::ReturnCallIndirect { .. }
                            | InstData::LandingPad { .. }
                    )
            });
        let mut private_callers = Vec::new();
        if argument_bank {
            private_callers.extend(0..8);
        }
        private_callers.extend_from_slice(pools.caller_saved);
        let callee_saved: Vec<u8> = (pools.callee_saved.iter().copied())
            .filter(|&r| !(moves_sp && r == BASE) && !asm_saved.contains(&r))
            .collect();
        let float_callee_saved: Vec<u8> = (pools.float_callee_saved.iter().copied())
            .filter(|r| !asm_float_saved.contains(r))
            .collect();
        let pools = Pools {
            caller_saved: &private_callers,
            callee_saved: &callee_saved,
            float_callee_saved: &float_callee_saved,
            ..*pools
        };
        let mut selection = {
            backend_scope!(Selection);
            pipeline
                .selector
                .select(func, crate::select::Target::Aarch64, &|data| {
                    folded_imm(func, data)
                        .map(|(v, _)| v)
                        .into_iter()
                        .chain(folded_divisor(func, data))
                        .collect()
                })
        };
        // Raw IR use counts never change while selecting this function.
        let uses = fold::use_counts(func);
        // Each cover is anchored at an instruction kind; without one it
        // selects nothing and leaves the selection untouched.
        let absolute_values = if present.select {
            absolute::select(func, &mut selection, &uses)
        } else {
            HashMap::default()
        };
        let absolute_differences = if present.widen_low {
            absolute_diff::select(func, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_absolute_values = if present.vector_select {
            crate::packed_absolute::select(func, &mut selection)
        } else {
            HashMap::default()
        };
        let packed_minmax = if present.vector_icmp {
            packed_minmax::select(func, &mut selection)
        } else {
            HashMap::default()
        };
        let extended_adds = if present.ptradd {
            extended_add::select(func, &mut selection, &uses)
        } else {
            HashMap::default()
        };
        if present.select {
            selection.fuse_aarch64_selects(
                func,
                |data| folded_imm(func, data).map(|(v, _)| v).into_iter().collect(),
                &uses,
            );
        }
        let (packed_low_loads, replicate_loads) = if present.vector_splat {
            (
                packed_load::select(func, &mut selection),
                vector::select_replicate_loads(func, &mut selection),
            )
        } else {
            (HashMap::default(), HashMap::default())
        };
        let widening_adds = if present.widen_low {
            vector::select_widening_adds(func, &mut selection)
        } else {
            HashMap::default()
        };
        let fused_compares = if present.compare && present.brif {
            backend_scope!(CompareFusion);
            selection.fuse_aarch64_compares(
                func,
                |data| folded_imm(func, data).map(|(v, _)| v).into_iter().collect(),
                &uses,
            )
        } else {
            HashSet::default()
        };
        let calls = present.call && !leaf_summaries.is_empty();
        for (_, inst) in func.layout_insts().filter(|_| calls) {
            if let InstData::Call { func: callee, .. } = func.inst_data(inst) {
                let decl = func.callee(*callee);
                if let Some((sig, preserved)) = leaf_summaries.get(&decl.name) {
                    if &decl.sig == sig {
                        selection.preserved_call_gprs.insert(inst, *preserved);
                    }
                }
            }
        }
        let is_call = |data: &InstData| is_call(func, data);
        let returns_twice = |data: &InstData| returns_twice(func, data);
        let input = crate::regalloc::AllocationInput {
            func,
            pools: &pools,
            selection: &selection,
            is_call: &is_call,
            returns_twice: &returns_twice,
        };
        let mut alloc = pipeline.allocator.allocate(&input);
        alloc.finish(&input);
        alloc.validate(&input).map_err(err)?;
        if moves_sp {
            asm_saved.push(BASE);
        }
        for (used, extra) in [
            (&mut alloc.callee_saved_used, asm_saved),
            (&mut alloc.float_callee_saved_used, asm_float_saved),
        ] {
            used.extend(extra);
            used.sort_unstable();
            used.dedup();
        }
        let mut vector_spills = HashSet::default();
        let has_vectors = (0..func.num_values()).any(|i| {
            func.value_type(Value::new(i)).is_vector() && alloc.loc(Value::new(i)).is_some()
        });
        if has_vectors {
            for i in 0..func.num_values() {
                let v = Value::new(i);
                if func.value_type(v).is_vector()
                    && let Some(Loc::Stack(slot)) = alloc.loc(v)
                {
                    vector_spills.insert(slot);
                }
            }
        }
        let copy_temp = has_vectors.then_some(alloc.spill_slots);
        if let Some(slot) = copy_temp {
            vector_spills.insert(slot);
        }
        let offset =
            spill_base + 8 * (alloc.spill_slots as usize + if has_vectors { 2 } else { 0 });
        if offset > MAX_SLOT_OFFSET {
            return Err(err(format!(
                "{offset} bytes of spill slots is too many for this allocator"
            )));
        }
        // `sp` is 16-byte aligned and slot alignment is at most 16, so an
        // aligned offset gives an aligned address.
        let (stack_slot_offsets, offset) =
            crate::stack_frame::SlotOffsets::layout(func, offset, !module.files.is_empty());
        let frame_size = offset.next_multiple_of(16);
        if frame_size > MAX_FRAME_SIZE {
            return Err(err(format!("frame of {frame_size} bytes is too large")));
        }
        let frameless = func.sig.cc == CallConv::Fast
            && !has_tls_getter
            && module.files.is_empty()
            && func.personality.is_none()
            && frame_size == 0
            && incoming == 0
            && alloc.callee_saved_used.is_empty()
            && alloc.float_callee_saved_used.is_empty()
            && func.sig.params.iter().all(|p| p.purpose == Purpose::Normal)
            && !func.layout_insts().any(|(_, i)| {
                let d = func.inst_data(i);
                is_call(d)
                    || matches!(
                        d,
                        InstData::Intrinsic { .. }
                            | InstData::InlineAsm { .. }
                            | InstData::ReturnCall { .. }
                            | InstData::ReturnCallIndirect { .. }
                            | InstData::LandingPad { .. }
                    )
            });
        let folded_values = {
            backend_scope!(Folding);
            let folded_values = {
                backend_scope!(FoldValues);
                fold::folded_values(func, &selection, |data, out| {
                    folds(func, &stack_slot_offsets, data, out)
                })
            };
            folded_values
        };
        #[cfg(feature = "experimental-dispatch")]
        let dispatch_tails = dispatch_tail::plan(
            func,
            module,
            &alloc,
            &selection,
            pipeline.selector.experimental_aarch64_dispatch_tails()
                && pipeline.allocator.name() != "fast",
        );
        Ok(Self {
            #[cfg(feature = "experimental-dispatch")]
            dispatch_tails,
            func,
            symbols,
            alloc,
            current_inst: None,
            allocation_index: None,
            folded_values,
            fused_compares,
            has_inline_asm,
            selection,
            replicate_loads,
            packed_low_loads,
            packed_minmax,
            extended_adds,
            packed_absolute_values,
            absolute_values,
            widening_adds,
            absolute_differences,
            stack_slot_offsets,
            copy_base,
            sret_save,
            param_areas,
            spill_base,
            vector_spills,
            copy_temp,
            base: if moves_sp { "x19" } else { "sp" },
            variadic_offset: 16 + incoming.next_multiple_of(8),
            frame_size,
            frameless,
            next_block: None,
            out: Output::new(structured),
            uses_trap: false,
            eh,
            debug: false,
            srcloc: None,
            prologue_end: false,
            scope: None,
            scope_marks: Vec::new(),
        })
    }

    /// Appends the function's assembly to `out`; gives where its lexical
    /// scopes begin and end.
    fn compile(mut self, out: &mut Output) -> Vec<(String, Option<DebugScope>)> {
        profiling::scope!("instruction-lowering");
        backend_scope!(Lowering);
        let func = self.func;
        let name = &func.name;
        #[cfg(feature = "backend-profile")]
        let entry_profile =
            crate::backend_profile::scope(crate::backend_profile::Stage::LoweringEntry);
        if !func.local {
            out.directive(Directive::Global(format!("_{name}")));
        }
        if func.weak_odr {
            out.directive(Directive::WeakDefinition(format!("_{name}")));
        }
        out.align(2);
        out.label(format!("_{name}"));
        out.label(format!("L{name}_begin"));
        let start = self.out.begin_function(out);
        self.allocation_comment();

        if self.debug {
            let first = func.debug_loc.or_else(|| {
                func.layout_insts()
                    .find_map(|(_, inst)| func.inst_loc(inst))
            });
            let file = first.map_or(SourceFile::new(0), |loc| loc.file);
            let line = first.map_or(0, |loc| loc.line);
            self.set_srcloc(Some(SourceLoc {
                file,
                line,
                col: 0,
                scope: None,
            }));
            self.prologue_end = true;
        }
        self.out.directive(Directive::Cfi(Cfi::Start));
        // 155 is an indirect, pc-relative 4-byte pointer (through the GOT,
        // as the personality may be in a dylib), 16 a pc-relative one.
        if let Some(personality) = &func.personality {
            self.out
                .directive(Directive::Cfi(Cfi::Personality(format!("_{personality}"))));
            self.out
                .directive(Directive::Cfi(Cfi::Lsda(format!("L{name}_lsda"))));
        }
        if self.frameless {
            // The AArch64 CIE starts at CFA = sp. Keep LR and FP untouched;
            // offset zero also has the standard frameless compact encoding.
            self.out.directive(Directive::Cfi(Cfi::DefCfaOffset(0)));
        } else {
            self.native(
                Opcode::STP_64_ldstpair_pre,
                [
                    (FieldId::Rt, 29),
                    (FieldId::Rt2, 30),
                    (FieldId::Rn, 31),
                    (FieldId::imm7, 126),
                ],
                |out| out.write_str("stp\tx29, x30, [sp, #-16]!"),
            );
            self.native(
                Opcode::ADD_64_addsub_imm,
                [
                    (FieldId::Rd, 29),
                    (FieldId::Rn, 31),
                    (FieldId::imm12, 0),
                    (FieldId::sh, 0),
                ],
                |out| out.write_str("mov\tx29, sp"),
            );
            self.out.directive(Directive::Cfi(Cfi::DefCfa {
                register: 29,
                offset: 16,
            }));
            self.out.directive(Directive::Cfi(Cfi::Offset {
                register: 30,
                offset: -8,
            }));
            self.out.directive(Directive::Cfi(Cfi::Offset {
                register: 29,
                offset: -16,
            }));
        }
        // Push the callee-saved registers in pairs, keeping `sp` 16-byte
        // aligned; an odd one out still takes 16 bytes. Pair `k` is at
        // `x29 - 16 * (k + 1)`, which is 16 bytes further from the CFA.
        let pairs = self.alloc.callee_saved_used.clone();
        let float_pairs = self.alloc.float_callee_saved_used.clone();
        let saves =
            (pairs.chunks(2).map(|p| (p, 'x'))).chain(float_pairs.chunks(2).map(|p| (p, 'd')));
        for (k, (pair, class)) in saves.enumerate() {
            let offset = 32 + 16 * k;
            match pair {
                [a, b] => {
                    self.native(
                        if class == 'x' {
                            Opcode::STP_64_ldstpair_pre
                        } else {
                            Opcode::STP_D_ldstpair_pre
                        },
                        [
                            (FieldId::Rt, u32::from(*a)),
                            (FieldId::Rt2, u32::from(*b)),
                            (FieldId::Rn, 31),
                            (FieldId::imm7, 126),
                        ],
                        |out| {
                            native::write_pieces(
                                out,
                                &[
                                    Text("stp\t"),
                                    Class(class, *a),
                                    Text(", "),
                                    Class(class, *b),
                                    Text(", [sp, #-16]!"),
                                ],
                            )
                        },
                    );
                    self.out.directive(Directive::Cfi(Cfi::Offset {
                        register: *a + if class == 'd' { 64 } else { 0 },
                        offset: -(offset as i64),
                    }));
                    self.out.directive(Directive::Cfi(Cfi::Offset {
                        register: *b + if class == 'd' { 64 } else { 0 },
                        offset: -((offset - 8) as i64),
                    }));
                }
                [a] => {
                    self.native(
                        if class == 'x' {
                            Opcode::STR_64_ldst_immpre
                        } else {
                            Opcode::STR_D_ldst_immpre
                        },
                        [
                            (FieldId::Rt, u32::from(*a)),
                            (FieldId::Rn, 31),
                            (FieldId::imm9, 496),
                        ],
                        |out| {
                            native::write_pieces(
                                out,
                                &[Text("str\t"), Class(class, *a), Text(", [sp, #-16]!")],
                            )
                        },
                    );
                    self.out.directive(Directive::Cfi(Cfi::Offset {
                        register: *a + if class == 'd' { 64 } else { 0 },
                        offset: -(offset as i64),
                    }));
                }
                _ => unreachable!(),
            }
        }
        if self.frame_size > 0 {
            self.sub_sp(self.frame_size);
        }
        if self.base != "sp" {
            self.mach("mov", [(self.base).into(), (Reg::SP).into()]);
        }
        // Arguments arrive in x0–x7 and v0–v7. Scalar private leaves
        // may allocate the integer argument bank, so use a parallel copy
        // that also handles cycles; it may still store to spill slots.
        // Stack arguments are in the caller's frame, just above ours. An
        // `sret` address arrives in x8, which is also a scratch register,
        // so nothing may use scratch registers before it is copied: only
        // `load_stack_param` does, and it uses x16.
        if let Some(entry) = func.entry_block() {
            let params = func.block_params(entry).to_vec();
            let kinds = param_kinds(&func.sig.params);
            let (locs, _) = abi::arg_locs(&kinds, kinds.len());
            let mut moves = Vec::new();
            let mut addresses = Vec::new();
            for (i, (param, loc)) in params.into_iter().zip(locs).enumerate() {
                let Some(dst) = self
                    .alloc
                    .param_loc(entry, crate::regalloc::ValuePart::low(param))
                else {
                    continue;
                };
                let abi_param = &func.sig.params[i];
                let hi = self
                    .alloc
                    .param_loc(entry, crate::regalloc::ValuePart::high(param));
                match loc {
                    ArgLoc::Value(Slot::Reg(src)) | ArgLoc::Copy(Slot::Reg(src)) => {
                        moves.push((dst, src));
                        if let Some(hi) = hi {
                            moves.push((hi, Loc::Reg(loc_reg(src) + 1)));
                        }
                    }
                    ArgLoc::Value(Slot::Stack(offset)) | ArgLoc::Copy(Slot::Stack(offset)) => {
                        if let Some(hi) = hi {
                            self.load_stack_param(dst, Type::I64, 16 + offset);
                            self.load_stack_param(hi, Type::I64, 24 + offset);
                        } else {
                            self.load_stack_param(dst, abi_param.ty, 16 + offset)
                        }
                    }
                    ArgLoc::SretMemory => moves.push((dst, Loc::Reg(8))),
                    // The struct's address is where its bytes are now.
                    ArgLoc::InRegs(regs) => {
                        let area = self.param_areas[i].expect("planned");
                        let layout = abi_param.layout().expect("a struct");
                        self.store_struct_regs(&regs, layout, area);
                        addresses.push((dst, self.base, area));
                    }
                    ArgLoc::OnStack(offset) => addresses.push((dst, "x29", 16 + offset as usize)),
                    ArgLoc::SretRegs(_) => {
                        addresses.push((dst, self.base, self.param_areas[i].expect("planned")))
                    }
                }
            }
            self.parallel_copy(&moves);
            for (dst, base, offset) in addresses {
                self.address(dst, base, offset);
            }
        }
        #[cfg(feature = "backend-profile")]
        drop(entry_profile);
        {
            // Includes instruction comments, debug locations and return epilogues.
            // Time the whole traversal rather than taking a clock sample per instruction.
            backend_scope!(LoweringBody);
            // Only reachable blocks, in the order the allocator numbered them.
            let order = self.alloc.order.clone();
            for (i, &block) in order.iter().enumerate() {
                self.next_block = order.get(i + 1).copied();
                let label = self.label(block);
                self.out.label(label);
                for &inst in func.block_insts(block) {
                    if !self.out.structured() {
                        self.out.push_str("\t; ");
                        func.write_inst_to(&mut self.out, inst).unwrap();
                        self.out.push_str("\n");
                    }
                    if self.debug {
                        self.set_srcloc(func.inst_loc(inst));
                    }
                    self.allocated_inst(inst);
                }
            }
        }
        {
            backend_scope!(LoweringFinish);
            if self.uses_trap {
                self.out.label(format!("L{name}_trap"));
                self.native(Opcode::BRK_EX_exception, [(FieldId::imm16, 1)], |out| {
                    out.write_str("brk\t#1")
                });
            }
            self.out.label(format!("L{name}_end"));
            {
                backend_scope!(Relaxation);
                self.out.relax(name, start);
            }
            self.out.directive(Directive::Cfi(Cfi::End));
            if func.personality.is_some() {
                self.eh.emit(&mut self.out, name);
            }
            self.out.end_function(out);
        }
        self.scope_marks
    }

    /// Emits a `.loc` for the code that follows, unless it would repeat
    /// the last one. `None` keeps the last one.
    /// A change of lexical scope is marked with a label.
    fn set_srcloc(&mut self, loc: Option<SourceLoc>) {
        let Some(loc) = loc else { return };
        if loc.scope != self.scope {
            let label = format!("L{}_ds{}", self.func.name, self.scope_marks.len());
            self.out.label(label.clone());
            self.scope_marks.push((label, loc.scope));
            self.scope = loc.scope;
        }
        if self
            .srcloc
            .is_some_and(|l| (l.file, l.line, l.col) == (loc.file, loc.line, loc.col))
            && !self.prologue_end
        {
            return;
        }
        let SourceLoc {
            file, line, col, ..
        } = loc;
        self.out.directive(Directive::Loc {
            file: (file.index() + 1) as u32,
            line,
            column: col,
            prologue_end: self.prologue_end,
        });
        self.srcloc = Some(loc);
        self.prologue_end = false;
    }

    /// A comment listing where each value lives, to make the output
    /// readable.
    fn allocation_comment(&mut self) {
        if self.out.structured() {
            return;
        }
        backend_scope!(AllocationAnnotations);
        // Write straight into the output, set aside while the locations are
        // read, rather than building the line in its own string.
        let mut out = std::mem::replace(&mut self.out, Output::Text(String::new()));
        out.push_str("\t; regalloc:");
        for value in (0..self.func.num_values()).map(Value::new) {
            if let Some(loc) = self.alloc.loc(value) {
                crate::text::write_decimal(&mut out, " v", value.index() as i128).unwrap();
                out.write_char('=').unwrap();
                self.write_loc_name(&mut out, loc);
            }
            if let Some(hi) = self.alloc.hi_loc(value) {
                out.write_char(':').unwrap();
                self.write_loc_name(&mut out, hi);
            }
        }
        out.push_str("\n");
        self.out = out;
    }

    fn selected(&mut self, result: Option<Value>, matched: crate::select::Match) {
        use crate::select::Machine;
        let ty = self.ty(matched.args[0]);
        let a = self.use_reg(matched.args[0], S0);
        if let Machine::Ubfx { lsb, width } = matched.machine {
            let d = self.def_reg(result);
            self.native(
                if ty == Type::I64 {
                    Opcode::UBFM_64M_bitfield
                } else {
                    Opcode::UBFM_32M_bitfield
                },
                [
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, a.into()),
                    (FieldId::immr, lsb.into()),
                    (FieldId::imms, u32::from(lsb) + u32::from(width) - 1),
                ],
                |out| {
                    write!(
                        out,
                        "ubfx\t{}, {}, #{lsb}, #{width}",
                        reg(d, ty),
                        reg(a, ty)
                    )
                },
            );
            self.def_done(result, d);
            return;
        }
        let b = self.use_reg(matched.args[1], S1);
        let d = self.def_reg(result);
        let (ra, rb, rd) = (reg(a, ty), reg(b, ty), reg(d, ty));
        match matched.machine {
            Machine::Madd | Machine::Msub => {
                let c = self.use_reg(matched.args[2], S2);
                let rc = reg(c, ty);
                let op = if matches!(matched.machine, Machine::Madd) {
                    "madd"
                } else {
                    "msub"
                };
                self.mach(op, [(rd).into(), (rb).into(), (rc).into(), (ra).into()]);
            }
            machine => {
                let op = match machine {
                    Machine::ShiftAdd => "add",
                    Machine::ShiftSub => "sub",
                    Machine::ShiftAnd => "and",
                    Machine::ShiftOr => "orr",
                    Machine::ShiftXor => "eor",
                    _ => unreachable!(),
                };
                self.mach(
                    op,
                    [
                        (rd).into(),
                        (ra).into(),
                        (rb).into(),
                        Arg::shift("lsl", matched.shift),
                    ],
                );
            }
        }
        self.def_done(result, d);
    }

    #[inline]
    fn allocated_inst(&mut self, inst: Inst) {
        self.current_inst = Some(inst);
        self.allocation_index = self.alloc.edits.instruction_index(inst);
        if let Some(i) = self.allocation_index {
            self.allocation_moves(i, true);
        }
        self.inst(inst);
        if let Some(i) = self.allocation_index {
            self.allocation_moves(i, false);
        }
    }

    // Keep vector copying and cycle resolution out of the default loop's
    // inline path. Existing tiers have no instruction transfers.
    fn allocation_moves(&mut self, index: usize, before: bool) {
        let edit = self.alloc.edits.instruction(index);
        let moves = if before { &edit.before } else { &edit.after };
        if !moves.is_empty() {
            let moves = moves.clone();
            self.parallel_copy(&moves);
        }
    }

    fn defined(&self, value: Value) -> Option<Loc> {
        self.alloc.operand_loc(
            self.allocation_index,
            crate::regalloc::ValuePart::low(value),
            crate::regalloc::OperandKind::Def,
        )
    }
    fn defined_hi(&self, value: Value) -> Option<Loc> {
        self.alloc.operand_loc(
            self.allocation_index,
            crate::regalloc::ValuePart::high(value),
            crate::regalloc::OperandKind::Def,
        )
    }
    fn edge_moves(&self, successor: usize) -> &[crate::regalloc::Transfer] {
        self.alloc
            .edits
            .edge(self.current_inst.expect("edge in a terminator"), successor)
    }

    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
        if self.selection.covered(inst) {
            return;
        }
        if let Some(&p) = self.extended_adds.get(&inst) {
            let a = self.use_reg(p.base, S0);
            let b = self.use_reg(p.index, S1);
            let d = self.def_reg(result);
            self.mach(
                "add",
                [
                    Reg::x(d).into(),
                    Reg::x(a).into(),
                    Reg::w(b).into(),
                    Arg::shift(if p.signed { "sxtw" } else { "uxtw" }, p.shift),
                ],
            );
            self.def_done(result, d);
            return;
        }
        if let Some(&value) = self.absolute_values.get(&inst) {
            let ty = self.ty(value);
            let source = self.use_reg(value, S0);
            let d = self.def_reg(result);
            self.mach("cmp", [reg(source, ty).into(), Arg::imm(0)]);
            self.native(
                if ty == Type::I64 {
                    Opcode::CSNEG_64_condsel
                } else {
                    Opcode::CSNEG_32_condsel
                },
                [
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, source.into()),
                    (FieldId::Rm, source.into()),
                    (FieldId::cond, 10),
                ],
                |out| {
                    write!(
                        out,
                        "csneg\t{}, {}, {}, ge",
                        reg(d, ty),
                        reg(source, ty),
                        reg(source, ty)
                    )
                },
            );
            self.def_done(result, d);
            return;
        }
        if let Some(&difference) = self.absolute_differences.get(&inst) {
            self.vector_absolute_difference(difference, result);
            return;
        }
        if let Some(&value) = self.packed_absolute_values.get(&inst) {
            self.vector_absolute(value, result);
            return;
        }
        if let Some(&minmax) = self.packed_minmax.get(&inst) {
            self.vector_minmax(minmax, result);
            return;
        }
        if let Some(&addr) = self.packed_low_loads.get(&inst) {
            let at = self.mem(addr, S0, Type::F64);
            let d = self.def_freg(result);
            self.mach("ldr", [Reg::d(d).into(), at.into()]);
            self.def_done(result, d);
            return;
        }
        if let Some(&addr) = self.replicate_loads.get(&inst) {
            self.vector_replicate_load(addr, result);
            return;
        }
        if let Some(&add) = self.widening_adds.get(&inst) {
            self.vector_widening_add(add, result);
            return;
        }
        if let Some(matched) = self.selection.matches.get(&inst).cloned() {
            self.selected(result, matched);
            return;
        }
        if self.lower_i128(inst, func.inst_data(inst), result) {
            return;
        }
        match func.inst_data(inst).clone() {
            InstData::VectorIcmp { cond, args } => self.vector_compare(cond, args, result),
            InstData::VectorSelect { args } => self.vector_select(args, result),
            InstData::VectorBinary { op, args } => self.vector_binary(op, args, result),
            InstData::VectorUnary { op, arg } => self.vector_unary(op, arg, result),
            InstData::VectorSplat { ty, arg } => self.vector_splat(ty, arg, result),
            InstData::VectorShuffle { args, lanes } => {
                self.vector_shuffle_window(args, &lanes, result)
            }
            InstData::VectorExtract { arg, lane } => self.vector_extract(arg, lane, result),
            InstData::VectorInsert { .. } | InstData::VectorFcmp { .. } => {
                unreachable!("legalized vector operation")
            }
            InstData::StackAddr { .. } | InstData::Iconst { .. }
                if result.is_some_and(|r| {
                    self.folded_values.contains(&r)
                        || matches!(self.defined(r), Some(Loc::Remat(_)))
                }) => {}
            InstData::Convert {
                op: ConvertOp::IntToPtr,
                ty: Type::Ptr,
                ..
            } if result.is_some_and(|r| {
                matches!(self.defined(r), Some(Loc::Remat(v)) if v == r)
                    && matches!(
                        crate::regalloc::remat_recipe(self.func, r),
                        Some(crate::regalloc::RematRecipe::NullPtr)
                    )
            }) => {}
            InstData::Icmp { .. } | InstData::Fcmp { .. }
                if result.is_some_and(|r| self.fused_compares.contains(&r)) => {}
            InstData::Iconst { imm, .. } => {
                let d = self.def_reg(result);
                self.materialize(d, imm as u64, true);
                self.def_done(result, d);
            }
            InstData::Fconst { ty, bits } => {
                let d = self.def_freg(result);
                let fd = freg(d, ty);
                if bits == 0 {
                    let zero = if ty == Type::F32 { "wzr" } else { "xzr" };
                    self.mach("fmov", [(fd).into(), (zero).into()]);
                } else {
                    let is64 = ty == Type::F64;
                    self.materialize(S0, bits, is64);
                    let src = if is64 { Reg::x(S0) } else { Reg::w(S0) };
                    self.mach("fmov", [(fd).into(), (src).into()]);
                }
                self.def_done(result, d);
            }
            InstData::Call { func: callee, args } => {
                let decl = func.callee(callee);
                let sret = self.move_args(&args, &decl.sig.params);
                self.out.branch(true, format!("_{}", decl.name));
                self.call_result(result, sret);
            }
            // Only the call itself can throw, so the call-site entry
            // covers just the `bl`.
            InstData::Invoke {
                func: callee,
                args,
                dests,
            } => {
                let decl = func.callee(callee);
                let sret = self.move_args(&args, &decl.sig.params);
                let site = format!("L{}_{inst}", func.name);
                self.out.label(format!("{site}_begin"));
                self.out.branch(true, format!("_{}", decl.name));
                self.out.label(format!("{site}_end"));
                let pad = self.label(dests[1].block);
                self.eh.sites.push((site, pad, dests[1].block));
                self.call_result(result, sret);
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
            InstData::ReturnCall { func: callee, args } => {
                let decl = func.callee(callee);
                self.move_args(&args, &decl.sig.params);
                self.epilogue();
                self.out.branch(false, format!("_{}", decl.name));
            }
            InstData::ReturnCallIndirect { sig, callee, args } => {
                self.move_args(&args, &func.signature(sig).params);
                // The epilogue restores callee-saved registers, which may
                // hold the target, so it moves to x16 first.
                let target = self.use_reg(callee, S0);
                self.move_gp(S0, target, true);
                self.epilogue();
                self.mach("br", [(Reg::x(S0)).into()]);
            }
            InstData::CallIndirect { sig, callee, args } => {
                let sret = self.move_args(&args, &func.signature(sig).params);
                // The callee is in an allocated register, which the argument
                // moves leave alone (they write x0–x8, x16 and x17), or in a
                // spill slot, reloaded after the moves so x16 is free.
                let target = self.use_reg(callee, S0);
                self.mach("blr", [(Reg::x(target)).into()]);
                self.call_result(result, sret);
            }
            InstData::SymbolAddr { symbol } => {
                let d = self.def_reg(result);
                let name = func.symbol_name(symbol);
                let thread_local = self
                    .symbols
                    .data_object(name)
                    .is_some_and(|d| d.thread_local);
                if thread_local {
                    // The symbol is the variable's descriptor; calling its
                    // getter with the descriptor in x0 returns the address
                    // in x0. The getter preserves every register but x0,
                    // x16, x17 and x30, none of which hold values.
                    self.out.symbolic(Symbolic::PageAddress {
                        register: 0,
                        kind: PageKind::ThreadLocal,
                        target: format!("_{name}"),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: 0,
                        base: 0,
                        load: true,
                        kind: PageKind::ThreadLocal,
                        target: format!("_{name}"),
                    });
                    self.mach(
                        "ldr",
                        [(Reg::x(S0)).into(), Address::base(Reg::x(0)).into()],
                    );
                    self.mach("blr", [(Reg::x(S0)).into()]);
                    self.mach("mov", [(Reg::x(d)).into(), (Reg::x(0)).into()]);
                } else if self.symbols.defines(name) {
                    self.out.symbolic(Symbolic::PageAddress {
                        register: d,
                        kind: PageKind::Direct,
                        target: format!("_{name}"),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: d,
                        base: d,
                        load: false,
                        kind: PageKind::Direct,
                        target: format!("_{name}"),
                    });
                } else {
                    self.out.symbolic(Symbolic::PageAddress {
                        register: d,
                        kind: PageKind::Got,
                        target: format!("_{name}"),
                    });
                    self.out.symbolic(Symbolic::PageOffset {
                        register: d,
                        base: d,
                        load: true,
                        kind: PageKind::Got,
                        target: format!("_{name}"),
                    });
                }
                self.def_done(result, d);
            }
            InstData::StackAddr { slot } => {
                let d = self.def_reg(result);
                let offset = self.stack_slot_offsets[slot.index()];
                // `add` takes a 12-bit immediate, optionally shifted by 12.
                self.mach(
                    "add",
                    [
                        (Reg::x(d)).into(),
                        (self.base).into(),
                        Arg::imm(offset & 0xfff),
                    ],
                );
                if offset >> 12 != 0 {
                    self.mach(
                        "add",
                        [
                            (Reg::x(d)).into(),
                            (Reg::x(d)).into(),
                            Arg::imm(offset >> 12),
                            Arg::shift("lsl", 12),
                        ],
                    );
                }
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } if op.is_float() => {
                let ty = self.ty(arg);
                let d = self.def_freg(result);
                let a = self.use_freg(arg, FS0);
                let mnemonic = match op {
                    UnaryOp::Fneg => "fneg",
                    UnaryOp::Fabs => "fabs",
                    UnaryOp::Fsqrt => "fsqrt",
                    UnaryOp::Fceil => "frintp",
                    UnaryOp::Ffloor => "frintm",
                    UnaryOp::Ftrunc => "frintz",
                    UnaryOp::Fnearest => "frintn",
                    _ => unreachable!("integer operation"),
                };
                self.mach(mnemonic, [(freg(d, ty)).into(), (freg(a, ty)).into()]);
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } => {
                let d = self.def_reg(result);
                self.unary(op, arg, d);
                self.def_done(result, d);
            }
            InstData::Binary {
                op: BinaryOp::Fcopysign,
                args,
            } => {
                // The sign bit of `args[1]` with the rest of `args[0]`,
                // spliced in general registers.
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let (bits, ra, rb) = if ty == Type::F32 {
                    (31, Reg::w(S0), Reg::w(S1))
                } else {
                    (63, Reg::x(S0), Reg::x(S1))
                };
                self.mach("fmov", [(ra).into(), (freg(a, ty)).into()]);
                self.mach("fmov", [(rb).into(), (freg(b, ty)).into()]);
                self.mach(
                    "bfxil",
                    [(rb).into(), (ra).into(), Arg::imm(0), Arg::imm(bits)],
                );
                self.mach("fmov", [(freg(d, ty)).into(), (rb).into()]);
                self.def_done(result, d);
            }
            InstData::Fma { args } => {
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let c = self.use_freg(args[2], FS2);
                let [fa, fb, fc, fd] = [a, b, c, d].map(|r| freg(r, ty));
                self.mach(
                    "fmadd",
                    [(fd).into(), (fa).into(), (fb).into(), (fc).into()],
                );
                self.def_done(result, d);
            }
            InstData::Overflow { op, args } => self.overflow(inst, op, args),
            InstData::Binary { op, args } if op.is_float() => {
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let mnemonic = match op {
                    BinaryOp::Fadd => "fadd",
                    BinaryOp::Fsub => "fsub",
                    BinaryOp::Fmul => "fmul",
                    BinaryOp::Fdiv => "fdiv",
                    BinaryOp::Fmin => "fmin",
                    BinaryOp::Fmax => "fmax",
                    _ => unreachable!("integer operation"),
                };
                let (fa, fb, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.mach(mnemonic, [(fd).into(), (fa).into(), (fb).into()]);
                self.def_done(result, d);
            }
            InstData::Binary { op, args } => {
                let d = self.def_reg(result);
                self.binary(op, args, d);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_vector() => {
                let d = self.def_freg(result);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let skip = format!("L{}_{inst}_vselect", func.name);
                self.move_vector(FS0, b);
                self.conditional("eq", skip.clone());
                self.move_vector(FS0, a);
                self.out.label(skip);
                self.move_vector(d, FS0);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_float() => {
                let ty = self.ty(args[1]);
                let d = self.def_freg(result);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let (a, b, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.mach(
                    "fcsel",
                    [(fd).into(), (a).into(), (b).into(), Arg::from("ne")],
                );
                self.def_done(result, d);
            }
            InstData::Select { args } => {
                let d = self.def_reg(result);
                let ty = self.ty(args[1]);
                let cc = if let Some(&compare) = self.selection.select_compares.get(&inst) {
                    self.compare(compare)
                } else {
                    self.test_value(args[0]);
                    "ne"
                };
                // CMP uses S0/S1 before either arm is staged. use_reg only
                // loads, moves wide constants, or computes a stack address;
                // all preserve NZCV and the other arm's reserved scratch.
                let a = self.use_reg(args[1], S1);
                let b = self.use_reg(args[2], S2);
                let (a, b, rd) = (reg(a, ty), reg(b, ty), reg(d, ty));
                self.mach("csel", [(rd).into(), (a).into(), (b).into(), Arg::from(cc)]);
                self.def_done(result, d);
            }
            InstData::Convert {
                op: op @ (ConvertOp::SwidenLow | ConvertOp::UwidenLow),
                ty,
                arg,
            } => {
                self.vector_widen(op, ty, arg, result);
            }
            InstData::Convert {
                op: ConvertOp::Bitcast,
                ty,
                arg,
            } if ty.is_vector() && self.ty(arg).is_vector() => {
                self.vector_bitcast(arg, result);
            }
            InstData::Convert { op, ty, arg } if ty.is_float() || self.ty(arg).is_float() => {
                self.float_convert(op, ty, arg, result);
            }
            InstData::Convert { op, ty, arg } => {
                let d = self.def_reg(result);
                let a = self.use_reg(arg, S0);
                let from = self.ty(arg);
                match op {
                    ConvertOp::Sext => match from {
                        Type::I8 => self.mach("sxtb", [(reg(d, ty)).into(), (Reg::w(a)).into()]),
                        Type::I16 => self.mach("sxth", [(reg(d, ty)).into(), (Reg::w(a)).into()]),
                        _ => self.mach("sxtw", [(Reg::x(d)).into(), (Reg::w(a)).into()]),
                    },
                    // Writing a w register clears the upper 32 bits.
                    ConvertOp::Uext if self.zero_extended_narrow(arg) => {
                        if d != a {
                            self.move_gp(d, a, false);
                        }
                    }
                    ConvertOp::Uext => match from {
                        Type::I8 => self.mach("uxtb", [(Reg::w(d)).into(), (Reg::w(a)).into()]),
                        Type::I16 => self.mach("uxth", [(Reg::w(d)).into(), (Reg::w(a)).into()]),
                        _ => self.move_gp(d, a, false),
                    },
                    // The low bits are already right; upper bits are
                    // unspecified. A pointer is a 64-bit address already.
                    // Either way the bits just move.
                    ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                        if a != d {
                            self.move_gp(d, a, true);
                        }
                    }
                    _ => unreachable!("float conversions are lowered by float_convert"),
                }
                self.def_done(result, d);
            }
            InstData::Icmp { .. } | InstData::Fcmp { .. } => {
                let d = self.def_reg(result);
                let cc = self.compare(inst);
                self.mach("cset", [(Reg::w(d)).into(), (cc).into()]);
                self.def_done(result, d);
            }
            // Load the pointer, store it advanced by a slot, then read the
            // argument: the result may share the list's register.
            InstData::VaArg { ty, list } => {
                let a = self.use_reg(list, S2);
                self.mach(
                    "ldr",
                    [(Reg::x(S0)).into(), Address::base(Reg::x(a)).into()],
                );
                self.mach(
                    "add",
                    [(Reg::x(S1)).into(), (Reg::x(S0)).into(), Arg::imm(8)],
                );
                self.mach(
                    "str",
                    [(Reg::x(S1)).into(), Address::base(Reg::x(a)).into()],
                );
                if ty.is_float() {
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
            InstData::AtomicCas { ordering, args } => {
                // `cas` compares with its first register and overwrites it
                // with the old value, so the expected value goes in a
                // scratch register.
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S0);
                let expected = self.use_reg(args[1], S1);
                self.move_gp(S2, expected, true);
                let new = self.use_reg(args[2], S1);
                let order = ordering_suffix(ordering);
                let (rs, rt) = (reg(S2, ty), reg(new, ty));
                self.atomic(
                    "cas",
                    order,
                    ty,
                    [rs.into(), rt.into(), Address::base(Reg::x(a)).into()],
                );
                if let Some(loc) = result.and_then(|r| self.defined(r)) {
                    self.emit_move(loc, Loc::Reg(S2));
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
            InstData::Jump { dest } => {
                self.block_args(&dest, 0);
                #[cfg(feature = "experimental-dispatch")]
                if !self.emit_dispatch_tail(inst) {
                    self.branch_unless_next(dest.block);
                }
                #[cfg(not(feature = "experimental-dispatch"))]
                self.branch_unless_next(dest.block);
            }
            InstData::Brif {
                cond,
                dests: [then_dest, else_dest],
            } => {
                // Take the same false edge, with the same landing pads and
                // edge copies, whether testing a register or a comparison.
                let test = match func.value_def(func.resolve(cond)) {
                    ValueDef::Result(compare, _)
                        if self.fused_compares.contains(&func.resolve(cond)) =>
                    {
                        BrifTest::Flags(invert_cond(self.compare(compare)))
                    }
                    _ if !self.has_inline_asm && matches!(self.ty(cond), Type::I32 | Type::I64) => {
                        let c = self.use_reg(cond, S0);
                        BrifTest::Zero(reg(c, self.ty(cond)))
                    }
                    _ => {
                        self.test_value(cond);
                        BrifTest::Flags("eq")
                    }
                };
                // Copy block arguments only on their own edge. A fallthrough
                // edge can copy inline; otherwise it needs a landing pad.
                if self.next_block == Some(else_dest.block) && self.block_args_noop(&then_dest, 0) {
                    match &test {
                        BrifTest::Flags(condition) => {
                            self.conditional(invert_cond(condition), self.label(then_dest.block));
                        }
                        BrifTest::Zero(register) => {
                            self.compare_branch(*register, true, self.label(then_dest.block));
                        }
                    }
                    self.block_args(&else_dest, 1);
                } else if self.block_args_noop(&else_dest, 1) {
                    self.brif_false(&test, self.label(else_dest.block));
                    self.block_args(&then_dest, 0);
                    self.branch_unless_next(then_dest.block);
                } else {
                    let pad = format!("L{}_{inst}_else", func.name);
                    self.brif_false(&test, pad.clone());
                    self.block_args(&then_dest, 0);
                    self.out.branch(false, self.label(then_dest.block));
                    self.out.label(pad.clone());
                    self.block_args(&else_dest, 1);
                    self.branch_unless_next(else_dest.block);
                }
            }
            InstData::Switch { arg, dests, cases } => self.switch(inst, arg, &dests, &cases, None),
            InstData::Intrinsic { id, args } => match id {
                // The libc functions take the same arguments in the same
                // order. memset's `int c` only uses its low byte, so the
                // unspecified upper bits of the i8 are harmless.
                Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp => {
                    let params: Vec<AbiParam> = id
                        .info()
                        .params
                        .iter()
                        .map(|&ty| AbiParam::new(ty))
                        .collect();
                    self.move_args(&args, &params);
                    self.out.branch(true, format!("_{id}"));
                    if id == Intrinsic::Memcmp {
                        self.call_result(result, None);
                    }
                }
                // What clang emits for __builtin_debugtrap; a debugger can
                // step past it.
                Intrinsic::Debugtrap => self.mach("brk", [Arg::imm(0xf000)]),
                // Apple's va_list is a pointer to the next argument slot.
                Intrinsic::VaStart => {
                    let list = self.use_reg(args[0], S1);
                    self.mach(
                        "add",
                        [
                            (Reg::x(S0)).into(),
                            (Reg::x(29)).into(),
                            Arg::imm(self.variadic_offset),
                        ],
                    );
                    self.mach(
                        "str",
                        [(Reg::x(S0)).into(), Address::base(Reg::x(list)).into()],
                    );
                }
                // Round up to keep `sp` 16-byte aligned; the memory starts
                // above the outgoing arguments, which stay at `sp`.
                Intrinsic::StackAlloc => {
                    let size = self.use_reg(args[0], S0);
                    self.mach(
                        "add",
                        [(Reg::x(S0)).into(), (Reg::x(size)).into(), Arg::imm(15)],
                    );
                    self.mach(
                        "and",
                        [(Reg::x(S0)).into(), (Reg::x(S0)).into(), Arg::imm(-16)],
                    );
                    self.mach(
                        "sub",
                        [(Reg::SP).into(), (Reg::SP).into(), (Reg::x(S0)).into()],
                    );
                    let d = self.def_reg(result);
                    self.mach(
                        "add",
                        [
                            (Reg::x(d)).into(),
                            (Reg::SP).into(),
                            Arg::imm(self.copy_base),
                        ],
                    );
                    self.def_done(result, d);
                }
                Intrinsic::StackSave => {
                    let d = self.def_reg(result);
                    self.mach("mov", [(Reg::x(d)).into(), (Reg::SP).into()]);
                    self.def_done(result, d);
                }
                Intrinsic::StackRestore => {
                    let mark = self.use_reg(args[0], S0);
                    self.mach("mov", [(Reg::SP).into(), (Reg::x(mark)).into()]);
                }
                // The frame record is where the prologue points x29.
                Intrinsic::FrameAddress => {
                    let d = self.def_reg(result);
                    self.mach("mov", [(Reg::x(d)).into(), (Reg::x(29)).into()]);
                    self.def_done(result, d);
                }
            },
            InstData::InlineAsm { asm, args } => {
                let regs = asm_regs(&asm).expect("checked when the compiler was made");
                let n_out = asm.outputs.len();
                // Inputs into their registers. No value is allocated to
                // those, so no move overwrites another's source and they
                // can go in any order (x16 included, which the parallel
                // copy would need for itself).
                for (i, &arg) in args.iter().enumerate() {
                    if let Some(r) = regs[n_out + i] {
                        self.emit_move(asm_loc(r), self.loc(arg));
                    }
                }
                let text = asm_text(func, &asm, &args, &regs).expect("checked");
                self.out.inline_assembly(&text);
                for (k, &output) in func.inst_results(inst).iter().enumerate() {
                    if let (Some(dst), Some(r)) = (self.defined(output), regs[k]) {
                        self.emit_move(dst, asm_loc(r));
                    }
                }
            }
            InstData::Trap => self.mach("brk", [Arg::imm(1)]),
            InstData::Return { args } => {
                // A struct result in registers is loaded from where the
                // function wrote it.
                if let Some(Purpose::Sret(layout)) = func.sig.params.first().map(|p| &p.purpose)
                    && let Some(area) = self.param_areas[0]
                {
                    self.load_struct_regs(layout, area);
                }
                if let Some(&value) = args.first() {
                    let src = self.loc(value);
                    self.emit_move(return_loc(self.ty(value)), src);
                    if self.ty(value) == Type::I128 {
                        let hi = self.hi(value);
                        self.emit_move(Loc::Reg(1), hi);
                    }
                    let ret = &func.sig.returns[0];
                    self.extend_in_place(0, ret.ty, ret.ext);
                }
                self.epilogue();
                self.native(Opcode::RET_64R_branch_reg, [(FieldId::Rn, 30)], |out| {
                    out.write_str("ret")
                });
            }
        }
    }

    /// Moves call arguments into x0–x8, v0–v7 and the bottom of the frame,
    /// for parameters `params` (any further arguments are variadic). Values
    /// live across the call are in callee-saved registers or spill slots
    /// (the allocator sees to that), so nothing needs saving here.
    ///
    /// Stack arguments and struct copies come first, then the register
    /// moves, then the registers filled from memory: struct bytes, struct
    /// addresses and x8. None of them overwrites another's source, since
    /// arguments are never allocated to x0–x8, v0–v7 or the bottom of the
    /// frame.
    ///
    /// Narrow arguments marked `sext`/`uext` are extended to 32 bits in
    /// their register, as Apple's ABI has the caller do. On the stack they
    /// take their own size, so there is nothing to extend.
    ///
    /// Returns the registers and layout of an `sret` result that comes back
    /// in registers, for [`Self::call_result`] to store.
    fn move_args<'s>(
        &mut self,
        args: &[Value],
        params: &'s [AbiParam],
    ) -> Option<RegisterSret<'s>> {
        let kinds = arg_kinds(self.func, params, args);
        let (locs, _) = abi::arg_locs(&kinds, params.len());
        let mut moves = Vec::new();
        let mut extend = Vec::new();
        // Registers filled after the moves.
        enum Late<'l> {
            Address(Loc, usize),
            Sret(Value),
            Struct(Vec<Loc>, &'l Layout, Value),
        }
        let mut late = Vec::new();
        let mut copied: u64 = 0;
        let mut sret = None;
        for (i, ((&arg, loc), kind)) in args.iter().zip(locs).zip(&kinds).enumerate() {
            let layout = match kind {
                ArgKind::ByVal(layout) | ArgKind::Sret(layout) => Some(*layout),
                ArgKind::Value(_) => None,
            };
            match loc {
                ArgLoc::Value(Slot::Reg(dst)) => {
                    moves.push((dst, self.loc(arg)));
                    if self.ty(arg) == Type::I128 {
                        moves.push((Loc::Reg(loc_reg(dst) + 1), self.hi(arg)));
                    }
                    if let (Some(param), Loc::Reg(r)) = (params.get(i), dst) {
                        extend.push((r, param.ty, param.ext));
                    }
                }
                ArgLoc::Value(Slot::Stack(offset)) => {
                    self.store_stack_arg(arg, self.ty(arg), offset)
                }
                ArgLoc::OnStack(offset) => {
                    let src = self.use_reg(arg, S0);
                    let size = layout.expect("a struct").size();
                    self.copy_bytes("sp", offset as usize, src, size);
                }
                ArgLoc::Copy(slot) => {
                    let size = layout.expect("a struct").size();
                    copied = copied.next_multiple_of(16);
                    let at = self.copy_base + copied as usize;
                    copied += size;
                    let src = self.use_reg(arg, S0);
                    self.copy_bytes(self.base, at, src, size);
                    match slot {
                        Slot::Reg(dst) => late.push(Late::Address(dst, at)),
                        Slot::Stack(offset) => {
                            self.address(Loc::Reg(S0), self.base, at);
                            self.mach(
                                "str",
                                [(Reg::x(S0)).into(), Address::offset(Reg::SP, offset).into()],
                            );
                        }
                    }
                }
                ArgLoc::InRegs(regs) => {
                    late.push(Late::Struct(regs, layout.expect("a struct"), arg))
                }
                ArgLoc::SretMemory => late.push(Late::Sret(arg)),
                ArgLoc::SretRegs(regs) => {
                    let stack_offset = self.fixed_sret_stack_offset(arg);
                    if stack_offset.is_none() {
                        let p = self.use_reg(arg, S0);
                        let save = self.sret_save.expect("planned");
                        self.mach(
                            "str",
                            [(Reg::x(p)).into(), Address::offset(self.base, save).into()],
                        );
                    }
                    sret = Some(RegisterSret {
                        regs,
                        layout: layout.expect("a struct"),
                        stack_offset,
                    });
                }
            }
        }
        self.parallel_copy(&moves);
        for (r, ty, ext) in extend {
            self.extend_in_place(r, ty, ext);
        }
        for step in late {
            match step {
                Late::Address(dst, at) => self.address(dst, self.base, at),
                Late::Sret(arg) => {
                    let p = self.use_reg(arg, S0);
                    self.mach("mov", [(Reg::x(8)).into(), (Reg::x(p)).into()]);
                }
                Late::Struct(regs, layout, arg) => {
                    let p = self.use_reg(arg, S0);
                    self.load_struct_exact(&regs, layout, p);
                }
            }
        }
        sret
    }

    /// Extends the narrow value in `w<r>` to 32 bits, if `ext` asks.
    fn extend_in_place(&mut self, r: u8, ty: Type, ext: ArgExt) {
        let signed = match ext {
            ArgExt::None => return,
            ArgExt::Sext => true,
            ArgExt::Uext => false,
        };
        let extended = self.extend(r, r, ty, signed);
        debug_assert_eq!(extended, r);
    }

    /// Stores `arg` at `[sp, #offset]` with its own width.
    fn store_stack_arg(&mut self, arg: Value, ty: Type, offset: u32) {
        if ty == Type::I128 {
            let lo = self.use_reg(arg, S1);
            self.mach(
                "str",
                [(Reg::x(lo)).into(), Address::offset(Reg::SP, offset).into()],
            );
            let hi = self.use_reg_hi(arg, S1);
            self.mach(
                "str",
                [
                    (Reg::x(hi)).into(),
                    Address::offset(Reg::SP, offset + 8).into(),
                ],
            );
        } else if ty.is_float() || ty.is_vector() {
            let r = self.use_freg(arg, FS1);
            self.mach(
                "str",
                [
                    (freg(r, ty)).into(),
                    Address::offset(Reg::SP, offset).into(),
                ],
            );
        } else {
            let r = self.use_reg(arg, S1);
            let op = match ty {
                Type::I8 => "strb",
                Type::I16 => "strh",
                _ => "str",
            };
            self.mach(
                op,
                [(reg(r, ty)).into(), Address::offset(Reg::SP, offset).into()],
            );
        }
    }

    /// Loads a stack parameter at `[x29, #offset]` into `dst`.
    fn load_stack_param(&mut self, dst: Loc, ty: Type, offset: u32) {
        let (r, scratch) = match dst {
            Loc::Reg(r) | Loc::FReg(r) => (r, None),
            Loc::Stack(s) => (
                if ty.is_float() || ty.is_vector() {
                    FS0
                } else {
                    S0
                },
                Some(s),
            ),
            Loc::Remat(_) => unreachable!("a parameter cannot define a recipe"),
        };
        let op = match ty {
            Type::I8 => "ldrb",
            Type::I16 => "ldrh",
            _ => "ldr",
        };
        let rd = if ty.is_float() || ty.is_vector() {
            freg(r, ty)
        } else {
            reg(r, ty)
        };
        self.mach(
            op,
            [(rd).into(), Address::offset(Reg::x(29), offset).into()],
        );
        if let Some(s) = scratch {
            if ty.is_vector() {
                self.vector_spill(true, r, s);
                return;
            }
            let class = if ty.is_float() { 'd' } else { 'x' };
            self.mach(
                "str",
                [(Reg::scalar(class, r)).into(), (self.slot(s)).into()],
            );
        }
    }

    /// Lowers `data` if it takes or makes an `i128`, and returns whether it
    /// did. Operands are first copied into x0–x7 (which hold nothing
    /// outside calls) in order, an `i128` as a pair with its low half
    /// first; the result is built in x4:x5 (or wherever the lowering says)
    /// and moved out last, so every operand is read before the result is
    /// written.
    fn lower_i128(&mut self, inst: Inst, data: &InstData, result: Option<Value>) -> bool {
        let wide = |this: &Self, v: Value| this.ty(v) == Type::I128;
        let wide_result = result.is_some_and(|r| self.func.value_type(r) == Type::I128);
        match *data {
            InstData::Iconst {
                ty: Type::I128,
                imm,
            } => {
                self.materialize(4, imm as u64, true);
                self.materialize(5, (imm >> 63) as u64, true);
                self.pair_out(result, 4, 5);
            }
            InstData::Binary { op, args } if wide_result => {
                self.operands(&args);
                let (lo, hi) = self.binary_i128(inst, op);
                self.pair_out(result, lo, hi);
            }
            InstData::Unary { op, arg } if wide_result => {
                self.operands(&[arg]);
                self.unary_i128(op);
                self.pair_out(result, 4, 5);
            }
            InstData::Icmp { cond, args } if wide(self, args[0]) => {
                self.operands(&args);
                let d = self.def_reg(result);
                match cond {
                    IntCC::Eq | IntCC::Ne => {
                        self.mach("cmp", [(Reg::x(0)).into(), (Reg::x(2)).into()]);
                        self.mach(
                            "ccmp",
                            [
                                (Reg::x(1)).into(),
                                (Reg::x(3)).into(),
                                Arg::imm(0),
                                Arg::from("eq"),
                            ],
                        );
                    }
                    // a > b is b < a, and a <= b is b >= a: compare
                    // swapped. The borrow of the low halves feeds the high.
                    _ => {
                        let swap =
                            matches!(cond, IntCC::Sgt | IntCC::Sle | IntCC::Ugt | IntCC::Ule);
                        let (x, y) = if swap { (2, 0) } else { (0, 2) };
                        self.mach("cmp", [(Reg::x(x)).into(), (Reg::x(y)).into()]);
                        self.mach(
                            "sbcs",
                            [
                                (Reg::XZR).into(),
                                (Reg::x(x + 1)).into(),
                                (Reg::x(y + 1)).into(),
                            ],
                        );
                    }
                }
                let code = match cond {
                    IntCC::Eq => "eq",
                    IntCC::Ne => "ne",
                    IntCC::Slt | IntCC::Sgt => "lt",
                    IntCC::Sle | IntCC::Sge => "ge",
                    IntCC::Ult | IntCC::Ugt => "lo",
                    IntCC::Ule | IntCC::Uge => "hs",
                };
                self.mach("cset", [(Reg::w(d)).into(), (code).into()]);
                self.def_done(result, d);
            }
            InstData::Select { args } if wide(self, args[1]) => {
                // x0:x1 and x2:x3 are the arms, x4 (or x4:x5) the condition.
                self.operands(&[args[1], args[2], args[0]]);
                if wide(self, args[0]) {
                    self.mach(
                        "orr",
                        [(Reg::x(6)).into(), (Reg::x(4)).into(), (Reg::x(5)).into()],
                    );
                    self.mach("cmp", [(Reg::x(6)).into(), Arg::imm(0)]);
                } else {
                    self.test_nonzero(4, self.ty(args[0]));
                }
                self.mach(
                    "csel",
                    [
                        (Reg::x(6)).into(),
                        (Reg::x(0)).into(),
                        (Reg::x(2)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.mach(
                    "csel",
                    [
                        (Reg::x(7)).into(),
                        (Reg::x(1)).into(),
                        (Reg::x(3)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.pair_out(result, 6, 7);
            }
            InstData::Convert {
                op: op @ (ConvertOp::Sext | ConvertOp::Uext),
                ty: Type::I128,
                arg,
            } => {
                self.operands(&[arg]);
                let signed = op == ConvertOp::Sext;
                match (self.ty(arg), signed) {
                    (Type::I64, _) => self.mach("mov", [(Reg::x(4)).into(), (Reg::x(0)).into()]),
                    (Type::I32, true) => {
                        self.mach("sxtw", [(Reg::x(4)).into(), (Reg::w(0)).into()])
                    }
                    (Type::I32, false) => {
                        self.mach("mov", [(Reg::w(4)).into(), (Reg::w(0)).into()])
                    }
                    (Type::I16, true) => {
                        self.mach("sxth", [(Reg::x(4)).into(), (Reg::w(0)).into()])
                    }
                    (Type::I16, false) => {
                        self.mach("uxth", [(Reg::w(4)).into(), (Reg::w(0)).into()])
                    }
                    (_, true) => self.mach("sxtb", [(Reg::x(4)).into(), (Reg::w(0)).into()]),
                    (_, false) => self.mach("uxtb", [(Reg::w(4)).into(), (Reg::w(0)).into()]),
                }
                if signed {
                    self.mach(
                        "asr",
                        [(Reg::x(5)).into(), (Reg::x(4)).into(), Arg::imm(63)],
                    );
                } else {
                    self.mach("mov", [(Reg::x(5)).into(), Arg::imm(0)]);
                }
                self.pair_out(result, 4, 5);
            }
            InstData::Load {
                ty: Type::I128,
                addr,
                ..
            } => {
                let a = self.use_reg(addr, S0);
                self.mach(
                    "ldp",
                    [
                        (Reg::x(4)).into(),
                        (Reg::x(5)).into(),
                        Address::base(Reg::x(a)).into(),
                    ],
                );
                self.pair_out(result, 4, 5);
            }
            InstData::Store { value, addr, .. } if wide(self, value) => {
                self.operands(&[value, addr]);
                self.mach(
                    "stp",
                    [
                        (Reg::x(0)).into(),
                        (Reg::x(1)).into(),
                        Address::base(Reg::x(2)).into(),
                    ],
                );
            }
            // Multiplication is rejected in `new`. The carry or borrow of
            // the low halves feeds the high; the flags then describe the
            // whole 128 bits.
            InstData::Overflow { op, args } if wide(self, args[0]) => {
                self.operands(&args);
                let (lo, hi, code) = match op {
                    OverflowOp::Uadd => ("adds", "adcs", "hs"),
                    OverflowOp::Sadd => ("adds", "adcs", "vs"),
                    OverflowOp::Usub => ("subs", "sbcs", "lo"),
                    OverflowOp::Ssub => ("subs", "sbcs", "vs"),
                    OverflowOp::Umul | OverflowOp::Smul => unreachable!("rejected"),
                };
                self.mach(
                    lo,
                    [(Reg::x(4)).into(), (Reg::x(0)).into(), (Reg::x(2)).into()],
                );
                self.mach(
                    hi,
                    [(Reg::x(5)).into(), (Reg::x(1)).into(), (Reg::x(3)).into()],
                );
                self.mach("cset", [(Reg::w(6)).into(), (code).into()]);
                self.pair_out(result, 4, 5);
                let flag = self.func.inst_results(inst)[1];
                if let Some(loc) = self.defined(flag) {
                    self.emit_move(loc, Loc::Reg(6));
                }
            }
            _ => return false,
        }
        true
    }

    /// Copies `values` into x0, x1, ... in order, an `i128` taking two.
    fn operands(&mut self, values: &[Value]) {
        let mut moves = Vec::new();
        let mut next = 0;
        for &v in values {
            moves.push((Loc::Reg(next), self.loc(v)));
            next += 1;
            if self.ty(v) == Type::I128 {
                moves.push((Loc::Reg(next), self.hi(v)));
                next += 1;
            }
        }
        self.parallel_copy(&moves);
    }

    /// Moves an `i128` result from `x<lo>`:`x<hi>` to its locations.
    fn pair_out(&mut self, result: Option<Value>, lo: u8, hi: u8) {
        let Some(value) = result else { return };
        if let Some(loc) = self.defined(value) {
            self.emit_move(loc, Loc::Reg(lo));
        }
        if let Some(loc) = self.defined_hi(value) {
            self.emit_move(loc, Loc::Reg(hi));
        }
    }

    /// `x0:x1 op x2:x3` (or `x0:x1 op x2` for a shift amount), returning
    /// the registers of the result.
    fn binary_i128(&mut self, inst: Inst, op: BinaryOp) -> (u8, u8) {
        let pair = |this: &mut Self, lo: &str, hi: &str| {
            this.mach(lo, [Reg::x(4).into(), Reg::x(0).into(), Reg::x(2).into()]);
            this.mach(hi, [Reg::x(5).into(), Reg::x(1).into(), Reg::x(3).into()]);
        };
        match op {
            BinaryOp::Iadd => pair(self, "adds", "adc"),
            BinaryOp::Isub => pair(self, "subs", "sbc"),
            BinaryOp::And => pair(self, "and", "and"),
            BinaryOp::Or => pair(self, "orr", "orr"),
            BinaryOp::Xor => pair(self, "eor", "eor"),
            // (a1:a0)(b1:b0) mod 2^128 = a0 b0 + 2^64 (a0 b1 + a1 b0)
            BinaryOp::Imul => {
                self.mach(
                    "mul",
                    [(Reg::x(4)).into(), (Reg::x(0)).into(), (Reg::x(2)).into()],
                );
                self.mach(
                    "umulh",
                    [(Reg::x(5)).into(), (Reg::x(0)).into(), (Reg::x(2)).into()],
                );
                self.mach(
                    "madd",
                    [
                        (Reg::x(5)).into(),
                        (Reg::x(0)).into(),
                        (Reg::x(3)).into(),
                        (Reg::x(5)).into(),
                    ],
                );
                self.mach(
                    "madd",
                    [
                        (Reg::x(5)).into(),
                        (Reg::x(1)).into(),
                        (Reg::x(2)).into(),
                        (Reg::x(5)).into(),
                    ],
                );
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                self.mach(
                    "and",
                    [(Reg::x(2)).into(), (Reg::x(2)).into(), Arg::imm(127)],
                );
                self.shift_i128(op, 2, (4, 5), [3, 6, 7]);
            }
            // rotl(x, n) = (x << n) | (x >> (-n mod 128)), and the mirror.
            BinaryOp::Rotl | BinaryOp::Rotr => {
                self.mach(
                    "and",
                    [(Reg::x(2)).into(), (Reg::x(2)).into(), Arg::imm(127)],
                );
                self.mach("neg", [(Reg::x(3)).into(), (Reg::x(2)).into()]);
                self.mach(
                    "and",
                    [(Reg::x(3)).into(), (Reg::x(3)).into(), Arg::imm(127)],
                );
                let (first, second) = match op {
                    BinaryOp::Rotl => (BinaryOp::Shl, BinaryOp::Ushr),
                    _ => (BinaryOp::Ushr, BinaryOp::Shl),
                };
                self.shift_i128(first, 2, (4, 5), [6, 7, 8]);
                self.shift_i128(second, 3, (6, 7), [8, 16, 17]);
                self.mach(
                    "orr",
                    [(Reg::x(4)).into(), (Reg::x(4)).into(), (Reg::x(6)).into()],
                );
                self.mach(
                    "orr",
                    [(Reg::x(5)).into(), (Reg::x(5)).into(), (Reg::x(7)).into()],
                );
            }
            // compiler-rt takes and returns the pairs in x0:x1 and x2:x3,
            // where they already are. Traps first, as for narrower types.
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => {
                let trap = self.trap_label();
                self.mach(
                    "orr",
                    [(Reg::x(S0)).into(), (Reg::x(2)).into(), (Reg::x(3)).into()],
                );
                self.compare_branch(Reg::x(S0), false, trap.clone());
                if op == BinaryOp::Sdiv {
                    // MIN / -1: the divisor's halves are both all ones and
                    // the dividend is 2^127.
                    let ok = format!("L{}_{inst}_divok", self.func.name);
                    self.mach(
                        "and",
                        [(Reg::x(S0)).into(), (Reg::x(2)).into(), (Reg::x(3)).into()],
                    );
                    self.mach("cmn", [(Reg::x(S0)).into(), Arg::imm(1)]);
                    self.conditional("ne", ok.clone());
                    self.compare_branch(Reg::x(0), true, ok.clone());
                    self.materialize(S0, 1 << 63, true);
                    self.mach("cmp", [(Reg::x(1)).into(), (Reg::x(S0)).into()]);
                    self.conditional("eq", trap.clone());
                    self.out.label(ok.clone());
                }
                let function = match op {
                    BinaryOp::Udiv => "___udivti3",
                    BinaryOp::Sdiv => "___divti3",
                    BinaryOp::Urem => "___umodti3",
                    _ => "___modti3",
                };
                self.out.branch(true, function.to_string());
                return (0, 1);
            }
            _ => unreachable!("{op} on i128 is rejected or not an integer operation"),
        }
        (4, 5)
    }

    /// A 128-bit shift of x0:x1 by `x<n>` (already in 0..128) into
    /// `out`, using three temporaries; x0, x1 and `x<n>` are left alone.
    /// The halves shift by `n mod 64`; when `n >= 64` the result comes from
    /// the other half. The bits crossing between halves are `x >> (64 - n)`,
    /// computed as `(x >> 1) >> (63 - n)` so that `n = 0` gives 0.
    fn shift_i128(&mut self, op: BinaryOp, n: u8, (ol, oh): (u8, u8), [a, b, c]: [u8; 3]) {
        match op {
            BinaryOp::Shl => {
                self.mach(
                    "lsl",
                    [(Reg::x(a)).into(), (Reg::x(1)).into(), (Reg::x(n)).into()],
                );
                self.mach("mvn", [(Reg::w(b)).into(), (Reg::w(n)).into()]);
                self.mach("lsr", [(Reg::x(c)).into(), (Reg::x(0)).into(), Arg::imm(1)]);
                self.mach(
                    "lsr",
                    [(Reg::x(c)).into(), (Reg::x(c)).into(), (Reg::x(b)).into()],
                );
                self.mach(
                    "orr",
                    [(Reg::x(a)).into(), (Reg::x(a)).into(), (Reg::x(c)).into()],
                );
                self.mach(
                    "lsl",
                    [(Reg::x(c)).into(), (Reg::x(0)).into(), (Reg::x(n)).into()],
                );
                self.mach("tst", [(Reg::x(n)).into(), Arg::imm(64)]);
                self.mach(
                    "csel",
                    [
                        (Reg::x(oh)).into(),
                        (Reg::x(c)).into(),
                        (Reg::x(a)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.mach(
                    "csel",
                    [
                        (Reg::x(ol)).into(),
                        (Reg::XZR).into(),
                        (Reg::x(c)).into(),
                        Arg::from("ne"),
                    ],
                );
            }
            _ => {
                let shr = if op == BinaryOp::Sshr { "asr" } else { "lsr" };
                self.mach(
                    "lsr",
                    [(Reg::x(a)).into(), (Reg::x(0)).into(), (Reg::x(n)).into()],
                );
                self.mach("mvn", [(Reg::w(b)).into(), (Reg::w(n)).into()]);
                self.mach("lsl", [(Reg::x(c)).into(), (Reg::x(1)).into(), Arg::imm(1)]);
                self.mach(
                    "lsl",
                    [(Reg::x(c)).into(), (Reg::x(c)).into(), (Reg::x(b)).into()],
                );
                self.mach(
                    "orr",
                    [(Reg::x(a)).into(), (Reg::x(a)).into(), (Reg::x(c)).into()],
                );
                self.mach(
                    shr,
                    [(Reg::x(c)).into(), (Reg::x(1)).into(), (Reg::x(n)).into()],
                );
                // What fills the high half once n >= 64.
                if op == BinaryOp::Sshr {
                    self.mach(
                        "asr",
                        [(Reg::x(b)).into(), (Reg::x(1)).into(), Arg::imm(63)],
                    );
                } else {
                    self.mach("mov", [(Reg::x(b)).into(), Arg::imm(0)]);
                }
                self.mach("tst", [(Reg::x(n)).into(), Arg::imm(64)]);
                self.mach(
                    "csel",
                    [
                        (Reg::x(ol)).into(),
                        (Reg::x(c)).into(),
                        (Reg::x(a)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.mach(
                    "csel",
                    [
                        (Reg::x(oh)).into(),
                        (Reg::x(b)).into(),
                        (Reg::x(c)).into(),
                        Arg::from("ne"),
                    ],
                );
            }
        }
    }

    /// `op x0:x1` into x4:x5.
    fn unary_i128(&mut self, op: UnaryOp) {
        match op {
            UnaryOp::Ineg => {
                self.mach("negs", [(Reg::x(4)).into(), (Reg::x(0)).into()]);
                self.mach("ngc", [(Reg::x(5)).into(), (Reg::x(1)).into()]);
            }
            UnaryOp::Bnot => {
                self.mach("mvn", [(Reg::x(4)).into(), (Reg::x(0)).into()]);
                self.mach("mvn", [(Reg::x(5)).into(), (Reg::x(1)).into()]);
            }
            // The high half's count, or 64 plus the low half's.
            UnaryOp::Clz => {
                self.mach("cmp", [(Reg::x(1)).into(), Arg::imm(0)]);
                self.mach("clz", [(Reg::x(4)).into(), (Reg::x(1)).into()]);
                self.mach("clz", [(Reg::x(5)).into(), (Reg::x(0)).into()]);
                self.mach(
                    "add",
                    [(Reg::x(5)).into(), (Reg::x(5)).into(), Arg::imm(64)],
                );
                self.mach(
                    "csel",
                    [
                        (Reg::x(4)).into(),
                        (Reg::x(4)).into(),
                        (Reg::x(5)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.mach("mov", [(Reg::x(5)).into(), Arg::imm(0)]);
            }
            UnaryOp::Ctz => {
                self.mach("cmp", [(Reg::x(0)).into(), Arg::imm(0)]);
                self.mach("rbit", [(Reg::x(4)).into(), (Reg::x(0)).into()]);
                self.mach("clz", [(Reg::x(4)).into(), (Reg::x(4)).into()]);
                self.mach("rbit", [(Reg::x(5)).into(), (Reg::x(1)).into()]);
                self.mach("clz", [(Reg::x(5)).into(), (Reg::x(5)).into()]);
                self.mach(
                    "add",
                    [(Reg::x(5)).into(), (Reg::x(5)).into(), Arg::imm(64)],
                );
                self.mach(
                    "csel",
                    [
                        (Reg::x(4)).into(),
                        (Reg::x(4)).into(),
                        (Reg::x(5)).into(),
                        Arg::from("ne"),
                    ],
                );
                self.mach("mov", [(Reg::x(5)).into(), Arg::imm(0)]);
            }
            UnaryOp::Popcnt => {
                self.mach("fmov", [(Reg::d(0)).into(), (Reg::x(0)).into()]);
                self.mach("fmov", [(Reg::d(1)).into(), (Reg::x(1)).into()]);
                self.mach("cnt", [(Reg::v8b(0)).into(), (Reg::v8b(0)).into()]);
                self.mach("cnt", [(Reg::v8b(1)).into(), (Reg::v8b(1)).into()]);
                self.mach("addv", [(Reg::b(0)).into(), (Reg::v8b(0)).into()]);
                self.mach("addv", [(Reg::b(1)).into(), (Reg::v8b(1)).into()]);
                self.mach("fmov", [(Reg::w(4)).into(), (Reg::s(0)).into()]);
                self.mach("fmov", [(Reg::w(5)).into(), (Reg::s(1)).into()]);
                self.mach(
                    "add",
                    [(Reg::x(4)).into(), (Reg::x(4)).into(), (Reg::x(5)).into()],
                );
                self.mach("mov", [(Reg::x(5)).into(), Arg::imm(0)]);
            }
            UnaryOp::Bswap => {
                self.mach("rev", [(Reg::x(4)).into(), (Reg::x(1)).into()]);
                self.mach("rev", [(Reg::x(5)).into(), (Reg::x(0)).into()]);
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    fn fixed_sret_stack_offset(&self, arg: Value) -> Option<usize> {
        if self.has_inline_asm {
            return None;
        }
        let value = self.func.resolve(arg);
        let ValueDef::Result(inst, 0) = self.func.value_def(value) else {
            return None;
        };
        let InstData::StackAddr { slot } = *self.func.inst_data(inst) else {
            return None;
        };
        (self.func.first_result(inst) == value).then(|| self.stack_slot_offsets[slot.index()])
    }

    /// Moves a call's result, if it has one, from x0 or v0 to its place,
    /// and stores a struct result that came back in registers to its
    /// `sret` address.
    fn call_result(&mut self, result: Option<Value>, sret: Option<RegisterSret<'_>>) {
        if let Some(value) = result
            && let Some(loc) = self.defined(value)
        {
            self.emit_move(loc, return_loc(self.func.value_type(value)));
            if let Some(hi) = self.defined_hi(value) {
                self.emit_move(hi, Loc::Reg(1));
            }
        }
        if let Some(RegisterSret {
            regs,
            layout,
            stack_offset,
        }) = sret
        {
            if let Some(offset) = stack_offset {
                self.address(Loc::Reg(S0), self.base, offset);
            } else {
                let save = self.sret_save.expect("planned");
                self.mach(
                    "ldr",
                    [(Reg::x(S0)).into(), Address::offset(self.base, save).into()],
                );
            }
            match abi::classify(layout) {
                StructClass::Hfa { ty, .. } => {
                    for (&(offset, _), &r) in layout.leaves().iter().zip(&regs) {
                        self.mach(
                            "str",
                            [
                                (freg(loc_reg(r), ty)).into(),
                                Address::offset(Reg::x(S0), offset).into(),
                            ],
                        );
                    }
                }
                _ => {
                    for (k, &r) in regs.iter().enumerate() {
                        let offset = 8 * k as u64;
                        let n = (layout.size() - offset).min(8);
                        self.store_chunk(loc_reg(r), S0, offset, n);
                    }
                }
            }
        }
    }

    /// Stores struct registers `regs` (as passed or returned) into the
    /// frame at `area`, which is rounded up to 8 bytes, so whole registers
    /// fit.
    fn store_struct_regs(&mut self, regs: &[Loc], layout: &Layout, area: usize) {
        let base = self.base;
        match abi::classify(layout) {
            StructClass::Hfa { ty, .. } => {
                for (&(offset, _), &r) in layout.leaves().iter().zip(regs) {
                    let fr = freg(loc_reg(r), ty);
                    self.mach(
                        "str",
                        [
                            (fr).into(),
                            Address::offset(base, area + offset as usize).into(),
                        ],
                    );
                }
            }
            _ => {
                for (k, &r) in regs.iter().enumerate() {
                    let r = loc_reg(r);
                    self.mach(
                        "str",
                        [
                            (Reg::x(r)).into(),
                            Address::offset(base, area + 8 * k).into(),
                        ],
                    );
                }
            }
        }
    }

    /// Loads the registers a struct of `layout` is returned in from the
    /// frame at `area`.
    fn load_struct_regs(&mut self, layout: &Layout, area: usize) {
        let base = self.base;
        match abi::classify(layout) {
            StructClass::Hfa { ty, .. } => {
                for (i, &(offset, _)) in layout.leaves().iter().enumerate() {
                    let fr = freg(i as u8, ty);
                    self.mach(
                        "ldr",
                        [
                            (fr).into(),
                            Address::offset(base, area + offset as usize).into(),
                        ],
                    );
                }
            }
            StructClass::Gprs { count } => {
                for k in 0..count as usize {
                    self.mach(
                        "ldr",
                        [
                            (Reg::x(k)).into(),
                            Address::offset(base, area + 8 * k).into(),
                        ],
                    );
                }
            }
            StructClass::Indirect => unreachable!("returned through memory"),
        }
    }

    /// Loads a struct at `[x<p>]` into argument registers `regs`, reading
    /// exactly its bytes (reading past its end could fault).
    fn load_struct_exact(&mut self, regs: &[Loc], layout: &Layout, p: u8) {
        match abi::classify(layout) {
            StructClass::Hfa { ty, .. } => {
                for (&(offset, _), &r) in layout.leaves().iter().zip(regs) {
                    self.mach(
                        "ldr",
                        [
                            (freg(loc_reg(r), ty)).into(),
                            Address::offset(Reg::x(p), offset).into(),
                        ],
                    );
                }
            }
            _ => {
                for (k, &r) in regs.iter().enumerate() {
                    let offset = 8 * k as u64;
                    let n = (layout.size() - offset).min(8);
                    self.load_chunk(loc_reg(r), p, offset, n);
                }
            }
        }
    }

    /// Loads `n` (1 to 8) bytes at `[x<p>, #offset]` into `x<r>`, zero
    /// above them, piece by piece through x17. `r` and `p` are not x17.
    fn load_chunk(&mut self, r: u8, p: u8, offset: u64, n: u64) {
        for (i, (at, size)) in pieces(n).into_iter().enumerate() {
            let (op, _) = piece_ops(size);
            let target = if i == 0 { r } else { S1 };
            let wide = if size == 8 { 'x' } else { 'w' };
            self.mach(
                op,
                [
                    (Reg::scalar(wide, target)).into(),
                    Address::offset(Reg::x(p), offset + at).into(),
                ],
            );
            if i > 0 {
                self.mach(
                    "orr",
                    [
                        (Reg::x(r)).into(),
                        (Reg::x(r)).into(),
                        (Reg::x(S1)).into(),
                        Arg::shift("lsl", 8 * at),
                    ],
                );
            }
        }
    }

    /// Stores the low `n` (1 to 8) bytes of `x<r>` at `[x<p>, #offset]`.
    fn store_chunk(&mut self, r: u8, p: u8, offset: u64, n: u64) {
        for (at, size) in pieces(n) {
            let (_, op) = piece_ops(size);
            let wide = if size == 8 { 'x' } else { 'w' };
            let src = if at == 0 {
                r
            } else {
                self.mach(
                    "lsr",
                    [(Reg::x(S1)).into(), (Reg::x(r)).into(), Arg::imm(8 * at)],
                );
                S1
            };
            self.mach(
                op,
                [
                    (Reg::scalar(wide, src)).into(),
                    Address::offset(Reg::x(p), offset + at).into(),
                ],
            );
        }
    }

    /// Copies `size` bytes from `[x<src>]` to `[<base>, #to]` through x17.
    fn copy_bytes(&mut self, base: &str, to: usize, src: u8, size: u64) {
        for (at, n) in pieces(size) {
            let (load, store) = piece_ops(n);
            let wide = if n == 8 { 'x' } else { 'w' };
            self.mach(
                load,
                [
                    (Reg::scalar(wide, S1)).into(),
                    Address::offset(Reg::x(src), at).into(),
                ],
            );
            self.mach(
                store,
                [
                    (Reg::scalar(wide, S1)).into(),
                    Address::offset(base, to + at as usize).into(),
                ],
            );
        }
    }

    /// Puts the address `base + offset` in `dst`.
    fn address(&mut self, dst: Loc, base: &str, offset: usize) {
        let r = match dst {
            Loc::Reg(r) => r,
            Loc::Remat(_) => unreachable!("an address destination cannot be a recipe"),
            _ => S0,
        };
        // `add` takes a 12-bit immediate, optionally shifted by 12.
        self.mach(
            "add",
            [
                (Reg::x(r)).into(),
                Reg::from(base).into(),
                Arg::imm(offset & 0xfff),
            ],
        );
        if offset >> 12 != 0 {
            self.mach(
                "add",
                [
                    (Reg::x(r)).into(),
                    (Reg::x(r)).into(),
                    Arg::imm(offset >> 12),
                    Arg::shift("lsl", 12),
                ],
            );
        }
        if let Loc::Stack(s) = dst {
            self.mach("str", [(Reg::x(r)).into(), (self.slot(s)).into()]);
        }
    }

    /// Lowers a conversion to, from or between floats.
    fn float_convert(&mut self, op: ConvertOp, to: Type, arg: Value, result: Option<Value>) {
        let from = self.ty(arg);
        if to.is_float() {
            let d = self.def_freg(result);
            let fd = freg(d, to);
            if from.is_float() {
                // fpext, fptrunc
                let a = self.use_freg(arg, FS0);
                self.mach("fcvt", [(fd).into(), (freg(a, from)).into()]);
            } else {
                let a = self.use_reg(arg, S0);
                match op {
                    ConvertOp::Bitcast => self.mach("fmov", [(fd).into(), (reg(a, from)).into()]),
                    _ => {
                        let signed = op == ConvertOp::Sitofp;
                        let x = reg(self.extend(S0, a, from, signed), from);
                        let mnemonic = if signed { "scvtf" } else { "ucvtf" };
                        self.mach(mnemonic, [(fd).into(), (x).into()]);
                    }
                }
            }
            self.def_done(result, d);
        } else {
            // fptosi, fptoui, bitcast to an integer
            let d = self.def_reg(result);
            let a = self.use_freg(arg, FS0);
            let mnemonic = match op {
                ConvertOp::Fptosi => "fcvtzs",
                ConvertOp::Fptoui => "fcvtzu",
                _ => "fmov",
            };
            self.mach(mnemonic, [(reg(d, to)).into(), (freg(a, from)).into()]);
            self.def_done(result, d);
        }
    }

    /// Lowers `*_overflow` on up to 64 bits: the result into x16, the flag
    /// into x8, both moved out at the end. `i8` and `i16` compute in 32
    /// bits from extended operands, which cannot overflow there, and check
    /// that the result fits.
    fn overflow(&mut self, inst: Inst, op: OverflowOp, args: [Value; 2]) {
        let ty = self.ty(args[0]);
        let signed = matches!(op, OverflowOp::Sadd | OverflowOp::Ssub | OverflowOp::Smul);
        let a = self.use_reg(args[0], S0);
        let a = self.extend(S0, a, ty, signed);
        let b = self.use_reg(args[1], S1);
        let b = self.extend(S1, b, ty, signed);
        match narrow_bits(ty) {
            Some(bits) => {
                let mnemonic = match op {
                    OverflowOp::Uadd | OverflowOp::Sadd => "add",
                    OverflowOp::Usub | OverflowOp::Ssub => "sub",
                    OverflowOp::Umul | OverflowOp::Smul => "mul",
                };
                self.mach(
                    mnemonic,
                    [(Reg::w(S0)).into(), (Reg::w(a)).into(), (Reg::w(b)).into()],
                );
                if signed {
                    let ext = if bits == 8 { "sxtb" } else { "sxth" };
                    self.mach(
                        "cmp",
                        [(Reg::w(S0)).into(), (Reg::w(S0)).into(), (ext).into()],
                    );
                } else {
                    let high = !((1u32 << bits) - 1);
                    self.mach("tst", [(Reg::w(S0)).into(), Arg::imm(high)]);
                }
                self.mach("cset", [(Reg::w(S2)).into(), Arg::from("ne")]);
            }
            None => {
                let (ra, rb, rd) = (reg(a, ty), reg(b, ty), reg(S0, ty));
                match (op, ty) {
                    (OverflowOp::Uadd | OverflowOp::Sadd, _) => {
                        self.mach("adds", [(rd).into(), (ra).into(), (rb).into()])
                    }
                    (OverflowOp::Usub | OverflowOp::Ssub, _) => {
                        self.mach("subs", [(rd).into(), (ra).into(), (rb).into()])
                    }
                    // The full product's high half must be zero, or the
                    // sign extension of the low half.
                    (OverflowOp::Umul, Type::I64) => {
                        self.mach(
                            "umulh",
                            [(Reg::x(S2)).into(), (Reg::x(a)).into(), (Reg::x(b)).into()],
                        );
                        self.mach(
                            "mul",
                            [(Reg::x(S0)).into(), (Reg::x(a)).into(), (Reg::x(b)).into()],
                        );
                        self.mach("cmp", [(Reg::x(S2)).into(), Arg::imm(0)]);
                    }
                    (OverflowOp::Smul, Type::I64) => {
                        self.mach(
                            "smulh",
                            [(Reg::x(S2)).into(), (Reg::x(a)).into(), (Reg::x(b)).into()],
                        );
                        self.mach(
                            "mul",
                            [(Reg::x(S0)).into(), (Reg::x(a)).into(), (Reg::x(b)).into()],
                        );
                        self.mach(
                            "cmp",
                            [
                                (Reg::x(S2)).into(),
                                (Reg::x(S0)).into(),
                                Arg::shift("asr", 63),
                            ],
                        );
                    }
                    (OverflowOp::Umul, _) => {
                        self.mach(
                            "umull",
                            [(Reg::x(S0)).into(), (Reg::w(a)).into(), (Reg::w(b)).into()],
                        );
                        self.mach(
                            "cmp",
                            [
                                (Reg::XZR).into(),
                                (Reg::x(S0)).into(),
                                Arg::shift("lsr", 32),
                            ],
                        );
                    }
                    (OverflowOp::Smul, _) => {
                        self.mach(
                            "smull",
                            [(Reg::x(S0)).into(), (Reg::w(a)).into(), (Reg::w(b)).into()],
                        );
                        self.mach(
                            "cmp",
                            [(Reg::x(S0)).into(), (Reg::w(S0)).into(), Arg::from("sxtw")],
                        );
                    }
                }
                let code = match op {
                    OverflowOp::Uadd => "hs",
                    OverflowOp::Usub => "lo",
                    OverflowOp::Sadd | OverflowOp::Ssub => "vs",
                    OverflowOp::Umul | OverflowOp::Smul => "ne",
                };
                self.mach("cset", [(Reg::w(S2)).into(), (code).into()]);
            }
        }
        let results = self.func.inst_results(inst);
        let (value, flag) = (results[0], results[1]);
        if let Some(loc) = self.defined(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.defined(flag) {
            self.emit_move(loc, Loc::Reg(S2));
        }
    }

    /// Restores the callee-saved registers, `sp` and the frame pointer.
    fn epilogue(&mut self) {
        if self.frameless {
            return;
        }
        let saved = self.alloc.callee_saved_used.clone();
        let float_saved = self.alloc.float_callee_saved_used.clone();
        let pairs = saved.len().div_ceil(2) + float_saved.len().div_ceil(2);
        // Point `sp` at the last pair pushed and pop in reverse. Measuring
        // from x29 makes the size of the rest of the frame irrelevant.
        if pairs == 0 {
            self.native(
                Opcode::ADD_64_addsub_imm,
                [
                    (FieldId::Rd, 31),
                    (FieldId::Rn, 29),
                    (FieldId::imm12, 0),
                    (FieldId::sh, 0),
                ],
                |out| out.write_str("mov\tsp, x29"),
            );
        } else {
            self.native(
                Opcode::SUB_64_addsub_imm,
                [
                    (FieldId::Rd, 31),
                    (FieldId::Rn, 29),
                    (FieldId::imm12, (16 * pairs) as u32),
                    (FieldId::sh, 0),
                ],
                |out| write!(out, "sub\tsp, x29, #{}", 16 * pairs),
            );
        }
        for pair in float_saved.chunks(2).rev() {
            match pair {
                [a, b] => self.native(
                    Opcode::LDP_D_ldstpair_post,
                    [
                        (FieldId::Rt, (*a).into()),
                        (FieldId::Rt2, (*b).into()),
                        (FieldId::Rn, 31),
                        (FieldId::imm7, 2),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text("ldp\t"),
                                Class('d', *a),
                                Text(", "),
                                Class('d', *b),
                                Text(", [sp], #16"),
                            ],
                        )
                    },
                ),
                [a] => self.native(
                    Opcode::LDR_D_ldst_immpost,
                    [
                        (FieldId::Rt, (*a).into()),
                        (FieldId::Rn, 31),
                        (FieldId::imm9, 16),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[Text("ldr\t"), Class('d', *a), Text(", [sp], #16")],
                        )
                    },
                ),
                _ => unreachable!(),
            }
        }
        for pair in saved.chunks(2).rev() {
            match pair {
                [a, b] => self.native(
                    Opcode::LDP_64_ldstpair_post,
                    [
                        (FieldId::Rt, (*a).into()),
                        (FieldId::Rt2, (*b).into()),
                        (FieldId::Rn, 31),
                        (FieldId::imm7, 2),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text("ldp\t"),
                                Class('x', *a),
                                Text(", "),
                                Class('x', *b),
                                Text(", [sp], #16"),
                            ],
                        )
                    },
                ),
                [a] => self.native(
                    Opcode::LDR_64_ldst_immpost,
                    [
                        (FieldId::Rt, (*a).into()),
                        (FieldId::Rn, 31),
                        (FieldId::imm9, 16),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[Text("ldr\t"), Class('x', *a), Text(", [sp], #16")],
                        )
                    },
                ),
                _ => unreachable!(),
            }
        }
        self.native(
            Opcode::LDP_64_ldstpair_post,
            [
                (FieldId::Rt, 29),
                (FieldId::Rt2, 30),
                (FieldId::Rn, 31),
                (FieldId::imm7, 2),
            ],
            |out| out.write_str("ldp\tx29, x30, [sp], #16"),
        );
    }

    /// Computes `args[0] op args[1]` into `d`.
    fn binary(&mut self, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let data = InstData::Binary { op, args };
        if let Some(plan) = addsub_immediate(self.func, &data) {
            let a = self.use_reg(plan.input, S0);
            let mnemonic = if plan.subtract { "sub" } else { "add" };
            if plan.shift == 0 {
                self.mach(
                    mnemonic,
                    [reg(d, ty).into(), reg(a, ty).into(), Arg::imm(plan.imm12)],
                );
            } else {
                self.mach(
                    mnemonic,
                    [
                        reg(d, ty).into(),
                        reg(a, ty).into(),
                        Arg::imm(plan.imm12),
                        Arg::shift("lsl", plan.shift),
                    ],
                );
            }
            return;
        }
        if matches!(op, BinaryOp::And | BinaryOp::Or | BinaryOp::Xor)
            && let Some((folded, k)) = folded_imm(self.func, &data)
        {
            let is64 = ty == Type::I64;
            // Use the very same width normalization and checked encoding as
            // eligibility. Only one operand occurrence is folded, even for k op k.
            if let Some(plan) = native::logical_immediate(k as u64, if is64 { 64 } else { 32 }) {
                let input = if self.func.resolve(args[1]) == folded {
                    args[0]
                } else {
                    args[1]
                };
                let a = self.use_reg(input, S0);
                let (mnemonic, opcode) = match (op, is64) {
                    (BinaryOp::And, false) => ("and", Opcode::AND_32_log_imm),
                    (BinaryOp::And, true) => ("and", Opcode::AND_64_log_imm),
                    (BinaryOp::Or, false) => ("orr", Opcode::ORR_32_log_imm),
                    (BinaryOp::Or, true) => ("orr", Opcode::ORR_64_log_imm),
                    (BinaryOp::Xor, false) => ("eor", Opcode::EOR_32_log_imm),
                    (BinaryOp::Xor, true) => ("eor", Opcode::EOR_64_log_imm),
                    _ => unreachable!(),
                };
                let text = |out: &mut String| {
                    write!(
                        out,
                        "{mnemonic}\t{}, {}, #0x{:x}",
                        reg(d, ty),
                        reg(a, ty),
                        plan.value
                    )
                };
                if is64 {
                    self.native(
                        opcode,
                        [
                            (FieldId::Rd, d.into()),
                            (FieldId::Rn, a.into()),
                            (FieldId::immr, plan.immr),
                            (FieldId::imms, plan.imms),
                            (FieldId::N, plan.n),
                        ],
                        text,
                    );
                } else {
                    self.native(
                        opcode,
                        [
                            (FieldId::Rd, d.into()),
                            (FieldId::Rn, a.into()),
                            (FieldId::immr, plan.immr),
                            (FieldId::imms, plan.imms),
                        ],
                        text,
                    );
                }
                return;
            }
        }
        let a = self.use_reg(args[0], S0);
        if let Some((_, k)) = folded_imm(self.func, &data) {
            let rd = reg(d, ty);
            let mnemonic = match op {
                BinaryOp::Iadd | BinaryOp::Ptradd => "add",
                BinaryOp::Isub => "sub",
                BinaryOp::Shl => "lsl",
                BinaryOp::Ushr => "lsr",
                BinaryOp::Sshr => "asr",
                _ => unreachable!("folded binary opcode must have explicit emission"),
            };
            let a = match op {
                BinaryOp::Ushr => self.extend(S0, a, ty, false),
                BinaryOp::Sshr => self.extend(S0, a, ty, true),
                _ => a,
            };
            let is64 = ty.bytes().is_none_or(|n| n > 4);
            let bits = if is64 { 64 } else { 32 };
            let opcode = match (mnemonic, is64) {
                ("add", true) => Opcode::ADD_64_addsub_imm,
                ("add", false) => Opcode::ADD_32_addsub_imm,
                ("sub", true) => Opcode::SUB_64_addsub_imm,
                ("sub", false) => Opcode::SUB_32_addsub_imm,
                ("asr", true) => Opcode::SBFM_64M_bitfield,
                ("asr", false) => Opcode::SBFM_32M_bitfield,
                (_, true) => Opcode::UBFM_64M_bitfield,
                (_, false) => Opcode::UBFM_32M_bitfield,
            };
            if matches!(mnemonic, "add" | "sub") {
                self.native(
                    opcode,
                    [
                        (FieldId::Rd, d.into()),
                        (FieldId::Rn, a.into()),
                        (FieldId::imm12, k as u32),
                        (FieldId::sh, 0),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text(mnemonic),
                                Text("\t"),
                                R(rd),
                                Text(", "),
                                R(reg(a, ty)),
                                Text(", #"),
                                Number(k.into()),
                            ],
                        )
                    },
                );
            } else {
                let amount = k as u32;
                let (immr, imms) = if mnemonic == "lsl" {
                    ((bits - amount) % bits, bits - 1 - amount)
                } else {
                    (amount, bits - 1)
                };
                self.native(
                    opcode,
                    [
                        (FieldId::Rd, d.into()),
                        (FieldId::Rn, a.into()),
                        (FieldId::immr, immr),
                        (FieldId::imms, imms),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text(mnemonic),
                                Text("\t"),
                                R(rd),
                                Text(", "),
                                R(reg(a, ty)),
                                Text(", #"),
                                Number(k.into()),
                            ],
                        )
                    },
                );
            }
            return;
        }
        if let Some(divisor) = reducible_divisor(self.func, &data) {
            self.constant_division(op, ty, a, args[1], divisor, d);
            return;
        }
        let b = self.use_reg(args[1], S1);
        let (ra, rb, rd) = (reg(a, ty), reg(b, ty), reg(d, ty));
        let simple = match op {
            BinaryOp::Iadd | BinaryOp::Ptradd => Some("add"),
            BinaryOp::Isub => Some("sub"),
            BinaryOp::Imul => Some("mul"),
            BinaryOp::And => Some("and"),
            BinaryOp::Or => Some("orr"),
            BinaryOp::Xor => Some("eor"),
            _ => None,
        };
        if let Some(mnemonic) = simple {
            let is64 = ty.bytes().is_none_or(|n| n > 4);
            let opcode = match (mnemonic, is64) {
                ("add", true) => Opcode::ADD_64_addsub_shift,
                ("add", false) => Opcode::ADD_32_addsub_shift,
                ("sub", true) => Opcode::SUB_64_addsub_shift,
                ("sub", false) => Opcode::SUB_32_addsub_shift,
                ("and", true) => Opcode::AND_64_log_shift,
                ("and", false) => Opcode::AND_32_log_shift,
                ("orr", true) => Opcode::ORR_64_log_shift,
                ("orr", false) => Opcode::ORR_32_log_shift,
                ("eor", true) => Opcode::EOR_64_log_shift,
                ("eor", false) => Opcode::EOR_32_log_shift,
                ("mul", true) => Opcode::MADD_64A_dp_3src,
                ("mul", false) => Opcode::MADD_32A_dp_3src,
                _ => unreachable!(),
            };
            if mnemonic == "mul" {
                self.native(
                    opcode,
                    [
                        (FieldId::Rd, d.into()),
                        (FieldId::Rn, a.into()),
                        (FieldId::Rm, b.into()),
                        (FieldId::Ra, 31),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text(mnemonic),
                                Text("\t"),
                                R(rd),
                                Text(", "),
                                R(ra),
                                Text(", "),
                                R(rb),
                            ],
                        )
                    },
                );
            } else {
                self.native(
                    opcode,
                    [
                        (FieldId::Rd, d.into()),
                        (FieldId::Rn, a.into()),
                        (FieldId::Rm, b.into()),
                        (FieldId::shift, 0),
                        (FieldId::imm6, 0),
                    ],
                    |out| {
                        native::write_pieces(
                            out,
                            &[
                                Text(mnemonic),
                                Text("\t"),
                                R(rd),
                                Text(", "),
                                R(ra),
                                Text(", "),
                                R(rb),
                            ],
                        )
                    },
                );
            }
            return;
        }
        match op {
            BinaryOp::Rotl | BinaryOp::Rotr => match narrow_bits(ty) {
                // `ror` takes its amount modulo the width; rotating left by
                // n is rotating right by -n.
                None => {
                    if op == BinaryOp::Rotl {
                        let neg = reg(S1, ty);
                        self.mach("neg", [(neg).into(), (rb).into()]);
                        self.mach("ror", [(rd).into(), (ra).into(), (neg).into()]);
                    } else {
                        self.mach("ror", [(rd).into(), (ra).into(), (rb).into()]);
                    }
                }
                // No narrow rotate instruction: (x << n) | (x >> (bits - n))
                // on the zero-extended value. n = 0 shifts right by `bits`,
                // which gives 0 in a 32-bit register, as needed.
                Some(bits) => {
                    let x = self.extend(S0, a, ty, false);
                    // w17 = n, w8 = bits - n
                    self.mach(
                        "and",
                        [(Reg::w(S1)).into(), (Reg::w(b)).into(), Arg::imm(bits - 1)],
                    );
                    self.mach("mov", [(Reg::w(S2)).into(), Arg::imm(bits)]);
                    self.mach(
                        "sub",
                        [
                            (Reg::w(S2)).into(),
                            (Reg::w(S2)).into(),
                            (Reg::w(S1)).into(),
                        ],
                    );
                    let (first, second) = match op {
                        BinaryOp::Rotl => ("lsl", "lsr"),
                        _ => ("lsr", "lsl"),
                    };
                    self.mach(
                        first,
                        [(Reg::w(S1)).into(), (Reg::w(x)).into(), (Reg::w(S1)).into()],
                    );
                    self.mach(
                        second,
                        [(Reg::w(S2)).into(), (Reg::w(x)).into(), (Reg::w(S2)).into()],
                    );
                    self.mach(
                        "orr",
                        [(Reg::w(d)).into(), (Reg::w(S1)).into(), (Reg::w(S2)).into()],
                    );
                }
            },
            BinaryOp::Umulhi | BinaryOp::Smulhi => {
                let signed = op == BinaryOp::Smulhi;
                match ty {
                    Type::I64 => {
                        let mnemonic = if signed { "smulh" } else { "umulh" };
                        self.mach(
                            mnemonic,
                            [(Reg::x(d)).into(), (Reg::x(a)).into(), (Reg::x(b)).into()],
                        );
                    }
                    // Widening 32x32 -> 64 multiply, then take the top half.
                    Type::I32 => {
                        let (mul, shr) = if signed {
                            ("smull", "asr")
                        } else {
                            ("umull", "lsr")
                        };
                        self.mach(
                            mul,
                            [(Reg::x(d)).into(), (Reg::w(a)).into(), (Reg::w(b)).into()],
                        );
                        self.mach(shr, [(Reg::x(d)).into(), (Reg::x(d)).into(), Arg::imm(32)]);
                    }
                    // The full product of two extended i8/i16 fits in 32 bits.
                    _ => {
                        let bits = narrow_bits(ty).expect("i8 or i16");
                        let a = self.extend(S0, a, ty, signed);
                        let b = self.extend(S1, b, ty, signed);
                        self.mach(
                            "mul",
                            [(Reg::w(d)).into(), (Reg::w(a)).into(), (Reg::w(b)).into()],
                        );
                        let shr = if signed { "asr" } else { "lsr" };
                        self.mach(
                            shr,
                            [(Reg::w(d)).into(), (Reg::w(d)).into(), Arg::imm(bits)],
                        );
                    }
                }
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                // Shift amounts are taken modulo the bit width. The hardware
                // does that for 32 and 64 bits; narrower types need a mask.
                let b = match narrow_bits(ty) {
                    Some(bits) => {
                        self.mach(
                            "and",
                            [(Reg::w(S1)).into(), (Reg::w(b)).into(), Arg::imm(bits - 1)],
                        );
                        S1
                    }
                    None => b,
                };
                let (mnemonic, a) = match op {
                    BinaryOp::Shl => ("lsl", a),
                    BinaryOp::Ushr => ("lsr", self.extend(S0, a, ty, false)),
                    _ => ("asr", self.extend(S0, a, ty, true)),
                };
                self.mach(
                    mnemonic,
                    [(rd).into(), (reg(a, ty)).into(), (reg(b, ty)).into()],
                );
            }
            BinaryOp::Udiv | BinaryOp::Urem | BinaryOp::Sdiv | BinaryOp::Srem => {
                let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
                let a = reg(self.extend(S0, a, ty, signed), ty);
                let b = reg(self.extend(S1, b, ty, signed), ty);
                let width = ty.bytes().unwrap() * 8;
                let mask = u64::MAX >> (64 - width);
                // Iconst's immediate wraps to the IR width, even if it spells
                // zero or -1 with a noncanonical bit pattern.
                let divisor = iconst(self.func, args[1]).map(|n| n as u64 & mask);
                // Hardware division returns 0 for a zero divisor; the IR traps.
                if divisor.is_none_or(|n| n == 0) {
                    let trap = self.trap_label();
                    self.compare_branch(b, false, trap);
                }
                if op == BinaryOp::Sdiv && divisor.is_none_or(|n| n == mask) {
                    // MIN / -1 overflows. Hardware wraps; the IR traps.
                    let min = reg(S2, ty);
                    self.materialize(S2, min_value(ty), ty == Type::I64);
                    self.mach("cmn", [(b).into(), Arg::imm(1)]);
                    self.mach(
                        "ccmp",
                        [(a).into(), (min).into(), Arg::imm(0), Arg::from("eq")],
                    );
                    let trap = self.trap_label();
                    self.conditional("eq", trap);
                }
                let div = if signed { "sdiv" } else { "udiv" };
                match op {
                    // remainder = a - (a / b) * b, with the quotient in a
                    // scratch register since `a` and `b` are read again.
                    BinaryOp::Urem | BinaryOp::Srem => {
                        let q = reg(S2, ty);
                        self.mach(div, [(q).into(), (a).into(), (b).into()]);
                        self.mach("msub", [(rd).into(), (q).into(), (b).into(), (a).into()]);
                    }
                    _ => self.mach(div, [(rd).into(), (a).into(), (b).into()]),
                }
            }
            _ => unreachable!("handled above"),
        }
    }

    /// Divides by a [`reducible_divisor`] without `sdiv`/`udiv`: shifts for
    /// a power of two, else a multiply by a fixed-point reciprocal (Hacker's
    /// Delight, chapter 10).
    ///
    /// Narrow operands are extended to 32 bits and divided as i32, which
    /// gives the same register bits as `sdiv`/`udiv` on extended operands.
    /// Intermediate values use reserved scratch registers. The result may
    /// alias the dividend or the divisor. Only a remainder reads the divisor.
    fn constant_division(
        &mut self,
        op: BinaryOp,
        ty: Type,
        a: u8,
        divisor_value: Value,
        divisor: i64,
        d: u8,
    ) {
        let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
        let remainder = matches!(op, BinaryOp::Urem | BinaryOp::Srem);
        let is64 = ty == Type::I64;
        let bits = if is64 { 64 } else { 32 };
        let magnitude = if signed {
            divisor.unsigned_abs()
        } else {
            divisor as u64
        };
        let r = |n: u8| if is64 { Reg::x(n) } else { Reg::w(n) };
        let a = self.extend(S0, a, ty, signed);
        // The quotient's register: a remainder still reads `a` afterwards.
        let q = if remainder { S2 } else { d };
        if magnitude.is_power_of_two() {
            let k = magnitude.trailing_zeros();
            if !signed {
                if remainder {
                    self.mach("and", [r(d).into(), r(a).into(), Arg::imm(magnitude - 1)]);
                } else {
                    self.mach("lsr", [r(d).into(), r(a).into(), Arg::imm(k)]);
                }
                return;
            }
            // Add |d| - 1 to a negative dividend so the shift rounds toward
            // zero. Neither the bias nor the negation overflows, d = MIN too.
            if k == 1 {
                self.mach(
                    "add",
                    [
                        r(S2).into(),
                        r(a).into(),
                        r(a).into(),
                        Arg::shift("lsr", bits - 1),
                    ],
                );
            } else {
                self.mach("asr", [r(S2).into(), r(a).into(), Arg::imm(bits - 1)]);
                self.mach(
                    "add",
                    [
                        r(S2).into(),
                        r(a).into(),
                        r(S2).into(),
                        Arg::shift("lsr", bits - k),
                    ],
                );
            }
            if divisor > 0 {
                self.mach("asr", [r(q).into(), r(S2).into(), Arg::imm(k)]);
            } else {
                let zero = if is64 { Reg::XZR } else { Reg::WZR };
                self.mach(
                    "sub",
                    [r(q).into(), zero.into(), r(S2).into(), Arg::shift("asr", k)],
                );
            }
        } else if signed {
            let (multiplier, shift) = signed_reciprocal(bits, divisor);
            // The multiplier is used as signed; when its sign differs from
            // the divisor's, the dividend corrects the high product.
            let correction = match (divisor > 0, multiplier >> (bits - 1) == 1) {
                (true, true) => Some("add"),
                (false, false) => Some("sub"),
                _ => None,
            };
            self.materialize(S2, multiplier, is64);
            if is64 {
                self.mach(
                    "smulh",
                    [(Reg::x(S2)).into(), (Reg::x(a)).into(), (Reg::x(S2)).into()],
                );
                if let Some(c) = correction {
                    self.mach(
                        c,
                        [(Reg::x(S2)).into(), (Reg::x(S2)).into(), (Reg::x(a)).into()],
                    );
                }
                if shift > 0 {
                    self.mach(
                        "asr",
                        [(Reg::x(S2)).into(), (Reg::x(S2)).into(), Arg::imm(shift)],
                    );
                }
            } else {
                self.mach(
                    "smull",
                    [(Reg::x(S2)).into(), (Reg::w(a)).into(), (Reg::w(S2)).into()],
                );
                match correction {
                    // The high-half and final shifts combine.
                    None => self.mach(
                        "asr",
                        [
                            (Reg::x(S2)).into(),
                            (Reg::x(S2)).into(),
                            Arg::imm(32 + shift),
                        ],
                    ),
                    Some(c) => {
                        self.mach(
                            "asr",
                            [(Reg::x(S2)).into(), (Reg::x(S2)).into(), Arg::imm(32)],
                        );
                        self.mach(
                            c,
                            [(Reg::w(S2)).into(), (Reg::w(S2)).into(), (Reg::w(a)).into()],
                        );
                        if shift > 0 {
                            self.mach(
                                "asr",
                                [(Reg::w(S2)).into(), (Reg::w(S2)).into(), Arg::imm(shift)],
                            );
                        }
                    }
                }
            }
            // The shifts round down; a negative quotient rounds up by one.
            self.mach(
                "add",
                [
                    r(q).into(),
                    r(S2).into(),
                    r(S2).into(),
                    Arg::shift("lsr", bits - 1),
                ],
            );
        } else {
            let reciprocal = unsigned_reciprocal(bits, magnitude);
            self.materialize(S2, reciprocal.multiplier, is64);
            if is64 {
                self.mach(
                    "umulh",
                    [(Reg::x(S2)).into(), (Reg::x(a)).into(), (Reg::x(S2)).into()],
                );
                if reciprocal.add {
                    // hi + (n - hi) / 2 averages without overflowing.
                    self.mach(
                        "sub",
                        [(Reg::x(S1)).into(), (Reg::x(a)).into(), (Reg::x(S2)).into()],
                    );
                    self.mach(
                        "add",
                        [
                            (Reg::x(S2)).into(),
                            (Reg::x(S2)).into(),
                            (Reg::x(S1)).into(),
                            Arg::shift("lsr", 1),
                        ],
                    );
                }
                self.mach(
                    "lsr",
                    [
                        (Reg::x(q)).into(),
                        (Reg::x(S2)).into(),
                        Arg::imm(reciprocal.shift),
                    ],
                );
            } else {
                self.mach(
                    "umull",
                    [(Reg::x(S2)).into(), (Reg::w(a)).into(), (Reg::w(S2)).into()],
                );
                if reciprocal.add {
                    // hi + n fits in 64 bits, so average it directly.
                    self.mach(
                        "lsr",
                        [(Reg::x(S2)).into(), (Reg::x(S2)).into(), Arg::imm(32)],
                    );
                    self.mach(
                        "add",
                        [
                            (Reg::x(S2)).into(),
                            (Reg::x(S2)).into(),
                            (Reg::w(a)).into(),
                            Arg::from("uxtw"),
                        ],
                    );
                    self.mach(
                        "lsr",
                        [
                            (Reg::x(q)).into(),
                            (Reg::x(S2)).into(),
                            Arg::imm(reciprocal.shift + 1),
                        ],
                    );
                } else {
                    self.mach(
                        "lsr",
                        [
                            (Reg::x(q)).into(),
                            (Reg::x(S2)).into(),
                            Arg::imm(32 + reciprocal.shift),
                        ],
                    );
                }
            }
        }
        if remainder {
            // remainder = a - q * d, reading the divisor only now.
            let b = self.use_reg(divisor_value, S1);
            let b = self.extend(S1, b, ty, signed);
            self.mach(
                "msub",
                [r(d).into(), r(S2).into(), r(b).into(), r(a).into()],
            );
        }
    }

    /// Computes `op arg` into `d`.
    fn unary(&mut self, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_reg(arg, S0);
        let (ra, rd) = (reg(a, ty), reg(d, ty));
        match op {
            UnaryOp::Ineg => self.mach("neg", [(rd).into(), (ra).into()]),
            UnaryOp::Bnot => self.mach("mvn", [(rd).into(), (ra).into()]),
            UnaryOp::Clz => {
                // Count in 32 bits, then drop the zeros above the narrow width.
                let x = reg(self.extend(S0, a, ty, false), ty);
                self.mach("clz", [(rd).into(), (x).into()]);
                if let Some(bits) = narrow_bits(ty) {
                    self.mach(
                        "sub",
                        [(Reg::w(d)).into(), (Reg::w(d)).into(), Arg::imm(32 - bits)],
                    );
                }
            }
            UnaryOp::Ctz => {
                // ctz = clz of the bit-reversed value. For narrow types, a
                // set bit just above the width caps the count at the width
                // and hides the unspecified upper bits.
                let x = reg(S0, ty);
                match narrow_bits(ty) {
                    Some(bits) => {
                        self.mach("orr", [(x).into(), (ra).into(), Arg::imm(1u32 << bits)]);
                        self.mach("rbit", [(x).into(), (x).into()]);
                    }
                    None => self.mach("rbit", [(x).into(), (ra).into()]),
                }
                self.mach("clz", [(rd).into(), (x).into()]);
            }
            UnaryOp::Popcnt => {
                // No scalar popcount: count bits per byte in a SIMD register
                // (`cnt`), then add the byte counts (`addv`).
                let x = self.extend(S0, a, ty, false);
                if ty == Type::I64 {
                    self.mach("fmov", [(Reg::d(0)).into(), (Reg::x(x)).into()]);
                } else {
                    self.mach("fmov", [(Reg::s(0)).into(), (Reg::w(x)).into()]);
                }
                self.mach("cnt", [(Reg::v8b(0)).into(), (Reg::v8b(0)).into()]);
                self.mach("addv", [(Reg::b(0)).into(), (Reg::v8b(0)).into()]);
                self.mach("fmov", [(Reg::w(d)).into(), (Reg::s(0)).into()]);
            }
            UnaryOp::Bswap => match ty {
                // Swaps the bytes within each halfword; the low one is ours.
                Type::I16 => self.mach("rev16", [(Reg::w(d)).into(), (Reg::w(a)).into()]),
                _ => self.mach("rev", [(rd).into(), (ra).into()]),
            },
            _ => unreachable!("float operations are lowered in `inst`"),
        }
    }

    /// Lowers `switch`: a jump table if the cases are dense, else a chain of
    /// compares. Like `brif`, a target with block arguments gets a landing
    /// pad that copies them.
    fn switch(
        &mut self,
        inst: Inst,
        arg: Value,
        dests: &[BlockCall],
        cases: &[i64],
        site: Option<Inst>,
    ) {
        // Allocation identity remains `inst`; only assembly identities vary.
        let inst = site.map_or_else(|| inst.to_string(), |site| format!("{inst}_tail{site}"));
        let ty = self.ty(arg);
        let is64 = ty == Type::I64;
        // Compare the zero-extended bits with the cases as unsigned bits.
        let mask = u64::MAX >> (64 - 8 * ty.bytes().expect("integer"));
        let x = self.use_reg(arg, S0);
        let x = self.extend(S0, x, ty, false);
        let values: Vec<u64> = cases.iter().map(|&c| c as u64 & mask).collect();
        let targets: Vec<String> = dests
            .iter()
            .enumerate()
            .map(
                |(i, d)| match d.args.is_empty() && self.edge_moves(i).is_empty() {
                    true => self.label(d.block),
                    false => format!("L{}_{inst}_edge{i}", self.func.name),
                },
            )
            .collect();

        let (min, max) = match (values.iter().min(), values.iter().max()) {
            (Some(&min), Some(&max)) => (min, max),
            _ => (0, 0),
        };
        let span = max - min;
        if values.len() >= 4 && span < 3 * values.len() as u64 && span < 4096 {
            // idx = x - min, unsigned; anything above the span is default.
            let idx = if min == 0 {
                x
            } else {
                let sub = self.operand(S1, min, is64);
                self.mach(
                    "sub",
                    [(reg(S0, ty)).into(), (reg(x, ty)).into(), (sub).into()],
                );
                S0
            };
            self.mach("cmp", [(reg(idx, ty)).into(), Arg::imm(span)]);
            self.conditional("hi", targets[0].clone());
            let table = format!("L{}_{inst}_table", self.func.name);
            let index = Address::index(
                Reg::x(S1),
                reg(idx, ty),
                if is64 { "lsl" } else { "uxtw" },
                Some(2),
            );
            self.out.symbolic(Symbolic::Address {
                register: S1,
                target: table.clone(),
            });
            self.mach("ldrsw", [(Reg::x(S0)).into(), index.into()]);
            self.mach(
                "add",
                [
                    (Reg::x(S1)).into(),
                    (Reg::x(S1)).into(),
                    (Reg::x(S0)).into(),
                ],
            );
            self.mach("br", [(Reg::x(S1)).into()]);
            // Offsets from the table to each target, never executed.
            self.out.label(table.clone());
            for k in 0..=span {
                let i = values
                    .iter()
                    .position(|&v| v == min + k)
                    .map_or(0, |i| i + 1);
                self.out
                    .integer(4, Expression::difference(targets[i].clone(), table.clone()));
            }
        } else {
            for (i, &value) in values.iter().enumerate() {
                let operand = self.operand(S1, value, is64);
                self.mach("cmp", [(reg(x, ty)).into(), (operand).into()]);
                self.conditional("eq", targets[i + 1].clone());
            }
            // Case landing pads are emitted before the next IR block. The
            // default may fall through only when none of those pads intervene.
            if dests
                .iter()
                .enumerate()
                .all(|(i, dest)| dest.args.is_empty() && self.edge_moves(i).is_empty())
            {
                self.branch_unless_next(dests[0].block);
            } else {
                self.out.branch(false, targets[0].clone());
            }
        }
        for (i, (dest, target)) in dests.iter().zip(&targets).enumerate() {
            if !dest.args.is_empty() || !self.edge_moves(i).is_empty() {
                self.out.label(target.clone());
                self.block_args(dest, i);
                let label = self.label(dest.block);
                self.out.branch(false, label);
            }
        }
    }

    /// Emits the compare of `icmp`/`fcmp` `inst`, returning the condition
    /// code that holds when its condition does.
    fn compare(&mut self, inst: Inst) -> &'static str {
        if let Some(&(source, invert)) = self.selection.conditions.get(&inst) {
            let cc = self.compare(source);
            return if invert { invert_cond(cc) } else { cc };
        }
        match *self.func.inst_data(inst) {
            InstData::Icmp { cond, args } => {
                let ty = self.ty(args[0]);
                if let Some(plan) = addsub_immediate(self.func, self.func.inst_data(inst)) {
                    let a = self.use_reg(plan.input, S0);
                    let mnemonic = if plan.subtract { "cmp" } else { "cmn" };
                    if plan.shift == 0 {
                        self.mach(mnemonic, [reg(a, ty).into(), Arg::imm(plan.imm12)]);
                    } else {
                        self.mach(
                            mnemonic,
                            [
                                reg(a, ty).into(),
                                Arg::imm(plan.imm12),
                                Arg::shift("lsl", plan.shift),
                            ],
                        );
                    }
                    return cond_code(cond);
                }
                let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
                let a = self.use_reg(args[0], S0);
                let a = if !signed && self.zero_extended_narrow(args[0]) {
                    a
                } else {
                    self.extend(S0, a, ty, signed)
                };
                let b = match folded_imm(self.func, self.func.inst_data(inst)) {
                    Some((_, k)) => Arg::imm(k),
                    None => {
                        let b = self.use_reg(args[1], S1);
                        let b = if !signed && self.zero_extended_narrow(args[1]) {
                            b
                        } else {
                            self.extend(S1, b, ty, signed)
                        };
                        reg(b, ty).into()
                    }
                };
                self.mach("cmp", [(reg(a, ty)).into(), (b).into()]);
                cond_code(cond)
            }
            InstData::Fcmp { cond, args } => {
                let ty = self.ty(args[0]);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                self.mach("fcmp", [(freg(a, ty)).into(), (freg(b, ty)).into()]);
                float_cond_code(cond)
            }
            _ => unreachable!("not a compare"),
        }
    }

    /// `value` as the second operand of `cmp`/`sub`: an immediate if it fits
    /// in 12 bits, else materialized in `scratch`.
    fn operand(&mut self, scratch: u8, value: u64, is64: bool) -> Arg {
        if value < 4096 {
            return Arg::imm(value);
        }
        self.materialize(scratch, value, is64);
        if is64 {
            Reg::x(scratch).into()
        } else {
            Reg::w(scratch).into()
        }
    }

    /// Sets flags so that `ne` means the value in `reg` is non-zero.
    fn test_nonzero(&mut self, reg: u8, ty: Type) {
        match ty {
            // Only the low bits of a narrow value are defined.
            Type::I8 => self.mach("tst", [(Reg::w(reg)).into(), Arg::imm(0xff)]),
            Type::I16 => self.mach("tst", [(Reg::w(reg)).into(), Arg::imm(0xffff)]),
            Type::I32 => self.mach("cmp", [(Reg::w(reg)).into(), Arg::imm(0)]),
            _ => self.mach("cmp", [(Reg::x(reg)).into(), Arg::imm(0)]),
        }
    }

    /// Copies block-call arguments into the target's parameters. All copies
    /// happen at once (`jump block1(v4, v3)` may swap two parameters), so
    /// they go through the parallel-copy sequentializer.
    fn block_args_noop(&self, call: &BlockCall, successor: usize) -> bool {
        self.edge_moves(successor)
            .iter()
            .all(|(dst, src)| dst == src)
            && self
                .func
                .block_params(call.block)
                .iter()
                .zip(&call.args)
                .all(|(&param, &arg)| {
                    let low = self
                        .alloc
                        .param_loc(call.block, crate::regalloc::ValuePart::low(param));
                    let high = self
                        .alloc
                        .param_loc(call.block, crate::regalloc::ValuePart::high(param));
                    low.is_none_or(|dst| dst == self.loc(arg))
                        && high.is_none_or(|dst| dst == self.hi(arg))
                })
    }

    fn block_args(&mut self, call: &BlockCall, successor: usize) {
        let params = self.func.block_params(call.block);
        let mut moves = self.edge_moves(successor).to_vec();
        for (&param, &arg) in params.iter().zip(&call.args) {
            // A parameter with no location is an alias, never read.
            let Some(dst) = self
                .alloc
                .param_loc(call.block, crate::regalloc::ValuePart::low(param))
            else {
                continue;
            };
            moves.push((dst, self.loc(arg)));
            if let Some(hi) = self
                .alloc
                .param_loc(call.block, crate::regalloc::ValuePart::high(param))
            {
                moves.push((hi, self.hi(arg)));
            }
        }
        self.parallel_copy(&moves);
    }

    /// Emits `moves`, each `(dst, src)`, as if they all happened at once.
    /// Pure FP-register copies in vector functions can use reserved v18 for
    /// full-width cycles. Other vector copies need the 16-byte stack temporary;
    /// otherwise x16 suffices for either register class.
    fn parallel_copy(&mut self, moves: &[(Loc, Loc)]) {
        // FS2 is outside the supported allocation pools. Check even self-copies
        // for its presence: the sequentializer requires an absent temporary.
        // Only FReg-to-FReg moves are safe while it holds a cycle value;
        // notably, vector stack-to-stack emit_move uses FS2 itself.
        let register_only = self.copy_temp.is_some()
            && moves.iter().all(|&(dst, src)| {
                dst != Loc::FReg(FS2)
                    && src != Loc::FReg(FS2)
                    && (dst == src || matches!((dst, src), (Loc::FReg(_), Loc::FReg(_))))
            });
        let temp = if register_only {
            Loc::FReg(FS2)
        } else {
            self.copy_temp.map_or(Loc::Reg(S0), Loc::Stack)
        };
        for (dst, src) in sequentialize(moves, temp) {
            self.emit_move(dst, src);
        }
    }

    /// One 64-bit move between any two locations. Narrow values have
    /// unspecified upper bits, so copying all 64 is always fine.
    fn emit_move(&mut self, dst: Loc, src: Loc) {
        match (dst, src) {
            (Loc::Remat(_), _) => unreachable!("a move cannot define a recipe"),
            _ if dst == src => {}
            (Loc::Reg(d), Loc::Remat(value)) => self.rematerialize(d, value),
            (Loc::FReg(d), Loc::Remat(value)) => {
                self.rematerialize(S1, value);
                self.emit_move(Loc::FReg(d), Loc::Reg(S1));
            }
            (Loc::Stack(d), Loc::Remat(value)) => {
                // x16 may contain a pending parallel-copy cycle value.
                self.rematerialize(S1, value);
                self.spill(true, false, S1, d);
            }
            (Loc::Reg(d), Loc::Reg(s)) => self.native(
                Opcode::ORR_64_log_shift,
                [
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, 31),
                    (FieldId::Rm, s.into()),
                    (FieldId::shift, 0),
                    (FieldId::imm6, 0),
                ],
                |out| {
                    native::write_pieces(
                        out,
                        &[Text("mov\t"), Class('x', d), Text(", "), Class('x', s)],
                    )
                },
            ),
            (Loc::FReg(d), Loc::FReg(s)) if self.copy_temp.is_some() => self.move_vector(d, s),
            (Loc::FReg(d), Loc::FReg(s)) => self.native(
                Opcode::FMOV_D_floatdp1,
                [(FieldId::Rd, d.into()), (FieldId::Rn, s.into())],
                |out| {
                    native::write_pieces(
                        out,
                        &[Text("fmov\t"), Class('d', d), Text(", "), Class('d', s)],
                    )
                },
            ),
            (Loc::Reg(d), Loc::FReg(s)) => self.native(
                Opcode::FMOV_64D_float2int,
                [(FieldId::Rd, d.into()), (FieldId::Rn, s.into())],
                |out| {
                    native::write_pieces(
                        out,
                        &[Text("fmov\t"), Class('x', d), Text(", "), Class('d', s)],
                    )
                },
            ),
            (Loc::FReg(d), Loc::Reg(s)) => self.native(
                Opcode::FMOV_D64_float2int,
                [(FieldId::Rd, d.into()), (FieldId::Rn, s.into())],
                |out| {
                    native::write_pieces(
                        out,
                        &[Text("fmov\t"), Class('d', d), Text(", "), Class('x', s)],
                    )
                },
            ),
            (Loc::Reg(d), Loc::Stack(s)) => self.spill(false, false, d, s),
            (Loc::FReg(d), Loc::Stack(s)) => self.spill(false, true, d, s),
            (Loc::Stack(d), Loc::Reg(s)) => self.spill(true, false, s, d),
            (Loc::Stack(d), Loc::FReg(s)) => self.spill(true, true, s, d),
            (Loc::Stack(d), Loc::Stack(s)) if self.vector_spills.contains(&d) => {
                self.spill(false, true, FS2, s);
                self.spill(true, true, FS2, d);
            }
            (Loc::Stack(d), Loc::Stack(s)) => {
                self.spill(false, false, S1, s);
                self.spill(true, false, S1, d);
            }
        }
    }

    fn branch_unless_next(&mut self, target: Block) {
        if self.next_block != Some(target) {
            let label = self.label(target);
            self.out.branch(false, label);
        }
    }

    fn zero_extended_narrow(&self, value: Value) -> bool {
        crate::fold::zero_extended_narrow(self.func, &self.selection, &self.alloc, value, true)
    }

    /// Sign- or zero-extends a narrow value in `w<src>` into `w<scratch>`
    /// and returns `scratch`; returns `src` unchanged for 32- and 64-bit
    /// types. Extending in place would clobber the operand's register.
    fn extend(&mut self, scratch: u8, src: u8, ty: Type, signed: bool) -> u8 {
        let op = match (ty, signed) {
            (Type::I8, true) => "sxtb",
            (Type::I8, false) => "uxtb",
            (Type::I16, true) => "sxth",
            (Type::I16, false) => "uxth",
            _ => return src,
        };
        self.mach(op, [(Reg::w(scratch)).into(), (Reg::w(src)).into()]);
        scratch
    }

    /// Loads the exact requested width without changing NZCV, including when
    /// rematerializing between a compare and its branch during parallel copies.
    fn materialize(&mut self, reg: u8, bits: u64, is64: bool) {
        let plan = constant_plan::Plan::new(bits, is64);
        let class = if is64 { 'x' } else { 'w' };
        let (seed_lane, seed_imm) = plan.seed();
        let opcode = match (is64, plan.inverted()) {
            (true, false) => Opcode::MOVZ_64_movewide,
            (false, false) => Opcode::MOVZ_32_movewide,
            (true, true) => Opcode::MOVN_64_movewide,
            (false, true) => Opcode::MOVN_32_movewide,
        };
        let mnemonic = if plan.inverted() { "movn" } else { "movz" };
        self.native(
            opcode,
            [
                (FieldId::Rd, reg.into()),
                (FieldId::imm16, seed_imm.into()),
                (FieldId::hw, seed_lane.into()),
            ],
            |out| {
                native::write_pieces(
                    out,
                    &[
                        Text(mnemonic),
                        Text("\t"),
                        Class(class, reg),
                        Text(", #"),
                        Hex(seed_imm.into()),
                    ],
                )?;
                if seed_lane != 0 {
                    native::write_pieces(
                        out,
                        &[Text(", lsl #"), Number(16 * i128::from(seed_lane))],
                    )?;
                }
                Ok(())
            },
        );
        for (lane, imm) in plan.patches() {
            self.native(
                if is64 {
                    Opcode::MOVK_64_movewide
                } else {
                    Opcode::MOVK_32_movewide
                },
                [
                    (FieldId::Rd, reg.into()),
                    (FieldId::imm16, imm.into()),
                    (FieldId::hw, lane.into()),
                ],
                |out| {
                    native::write_pieces(
                        out,
                        &[
                            Text("movk\t"),
                            Class(class, reg),
                            Text(", #"),
                            Hex(imm.into()),
                            Text(", lsl #"),
                            Number(16 * i128::from(lane)),
                        ],
                    )
                },
            );
        }
    }

    fn sub_sp(&mut self, bytes: usize) {
        if bytes >> 12 != 0 {
            self.native(
                Opcode::SUB_64_addsub_imm,
                [
                    (FieldId::Rd, 31),
                    (FieldId::Rn, 31),
                    (FieldId::imm12, (bytes >> 12) as u32),
                    (FieldId::sh, 1),
                ],
                |out| write!(out, "sub\tsp, sp, #{}, lsl #12", bytes >> 12),
            );
        }
        if bytes & 0xfff != 0 {
            self.native(
                Opcode::SUB_64_addsub_imm,
                [
                    (FieldId::Rd, 31),
                    (FieldId::Rn, 31),
                    (FieldId::imm12, (bytes & 0xfff) as u32),
                    (FieldId::sh, 0),
                ],
                |out| write!(out, "sub\tsp, sp, #{}", bytes & 0xfff),
            );
        }
    }

    fn trap_label(&mut self) -> String {
        self.uses_trap = true;
        format!("L{}_trap", self.func.name)
    }

    /// The location of an operand.
    fn loc(&self, value: Value) -> Loc {
        self.alloc
            .operand_loc(
                self.allocation_index,
                crate::regalloc::ValuePart::low(self.func.resolve(value)),
                crate::regalloc::OperandKind::Use,
            )
            .expect("operand in reachable code is allocated")
    }

    /// The register holding operand `value`: its own, or `scratch` after
    /// reloading it from its spill slot.
    /// The memory operand for an access of type `ty` at `addr`: the stack
    /// slot itself if `addr` is one's address and the offset is encodable,
    /// else the address in a register.
    fn mem(&mut self, addr: Value, scratch: u8, ty: Type) -> Address {
        if let Some(offset) = folded_offset(self.func, &self.stack_slot_offsets, addr, ty) {
            return Address::offset(self.base, offset);
        }
        if let Some(&(base, offset)) = self.selection.addresses.get(&self.func.resolve(addr)) {
            let base = self.use_reg(base, scratch);
            return Address::offset(Reg::x(base), offset);
        }
        if let Some(&address) = self
            .selection
            .indexed_addresses
            .get(&self.func.resolve(addr))
        {
            let base = self.use_reg(address.base, scratch);
            // Stores may already hold a spilled value in S0 and use S1 for
            // their base. S2 is distinct from both and from allocated GPRs.
            let index = self.use_reg(address.index, S2);
            return Address::index(
                Reg::x(base),
                Reg::x(index),
                "lsl",
                Some(address.shift.into()),
            );
        }
        Address::base(Reg::x(self.use_reg(addr, scratch)))
    }

    /// Recreates a pure scalar value without changing NZCV. This also runs
    /// during parallel copies, where flags may hold a pending branch result.
    fn rematerialize(&mut self, scratch: u8, value: Value) {
        match regalloc::remat_recipe(self.func, value).expect("validated recipe") {
            RematRecipe::Iconst(bits) => {
                #[cfg(feature = "backend-profile")]
                crate::backend_profile::count(crate::backend_profile::Counter::RematConstants);
                self.materialize(scratch, bits, true);
            }
            RematRecipe::NullPtr => {
                #[cfg(feature = "backend-profile")]
                crate::backend_profile::count(crate::backend_profile::Counter::RematConstants);
                self.materialize(scratch, 0, true);
            }
            RematRecipe::StackAddr(slot) => {
                #[cfg(feature = "backend-profile")]
                crate::backend_profile::count(crate::backend_profile::Counter::RematStackAddresses);
                let offset = self.stack_slot_offsets[slot.index()];
                self.address(Loc::Reg(scratch), self.base, offset);
            }
        }
    }

    fn use_reg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::Reg(r) => r,
            Loc::Stack(s) => {
                self.spill(false, false, scratch, s);
                scratch
            }
            Loc::Remat(value) => {
                self.rematerialize(scratch, value);
                scratch
            }
            Loc::FReg(_) => unreachable!("{value} is a float"),
        }
    }

    /// The high half of `i128` operand `value`, like [`Self::use_reg`].
    fn use_reg_hi(&mut self, value: Value, scratch: u8) -> u8 {
        match self.hi(value) {
            Loc::Reg(r) => r,
            Loc::Stack(s) => {
                self.spill(false, false, scratch, s);
                scratch
            }
            Loc::FReg(_) | Loc::Remat(_) => unreachable!("an integer half"),
        }
    }

    /// The location of the high half of `i128` operand `value`.
    fn hi(&self, value: Value) -> Loc {
        self.alloc
            .operand_loc(
                self.allocation_index,
                crate::regalloc::ValuePart::high(self.func.resolve(value)),
                crate::regalloc::OperandKind::Use,
            )
            .expect("operand in reachable code is allocated")
    }

    /// Sets flags so that `ne` means `value` is non-zero; an `i128` is
    /// non-zero if either half is.
    fn test_value(&mut self, value: Value) {
        if self.ty(value) == Type::I128 {
            let lo = self.use_reg(value, S0);
            let hi = self.use_reg_hi(value, S1);
            self.mach(
                "orr",
                [
                    (Reg::x(S0)).into(),
                    (Reg::x(lo)).into(),
                    (Reg::x(hi)).into(),
                ],
            );
            self.mach("cmp", [(Reg::x(S0)).into(), Arg::imm(0)]);
        } else {
            let c = self.use_reg(value, S0);
            self.test_nonzero(c, self.ty(value));
        }
    }

    /// [`Self::use_reg`] for floats: the FP register holding `value`.
    fn use_freg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::FReg(r) => r,
            Loc::Stack(s) => {
                self.spill(false, true, scratch, s);
                scratch
            }
            Loc::Reg(_) | Loc::Remat(_) => unreachable!("{value} is not a float"),
        }
    }

    /// The register to compute `result` into: its own, or x16 if it is
    /// spilled (or an alias, whose value is never read).
    fn def_reg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.defined(value) {
            Some(Loc::Reg(r)) => r,
            Some(Loc::Remat(_)) => unreachable!("a lowered instruction cannot define a recipe"),
            _ => S0,
        }
    }

    /// [`Self::def_reg`] for floats: the result's FP register, or v16.
    fn def_freg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.defined(value) {
            Some(Loc::FReg(r)) => r,
            Some(Loc::Remat(_)) => unreachable!("a float instruction cannot define a recipe"),
            _ => FS0,
        }
    }

    /// Stores a spilled result computed into `reg` by [`Self::def_reg`] or
    /// [`Self::def_freg`].
    fn def_done(&mut self, result: Option<Value>, reg: u8) {
        let Some(value) = result else { return };
        match self.defined(value) {
            Some(Loc::Stack(s)) => {
                self.spill(
                    true,
                    self.func.value_type(value).is_float()
                        || self.func.value_type(value).is_vector(),
                    reg,
                    s,
                );
            }
            Some(Loc::Remat(_)) => unreachable!("a lowered instruction cannot define a recipe"),
            _ => {}
        }
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(self.func.resolve(value))
    }

    /// The address of spill slot `index`: above the outgoing arguments at
    /// the bottom of the frame, close enough to `sp` for a scaled immediate.
    fn slot(&self, index: u32) -> Address {
        Address::offset(self.base, self.spill_base + 8 * index as usize)
    }

    fn write_loc_name<W: Write + ?Sized>(&self, out: &mut W, loc: Loc) {
        match loc {
            Loc::Reg(r) => crate::text::write_class(out, 'x', r.into()),
            Loc::FReg(r) => crate::text::write_class(out, 'd', r.into()),
            Loc::Stack(s) => self.slot(s).write_to(out),
            Loc::Remat(value) => {
                crate::text::write_decimal(out, "remat(v", value.index() as i128).unwrap();
                out.write_str(")")
            }
        }
        .unwrap();
    }

    fn label(&self, block: Block) -> String {
        crate::text::block_label("L", &self.func.name, block)
    }

    fn conditional(&mut self, condition: &str, target: String) {
        self.out.symbolic(Symbolic::Conditional {
            condition: native::condition(condition) as u8,
            target,
        });
    }
    fn brif_false(&mut self, test: &BrifTest, target: String) {
        match test {
            BrifTest::Flags(condition) => self.conditional(condition, target),
            BrifTest::Zero(register) => self.compare_branch(*register, false, target),
        }
    }
    fn compare_branch(&mut self, r: Reg, nonzero: bool, target: String) {
        self.out.symbolic(Symbolic::Compare {
            register: r.number as u8,
            is64: r.bits() == 64,
            nonzero,
            target,
        });
    }
    fn atomic<const N: usize>(&mut self, root: &str, order: &str, ty: Type, args: [Arg; N]) {
        let width = width_suffix(ty);
        if self.out.structured() {
            let i = native::atomic(root, order, width, &args);
            self.out.native(|| i, |_| unreachable!());
        } else {
            write!(self.out, "\t{root}{order}{width}\t").unwrap();
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                write!(self.out, "{arg}").unwrap();
            }
            self.out.push_str("\n");
        }
    }

    fn mach<const N: usize>(&mut self, op: &str, args: [Arg; N]) {
        if self.out.structured() {
            let instruction = native::lower(op, &args);
            self.out.native(|| instruction, |_| unreachable!());
        } else {
            self.out.push_str("\t");
            self.out.push_str(op);
            for (i, a) in args.iter().enumerate() {
                self.out.push_str(if i == 0 { "\t" } else { ", " });
                a.write_to(&mut self.out).unwrap();
            }
            self.out.push_str("\n");
        }
    }

    fn native<const N: usize>(
        &mut self,
        opcode: Opcode,
        fields: [(FieldId, u32); N],
        text: impl FnOnce(&mut String) -> std::fmt::Result,
    ) {
        self.out
            .native(|| NativeInstruction::new(opcode, fields), text);
    }

    fn spill(&mut self, store: bool, float: bool, register: u8, slot: u32) {
        #[cfg(feature = "backend-profile")]
        crate::backend_profile::count(if store {
            crate::backend_profile::Counter::SpillHelperStores
        } else {
            crate::backend_profile::Counter::SpillHelperLoads
        });
        if float && self.vector_spills.contains(&slot) {
            self.vector_spill(store, register, slot);
            return;
        }
        let opcode = match (store, float) {
            (false, false) => Opcode::LDR_64_ldst_pos,
            (false, true) => Opcode::LDR_D_ldst_pos,
            (true, false) => Opcode::STR_64_ldst_pos,
            (true, true) => Opcode::STR_D_ldst_pos,
        };
        let offset = self.spill_base + 8 * slot as usize;
        let base = self.base;
        self.native(
            opcode,
            [
                (FieldId::Rt, register.into()),
                (FieldId::Rn, if base == "sp" { 31 } else { 19 }),
                (FieldId::imm12, (offset / 8) as u32),
            ],
            |out| {
                native::write_pieces(
                    out,
                    &[
                        Text(if store { "str\t" } else { "ldr\t" }),
                        Class(if float { 'd' } else { 'x' }, register),
                        Text(", ["),
                        Text(base),
                        Text(", #"),
                        Number(offset as i128),
                        Text("]"),
                    ],
                )
            },
        );
    }

    fn move_gp(&mut self, d: u8, n: u8, is64: bool) {
        self.native(
            if is64 {
                Opcode::ORR_64_log_shift
            } else {
                Opcode::ORR_32_log_shift
            },
            [
                (FieldId::Rd, d.into()),
                (FieldId::Rn, 31),
                (FieldId::Rm, n.into()),
                (FieldId::shift, 0),
                (FieldId::imm6, 0),
            ],
            |out| {
                write!(
                    out,
                    "mov\t{}{d}, {}{n}",
                    if is64 { 'x' } else { 'w' },
                    if is64 { 'x' } else { 'w' }
                )
            },
        );
    }
}

/// A function's language-specific data area (LSDA), the table its
/// personality reads while unwinding, in the format of GCC's
/// `__gcc_except_tab`:
///
/// - a **call-site table**: for every range of code that can throw, the
///   landing pad to enter and the first action. It must cover every call,
///   so the gaps between invokes are listed with no landing pad, meaning
///   "keep unwinding" (a call missing from the table terminates).
/// - an **action table**: per landing pad, a chain of records (filter,
///   offset to the next). A filter `n > 0` catches the type in entry `n`
///   of the type table, and 0 is a cleanup. The personality enters the
///   pad with the first matching filter as the selector.
/// - a **type table** of typeinfo pointers, indexed backwards from its
///   end; a null entry catches everything.
#[derive(Default)]
struct ExceptionTable {
    /// Typeinfos in the type table, in order of first appearance; `None`
    /// catches any exception.
    types: Vec<Option<DataRef>>,
    /// Typeinfo names, for the type table.
    type_names: Vec<String>,
    /// The action table, as single-byte filter and next fields.
    actions: Vec<i8>,
    /// Each landing pad's first action, 1-based (0: cleanup only).
    pad_actions: Vec<(Block, u32)>,
    /// Each invoke's call: its label prefix, its landing pad's label and
    /// block, in code order.
    sites: Vec<(String, String, Block)>,
}

impl ExceptionTable {
    fn new(func: &Function) -> Result<Self, String> {
        let mut table = ExceptionTable {
            types: Vec::new(),
            type_names: Vec::new(),
            actions: Vec::new(),
            pad_actions: Vec::new(),
            sites: Vec::new(),
        };
        for (block, inst) in func.layout_insts() {
            let InstData::LandingPad { clauses } = func.inst_data(inst) else {
                continue;
            };
            if clauses == &[Clause::Cleanup] {
                table.pad_actions.push((block, 0));
                continue;
            }
            table
                .pad_actions
                .push((block, table.actions.len() as u32 + 1));
            for (i, clause) in clauses.iter().enumerate() {
                let filter = match *clause {
                    Clause::Catch(typeinfo) => {
                        if !table.types.contains(&typeinfo) {
                            table.types.push(typeinfo);
                            let name = typeinfo.map_or(String::new(), |t| {
                                func.symbol_name(keel_ir::Symbol::Data(t)).to_string()
                            });
                            table.type_names.push(name);
                        }
                        table.type_index(typeinfo)
                    }
                    Clause::Cleanup => 0,
                };
                // Both fields are SLEB128, one byte while they are below
                // 64; the next record follows directly, one byte on.
                if filter >= 64 {
                    return Err("more than 63 caught types in one function".into());
                }
                table.actions.push(filter as i8);
                table
                    .actions
                    .push(if i + 1 < clauses.len() { 1 } else { 0 });
            }
        }
        Ok(table)
    }

    /// The selector for `typeinfo`: its 1-based type table index.
    fn type_index(&self, typeinfo: Option<DataRef>) -> u64 {
        let i = self.types.iter().position(|&t| t == typeinfo);
        i.expect("a caught type") as u64 + 1
    }

    /// Emits the LSDA after the function `name`, and returns to `__text`.
    fn emit<T: Sink>(&self, out: &mut T, name: &str) {
        let l = |what: &str| format!("L{name}_{what}");
        let begin = l("begin");
        out.section("__TEXT", "__gcc_except_tab", SectionKind::Data);
        out.align(2);
        out.label(format!("GCC_except_table_{name}"));
        out.label(l("lsda"));
        out.number(1, 255);
        if self.types.is_empty() {
            out.number(1, 255);
        } else {
            out.number(1, 155);
            out.leb(false, Expression::difference(l("ttbase"), l("ttref")));
            out.label(l("ttref"));
        }
        out.number(1, 1);
        out.leb(false, Expression::difference(l("cst_end"), l("cst_begin")));
        out.label(l("cst_begin"));
        let entry = |out: &mut T, from: &str, to: &str, pad: Option<(&str, u32)>| {
            out.leb(false, Expression::difference(from, &begin));
            out.leb(false, Expression::difference(to, from));
            if let Some((pad, action)) = pad {
                out.leb(false, Expression::difference(pad, &begin));
                out.leb(false, Expression::constant(i128::from(action)));
            } else {
                out.bytes(&[0, 0]);
            }
        };
        let mut covered = begin.clone();
        for (site, pad, block) in &self.sites {
            let (from, to) = (format!("{site}_begin"), format!("{site}_end"));
            entry(out, &covered, &from, None);
            let action = self
                .pad_actions
                .iter()
                .find(|(b, _)| b == block)
                .map_or(0, |&(_, a)| a);
            entry(out, &from, &to, Some((pad, action)));
            covered = to;
        }
        entry(out, &covered, &l("end"), None);
        out.label(l("cst_end"));
        out.bytes(&self.actions.iter().map(|&b| b as u8).collect::<Vec<_>>());
        if !self.types.is_empty() {
            out.align(2);
            for (i, name) in self.type_names.iter().enumerate().rev() {
                if name.is_empty() {
                    out.number(4, 0);
                } else {
                    let here = l(&format!("tt{i}"));
                    out.label(here.clone());
                    out.directive(Directive::Integer {
                        width: 4,
                        expression: Expression::difference(format!("_{name}"), here),
                        got: true,
                    });
                }
            }
            out.label(l("ttbase"));
        }
        out.section("__TEXT", "__text", SectionKind::Data);
    }
}

/// Rejects calls the C convention support cannot make yet. `external`
/// callees may be C code relying on the full ABI.
/// `what` describes the call, only when reporting an error.
fn check_call_signature(
    what: impl Fn() -> String,
    sig: &Signature,
    external: bool,
) -> Result<(), String> {
    let (CallConv::C | CallConv::Fast) = sig.cc;
    if sig.returns.len() > 1 {
        return Err(format!(
            "{}: multiple return values are not supported by the C convention",
            what()
        ));
    }
    // The Apple ABI has the caller extend narrow arguments to 32 bits by C
    // signedness. Keel functions do not rely on it, but external ones may,
    // so their signatures must say which.
    if external
        && sig.cc == CallConv::C
        && sig
            .params
            .iter()
            .any(|p| matches!(p.ty, Type::I8 | Type::I16) && p.ext == ArgExt::None)
    {
        return Err(format!(
            "{}: i8/i16 arguments to external code need `sext` or `uext`",
            what()
        ));
    }
    Ok(())
}

/// The convention's view of a function's own parameters.
fn param_kinds(params: &[AbiParam]) -> Vec<ArgKind<'_>> {
    params
        .iter()
        .map(|p| ArgKind::of(p.ty, &p.purpose))
        .collect()
}

/// The convention's view of a call's arguments: the parameters `params`,
/// then any variadic arguments by their types.
fn arg_kinds<'s>(func: &Function, params: &'s [AbiParam], args: &[Value]) -> Vec<ArgKind<'s>> {
    args.iter()
        .enumerate()
        .map(|(i, &arg)| match params.get(i) {
            Some(p) => ArgKind::of(p.ty, &p.purpose),
            None => ArgKind::Value(func.value_type(func.resolve(arg))),
        })
        .collect()
}

/// Picks a register for each operand of `asm`, outputs then inputs, from
/// [`ASM_REGS`] and [`ASM_FREGS`]: the named ones, then the first free one
/// for each other operand, so no two operands share a register unless
/// tied (or an input names an output's register: it is read before the
/// output is written). A constant gets none.
fn asm_regs(asm: &InlineAsm) -> Result<Vec<Option<PhysReg>>, String> {
    let n_out = asm.outputs.len();
    let operands: Vec<AsmOperand> = (asm.outputs.iter().map(|&(op, _)| op))
        .chain(asm.inputs.iter().copied())
        .collect();
    let clobbered: Vec<PhysReg> = (asm.clobbers.iter())
        .filter_map(|c| match *c {
            AsmClobber::Reg(r) => Some(r),
            _ => None,
        })
        .collect();
    let mut taken = clobbered.clone();
    let mut regs = vec![None; operands.len()];
    for (i, &op) in operands.iter().enumerate() {
        let AsmOperand::Fixed(r) = op else { continue };
        let pool: &[u8] = if r.float { &ASM_FREGS } else { &ASM_REGS };
        if !pool.contains(&r.num) {
            return Err(format!(
                "operand {i} names {r}, but asm operands can only be in x0–x8, x16, x17, v0–v7 and v16–v18"
            ));
        }
        if clobbered.contains(&r) {
            return Err(format!("{r} is both an operand and clobbered"));
        }
        if i < n_out && regs[..i].contains(&Some(r)) {
            return Err(format!("{r} is named by two outputs"));
        }
        regs[i] = Some(r);
        taken.push(r);
    }
    for (i, &op) in operands.iter().enumerate() {
        let float = match op {
            AsmOperand::Reg | AsmOperand::Mem => false,
            AsmOperand::FloatReg => true,
            AsmOperand::Tied(out) => {
                regs[i] = regs[out as usize];
                continue;
            }
            AsmOperand::Fixed(_) | AsmOperand::Imm => continue,
        };
        let pool: &[u8] = if float { &ASM_FREGS } else { &ASM_REGS };
        let r = (pool.iter())
            .map(|&num| PhysReg { float, num })
            .find(|r| !taken.contains(r))
            .ok_or("too many asm operands for the free registers")?;
        regs[i] = Some(r);
        taken.push(r);
    }
    // Two inputs in one register would need two values there at once.
    for (i, r) in regs.iter().enumerate().skip(n_out) {
        if r.is_some() && regs[n_out..i].contains(r) {
            return Err(format!("{} is named by two inputs", r.unwrap()));
        }
    }
    Ok(regs)
}

fn asm_loc(r: PhysReg) -> Loc {
    if r.float {
        Loc::FReg(r.num)
    } else {
        Loc::Reg(r.num)
    }
}

/// `asm`'s template with its operands substituted. `%N` is operand N:
/// a register named for its type (`x0` or `w0`, `d0` or `s0`), `[x0]` for
/// memory, or the number for a constant. A modifier picks another view of
/// a register (`%w0`, `%x0`; `%s0`, `%d0`, `%q0`, `%h0`, `%b0`), and `%c0`
/// is a constant; `%%` is `%`.
fn asm_text(
    func: &Function,
    asm: &InlineAsm,
    args: &[Value],
    regs: &[Option<PhysReg>],
) -> Result<String, String> {
    let n_out = asm.outputs.len();
    let mut out = String::new();
    let mut chars = asm.template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.next_if_eq(&'%').is_some() {
            out.push('%');
            continue;
        }
        let modifier = chars.next_if(|c| c.is_ascii_alphabetic());
        let mut digits = String::new();
        while let Some(d) = chars.next_if(char::is_ascii_digit) {
            digits.push(d);
        }
        let n: usize = digits
            .parse()
            .map_err(|_| "`%` must be followed by an operand number, or be `%%`".to_string())?;
        if n >= regs.len() {
            return Err(format!(
                "`%{n}`: the asm has only {} operand(s)",
                regs.len()
            ));
        }
        let (op, ty) = if n < n_out {
            asm.outputs[n]
        } else {
            let arg = func.resolve(args[n - n_out]);
            (asm.inputs[n - n_out], func.value_type(arg))
        };
        let bad = || {
            let shown = modifier.map_or(String::new(), String::from);
            format!("`%{shown}{n}` does not fit operand {n} (`{op}`)")
        };
        match (op, regs[n], modifier) {
            (AsmOperand::Imm, _, None | Some('c')) => {
                let ValueDef::Result(def, _) = func.value_def(func.resolve(args[n - n_out])) else {
                    unreachable!("the verifier checks constants");
                };
                let InstData::Iconst { imm, .. } = func.inst_data(def) else {
                    unreachable!("the verifier checks constants");
                };
                write!(out, "{imm}").unwrap();
            }
            (AsmOperand::Mem, Some(r), None) => write!(out, "[x{}]", r.num).unwrap(),
            (AsmOperand::Imm | AsmOperand::Mem, _, _) => return Err(bad()),
            (_, Some(r), None) if r.float => write!(out, "{}", freg(r.num, ty)).unwrap(),
            (_, Some(r), None) => write!(out, "{}", reg(r.num, ty)).unwrap(),
            (_, Some(r), Some(m @ ('s' | 'd' | 'q' | 'h' | 'b'))) if r.float => {
                write!(out, "{m}{}", r.num).unwrap()
            }
            (_, Some(r), Some(m @ ('w' | 'x'))) if !r.float => write!(out, "{m}{}", r.num).unwrap(),
            _ => return Err(bad()),
        }
    }
    Ok(out)
}

/// The register number of an argument register.
/// Load and store mnemonics for a piece of `size` bytes.
fn piece_ops(size: u64) -> (&'static str, &'static str) {
    match size {
        1 => ("ldrb", "strb"),
        2 => ("ldrh", "strh"),
        _ => ("ldr", "str"),
    }
}

/// Where a single return value of type `ty` goes: x0 or v0.
fn return_loc(ty: Type) -> Loc {
    if ty.is_float() || ty.is_vector() {
        Loc::FReg(0)
    } else {
        Loc::Reg(0)
    }
}

/// `w<n>` for types up to 32 bits, `x<n>` otherwise.
fn reg(n: u8, ty: Type) -> Reg {
    match ty.bytes() {
        Some(bytes) if bytes <= 4 => Reg::w(n),
        _ => Reg::x(n),
    }
}

/// `s<n>` for f32, `d<n>` for f64, `q<n>` for vectors.
fn freg(n: u8, ty: Type) -> Reg {
    match ty {
        Type::Vector(_) => Reg::q(n),
        Type::F32 => Reg::s(n),
        _ => Reg::d(n),
    }
}

/// The suffix sizing a load, store or atomic to `ty`: `b`, `h` or none.
fn width_suffix(ty: Type) -> &'static str {
    match ty {
        Type::I8 => "b",
        Type::I16 => "h",
        _ => "",
    }
}

/// The suffix giving an LSE atomic its ordering: `a`cquire and/or
/// re`l`ease.
fn ordering_suffix(ordering: Ordering) -> &'static str {
    match ordering {
        Ordering::Relaxed => "",
        Ordering::Acquire => "a",
        Ordering::Release => "l",
        Ordering::AcqRel | Ordering::SeqCst => "al",
    }
}

fn narrow_bits(ty: Type) -> Option<u32> {
    match ty {
        Type::I8 => Some(8),
        Type::I16 => Some(16),
        _ => None,
    }
}

/// The most negative value of `ty`, sign-extended to the register width.
fn min_value(ty: Type) -> u64 {
    match ty {
        Type::I8 => i8::MIN as i32 as u32 as u64,
        Type::I16 => i16::MIN as i32 as u32 as u64,
        Type::I32 => i32::MIN as u32 as u64,
        _ => i64::MIN as u64,
    }
}

fn cond_code(cond: IntCC) -> &'static str {
    match cond {
        IntCC::Eq => "eq",
        IntCC::Ne => "ne",
        IntCC::Slt => "lt",
        IntCC::Sle => "le",
        IntCC::Sgt => "gt",
        IntCC::Sge => "ge",
        IntCC::Ult => "lo",
        IntCC::Ule => "ls",
        IntCC::Ugt => "hi",
        IntCC::Uge => "hs",
    }
}

/// Condition codes after `fcmp`, which sets N for less, Z for equal, C for
/// greater or equal *or unordered*, and V only for unordered. `lt` would also
/// be true when unordered, so less-than tests N (`mi`) and less-or-equal
/// tests "C clear or Z set" (`ls`).
fn float_cond_code(cond: FloatCC) -> &'static str {
    match cond {
        FloatCC::Eq => "eq",
        FloatCC::Ne => "ne",
        FloatCC::Lt => "mi",
        FloatCC::Le => "ls",
        FloatCC::Gt => "gt",
        FloatCC::Ge => "ge",
        FloatCC::Ord => "vc",
        FloatCC::Uno => "vs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wraps to `width` bits, signed.
    fn wrap(width: u32, x: i128) -> i128 {
        let unused = 128 - width;
        x << unused >> unused
    }

    /// The emitted signed sequence in `width`-bit arithmetic.
    fn signed_quotient(width: u32, n: i128, d: i64) -> i128 {
        let ad = d.unsigned_abs();
        if ad.is_power_of_two() {
            let k = ad.trailing_zeros();
            let mask = (1i128 << width) - 1;
            // asr by width - 1, then lsr by width - k, then add.
            let sign = (n >> (width - 1)) & mask;
            let t = wrap(width, n + (sign >> (width - k)));
            let q = t >> k;
            return if d > 0 { q } else { wrap(width, -q) };
        }
        let (m, shift) = signed_reciprocal(width, d);
        let m = wrap(width, i128::from(m));
        let mut hi = (n * m) >> width;
        if d > 0 && m < 0 {
            hi = wrap(width, hi + n);
        } else if d < 0 && m > 0 {
            hi = wrap(width, hi - n);
        }
        let q = hi >> shift;
        q + i128::from(q < 0)
    }

    /// The emitted unsigned sequence, both the 64-bit averaging and the
    /// 32-bit wide-sum forms.
    fn unsigned_quotient(width: u32, n: u128, d: u64) -> u128 {
        let r = unsigned_reciprocal(width, d);
        let hi = (n * u128::from(r.multiplier)) >> width;
        if !r.add {
            return hi >> r.shift;
        }
        let averaged = (hi + ((n - hi) >> 1)) >> r.shift;
        assert_eq!(averaged, (hi + n) >> (r.shift + 1));
        averaged
    }

    fn signed_divisors(width: u32) -> impl Iterator<Item = i64> {
        let half = 1i64 << (width - 1);
        (-half..half).filter(|d| !matches!(d, -1..=1))
    }

    #[test]
    fn signed_reciprocals_are_exact_for_small_widths() {
        for width in 3..=10 {
            let half = 1i128 << (width - 1);
            for d in signed_divisors(width) {
                for n in -half..half {
                    assert_eq!(
                        signed_quotient(width, n, d),
                        n / i128::from(d),
                        "w={width} n={n} d={d}"
                    );
                }
            }
        }
    }

    #[test]
    fn unsigned_reciprocals_are_exact_for_small_widths() {
        for width in 3..=10 {
            for d in 3..1u64 << width {
                if d.is_power_of_two() {
                    continue;
                }
                for n in 0..1u128 << width {
                    assert_eq!(unsigned_quotient(width, n, d), n / u128::from(d));
                }
            }
        }
    }

    /// i8 and i16 divide as i32 on extended values: every divisor, with all
    /// i8 dividends and a spread of i16 ones.
    #[test]
    fn narrow_divisions_through_32_bit_reciprocals() {
        for width in [8u32, 16] {
            let half = 1i64 << (width - 1);
            let dividends: Vec<i128> = if width == 8 {
                (-half..half).map(i128::from).collect()
            } else {
                (-half..half)
                    .step_by(97)
                    .chain([-half, -half + 1, -1, 0, 1, half - 1])
                    .map(i128::from)
                    .collect()
            };
            for d in signed_divisors(width) {
                for &n in &dividends {
                    assert_eq!(signed_quotient(32, n, d), n / i128::from(d), "n={n} d={d}");
                    let (un, ud) = (
                        (n as u128) & ((1 << width) - 1),
                        (d as u64) & ((1 << width) - 1),
                    );
                    if ud > 2 && !ud.is_power_of_two() {
                        assert_eq!(unsigned_quotient(32, un, ud), un / u128::from(ud));
                    }
                }
            }
        }
    }

    #[test]
    fn wide_reciprocals_at_quotient_boundaries() {
        let mut random = 0x9e37_79b9_7f4a_7c15u64;
        for width in [32u32, 64] {
            let half = 1i128 << (width - 1);
            let mask = (1u128 << width) - 1;
            for i in 0..20_000u64 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let raw = match i {
                    0..=200 => i as i128 - 100,
                    201 => half - 1,
                    202 => -half,
                    203 => -half + 1,
                    204 => half / 3,
                    _ => wrap(width, i128::from(random >> (i % 61))),
                };
                let d = wrap(width, raw);
                if (-1..=1).contains(&d) {
                    continue;
                }
                let d = d as i64;
                let ad = i128::from(d.unsigned_abs());
                let boundary = (half - 1) / ad * ad;
                let r = wrap(width, i128::from(random.rotate_left(17)));
                for n in [
                    -half,
                    -half + 1,
                    -boundary,
                    -boundary - 1,
                    -1,
                    0,
                    1,
                    ad - 1,
                    ad,
                    boundary - 1,
                    boundary,
                    half - 1,
                    r,
                ] {
                    let n = wrap(width, n);
                    assert_eq!(
                        signed_quotient(width, n, d),
                        n / i128::from(d),
                        "w={width} n={n} d={d}"
                    );
                }
                let ud = (d as u64) & mask as u64;
                if ud > 2 && !ud.is_power_of_two() {
                    let ubound = mask / u128::from(ud) * u128::from(ud);
                    for n in [
                        0,
                        1,
                        u128::from(ud) - 1,
                        u128::from(ud),
                        ubound - 1,
                        ubound,
                        mask,
                        r as u128 & mask,
                    ] {
                        assert_eq!(
                            unsigned_quotient(width, n, ud),
                            n / u128::from(ud),
                            "w={width} n={n} d={ud}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn known_reciprocals() {
        assert_eq!(signed_reciprocal(32, 3), (0x5555_5556, 0));
        assert_eq!(signed_reciprocal(32, -5), (0x9999_9999, 1));
        assert_eq!(signed_reciprocal(32, 7), (0x9249_2493, 2));
        assert_eq!(signed_reciprocal(64, 3), (0x5555_5555_5555_5556, 0));
    }
}

#[cfg(test)]
mod parallel_copy_tests;
