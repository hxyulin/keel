use keel_asm_ir::{Instruction, Item, Program};
use std::fmt::{self, Write};

/// Text emission remains byte compatible. Structured emission calls a lazy text
/// renderer only for paths not migrated yet; native instructions never format.
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
    pub(super) fn native(&mut self, instruction: Instruction, text: impl FnOnce() -> String) {
        match self {
            Self::Text(out) => {
                writeln!(out, "\t{}", text()).unwrap();
            }
            Self::Structured(out) => out.instruction(instruction),
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
    pub(super) fn append(&mut self, other: Self) {
        match (self, other) {
            (Self::Text(out), Self::Text(other)) => out.push_str(&other),
            (Self::Structured(out), Self::Structured(other)) => out.append(other),
            (Self::Structured(out), Self::Text(other)) => out.text(&other),
            _ => unreachable!("structured output cannot be implicitly serialized"),
        }
    }
    pub(super) fn needs_relaxation(&self) -> bool {
        let Self::Structured(program) = self else {
            return false;
        };
        let instructions = program
            .items
            .iter()
            .map(|item| match item {
                Item::Instruction(_) | Item::Branch { .. } => 1,
                Item::Label(_) => 0,
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
            Self::Structured(p) => p.text(text),
        }
        Ok(())
    }
}
