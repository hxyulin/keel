//! Typed x86 output. Text is serialized only when explicitly requested.
use crate::metadata::Sink;
use keel_asm_ir::x86::{
    Address, Instruction, Memory, Modifier, Opcode, Operand, Register, RegisterClass,
};
use keel_asm_ir::{Directive, Program};
use std::fmt::{self, Write};

pub(super) enum Output {
    Text(String),
    Structured(Program),
}
impl Output {
    pub(super) fn new(structured: bool) -> Self {
        if structured {
            Self::Structured(Program::new())
        } else {
            Self::Text(String::new())
        }
    }
    pub(super) fn instruction<const N: usize>(
        &mut self,
        opcode: Opcode,
        operands: [Operand; N],
        lock: bool,
    ) {
        match self {
            // Text only formats the instruction, straight from its parts.
            Self::Text(s) => {
                s.push('\t');
                keel_asm_ir::x86::write_instruction(s, opcode, &operands, lock).unwrap();
                s.push('\n');
            }
            Self::Structured(p) => p.x86_instruction(Instruction {
                opcode,
                operands: Vec::from(operands),
                lock,
            }),
        }
    }
    /// Continue `module`'s text in place instead of building the function in
    /// its own string and copying it over. Gives where the function begins.
    /// Structured functions keep their own program until [`Self::end_function`].
    pub(super) fn begin_function(&mut self, module: &mut Self) -> usize {
        if let (Self::Text(own), Self::Text(text)) = (&mut *self, module) {
            debug_assert!(own.is_empty());
            std::mem::swap(own, text);
            return own.len();
        }
        0
    }
    /// Hands the module's output, now ending with this function, back.
    pub(super) fn end_function(self, module: &mut Self) {
        match (self, module) {
            (Self::Text(own), Self::Text(text)) => *text = own,
            (own, module) => module.append(own),
        }
    }
    pub(super) fn append(&mut self, other: Self) {
        match (self, other) {
            (Self::Text(s), Self::Text(t)) => s.push_str(&t),
            (Self::Structured(p), Self::Structured(q)) => p.append(q),
            _ => unreachable!(),
        }
    }
    /// The instruction's IR text as a comment, written without `fmt::write`.
    pub(super) fn inst_comment(&mut self, func: &keel_ir::Function, inst: keel_ir::Inst) {
        if let Self::Text(s) = self {
            s.push_str("\t# ");
            func.write_inst_to(s, inst).unwrap();
            s.push('\n');
        }
    }
}
impl Write for Output {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            Self::Text(t) => t.push_str(s),
            Self::Structured(_) => panic!("compiler-generated x86 text must use typed emission"),
        };
        Ok(())
    }
    fn write_char(&mut self, c: char) -> fmt::Result {
        match self {
            Self::Text(t) => t.push(c),
            Self::Structured(_) => panic!("compiler-generated x86 text must use typed emission"),
        };
        Ok(())
    }
}
impl Sink for Output {
    fn label(&mut self, name: String) {
        match self {
            Self::Text(s) => {
                if name.starts_with(".L")
                    && name
                        .as_str()
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
                {
                    s.push_str(&name);
                    s.push_str(":\n");
                } else {
                    write_quoted(s, &name);
                    s.push_str(":\n");
                }
            }
            Self::Structured(p) => p.label(name),
        }
    }
    fn directive(&mut self, d: Directive) {
        match self {
            Self::Text(s) => {
                use crate::text::write_decimal;
                match &d {
                    Directive::Cfi(keel_asm_ir::Cfi::Offset { register, offset }) => {
                        write_decimal(s, "\t.cfi_offset ", (*register).into()).unwrap();
                        write_decimal(s, ", ", (*offset).into()).unwrap();
                    }
                    Directive::Cfi(keel_asm_ir::Cfi::DefCfa { register, offset }) => {
                        write_decimal(s, "\t.cfi_def_cfa ", (*register).into()).unwrap();
                        write_decimal(s, ", ", (*offset).into()).unwrap();
                    }
                    Directive::Global(name) => {
                        s.push_str("\t.globl ");
                        write_quoted(s, name);
                    }
                    Directive::Integer {
                        width,
                        expression,
                        got: false,
                    } => {
                        s.push_str(match width {
                            1 => "\t.byte ",
                            2 => "\t.short ",
                            4 => "\t.long ",
                            8 => "\t.quad ",
                            _ => unreachable!(),
                        });
                        keel_asm_ir::x86::write_expression(s, expression, Modifier::None).unwrap();
                    }
                    Directive::Size { name, expression } => {
                        s.push_str("\t.size ");
                        write_quoted(s, name);
                        s.push_str(", ");
                        keel_asm_ir::x86::write_expression(s, expression, Modifier::None).unwrap();
                    }
                    Directive::Align(n) => write_decimal(s, "\t.p2align\t", (*n).into()).unwrap(),
                    Directive::Bytes(bytes) if !bytes.is_empty() => write_bytes(s, bytes),
                    _ => {
                        writeln!(s, "\t{d}").unwrap();
                        return;
                    }
                }
                s.push('\n');
            }
            Self::Structured(p) => p.directive(d),
        }
    }
    fn bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        match self {
            Self::Text(s) => {
                write_bytes(s, bytes);
                s.push('\n');
            }
            Self::Structured(p) => p.directive(Directive::Bytes(bytes.to_vec())),
        }
    }
}

