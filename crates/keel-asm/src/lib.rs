//! A small A64 assembler for Apple arm64 Mach-O objects.
//!
//! Instruction bit layouts come from Arm's machine-readable specification.
//! Syntax, operand transformations, and object layout are implemented here.
//! DWARF 4 line tables, debug data, and ordinary call-frame directives are
//! supported, including Darwin exception personality/LSDA references.
//! Symbol-defined subsections support linker reordering and dead stripping.
//! Unsupported instructions/directives are errors.
mod generated;
mod macho;
mod parser;

use std::fmt;

/// An assembly diagnostic with a one-based source line (zero for object errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl Error {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line != 0 {
            write!(f, "line {}: ", self.line)?;
        }
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

/// A defined text symbol or an undefined external symbol. Names are preserved, including Darwin's `_` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// Offset within the defining section; `None` for undefined symbols.
    pub offset: Option<u64>,
    /// Zero is __TEXT,__text; subsequent indices address Assembly::sections.
    pub section: Option<usize>,
    pub global: bool,
}

/// Darwin arm64 instruction relocation types (Apple's arm64/reloc.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RelocationKind {
    Unsigned = 0,
    Branch26 = 2,
    Page21 = 3,
    PageOff12 = 4,
    GotPage21 = 5,
    GotPageOff12 = 6,
    /// PC-relative 32-bit data pointer to a linker-created GOT slot.
    PointerToGot = 7,
    TlvpPage21 = 8,
    TlvpPageOff12 = 9,
}
impl RelocationKind {
    fn pc_relative(self) -> bool {
        matches!(
            self,
            Self::Branch26 | Self::Page21 | Self::GotPage21 | Self::TlvpPage21 | Self::PointerToGot
        )
    }
    fn accepts_addend(self) -> bool {
        matches!(self, Self::Branch26 | Self::Page21 | Self::PageOff12)
    }
}

/// A symbol reference to be resolved by the linker. Instruction addends use
/// ARM64_RELOC_ADDEND pairs; data addends are serialized into the data word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocation {
    pub section: usize,
    /// Number of relocated bytes: 4 for instructions, 4/8 for data.
    pub size: u8,
    /// A data difference uses SUBTRACTOR(subtract_symbol) then UNSIGNED(symbol).
    pub subtract_symbol: Option<String>,
    pub offset: u32,
    pub symbol: String,
    pub kind: RelocationKind,
    pub addend: i64,
}

