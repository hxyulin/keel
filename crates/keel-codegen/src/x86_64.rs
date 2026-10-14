//! IR → x86-64 Linux (System V, ELF) assembly text in Intel syntax, with
//! values in registers chosen by the linear-scan allocator in
//! [`crate::regalloc`].
//!
//! The output is for the GNU assembler (`.intel_syntax noprefix`):
//! destination first, as in the Intel manuals and keel's arm64 output.
//! The baseline is x86-64-v2 (SSE4.1 `roundsd`, `popcnt`), which every
//! x86-64 CPU since 2009 has.
//!
//! # Registers
//!
//! ```text
//! rax, rcx, rdx   scratch: S0, S1, S2. Also the fixed registers of the
//!                 instructions that need them: rdx:rax for mul and div,
//!                 cl for shift counts, rax for results, al for variadic
//!                 calls, rcx and rdx as the 4th and 3rd arguments
//! rbx, r12–r15    allocatable, callee-saved
//! rsi, rdi, r8, r9  allocatable, caller-saved; also arguments
//! r10             the target of indirect calls, never allocated
//! r11             scratch: breaks cycles in parallel copies
//! rbp, rsp        frame pointer and stack pointer
//!
//! xmm0–xmm12      allocatable, caller-saved (SysV has no callee-saved
//!                 vector registers); xmm0–xmm7 are also arguments
//! xmm13–xmm15     scratch
//! ```
//!
//! With only 16 general registers, keel's arm64 rule of never allocating
//! argument registers would leave five. Here four of them are allocatable
//! and a call's arguments move into place as one parallel copy, which
//! handles a value that is itself in an argument register (`f(b, a)` with
//! `a` in rdi and `b` in rsi is a swap). A value live across a call is
//! never in one, since they are caller-saved.
//!
//! **Two-address instructions.** `add rdi, rsi` overwrites its first
//! operand, so `d = a + b` is `mov d, a; add d, b`. A result may share a
//! register with an operand that dies at the same instruction (see the
//! regalloc docs on program points): if `d` is `b`'s register, the copy
//! would overwrite `b` before it is read, so the operation runs in rax and
//! the result moves to `d` after. **A lowering must read every operand
//! before it writes the destination.**
//!
//! Values narrower than 64 bits have unspecified upper bits. Arithmetic on
//! them uses the 32-bit registers (`add eax, ecx`), whose low bits are
//! right whatever is above; compares and stores use the exact width
//! (`cmp dil, sil`); division and right shifts extend first.
//!
//! # Frame
//!
//! ```text
//! rbp + 16          incoming stack arguments, in the caller's frame
//! rbp + 8           return address
//! rbp               saved rbp
//! rbp - 8 * k       the callee-saved registers this function uses
//! ...               IR stack slots, spill slots
//! rsp               outgoing stack arguments, for the call needing most
//! ```
//!
//! The fixed frame is addressed from rbp, so it stays reachable when `rsp`
//! moves for a dynamic allocation; only the outgoing arguments are at
//! `rsp`, where calls need them. `rsp` is 16-byte aligned at every call.
//!
//! # Symbols
//!
//! Linux executables are position independent (PIE) by default: code may
//! not contain absolute addresses. A symbol defined in this module is
//! `rip`-relative (`lea rax, [rip + counter]`); one defined elsewhere (in
//! libc) comes from the global offset table (`[rip + stdout@GOTPCREL]`),
//! which the loader fills in, and calls to it go through the procedure
//! linkage table (`call puts@PLT`).

use crate::dwarf::{self, FrameDebug};
use crate::fold::{self, iconst};
use crate::parallel_copy::sequentialize;
use crate::regalloc::{self, Allocation, Loc, Pools};
use crate::sysv::{self, ArgKind, ArgLoc, RAX, RCX, RDI, RDX, Slot};
use crate::{CodegenError, Format, data, loc_reg, pieces};
use keel_ir::{
    AbiParam, ArgExt, BinaryOp, Block, BlockCall, CallConv, ConvertOp, DebugScope, EntityRef,
    FloatCC, Function, Inst, InstData, IntCC, Intrinsic, Layout, Module, Ordering, OverflowOp,
    Purpose, RmwOp, SourceFile, SourceLoc, Type, UnaryOp, Value, ValueDef,
};
use std::collections::HashSet;
use std::fmt::Write;

/// The registers values are allocated to.
pub const REGISTERS: Pools = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13, 14, 15],
    float_caller_saved: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    float_callee_saved: &[],
};

/// Scratch registers, never allocated. Spilled operands are reloaded into
/// them, a spilled result is computed in `S0`.
const S0: u8 = RAX;
const S1: u8 = RCX;
const S2: u8 = RDX;
/// Breaks cycles in parallel copies.
const S3: u8 = 11;
/// Holds the target of an indirect call while the arguments move.
const R10: u8 = 10;

/// DWARF's number for rbp, the frame base debuggers find variables from.
const DWARF_RBP: u8 = 6;

/// Vector scratch registers.
const FS0: u8 = 13;
const FS1: u8 = 14;
const FS2: u8 = 15;

/// Compiles a module into one assembly file for `as`/`gcc`. Declared
/// functions are left as undefined symbols for the linker to resolve.
pub fn compile(module: &Module) -> Result<String, CodegenError> {
    compile_with(module, &REGISTERS)
}

/// Like [`compile`], allocating only from `pools`, which must be drawn from
/// [`REGISTERS`]. Tests use a tiny set to force spills.
pub fn compile_with(module: &Module, pools: &Pools) -> Result<String, CodegenError> {
    let subset = |pool: &[u8], of: &[u8]| pool.iter().all(|r| of.contains(r));
    assert!(
        subset(pools.caller_saved, REGISTERS.caller_saved)
            && subset(pools.callee_saved, REGISTERS.callee_saved)
            && subset(pools.float_caller_saved, REGISTERS.float_caller_saved)
            && subset(pools.float_callee_saved, REGISTERS.float_callee_saved),
        "register pools must be subsets of REGISTERS"
    );
    let mut out = String::from("\t.intel_syntax noprefix\n");
    let debug = !module.files.is_empty() && !module.functions.is_empty();
    if debug {
        for (i, path) in module.files.iter().enumerate() {
            writeln!(out, "\t.file\t{} {}", i + 1, data::asm_string(path)).unwrap();
        }
    }
    let mut frames = Vec::new();
    for func in &module.functions {
        let mut compiler = FunctionCompiler::new(func, module, pools)?;
        compiler.debug = debug;
        let slots = compiler.slot_frame_offsets();
        let scope_marks = compiler.compile(&mut out);
        frames.push(FrameDebug { slots, scope_marks });
    }
    for data in &module.data {
        if let Some(init) = &data.init {
            data::emit_data(&mut out, Format::Elf, data, init);
        }
    }
    if debug {
        dwarf::emit_debug_info(&mut out, Format::Elf, DWARF_RBP, module, &frames);
    }
    // No executable stack.
    out.push_str("\t.section\t.note.GNU-stack,\"\",@progbits\n");
    Ok(out)
}

/// What this backend does not handle yet, if `inst` needs it.
fn unsupported(func: &Function, inst: Inst) -> Option<&'static str> {
    let data = func.inst_data(inst);
    let wide = |v: Value| func.value_type(func.resolve(v)) == Type::I128;
    Some(match data {
        InstData::Binary {
            op: BinaryOp::Umulhi | BinaryOp::Smulhi,
            args,
        } if wide(args[0]) => "i128 umulhi/smulhi",
        InstData::Overflow {
            op: OverflowOp::Umul | OverflowOp::Smul,
            args,
        } if wide(args[0]) => "i128 overflow multiplication",
        InstData::Convert { ty, arg, .. } if (*ty == Type::I128) != wide(*arg) => match data {
            InstData::Convert {
                op: ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc,
                ..
            } => return None,
            _ => "conversions between i128 and floats",
        },
        InstData::Switch { arg, .. } if wide(*arg) => "switch on i128",
        InstData::Invoke { .. } | InstData::LandingPad { .. } | InstData::EhTypeid { .. } => {
            "exceptions"
        }
        InstData::InlineAsm { .. } => "inline assembly",
        _ => return None,
    })
}

/// Instructions that clobber the caller-saved registers: calls, and the
/// operations that are calls to libc or libgcc (`i128` division).
fn is_call(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Binary { op, args } => {
            op.can_trap() && func.value_type(func.resolve(args[0])) == Type::I128
        }
        InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Fma { .. } => true,
        InstData::Intrinsic { id, .. } => {
            matches!(
                id,
                Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset
            )
        }
        _ => false,
    }
}

/// Calls that may return a second time, from a `longjmp`.
fn returns_twice(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { func: callee, .. } => func.callee(*callee).sig.returns_twice,
        InstData::CallIndirect { sig, .. } => func.signature(*sig).returns_twice,
        _ => false,
    }
}

/// The constant operand of `data` encoded in the instruction itself, and
/// the immediate to write. x86 immediates are 32 bits, sign-extended to 64
/// for 64-bit operations; a narrower operation only uses the low bits.
fn folded_imm(func: &Function, data: &InstData) -> Option<(Value, i64)> {
    let ty_of = |v: Value| func.value_type(func.resolve(v));
    // The immediate for an operation of type `ty`, if `v` is one.
    let imm = |v: Value, ty: Type| {
        let k = iconst(func, v)?;
        match ty {
            Type::I64 | Type::Ptr => i32::try_from(k).is_ok().then_some(k),
            Type::I128 => None,
            _ => Some(k & mask(ty) as i64),
        }
    };
    match *data {
        InstData::Binary { op, args } => {
            let ty = ty_of(args[0]);
            match op {
                BinaryOp::Iadd
                | BinaryOp::Ptradd
                | BinaryOp::Isub
                | BinaryOp::Imul
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Xor => imm(args[1], ty).map(|k| (args[1], k)),
                BinaryOp::Shl
                | BinaryOp::Ushr
                | BinaryOp::Sshr
                | BinaryOp::Rotl
                | BinaryOp::Rotr
                    if ty != Type::I128 =>
                {
                    let bits = 8 * i64::from(ty.bytes()?);
                    iconst(func, args[1]).map(|k| (args[1], k & (bits - 1)))
                }
                _ => None,
            }
        }
        InstData::Icmp { args, .. } => imm(args[1], ty_of(args[0])).map(|k| (args[1], k)),
        InstData::Store { value, .. } if !ty_of(value).is_float() => {
            imm(value, ty_of(value)).map(|k| (value, k))
        }
        _ => None,
    }
}

/// The operands of `data` folded into it: the stack address of a load or
/// store, which addresses the slot as `[rbp - n]`, and a [`folded_imm`].
fn folds(func: &Function, data: &InstData) -> Vec<Value> {
    let wide = |v: Value| func.value_type(func.resolve(v)) == Type::I128;
    let addr = match *data {
        InstData::Load { ty, addr } if ty != Type::I128 => Some(addr),
        InstData::Store { value, addr } if !wide(value) => Some(addr),
        _ => None,
    }
    .filter(|&addr| stack_slot(func, addr).is_some());
    addr.into_iter()
        .chain(folded_imm(func, data).map(|(v, _)| v))
        .collect()
}

/// The stack slot whose address `addr` is, if a `stack_addr` defines it.
fn stack_slot(func: &Function, addr: Value) -> Option<usize> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(addr)) else {
        return None;
    };
    match *func.inst_data(inst) {
        InstData::StackAddr { slot } => Some(slot.index()),
        _ => None,
    }
}

