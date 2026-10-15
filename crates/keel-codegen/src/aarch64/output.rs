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
                writeln!(out, "{label}:").unwrap();
            }
            Self::Structured(out) => out.label(label),
        }
    }
    pub(super) fn branch(&mut self, link: bool, target: String) {
        match self {
            Self::Text(out) => {
                writeln!(out, "\t{}\t{target}", if link { "bl" } else { "b" }).unwrap();
            }
            Self::Structured(out) => out.branch(link, target),
        }
    }
    pub(super) fn symbolic(&mut self, s: keel_asm_ir::SymbolicInstruction) {
        match self {
            Self::Text(out) => {
                writeln!(out, "\t{s}").unwrap();
            }
            Self::Structured(out) => out.items.push(Item::Symbolic(s)),
        }
    }
    pub(super) fn append(&mut self, other: Self) {
        match (self, other) {
            (Self::Text(out), Self::Text(other)) => out.push_str(&other),
            (Self::Structured(out), Self::Structured(other)) => out.append(other),

            _ => unreachable!("structured output cannot be implicitly serialized"),
        }
    }
    pub(super) fn relax(&mut self, name: &str) {
        if let Self::Text(text) = self {
            super::relax_branches(text, name);
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
                Item::Instruction(_) | Item::Symbolic(_) | Item::Branch { .. } => 1,
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
impl Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        match self {
            Self::Text(s) => s.push_str(text),
            Self::Structured(_) => panic!("compiler-generated text must use a typed emission path"),
        }
        Ok(())
    }
}

impl crate::metadata::Sink for Output {
    fn directive(&mut self, d: keel_asm_ir::Directive) {
        match self {
            Self::Text(text) => {
                writeln!(text, "\t{d}").unwrap();
            }
            Self::Structured(p) => p.items.push(Item::Directive(d)),
        }
    }
    fn label(&mut self, name: String) {
        Output::label(self, name);
    }
}
