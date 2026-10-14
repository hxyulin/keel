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
//! The prologue takes the first location in the function with column 0,
//! and the first located instruction is marked `prologue_end`, where a
//! debugger stops for a breakpoint on the function. An instruction
//! without a location continues the previous one's. A minimal
//! `__debug_info` (a compile unit and a subprogram per function) makes
//! the linker record the object in the executable's debug map, which is
//! how lldb finds the line table.

use crate::CodegenError;
use crate::abi::{self, ArgKind, ArgLoc, Slot, StructClass};
use crate::parallel_copy::sequentialize;
use crate::regalloc::{self, Allocation, Loc, Pools};
use keel_ir::{
    AbiParam, ArgExt, BinaryOp, Block, BlockCall, CallConv, ConvertOp, DataInit, DataItem,
    DataObject, EntityRef, FloatCC, Function, Inst, InstData, IntCC, Intrinsic, Layout, Module,
    Ordering, OverflowOp, Purpose, RmwOp, Signature, SourceFile, SourceLoc, Type, UnaryOp, Value,
};
use std::fmt::Write;

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

/// Compiles a module into one assembly file for `clang`/`as`. Declared
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
    let mut out = String::from("\t.section\t__TEXT,__text,regular,pure_instructions\n");
    let debug = !module.files.is_empty() && !module.functions.is_empty();
    if debug {
        for (i, path) in module.files.iter().enumerate() {
            writeln!(out, "\t.file\t{} {}", i + 1, asm_string(path)).unwrap();
        }
    }
    for func in &module.functions {
        let mut compiler = FunctionCompiler::new(func, module, pools)?;
        compiler.debug = debug;
        compiler.compile(&mut out);
    }
    for data in &module.data {
        if let Some(init) = &data.init {
            emit_data(&mut out, data, init);
        }
    }
    if debug {
        emit_debug_info(&mut out, module);
    }
    out.push_str("\t.subsections_via_symbols\n");
    Ok(out)
}

/// Instructions that clobber the caller-saved registers: calls, the memory
/// intrinsics, which are calls to libc, and `i128` division, a call to
/// compiler-rt.
fn is_call(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { .. } | InstData::CallIndirect { .. } => true,
        InstData::Binary { op, args } => {
            op.can_trap() && func.value_type(func.resolve(args[0])) == Type::I128
        }
        InstData::Intrinsic { id, .. } => match id {
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset => true,
            Intrinsic::Debugtrap
            | Intrinsic::VaStart
            | Intrinsic::StackAlloc
            | Intrinsic::StackSave
            | Intrinsic::StackRestore => false,
        },
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

struct FunctionCompiler<'a> {
    func: &'a Function,
    module: &'a Module,
    alloc: Allocation,
    /// Offset from `sp` of each IR stack slot.
    stack_slot_offsets: Vec<usize>,
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
    /// The register the fixed frame is addressed from: `sp`, or x19 in a
    /// function that moves `sp`.
    base: &'static str,
    /// Offset from x29 of the first variadic argument: the next 8-byte slot
    /// after the named stack parameters.
    variadic_offset: u32,
    /// Outgoing arguments, spill slots and IR stack slots; the
    /// callee-saved area is separate.
    frame_size: usize,
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
}

impl<'a> FunctionCompiler<'a> {
    fn new(func: &'a Function, module: &'a Module, pools: &Pools) -> Result<Self, CodegenError> {
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        let CallConv::C = func.sig.cc;
        let param_kinds = param_kinds(&func.sig.params);
        let (param_locs, incoming) = abi::arg_locs(&param_kinds, param_kinds.len());
        if incoming > MAX_STACK_ARGS {
            return Err(err(format!(
                "{incoming} bytes of stack parameters is more than the {MAX_STACK_ARGS} supported"
            )));
        }
        match func.sig.returns.as_slice() {
            [] => {}
            // The Apple ABI has the callee extend these to 32 bits, and every
            // function is exported, so C may be relying on it.
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
        for (_, callee) in func.callees() {
            let what = format!("call to @{}", callee.name);
            check_call_signature(&what, &callee.sig, module.is_external(&callee.name))
                .map_err(err)?;
        }
        // The target of an indirect call may be an external function.
        for (_, sig) in func.signatures() {
            check_call_signature(&format!("indirect call {sig}"), sig, true).map_err(err)?;
        }
        for (_, inst) in func.layout_insts() {
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

        // The bottom of the frame holds what calls need: their stack
        // arguments, copies of big structs passed by value, and a slot for
        // an `sret` address.
        let (mut outgoing, mut copies, mut needs_sret_save) = (0, 0u64, false);
        for (_, inst) in func.layout_insts() {
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

        let moves_sp = func.layout_insts().any(|(_, inst)| {
            matches!(
                func.inst_data(inst),
                InstData::Intrinsic {
                    id: Intrinsic::StackAlloc | Intrinsic::StackRestore,
                    ..
                }
            )
        });
        let callee_saved: Vec<u8> = (pools.callee_saved.iter().copied())
            .filter(|&r| !moves_sp || r != BASE)
            .collect();
        let pools = Pools {
            callee_saved: &callee_saved,
            ..*pools
        };
        let mut alloc = regalloc::allocate(func, &pools, |data| is_call(func, data));
        if moves_sp {
            alloc.callee_saved_used.insert(0, BASE);
        }
        let mut offset = spill_base + 8 * alloc.spill_slots as usize;
        if offset > MAX_SLOT_OFFSET {
            return Err(err(format!(
                "{offset} bytes of spill slots is too many for this allocator"
            )));
        }
        // `sp` is 16-byte aligned and slot alignment is at most 16, so an
        // aligned offset gives an aligned address.
        let mut stack_slot_offsets = Vec::new();
        for (_, slot) in func.stack_slots() {
            offset = offset.next_multiple_of(slot.align as usize);
            stack_slot_offsets.push(offset);
            offset += slot.size as usize;
        }
        let frame_size = offset.next_multiple_of(16);
        if frame_size > MAX_FRAME_SIZE {
            return Err(err(format!("frame of {frame_size} bytes is too large")));
        }
        Ok(Self {
            func,
            module,
            alloc,
            stack_slot_offsets,
            copy_base,
            sret_save,
            param_areas,
            spill_base,
            base: if moves_sp { "x19" } else { "sp" },
            variadic_offset: 16 + incoming.next_multiple_of(8),
            frame_size,
            next_block: None,
            out: String::new(),
            uses_trap: false,
            debug: false,
            srcloc: None,
            prologue_end: false,
        })
    }

    fn compile(mut self, out: &mut String) {
        let func = self.func;
        let name = &func.name;
        writeln!(
            out,
            "\t.globl\t_{name}\n\t.p2align\t2\n_{name}:\nL{name}_begin:"
        )
        .unwrap();
        self.allocation_comment();

        if self.debug {
            let first = func
                .layout_insts()
                .find_map(|(_, inst)| func.inst_loc(inst));
            let file = first.map_or(SourceFile::new(0), |loc| loc.file);
            let line = first.map_or(0, |loc| loc.line);
            self.set_srcloc(Some(SourceLoc { file, line, col: 0 }));
            self.prologue_end = true;
        }
        self.emit(".cfi_startproc");
        self.emit("stp\tx29, x30, [sp, #-16]!");
        self.emit("mov\tx29, sp");
        self.emit(".cfi_def_cfa x29, 16");
        self.emit(".cfi_offset x30, -8");
        self.emit(".cfi_offset x29, -16");
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
                    self.emit(format!("stp\t{class}{a}, {class}{b}, [sp, #-16]!"));
                    self.emit(format!(".cfi_offset {class}{a}, -{offset}"));
                    self.emit(format!(".cfi_offset {class}{b}, -{}", offset - 8));
                }
                [a] => {
                    self.emit(format!("str\t{class}{a}, [sp, #-16]!"));
                    self.emit(format!(".cfi_offset {class}{a}, -{offset}"));
                }
                _ => unreachable!(),
            }
        }
        if self.frame_size > 0 {
            self.sub_sp(self.frame_size);
        }
        if self.base != "sp" {
            self.emit(format!("mov\t{}, sp", self.base));
        }
        // Arguments arrive in x0–x7 and v0–v7, which are never allocated,
        // so this copy has no cycles; it may still store to spill slots.
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
                let Some(dst) = self.alloc.loc(param) else {
                    continue;
                };
                let abi_param = &func.sig.params[i];
                let hi = self.alloc.hi_loc(param);
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

        // Only reachable blocks, in the order the allocator numbered them.
        let order = self.alloc.order.clone();
        for (i, &block) in order.iter().enumerate() {
            self.next_block = order.get(i + 1).copied();
            writeln!(self.out, "{}:", self.label(block)).unwrap();
            for &inst in func.block_insts(block) {
                writeln!(self.out, "\t; {}", func.display_inst(inst)).unwrap();
                if self.debug {
                    self.set_srcloc(func.inst_loc(inst));
                }
                self.inst(inst);
            }
        }
        if self.uses_trap {
            writeln!(self.out, "L{name}_trap:").unwrap();
            self.emit("brk\t#1");
        }
        writeln!(self.out, "L{name}_end:").unwrap();
        self.emit(".cfi_endproc");
        out.push_str(&self.out);
    }

