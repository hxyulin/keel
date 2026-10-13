//! IR → Apple arm64 assembly text, with a spill-everything register
//! allocator.
//!
//! Every SSA value lives in its own 8-byte stack slot. Each instruction loads
//! its operands into scratch registers (x9–x12), computes, and stores the
//! result back. The output is slow but obviously correct, which makes it a
//! baseline to test a real register allocator against.
//!
//! Values narrower than 64 bits are stored with unspecified upper bits.
//! Operations whose result depends on those bits (comparisons, division,
//! right shifts, extensions) extend their operands first.
//!
//! Frame layout, from the stack pointer up:
//!
//! ```text
//! sp + 0              value slots, 8 bytes each, indexed by value number
//! sp + 8 * values     scratch slots for block-argument copies
//! ...                 padding to 16 bytes
//! x29                 saved x29 (frame pointer), saved x30 (return address)
//! ```

use crate::CodegenError;
use keel_ir::{
    BinaryOp, Block, BlockCall, CallConv, ConvertOp, EntityRef, Function, Inst, InstData, IntCC,
    Type, Value,
};
use std::fmt::Write;

/// Compiles `funcs` into one assembly file for `clang`/`as`.
pub fn compile(funcs: &[Function]) -> Result<String, CodegenError> {
    let mut out = String::from("\t.section\t__TEXT,__text,regular,pure_instructions\n");
    for func in funcs {
        FunctionCompiler::new(func)?.compile(&mut out);
    }
    out.push_str("\t.subsections_via_symbols\n");
    Ok(out)
}

/// The largest offset `ldr x, [sp, #imm]` can encode (12 bits, scaled by 8).
const MAX_SLOT_OFFSET: usize = 4095 * 8;

struct FunctionCompiler<'a> {
    func: &'a Function,
    /// Offset of the first scratch slot.
    scratch_base: usize,
    frame_size: usize,
    out: String,
    uses_trap: bool,
}