/// Mach-O data-section types used by Keel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Data,
    /// DWARF metadata, excluded from the linked image.
    Debug,
    /// Runtime DWARF call-frame records.
    EhFrame,
    ZeroFill,
    ThreadData,
    ThreadZeroFill,
    ThreadVariables,
}
impl SectionKind {
    pub fn is_zero_fill(self) -> bool {
        matches!(self, Self::ZeroFill | Self::ThreadZeroFill)
    }
    fn flags(self) -> u32 {
        match self {
            Self::Data => 0,
            Self::Debug => 0x0200_0000,
            Self::EhFrame => 0x6800_000b,
            Self::ZeroFill => 1,
            Self::ThreadData => 0x11,
            Self::ThreadZeroFill => 0x12,
            Self::ThreadVariables => 0x13,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionContents {
    Bytes(Vec<u8>),
    ZeroFill(u64),
}

/// An additional section. The text section remains in Assembly::text;
/// this vector has no duplicate copy of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub segment: String,
    pub name: String,
    pub alignment: u32,
    pub kind: SectionKind,
    pub contents: SectionContents,
}
impl Section {
    pub fn size(&self) -> u64 {
        match &self.contents {
            SectionContents::Bytes(bytes) => bytes.len() as u64,
            SectionContents::ZeroFill(size) => *size,
        }
    }
    pub fn data(&self) -> &[u8] {
        match &self.contents {
            SectionContents::Bytes(bytes) => bytes,
            SectionContents::ZeroFill(_) => &[],
        }
    }
}

/// Encoded text, extra data sections, and symbols before writing the object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembly {
    /// Permit the Mach-O linker to reorder/dead-strip symbol-defined atoms.
    pub subsections_via_symbols: bool,
    pub text: Vec<u8>,
    /// Section alignment as a power of two (at least 2 for A64).
    pub text_alignment: u32,
    pub symbols: Vec<Symbol>,
    pub relocations: Vec<Relocation>,
    /// Extra sections, indexed from one in symbols and relocations.
    pub sections: Vec<Section>,
}

impl Assembly {
    /// Write a little-endian arm64 Mach-O relocatable object.
    pub fn to_macho(&self) -> Result<Vec<u8>, Error> {
        macho::write(self)
    }
}

/// Assemble A64 syntax, resolving local branches/ADR and retaining linker relocations.
pub fn assemble(source: &str) -> Result<Assembly, Error> {
    parser::assemble(source)
}

/// Assemble source directly into an Apple arm64 `.o` file.
pub fn assemble_object(source: &str) -> Result<Vec<u8>, Error> {
    assemble(source)?.to_macho()
}

struct Field {
    name: &'static str,
    start: u8,
    width: u8,
}
enum Requirement {
    Feature(&'static str),
    NotEqual(&'static str, u32),
}

// Fixed Apple Silicon target: scalar FP, Advanced SIMD, and LSE atomics.
const TARGET_FEATURES: &[&str] = &["FEAT_FP", "FEAT_AdvSIMD", "FEAT_LSE"];

struct Encoding {
    name: &'static str,
    base: u32,
    mask: u32,
    fields: &'static [Field],
    requirements: &'static [Requirement],
}

/// Every variable field must be supplied once. Fixed bits within partially
/// variable fields (e.g. MOVZ wN's `hw`) remain constrained by the ISA mask.
fn encode(name: &str, values: &[(&str, u32)]) -> Result<u32, String> {
    let encoding =
        generated::encoding(name).ok_or_else(|| format!("unsupported encoding `{name}`"))?;
    let name = encoding.name;
    if values.len() != encoding.fields.len() {
        return Err(format!("wrong number of fields for `{name}`"));
    }
    let mut word = encoding.base;
    for field in encoding.fields {
        let mut matches = values.iter().filter(|(n, _)| *n == field.name);
        let (_, value) = matches
            .next()
            .ok_or_else(|| format!("missing field `{}`", field.name))?;
        if matches.next().is_some() {
            return Err(format!("duplicate field `{}`", field.name));
        }
        if u64::from(*value) >= 1u64 << field.width {
            return Err(format!("field `{}` is out of range", field.name));
        }
        let span = (((1u64 << field.width) - 1) as u32) << field.start;
        word = (word & !span) | (value << field.start);
    }
    if word & encoding.mask != encoding.base {
        return Err(format!("reserved field value for `{name}`"));
    }
    for requirement in encoding.requirements {
        match *requirement {
            Requirement::Feature(feature) if !TARGET_FEATURES.contains(&feature) => {
                return Err(format!("`{name}` requires {feature}"));
            }
            Requirement::NotEqual(field, excluded)
                if values.iter().any(|(n, v)| *n == field && *v == excluded) =>
            {
                return Err(format!("reserved field `{field}` for `{name}`"));
            }
            _ => {}
        }
    }
    Ok(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_lookup_returns_every_encoding_record() {
        for expected in generated::ENCODINGS {
            let actual = generated::encoding(expected.name).unwrap();
            assert!(std::ptr::eq(actual, expected), "{}", expected.name);
        }
        for unsupported in [
            "",
            "add",
            "ADD_64_addsub_imm ",
            "add_64_addsub_imm",
            "UNKNOWN",
        ] {
            assert!(generated::encoding(unsupported).is_none());
            assert_eq!(
                encode(unsupported, &[]),
                Err(format!("unsupported encoding `{unsupported}`"))
            );
        }
    }
}
