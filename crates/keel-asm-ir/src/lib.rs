//! Structured assembly transport, independent of source parsing and object writers.
//!
//! Opcode and field identities come from the same pinned Arm data as Keel's
//! encoder. IDs are Rust API identities, not a stable serialized wire format.
//! An instruction selects an exact encoding, with numeric field values. The
//! assembler still validates field ranges, reserved bits and CPU requirements.
//! Explicit text fragments permit incremental emitter migration and inline asm.
mod generated;
pub use generated::{FieldId, Opcode};
use smallvec::SmallVec;

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
    Instruction(Instruction),
    /// A symbolic B/BL; the assembler owns displacement and relocation handling.
    Branch {
        link: bool,
        target: String,
    },
    Label(String),
    /// An explicit source fragment, scanned only when consumed by an assembler.
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
        self.items.extend(other.items);
    }
    pub fn stats(&self) -> ProgramStats {
        let mut stats = ProgramStats::default();
        for item in &self.items {
            match item {
                Item::Instruction(_) => stats.instructions += 1,
                Item::Branch { .. } => stats.instructions += 1,
                Item::Label(_) => stats.labels += 1,
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
    /// Remaining explicitly textual source, including metadata and inline asm.
    pub text_bytes: usize,
}
