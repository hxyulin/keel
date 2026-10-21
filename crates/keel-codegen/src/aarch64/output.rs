use keel_asm_ir::{Instruction, Item, Program};
use std::fmt::{self, Write};

/// Compiler instructions and metadata have typed emission paths. Only explicit
/// user inline assembly is permitted to enter a structured program as text.
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
    pub(super) fn structured(&self) -> bool {
        matches!(self, Self::Structured(_))
    }
    pub(super) fn push_str(&mut self, text: &str) {
        self.write_str(text).unwrap();
    }
    pub(super) fn inline_assembly(&mut self, text: &str) {
        let mut normalized = String::new();
        for line in text.lines() {
            writeln!(normalized, "\t{}", line.trim()).unwrap();
        }
        match self {
            Self::Text(out) => out.push_str(&normalized),
            Self::Structured(out) => out.text(&normalized),
        }
    }
    pub(super) fn native(
        &mut self,
        instruction: impl FnOnce() -> Instruction,
        text: impl FnOnce(&mut String) -> fmt::Result,
    ) {
        match self {
            Self::Text(out) => {
                out.push('\t');
                text(out).unwrap();
                out.push('\n');
            }
            Self::Structured(out) => out.instruction(instruction()),
        }
    }
    pub(super) fn label(&mut self, label: String) {
        match self {
            Self::Text(out) => {
                out.push_str(&label);
                out.push_str(":\n");
            }
            Self::Structured(out) => out.label(label),
        }
    }
    pub(super) fn branch(&mut self, link: bool, target: String) {
        match self {
            Self::Text(out) => {
                out.push_str(if link { "\tbl\t" } else { "\tb\t" });
                out.push_str(&target);
                out.push('\n');
            }
            Self::Structured(out) => out.branch(link, target),
        }
    }
    pub(super) fn symbolic(&mut self, s: keel_asm_ir::SymbolicInstruction) {
        match self {
            Self::Text(out) => write_symbolic(out, &s),
            Self::Structured(out) => out.items.push(Item::Symbolic(s)),
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
            (Self::Text(out), Self::Text(other)) => out.push_str(&other),
            (Self::Structured(out), Self::Structured(other)) => out.append(other),

            _ => unreachable!("structured output cannot be implicitly serialized"),
        }
    }
    /// Hand everything structured so far to an incremental consumer.
    pub(super) fn flush(&mut self, sink: &mut Option<&mut dyn FnMut(Program)>) {
        if let (Some(sink), Self::Structured(program)) = (sink, self) {
            sink(std::mem::take(program));
        }
    }
    /// Relaxes the function's branches; text before `start` is not its own.
    pub(super) fn relax(&mut self, name: &str, start: usize) {
        if let Self::Text(text) = self {
            super::relax_branches(text, start, name);
            return;
        }
        if !self.needs_relaxation() {
            return;
        }
        let Self::Structured(program) = self else {
            unreachable!()
        };
        let mut n = 0;
        let mut items = Vec::with_capacity(program.items.len());
        for item in std::mem::take(&mut program.items) {
            match item {
                Item::Symbolic(s)
                    if matches!(
                        s,
                        keel_asm_ir::SymbolicInstruction::Conditional { .. }
                            | keel_asm_ir::SymbolicInstruction::Compare { .. }
                    ) =>
                {
                    let near = format!("L{name}_near{n}");
                    n += 1;
                    let inverse = s.inverted_to(near.clone()).unwrap();
                    let target = s.target().to_owned();
                    items.push(Item::Symbolic(inverse));
                    items.push(Item::Branch {
                        link: false,
                        target,
                    });
                    items.push(Item::Label(near));
                }
                Item::Text(mut text) => {
                    super::relax_text(&mut text, name, &mut n);
                    items.push(Item::Text(text));
                }
                other => items.push(other),
            }
        }
        program.items = items;
    }
    fn needs_relaxation(&self) -> bool {
        let Self::Structured(program) = self else {
            return false;
        };
        let instructions = program
            .items
            .iter()
            .map(|item| match item {
                Item::Instruction(_)
                | Item::X86Instruction(_)
                | Item::Symbolic(_)
                | Item::Branch { .. } => 1,
                Item::Label(_) => 0,
                Item::Directive(keel_asm_ir::Directive::Integer { width, .. }) => {
                    usize::from(*width).div_ceil(4)
                }
                Item::Directive(keel_asm_ir::Directive::Bytes(b)) => b.len().div_ceil(4),
                Item::Directive(keel_asm_ir::Directive::Space(n)) => (*n as usize).div_ceil(4),
                Item::Directive(_) => 0,
                Item::Text(s) => s
                    .lines()
                    .filter(|l| l.starts_with('\t') && !l.starts_with("\t;"))
                    .filter(|l| !l.starts_with("\t.") || l.starts_with("\t.long"))
                    .count(),
            })
            .sum::<usize>();
        instructions >= super::COND_BRANCH_RANGE / 4
    }
}
/// Exactly the `Display` text of `s` and a newline, without `fmt::write`.
fn write_symbolic(out: &mut String, s: &keel_asm_ir::SymbolicInstruction) {
    use keel_asm_ir::{PageKind, SymbolicInstruction as S};
    let suffix = |kind: PageKind, offset| match (kind, offset) {
        (PageKind::Direct, false) => "@PAGE",
        (PageKind::Direct, true) => "@PAGEOFF",
        (PageKind::Got, false) => "@GOTPAGE",
        (PageKind::Got, true) => "@GOTPAGEOFF",
        (PageKind::ThreadLocal, false) => "@TLVPPAGE",
        (PageKind::ThreadLocal, true) => "@TLVPPAGEOFF",
    };
    let x = |out: &mut String, register: u8| {
        crate::text::write_decimal(out, "x", register.into()).unwrap();
    };
    out.push('\t');
    match s {
        S::Conditional { condition, target } => {
            out.push_str("b.");
            out.push_str(
                [
                    "eq", "ne", "hs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt",
                    "le", "al", "nv",
                ][*condition as usize],
            );
            out.push('\t');
            out.push_str(target);
        }
        S::Compare {
            register,
            is64,
            nonzero,
            target,
        } => {
            out.push_str(if *nonzero { "cbnz\t" } else { "cbz\t" });
            out.push(if *is64 { 'x' } else { 'w' });
            crate::text::write_decimal(out, "", (*register).into()).unwrap();
            out.push_str(", ");
            out.push_str(target);
        }
        S::Address { register, target } => {
            out.push_str("adr\t");
            x(out, *register);
            out.push_str(", ");
            out.push_str(target);
        }
        S::PageAddress {
            register,
            kind,
            target,
        } => {
            out.push_str("adrp\t");
            x(out, *register);
            out.push_str(", ");
            out.push_str(target);
            out.push_str(suffix(*kind, false));
        }
        S::PageOffset {
            register,
            base,
            load,
            kind,
            target,
        } => {
            out.push_str(if *load { "ldr\t" } else { "add\t" });
            x(out, *register);
            out.push_str(if *load { ", [" } else { ", " });
            x(out, *base);
            out.push_str(", ");
            out.push_str(target);
            out.push_str(suffix(*kind, true));
            if *load {
                out.push(']');
            }
        }
    }
    out.push('\n');
}