    /// Emits a `.loc` for the code that follows, unless it would repeat
    /// the last one. `None` keeps the last one.
    fn set_srcloc(&mut self, loc: Option<SourceLoc>) {
        let Some(loc) = loc else { return };
        if self.srcloc == Some(loc) && !self.prologue_end {
            return;
        }
        let SourceLoc { file, line, col } = loc;
        let flag = if self.prologue_end {
            " prologue_end"
        } else {
            ""
        };
        self.emit(format!(".loc\t{} {line} {col}{flag}", file.index() + 1));
        self.srcloc = Some(loc);
        self.prologue_end = false;
    }

    /// A comment listing where each value lives, to make the output
    /// readable.
    fn allocation_comment(&mut self) {
        let mut line = String::from("\t; regalloc:");
        for value in (0..self.func.num_values()).map(Value::new) {
            if let Some(loc) = self.alloc.loc(value) {
                write!(line, " {value}={}", self.loc_name(loc)).unwrap();
            }
            if let Some(hi) = self.alloc.hi_loc(value) {
                write!(line, ":{}", self.loc_name(hi)).unwrap();
            }
        }
        writeln!(self.out, "{line}").unwrap();
    }

    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
        if self.lower_i128(inst, func.inst_data(inst), result) {
            return;
        }
        match func.inst_data(inst).clone() {
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
                    self.emit(format!("fmov\t{fd}, {zero}"));
                } else {
                    let is64 = ty == Type::F64;
                    self.materialize(S0, bits, is64);
                    let src = if is64 {
                        format!("x{S0}")
                    } else {
                        format!("w{S0}")
                    };
                    self.emit(format!("fmov\t{fd}, {src}"));
                }
                self.def_done(result, d);
            }
            InstData::Call { func: callee, args } => {
                let decl = func.callee(callee);
                let sret = self.move_args(&args, &decl.sig.params);
                self.emit(format!("bl\t_{}", decl.name));
                self.call_result(result, sret);
            }
            InstData::CallIndirect { sig, callee, args } => {
                let sret = self.move_args(&args, &func.signature(sig).params);
                // The callee is in an allocated register, which the argument
                // moves leave alone (they write x0–x8, x16 and x17), or in a
                // spill slot, reloaded after the moves so x16 is free.
                let target = self.use_reg(callee, S0);
                self.emit(format!("blr\tx{target}"));
                self.call_result(result, sret);
            }
            InstData::SymbolAddr { symbol } => {
                let d = self.def_reg(result);
                let name = func.symbol_name(symbol);
                let thread_local = self
                    .module
                    .data_object(name)
                    .is_some_and(|d| d.thread_local);
                if thread_local {
                    // The symbol is the variable's descriptor; calling its
                    // getter with the descriptor in x0 returns the address
                    // in x0. The getter preserves every register but x0,
                    // x16, x17 and x30, none of which hold values.
                    self.emit(format!("adrp\tx0, _{name}@TLVPPAGE"));
                    self.emit(format!("ldr\tx0, [x0, _{name}@TLVPPAGEOFF]"));
                    self.emit(format!("ldr\tx{S0}, [x0]"));
                    self.emit(format!("blr\tx{S0}"));
                    self.emit(format!("mov\tx{d}, x0"));
                } else if self.module.defines(name) {
                    self.emit(format!("adrp\tx{d}, _{name}@PAGE"));
                    self.emit(format!("add\tx{d}, x{d}, _{name}@PAGEOFF"));
                } else {
                    self.emit(format!("adrp\tx{d}, _{name}@GOTPAGE"));
                    self.emit(format!("ldr\tx{d}, [x{d}, _{name}@GOTPAGEOFF]"));
                }
                self.def_done(result, d);
            }
            InstData::StackAddr { slot } => {
                let d = self.def_reg(result);
                let offset = self.stack_slot_offsets[slot.index()];
                // `add` takes a 12-bit immediate, optionally shifted by 12.
                self.emit(format!("add\tx{d}, {}, #{}", self.base, offset & 0xfff));
                if offset >> 12 != 0 {
                    self.emit(format!("add\tx{d}, x{d}, #{}, lsl #12", offset >> 12));
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
                self.emit(format!("{mnemonic}\t{}, {}", freg(d, ty), freg(a, ty)));
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
                    (31, format!("w{S0}"), format!("w{S1}"))
                } else {
                    (63, format!("x{S0}"), format!("x{S1}"))
                };
                self.emit(format!("fmov\t{ra}, {}", freg(a, ty)));
                self.emit(format!("fmov\t{rb}, {}", freg(b, ty)));
                self.emit(format!("bfxil\t{rb}, {ra}, #0, #{bits}"));
                self.emit(format!("fmov\t{}, {rb}", freg(d, ty)));
                self.def_done(result, d);
            }
            InstData::Fma { args } => {
                let ty = self.ty(args[0]);
                let d = self.def_freg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                let c = self.use_freg(args[2], FS2);
                let [fa, fb, fc, fd] = [a, b, c, d].map(|r| freg(r, ty));
                self.emit(format!("fmadd\t{fd}, {fa}, {fb}, {fc}"));
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
                self.emit(format!("{mnemonic}\t{fd}, {fa}, {fb}"));
                self.def_done(result, d);
            }
            InstData::Binary { op, args } => {
                let d = self.def_reg(result);
                self.binary(op, args, d);
                self.def_done(result, d);
            }
            InstData::Select { args } if self.ty(args[1]).is_float() => {
                let ty = self.ty(args[1]);
                let d = self.def_freg(result);
                self.test_value(args[0]);
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let (a, b, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.emit(format!("fcsel\t{fd}, {a}, {b}, ne"));
                self.def_done(result, d);
            }
            InstData::Select { args } => {
                let d = self.def_reg(result);
                let ty = self.ty(args[1]);
                self.test_value(args[0]);
                let a = self.use_reg(args[1], S1);
                let b = self.use_reg(args[2], S2);
                let (a, b, rd) = (reg(a, ty), reg(b, ty), reg(d, ty));
                self.emit(format!("csel\t{rd}, {a}, {b}, ne"));
                self.def_done(result, d);
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
                        Type::I8 => self.emit(format!("sxtb\t{}, w{a}", reg(d, ty))),
                        Type::I16 => self.emit(format!("sxth\t{}, w{a}", reg(d, ty))),
                        _ => self.emit(format!("sxtw\tx{d}, w{a}")),
                    },
                    // Writing a w register clears the upper 32 bits.
                    ConvertOp::Uext => match from {
                        Type::I8 => self.emit(format!("uxtb\tw{d}, w{a}")),
                        Type::I16 => self.emit(format!("uxth\tw{d}, w{a}")),
                        _ => self.emit(format!("mov\tw{d}, w{a}")),
                    },
                    // The low bits are already right; upper bits are
                    // unspecified. A pointer is a 64-bit address already.
                    // Either way the bits just move.
                    ConvertOp::Trunc | ConvertOp::PtrToInt | ConvertOp::IntToPtr => {
                        if a != d {
                            self.emit(format!("mov\tx{d}, x{a}"));
                        }
                    }
                    _ => unreachable!("float conversions are lowered by float_convert"),
                }
                self.def_done(result, d);
            }
            InstData::Icmp { cond, args } => {
                let d = self.def_reg(result);
                let ty = self.ty(args[0]);
                let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
                let a = self.use_reg(args[0], S0);
                let a = self.extend(S0, a, ty, signed);
                let b = self.use_reg(args[1], S1);
                let b = self.extend(S1, b, ty, signed);
                self.emit(format!("cmp\t{}, {}", reg(a, ty), reg(b, ty)));
                self.emit(format!("cset\tw{d}, {}", cond_code(cond)));
                self.def_done(result, d);
            }
            InstData::Fcmp { cond, args } => {
                let ty = self.ty(args[0]);
                let d = self.def_reg(result);
                let a = self.use_freg(args[0], FS0);
                let b = self.use_freg(args[1], FS1);
                self.emit(format!("fcmp\t{}, {}", freg(a, ty), freg(b, ty)));
                self.emit(format!("cset\tw{d}, {}", float_cond_code(cond)));
                self.def_done(result, d);
            }
            // Load the pointer, store it advanced by a slot, then read the
            // argument: the result may share the list's register.
            InstData::VaArg { ty, list } => {
                let a = self.use_reg(list, S2);
                self.emit(format!("ldr\tx{S0}, [x{a}]"));
                self.emit(format!("add\tx{S1}, x{S0}, #8"));
                self.emit(format!("str\tx{S1}, [x{a}]"));
                if ty.is_float() {
                    let d = self.def_freg(result);
                    self.emit(format!("ldr\t{}, [x{S0}]", freg(d, ty)));
                    self.def_done(result, d);
                } else {
                    let d = self.def_reg(result);
                    self.emit(format!("ldr\t{}, [x{S0}]", reg(d, ty)));
                    self.def_done(result, d);
                }
            }
            InstData::Load { ty, addr } if ty.is_float() => {
                let d = self.def_freg(result);
                let a = self.use_reg(addr, S0);
                self.emit(format!("ldr\t{}, [x{a}]", freg(d, ty)));
                self.def_done(result, d);
            }
            InstData::Load { ty, addr } => {
                let d = self.def_reg(result);
                let a = self.use_reg(addr, S0);
                let op = match ty {
                    Type::I8 => "ldrb",
                    Type::I16 => "ldrh",
                    _ => "ldr",
                };
                self.emit(format!("{op}\t{}, [x{a}]", reg(d, ty)));
                self.def_done(result, d);
            }
            InstData::Store { value, addr } if self.ty(value).is_float() => {
                let ty = self.ty(value);
                let v = self.use_freg(value, FS0);
                let a = self.use_reg(addr, S1);
                self.emit(format!("str\t{}, [x{a}]", freg(v, ty)));
            }
            InstData::Store { value, addr } => {
                let ty = self.ty(value);
                let v = self.use_reg(value, S0);
                let a = self.use_reg(addr, S1);
                let op = match ty {
                    Type::I8 => "strb",
                    Type::I16 => "strh",
                    _ => "str",
                };
                self.emit(format!("{op}\t{}, [x{a}]", reg(v, ty)));
            }
            InstData::AtomicLoad { ty, ordering, addr } => {
                let d = self.def_reg(result);
                let a = self.use_reg(addr, S0);
                let op = if ordering == Ordering::Relaxed {
                    "ldr"
                } else {
                    "ldar"
                };
                self.emit(format!("{op}{}\t{}, [x{a}]", width_suffix(ty), reg(d, ty)));
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
                self.emit(format!("{op}{}\t{}, [x{a}]", width_suffix(ty), reg(v, ty)));
            }
            InstData::AtomicRmw { op, ordering, args } => {
                let ty = self.ty(args[1]);
                let a = self.use_reg(args[0], S0);
                let mut v = self.use_reg(args[1], S1);
                // LSE has no subtract or and: add the negation, clear the
                // complement.
                let mnemonic = match op {
                    RmwOp::Sub => {
                        self.emit(format!("neg\t{}, {}", reg(S1, ty), reg(v, ty)));
                        v = S1;
                        "ldadd"
                    }
                    RmwOp::And => {
                        self.emit(format!("mvn\t{}, {}", reg(S1, ty), reg(v, ty)));
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
                self.emit(format!(
                    "{mnemonic}{order}{}\t{rv}, {rd}, [x{a}]",
                    width_suffix(ty)
                ));
                if let Some(loc) = result.and_then(|r| self.alloc.loc(r)) {
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
                self.emit(format!("mov\tx{S2}, x{expected}"));
                let new = self.use_reg(args[2], S1);
                let order = ordering_suffix(ordering);
                let (rs, rt) = (reg(S2, ty), reg(new, ty));
                self.emit(format!(
                    "cas{order}{}\t{rs}, {rt}, [x{a}]",
                    width_suffix(ty)
                ));
                if let Some(loc) = result.and_then(|r| self.alloc.loc(r)) {
                    self.emit_move(loc, Loc::Reg(S2));
                }
            }
            InstData::Fence { ordering } => {
                let barrier = if ordering == Ordering::Acquire {
                    "dmb\tishld"
                } else {
                    "dmb\tish"
                };
                self.emit(barrier);
            }
            InstData::Jump { dest } => {
                self.block_args(&dest);
                self.branch_unless_next(dest.block);
            }
            InstData::Brif {
                cond,
                dests: [then_dest, else_dest],
            } => {
                self.test_value(cond);
                // Block arguments are copied on the edge, so a branch with
                // else-arguments needs its own landing pad. The copies only
                // move data, so the flags survive them.
                if else_dest.args.is_empty() {
                    self.emit(format!("b.eq\t{}", self.label(else_dest.block)));
                    self.block_args(&then_dest);
                    self.branch_unless_next(then_dest.block);
                } else {
                    let pad = format!("L{}_{inst}_else", func.name);
                    self.emit(format!("b.eq\t{pad}"));
                    self.block_args(&then_dest);
                    self.emit(format!("b\t{}", self.label(then_dest.block)));
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
                    let params: Vec<AbiParam> = id
                        .info()
                        .params
                        .iter()
                        .map(|&ty| AbiParam::new(ty))
                        .collect();
                    self.move_args(&args, &params);
                    self.emit(format!("bl\t_{id}"));
                }
                // What clang emits for __builtin_debugtrap; a debugger can
                // step past it.
                Intrinsic::Debugtrap => self.emit("brk\t#0xf000"),
                // Apple's va_list is a pointer to the next argument slot.
                Intrinsic::VaStart => {
                    let list = self.use_reg(args[0], S1);
                    self.emit(format!("add\tx{S0}, x29, #{}", self.variadic_offset));
                    self.emit(format!("str\tx{S0}, [x{list}]"));
                }
                // Round up to keep `sp` 16-byte aligned; the memory starts
                // above the outgoing arguments, which stay at `sp`.
                Intrinsic::StackAlloc => {
                    let size = self.use_reg(args[0], S0);
                    self.emit(format!("add\tx{S0}, x{size}, #15"));
                    self.emit(format!("and\tx{S0}, x{S0}, #-16"));
                    self.emit(format!("sub\tsp, sp, x{S0}"));
                    let d = self.def_reg(result);
                    self.emit(format!("add\tx{d}, sp, #{}", self.copy_base));
                    self.def_done(result, d);
                }
                Intrinsic::StackSave => {
                    let d = self.def_reg(result);
                    self.emit(format!("mov\tx{d}, sp"));
                    self.def_done(result, d);
                }
                Intrinsic::StackRestore => {
                    let mark = self.use_reg(args[0], S0);
                    self.emit(format!("mov\tsp, x{mark}"));
                }
            },
            InstData::Trap => self.emit("brk\t#1"),
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
                self.emit("ret");
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
    ) -> Option<(Vec<Loc>, &'s Layout)> {
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
                            self.emit(format!("str\tx{S0}, [sp, #{offset}]"));
                        }
                    }
                }
                ArgLoc::InRegs(regs) => {
                    late.push(Late::Struct(regs, layout.expect("a struct"), arg))
                }
                ArgLoc::SretMemory => late.push(Late::Sret(arg)),
                ArgLoc::SretRegs(regs) => {
                    let p = self.use_reg(arg, S0);
                    let save = self.sret_save.expect("planned");
                    self.emit(format!("str\tx{p}, [{}, #{save}]", self.base));
                    sret = Some((regs, layout.expect("a struct")));
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
                    self.emit(format!("mov\tx8, x{p}"));
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
            self.emit(format!("str\tx{lo}, [sp, #{offset}]"));
            let hi = self.use_reg_hi(arg, S1);
            self.emit(format!("str\tx{hi}, [sp, #{}]", offset + 8));
        } else if ty.is_float() {
            let r = self.use_freg(arg, FS1);
            self.emit(format!("str\t{}, [sp, #{offset}]", freg(r, ty)));
        } else {
            let r = self.use_reg(arg, S1);
            let op = match ty {
                Type::I8 => "strb",
                Type::I16 => "strh",
                _ => "str",
            };
            self.emit(format!("{op}\t{}, [sp, #{offset}]", reg(r, ty)));
        }
    }

    /// Loads a stack parameter at `[x29, #offset]` into `dst`.
    fn load_stack_param(&mut self, dst: Loc, ty: Type, offset: u32) {
        let (r, scratch) = match dst {
            Loc::Reg(r) | Loc::FReg(r) => (r, None),
            Loc::Stack(s) => (if ty.is_float() { FS0 } else { S0 }, Some(s)),
        };
        let op = match ty {
            Type::I8 => "ldrb",
            Type::I16 => "ldrh",
            _ => "ldr",
        };
        let rd = if ty.is_float() {
            freg(r, ty)
        } else {
            reg(r, ty)
        };
        self.emit(format!("{op}\t{rd}, [x29, #{offset}]"));
        if let Some(s) = scratch {
            let class = if ty.is_float() { 'd' } else { 'x' };
            self.emit(format!("str\t{class}{r}, {}", self.slot(s)));
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
                        self.emit("cmp\tx0, x2");
                        self.emit("ccmp\tx1, x3, #0, eq");
                    }
                    // a > b is b < a, and a <= b is b >= a: compare
                    // swapped. The borrow of the low halves feeds the high.
                    _ => {
                        let swap =
                            matches!(cond, IntCC::Sgt | IntCC::Sle | IntCC::Ugt | IntCC::Ule);
                        let (x, y) = if swap { (2, 0) } else { (0, 2) };
                        self.emit(format!("cmp\tx{x}, x{y}"));
                        self.emit(format!("sbcs\txzr, x{}, x{}", x + 1, y + 1));
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
                self.emit(format!("cset\tw{d}, {code}"));
                self.def_done(result, d);
            }
            InstData::Select { args } if wide(self, args[1]) => {
                // x0:x1 and x2:x3 are the arms, x4 (or x4:x5) the condition.
                self.operands(&[args[1], args[2], args[0]]);
                if wide(self, args[0]) {
                    self.emit("orr\tx6, x4, x5");
                    self.emit("cmp\tx6, #0");
                } else {
                    self.test_nonzero(4, self.ty(args[0]));
                }
                self.emit("csel\tx6, x0, x2, ne");
                self.emit("csel\tx7, x1, x3, ne");
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
                    (Type::I64, _) => self.emit("mov\tx4, x0"),
                    (Type::I32, true) => self.emit("sxtw\tx4, w0"),
                    (Type::I32, false) => self.emit("mov\tw4, w0"),
                    (Type::I16, true) => self.emit("sxth\tx4, w0"),
                    (Type::I16, false) => self.emit("uxth\tw4, w0"),
                    (_, true) => self.emit("sxtb\tx4, w0"),
                    (_, false) => self.emit("uxtb\tw4, w0"),
                }
                if signed {
                    self.emit("asr\tx5, x4, #63");
                } else {
                    self.emit("mov\tx5, #0");
                }
                self.pair_out(result, 4, 5);
            }
            InstData::Load {
                ty: Type::I128,
                addr,
            } => {
                let a = self.use_reg(addr, S0);
                self.emit(format!("ldp\tx4, x5, [x{a}]"));
                self.pair_out(result, 4, 5);
            }
            InstData::Store { value, addr } if wide(self, value) => {
                self.operands(&[value, addr]);
                self.emit("stp\tx0, x1, [x2]");
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
                self.emit(format!("{lo}\tx4, x0, x2"));
                self.emit(format!("{hi}\tx5, x1, x3"));
                self.emit(format!("cset\tw6, {code}"));
                self.pair_out(result, 4, 5);
                let flag = self.func.inst_results(inst)[1];
                if let Some(loc) = self.alloc.loc(flag) {
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
        if let Some(loc) = self.alloc.loc(value) {
            self.emit_move(loc, Loc::Reg(lo));
        }
        if let Some(loc) = self.alloc.hi_loc(value) {
            self.emit_move(loc, Loc::Reg(hi));
        }
    }

    /// `x0:x1 op x2:x3` (or `x0:x1 op x2` for a shift amount), returning
    /// the registers of the result.
    fn binary_i128(&mut self, inst: Inst, op: BinaryOp) -> (u8, u8) {
        let pair = |this: &mut Self, lo: &str, hi: &str| {
            this.emit(format!("{lo}\tx4, x0, x2"));
            this.emit(format!("{hi}\tx5, x1, x3"));
        };
        match op {
            BinaryOp::Iadd => pair(self, "adds", "adc"),
            BinaryOp::Isub => pair(self, "subs", "sbc"),
            BinaryOp::And => pair(self, "and", "and"),
            BinaryOp::Or => pair(self, "orr", "orr"),
            BinaryOp::Xor => pair(self, "eor", "eor"),
            // (a1:a0)(b1:b0) mod 2^128 = a0 b0 + 2^64 (a0 b1 + a1 b0)
            BinaryOp::Imul => {
                self.emit("mul\tx4, x0, x2");
                self.emit("umulh\tx5, x0, x2");
                self.emit("madd\tx5, x0, x3, x5");
                self.emit("madd\tx5, x1, x2, x5");
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                self.emit("and\tx2, x2, #127");
                self.shift_i128(op, 2, (4, 5), [3, 6, 7]);
            }
            // rotl(x, n) = (x << n) | (x >> (-n mod 128)), and the mirror.
            BinaryOp::Rotl | BinaryOp::Rotr => {
                self.emit("and\tx2, x2, #127");
                self.emit("neg\tx3, x2");
                self.emit("and\tx3, x3, #127");
                let (first, second) = match op {
                    BinaryOp::Rotl => (BinaryOp::Shl, BinaryOp::Ushr),
                    _ => (BinaryOp::Ushr, BinaryOp::Shl),
                };
                self.shift_i128(first, 2, (4, 5), [6, 7, 8]);
                self.shift_i128(second, 3, (6, 7), [8, 16, 17]);
                self.emit("orr\tx4, x4, x6");
                self.emit("orr\tx5, x5, x7");
            }
            // compiler-rt takes and returns the pairs in x0:x1 and x2:x3,
            // where they already are. Traps first, as for narrower types.
            BinaryOp::Udiv | BinaryOp::Sdiv | BinaryOp::Urem | BinaryOp::Srem => {
                let trap = self.trap_label();
                self.emit(format!("orr\tx{S0}, x2, x3"));
                self.emit(format!("cbz\tx{S0}, {trap}"));
                if op == BinaryOp::Sdiv {
                    // MIN / -1: the divisor's halves are both all ones and
                    // the dividend is 2^127.
                    let ok = format!("L{}_{inst}_divok", self.func.name);
                    self.emit(format!("and\tx{S0}, x2, x3"));
                    self.emit(format!("cmn\tx{S0}, #1"));
                    self.emit(format!("b.ne\t{ok}"));
                    self.emit(format!("cbnz\tx0, {ok}"));
                    self.materialize(S0, 1 << 63, true);
                    self.emit(format!("cmp\tx1, x{S0}"));
                    self.emit(format!("b.eq\t{trap}"));
                    writeln!(self.out, "{ok}:").unwrap();
                }
                let function = match op {
                    BinaryOp::Udiv => "___udivti3",
                    BinaryOp::Sdiv => "___divti3",
                    BinaryOp::Urem => "___umodti3",
                    _ => "___modti3",
                };
                self.emit(format!("bl\t{function}"));
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
                self.emit(format!("lsl\tx{a}, x1, x{n}"));
                self.emit(format!("mvn\tw{b}, w{n}"));
                self.emit(format!("lsr\tx{c}, x0, #1"));
                self.emit(format!("lsr\tx{c}, x{c}, x{b}"));
                self.emit(format!("orr\tx{a}, x{a}, x{c}"));
                self.emit(format!("lsl\tx{c}, x0, x{n}"));
                self.emit(format!("tst\tx{n}, #64"));
                self.emit(format!("csel\tx{oh}, x{c}, x{a}, ne"));
                self.emit(format!("csel\tx{ol}, xzr, x{c}, ne"));
            }
            _ => {
                let shr = if op == BinaryOp::Sshr { "asr" } else { "lsr" };
                self.emit(format!("lsr\tx{a}, x0, x{n}"));
                self.emit(format!("mvn\tw{b}, w{n}"));
                self.emit(format!("lsl\tx{c}, x1, #1"));
                self.emit(format!("lsl\tx{c}, x{c}, x{b}"));
                self.emit(format!("orr\tx{a}, x{a}, x{c}"));
                self.emit(format!("{shr}\tx{c}, x1, x{n}"));
                // What fills the high half once n >= 64.
                if op == BinaryOp::Sshr {
                    self.emit(format!("asr\tx{b}, x1, #63"));
                } else {
                    self.emit(format!("mov\tx{b}, #0"));
                }
                self.emit(format!("tst\tx{n}, #64"));
                self.emit(format!("csel\tx{ol}, x{c}, x{a}, ne"));
                self.emit(format!("csel\tx{oh}, x{b}, x{c}, ne"));
            }
        }
    }

    /// `op x0:x1` into x4:x5.
    fn unary_i128(&mut self, op: UnaryOp) {
        match op {
            UnaryOp::Ineg => {
                self.emit("negs\tx4, x0");
                self.emit("ngc\tx5, x1");
            }
            UnaryOp::Bnot => {
                self.emit("mvn\tx4, x0");
                self.emit("mvn\tx5, x1");
            }
            // The high half's count, or 64 plus the low half's.
            UnaryOp::Clz => {
                self.emit("cmp\tx1, #0");
                self.emit("clz\tx4, x1");
                self.emit("clz\tx5, x0");
                self.emit("add\tx5, x5, #64");
                self.emit("csel\tx4, x4, x5, ne");
                self.emit("mov\tx5, #0");
            }
            UnaryOp::Ctz => {
                self.emit("cmp\tx0, #0");
                self.emit("rbit\tx4, x0");
                self.emit("clz\tx4, x4");
                self.emit("rbit\tx5, x1");
                self.emit("clz\tx5, x5");
                self.emit("add\tx5, x5, #64");
                self.emit("csel\tx4, x4, x5, ne");
                self.emit("mov\tx5, #0");
            }
            UnaryOp::Popcnt => {
                self.emit("fmov\td0, x0");
                self.emit("fmov\td1, x1");
                self.emit("cnt\tv0.8b, v0.8b");
                self.emit("cnt\tv1.8b, v1.8b");
                self.emit("addv\tb0, v0.8b");
                self.emit("addv\tb1, v1.8b");
                self.emit("fmov\tw4, s0");
                self.emit("fmov\tw5, s1");
                self.emit("add\tx4, x4, x5");
                self.emit("mov\tx5, #0");
            }
            UnaryOp::Bswap => {
                self.emit("rev\tx4, x1");
                self.emit("rev\tx5, x0");
            }
            _ => unreachable!("{op} is not an integer operation"),
        }
    }

    /// Moves a call's result, if it has one, from x0 or v0 to its place,
    /// and stores a struct result that came back in registers to its
    /// `sret` address.
    fn call_result(&mut self, result: Option<Value>, sret: Option<(Vec<Loc>, &Layout)>) {
        if let Some(value) = result
            && let Some(loc) = self.alloc.loc(value)
        {
            self.emit_move(loc, return_loc(self.func.value_type(value)));
            if let Some(hi) = self.alloc.hi_loc(value) {
                self.emit_move(hi, Loc::Reg(1));
            }
        }
        if let Some((regs, layout)) = sret {
            let save = self.sret_save.expect("planned");
            self.emit(format!("ldr\tx{S0}, [{}, #{save}]", self.base));
            match abi::classify(layout) {
                StructClass::Hfa { ty, .. } => {
                    for (&(offset, _), &r) in layout.leaves().iter().zip(&regs) {
                        self.emit(format!("str\t{}, [x{S0}, #{offset}]", freg(loc_reg(r), ty)));
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
                    self.emit(format!("str\t{fr}, [{base}, #{}]", area + offset as usize));
                }
            }
            _ => {
                for (k, &r) in regs.iter().enumerate() {
                    let r = loc_reg(r);
                    self.emit(format!("str\tx{r}, [{base}, #{}]", area + 8 * k));
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
                    self.emit(format!("ldr\t{fr}, [{base}, #{}]", area + offset as usize));
                }
            }
            StructClass::Gprs { count } => {
                for k in 0..count as usize {
                    self.emit(format!("ldr\tx{k}, [{base}, #{}]", area + 8 * k));
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
                    self.emit(format!("ldr\t{}, [x{p}, #{offset}]", freg(loc_reg(r), ty)));
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
            self.emit(format!("{op}\t{wide}{target}, [x{p}, #{}]", offset + at));
            if i > 0 {
                self.emit(format!("orr\tx{r}, x{r}, x{S1}, lsl #{}", 8 * at));
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
                self.emit(format!("lsr\tx{S1}, x{r}, #{}", 8 * at));
                S1
            };
            self.emit(format!("{op}\t{wide}{src}, [x{p}, #{}]", offset + at));
        }
    }

    /// Copies `size` bytes from `[x<src>]` to `[<base>, #to]` through x17.
    fn copy_bytes(&mut self, base: &str, to: usize, src: u8, size: u64) {
        for (at, n) in pieces(size) {
            let (load, store) = piece_ops(n);
            let wide = if n == 8 { 'x' } else { 'w' };
            self.emit(format!("{load}\t{wide}{S1}, [x{src}, #{at}]"));
            self.emit(format!(
                "{store}\t{wide}{S1}, [{base}, #{}]",
                to + at as usize
            ));
        }
    }

    /// Puts the address `base + offset` in `dst`.
    fn address(&mut self, dst: Loc, base: &str, offset: usize) {
        let r = match dst {
            Loc::Reg(r) => r,
            _ => S0,
        };
        // `add` takes a 12-bit immediate, optionally shifted by 12.
        self.emit(format!("add\tx{r}, {base}, #{}", offset & 0xfff));
        if offset >> 12 != 0 {
            self.emit(format!("add\tx{r}, x{r}, #{}, lsl #12", offset >> 12));
        }
        if let Loc::Stack(s) = dst {
            self.emit(format!("str\tx{r}, {}", self.slot(s)));
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
                self.emit(format!("fcvt\t{fd}, {}", freg(a, from)));
            } else {
                let a = self.use_reg(arg, S0);
                match op {
                    ConvertOp::Bitcast => self.emit(format!("fmov\t{fd}, {}", reg(a, from))),
                    _ => {
                        let signed = op == ConvertOp::Sitofp;
                        let x = reg(self.extend(S0, a, from, signed), from);
                        let mnemonic = if signed { "scvtf" } else { "ucvtf" };
                        self.emit(format!("{mnemonic}\t{fd}, {x}"));
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
            self.emit(format!("{mnemonic}\t{}, {}", reg(d, to), freg(a, from)));
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
                self.emit(format!("{mnemonic}\tw{S0}, w{a}, w{b}"));
                if signed {
                    let ext = if bits == 8 { "sxtb" } else { "sxth" };
                    self.emit(format!("cmp\tw{S0}, w{S0}, {ext}"));
                } else {
                    let high = !((1u32 << bits) - 1);
                    self.emit(format!("tst\tw{S0}, #{high:#x}"));
                }
                self.emit(format!("cset\tw{S2}, ne"));
            }
            None => {
                let (ra, rb, rd) = (reg(a, ty), reg(b, ty), reg(S0, ty));
                match (op, ty) {
                    (OverflowOp::Uadd | OverflowOp::Sadd, _) => {
                        self.emit(format!("adds\t{rd}, {ra}, {rb}"))
                    }
                    (OverflowOp::Usub | OverflowOp::Ssub, _) => {
                        self.emit(format!("subs\t{rd}, {ra}, {rb}"))
                    }
                    // The full product's high half must be zero, or the
                    // sign extension of the low half.
                    (OverflowOp::Umul, Type::I64) => {
                        self.emit(format!("umulh\tx{S2}, x{a}, x{b}"));
                        self.emit(format!("mul\tx{S0}, x{a}, x{b}"));
                        self.emit(format!("cmp\tx{S2}, #0"));
                    }
                    (OverflowOp::Smul, Type::I64) => {
                        self.emit(format!("smulh\tx{S2}, x{a}, x{b}"));
                        self.emit(format!("mul\tx{S0}, x{a}, x{b}"));
                        self.emit(format!("cmp\tx{S2}, x{S0}, asr #63"));
                    }
                    (OverflowOp::Umul, _) => {
                        self.emit(format!("umull\tx{S0}, w{a}, w{b}"));
                        self.emit(format!("cmp\txzr, x{S0}, lsr #32"));
                    }
                    (OverflowOp::Smul, _) => {
                        self.emit(format!("smull\tx{S0}, w{a}, w{b}"));
                        self.emit(format!("cmp\tx{S0}, w{S0}, sxtw"));
                    }
                }
                let code = match op {
                    OverflowOp::Uadd => "hs",
                    OverflowOp::Usub => "lo",
                    OverflowOp::Sadd | OverflowOp::Ssub => "vs",
                    OverflowOp::Umul | OverflowOp::Smul => "ne",
                };
                self.emit(format!("cset\tw{S2}, {code}"));
            }
        }
        let results = self.func.inst_results(inst);
        let (value, flag) = (results[0], results[1]);
        if let Some(loc) = self.alloc.loc(value) {
            self.emit_move(loc, Loc::Reg(S0));
        }
        if let Some(loc) = self.alloc.loc(flag) {
            self.emit_move(loc, Loc::Reg(S2));
        }
    }

    /// Restores the callee-saved registers, `sp` and the frame pointer.
    fn epilogue(&mut self) {
        let saved = self.alloc.callee_saved_used.clone();
        let float_saved = self.alloc.float_callee_saved_used.clone();
        let pairs = saved.len().div_ceil(2) + float_saved.len().div_ceil(2);
        // Point `sp` at the last pair pushed and pop in reverse. Measuring
        // from x29 makes the size of the rest of the frame irrelevant.
        if pairs == 0 {
            self.emit("mov\tsp, x29");
        } else {
            self.emit(format!("sub\tsp, x29, #{}", 16 * pairs));
        }
        for pair in float_saved.chunks(2).rev() {
            match pair {
                [a, b] => self.emit(format!("ldp\td{a}, d{b}, [sp], #16")),
                [a] => self.emit(format!("ldr\td{a}, [sp], #16")),
                _ => unreachable!(),
            }
        }
        for pair in saved.chunks(2).rev() {
            match pair {
                [a, b] => self.emit(format!("ldp\tx{a}, x{b}, [sp], #16")),
                [a] => self.emit(format!("ldr\tx{a}, [sp], #16")),
                _ => unreachable!(),
            }
        }
        self.emit("ldp\tx29, x30, [sp], #16");
    }

    /// Computes `args[0] op args[1]` into `d`.
    fn binary(&mut self, op: BinaryOp, args: [Value; 2], d: u8) {
        let ty = self.ty(args[0]);
        let a = self.use_reg(args[0], S0);
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
            self.emit(format!("{mnemonic}\t{rd}, {ra}, {rb}"));
            return;
        }
        match op {
            BinaryOp::Rotl | BinaryOp::Rotr => match narrow_bits(ty) {
                // `ror` takes its amount modulo the width; rotating left by
                // n is rotating right by -n.
                None => {
                    if op == BinaryOp::Rotl {
                        let neg = reg(S1, ty);
                        self.emit(format!("neg\t{neg}, {rb}"));
                        self.emit(format!("ror\t{rd}, {ra}, {neg}"));
                    } else {
                        self.emit(format!("ror\t{rd}, {ra}, {rb}"));
                    }
                }
                // No narrow rotate instruction: (x << n) | (x >> (bits - n))
                // on the zero-extended value. n = 0 shifts right by `bits`,
                // which gives 0 in a 32-bit register, as needed.
                Some(bits) => {
                    let x = self.extend(S0, a, ty, false);
                    // w17 = n, w8 = bits - n
                    self.emit(format!("and\tw{S1}, w{b}, #{}", bits - 1));
                    self.emit(format!("mov\tw{S2}, #{bits}"));
                    self.emit(format!("sub\tw{S2}, w{S2}, w{S1}"));
                    let (first, second) = match op {
                        BinaryOp::Rotl => ("lsl", "lsr"),
                        _ => ("lsr", "lsl"),
                    };
                    self.emit(format!("{first}\tw{S1}, w{x}, w{S1}"));
                    self.emit(format!("{second}\tw{S2}, w{x}, w{S2}"));
                    self.emit(format!("orr\tw{d}, w{S1}, w{S2}"));
                }
            },
            BinaryOp::Umulhi | BinaryOp::Smulhi => {
                let signed = op == BinaryOp::Smulhi;
                match ty {
                    Type::I64 => {
                        let mnemonic = if signed { "smulh" } else { "umulh" };
                        self.emit(format!("{mnemonic}\tx{d}, x{a}, x{b}"));
                    }
                    // Widening 32x32 -> 64 multiply, then take the top half.
                    Type::I32 => {
                        let (mul, shr) = if signed {
                            ("smull", "asr")
                        } else {
                            ("umull", "lsr")
                        };
                        self.emit(format!("{mul}\tx{d}, w{a}, w{b}"));
                        self.emit(format!("{shr}\tx{d}, x{d}, #32"));
                    }
                    // The full product of two extended i8/i16 fits in 32 bits.
                    _ => {
                        let bits = narrow_bits(ty).expect("i8 or i16");
                        let a = self.extend(S0, a, ty, signed);
                        let b = self.extend(S1, b, ty, signed);
                        self.emit(format!("mul\tw{d}, w{a}, w{b}"));
                        let shr = if signed { "asr" } else { "lsr" };
                        self.emit(format!("{shr}\tw{d}, w{d}, #{bits}"));
                    }
                }
            }
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                // Shift amounts are taken modulo the bit width. The hardware
                // does that for 32 and 64 bits; narrower types need a mask.
                let b = match narrow_bits(ty) {
                    Some(bits) => {
                        self.emit(format!("and\tw{S1}, w{b}, #{}", bits - 1));
                        S1
                    }
                    None => b,
                };
                let (mnemonic, a) = match op {
                    BinaryOp::Shl => ("lsl", a),
                    BinaryOp::Ushr => ("lsr", self.extend(S0, a, ty, false)),
                    _ => ("asr", self.extend(S0, a, ty, true)),
                };
                self.emit(format!("{mnemonic}\t{rd}, {}, {}", reg(a, ty), reg(b, ty)));
            }
            BinaryOp::Udiv | BinaryOp::Urem | BinaryOp::Sdiv | BinaryOp::Srem => {
                let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
                let a = reg(self.extend(S0, a, ty, signed), ty);
                let b = reg(self.extend(S1, b, ty, signed), ty);
                let trap = self.trap_label();
                // Hardware division returns 0 for a zero divisor; the IR traps.
                self.emit(format!("cbz\t{b}, {trap}"));
                if op == BinaryOp::Sdiv {
                    // MIN / -1 overflows. Hardware wraps; the IR traps.
                    let min = reg(S2, ty);
                    self.materialize(S2, min_value(ty), ty == Type::I64);
                    self.emit(format!("cmn\t{b}, #1"));
                    self.emit(format!("ccmp\t{a}, {min}, #0, eq"));
                    self.emit(format!("b.eq\t{trap}"));
                }
                let div = if signed { "sdiv" } else { "udiv" };
                match op {
                    // remainder = a - (a / b) * b, with the quotient in a
                    // scratch register since `a` and `b` are read again.
                    BinaryOp::Urem | BinaryOp::Srem => {
                        let q = reg(S2, ty);
                        self.emit(format!("{div}\t{q}, {a}, {b}"));
                        self.emit(format!("msub\t{rd}, {q}, {b}, {a}"));
                    }
                    _ => self.emit(format!("{div}\t{rd}, {a}, {b}")),
                }
            }
            _ => unreachable!("handled above"),
        }
    }

    /// Computes `op arg` into `d`.
    fn unary(&mut self, op: UnaryOp, arg: Value, d: u8) {
        let ty = self.ty(arg);
        let a = self.use_reg(arg, S0);
        let (ra, rd) = (reg(a, ty), reg(d, ty));
        match op {
            UnaryOp::Ineg => self.emit(format!("neg\t{rd}, {ra}")),
            UnaryOp::Bnot => self.emit(format!("mvn\t{rd}, {ra}")),
            UnaryOp::Clz => {
                // Count in 32 bits, then drop the zeros above the narrow width.
                let x = reg(self.extend(S0, a, ty, false), ty);
                self.emit(format!("clz\t{rd}, {x}"));
                if let Some(bits) = narrow_bits(ty) {
                    self.emit(format!("sub\tw{d}, w{d}, #{}", 32 - bits));
                }
            }
            UnaryOp::Ctz => {
                // ctz = clz of the bit-reversed value. For narrow types, a
                // set bit just above the width caps the count at the width
                // and hides the unspecified upper bits.
                let x = reg(S0, ty);
                match narrow_bits(ty) {
                    Some(bits) => {
                        self.emit(format!("orr\t{x}, {ra}, #{:#x}", 1u32 << bits));
                        self.emit(format!("rbit\t{x}, {x}"));
                    }
                    None => self.emit(format!("rbit\t{x}, {ra}")),
                }
                self.emit(format!("clz\t{rd}, {x}"));
            }
            UnaryOp::Popcnt => {
                // No scalar popcount: count bits per byte in a SIMD register
                // (`cnt`), then add the byte counts (`addv`).
                let x = self.extend(S0, a, ty, false);
                if ty == Type::I64 {
                    self.emit(format!("fmov\td0, x{x}"));
                } else {
                    self.emit(format!("fmov\ts0, w{x}"));
                }
                self.emit("cnt\tv0.8b, v0.8b");
                self.emit("addv\tb0, v0.8b");
                self.emit(format!("fmov\tw{d}, s0"));
            }
            UnaryOp::Bswap => match ty {
                // Swaps the bytes within each halfword; the low one is ours.
                Type::I16 => self.emit(format!("rev16\tw{d}, w{a}")),
                _ => self.emit(format!("rev\t{rd}, {ra}")),
            },
            _ => unreachable!("float operations are lowered in `inst`"),
        }
    }

    /// Lowers `switch`: a jump table if the cases are dense, else a chain of
    /// compares. Like `brif`, a target with block arguments gets a landing
    /// pad that copies them.
    fn switch(&mut self, inst: Inst, arg: Value, dests: &[BlockCall], cases: &[i64]) {
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
            .map(|(i, d)| match d.args.is_empty() {
                true => self.label(d.block),
                false => format!("L{}_{inst}_edge{i}", self.func.name),
            })
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
                self.emit(format!("sub\t{}, {}, {sub}", reg(S0, ty), reg(x, ty)));
                S0
            };
            self.emit(format!("cmp\t{}, #{span}", reg(idx, ty)));
            self.emit(format!("b.hi\t{}", targets[0]));
            let table = format!("L{}_{inst}_table", self.func.name);
            let index = if is64 {
                format!("x{idx}, lsl #2")
            } else {
                format!("w{idx}, uxtw #2")
            };
            self.emit(format!("adr\tx{S1}, {table}"));
            self.emit(format!("ldrsw\tx{S0}, [x{S1}, {index}]"));
            self.emit(format!("add\tx{S1}, x{S1}, x{S0}"));
            self.emit(format!("br\tx{S1}"));
            // Offsets from the table to each target, never executed.
            writeln!(self.out, "{table}:").unwrap();
            for k in 0..=span {
                let i = values
                    .iter()
                    .position(|&v| v == min + k)
                    .map_or(0, |i| i + 1);
                self.emit(format!(".long\t{} - {table}", targets[i]));
            }
        } else {
            for (i, &value) in values.iter().enumerate() {
                let operand = self.operand(S1, value, is64);
                self.emit(format!("cmp\t{}, {operand}", reg(x, ty)));
                self.emit(format!("b.eq\t{}", targets[i + 1]));
            }
            if dests[0].args.is_empty() {
                self.branch_unless_next(dests[0].block);
            } else {
                self.emit(format!("b\t{}", targets[0]));
            }
        }
        for (dest, target) in dests.iter().zip(&targets) {
            if !dest.args.is_empty() {
                writeln!(self.out, "{target}:").unwrap();
                self.block_args(dest);
                self.emit(format!("b\t{}", self.label(dest.block)));
            }
        }
    }

    /// `value` as the second operand of `cmp`/`sub`: an immediate if it fits
    /// in 12 bits, else materialized in `scratch`.
    fn operand(&mut self, scratch: u8, value: u64, is64: bool) -> String {
        if value < 4096 {
            return format!("#{value}");
        }
        self.materialize(scratch, value, is64);
        if is64 {
            format!("x{scratch}")
        } else {
            format!("w{scratch}")
        }
    }

    /// Sets flags so that `ne` means the value in `reg` is non-zero.
    fn test_nonzero(&mut self, reg: u8, ty: Type) {
        match ty {
            // Only the low bits of a narrow value are defined.
            Type::I8 => self.emit(format!("tst\tw{reg}, #0xff")),
            Type::I16 => self.emit(format!("tst\tw{reg}, #0xffff")),
            Type::I32 => self.emit(format!("cmp\tw{reg}, #0")),
            _ => self.emit(format!("cmp\tx{reg}, #0")),
        }
    }

    /// Copies block-call arguments into the target's parameters. All copies
    /// happen at once (`jump block1(v4, v3)` may swap two parameters), so
    /// they go through the parallel-copy sequentializer.
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
    /// x16 breaks cycles, of either register class (`fmov` moves bits
    /// between classes); `emit_move` uses x17 for slot-to-slot moves.
    fn parallel_copy(&mut self, moves: &[(Loc, Loc)]) {
        for (dst, src) in sequentialize(moves, Loc::Reg(S0)) {
            self.emit_move(dst, src);
        }
    }

    /// One 64-bit move between any two locations. Narrow values have
    /// unspecified upper bits, so copying all 64 is always fine.
    fn emit_move(&mut self, dst: Loc, src: Loc) {
        match (dst, src) {
            _ if dst == src => {}
            (Loc::Reg(d), Loc::Reg(s)) => self.emit(format!("mov\tx{d}, x{s}")),
            (Loc::FReg(d), Loc::FReg(s)) => self.emit(format!("fmov\td{d}, d{s}")),
            (Loc::Reg(d), Loc::FReg(s)) => self.emit(format!("fmov\tx{d}, d{s}")),
            (Loc::FReg(d), Loc::Reg(s)) => self.emit(format!("fmov\td{d}, x{s}")),
            (Loc::Reg(d), Loc::Stack(s)) => self.emit(format!("ldr\tx{d}, {}", self.slot(s))),
            (Loc::FReg(d), Loc::Stack(s)) => self.emit(format!("ldr\td{d}, {}", self.slot(s))),
            (Loc::Stack(d), Loc::Reg(s)) => self.emit(format!("str\tx{s}, {}", self.slot(d))),
            (Loc::Stack(d), Loc::FReg(s)) => self.emit(format!("str\td{s}, {}", self.slot(d))),
            (Loc::Stack(d), Loc::Stack(s)) => {
                self.emit(format!("ldr\tx{S1}, {}", self.slot(s)));
                self.emit(format!("str\tx{S1}, {}", self.slot(d)));
            }
        }
    }

    fn branch_unless_next(&mut self, target: Block) {
        if self.next_block != Some(target) {
            self.emit(format!("b\t{}", self.label(target)));
        }
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
        self.emit(format!("{op}\tw{scratch}, w{src}"));
        scratch
    }

    /// Loads a constant with `movz` and up to three `movk`s.
    fn materialize(&mut self, reg: u8, bits: u64, is64: bool) {
        let r = if is64 {
            format!("x{reg}")
        } else {
            format!("w{reg}")
        };
        let chunks = if is64 { 4 } else { 2 };
        self.emit(format!("movz\t{r}, #{:#x}", bits & 0xffff));
        for k in 1..chunks {
            let chunk = (bits >> (16 * k)) & 0xffff;
            if chunk != 0 {
                self.emit(format!("movk\t{r}, #{chunk:#x}, lsl #{}", 16 * k));
            }
        }
    }

    fn sub_sp(&mut self, bytes: usize) {
        if bytes >> 12 != 0 {
            self.emit(format!("sub\tsp, sp, #{}, lsl #12", bytes >> 12));
        }
        if bytes & 0xfff != 0 {
            self.emit(format!("sub\tsp, sp, #{}", bytes & 0xfff));
        }
    }

    fn trap_label(&mut self) -> String {
        self.uses_trap = true;
        format!("L{}_trap", self.func.name)
    }

    /// The location of an operand.
    fn loc(&self, value: Value) -> Loc {
        self.alloc
            .loc(self.func.resolve(value))
            .expect("operand in reachable code is allocated")
    }

    /// The register holding operand `value`: its own, or `scratch` after
    /// reloading it from its spill slot.
    fn use_reg(&mut self, value: Value, scratch: u8) -> u8 {
        match self.loc(value) {
            Loc::Reg(r) => r,
            Loc::Stack(s) => {
                self.emit(format!("ldr\tx{scratch}, {}", self.slot(s)));
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
                self.emit(format!("ldr\tx{scratch}, {}", self.slot(s)));
                scratch
            }
            Loc::FReg(_) => unreachable!("an integer half"),
        }
    }

    /// The location of the high half of `i128` operand `value`.
    fn hi(&self, value: Value) -> Loc {
        self.alloc
            .hi_loc(self.func.resolve(value))
            .expect("operand in reachable code is allocated")
    }

    /// Sets flags so that `ne` means `value` is non-zero; an `i128` is
    /// non-zero if either half is.
    fn test_value(&mut self, value: Value) {
        if self.ty(value) == Type::I128 {
            let lo = self.use_reg(value, S0);
            let hi = self.use_reg_hi(value, S1);
            self.emit(format!("orr\tx{S0}, x{lo}, x{hi}"));
            self.emit(format!("cmp\tx{S0}, #0"));
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
                self.emit(format!("ldr\td{scratch}, {}", self.slot(s)));
                scratch
            }
            Loc::Reg(_) => unreachable!("{value} is not a float"),
        }
    }

    /// The register to compute `result` into: its own, or x16 if it is
    /// spilled (or an alias, whose value is never read).
    fn def_reg(&self, result: Option<Value>) -> u8 {
        let value = result.expect("instruction has a result");
        match self.alloc.loc(value) {
            Some(Loc::Reg(r)) => r,
            _ => S0,
        }
    }

    /// [`Self::def_reg`] for floats: the result's FP register, or v16.
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
            let class = if self.func.value_type(value).is_float() {
                'd'
            } else {
                'x'
            };
            self.emit(format!("str\t{class}{reg}, {}", self.slot(s)));
        }
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(self.func.resolve(value))
    }

    /// The address of spill slot `index`: above the outgoing arguments at
    /// the bottom of the frame, close enough to `sp` for a scaled immediate.
    fn slot(&self, index: u32) -> String {
        format!("[{}, #{}]", self.base, self.spill_base + 8 * index as usize)
    }

    fn loc_name(&self, loc: Loc) -> String {
        match loc {
            Loc::Reg(r) => format!("x{r}"),
            Loc::FReg(r) => format!("d{r}"),
            Loc::Stack(s) => self.slot(s),
        }
    }

    fn label(&self, block: Block) -> String {
        format!("L{}_{block}", self.func.name)
    }

    fn emit(&mut self, line: impl AsRef<str>) {
        writeln!(self.out, "\t{}", line.as_ref()).unwrap();
    }
}