struct FunctionCompiler<'a> {
    func: &'a Function,
    module: &'a Module,
    alloc: Allocation,
    /// Offset from `rsp` of each IR stack slot.
    stack_slot_offsets: Vec<usize>,
    /// `stack_addr` and `iconst` results folded into every use, which
    /// need no code.
    folded_values: HashSet<Value>,
    fused_compares: HashSet<Value>,
    /// Bytes at the bottom of the frame for outgoing stack arguments,
    /// rounded to 16; memory from `stack_alloc` starts just above.
    outgoing: usize,
    /// Offset from `rsp` of spill slot 0. Slots past the allocator's are
    /// staging for struct arguments in registers; see [`Self::move_args`].
    spill_base: usize,
    /// Offset from `rsp` of an 8-byte slot keeping a call's `sret` address
    /// across the call, for results that come back in registers.
    sret_save: Option<usize>,
    /// Offset from `rsp` of the slot keeping this function's own `sret`
    /// address, which it returns in rax.
    sret_ptr: Option<usize>,
    /// Offset from `rsp` of the memory behind each `byval` parameter that
    /// arrived in registers, and behind an `sret` result returned in them.
    param_areas: Vec<Option<usize>>,
    /// Offset from `rsp` of a variadic function's register save area:
    /// rdi–r9 (48 bytes), then xmm0–xmm7 (16 bytes each).
    reg_save: Option<usize>,
    /// The named parameters' registers and stack, where `va_start` begins.
    named: sysv::Placement,
    /// Outgoing arguments, spill slots and IR stack slots: what `sub rsp`
    /// allocates below the saved registers.
    frame_size: usize,
    /// The callee-saved registers pushed, in order.
    saved: Vec<u8>,
    /// The block emitted after the current one, which a branch can fall
    /// through to.
    next_block: Option<Block>,
    out: String,
    uses_trap: bool,
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
    /// The offset of each stack slot from rbp, where debuggers find the
    /// variables in them: rbp stays put while `rsp` may move.
    fn slot_frame_offsets(&self) -> Vec<i64> {
        let top = (self.frame_size + 8 * self.saved.len()) as i64;
        self.stack_slot_offsets
            .iter()
            .map(|&offset| offset as i64 - top)
            .collect()
    }

    fn new(func: &'a Function, module: &'a Module, pools: &Pools) -> Result<Self, CodegenError> {
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        let CallConv::C = func.sig.cc;
        let not_yet = |what: &str| err(format!("{what} not supported on x86-64 yet"));
        match func.sig.returns.as_slice() {
            [] => {}
            // clang assumes the callee extends these to 32 bits.
            [
                AbiParam {
                    ty: Type::I8 | Type::I16,
                    ext: ArgExt::None,
                    ..
                },
            ] => {
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
        if func.personality.is_some() {
            return Err(not_yet("exceptions are"));
        }
        for (_, inst) in func.layout_insts() {
            if let Some(what) = unsupported(func, inst) {
                return Err(err(format!(
                    "`{}`: {what} not supported on x86-64 yet",
                    func.display_inst(inst)
                )));
            }
        }
        let named = sysv::arg_locs(&param_kinds(&func.sig.params));
        // A tail call jumps to the callee with our caller's return address
        // on the stack, so its stack arguments go where ours came in:
        // they must fit there. Structs are not supported.
        let byval_on_stack = named.locs.iter().any(|l| matches!(l, ArgLoc::OnStack(_)));
        for (_, inst) in func.layout_insts() {
            let (args, sig) = match func.inst_data(inst) {
                InstData::ReturnCall { func: callee, args } => (args, &func.callee(*callee).sig),
                InstData::ReturnCallIndirect { sig, args, .. } => (args, func.signature(*sig)),
                _ => continue,
            };
            let placement = sysv::arg_locs(&arg_kinds(func, &sig.params, args));
            let scalars = placement.locs.iter().all(|l| matches!(l, ArgLoc::Value(_)));
            let has_sret = func
                .sig
                .params
                .iter()
                .any(|p| matches!(p.purpose, Purpose::Sret(_)));
            if !scalars || has_sret || byval_on_stack || placement.stack > named.stack {
                return Err(err(format!(
                    "`{}` needs more stack arguments than this function received, or \
                     involves structs, which tail calls do not support",
                    func.display_inst(inst)
                )));
            }
        }

        // The bottom of the frame holds what calls need: their stack
        // arguments, staging for struct arguments in registers, and a slot
        // for an `sret` address.
        let (mut outgoing, mut staging, mut needs_sret_save) = (0, 0, false);
        for (_, inst) in func.layout_insts() {
            let (args, params) = match func.inst_data(inst) {
                InstData::Call { func: callee, args } => (args, &func.callee(*callee).sig.params),
                InstData::CallIndirect { sig, args, .. } => (args, &func.signature(*sig).params),
                _ => continue,
            };
            let placement = sysv::arg_locs(&arg_kinds(func, params, args));
            outgoing = outgoing.max(placement.stack);
            let mut halves = 0;
            for loc in &placement.locs {
                match loc {
                    ArgLoc::InRegs(regs) => halves += regs.len(),
                    ArgLoc::SretRegs(_) => needs_sret_save = true,
                    _ => {}
                }
            }
            staging = staging.max(halves);
        }
        let outgoing = (outgoing as usize).next_multiple_of(16);
        let mut offset = outgoing;
        let slot8 = |offset: &mut usize| {
            *offset += 8;
            *offset - 8
        };
        let sret_save = needs_sret_save.then(|| slot8(&mut offset));
        let sret_ptr = named
            .locs
            .contains(&ArgLoc::SretMemory)
            .then(|| slot8(&mut offset));
        let mut param_areas = Vec::new();
        for (loc, param) in named.locs.iter().zip(&func.sig.params) {
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
        // `movaps` stores need 16-byte alignment.
        let reg_save = func.sig.variadic.then(|| {
            offset = offset.next_multiple_of(16) + 176;
            offset - 176
        });

        let alloc = regalloc::allocate(
            func,
            pools,
            |data| is_call(func, data),
            |data| returns_twice(func, data),
        );
        let spill_base = offset.next_multiple_of(8);
        let mut offset = spill_base + 8 * (alloc.spill_slots as usize + staging);
        let mut stack_slot_offsets = Vec::new();
        for (_, slot) in func.stack_slots() {
            offset = offset.next_multiple_of(slot.align as usize);
            stack_slot_offsets.push(offset);
            offset += slot.size as usize;
        }
        // rsp is 16-byte aligned after `push rbp`; the pushes and the frame
        // together keep it so.
        let saved = alloc.callee_saved_used.clone();
        let frame_size = (offset + 8 * saved.len()).next_multiple_of(16) - 8 * saved.len();
        if frame_size > i32::MAX as usize / 2 {
            return Err(err(format!("frame of {frame_size} bytes is too large")));
        }
        Ok(Self {
            func,
            module,
            alloc,
            folded_values: fold::folded_values(func, |data| folds(func, data)),
            fused_compares: fold::fused_compares(func),
            stack_slot_offsets,
            outgoing,
            spill_base,
            sret_save,
            sret_ptr,
            param_areas,
            reg_save,
            named,
            frame_size,
            saved,
            next_block: None,
            out: String::new(),
            uses_trap: false,
            debug: false,
            srcloc: None,
            prologue_end: false,
            scope: None,
            scope_marks: Vec::new(),
        })
    }

    /// Appends the function's assembly to `out`; gives where its lexical
    /// scopes begin and end.
    fn compile(mut self, out: &mut String) -> Vec<(String, Option<DebugScope>)> {
        let func = self.func;
        let name = &func.name;
        out.push_str("\t.text\n");
        if !func.local {
            writeln!(out, "\t.globl\t{name}").unwrap();
        }
        writeln!(
            out,
            "\t.p2align\t4\n\t.type\t{name},@function\n{name}:\n.L{name}_begin:"
        )
        .unwrap();
        // The prologue takes the function's location, or else that of its
        // first located instruction, with column 0; the first located
        // instruction after it ends the prologue.
        if self.debug {
            let first = func.debug_loc.or_else(|| {
                func.layout_insts()
                    .find_map(|(_, inst)| func.inst_loc(inst))
            });
            self.set_srcloc(Some(SourceLoc {
                file: first.map_or(SourceFile::new(0), |loc| loc.file),
                line: first.map_or(0, |loc| loc.line),
                col: 0,
                scope: None,
            }));
            self.prologue_end = true;
        }
        self.emit(".cfi_startproc");
        self.emit("push\trbp");
        self.emit(".cfi_def_cfa_offset 16");
        self.emit(".cfi_offset rbp, -16");
        self.emit("mov\trbp, rsp");
        self.emit(".cfi_def_cfa_register rbp");
        // Each push is 8 bytes further from the CFA, which is rbp + 16.
        for (k, r) in self.saved.clone().into_iter().enumerate() {
            self.emit(format!("push\t{}", r64(r)));
            self.emit(format!(".cfi_offset {}, -{}", r64(r), 24 + 8 * k));
        }
        if self.frame_size > 0 {
            self.emit(format!("sub\trsp, {}", self.frame_size));
        }
        self.entry();

        // Only reachable blocks, in the order the allocator numbered them.
        let order = self.alloc.order.clone();
        for (i, &block) in order.iter().enumerate() {
            self.next_block = order.get(i + 1).copied();
            writeln!(self.out, "{}:", self.label(block)).unwrap();
            for &inst in func.block_insts(block) {
                writeln!(self.out, "\t# {}", func.display_inst(inst)).unwrap();
                if self.debug {
                    self.set_srcloc(func.inst_loc(inst));
                }
                self.inst(inst);
            }
        }
        if self.uses_trap {
            writeln!(self.out, ".L{name}_trap:").unwrap();
            self.emit("ud2");
        }
        writeln!(self.out, ".L{name}_end:").unwrap();
        self.emit(".cfi_endproc");
        writeln!(self.out, "\t.size\t{name}, .-{name}").unwrap();
        out.push_str(&self.out);
        self.scope_marks
    }

    /// Emits a `.loc` for the code that follows, unless it would repeat
    /// the last one. `None` keeps the last one. A change of lexical scope
    /// is marked with a label.
    fn set_srcloc(&mut self, loc: Option<SourceLoc>) {
        let Some(loc) = loc else { return };
        if loc.scope != self.scope {
            let label = format!(".L{}_ds{}", self.func.name, self.scope_marks.len());
            writeln!(self.out, "{label}:").unwrap();
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
        let flag = if self.prologue_end {
            " prologue_end"
        } else {
            ""
        };
        self.emit(format!(".loc\t{} {line} {col}{flag}", file.index() + 1));
        self.srcloc = Some(loc);
        self.prologue_end = false;
    }

    /// Moves the parameters to their places. Arguments arrive in
    /// registers, some of which are allocatable, so they move as one
    /// parallel copy; before it, nothing may write an argument register,
    /// so what is read from them directly comes first:
    ///
    /// - a variadic function saves every argument register to its save
    ///   area, where `va_arg` finds them (the vector ones only if `al`,
    ///   the count the caller passes, is non-zero);
    /// - an `sret` address in rdi is kept, to return in rax;
    /// - a struct in registers is stored to memory, and its parameter is
    ///   that memory's address.
    ///
    /// Stack parameters are in the caller's frame, above the return
    /// address, and load after the copy: one may be allocated to an
    /// argument register.
    fn entry(&mut self) {
        let func = self.func;
        let Some(entry) = func.entry_block() else {
            return;
        };
        if let Some(area) = self.reg_save {
            for (k, &r) in sysv::INT_ARGS.iter().enumerate() {
                let at = self.frame(area + 8 * k);
                self.emit(format!("mov\tqword ptr {at}, {}", r64(r)));
            }
            let skip = format!(".L{}_saved", func.name);
            self.emit("test\tal, al");
            self.emit(format!("je\t{skip}"));
            for k in 0..8 {
                let at = self.frame(area + 48 + 16 * k);
                self.emit(format!("movaps\txmmword ptr {at}, xmm{k}"));
            }
            writeln!(self.out, "{skip}:").unwrap();
        }
        if let Some(slot) = self.sret_ptr {
            let at = self.frame(slot);
            self.emit(format!("mov\tqword ptr {at}, rdi"));
        }
        let params = func.block_params(entry).to_vec();
        let locs = self.named.locs.clone();
        // What happens after the copy: a stack parameter loads, a struct's
        // parameter gets the address of its bytes.
        enum Later {
            Load(Loc, Type, String),
            Address(Loc, String),
        }
        let (mut moves, mut later) = (Vec::new(), Vec::new());
        for (i, (param, loc)) in params.into_iter().zip(locs).enumerate() {
            let Some(dst) = self.alloc.loc(param) else {
                continue;
            };
            let abi_param = &func.sig.params[i];
            match loc {
                ArgLoc::Value(Slot::Regs(regs)) => {
                    moves.push((dst, regs[0]));
                    if let Some(hi) = self.alloc.hi_loc(param) {
                        moves.push((hi, regs[1]));
                    }
                }
                ArgLoc::Value(Slot::Stack(offset)) => {
                    let at = |o: u32| format!("[rbp + {}]", 16 + offset + o);
                    match self.alloc.hi_loc(param) {
                        Some(hi) => {
                            later.push(Later::Load(dst, Type::I64, at(0)));
                            later.push(Later::Load(hi, Type::I64, at(8)));
                        }
                        None => later.push(Later::Load(dst, abi_param.ty, at(0))),
                    }
                }
                ArgLoc::SretMemory => moves.push((dst, Loc::Reg(RDI))),
                ArgLoc::InRegs(regs) => {
                    let area = self.param_areas[i].expect("planned");
                    for (k, r) in regs.into_iter().enumerate() {
                        let at = format!("qword ptr {}", self.frame(area + 8 * k));
                        match r {
                            Loc::Reg(r) => self.emit(format!("mov\t{at}, {}", r64(r))),
                            _ => self.emit(format!("movsd\t{at}, {}", xmm(loc_reg(r)))),
                        }
                    }
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::SretRegs(_) => {
                    let area = self.param_areas[i].expect("planned");
                    later.push(Later::Address(dst, self.frame(area)));
                }
                ArgLoc::OnStack(offset) => {
                    later.push(Later::Address(dst, format!("[rbp + {}]", 16 + offset)))
                }
            }
        }
        self.parallel_copy(&moves);
        for step in later {
            match step {
                Later::Load(dst, ty, at) => self.load_into(dst, ty, &at),
                Later::Address(dst, at) => self.address(dst, &at),
            }
        }
    }

    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
        if self.lower_i128(inst, func.inst_data(inst), result) {
            return;
        }
        match func.inst_data(inst).clone() {
            InstData::StackAddr { .. } | InstData::Iconst { .. }
                if result.is_some_and(|r| self.folded_values.contains(&r)) => {}
            InstData::Icmp { .. } | InstData::Fcmp { .. }
                if result.is_some_and(|r| self.fused_compares.contains(&r)) => {}
            InstData::Iconst { ty, imm } => {
                let d = self.def_reg(result);
                self.materialize(d, imm, ty);
                self.def_done(result, d);
            }
            InstData::Fconst { bits, .. } => {
                let d = self.def_freg(result);
                if bits == 0 {
                    self.emit(format!("xorps\t{0}, {0}", xmm(d)));
                } else {
                    self.emit(format!("movabs\trax, {bits:#x}"));
                    self.emit(format!("movq\t{}, rax", xmm(d)));
                }
                self.def_done(result, d);
            }
            InstData::Call { func: callee, args } => {
                let decl = func.callee(callee);
                let sret = self.move_args(&args, &decl.sig, None, false);
                let target = self.call_target(&decl.name);
                self.emit(format!("call\t{target}"));
                self.call_result(result, sret);
            }
            InstData::CallIndirect { sig, callee, args } => {
                let sret = self.move_args(&args, func.signature(sig), Some(callee), false);
                self.emit(format!("call\t{}", r64(R10)));
                self.call_result(result, sret);
            }
            // Arguments into registers, then this function's epilogue, then
            // a jump: the callee returns straight to our caller.
            InstData::ReturnCall { func: callee, args } => {
                let decl = func.callee(callee);
                self.move_args(&args, &decl.sig, None, true);
                self.epilogue();
                let target = self.call_target(&decl.name);
                self.emit(format!("jmp\t{target}"));
            }
            // The epilogue only restores callee-saved registers and rbp,
            // so the target stays in r10.
            InstData::ReturnCallIndirect { sig, callee, args } => {
                self.move_args(&args, func.signature(sig), Some(callee), true);
                self.epilogue();
                self.emit(format!("jmp\t{}", r64(R10)));
            }
            InstData::SymbolAddr { symbol } => {
                let d = self.def_reg(result);
                let name = func.symbol_name(symbol);
                let thread_local = self
                    .module
                    .data_object(name)
                    .is_some_and(|d| d.thread_local);
                // A thread's copies of the thread-locals are at fixed
                // offsets from its thread pointer, the base of fs, whose
                // first word points to itself. An executable knows the
                // offsets of its own at link time (local-exec); others come
                // from the GOT (initial-exec).
                if thread_local && self.module.defines(name) {
                    self.emit(format!("mov\t{}, qword ptr fs:0", r64(d)));
                    self.emit(format!("lea\t{0}, [{0} + {name}@tpoff]", r64(d)));
                } else if thread_local {
                    self.emit(format!(
                        "mov\t{}, qword ptr [rip + {name}@gottpoff]",
                        r64(d)
                    ));
                    self.emit(format!("add\t{}, qword ptr fs:0", r64(d)));
                } else if self.module.defines(name) {
                    self.emit(format!("lea\t{}, [rip + {name}]", r64(d)));
                } else {
                    self.emit(format!(
                        "mov\t{}, qword ptr [rip + {name}@GOTPCREL]",
                        r64(d)
                    ));
                }
                self.def_done(result, d);
            }
            InstData::StackAddr { slot } => {
                let d = self.def_reg(result);
                let at = self.frame(self.stack_slot_offsets[slot.index()]);
                self.emit(format!("lea\t{}, {at}", r64(d)));
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } if op.is_float() => {
                let d = self.def_freg(result);
                self.float_unary(op, arg, d);
                self.def_done(result, d);
            }
            InstData::Unary { op, arg } => {
                let d = self.def_reg(result);
                self.unary(op, arg, d);
                self.def_done(result, d);
            }
            InstData::Fma { args } => {
                let ty = self.ty(args[0]);
                let moves: Vec<(Loc, Loc)> = (0..3)
                    .map(|i| (Loc::FReg(i as u8), self.loc(args[i])))
                    .collect();
                self.parallel_copy(&moves);
                let name = if ty == Type::F32 { "fmaf" } else { "fma" };
                self.emit(format!("call\t{name}@PLT"));
                self.call_result(result, None);
            }
            InstData::Overflow { op, args } => self.overflow(inst, op, args),
            InstData::Binary { op, args } if op.is_float() => {
                let d = self.def_freg(result);
                self.float_binary(inst, op, args, d);
                self.def_done(result, d);
            }
            InstData::Binary { op, args } => {
                let d = self.def_reg(result);
                self.binary(inst, op, args, d);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_float() => {
                let d = self.def_freg(result);
                let skip = format!(".L{}_{inst}_select", func.name);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                self.emit(format!("movaps\t{}, {}", xmm(FS0), xmm(b)));
                self.emit(format!("je\t{skip}"));
                self.emit(format!("movaps\t{}, {}", xmm(FS0), xmm(a)));
                writeln!(self.out, "{skip}:").unwrap();
                self.emit(format!("movaps\t{}, {}", xmm(d), xmm(FS0)));
                self.def_done(result, d);
            }
            // The false value in rax, replaced by the true one if the
            // condition is non-zero; the moves leave the flags alone.
            InstData::Select { args } => {
                let d = self.def_reg(result);
                self.test_value(args[0]);
                let a = self.use_reg(args[1], S1);
                let b = self.use_reg(args[2], S2);
                self.emit(format!("mov\trax, {}", r64(b)));
                self.emit(format!("cmovne\trax, {}", r64(a)));
                self.emit(format!("mov\t{}, rax", r64(d)));
                self.def_done(result, d);
            }
            InstData::Convert { op, ty, arg } if ty.is_float() || self.ty(arg).is_float() => {
                self.float_convert(inst, op, ty, arg, result);
            }
            InstData::Convert { op, ty, arg } => {
                let d = self.def_reg(result);
                let a = self.use_reg(arg, S0);
                let from = self.ty(arg);
                match op {
                    ConvertOp::Sext => match from {
                        Type::I32 => self.emit(format!("movsxd\t{}, {}", r64(d), r32(a))),
                        _ => self.emit(format!("movsx\t{}, {}", wide(d, ty), reg(a, from))),
                    },
                    // Writing a 32-bit register clears the upper 32 bits.
                    ConvertOp::Uext => match from {
                        Type::I32 => self.emit(format!("mov\t{}, {}", r32(d), r32(a))),
                        _ => self.emit(format!("movzx\t{}, {}", r32(d), reg(a, from))),
                    },
                    // The low bits are already right; upper bits are
                    // unspecified. A pointer is a 64-bit address already.
                    ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                        if a != d {
                            self.emit(format!("mov\t{}, {}", r64(d), r64(a)));
                        }
                    }
                    _ => unreachable!("float conversions are lowered by float_convert"),
                }
                self.def_done(result, d);
            }
            InstData::Icmp { .. } | InstData::Fcmp { .. } => {
                let d = self.def_reg(result);
                let cc = self.compare(inst);
                self.emit(format!("set{cc}\t{}", r8(d)));
                self.def_done(result, d);
            }
            InstData::Load { ty, addr } if ty.is_float() => {
                let d = self.def_freg(result);
                let a = self.mem(addr, S0, ty);
                self.emit(format!("{}\t{}, {a}", movs(ty), xmm(d)));
                self.def_done(result, d);
            }
            // A narrow load zero-extends to 32 bits, which avoids writing
            // part of a register (and depending on its old value).
            InstData::Load { ty, addr } => {
                let d = self.def_reg(result);
                let a = self.mem(addr, S0, ty);
                match ty {
                    Type::I8 | Type::I16 => self.emit(format!("movzx\t{}, {a}", r32(d))),
                    _ => self.emit(format!("mov\t{}, {a}", reg(d, ty))),
                }
                self.def_done(result, d);
            }
            InstData::Store { value, addr } if self.ty(value).is_float() => {
                let ty = self.ty(value);
                let v = self.use_freg(value, FS0);
                let a = self.mem(addr, S1, ty);
                self.emit(format!("{}\t{a}, {}", movs(ty), xmm(v)));
            }
            InstData::Store { value, addr } => {
                let ty = self.ty(value);
                let v = match folded_imm(func, func.inst_data(inst)) {
                    Some((_, k)) => k.to_string(),
                    None => reg(self.use_reg(value, S0), ty).to_string(),
                };
                let a = self.mem(addr, S1, ty);
                self.emit(format!("mov\t{a}, {v}"));
            }
            // x86 is strongly ordered: every load is an acquire and every
            // store a release. Only a sequentially consistent store needs
            // more, so that later loads cannot pass it: `xchg` with memory
            // is locked, a full barrier.
            InstData::AtomicLoad { ty, addr, .. } => {
                let d = self.def_reg(result);
                let a = self.use_reg(addr, S0);
                let src = format!("{} [{}]", ptr(ty), r64(a));
                match ty {
                    Type::I8 | Type::I16 => self.emit(format!("movzx\t{}, {src}", r32(d))),
                    _ => self.emit(format!("mov\t{}, {src}", reg(d, ty))),
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
                    "xchg"
                } else {
                    "mov"
                };
                if op == "xchg" && v != S0 {
                    // xchg writes its register operand.
                    self.emit(format!("mov\trax, {}", r64(v)));
                }
                let v = if op == "xchg" { S0 } else { v };
                self.emit(format!("{op}\t{} [{}], {}", ptr(ty), r64(a), reg(v, ty)));
            }
            InstData::AtomicRmw { op, args, .. } => self.atomic_rmw(inst, op, args, result),
            // `cmpxchg` compares memory with rax and writes the old value
            // to rax either way.
            InstData::AtomicCas { args, .. } => {
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S1);
                let expected = self.use_reg(args[1], S0);
                self.emit(format!("mov\trax, {}", r64(expected)));
                let new = self.use_reg(args[2], S2);
                self.emit(format!(
                    "lock cmpxchg\t{} [{}], {}",
                    ptr(ty),
                    r64(a),
                    reg(new, ty)
                ));
                if let Some(loc) = result.and_then(|r| self.alloc.loc(r)) {
                    self.emit_move(loc, Loc::Reg(S0));
                }
            }
            InstData::Fence { ordering } => {
                if ordering == Ordering::SeqCst {
                    self.emit("mfence");
                }
            }
            InstData::Jump { dest } => {
                self.block_args(&dest);
                self.branch_unless_next(dest.block);
            }
            InstData::Brif {
                cond,
                dests: [then_dest, else_dest],
            } => {
                // To the else-branch when the condition is zero, or when a
                // fused compare's condition does not hold.
                let not = match func.value_def(func.resolve(cond)) {
                    ValueDef::Result(compare, _)
                        if self.fused_compares.contains(&func.resolve(cond)) =>
                    {
                        invert_cond(self.compare(compare))
                    }
                    _ => {
                        self.test_value(cond);
                        "e"
                    }
                };
                // Block arguments are copied on the edge, so a branch with
                // else-arguments needs its own landing pad. The copies are
                // `mov`s, which leave the flags alone.
                if else_dest.args.is_empty() {
                    self.emit(format!("j{not}\t{}", self.label(else_dest.block)));
                    self.block_args(&then_dest);
                    self.branch_unless_next(then_dest.block);
                } else {
                    let pad = format!(".L{}_{inst}_else", func.name);
                    self.emit(format!("j{not}\t{pad}"));
                    self.block_args(&then_dest);
                    self.emit(format!("jmp\t{}", self.label(then_dest.block)));
                    writeln!(self.out, "{pad}:").unwrap();
                    self.block_args(&else_dest);
                    self.branch_unless_next(else_dest.block);
                }
            }
            InstData::Switch { arg, dests, cases } => self.switch(inst, arg, &dests, &cases),
            InstData::Intrinsic { id, args } => match id {
                // The libc functions take the same arguments in the same
                // order. memset's `int c` only uses its low byte, so the
                // unspecified upper bits of the i8 are harmless.
                Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset => {
                    let sig = keel_ir::Signature {
                        params: id.info().params.iter().map(|&t| AbiParam::new(t)).collect(),
                        returns: Vec::new(),
                        cc: CallConv::C,
                        variadic: false,
                        returns_twice: false,
                    };
                    self.move_args(&args, &sig, None, false);
                    self.emit(format!("call\t{id}@PLT"));
                }
                // What clang emits for __builtin_debugtrap; a debugger can
                // continue past it.
                Intrinsic::Debugtrap => self.emit("int3"),
                // Round up to keep rsp 16-byte aligned; the memory starts
                // above the outgoing arguments, which stay at rsp.
                Intrinsic::StackAlloc => {
                    let size = self.use_reg(args[0], S0);
                    self.emit(format!("lea\trax, [{} + 15]", r64(size)));
                    self.emit("and\trax, -16");
                    self.emit("sub\trsp, rax");
                    let d = self.def_reg(result);
                    self.emit(format!("lea\t{}, [rsp + {}]", r64(d), self.outgoing));
                    self.def_done(result, d);
                }
                Intrinsic::StackSave => {
                    let d = self.def_reg(result);
                    self.emit(format!("mov\t{}, rsp", r64(d)));
                    self.def_done(result, d);
                }
                Intrinsic::StackRestore => {
                    let mark = self.use_reg(args[0], S0);
                    self.emit(format!("mov\trsp, {}", r64(mark)));
                }
                // Where the prologue points rbp: the saved rbp, with the
                // return address above it.
                Intrinsic::FrameAddress => {
                    let d = self.def_reg(result);
                    self.emit(format!("mov\t{}, rbp", r64(d)));
                    self.def_done(result, d);
                }
                // SysV's va_list: the offsets of the next general and vector
                // register in the save area (vector registers are 16 bytes,
                // after the 48 of the general ones), the next stack argument
                // and the save area.
                Intrinsic::VaStart => {
                    let p = self.use_reg(args[0], S1);
                    let p = r64(p);
                    let (ints, floats) = (self.named.ints, self.named.floats);
                    self.emit(format!("mov\tdword ptr [{p}], {}", 8 * u32::from(ints)));
                    self.emit(format!(
                        "mov\tdword ptr [{p} + 4], {}",
                        48 + 16 * u32::from(floats)
                    ));
                    self.emit(format!("lea\trax, [rbp + {}]", 16 + self.named.stack));
                    self.emit(format!("mov\tqword ptr [{p} + 8], rax"));
                    let save = self.frame(self.reg_save.expect("a variadic function"));
                    self.emit(format!("lea\trax, {save}"));
                    self.emit(format!("mov\tqword ptr [{p} + 16], rax"));
                }
            },
            InstData::Trap => self.emit("ud2"),
            InstData::Return { args } => {
                // A struct result in registers is loaded from where the
                // function wrote it; one in memory returns its address.
                if let Some(ArgLoc::SretRegs(regs)) = self.named.locs.first().cloned() {
                    let area = self.param_areas[0].expect("planned");
                    for (k, r) in regs.into_iter().enumerate() {
                        let at = format!("qword ptr {}", self.frame(area + 8 * k));
                        match r {
                            Loc::Reg(r) => self.emit(format!("mov\t{}, {at}", r64(r))),
                            _ => self.emit(format!("movsd\t{}, {at}", xmm(loc_reg(r)))),
                        }
                    }
                }
                if let Some(slot) = self.sret_ptr {
                    let at = self.frame(slot);
                    self.emit(format!("mov\trax, qword ptr {at}"));
                }
                if let Some(&value) = args.first() {
                    let ty = self.ty(value);
                    self.emit_move(sysv::return_loc(ty), self.loc(value));
                    if ty == Type::I128 {
                        self.emit_move(Loc::Reg(RDX), self.hi(value));
                    }
                    let ret = &func.sig.returns[0];
                    self.extend_in_place(RAX, ret.ty, ret.ext);
                }
                self.epilogue();
                self.emit("ret");
            }
            InstData::VaArg { ty, list } => self.va_arg(inst, ty, list, result),
            InstData::Invoke { .. }
            | InstData::LandingPad { .. }
            | InstData::EhTypeid { .. }
            | InstData::InlineAsm { .. } => unreachable!("rejected in `new`"),
        }
    }

    /// The next variadic argument from the `va_list` at `list`: from the
    /// register save area while registers of its class are left (the
    /// offset is below 48 for general ones, 176 for vector ones), then from
    /// the stack, 8 bytes each. Its address ends up in rax.
    fn va_arg(&mut self, inst: Inst, ty: Type, list: Value, result: Option<Value>) {
        let p = r64(self.use_reg(list, S2));
        let (field, end, step) = if ty.is_float() {
            (4, 176, 16)
        } else {
            (0, 48, 8)
        };
        let (stack, load) = (
            format!(".L{}_{inst}_stack", self.func.name),
            format!(".L{}_{inst}_load", self.func.name),
        );
        self.emit(format!("mov\tecx, dword ptr [{p} + {field}]"));
        self.emit(format!("cmp\tecx, {end}"));
        self.emit(format!("jae\t{stack}"));
        self.emit(format!("mov\trax, qword ptr [{p} + 16]"));
        self.emit("add\trax, rcx");
        self.emit(format!("add\tecx, {step}"));
        self.emit(format!("mov\tdword ptr [{p} + {field}], ecx"));
        self.emit(format!("jmp\t{load}"));
        writeln!(self.out, "{stack}:").unwrap();
        self.emit(format!("mov\trax, qword ptr [{p} + 8]"));
        self.emit("lea\trcx, [rax + 8]");
        self.emit(format!("mov\tqword ptr [{p} + 8], rcx"));
        writeln!(self.out, "{load}:").unwrap();
        if ty.is_float() {
            let d = self.def_freg(result);
            self.emit(format!("movsd\t{}, qword ptr [rax]", xmm(d)));
            self.def_done(result, d);
        } else {
            let d = self.def_reg(result);
            self.emit(format!("mov\t{}, {} [rax]", reg(d, ty), ptr(ty)));
            self.def_done(result, d);
        }
    }

    /// Lowers `data` if it takes or makes an `i128`, and returns whether it
    /// did. An `i128` is a pair of 64-bit halves, each in its own register
    /// or spill slot. x86 instructions take their second operand from
    /// either, so the halves are read where they are ([`Self::half`]); the
    /// result is built in rax (low) and rdx (high) and moved out last, so
    /// every operand is read before the result is written.
    fn lower_i128(&mut self, inst: Inst, data: &InstData, result: Option<Value>) -> bool {
        let wide = |this: &Self, v: Value| this.ty(v) == Type::I128;
        let wide_result = result.is_some_and(|r| self.func.value_type(r) == Type::I128);
        match *data {
            InstData::Iconst {
                ty: Type::I128,
                imm,
            } => {
                self.materialize(S0, imm, Type::I64);
                self.materialize(S2, imm >> 63, Type::I64);
                self.pair_out(result);
            }
            InstData::Binary { op, args } if wide_result => {
                self.binary_i128(inst, op, args);
                self.pair_out(result);
            }
            InstData::Unary { op, arg } if wide_result => {
                self.unary_i128(inst, op, arg);
                self.pair_out(result);
            }
            InstData::Icmp { cond, args } if wide(self, args[0]) => {
                let [a0, a1] = self.halves(args[0]);
                let [b0, b1] = self.halves(args[1]);
                let code = match cond {
                    // Equal if both halves are: their XORs are zero.
                    IntCC::Eq | IntCC::Ne => {
                        self.emit(format!("mov\trax, {a0}"));
                        self.emit(format!("xor\trax, {b0}"));
                        self.emit(format!("mov\trdx, {a1}"));
                        self.emit(format!("xor\trdx, {b1}"));
                        self.emit("or\trax, rdx");
                        cond_code(cond)
                    }
                    // a - b, with the borrow of the low halves feeding the
                    // high: the flags compare all 128 bits. a > b is b < a,
                    // and a <= b is b >= a: compare swapped.
                    _ => {
                        let swap =
                            matches!(cond, IntCC::Sgt | IntCC::Sle | IntCC::Ugt | IntCC::Ule);
                        let ((x0, x1), (y0, y1)) = if swap {
                            ((b0, b1), (a0, a1))
                        } else {
                            ((a0, a1), (b0, b1))
                        };
                        self.emit(format!("mov\trax, {x0}"));
                        self.emit(format!("cmp\trax, {y0}"));
                        self.emit(format!("mov\trdx, {x1}"));
                        self.emit(format!("sbb\trdx, {y1}"));
                        match cond {
                            IntCC::Slt | IntCC::Sgt => "l",
                            IntCC::Sle | IntCC::Sge => "ge",
                            IntCC::Ult | IntCC::Ugt => "b",
                            _ => "ae",
                        }
                    }
                };
                let d = self.def_reg(result);
                self.emit(format!("set{code}\t{}", r8(d)));
                self.def_done(result, d);
            }
            InstData::Select { args } if wide(self, args[1]) => {
                if wide(self, args[0]) {
                    let [c0, c1] = self.halves(args[0]);
                    self.emit(format!("mov\trax, {c0}"));
                    self.emit(format!("or\trax, {c1}"));
                } else {
                    self.test_value(args[0]);
                }
                let [t0, t1] = self.halves(args[1]);
                let [f0, f1] = self.halves(args[2]);
                self.emit(format!("mov\trax, {f0}"));
                self.emit(format!("cmovne\trax, {t0}"));
                self.emit(format!("mov\trdx, {f1}"));
                self.emit(format!("cmovne\trdx, {t1}"));
                self.pair_out(result);
            }
            InstData::Convert {
                op: op @ (ConvertOp::Sext | ConvertOp::Uext),
                ty: Type::I128,
                arg,
            } => {
                let from = self.ty(arg);
                let a = self.use_reg(arg, S0);
                let signed = op == ConvertOp::Sext;
                match (from, signed) {
                    (Type::I64, _) => self.emit(format!("mov\trax, {}", r64(a))),
                    (Type::I32, true) => self.emit(format!("movsxd\trax, {}", r32(a))),
                    (Type::I32, false) => self.emit(format!("mov\teax, {}", r32(a))),
                    (_, true) => self.emit(format!("movsx\trax, {}", reg(a, from))),
                    (_, false) => self.emit(format!("movzx\teax, {}", reg(a, from))),
                }
                if signed {
                    self.emit("mov\trdx, rax");
                    self.emit("sar\trdx, 63");
                } else {
                    self.emit("xor\tedx, edx");
                }
                self.pair_out(result);
            }
            InstData::Load {
                ty: Type::I128,
                addr,
            } => {
                let a = r64(self.use_reg(addr, S1));
                self.emit(format!("mov\trax, qword ptr [{a}]"));
                self.emit(format!("mov\trdx, qword ptr [{a} + 8]"));
                self.pair_out(result);
            }
            InstData::Store { value, addr } if wide(self, value) => {
                let [v0, v1] = self.halves(value);
                let a = r64(self.use_reg(addr, S1));
                self.emit(format!("mov\trax, {v0}"));
                self.emit(format!("mov\tqword ptr [{a}], rax"));
                self.emit(format!("mov\trax, {v1}"));
                self.emit(format!("mov\tqword ptr [{a} + 8], rax"));
            }
            // The carry or borrow of the low halves feeds the high; the
            // flags then describe the whole 128 bits. Multiplication is
            // rejected in `new`.
            InstData::Overflow { op, args } if wide(self, args[0]) => {
                let [a0, a1] = self.halves(args[0]);
                let [b0, b1] = self.halves(args[1]);
                let (lo, hi, code) = match op {
                    OverflowOp::Uadd => ("add", "adc", "c"),
                    OverflowOp::Sadd => ("add", "adc", "o"),
                    OverflowOp::Usub => ("sub", "sbb", "c"),
                    OverflowOp::Ssub => ("sub", "sbb", "o"),
                    OverflowOp::Umul | OverflowOp::Smul => unreachable!("rejected"),
                };
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("{lo}\trax, {b0}"));
                self.emit(format!("mov\trdx, {a1}"));
                self.emit(format!("{hi}\trdx, {b1}"));
                self.emit(format!("set{code}\tcl"));
                self.pair_out(result);
                let flag = self.func.inst_results(inst)[1];
                if let Some(loc) = self.alloc.loc(flag) {
                    self.emit_move(loc, Loc::Reg(S1));
                }
            }
            _ => return false,
        }
        true
    }

    /// The halves of `i128` operand `value`, low first, as operands: a
    /// register or a spill slot.
    fn halves(&self, value: Value) -> [String; 2] {
        [self.loc(value), self.hi(value)].map(|loc| self.operand(loc))
    }

    /// Integer location `loc` as a 64-bit operand.
    fn operand(&self, loc: Loc) -> String {
        match loc {
            Loc::Reg(r) => r64(r).to_string(),
            Loc::Stack(s) => self.slot(s),
            Loc::FReg(_) => unreachable!("not an integer"),
        }
    }

    /// The location of the high half of `i128` operand `value`.
    fn hi(&self, value: Value) -> Loc {
        self.alloc
            .hi_loc(self.func.resolve(value))
            .expect("operand in reachable code is allocated")
    }

    /// Moves an `i128` result from rax (low) and rdx (high) to its places.
    fn pair_out(&mut self, result: Option<Value>) {
        let Some(value) = result else { return };
        if let Some(loc) = self.alloc.loc(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.alloc.hi_loc(value) {
            self.emit_move(loc, Loc::Reg(S2));
        }
    }

    /// `args[0] op args[1]` on `i128`s into rdx:rax. A shift amount may be
    /// narrower; only its low bits count.
    fn binary_i128(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2]) {
        let [a0, a1] = self.halves(args[0]);
        let [b0, b1] = if self.ty(args[1]) == Type::I128 {
            self.halves(args[1])
        } else {
            [self.operand(self.loc(args[1])), String::new()]
        };
        let label = |what: &str| format!(".L{}_{inst}_{what}", self.func.name);
        let pair = |this: &mut Self, lo: &str, hi: &str| {
            this.emit(format!("mov\trax, {a0}"));
            this.emit(format!("{lo}\trax, {b0}"));
            this.emit(format!("mov\trdx, {a1}"));
            this.emit(format!("{hi}\trdx, {b1}"));
        };
        match op {
            BinaryOp::Iadd => pair(self, "add", "adc"),
            BinaryOp::Isub => pair(self, "sub", "sbb"),
            BinaryOp::And => pair(self, "and", "and"),
            BinaryOp::Or => pair(self, "or", "or"),
            BinaryOp::Xor => pair(self, "xor", "xor"),
            // (a1:a0)(b1:b0) mod 2^128 = a0 b0 + 2^64 (a0 b1 + a1 b0); `mul`
            // gives the full 128-bit a0 b0 in rdx:rax.
            BinaryOp::Imul => {
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("mul\t{b0}"));
                self.emit(format!("mov\trcx, {a0}"));
                self.emit(format!("imul\trcx, {b1}"));
                self.emit("add\trdx, rcx");
                self.emit(format!("mov\trcx, {a1}"));
                self.emit(format!("imul\trcx, {b0}"));
                self.emit("add\trdx, rcx");
            }
            // `shld`/`shrd` shift one half while filling from the other,
            // by the count modulo 64; at 64 or more (bit 6 of the count),
            // the result is the other half shifted, and zeros or the sign.
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                let done = label("shifted");
                self.emit(format!("mov\trcx, {b0}"));
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("mov\trdx, {a1}"));
                match op {
                    BinaryOp::Shl => {
                        self.emit("shld\trdx, rax, cl");
                        self.emit("shl\trax, cl");
                        self.emit("test\tcl, 64");
                        self.emit(format!("je\t{done}"));
                        self.emit("mov\trdx, rax");
                        self.emit("xor\teax, eax");
                    }
                    _ => {
                        let shr = if op == BinaryOp::Sshr { "sar" } else { "shr" };
                        self.emit("shrd\trax, rdx, cl");
                        self.emit(format!("{shr}\trdx, cl"));
                        self.emit("test\tcl, 64");
                        self.emit(format!("je\t{done}"));
                        self.emit("mov\trax, rdx");
                        if op == BinaryOp::Sshr {
                            self.emit("sar\trdx, 63");
                        } else {
                            self.emit("xor\tedx, edx");
                        }
                    }
                }
                writeln!(self.out, "{done}:").unwrap();
            }
            // Rotating by 64 swaps the halves; then each half rotates by the
            // rest, filling from the other's original bits.
            BinaryOp::Rotl | BinaryOp::Rotr => {
                let swapped = label("swapped");
                self.emit(format!("mov\trcx, {b0}"));
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("mov\trdx, {a1}"));
                self.emit("test\tcl, 64");
                self.emit(format!("je\t{swapped}"));
                self.emit("xchg\trax, rdx");
                writeln!(self.out, "{swapped}:").unwrap();
                let fill = if op == BinaryOp::Rotl { "shld" } else { "shrd" };
                self.emit("mov\tr11, rax");
                self.emit(format!("{fill}\trax, rdx, cl"));
                self.emit(format!("{fill}\trdx, r11, cl"));
            }
            // libgcc takes the pairs in rdi:rsi and rdx:rcx and returns
            // rdx:rax. Traps first, as for narrower types.
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => {
                let trap = self.trap_label();
                self.emit(format!("mov\trax, {b0}"));
                self.emit(format!("or\trax, {b1}"));
                self.emit(format!("je\t{trap}"));
                if op == BinaryOp::Sdiv {
                    // MIN / -1: the divisor's halves are both all ones and
                    // the dividend is 2^127.
                    let ok = label("divok");
                    self.emit(format!("mov\trax, {b0}"));
                    self.emit(format!("and\trax, {b1}"));
                    self.emit("cmp\trax, -1");
                    self.emit(format!("jne\t{ok}"));
                    self.emit(format!("mov\trax, {a0}"));
                    self.emit("test\trax, rax");
                    self.emit(format!("jne\t{ok}"));
                    self.emit("movabs\trax, 0x8000000000000000");
                    self.emit(format!("cmp\trax, {a1}"));
                    self.emit(format!("je\t{trap}"));
                    writeln!(self.out, "{ok}:").unwrap();
                }
                let moves = [
                    (Loc::Reg(RDI), self.loc(args[0])),
                    (Loc::Reg(6), self.hi(args[0])),
                    (Loc::Reg(RDX), self.loc(args[1])),
                    (Loc::Reg(RCX), self.hi(args[1])),
                ];
                self.parallel_copy(&moves);
                let function = match op {
                    BinaryOp::Udiv => "__udivti3",
                    BinaryOp::Sdiv => "__divti3",
                    BinaryOp::Urem => "__umodti3",
                    _ => "__modti3",
                };
                self.emit(format!("call\t{function}@PLT"));
            }
            _ => unreachable!("{op} on i128 is rejected or not an integer operation"),
        }
    }

    /// `op` on an `i128` into rdx:rax.
    fn unary_i128(&mut self, inst: Inst, op: UnaryOp, arg: Value) {
        let [a0, a1] = self.halves(arg);
        let label = |what: &str| format!(".L{}_{inst}_{what}", self.func.name);
        match op {
            // -x = ~x + 1: `neg` the low half sets the carry unless it was
            // zero, which the high half then absorbs.
            UnaryOp::Ineg => {
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("mov\trdx, {a1}"));
                self.emit("neg\trax");
                self.emit("adc\trdx, 0");
                self.emit("neg\trdx");
            }
            UnaryOp::Bnot => {
                self.emit(format!("mov\trax, {a0}"));
                self.emit(format!("mov\trdx, {a1}"));
                self.emit("not\trax");
                self.emit("not\trdx");
            }
            // The high half's count, or 64 plus the low half's (see
            // `unary` for the `bsr` arithmetic).
            UnaryOp::Clz => {
                let (low, done) = (label("low"), label("counted"));
                self.emit(format!("mov\trdx, {a1}"));
                self.emit("test\trdx, rdx");
                self.emit(format!("je\t{low}"));
                self.emit("bsr\trax, rdx");
                self.emit("xor\teax, 63");
                self.emit(format!("jmp\t{done}"));
                writeln!(self.out, "{low}:").unwrap();
                self.emit(format!("mov\trdx, {a0}"));
                self.emit("bsr\trax, rdx");
                self.emit("mov\tecx, 127");
                self.emit("cmove\teax, ecx");
                self.emit("xor\teax, 63");
                self.emit("add\teax, 64");
                writeln!(self.out, "{done}:").unwrap();
                self.emit("xor\tedx, edx");
            }
            UnaryOp::Ctz => {
                let (high, done) = (label("high"), label("counted"));
                self.emit(format!("mov\trdx, {a0}"));
                self.emit("test\trdx, rdx");
                self.emit(format!("je\t{high}"));
                self.emit("bsf\trax, rdx");
                self.emit(format!("jmp\t{done}"));
                writeln!(self.out, "{high}:").unwrap();
                self.emit(format!("mov\trdx, {a1}"));
                self.emit("bsf\trax, rdx");
                self.emit("mov\tecx, 64");
                self.emit("cmove\teax, ecx");
                self.emit("add\teax, 64");
                writeln!(self.out, "{done}:").unwrap();
                self.emit("xor\tedx, edx");
            }
            UnaryOp::Popcnt => {
                self.emit(format!("popcnt\trax, {a0}"));
                self.emit(format!("popcnt\trdx, {a1}"));
                self.emit("add\trax, rdx");
                self.emit("xor\tedx, edx");
            }
            UnaryOp::Bswap => {
                self.emit(format!("mov\trax, {a1}"));
                self.emit(format!("mov\trdx, {a0}"));
                self.emit("bswap\trax");
                self.emit("bswap\trdx");
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// The operand of a `call` or `jmp` to function `name`: through the
    /// PLT if it is in another module.
    fn call_target(&self, name: &str) -> String {
        if self.module.defines(name) {
            name.to_string()
        } else {
            format!("{name}@PLT")
        }
    }

    /// Moves call arguments into their registers and the stack, for
    /// signature `sig`; any arguments past its parameters are variadic.
    /// `callee`, for an indirect call, moves into r10 in the same parallel
    /// copy, since it may be in an argument register. A tail call (`tail`)
    /// puts stack arguments where this function's own came in.
    ///
    /// Stack arguments and struct copies come first: their sources are
    /// allocated locations, and storing them only uses the scratch
    /// registers. A struct passed in registers is loaded into staging
    /// slots, from which the parallel copy fills the registers along with
    /// the scalars: loading it straight into them could overwrite another
    /// argument's source. Values live across the call are in callee-saved
    /// registers or spill slots, so nothing needs saving.
    ///
    /// Narrow arguments marked `sext`/`uext` are extended to 32 bits in
    /// their register, which clang relies on.
    ///
    /// Returns the registers and layout of an `sret` result that comes back
    /// in registers, for [`Self::call_result`] to store.
    fn move_args<'s>(
        &mut self,
        args: &[Value],
        sig: &'s keel_ir::Signature,
        callee: Option<Value>,
        tail: bool,
    ) -> Option<(Vec<Loc>, &'s Layout)> {
        let kinds = arg_kinds(self.func, &sig.params, args);
        let placement = sysv::arg_locs(&kinds);
        let mut moves = Vec::new();
        let mut extend = Vec::new();
        let mut staged = self.alloc.spill_slots;
        let mut sret = None;
        let stack_base = if tail { "rbp + 16" } else { "rsp" };
        for (i, ((&arg, loc), kind)) in args.iter().zip(placement.locs).zip(&kinds).enumerate() {
            let layout = match kind {
                ArgKind::ByVal(layout) | ArgKind::Sret(layout) => Some(*layout),
                ArgKind::Value(_) => None,
            };
            match loc {
                ArgLoc::Value(Slot::Regs(regs)) => {
                    moves.push((regs[0], self.loc(arg)));
                    if self.ty(arg) == Type::I128 {
                        moves.push((regs[1], self.hi(arg)));
                    }
                    if let (Some(param), Loc::Reg(r)) = (sig.params.get(i), regs[0]) {
                        extend.push((r, param.ty, param.ext));
                    }
                }
                ArgLoc::Value(Slot::Stack(offset)) => {
                    let at = |o: u32| format!("[{stack_base} + {}]", offset + o);
                    if self.ty(arg) == Type::I128 {
                        self.store_stack_arg(self.loc(arg), &at(0));
                        self.store_stack_arg(self.hi(arg), &at(8));
                    } else {
                        self.store_stack_arg(self.loc(arg), &at(0));
                    }
                }
                ArgLoc::OnStack(offset) => {
                    let p = self.use_reg(arg, S1);
                    let size = layout.expect("a struct").size();
                    self.copy_bytes(&format!("rsp + {offset}"), p, size);
                }
                ArgLoc::InRegs(regs) => {
                    let p = self.use_reg(arg, S1);
                    let size = layout.expect("a struct").size();
                    for (k, r) in regs.into_iter().enumerate() {
                        let offset = 8 * k as u64;
                        self.load_chunk(S0, p, offset, (size - offset).min(8));
                        self.emit(format!("mov\t{}, rax", self.slot(staged)));
                        moves.push((r, Loc::Stack(staged)));
                        staged += 1;
                    }
                }
                ArgLoc::SretMemory => moves.push((Loc::Reg(RDI), self.loc(arg))),
                ArgLoc::SretRegs(regs) => {
                    let p = self.use_reg(arg, S1);
                    let save = self.frame(self.sret_save.expect("planned"));
                    self.emit(format!("mov\tqword ptr {save}, {}", r64(p)));
                    sret = Some((regs, layout.expect("a struct")));
                }
            }
        }
        if let Some(callee) = callee {
            moves.push((Loc::Reg(R10), self.loc(callee)));
        }
        self.parallel_copy(&moves);
        for (r, ty, ext) in extend {
            self.extend_in_place(r, ty, ext);
        }
        // A variadic callee saves this many vector registers.
        if sig.variadic {
            self.emit(format!("mov\teax, {}", placement.floats));
        }
        sret
    }

    /// Moves a call's result, if it has one, from rax or xmm0 to its place,
    /// and stores a struct result that came back in registers to its
    /// `sret` address.
    fn call_result(&mut self, result: Option<Value>, sret: Option<(Vec<Loc>, &Layout)>) {
        if let Some(value) = result
            && let Some(loc) = self.alloc.loc(value)
        {
            self.emit_move(loc, sysv::return_loc(self.func.value_type(value)));
            if let Some(hi) = self.alloc.hi_loc(value) {
                self.emit_move(hi, Loc::Reg(RDX));
            }
        }
        if let Some((regs, layout)) = sret {
            let save = self.frame(self.sret_save.expect("planned"));
            self.emit(format!("mov\tr11, qword ptr {save}"));
            for (k, r) in regs.into_iter().enumerate() {
                let offset = 8 * k as u64;
                let n = (layout.size() - offset).min(8);
                match r {
                    Loc::Reg(r) => self.store_chunk(r, S3, offset, n),
                    _ => {
                        let mov = if n == 4 { "movss" } else { "movsd" };
                        let size = if n == 4 { "dword" } else { "qword" };
                        let x = xmm(loc_reg(r));
                        self.emit(format!("{mov}\t{size} ptr [r11 + {offset}], {x}"));
                    }
                }
            }
        }
    }

    /// Loads the `n` bytes (1 to 8) at `[p + offset]` into register `d`,
    /// zero-extended, without reading past them: a struct may end at the
    /// end of a page. Odd sizes are pieced together through rdx.
    fn load_chunk(&mut self, d: u8, p: u8, offset: u64, n: u64) {
        let at = |o: u64| format!("[{} + {}]", r64(p), offset + o);
        match n {
            8 => self.emit(format!("mov\t{}, qword ptr {}", r64(d), at(0))),
            4 => self.emit(format!("mov\t{}, dword ptr {}", r32(d), at(0))),
            2 => self.emit(format!("movzx\t{}, word ptr {}", r32(d), at(0))),
            1 => self.emit(format!("movzx\t{}, byte ptr {}", r32(d), at(0))),
            _ => {
                let low = if n > 4 { 4 } else { 2 };
                self.load_chunk(d, p, offset, low);
                for (piece, at_byte) in pieces(n - low).into_iter().map(|(o, k)| (k, low + o)) {
                    self.load_chunk(S2, p, offset + at_byte, piece);
                    self.emit(format!("shl\trdx, {}", 8 * at_byte));
                    self.emit(format!("or\t{}, rdx", r64(d)));
                }
            }
        }
    }

    /// Stores the low `n` bytes (1 to 8) of register `r` at
    /// `[p + offset]`, without writing past them. `r` is shifted down as
    /// pieces are written.
    fn store_chunk(&mut self, r: u8, p: u8, offset: u64, n: u64) {
        let mut done = 0;
        for (o, k) in pieces(n) {
            if o > done {
                self.emit(format!("shr\t{}, {}", r64(r), 8 * (o - done)));
                done = o;
            }
            let (size, src) = match k {
                8 => ("qword", r64(r)),
                4 => ("dword", r32(r)),
                2 => ("word", r16(r)),
                _ => ("byte", r8(r)),
            };
            self.emit(format!(
                "mov\t{size} ptr [{} + {}], {src}",
                r64(p),
                offset + o
            ));
        }
    }

    /// Copies `size` bytes from `[src]` to `[to]`, 8 at a time through rax
    /// and then the remainder exactly.
    fn copy_bytes(&mut self, to: &str, src: u8, size: u64) {
        let mut offset = 0;
        while offset < size {
            let n = (size - offset).min(8);
            for (o, k) in pieces(n) {
                let (ptr_kw, rax) = match k {
                    8 => ("qword", "rax"),
                    4 => ("dword", "eax"),
                    2 => ("word", "ax"),
                    _ => ("byte", "al"),
                };
                let at = offset + o;
                self.emit(format!("mov\t{rax}, {ptr_kw} ptr [{} + {at}]", r64(src)));
                self.emit(format!("mov\t{ptr_kw} ptr [{to} + {at}], {rax}"));
            }
            offset += n;
        }
    }

    /// Sets `dst` to the address of memory operand `at` (`[rbp - 16]`).
    fn address(&mut self, dst: Loc, at: &str) {
        match dst {
            Loc::Reg(r) => self.emit(format!("lea\t{}, {at}", r64(r))),
            _ => {
                self.emit(format!("lea\trax, {at}"));
                self.emit_move(dst, Loc::Reg(S0));
            }
        }
    }

    /// Extends the narrow value in register `r` to 32 bits, if `ext` asks.
    fn extend_in_place(&mut self, r: u8, ty: Type, ext: ArgExt) {
        let op = match ext {
            ArgExt::None => return,
            ArgExt::Sext => "movsx",
            ArgExt::Uext => "movzx",
        };
        if matches!(ty, Type::I8 | Type::I16) {
            self.emit(format!("{op}\t{}, {}", r32(r), reg(r, ty)));
        }
    }

    /// Stores an argument from `src` at memory `at`, a stack argument's
    /// slot. Each has 8 bytes, so all 64 bits of an integer can go.
    fn store_stack_arg(&mut self, src: Loc, at: &str) {
        match src {
            Loc::Reg(r) => self.emit(format!("mov\tqword ptr {at}, {}", r64(r))),
            Loc::FReg(r) => self.emit(format!("movsd\tqword ptr {at}, {}", xmm(r))),
            Loc::Stack(s) => {
                self.emit(format!("mov\trax, {}", self.slot(s)));
                self.emit(format!("mov\tqword ptr {at}, rax"));
            }
        }
    }

    /// Loads a value of type `ty` from memory `src` into `dst`.
    fn load_into(&mut self, dst: Loc, ty: Type, src: &str) {
        match dst {
            Loc::Reg(r) => self.emit(format!("mov\t{}, qword ptr {src}", r64(r))),
            Loc::FReg(r) => self.emit(format!("{}\t{}, {} {src}", movs(ty), xmm(r), ptr(ty))),
            Loc::Stack(_) => {
                let tmp = if ty.is_float() {
                    Loc::FReg(FS0)
                } else {
                    Loc::Reg(S0)
                };
                self.load_into(tmp, ty, src);
                self.emit_move(dst, tmp);
            }
        }
    }

    /// Restores the callee-saved registers, rsp and rbp.
    fn epilogue(&mut self) {
        if self.saved.is_empty() {
            self.emit("mov\trsp, rbp");
        } else {
            self.emit(format!("lea\trsp, [rbp - {}]", 8 * self.saved.len()));
            for r in self.saved.clone().into_iter().rev() {
                self.emit(format!("pop\t{}", r64(r)));
            }
        }
        self.emit("pop\trbp");
    }

    /// Computes `args[0] op args[1]` into `d`.
    fn binary(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let a = self.use_reg(args[0], S0);
        let imm = folded_imm(self.func, self.func.inst_data(inst)).map(|(_, k)| k);
        let simple = match op {
            BinaryOp::Iadd | BinaryOp::Ptradd => Some("add"),
            BinaryOp::Isub => Some("sub"),
            BinaryOp::And => Some("and"),
            BinaryOp::Or => Some("or"),
            BinaryOp::Xor => Some("xor"),
            BinaryOp::Imul if imm.is_none() => Some("imul"),
            _ => None,
        };
        if let Some(mnemonic) = simple {
            let b = match imm {
                Some(k) => Operand::Imm(k),
                None => Operand::Reg(self.use_reg(args[1], S1)),
            };
            self.two_address(mnemonic, ty, d, a, b);
            return;
        }
        match op {
            // The three-operand form writes a fresh destination.
            BinaryOp::Imul => {
                let k = imm.expect("register form handled above");
                self.emit(format!("imul\t{}, {}, {k}", wide(d, ty), wide(a, ty)));
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr | BinaryOp::Rotl | BinaryOp::Rotr => {
                let mnemonic = match op {
                    BinaryOp::Shl => "shl",
                    BinaryOp::Ushr => "shr",
                    BinaryOp::Sshr => "sar",
                    BinaryOp::Rotl => "rol",
                    _ => "ror",
                };
                // The count goes in cl. Shifts take it modulo 32 or 64; a
                // narrower type needs its own mask. Rotates of narrow
                // registers already rotate modulo their width.
                let count = match imm {
                    Some(k) => k.to_string(),
                    None => {
                        let b = self.use_reg(args[1], S1);
                        if b != RCX {
                            self.emit(format!("mov\tecx, {}", r32(b)));
                        }
                        let rotate = matches!(op, BinaryOp::Rotl | BinaryOp::Rotr);
                        if let (Some(bits), false) = (narrow_bits(ty), rotate) {
                            self.emit(format!("and\tecx, {}", bits - 1));
                        }
                        "cl".into()
                    }
                };
                // Right shifts read the upper bits, so a narrow value is
                // extended first; rotates work at the type's own width.
                let (a, width) = match op {
                    BinaryOp::Ushr => (self.extend(S0, a, ty, false), wide_ty(ty)),
                    BinaryOp::Sshr => (self.extend(S0, a, ty, true), wide_ty(ty)),
                    BinaryOp::Shl => (a, wide_ty(ty)),
                    _ => (a, ty),
                };
                if a != d {
                    self.emit(format!("mov\t{}, {}", r64(d), r64(a)));
                }
                self.emit(format!("{mnemonic}\t{}, {count}", reg(d, width)));
            }
            // The double-width product of the extended operands: rdx:rax
            // from a one-operand `mul`/`imul` for 32 and 64 bits; for
            // narrower types, the product fits 32 bits, shifted down.
            BinaryOp::Umulhi | BinaryOp::Smulhi => {
                let signed = op == BinaryOp::Smulhi;
                let b = self.use_reg(args[1], S1);
                match narrow_bits(ty) {
                    None => {
                        if a != S0 {
                            self.emit(format!("mov\trax, {}", r64(a)));
                        }
                        let mnemonic = if signed { "imul" } else { "mul" };
                        self.emit(format!("{mnemonic}\t{}", reg(b, ty)));
                        self.emit(format!("mov\t{}, rdx", r64(d)));
                    }
                    Some(bits) => {
                        let x = self.extend(S0, a, ty, signed);
                        let y = self.extend(S1, b, ty, signed);
                        if x != S0 {
                            self.emit(format!("mov\teax, {}", r32(x)));
                        }
                        self.emit(format!("imul\teax, {}", r32(y)));
                        let shr = if signed { "sar" } else { "shr" };
                        self.emit(format!("{shr}\teax, {bits}"));
                        self.emit(format!("mov\t{}, eax", r32(d)));
                    }
                }
            }
            BinaryOp::Udiv | BinaryOp::Urem | BinaryOp::Sdiv | BinaryOp::Srem => {
                self.divide(inst, op, args, a, d);
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// Division: the dividend in rax (edx:eax or rdx:rax extended from
    /// it), the divisor in a register; the quotient comes back in rax, the
    /// remainder in rdx. Narrow types are extended to 32 bits first.
    fn divide(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], a: u8, d: u8) {
        let ty = self.ty(args[0]);
        let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
        let width = wide_ty(ty);
        let b = self.use_reg(args[1], S1);
        let b = self.extend(S1, b, ty, signed);
        let x = self.extend(S0, a, ty, signed);
        if x != S0 {
            self.emit(format!("mov\t{}, {}", reg(S0, width), reg(x, width)));
        }
        let rb = reg(b, width);
        let trap = self.trap_label();
        // Hardware division faults on a zero divisor; the IR traps.
        self.emit(format!("test\t{rb}, {rb}"));
        self.emit(format!("je\t{trap}"));
        let name = &self.func.name;
        let done = format!(".L{name}_{inst}_div");
        if signed {
            // MIN / -1 overflows, which faults too. The IR traps on the
            // quotient and gives 0 for the remainder.
            self.emit(format!("cmp\t{rb}, -1"));
            let go = format!(".L{name}_{inst}_divide");
            self.emit(format!("jne\t{go}"));
            if op == BinaryOp::Sdiv {
                let min = min_value(ty);
                if width == Type::I64 {
                    self.emit(format!("movabs\trdx, {min:#x}"));
                    self.emit("cmp\trax, rdx");
                } else {
                    self.emit(format!("cmp\teax, {}", min as u32 as i32));
                }
                self.emit(format!("je\t{trap}"));
            } else {
                self.emit("xor\tedx, edx");
                self.emit(format!("jmp\t{done}"));
            }
            writeln!(self.out, "{go}:").unwrap();
            self.emit(if width == Type::I64 { "cqo" } else { "cdq" });
            self.emit(format!("idiv\t{rb}"));
        } else {
            self.emit("xor\tedx, edx");
            self.emit(format!("div\t{rb}"));
        }
        writeln!(self.out, "{done}:").unwrap();
        let src = match op {
            BinaryOp::Udiv | BinaryOp::Sdiv => S0,
            _ => S2,
        };
        self.emit(format!("mov\t{}, {}", r64(d), r64(src)));
    }

    /// `d = a op b` with a two-address instruction `op d, b`, for an
    /// integer operation of type `ty`, on 32 or 64 bits.
    fn two_address(&mut self, mnemonic: &str, ty: Type, d: u8, a: u8, b: Operand) {
        let width = wide_ty(ty);
        // `mov d, a` would overwrite b.
        let dst = match b {
            Operand::Reg(r) if r == d && r != a => S0,
            _ => d,
        };
        if dst != a {
            self.emit(format!("mov\t{}, {}", r64(dst), r64(a)));
        }
        let b = match b {
            Operand::Reg(r) => reg(r, width).to_string(),
            Operand::Imm(k) => k.to_string(),
        };
        self.emit(format!("{mnemonic}\t{}, {b}", reg(dst, width)));
        if dst != d {
            self.emit(format!("mov\t{}, {}", r64(d), r64(dst)));
        }
    }

    fn unary(&mut self, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_reg(arg, S0);
        let width = wide_ty(ty);
        match op {
            UnaryOp::Ineg | UnaryOp::Bnot => {
                if a != d {
                    self.emit(format!("mov\t{}, {}", r64(d), r64(a)));
                }
                let mnemonic = if op == UnaryOp::Ineg { "neg" } else { "not" };
                self.emit(format!("{mnemonic}\t{}", reg(d, width)));
            }
            // `bsr` gives the index of the highest set bit, i, and leaves
            // ZF set for zero. The count is (bits - 1) - i, which is
            // (bits - 1) ^ i; for zero, (2 * bits - 1) ^ (bits - 1) = bits.
            // Narrow values count as 32 bits, less the bits above them.
            UnaryOp::Clz => {
                let x = self.extend(S0, a, ty, false);
                let bits = if width == Type::I64 { 64 } else { 32 };
                let (rx, rax) = (reg(x, width), reg(S0, width));
                self.emit(format!("bsr\t{rax}, {rx}"));
                self.emit(format!("mov\tecx, {}", 2 * bits - 1));
                self.emit(format!("cmove\t{rax}, {}", reg(S1, width)));
                self.emit(format!("xor\teax, {}", bits - 1));
                if let Some(narrow) = narrow_bits(ty) {
                    self.emit(format!("sub\teax, {}", 32 - narrow));
                }
                self.emit(format!("mov\t{}, eax", r32(d)));
            }
            // `bsf` gives the index of the lowest set bit. A narrow value
            // gets a bit set just above it, which is the answer for zero.
            UnaryOp::Ctz => match narrow_bits(ty) {
                Some(bits) => {
                    self.emit(format!("mov\teax, {}", r32(a)));
                    self.emit(format!("or\teax, {}", 1u32 << bits));
                    self.emit("bsf\teax, eax");
                    self.emit(format!("mov\t{}, eax", r32(d)));
                }
                None => {
                    let bits = if width == Type::I64 { 64 } else { 32 };
                    self.emit(format!("bsf\t{}, {}", reg(S0, width), reg(a, width)));
                    self.emit(format!("mov\tecx, {bits}"));
                    self.emit(format!("cmove\t{}, {}", reg(S0, width), reg(S1, width)));
                    self.emit(format!("mov\t{}, {}", r64(d), r64(S0)));
                }
            },
            UnaryOp::Popcnt => {
                let x = self.extend(S0, a, ty, false);
                self.emit(format!("popcnt\t{}, {}", reg(d, width), reg(x, width)));
            }
            UnaryOp::Bswap => {
                if a != d {
                    self.emit(format!("mov\t{}, {}", r64(d), r64(a)));
                }
                match ty {
                    Type::I8 => {}
                    Type::I16 => self.emit(format!("rol\t{}, 8", r16(d))),
                    _ => self.emit(format!("bswap\t{}", reg(d, ty))),
                }
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// Float negation, absolute value and copysign flip or clear the sign
    /// bit; the rest are SSE instructions (`roundsd` is SSE4.1).
    fn float_unary(&mut self, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_freg(arg, FS0);
        let (gpr, sign) = if ty == Type::F32 {
            ("eax", 31)
        } else {
            ("rax", 63)
        };
        let mov = if ty == Type::F32 { "movd" } else { "movq" };
        match op {
            UnaryOp::Fneg | UnaryOp::Fabs => {
                let bit_op = if op == UnaryOp::Fneg { "btc" } else { "btr" };
                self.emit(format!("{mov}\t{gpr}, {}", xmm(a)));
                self.emit(format!("{bit_op}\t{gpr}, {sign}"));
                self.emit(format!("{mov}\t{}, {gpr}", xmm(d)));
            }
            UnaryOp::Fsqrt => self.emit(format!("sqrt{}\t{}, {}", suffix(ty), xmm(d), xmm(a))),
            _ => {
                // Bit 3 suppresses the inexact exception; bits 1:0 are the
                // rounding mode.
                let mode = match op {
                    UnaryOp::Fnearest => 8,
                    UnaryOp::Ffloor => 9,
                    UnaryOp::Fceil => 10,
                    UnaryOp::Ftrunc => 11,
                    _ => unreachable!("{op} is not a float operation"),
                };
                self.emit(format!(
                    "round{}\t{}, {}, {mode}",
                    suffix(ty),
                    xmm(d),
                    xmm(a)
                ));
            }
        }
    }

    fn float_binary(&mut self, inst: Inst, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let a = self.use_freg(args[0], FS0);
        let b = self.use_freg(args[1], FS1);
        let s = suffix(ty);
        // Two-address, computed in xmm13 unless d is free to overwrite.
        let dst = if b == d && a != d { FS0 } else { d };
        let (xd, xb) = (xmm(dst), xmm(b));
        if dst != a {
            self.emit(format!("movaps\t{xd}, {}", xmm(a)));
        }
        match op {
            BinaryOp::Fadd | BinaryOp::Fsub | BinaryOp::Fmul | BinaryOp::Fdiv => {
                let mnemonic = match op {
                    BinaryOp::Fadd => "add",
                    BinaryOp::Fsub => "sub",
                    BinaryOp::Fmul => "mul",
                    _ => "div",
                };
                self.emit(format!("{mnemonic}{s}\t{xd}, {xb}"));
            }
            // `minsd`/`maxsd` give the second operand for a NaN or two
            // zeros. IEEE minimum wants NaN, and -0 below +0: for equal
            // operands, OR the bits (min) or AND them (max), which only
            // differs from either operand for zeros of opposite signs.
            BinaryOp::Fmin | BinaryOp::Fmax => {
                let min = op == BinaryOp::Fmin;
                let name = &self.func.name;
                let (nan, ne, done) = (
                    format!(".L{name}_{inst}_nan"),
                    format!(".L{name}_{inst}_ne"),
                    format!(".L{name}_{inst}_done"),
                );
                let p = if ty == Type::F32 { "ps" } else { "pd" };
                self.emit(format!("ucomi{s}\t{xd}, {xb}"));
                self.emit(format!("jp\t{nan}"));
                self.emit(format!("jne\t{ne}"));
                let bits = if min { "or" } else { "and" };
                self.emit(format!("{bits}{p}\t{xd}, {xb}"));
                self.emit(format!("jmp\t{done}"));
                writeln!(self.out, "{ne}:").unwrap();
                let pick = if min { "min" } else { "max" };
                self.emit(format!("{pick}{s}\t{xd}, {xb}"));
                self.emit(format!("jmp\t{done}"));
                // Adding gives a quiet NaN.
                writeln!(self.out, "{nan}:").unwrap();
                self.emit(format!("add{s}\t{xd}, {xb}"));
                writeln!(self.out, "{done}:").unwrap();
            }
            // The sign bit of b with the rest of a, spliced in rax.
            BinaryOp::Fcopysign => {
                let (mov, ra, rc, sign) = if ty == Type::F32 {
                    ("movd", "eax", "ecx", 31)
                } else {
                    ("movq", "rax", "rcx", 63)
                };
                self.emit(format!("{mov}\t{ra}, {xd}"));
                self.emit(format!("{mov}\t{rc}, {xb}"));
                self.emit(format!("btr\t{ra}, {sign}"));
                self.emit(format!("shr\t{rc}, {sign}"));
                self.emit(format!("shl\t{rc}, {sign}"));
                self.emit(format!("or\t{ra}, {rc}"));
                self.emit(format!("{mov}\t{xd}, {ra}"));
            }
            _ => unreachable!("{op} is not a float operation"),
        }
        if dst != d {
            self.emit(format!("movaps\t{}, {xd}", xmm(d)));
        }
    }

    fn float_convert(
        &mut self,
        inst: Inst,
        op: ConvertOp,
        to: Type,
        arg: Value,
        result: Option<Value>,
    ) {
        let from = self.ty(arg);
        let name = self.func.name.clone();
        let label = |what: &str| format!(".L{name}_{inst}_{what}");
        match op {
            ConvertOp::Fpext | ConvertOp::Fptrunc => {
                let d = self.def_freg(result);
                let a = self.use_freg(arg, FS0);
                let mnemonic = if op == ConvertOp::Fpext {
                    "cvtss2sd"
                } else {
                    "cvtsd2ss"
                };
                self.emit(format!("{mnemonic}\t{}, {}", xmm(d), xmm(a)));
                self.def_done(result, d);
            }
            // `cvtsi2sd` reads a signed 32- or 64-bit integer. Narrow and
            // unsigned 32-bit values are extended to 64 bits first, which
            // makes them non-negative where needed.
            ConvertOp::Sitofp | ConvertOp::Uitofp => {
                let signed = op == ConvertOp::Sitofp;
                let d = self.def_freg(result);
                let a = self.use_reg(arg, S0);
                let s = suffix(to);
                let xd = xmm(d);
                // `cvtsi2sd` only writes the low lane; clearing the
                // register first breaks the dependency on its old value.
                self.emit(format!("xorps\t{xd}, {xd}"));
                match (from, signed) {
                    (Type::I64, true) => self.emit(format!("cvtsi2{s}\t{xd}, {}", r64(a))),
                    (_, true) => {
                        let x = self.extend(S0, a, from, true);
                        self.emit(format!("cvtsi2{s}\t{xd}, {}", r32(x)));
                    }
                    (Type::I64, false) => {
                        // Below 2^63 the signed conversion is right. Above,
                        // halve (keeping the low bit, so rounding is still
                        // right), convert and double.
                        let (big, done) = (label("big"), label("done"));
                        if a != S0 {
                            self.emit(format!("mov\trax, {}", r64(a)));
                        }
                        self.emit("test\trax, rax");
                        self.emit(format!("js\t{big}"));
                        self.emit(format!("cvtsi2{s}\t{xd}, rax"));
                        self.emit(format!("jmp\t{done}"));
                        writeln!(self.out, "{big}:").unwrap();
                        self.emit("mov\trcx, rax");
                        self.emit("shr\trcx, 1");
                        self.emit("and\teax, 1");
                        self.emit("or\trcx, rax");
                        self.emit(format!("cvtsi2{s}\t{xd}, rcx"));
                        self.emit(format!("add{s}\t{xd}, {xd}"));
                        writeln!(self.out, "{done}:").unwrap();
                    }
                    (_, false) => {
                        let x = self.extend(S0, a, from, false);
                        // A 32-bit move zero-extends to 64.
                        self.emit(format!("mov\teax, {}", r32(x)));
                        self.emit(format!("cvtsi2{s}\t{xd}, rax"));
                    }
                }
                self.def_done(result, d);
            }
            ConvertOp::Fptosi | ConvertOp::Fptoui => {
                let d = self.def_reg(result);
                let a = self.use_freg(arg, FS0);
                self.float_to_int(op == ConvertOp::Fptosi, from, to, a, &label);
                self.emit(format!("mov\t{}, rax", r64(d)));
                self.def_done(result, d);
            }
            // The same bits in the other register class.
            ConvertOp::Bitcast => {
                let mov = if from == Type::F32 || to == Type::F32 {
                    "movd"
                } else {
                    "movq"
                };
                if to.is_float() {
                    let d = self.def_freg(result);
                    let a = self.use_reg(arg, S0);
                    self.emit(format!("{mov}\t{}, {}", xmm(d), reg(a, from)));
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    let a = self.use_freg(arg, FS0);
                    self.emit(format!("{mov}\t{}, {}", reg(d, to), xmm(a)));
                    self.def_done(result, d);
                }
            }
            _ => unreachable!("{op} is not a float conversion"),
        }
    }

    /// Float `xmm<a>` to an integer of type `to` in rax, rounding toward
    /// zero and saturating, with NaN giving 0. `cvttsd2si` itself gives
    /// the "integer indefinite", the minimum, for NaN and out-of-range
    /// values, which the compares then correct.
    fn float_to_int(
        &mut self,
        signed: bool,
        from: Type,
        to: Type,
        a: u8,
        label: &dyn Fn(&str) -> String,
    ) {
        let (s, xa) = (suffix(from), xmm(a));
        let to64 = to == Type::I64;
        // The bits of 2^n as a float of type `from`, in xmm14.
        let power = |this: &mut Self, n: i32| {
            let bits = if from == Type::F32 {
                u64::from(2f32.powi(n).to_bits())
            } else {
                2f64.powi(n).to_bits()
            };
            this.emit(format!("movabs\tr11, {bits:#x}"));
            this.emit(format!("movq\t{}, r11", xmm(FS1)));
        };
        if signed {
            let (rax, rcx) = if to64 { ("rax", "rcx") } else { ("eax", "ecx") };
            let bits = if to64 { 63 } else { 31 };
            self.emit(format!("cvtt{s}2si\t{rax}, {xa}"));
            power(self, bits);
            self.emit(format!("ucomi{s}\t{xa}, {}", xmm(FS1)));
            // At or above 2^bits (and not NaN): the maximum.
            self.emit(format!("movabs\trcx, {:#x}", (1u64 << bits) - 1));
            self.emit(format!("cmovae\t{rax}, {rcx}"));
            self.emit("mov\tecx, 0");
            self.emit(format!("cmovp\t{rax}, {rcx}"));
            return;
        }
        if !to64 {
            // Every u32 is an in-range i64. Negative (or NaN, or huge,
            // which convert to the minimum) gives 0; then clamp above.
            self.emit(format!("cvtt{s}2si\trax, {xa}"));
            self.emit("mov\tecx, 0");
            self.emit("test\trax, rax");
            self.emit("cmovs\trax, rcx");
            self.emit("mov\tecx, 0xffffffff");
            self.emit("cmp\trax, rcx");
            self.emit("cmova\trax, rcx");
            power(self, 32);
            self.emit(format!("ucomi{s}\t{xa}, {}", xmm(FS1)));
            self.emit("cmovae\trax, rcx");
            return;
        }
        // Below 2^63 (or NaN) the signed conversion, clamped at 0. At or
        // above, convert x - 2^63 and set the top bit back, unless that
        // overflows too, which is the maximum.
        let (big, max, done) = (label("big"), label("max"), label("done"));
        power(self, 63);
        self.emit(format!("ucomi{s}\t{xa}, {}", xmm(FS1)));
        self.emit(format!("jae\t{big}"));
        self.emit(format!("cvtt{s}2si\trax, {xa}"));
        self.emit("mov\tecx, 0");
        self.emit("test\trax, rax");
        self.emit("cmovs\trax, rcx");
        self.emit(format!("jmp\t{done}"));
        writeln!(self.out, "{big}:").unwrap();
        self.emit(format!("movaps\t{}, {xa}", xmm(FS2)));
        self.emit(format!("sub{s}\t{}, {}", xmm(FS2), xmm(FS1)));
        self.emit(format!("cvtt{s}2si\trax, {}", xmm(FS2)));
        self.emit("test\trax, rax");
        self.emit(format!("js\t{max}"));
        self.emit("btc\trax, 63");
        self.emit(format!("jmp\t{done}"));
        writeln!(self.out, "{max}:").unwrap();
        self.emit("mov\trax, -1");
        writeln!(self.out, "{done}:").unwrap();
    }

    /// The value and overflow flag of `op`. The value is computed in rax
    /// and the flag in cl, then both move to their places.
    fn overflow(&mut self, inst: Inst, op: OverflowOp, args: [Value; 2]) {
        let ty = self.ty(args[0]);
        let signed = matches!(op, OverflowOp::Sadd | OverflowOp::Ssub | OverflowOp::Smul);
        let a = self.use_reg(args[0], S0);
        let a = self.extend(S0, a, ty, signed);
        let b = self.use_reg(args[1], S1);
        let b = self.extend(S1, b, ty, signed);
        let width = wide_ty(ty);
        if a != S0 {
            self.emit(format!("mov\trax, {}", r64(a)));
        }
        let rb = reg(b, width);
        match op {
            OverflowOp::Umul => self.emit(format!("mul\t{rb}")),
            OverflowOp::Smul => self.emit(format!("imul\t{}, {rb}", reg(S0, width))),
            OverflowOp::Uadd | OverflowOp::Sadd => {
                self.emit(format!("add\t{}, {rb}", reg(S0, width)))
            }
            OverflowOp::Usub | OverflowOp::Ssub => {
                self.emit(format!("sub\t{}, {rb}", reg(S0, width)))
            }
        }
        match narrow_bits(ty) {
            // The 32-bit result of the extended operands overflows the
            // narrow type if it differs from its own extension.
            Some(_) => {
                let x = self.extend(S1, S0, ty, signed);
                self.emit(format!("cmp\teax, {}", r32(x)));
                self.emit("setne\tcl");
            }
            None => {
                let cc = match op {
                    OverflowOp::Uadd | OverflowOp::Usub | OverflowOp::Umul => "c",
                    _ => "o",
                };
                self.emit(format!("set{cc}\tcl"));
            }
        }
        let results = self.func.inst_results(inst);
        let (value, flag) = (results[0], results[1]);
        if let Some(loc) = self.alloc.loc(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.alloc.loc(flag) {
            self.emit_move(loc, Loc::Reg(S1));
        }
    }

    /// `lock xadd` adds and gives the old value; `xchg` exchanges. The
    /// other operations have no instruction that gives the old value, so
    /// they loop: compute the new value from the old one in rax, and
    /// `lock cmpxchg` it in if memory still holds the old one (else rax
    /// gets the current value and it tries again).
    fn atomic_rmw(&mut self, inst: Inst, op: RmwOp, args: [Value; 2], result: Option<Value>) {
        let ty = self.ty(args[1]);
        let a = self.use_reg(args[0], S2);
        let v = self.use_reg(args[1], S3);
        let at = format!("{} [{}]", ptr(ty), r64(a));
        match op {
            RmwOp::Add | RmwOp::Sub | RmwOp::Xchg => {
                self.emit(format!("mov\trax, {}", r64(v)));
                if op == RmwOp::Sub {
                    self.emit(format!("neg\t{}", reg(S0, ty)));
                }
                let mnemonic = if op == RmwOp::Xchg {
                    "xchg"
                } else {
                    "lock xadd"
                };
                self.emit(format!("{mnemonic}\t{at}, {}", reg(S0, ty)));
            }
            _ => {
                let retry = format!(".L{}_{inst}_retry", self.func.name);
                match ty {
                    Type::I8 | Type::I16 => self.emit(format!("movzx\teax, {at}")),
                    _ => self.emit(format!("mov\t{}, {at}", reg(S0, ty))),
                }
                writeln!(self.out, "{retry}:").unwrap();
                self.emit("mov\trcx, rax");
                let (rc, rv) = (reg(S1, ty), reg(v, ty));
                let w = wide_ty(ty);
                let (wc, wv) = (reg(S1, w), reg(v, w));
                match op {
                    RmwOp::And => self.emit(format!("and\t{wc}, {wv}")),
                    RmwOp::Or => self.emit(format!("or\t{wc}, {wv}")),
                    RmwOp::Xor => self.emit(format!("xor\t{wc}, {wv}")),
                    // Replace the old value by v where v wins, comparing at
                    // the type's width (`cmov` has no 8-bit form, but the
                    // low bits are what matter).
                    _ => {
                        let cc = match op {
                            RmwOp::Umin => "a",
                            RmwOp::Umax => "b",
                            RmwOp::Smin => "g",
                            _ => "l",
                        };
                        self.emit(format!("cmp\t{rc}, {rv}"));
                        self.emit(format!("cmov{cc}\t{wc}, {wv}"));
                    }
                }
                self.emit(format!("lock cmpxchg\t{at}, {rc}"));
                self.emit(format!("jne\t{retry}"));
            }
        }
        if let Some(loc) = result.and_then(|r| self.alloc.loc(r)) {
            self.emit_move(loc, Loc::Reg(S0));
        }
    }

    fn switch(&mut self, inst: Inst, arg: Value, dests: &[BlockCall], cases: &[i64]) {
        let ty = self.ty(arg);
        // Compare the zero-extended bits with the cases as unsigned bits.
        let mask = mask(ty);
        let x = self.use_reg(arg, S0);
        let x = self.extend(S0, x, ty, false);
        let width = wide_ty(ty);
        let values: Vec<u64> = cases.iter().map(|&c| c as u64 & mask).collect();
        let targets: Vec<String> = dests
            .iter()
            .enumerate()
            .map(|(i, d)| match d.args.is_empty() {
                true => self.label(d.block),
                false => format!(".L{}_{inst}_edge{i}", self.func.name),
            })
            .collect();

        let (min, max) = match (values.iter().min(), values.iter().max()) {
            (Some(&min), Some(&max)) => (min, max),
            _ => (0, 0),
        };
        let span = max - min;
        // A value that fits a sign-extended 32-bit immediate.
        let imm = |this: &mut Self, value: u64| -> String {
            if width == Type::I64 && i32::try_from(value as i64).is_err() {
                this.emit(format!("movabs\trcx, {value:#x}"));
                "rcx".into()
            } else {
                (value as i64).to_string()
            }
        };
        if values.len() >= 4 && span < 3 * values.len() as u64 && span < 4096 {
            // idx = x - min, unsigned; anything above the span is default.
            // A 32-bit index is zero-extended by writing it.
            let rax = reg(S0, width);
            if min == 0 {
                self.emit(format!("mov\t{rax}, {}", reg(x, width)));
            } else {
                let sub = imm(self, min);
                if x != S0 {
                    self.emit(format!("mov\t{rax}, {}", reg(x, width)));
                }
                self.emit(format!("sub\t{rax}, {sub}"));
            }
            self.emit(format!("cmp\t{rax}, {span}"));
            self.emit(format!("ja\t{}", targets[0]));
            let table = format!(".L{}_{inst}_table", self.func.name);
            // Offsets from the table to each target, so the table needs no
            // relocations: position independent.
            self.emit(format!("lea\trcx, [rip + {table}]"));
            self.emit("movsxd\trax, dword ptr [rcx + 4*rax]");
            self.emit("add\trax, rcx");
            self.emit("jmp\trax");
            writeln!(self.out, "\t.p2align\t2\n{table}:").unwrap();
            for k in 0..=span {
                let i = values
                    .iter()
                    .position(|&v| v == min + k)
                    .map_or(0, |i| i + 1);
                self.emit(format!(".long\t{} - {table}", targets[i]));
            }
        } else {
            for (i, &value) in values.iter().enumerate() {
                let operand = imm(self, value);
                self.emit(format!("cmp\t{}, {operand}", reg(x, width)));
                self.emit(format!("je\t{}", targets[i + 1]));
            }
            if dests[0].args.is_empty() {
                self.branch_unless_next(dests[0].block);
            } else {
                self.emit(format!("jmp\t{}", targets[0]));
            }
        }
        for (dest, target) in dests.iter().zip(&targets) {
            if !dest.args.is_empty() {
                writeln!(self.out, "{target}:").unwrap();
                self.block_args(dest);
                self.emit(format!("jmp\t{}", self.label(dest.block)));
            }
        }
    }

    /// Emits the compare of `icmp`/`fcmp` `inst`, returning the condition
    /// code that holds when its condition does.
    ///
    /// `ucomisd a, b` sets ZF, PF and CF like an unsigned compare, and all
    /// three for unordered (NaN). So `a > b` is `a` (above: CF and ZF
    /// clear), false for NaN, and `a < b` is `b > a`. Equality needs two
    /// flags (ZF set, PF clear), combined into al and tested.
    fn compare(&mut self, inst: Inst) -> &'static str {
        match *self.func.inst_data(inst) {
            InstData::Icmp { cond, args } => {
                let ty = self.ty(args[0]);
                let a = self.use_reg(args[0], S0);
                let b = match folded_imm(self.func, self.func.inst_data(inst)) {
                    Some((_, k)) => k.to_string(),
                    None => reg(self.use_reg(args[1], S1), ty).to_string(),
                };
                self.emit(format!("cmp\t{}, {b}", reg(a, ty)));
                cond_code(cond)
            }
            InstData::Fcmp { cond, args } => {
                let ty = self.ty(args[0]);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let (xa, xb) = (xmm(a), xmm(b));
                let ucomi = format!("ucomi{}", suffix(ty));
                let (first, second, cc) = match cond {
                    FloatCC::Gt => (xa, xb, "a"),
                    FloatCC::Ge => (xa, xb, "ae"),
                    FloatCC::Lt => (xb, xa, "a"),
                    FloatCC::Le => (xb, xa, "ae"),
                    FloatCC::Ord => (xa, xb, "np"),
                    FloatCC::Uno => (xa, xb, "p"),
                    FloatCC::Eq | FloatCC::Ne => {
                        self.emit(format!("{ucomi}\t{xa}, {xb}"));
                        if cond == FloatCC::Eq {
                            self.emit("sete\tal");
                            self.emit("setnp\tcl");
                            self.emit("and\tal, cl");
                        } else {
                            self.emit("setne\tal");
                            self.emit("setp\tcl");
                            self.emit("or\tal, cl");
                        }
                        self.emit("test\tal, al");
                        return "ne";
                    }
                };
                self.emit(format!("{ucomi}\t{first}, {second}"));
                cc
            }
            _ => unreachable!("not a compare"),
        }
    }

    /// Sets flags so that `ne` means `value` is non-zero, at its width; an
    /// `i128` is non-zero if either half is.
    fn test_value(&mut self, value: Value) {
        let ty = self.ty(value);
        if ty == Type::I128 {
            let [lo, hi] = self.halves(value);
            self.emit(format!("mov\trax, {lo}"));
            self.emit(format!("or\trax, {hi}"));
            return;
        }
        let c = self.use_reg(value, S0);
        let r = reg(c, ty);
        self.emit(format!("test\t{r}, {r}"));
    }

    /// Copies block-call arguments into the target's parameters, all at
    /// once (`jump block1(v4, v3)` may swap two parameters).
    fn block_args(&mut self, call: &BlockCall) {
        let params = self.func.block_params(call.block);
        let mut moves = Vec::new();
        for (&param, &arg) in params.iter().zip(&call.args) {
            // A parameter with no location is an alias, never read.
            let Some(dst) = self.alloc.loc(param) else {
                continue;
            };
            moves.push((dst, self.loc(arg)));
            if let Some(hi) = self.alloc.hi_loc(param) {
                moves.push((hi, self.hi(arg)));
            }
        }
        self.parallel_copy(&moves);
    }

    /// Emits `moves`, each `(dst, src)`, as if they all happened at once.
    /// r11 breaks cycles, of either register class (`movq` moves bits
    /// between classes); `emit_move` uses rax for slot-to-slot moves.
    fn parallel_copy(&mut self, moves: &[(Loc, Loc)]) {
        for (dst, src) in sequentialize(moves, Loc::Reg(S3)) {
            self.emit_move(dst, src);
        }
    }

    /// One 64-bit move between any two locations. Narrow values have
    /// unspecified upper bits, so copying all 64 is always fine.
    fn emit_move(&mut self, dst: Loc, src: Loc) {
        match (dst, src) {
            _ if dst == src => {}
            (Loc::Reg(d), Loc::Reg(s)) => self.emit(format!("mov\t{}, {}", r64(d), r64(s))),
            (Loc::FReg(d), Loc::FReg(s)) => self.emit(format!("movaps\t{}, {}", xmm(d), xmm(s))),
            (Loc::Reg(d), Loc::FReg(s)) => self.emit(format!("movq\t{}, {}", r64(d), xmm(s))),
            (Loc::FReg(d), Loc::Reg(s)) => self.emit(format!("movq\t{}, {}", xmm(d), r64(s))),
            (Loc::Reg(d), Loc::Stack(s)) => self.emit(format!("mov\t{}, {}", r64(d), self.slot(s))),
            (Loc::FReg(d), Loc::Stack(s)) => {
                self.emit(format!("movsd\t{}, {}", xmm(d), self.slot(s)))
            }
            (Loc::Stack(d), Loc::Reg(s)) => self.emit(format!("mov\t{}, {}", self.slot(d), r64(s))),
            (Loc::Stack(d), Loc::FReg(s)) => {
                self.emit(format!("movsd\t{}, {}", self.slot(d), xmm(s)))
            }
            (Loc::Stack(d), Loc::Stack(s)) => {
                self.emit(format!("mov\trax, {}", self.slot(s)));
                self.emit(format!("mov\t{}, rax", self.slot(d)));
            }
        }
    }

    fn branch_unless_next(&mut self, target: Block) {
        if self.next_block != Some(target) {
            self.emit(format!("jmp\t{}", self.label(target)));
        }
    }

    /// Sign- or zero-extends a narrow value in `src` to 32 bits in
    /// `scratch` and returns `scratch`; returns `src` unchanged for 32-
    /// and 64-bit types. Extending in place would clobber the operand's
    /// register.
    fn extend(&mut self, scratch: u8, src: u8, ty: Type, signed: bool) -> u8 {
        if narrow_bits(ty).is_none() {
            return src;
        }
        let op = if signed { "movsx" } else { "movzx" };
        self.emit(format!("{op}\t{}, {}", r32(scratch), reg(src, ty)));
        scratch
    }

    /// Loads constant `imm` of type `ty`: a 32-bit `mov` zero-extends, a
    /// 64-bit one sign-extends its 32-bit immediate, and `movabs` takes
    /// any 64 bits.
    fn materialize(&mut self, d: u8, imm: i64, ty: Type) {
        let bits = imm as u64 & mask(ty);
        if bits == 0 {
            self.emit(format!("xor\t{0}, {0}", r32(d)));
        } else if bits <= u64::from(u32::MAX) {
            self.emit(format!("mov\t{}, {bits:#x}", r32(d)));
        } else if i32::try_from(bits as i64).is_ok() {
            self.emit(format!("mov\t{}, {}", r64(d), bits as i64));
        } else {
            self.emit(format!("movabs\t{}, {bits:#x}", r64(d)));
        }
    }

    fn trap_label(&mut self) -> String {
        self.uses_trap = true;
        format!(".L{}_trap", self.func.name)
    }

    /// The location of an operand.
    fn loc(&self, value: Value) -> Loc {
        self.alloc
            .loc(self.func.resolve(value))
            .expect("operand in reachable code is allocated")
    }

    /// The memory operand for an access of type `ty` at `addr`: the stack
    /// slot itself if `addr` is one's address, else the address in a
    /// register.
    fn mem(&mut self, addr: Value, scratch: u8, ty: Type) -> String {
        if let Some(slot) = stack_slot(self.func, addr) {
            return format!("{} {}", ptr(ty), self.frame(self.stack_slot_offsets[slot]));
        }
        format!("{} [{}]", ptr(ty), r64(self.use_reg(addr, scratch)))
    }

    /// The register holding operand `value`: its own, or `scratch` after
    /// reloading it from its spill slot.
    fn use_reg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::Reg(r) => r,
            Loc::Stack(s) => {
                self.emit(format!("mov\t{}, {}", r64(scratch), self.slot(s)));
                scratch
            }
            Loc::FReg(_) => unreachable!("{value} is a float"),
        }
    }

    /// [`Self::use_reg`] for floats.
    fn use_freg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::FReg(r) => r,
            Loc::Stack(s) => {
                self.emit(format!("movsd\t{}, {}", xmm(scratch), self.slot(s)));
                scratch
            }
            Loc::Reg(_) => unreachable!("{value} is not a float"),
        }
    }

    /// The register to compute `result` into: its own, or rax if it is
    /// spilled (or an alias, whose value is never read).
    fn def_reg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.alloc.loc(value) {
            Some(Loc::Reg(r)) => r,
            _ => S0,
        }
    }

    /// [`Self::def_reg`] for floats: the result's register, or xmm13.
    fn def_freg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.alloc.loc(value) {
            Some(Loc::FReg(r)) => r,
            _ => FS0,
        }
    }

    /// Stores a spilled result computed into `reg` by [`Self::def_reg`] or
    /// [`Self::def_freg`].
    fn def_done(&mut self, result: Option<Value>, reg: u8) {
        let Some(value) = result else { return };
        if let Some(Loc::Stack(s)) = self.alloc.loc(value) {
            if self.func.value_type(value).is_float() {
                self.emit(format!("movsd\t{}, {}", self.slot(s), xmm(reg)));
            } else {
                self.emit(format!("mov\t{}, {}", self.slot(s), r64(reg)));
            }
        }
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(self.func.resolve(value))
    }

    /// The fixed frame at `offset` from `rsp` after the prologue, from rbp,
    /// which stays put if `rsp` moves.
    fn frame(&self, offset: usize) -> String {
        let top = self.frame_size + 8 * self.saved.len();
        format!("[rbp - {}]", top - offset)
    }

    /// Spill slot `index`, as a 64-bit memory operand.
    fn slot(&self, index: u32) -> String {
        format!(
            "qword ptr {}",
            self.frame(self.spill_base + 8 * index as usize)
        )
    }

    fn label(&self, block: Block) -> String {
        format!(".L{}_{block}", self.func.name)
    }

    fn emit(&mut self, line: impl AsRef<str>) {
        writeln!(self.out, "\t{}", line.as_ref()).unwrap();
    }
}