/// `\t.byte\t` and the decimal bytes separated by `, `.
fn write_bytes(out: &mut String, bytes: &[u8]) {
    out.push_str("\t.byte\t");
    for (i, &b) in bytes.iter().enumerate() {
        let separator = if i > 0 { ", " } else { "" };
        crate::text::write_decimal(out, separator, b.into()).unwrap();
    }
}

pub(super) trait ToOperand {
    fn operand(&self) -> Operand;
}
impl ToOperand for Register {
    fn operand(&self) -> Operand {
        Operand::Register(*self)
    }
}
impl ToOperand for Memory {
    fn operand(&self) -> Operand {
        Operand::Memory(self.clone())
    }
}
impl ToOperand for Address {
    fn operand(&self) -> Operand {
        Operand::Immediate(self.clone())
    }
}
impl ToOperand for Operand {
    fn operand(&self) -> Operand {
        self.clone()
    }
}
impl ToOperand for String {
    fn operand(&self) -> Operand {
        Operand::Immediate(symbol_address(self, Modifier::None))
    }
}
impl<T: ToOperand + ?Sized> ToOperand for &T {
    fn operand(&self) -> Operand {
        (**self).operand()
    }
}
macro_rules! integers {($($t:ty),*)=>{$(impl ToOperand for $t {fn operand(&self)->Operand {Operand::Immediate(Address{expression:keel_asm_ir::Expression::constant(*self as i128),modifier:Modifier::None})}})*};}
integers!(u8, u16, u32, u64, usize, i8, i16, i32, i64, i128, isize);
pub(super) fn asm_arg(v: &impl ToOperand) -> Operand {
    v.operand()
}
pub(super) fn memory(base: Register, offset: i128) -> Memory {
    Memory {
        width: 0,
        base: Some(base),
        index: None,
        scale: 1,
        segment: None,
        displacement: Address {
            expression: keel_asm_ir::Expression::constant(offset),
            modifier: Modifier::None,
        },
    }
}
pub(super) fn sized(mut m: Memory, width: u16) -> Memory {
    m.width = width;
    m
}
pub(super) fn indexed(base: Register, index: Register, scale: u8) -> Memory {
    let mut m = memory(base, 0);
    m.index = Some(index);
    m.scale = scale;
    m
}
pub(super) fn symbol_address(name: &str, modifier: Modifier) -> Address {
    Address {
        expression: keel_asm_ir::Expression::symbol(name),
        modifier,
    }
}
pub(super) fn symbol_memory(base: Register, name: &str, modifier: Modifier) -> Memory {
    let mut m = memory(base, 0);
    m.displacement = symbol_address(name, modifier);
    m
}
pub(super) fn rip() -> Register {
    Register {
        class: RegisterClass::Rip,
        number: 0,
        width: 64,
        high_byte: false,
    }
}
pub(super) fn fs_zero() -> Memory {
    let mut m = memory(rip(), 0);
    m.base = None;
    m.segment = Some(0x64);
    m
}

pub(super) fn offset_memory(m: &Memory, offset: i128) -> Memory {
    let mut m = m.clone();
    m.displacement.expression.constant += offset;
    m
}

/// `name` as a quoted assembler string, with `"` and `\\` escaped
/// and other bytes outside printable ASCII in octal.
fn write_quoted(out: &mut String, name: &str) {
    out.push('"');
    for b in name.bytes() {
        match b {
            b'"' | b'\\' => {
                out.push('\\');
                out.push(char::from(b));
            }
            b' '..=b'~' => out.push(char::from(b)),
            _ => {
                out.push('\\');
                for shift in [6, 3, 0] {
                    out.push(char::from(b'0' + (b >> shift & 7)));
                }
            }
        }
    }
    out.push('"');
}