/// Rejects calls the C convention support cannot make yet. `external`
/// callees may be C code relying on the full ABI.
fn check_call_signature(what: &str, sig: &Signature, external: bool) -> Result<(), String> {
    let CallConv::C = sig.cc;
    if sig.returns.len() > 1 {
        return Err(format!(
            "{what}: multiple return values are not supported by the C convention"
        ));
    }
    // The Apple ABI has the caller extend narrow arguments to 32 bits by C
    // signedness. Keel functions do not rely on it, but external ones may,
    // so their signatures must say which.
    if external
        && sig
            .params
            .iter()
            .any(|p| matches!(p.ty, Type::I8 | Type::I16) && p.ext == ArgExt::None)
    {
        return Err(format!(
            "{what}: i8/i16 arguments to external code need `sext` or `uext`"
        ));
    }
    Ok(())
}

/// Emits the DWARF 4 compile unit: one subprogram per function and the
/// line table the assembler builds from the `.loc` directives. Relative
/// file names are taken relative to the directory of the compilation,
/// which, as for C compilers, is the current directory.
fn emit_debug_info(out: &mut String, module: &Module) {
    // Abbreviation 1 is the compile unit, 2 a subprogram: a tag, whether
    // it has children, then (attribute, form) pairs ending in (0, 0).
    out.push_str(concat!(
        "\t.section\t__DWARF,__debug_abbrev,regular,debug\n",
        "\t.byte\t1, 0x11, 1\t; DW_TAG_compile_unit, children\n",
        "\t.byte\t0x25, 0x08\t; DW_AT_producer, DW_FORM_string\n",
        "\t.byte\t0x03, 0x08\t; DW_AT_name, DW_FORM_string\n",
        "\t.byte\t0x1b, 0x08\t; DW_AT_comp_dir, DW_FORM_string\n",
        "\t.byte\t0x10, 0x17\t; DW_AT_stmt_list, DW_FORM_sec_offset\n",
        "\t.byte\t0x11, 0x01\t; DW_AT_low_pc, DW_FORM_addr\n",
        "\t.byte\t0x12, 0x06\t; DW_AT_high_pc, DW_FORM_data4\n",
        "\t.byte\t0, 0\n",
        "\t.byte\t2, 0x2e, 0\t; DW_TAG_subprogram, no children\n",
        "\t.byte\t0x03, 0x08\t; DW_AT_name, DW_FORM_string\n",
        "\t.byte\t0x11, 0x01\t; DW_AT_low_pc, DW_FORM_addr\n",
        "\t.byte\t0x12, 0x06\t; DW_AT_high_pc, DW_FORM_data4\n",
        "\t.byte\t0x3f, 0x19\t; DW_AT_external, DW_FORM_flag_present\n",
        "\t.byte\t0, 0\n",
        "\t.byte\t0\n",
    ));

    let comp_dir = std::env::current_dir().map_or(String::new(), |d| d.display().to_string());
    let first = &module.functions[0].name;
    let last = &module.functions[module.functions.len() - 1].name;
    out.push_str("\t.section\t__DWARF,__debug_info,regular,debug\n");
    out.push_str("\t.long\tLdebug_info_end-Ldebug_info_start\n");
    out.push_str("Ldebug_info_start:\n");
    // DWARF 4, abbreviations at offset 0, 8-byte addresses; then the
    // compile unit (abbreviation 1).
    out.push_str("\t.short\t4\n\t.long\t0\n\t.byte\t8\n");
    out.push_str("\t.byte\t1\n");
    writeln!(out, "\t.asciz\t{}", asm_string("keel")).unwrap();
    writeln!(out, "\t.asciz\t{}", asm_string(&module.files[0])).unwrap();
    writeln!(out, "\t.asciz\t{}", asm_string(&comp_dir)).unwrap();
    // The assembler's line table is all of `__debug_line`.
    out.push_str("\t.long\t0\n");
    writeln!(
        out,
        "\t.quad\tL{first}_begin\n\t.long\tL{last}_end-L{first}_begin"
    )
    .unwrap();
    for func in &module.functions {
        let name = &func.name;
        writeln!(out, "\t.byte\t2\n\t.asciz\t{}", asm_string(name)).unwrap();
        writeln!(
            out,
            "\t.quad\tL{name}_begin\n\t.long\tL{name}_end-L{name}_begin"
        )
        .unwrap();
    }
    out.push_str("\t.byte\t0\n");
    out.push_str("Ldebug_info_end:\n");
}