/// The second operand of a two-address instruction.
#[derive(Copy, Clone)]
enum Operand {
    Reg(u8),
    Imm(i64),
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

const R64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];
const R32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d",
];
const R16: [&str; 16] = [
    "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w",
    "r14w", "r15w",
];
const R8: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b",
    "r13b", "r14b", "r15b",
];

fn r64(n: u8) -> &'static str {
    R64[n as usize]
}

fn r32(n: u8) -> &'static str {
    R32[n as usize]
}

fn r16(n: u8) -> &'static str {
    R16[n as usize]
}

fn r8(n: u8) -> &'static str {
    R8[n as usize]
}

/// Register `n` at the width of `ty`.
fn reg(n: u8, ty: Type) -> &'static str {
    match ty {
        Type::I8 => r8(n),
        Type::I16 => r16(n),
        Type::I32 | Type::F32 => r32(n),
        _ => r64(n),
    }
}

/// The width integer arithmetic on `ty` runs at: 64 bits for 64-bit
/// types, else 32.
fn wide_ty(ty: Type) -> Type {
    match ty {
        Type::I64 | Type::Ptr => Type::I64,
        _ => Type::I32,
    }
}

/// Register `n` at the width arithmetic on `ty` runs at.
fn wide(n: u8, ty: Type) -> &'static str {
    reg(n, wide_ty(ty))
}