impl Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        match self {
            Self::Text(s) => s.push_str(text),
            Self::Structured(_) => panic!("compiler-generated text must use a typed emission path"),
        }
        Ok(())
    }
    fn write_char(&mut self, c: char) -> fmt::Result {
        match self {
            Self::Text(s) => s.push(c),
            Self::Structured(_) => panic!("compiler-generated text must use a typed emission path"),
        }
        Ok(())
    }
}

impl crate::metadata::Sink for Output {
    fn directive(&mut self, d: keel_asm_ir::Directive) {
        match self {
            Self::Text(text) => {
                if !write_common_directive(text, &d) {
                    writeln!(text, "\t{d}").unwrap();
                }
            }
            Self::Structured(p) => p.items.push(Item::Directive(d)),
        }
    }
    fn label(&mut self, name: String) {
        Output::label(self, name);
    }
    fn bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        match self {
            Self::Text(text) => write_bytes(text, bytes),
            Self::Structured(p) => p.items.push(Item::Directive(keel_asm_ir::Directive::Bytes(
                bytes.to_vec(),
            ))),
        }
    }
}

/// Writes `\t{d}\n` without `fmt::write` for the directives functions and
/// data use most, and returns whether `d` was one of them.
fn write_common_directive(out: &mut String, d: &keel_asm_ir::Directive) -> bool {
    use crate::text::write_decimal;
    use keel_asm_ir::{Cfi, Directive as D};
    // The DWARF register numbering of `.cfi` directives.
    let register = |out: &mut String, r: u8| match r {
        0..=30 => write_decimal(out, "x", r.into()),
        64..=95 => write_decimal(out, "d", (r - 64).into()),
        _ => write_decimal(out, "", r.into()),
    };
    match d {
        D::Align(n) => write_decimal(out, "\t.p2align\t", (*n).into()),
        D::Global(name) => {
            out.push_str("\t.globl\t");
            out.push_str(name);
            Ok(())
        }
        D::Space(n) => write_decimal(out, "\t.space\t", (*n).into()),
        D::Bytes(bytes) if !bytes.is_empty() => {
            write_bytes(out, bytes);
            return true;
        }
        D::Integer {
            width,
            expression,
            got: false,
        } => {
            out.push_str(match width {
                1 => "\t.byte\t",
                2 => "\t.short\t",
                4 => "\t.long\t",
                8 => "\t.quad\t",
                _ => return false,
            });
            match &expression.add {
                Some(add) => {
                    out.push_str(add);
                    if let Some(sub) = &expression.subtract {
                        out.push('-');
                        out.push_str(sub);
                    }
                    match expression.constant {
                        0 => Ok(()),
                        n if n > 0 => write_decimal(out, "+", n),
                        n => write_decimal(out, "", n),
                    }
                }
                None => write_decimal(out, "", expression.constant),
            }
        }
        D::Cfi(Cfi::Start) => {
            out.push_str("\t.cfi_startproc");
            Ok(())
        }
        D::Cfi(Cfi::End) => {
            out.push_str("\t.cfi_endproc");
            Ok(())
        }
        D::Cfi(Cfi::DefCfa {
            register: r,
            offset,
        }) => {
            out.push_str("\t.cfi_def_cfa ");
            register(out, *r).and_then(|()| write_decimal(out, ", ", (*offset).into()))
        }
        D::Cfi(Cfi::Offset {
            register: r,
            offset,
        }) => {
            out.push_str("\t.cfi_offset ");
            register(out, *r).and_then(|()| write_decimal(out, ", ", (*offset).into()))
        }
        _ => return false,
    }
    .unwrap();
    out.push('\n');
    true
}

/// `\t.byte\t` and the decimal bytes separated by `, `, then a newline.
fn write_bytes(out: &mut String, bytes: &[u8]) {
    out.push_str("\t.byte\t");
    for (i, &b) in bytes.iter().enumerate() {
        let separator = if i > 0 { ", " } else { "" };
        crate::text::write_decimal(out, separator, b.into()).unwrap();
    }
    out.push('\n');
}