/// `s` as a quoted assembler string.
fn asm_string(s: &str) -> String {
    let mut quoted = String::from("\"");
    for b in s.bytes() {
        match b {
            b'"' | b'\\' => write!(quoted, "\\{}", b as char).unwrap(),
            b' '..=b'~' => quoted.push(b as char),
            _ => write!(quoted, "\\{b:03o}").unwrap(),
        }
    }
    quoted.push('"');
    quoted
}

/// Emits a data object into the section its contents call for:
///
/// - mutable and all zero: zero-fill (`__bss`), which takes no file space;
/// - mutable: `__DATA,__data`;
/// - constant with pointers: `__DATA,__const`, since the loader writes the
///   pointers at startup and then makes the pages read-only;
/// - constant without pointers: `__TEXT,__const`, read-only from the start.
///
/// Every object is exported (`.globl`), like functions.
fn emit_data(out: &mut String, data: &DataObject, init: &DataInit) {
    let name = &data.name;
    let log2_align = init.align.trailing_zeros();
    if data.thread_local {
        emit_thread_local(out, data, init);
        return;
    }
    writeln!(out, "\t.globl\t_{name}").unwrap();
    if !init.constant && init.is_zero() {
        writeln!(
            out,
            "\t.zerofill\t__DATA,__bss,_{name},{},{log2_align}",
            init.size()
        )
        .unwrap();
        return;
    }
    let section = match (init.constant, init.has_relocations()) {
        (false, _) => "__DATA,__data",
        (true, true) => "__DATA,__const",
        (true, false) => "__TEXT,__const",
    };
    writeln!(
        out,
        "\t.section\t{section}\n\t.p2align\t{log2_align}\n_{name}:"
    )
    .unwrap();
    emit_items(out, init);
}