fn xmm(n: u8) -> String {
    format!("xmm{n}")
}

/// The size keyword of a memory operand of type `ty`.
fn ptr(ty: Type) -> &'static str {
    match ty {
        Type::I8 => "byte ptr",
        Type::I16 => "word ptr",
        Type::I32 | Type::F32 => "dword ptr",
        _ => "qword ptr",
    }
}

/// The scalar SSE suffix: `ss` for f32, `sd` for f64.
fn suffix(ty: Type) -> &'static str {
    if ty == Type::F32 { "ss" } else { "sd" }
}

/// The scalar float move.
fn movs(ty: Type) -> &'static str {
    if ty == Type::F32 { "movss" } else { "movsd" }
}

fn narrow_bits(ty: Type) -> Option<u32> {
    match ty {
        Type::I8 => Some(8),
        Type::I16 => Some(16),
        _ => None,
    }
}

/// The bits of a value of integer type `ty`.
fn mask(ty: Type) -> u64 {
    u64::MAX >> (64 - 8 * ty.bytes().unwrap_or(8).min(8))
}

/// The most negative value of `ty`, sign-extended to 64 bits.
fn min_value(ty: Type) -> u64 {
    let bits = 8 * ty.bytes().unwrap_or(8);
    u64::MAX << (bits - 1)
}

fn cond_code(cond: IntCC) -> &'static str {
    match cond {
        IntCC::Eq => "e",
        IntCC::Ne => "ne",
        IntCC::Slt => "l",
        IntCC::Sle => "le",
        IntCC::Sgt => "g",
        IntCC::Sge => "ge",
        IntCC::Ult => "b",
        IntCC::Ule => "be",
        IntCC::Ugt => "a",
        IntCC::Uge => "ae",
    }
}

/// The condition code that holds when `cond` does not.
fn invert_cond(cond: &str) -> &'static str {
    match cond {
        "e" => "ne",
        "ne" => "e",
        "l" => "ge",
        "ge" => "l",
        "le" => "g",
        "g" => "le",
        "b" => "ae",
        "ae" => "b",
        "be" => "a",
        "a" => "be",
        "p" => "np",
        "np" => "p",
        _ => panic!("unknown condition `{cond}`"),
    }
}
