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
//! the fixed frame, so `sp` never moves after the prologue.
//!
//! # Symbols
//!
//! Code may not contain absolute addresses: the executable is loaded at a
//! random address. A symbol's address is computed relative to the program
//! counter: `adrp` gives its 4 KiB page, `add` the offset within the page.
//! Symbols defined in another image (libc, a dylib) are only known at load
//! time, so their addresses come from the global offset table (GOT), a table
//! of pointers the loader fills in: `adrp` + `ldr` reads the GOT entry.

use crate::CodegenError;
use crate::parallel_copy::sequentialize;
use crate::regalloc::{self, Allocation, Loc, Pools};
use keel_ir::{
    BinaryOp, Block, BlockCall, CallConv, ConvertOp, DataInit, DataItem, DataObject, EntityRef,
    FloatCC, Function, Inst, InstData, IntCC, Intrinsic, Module, Signature, Type, UnaryOp, Value,
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
    for func in &module.functions {
        FunctionCompiler::new(func, module, pools)?.compile(&mut out);
    }
    for data in &module.data {
        if let Some(init) = &data.init {
            emit_data(&mut out, data, init);
        }
    }
    out.push_str("\t.subsections_via_symbols\n");
    Ok(out)
}

/// Instructions that clobber the caller-saved registers: calls, and the
/// memory intrinsics, which are calls to libc.
fn is_call(data: &InstData) -> bool {
    match data {
        InstData::Call { .. } | InstData::CallIndirect { .. } => true,
        InstData::Intrinsic { id, .. } => match id {
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset => true,
            Intrinsic::Debugtrap => false,
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
    /// Offset from `sp` of spill slot 0, above the outgoing arguments.
    spill_base: usize,
    /// Outgoing arguments, spill slots and IR stack slots; the
    /// callee-saved area is separate.
    frame_size: usize,
    /// The block emitted after the current one, which a branch can fall
    /// through to.
    next_block: Option<Block>,
    out: String,
    uses_trap: bool,
}

impl<'a> FunctionCompiler<'a> {
    fn new(func: &'a Function, module: &'a Module, pools: &Pools) -> Result<Self, CodegenError> {
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        let CallConv::C = func.sig.cc;
        if func.sig.variadic {
            return Err(err(
                "variadic functions can be called but not defined yet (there is no va_start)"
                    .into(),
            ));
        }
        let (_, incoming) = arg_locs(&func.sig.params, func.sig.params.len());
        if incoming > MAX_STACK_ARGS {
            return Err(err(format!(
                "{incoming} bytes of stack parameters is more than the {MAX_STACK_ARGS} supported"
            )));
        }
        match func.sig.returns.as_slice() {
            [] => {}
            [Type::I8 | Type::I16] => {
                // The Apple ABI has the callee extend these to 32 bits, as
                // signed or unsigned depending on the C type, which the IR
                // does not record.
                return Err(err(
                    "i8/i16 return values need signedness the IR does not have yet".into(),
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
        for value in (0..func.num_values()).map(Value::new) {
            let ty = func.value_type(value);
            if ty == Type::I128 {
                return Err(err(format!(
                    "{value}: type {ty} is not supported by aarch64 codegen yet"
                )));
            }
        }

        // The outgoing argument area at the bottom of the frame must hold
        // the stack arguments of every call this function makes.
        let mut outgoing = 0;
        for (_, inst) in func.layout_insts() {
            let (args, named) = match func.inst_data(inst) {
                InstData::Call { func: callee, args } => {
                    (args, func.callee(*callee).sig.params.len())
                }
                InstData::CallIndirect { sig, args, .. } => {
                    (args, func.signature(*sig).params.len())
                }
                _ => continue,
            };
            let tys: Vec<Type> = args
                .iter()
                .map(|&a| func.value_type(func.resolve(a)))
                .collect();
            outgoing = outgoing.max(arg_locs(&tys, named).1);
        }
        if outgoing > MAX_STACK_ARGS {
            return Err(err(format!(
                "a call passes {outgoing} bytes of stack arguments, more than the {MAX_STACK_ARGS} supported"
            )));
        }
        let spill_base = (outgoing as usize).next_multiple_of(8);

        let alloc = regalloc::allocate(func, pools, is_call);
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
            spill_base,
            frame_size,
            next_block: None,
            out: String::new(),
            uses_trap: false,
        })
    }

    fn compile(mut self, out: &mut String) {
        let func = self.func;
        let name = &func.name;
        writeln!(out, "\t.globl\t_{name}\n\t.p2align\t2\n_{name}:").unwrap();
        self.allocation_comment();

        self.emit("stp\tx29, x30, [sp, #-16]!");
        self.emit("mov\tx29, sp");
        // Push the callee-saved registers in pairs, keeping `sp` 16-byte
        // aligned; an odd one out still takes 16 bytes.
        for pair in self.alloc.callee_saved_used.clone().chunks(2) {
            match pair {
                [a, b] => self.emit(format!("stp\tx{a}, x{b}, [sp, #-16]!")),
                [a] => self.emit(format!("str\tx{a}, [sp, #-16]!")),
                _ => unreachable!(),
            }
        }
        for pair in self.alloc.float_callee_saved_used.clone().chunks(2) {
            match pair {
                [a, b] => self.emit(format!("stp\td{a}, d{b}, [sp, #-16]!")),
                [a] => self.emit(format!("str\td{a}, [sp, #-16]!")),
                _ => unreachable!(),
            }
        }
        if self.frame_size > 0 {
            self.sub_sp(self.frame_size);
        }
        // Arguments arrive in x0–x7 and v0–v7, which are never allocated,
        // so this copy has no cycles; it may still store to spill slots.
        // Stack arguments are in the caller's frame, just above ours.
        if let Some(entry) = func.entry_block() {
            let params = func.block_params(entry).to_vec();
            let tys = &func.sig.params;
            let (locs, _) = arg_locs(tys, tys.len());
            let mut moves = Vec::new();
            for ((param, loc), &ty) in params.into_iter().zip(locs).zip(tys) {
                let Some(dst) = self.alloc.loc(param) else {
                    continue;
                };
                match loc {
                    ArgLoc::Reg(src) => moves.push((dst, src)),
                    ArgLoc::Stack(offset) => self.load_stack_param(dst, ty, 16 + offset),
                }
            }
            self.parallel_copy(&moves);
        }

        // Only reachable blocks, in the order the allocator numbered them.
        let order = self.alloc.order.clone();
        for (i, &block) in order.iter().enumerate() {
            self.next_block = order.get(i + 1).copied();
            writeln!(self.out, "{}:", self.label(block)).unwrap();
            for &inst in func.block_insts(block) {
                writeln!(self.out, "\t; {}", func.display_inst(inst)).unwrap();
                self.inst(inst);
            }
        }
        if self.uses_trap {
            writeln!(self.out, "L{name}_trap:").unwrap();
            self.emit("brk\t#1");
        }
        out.push_str(&self.out);
    }

    /// A comment listing where each value lives, to make the output
    /// readable.
    fn allocation_comment(&mut self) {
        let mut line = String::from("\t; regalloc:");
        for value in (0..self.func.num_values()).map(Value::new) {
            if let Some(loc) = self.alloc.loc(value) {
                write!(line, " {value}={}", self.loc_name(loc)).unwrap();
            }
        }
        writeln!(self.out, "{line}").unwrap();
    }

    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
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
                let named = func.callee(callee).sig.params.len();
                self.call(&args, named, &format!("_{}", func.callee(callee).name));
                self.call_result(result);
            }
            InstData::CallIndirect { sig, callee, args } => {
                self.move_args(&args, func.signature(sig).params.len());
                // The callee is in an allocated register, which the argument
                // moves leave alone (they write x0–x7, x16 and x17), or in a
                // spill slot, reloaded after the moves so x16 is free.
                let target = self.use_reg(callee, S0);
                self.emit(format!("blr\tx{target}"));
                self.call_result(result);
            }
            InstData::SymbolAddr { symbol } => {
                let d = self.def_reg(result);
                let name = func.symbol_name(symbol);
                if self.module.defines(name) {
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
                self.emit(format!("add\tx{d}, sp, #{}", offset & 0xfff));
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
                let c = self.use_reg(args[0], S0);
                self.test_nonzero(c, self.ty(args[0]));
                let a = self.use_freg(args[1], FS1);
                let b = self.use_freg(args[2], FS2);
                let (a, b, fd) = (freg(a, ty), freg(b, ty), freg(d, ty));
                self.emit(format!("fcsel\t{fd}, {a}, {b}, ne"));
                self.def_done(result, d);
            }
            InstData::Select { args } => {
                let d = self.def_reg(result);
                let ty = self.ty(args[1]);
                let c = self.use_reg(args[0], S0);
                self.test_nonzero(c, self.ty(args[0]));
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
            InstData::Jump { dest } => {
                self.block_args(&dest);
                self.branch_unless_next(dest.block);
            }
            InstData::Brif {
                cond,
                dests: [then_dest, else_dest],
            } => {
                let c = self.use_reg(cond, S0);
                self.test_nonzero(c, self.ty(cond));
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
            InstData::Intrinsic { id, args } => match id {
                // The libc functions take the same arguments in the same
                // order. memset's `int c` only uses its low byte, so the
                // unspecified upper bits of the i8 are harmless.
                Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset => {
                    self.call(&args, args.len(), &format!("_{id}"));
                }
                // What clang emits for __builtin_debugtrap; a debugger can
                // step past it.
                Intrinsic::Debugtrap => self.emit("brk\t#0xf000"),
            },
            InstData::Trap => self.emit("brk\t#1"),
            InstData::Return { args } => {
                if let Some(&value) = args.first() {
                    let src = self.loc(value);
                    self.emit_move(return_loc(self.ty(value)), src);
                }
                self.epilogue();
                self.emit("ret");
            }
        }
    }

    /// Moves the arguments into place and calls `symbol`; the first `named`
    /// are named, the rest variadic. Values live across the call are in
    /// callee-saved registers or spill slots (the allocator sees to that),
    /// so nothing needs saving here.
    fn call(&mut self, args: &[Value], named: usize, symbol: &str) {
        self.move_args(args, named);
        self.emit(format!("bl\t{symbol}"));
    }

    /// Moves call arguments into x0–x7, v0–v7 and the outgoing argument
    /// area at `sp`. The two do not interfere: arguments are never
    /// allocated to x0–x7 or v0–v7, nor to the outgoing area.
    fn move_args(&mut self, args: &[Value], named: usize) {
        let tys: Vec<Type> = args.iter().map(|&a| self.ty(a)).collect();
        let (locs, _) = arg_locs(&tys, named);
        let mut moves = Vec::new();
        for ((&arg, loc), &ty) in args.iter().zip(locs).zip(&tys) {
            match loc {
                ArgLoc::Reg(dst) => moves.push((dst, self.loc(arg))),
                ArgLoc::Stack(offset) => self.store_stack_arg(arg, ty, offset),
            }
        }
        self.parallel_copy(&moves);
    }

    /// Stores `arg` at `[sp, #offset]` with its own width.
    fn store_stack_arg(&mut self, arg: Value, ty: Type, offset: u32) {
        if ty.is_float() {
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

    /// Moves a call's result, if it has one, from x0 or v0 to its place.
    fn call_result(&mut self, result: Option<Value>) {
        if let Some(value) = result
            && let Some(loc) = self.alloc.loc(value)
        {
            self.emit_move(loc, return_loc(self.func.value_type(value)));
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
        let moves: Vec<(Loc, Loc)> = params
            .iter()
            .zip(&call.args)
            // A parameter with no location is an alias, never read.
            .filter_map(|(&param, &arg)| Some((self.alloc.loc(param)?, self.loc(arg))))
            .collect();
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
        format!("[sp, #{}]", self.spill_base + 8 * index as usize)
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
    // signedness, which the IR does not record. Keel functions do not rely
    // on it, but external ones may.
    if external
        && sig
            .params
            .iter()
            .any(|ty| matches!(ty, Type::I8 | Type::I16))
    {
        return Err(format!(
            "{what}: i8/i16 arguments to external code need signedness the IR does not have yet"
        ));
    }
    Ok(())
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

/// Where the C convention puts one argument.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ArgLoc {
    /// x0–x7 or v0–v7.
    Reg(Loc),
    /// At this offset from the caller's `sp` at the call.
    Stack(u32),
}

/// Places arguments of types `tys`, the first `named` of them named and the
/// rest variadic, by Apple's arm64 C convention. Returns each argument's
/// location and the bytes of stack they take.
///
/// - Named integers and pointers go in x0–x7, floats in v0–v7, each class
///   counted on its own: `f(i64, f64, i64)` uses x0, d0 and x1.
/// - Once a class runs out, its arguments go on the stack in order, each
///   at its natural size and alignment: Apple packs them, where standard
///   AAPCS64 gives each its own 8-byte slot.
/// - Variadic arguments always go on the stack, each in its own 8-byte
///   slot (Apple's `va_list` is a pointer stepping 8 bytes at a time).
fn arg_locs(tys: &[Type], named: usize) -> (Vec<ArgLoc>, u32) {
    let (mut ints, mut floats, mut stack) = (0u8, 0u8, 0u32);
    let mut on_stack = |size: u32, align: u32| {
        let offset = stack.next_multiple_of(align);
        stack = offset + size;
        ArgLoc::Stack(offset)
    };
    let locs = tys
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            if i >= named {
                return on_stack(8, 8);
            }
            let (next, class): (_, fn(u8) -> Loc) = if ty.is_float() {
                (&mut floats, Loc::FReg)
            } else {
                (&mut ints, Loc::Reg)
            };
            if *next < 8 {
                *next += 1;
                return ArgLoc::Reg(class(*next - 1));
            }
            let size = ty.bytes().unwrap_or(8);
            on_stack(size, size)
        })
        .collect();
    (locs, stack)
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