/// A thread-local variable is a descriptor, `_name` in `__thread_vars`:
/// the getter (`__tlv_bootstrap` until first use), a key the loader fills
/// in, and the address of the initial value, `_name$tlv$init`, in
/// `__thread_data` (or `__thread_bss` if zero). Each thread's copy is
/// made from that.
fn emit_thread_local(out: &mut String, data: &DataObject, init: &DataInit) {
    let name = &data.name;
    let log2_align = init.align.trailing_zeros();
    if init.is_zero() {
        writeln!(
            out,
            "\t.tbss\t_{name}$tlv$init, {}, {log2_align}",
            init.size()
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "\t.section\t__DATA,__thread_data,thread_local_regular\n\t.p2align\t{log2_align}\n_{name}$tlv$init:"
        )
        .unwrap();
        emit_items(out, init);
    }
    writeln!(
        out,
        "\t.section\t__DATA,__thread_vars,thread_local_variables\n\t.globl\t_{name}\n\t.p2align\t3\n_{name}:"
    )
    .unwrap();
    writeln!(
        out,
        "\t.quad\t__tlv_bootstrap\n\t.quad\t0\n\t.quad\t_{name}$tlv$init"
    )
    .unwrap();
}

/// The bytes of a data object, as directives.
fn emit_items(out: &mut String, init: &DataInit) {
    for item in &init.items {
        match item {
            DataItem::Bytes(bytes) if bytes.is_empty() => {}
            DataItem::Bytes(bytes) => {
                let list: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
                writeln!(out, "\t.byte\t{}", list.join(", ")).unwrap();
            }
            // The directives truncate to their width, and the verifier has
            // checked the value fits, so the bits are right either way.
            DataItem::Int { ty, value } => {
                let directive = match ty {
                    Type::I8 => ".byte",
                    Type::I16 => ".short",
                    Type::I32 => ".long",
                    _ => ".quad",
                };
                let bits = 8 * ty.bytes().unwrap_or(8);
                let unsigned = (*value as u64) & (u64::MAX >> (64 - bits));
                writeln!(out, "\t{directive}\t{unsigned}").unwrap();
            }
            DataItem::Zero(0) => {}
            DataItem::Zero(n) => writeln!(out, "\t.space\t{n}").unwrap(),
            DataItem::Ptr { symbol, addend: 0 } => writeln!(out, "\t.quad\t_{symbol}").unwrap(),
            DataItem::Ptr { symbol, addend } => {
                writeln!(out, "\t.quad\t_{symbol}+{addend}").unwrap()
            }
        }
    }
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

/// The register number of an argument register.
fn loc_reg(loc: Loc) -> u8 {
    match loc {
        Loc::Reg(r) | Loc::FReg(r) => r,
        Loc::Stack(_) => unreachable!("not a register"),
    }
}

/// Splits `n` bytes into naturally aligned loads: 8-byte pieces, then at
/// most one each of 4, 2 and 1. Returns (offset, size) pairs.
fn pieces(n: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while n - at >= 8 {
        out.push((at, 8));
        at += 8;
    }
    for size in [4, 2, 1] {
        if n - at >= size {
            out.push((at, size));
            at += size;
        }
    }
    out
}

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
    if ty.is_float() {
        Loc::FReg(0)
    } else {
        Loc::Reg(0)
    }
}

/// `w<n>` for types up to 32 bits, `x<n>` otherwise.
fn reg(n: u8, ty: Type) -> String {
    match ty.bytes() {
        Some(bytes) if bytes <= 4 => format!("w{n}"),
        _ => format!("x{n}"),
    }
}

/// `s<n>` for f32, `d<n>` for f64.
fn freg(n: u8, ty: Type) -> String {
    match ty {
        Type::F32 => format!("s{n}"),
        _ => format!("d{n}"),
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