impl<'a> FunctionCompiler<'a> {
    fn new(func: &'a Function) -> Result<Self, CodegenError> {
        let err = |message: String| CodegenError {
            func: func.name.clone(),
            message,
        };
        let CallConv::C = func.sig.cc;
        if func.sig.params.len() > 8 {
            return Err(err(
                "more than 8 parameters (stack arguments) are not supported yet".into(),
            ));
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
        for value in (0..func.num_values()).map(Value::new) {
            let ty = func.value_type(value);
            if matches!(ty, Type::I128 | Type::F32 | Type::F64) {
                return Err(err(format!(
                    "{value}: type {ty} is not supported by aarch64 codegen yet"
                )));
            }
        }

        let max_args = func
            .blocks()
            .flat_map(|b| func.block_insts(b))
            .flat_map(|&inst| func.inst_data(inst).branch_dests())
            .map(|dest| dest.args.len())
            .max()
            .unwrap_or(0);
        let scratch_base = 8 * func.num_values();
        let frame_size = (scratch_base + 8 * max_args).next_multiple_of(16);
        if frame_size > MAX_SLOT_OFFSET {
            return Err(err(format!(
                "frame of {frame_size} bytes is too large for this allocator"
            )));
        }
        Ok(Self {
            func,
            scratch_base,
            frame_size,
            out: String::new(),
            uses_trap: false,
        })
    }

    fn compile(mut self, out: &mut String) {
        let func = self.func;
        let name = &func.name;
        writeln!(out, "\t.globl\t_{name}\n\t.p2align\t2\n_{name}:").unwrap();

        self.emit("stp\tx29, x30, [sp, #-16]!");
        self.emit("mov\tx29, sp");
        if self.frame_size > 0 {
            self.sub_sp(self.frame_size);
        }
        if let Some(entry) = func.entry_block() {
            for (i, &param) in func.block_params(entry).iter().enumerate() {
                self.emit(format!("str\tx{i}, {}", self.slot(param)));
            }
        }

        for block in func.blocks() {
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

    fn inst(&mut self, inst: Inst) {
        let func = self.func;
        let result = func.inst_results(inst).first().copied();
        match func.inst_data(inst).clone() {
            InstData::Iconst { imm, .. } => {
                self.materialize(9, imm as u64, true);
                self.store_result(result, 9);
            }
            InstData::Binary { op, args } => {
                self.binary(op, args);
                self.store_result(result, 9);
            }
            InstData::Convert { op, ty, arg } => {
                self.load(9, arg);
                let from = self.ty(arg);
                match op {
                    ConvertOp::Sext => match from {
                        Type::I8 => self.emit(format!("sxtb\t{}, w9", reg(9, ty))),
                        Type::I16 => self.emit(format!("sxth\t{}, w9", reg(9, ty))),
                        _ => self.emit("sxtw\tx9, w9"),
                    },
                    // Writing a w register clears the upper 32 bits.
                    ConvertOp::Uext => match from {
                        Type::I8 => self.emit("uxtb\tw9, w9"),
                        Type::I16 => self.emit("uxth\tw9, w9"),
                        _ => self.emit("mov\tw9, w9"),
                    },
                    // The low bits are already right; upper bits are unspecified.
                    ConvertOp::Trunc => {}
                }
                self.store_result(result, 9);
            }
            InstData::Icmp { cond, args } => {
                let ty = self.ty(args[0]);
                self.load(9, args[0]);
                self.load(10, args[1]);
                let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
                self.extend(9, ty, signed);
                self.extend(10, ty, signed);
                self.emit(format!("cmp\t{}, {}", reg(9, ty), reg(10, ty)));
                self.emit(format!("cset\tw9, {}", cond_code(cond)));
                self.store_result(result, 9);
            }
            InstData::Load { ty, addr } => {
                self.load(9, addr);
                let op = match ty {
                    Type::I8 => "ldrb",
                    Type::I16 => "ldrh",
                    _ => "ldr",
                };
                self.emit(format!("{op}\t{}, [x9]", reg(9, ty)));
                self.store_result(result, 9);
            }
            InstData::Store { value, addr } => {
                let ty = self.ty(value);
                self.load(9, value);
                self.load(10, addr);
                let op = match ty {
                    Type::I8 => "strb",
                    Type::I16 => "strh",
                    _ => "str",
                };
                self.emit(format!("{op}\t{}, [x10]", reg(9, ty)));
            }
            InstData::Jump { dest } => {
                self.block_args(&dest);
                self.emit(format!("b\t{}", self.label(dest.block)));
            }
            InstData::Brif {
                cond,
                dests: [then_dest, else_dest],
            } => {
                self.load(9, cond);
                match self.ty(cond) {
                    // Only the low bits of a narrow value are defined.
                    Type::I8 => self.emit("tst\tw9, #0xff"),
                    Type::I16 => self.emit("tst\tw9, #0xffff"),
                    Type::I32 => self.emit("cmp\tw9, #0"),
                    _ => self.emit("cmp\tx9, #0"),
                }
                // Block arguments are copied on the edge, so a branch with
                // else-arguments needs its own landing pad.
                let else_edge = if else_dest.args.is_empty() {
                    self.label(else_dest.block)
                } else {
                    format!("L{}_{inst}_else", func.name)
                };
                self.emit(format!("b.eq\t{else_edge}"));
                self.block_args(&then_dest);
                self.emit(format!("b\t{}", self.label(then_dest.block)));
                if !else_dest.args.is_empty() {
                    writeln!(self.out, "{else_edge}:").unwrap();
                    self.block_args(&else_dest);
                    self.emit(format!("b\t{}", self.label(else_dest.block)));
                }
            }
            InstData::Return { args } => {
                if let Some(&value) = args.first() {
                    self.load(0, value);
                }
                self.emit("mov\tsp, x29");
                self.emit("ldp\tx29, x30, [sp], #16");
                self.emit("ret");
            }
        }
    }

    /// Computes `args[0] op args[1]` into x9.
    fn binary(&mut self, op: BinaryOp, args: [Value; 2]) {
        let ty = self.ty(args[0]);
        let (a, b) = (reg(9, ty), reg(10, ty));
        self.load(9, args[0]);
        self.load(10, args[1]);
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
            self.emit(format!("{mnemonic}\t{a}, {a}, {b}"));
            return;
        }
        match op {
            BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
                // Shift amounts are taken modulo the bit width. The hardware
                // does that for 32 and 64 bits; narrower types need a mask.
                if let Some(bits) = narrow_bits(ty) {
                    self.emit(format!("and\tw10, w10, #{}", bits - 1));
                }
                let mnemonic = match op {
                    BinaryOp::Shl => "lsl",
                    BinaryOp::Ushr => {
                        self.extend(9, ty, false);
                        "lsr"
                    }
                    _ => {
                        self.extend(9, ty, true);
                        "asr"
                    }
                };
                self.emit(format!("{mnemonic}\t{a}, {a}, {b}"));
            }
            BinaryOp::Udiv | BinaryOp::Urem | BinaryOp::Sdiv | BinaryOp::Srem => {
                let signed = matches!(op, BinaryOp::Sdiv | BinaryOp::Srem);
                self.extend(9, ty, signed);
                self.extend(10, ty, signed);
                let trap = self.trap_label();
                // Hardware division returns 0 for a zero divisor; the IR traps.
                self.emit(format!("cbz\t{b}, {trap}"));
                if op == BinaryOp::Sdiv {
                    // MIN / -1 overflows. Hardware wraps; the IR traps.
                    let min = reg(12, ty);
                    self.materialize(12, min_value(ty), ty == Type::I64);
                    self.emit(format!("cmn\t{b}, #1"));
                    self.emit(format!("ccmp\t{a}, {min}, #0, eq"));
                    self.emit(format!("b.eq\t{trap}"));
                }
                let q = reg(11, ty);
                let div = if signed { "sdiv" } else { "udiv" };
                self.emit(format!("{div}\t{q}, {a}, {b}"));
                match op {
                    // remainder = a - (a / b) * b
                    BinaryOp::Urem | BinaryOp::Srem => {
                        self.emit(format!("msub\t{a}, {q}, {b}, {a}"))
                    }
                    _ => self.emit(format!("mov\t{a}, {q}")),
                }
            }
            _ => unreachable!("handled above"),
        }
    }

    /// Copies block-call arguments into the target's parameter slots. All
    /// copies happen at once: `jump block1(v4, v3)` may swap two parameters,
    /// so with more than one argument everything goes through scratch slots.
    fn block_args(&mut self, call: &BlockCall) {
        let params = self.func.block_params(call.block).to_vec();
        match call.args.as_slice() {
            [] => {}
            [arg] => {
                self.load(9, *arg);
                self.emit(format!("str\tx9, {}", self.slot(params[0])));
            }
            args => {
                for (i, &arg) in args.iter().enumerate() {
                    self.load(9, arg);
                    self.emit(format!("str\tx9, [sp, #{}]", self.scratch_base + 8 * i));
                }
                for (i, &param) in params.iter().enumerate() {
                    self.emit(format!("ldr\tx9, [sp, #{}]", self.scratch_base + 8 * i));
                    self.emit(format!("str\tx9, {}", self.slot(param)));
                }
            }
        }
    }

    /// Sign- or zero-extends a narrow value in `w<reg>` to 32 bits.
    fn extend(&mut self, reg: u8, ty: Type, signed: bool) {
        let op = match (ty, signed) {
            (Type::I8, true) => "sxtb",
            (Type::I8, false) => "uxtb",
            (Type::I16, true) => "sxth",
            (Type::I16, false) => "uxth",
            _ => return,
        };
        self.emit(format!("{op}\tw{reg}, w{reg}"));
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

    fn load(&mut self, reg: u8, value: Value) {
        let slot = self.slot(value);
        self.emit(format!("ldr\tx{reg}, {slot}"));
    }

    fn store_result(&mut self, result: Option<Value>, reg: u8) {
        let value = result.expect("instruction has a result");
        let slot = self.slot(value);
        self.emit(format!("str\tx{reg}, {slot}"));
    }

    fn slot(&self, value: Value) -> String {
        format!("[sp, #{}]", 8 * self.func.resolve(value).index())
    }

    fn ty(&self, value: Value) -> Type {
        self.func.value_type(self.func.resolve(value))
    }

    fn label(&self, block: Block) -> String {
        format!("L{}_{block}", self.func.name)
    }

    fn emit(&mut self, line: impl AsRef<str>) {
        writeln!(self.out, "\t{}", line.as_ref()).unwrap();
    }
}

/// `w<n>` for types up to 32 bits, `x<n>` otherwise.
fn reg(n: u8, ty: Type) -> String {
    match ty.bytes() {
        Some(bytes) if bytes <= 4 => format!("w{n}"),
        _ => format!("x{n}"),
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
