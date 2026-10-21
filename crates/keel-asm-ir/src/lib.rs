//! Structured assembly transport, independent of source parsing and object writers.
//!
//! AArch64 opcode/field identities come from the same pinned Arm data as
//! Keel's encoder and select exact encodings with numeric fields. x86 uses
//! separate opcode and operand identities in [`x86`]. IDs are Rust API
//! identities, not a stable serialized wire format. Assemblers validate the
//! instruction family, operand ranges and target requirements.
//! Explicit text fragments support user-authored inline assembly.
mod directive;
mod generated;
pub mod x86;
pub use directive::{Cfi, Directive, Expression, SectionKind};
mod symbolic;
pub use generated::{FieldId, Opcode};
use smallvec::SmallVec;
pub use symbolic::{PageKind, SymbolicInstruction};

/// An exact AArch64 encoding. Also available as [`aarch64::Instruction`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: Opcode,
    pub fields: SmallVec<[(FieldId, u32); 6]>,
}
impl Instruction {
    pub fn new(opcode: Opcode, fields: impl IntoIterator<Item = (FieldId, u32)>) -> Self {
        Self {
            opcode,
            fields: fields.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// AArch64 instruction, preserving the original shared API.
    Instruction(Instruction),
    X86Instruction(x86::Instruction),
    Symbolic(SymbolicInstruction),
    Directive(Directive),
    /// A symbolic B/BL; the assembler owns displacement and relocation handling.
    Branch {
        link: bool,
        target: String,
    },
    Label(String),
    /// An explicit user source fragment, scanned only when consumed by an assembler.
    /// Statement boundaries do not continue across item boundaries.
    Text(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub items: Vec<Item>,
}
impl Program {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn instruction(&mut self, instruction: Instruction) {
        self.items.push(Item::Instruction(instruction));
    }
    pub fn x86_instruction(&mut self, instruction: x86::Instruction) {
        self.items.push(Item::X86Instruction(instruction));
    }
    pub fn directive(&mut self, directive: Directive) {
        self.items.push(Item::Directive(directive));
    }
    pub fn symbolic(&mut self, instruction: SymbolicInstruction) {
        self.items.push(Item::Symbolic(instruction));
    }
    pub fn label(&mut self, name: impl Into<String>) {
        self.items.push(Item::Label(name.into()));
    }
    pub fn branch(&mut self, link: bool, target: impl Into<String>) {
        self.items.push(Item::Branch {
            link,
            target: target.into(),
        });
    }
    /// Consecutive writes coalesce; this is also suitable for a formatting sink.
    pub fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(Item::Text(previous)) = self.items.last_mut() {
            previous.push_str(text);
        } else {
            self.items.push(Item::Text(text.into()));
        }
    }
    pub fn append(&mut self, other: Self) {
        if self.items.is_empty() {
            // Adopt the buffer, rather than copying every item into this one.
            self.items = other.items;
        } else {
            self.items.extend(other.items);
        }
    }
    pub fn stats(&self) -> ProgramStats {
        let mut stats = ProgramStats::default();
        for item in &self.items {
            match item {
                Item::Instruction(_) | Item::X86Instruction(_) | Item::Symbolic(_) => {
                    stats.instructions += 1
                }
                Item::Branch { .. } => stats.instructions += 1,
                Item::Label(_) => stats.labels += 1,
                Item::Directive(_) => {}
                Item::Text(text) => stats.text_bytes += text.len(),
            }
        }
        stats
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgramStats {
    pub instructions: usize,
    pub labels: usize,
    /// Explicit source fragments, including inline assembly or target metadata.
    pub text_bytes: usize,
}

/// Explicit AArch64 namespace; root exports remain compatible with existing users.
pub mod aarch64 {
    pub use super::{FieldId, Instruction, Opcode, PageKind, SymbolicInstruction};
}
