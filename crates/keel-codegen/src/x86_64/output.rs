//! Typed x86 output. Text is serialized only when explicitly requested.
use crate::metadata::Sink;
use keel_asm_ir::x86::{
    Address, Instruction, Memory, Modifier, Opcode, Operand, Register, RegisterClass,
};
use keel_asm_ir::{Directive, Program};
use std::fmt::{self, Write};

pub(crate) enum Output {
    Text(crate::text::Text),
    Structured(Program),
}
impl Output {
    pub(super) fn new(structured: bool) -> Self {
        if structured {
            Self::Structured(Program::new())
        } else {
            Self::Text(Default::default())
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
    /// Keep the fragment in the enclosing assembly stream for directives,
    /// numeric local labels and relocations. Restore compiler syntax afterward.
    pub(super) fn inline_assembly(&mut self, text: &str) {
        let text = format!(".att_syntax prefix\n{text}\n.intel_syntax noprefix\n");
        match self {
            Self::Text(out) => out.push_str(&text),
            Self::Structured(out) => out.text(&text),
        }
    }
    /// Basic asm keeps literal percent signs and shares one source stream.
    pub(super) fn module_assembly(&mut self, fragments: &[String]) {
        if fragments.is_empty() {
            return;
        }
        let mut text = String::from(".text\n.att_syntax prefix\n");
        for fragment in fragments {
            text.push_str(fragment);
            text.push('\n');
        }
        text.push_str(".text\n.intel_syntax noprefix\n");
        match self {
            Self::Text(out) => out.push_str(&text),
            Self::Structured(out) => out.text(&text),
        }
    }
    /// Append the already resource-checked raw region identically in both paths.
    pub(super) fn checked_region(&mut self, program: Program) {
        match self {
            Self::Structured(out) => out.append(program),
            Self::Text(out) => {
                for item in program.items {
                    match item {
                        keel_asm_ir::Item::Label(label) => {
                            write_quoted(out, &label);
                            out.push_str(":\n");
                        }
                        keel_asm_ir::Item::X86Instruction(i) => {
                            out.push('\t');
                            keel_asm_ir::x86::write_instruction(out, i.opcode, &i.operands, i.lock)
                                .unwrap();
                            out.push('\n');
                        }
                        _ => unreachable!("resource checker excludes non-instruction region items"),
                    }
                }
            }
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
    /// The label of block `block` of `function`, `.L{function}_block{n}`,
    /// written straight into text. `bare` is [`bare_label`]`(function)`.
    pub(super) fn block_label(&mut self, function: &str, bare: bool, block: keel_ir::Block) {
        match self {
            Self::Text(s) if bare => {
                write_block_label(s, function, block);
                s.push_str(":\n");
            }
            _ => self.label(crate::text::block_label(".L", function, block)),
        }
    }
    /// A jump (`jmp` or `jcc`) to block `block` of `function`, as
    /// [`Self::instruction`] would write it. `bare` is
    /// [`bare_label`]`(function)`.
    pub(super) fn jump_to_block(
        &mut self,
        opcode: Opcode,
        function: &str,
        bare: bool,
        block: keel_ir::Block,
    ) {
        match self {
            Self::Text(s) => {
                s.push('\t');
                // The mnemonic alone: no operands.
                keel_asm_ir::x86::write_instruction(s, opcode, &[], false).unwrap();
                s.push_str("\t\".L");
                if bare {
                    s.push_str(function);
                } else {
                    write_escaped(s, function);
                }
                use keel_ir::EntityRef;
                crate::text::write_decimal(s, "_block", block.index() as i128).unwrap();
                s.push_str("\"\n");
            }
            Self::Structured(_) => {
                let label = crate::text::block_label(".L", function, block);
                self.instruction(opcode, [asm_arg(label)], false);
            }
        }
    }
    /// A `call` or `jmp` to function `name`, through the PLT if `plt`, as
    /// [`Self::instruction`] would write it.
    pub(super) fn branch_to_function(&mut self, opcode: Opcode, name: &str, plt: bool) {
        match self {
            Self::Text(s) => {
                s.push('\t');
                keel_asm_ir::x86::write_instruction(s, opcode, &[], false).unwrap();
                s.push_str("\t\"");
                write_escaped(s, name);
                s.push_str(if plt { "\"@PLT\n" } else { "\"\n" });
            }
            Self::Structured(_) => {
                let modifier = if plt { Modifier::Plt } else { Modifier::None };
                self.instruction(opcode, [asm_arg(symbol_address(name, modifier))], false);
            }
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
                if name.starts_with(".L") && bare_label(&name) {
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
fn write_bytes(out: &mut crate::text::Text, bytes: &[u8]) {
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
/// An operand taken by value, so that building one does not copy it.
pub(super) trait IntoOperand {
    fn into_operand(self) -> Operand;
}
impl<T: ToOperand + ?Sized> IntoOperand for &T {
    fn into_operand(self) -> Operand {
        self.operand()
    }
}
impl IntoOperand for Register {
    fn into_operand(self) -> Operand {
        Operand::Register(self)
    }
}
impl IntoOperand for Memory {
    fn into_operand(self) -> Operand {
        Operand::Memory(self)
    }
}
impl IntoOperand for Address {
    fn into_operand(self) -> Operand {
        Operand::Immediate(self)
    }
}
impl IntoOperand for Operand {
    fn into_operand(self) -> Operand {
        self
    }
}
impl IntoOperand for String {
    fn into_operand(self) -> Operand {
        Operand::Immediate(Address {
            expression: keel_asm_ir::Expression::symbol(self),
            modifier: Modifier::None,
        })
    }
}
macro_rules! owned_integers {($($t:ty),*)=>{$(impl IntoOperand for $t {fn into_operand(self)->Operand {(&self).operand()}})*};}
owned_integers!(u8, u16, u32, u64, usize, i8, i16, i32, i64, i128, isize);
pub(super) fn asm_arg(v: impl IntoOperand) -> Operand {
    v.into_operand()
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

/// Whether a local label of these characters needs no quotes.
pub(super) fn bare_label(name: &str) -> bool {
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
}

/// `.L{function}_block{n}`.
fn write_block_label(out: &mut crate::text::Text, function: &str, block: keel_ir::Block) {
    use keel_ir::EntityRef;
    out.push_str(".L");
    out.push_str(function);
    crate::text::write_decimal(out, "_block", block.index() as i128).unwrap();
}

/// `name` as a quoted assembler string, with `"` and `\\` escaped
/// and other bytes outside printable ASCII in octal.
fn write_quoted(out: &mut crate::text::Text, name: &str) {
    out.push('"');
    write_escaped(out, name);
    out.push('"');
}

/// `name` as it reads between quotes.
fn write_escaped(out: &mut crate::text::Text, name: &str) {
    // Names are almost always plain: copy those whole.
    if !(name.bytes()).any(|b| matches!(b, b'"' | b'\\') || !matches!(b, b' '..=b'~')) {
        out.push_str(name);
        return;
    }
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
}
