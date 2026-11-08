//! Semantic AArch64 Darwin link units. Locations are section-relative; atoms
//! carry local identity and boundaries independently of global linkage names.
pub mod cache;
pub use cache::Cache;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub sections: Vec<Section>,
    /// Numeric local identity, independent of linkage names.
    pub atoms: Vec<Atom>,
    pub unwind: Vec<Unwind>,
    pub symbols: Vec<Symbol>,
    pub minimum: u32,
    pub subsections: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub segment: String,
    pub name: String,
    pub size: usize,
    pub alignment: u32,
    pub flags: u32,
    pub data: Vec<u8>,
    /// Ordered, nonoverlapping operations; page pairs patch adjacent words.
    pub fixups: Vec<Fixup>,
}
/// Local identity, including interior labels which do not begin a new atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atom {
    /// Zero-based index in Object::sections.
    pub section: u32,
    /// Byte offset within that section, before final layout.
    pub offset: u32,
    pub boundary: bool,
    pub no_dead_strip: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Definition {
    Undefined,
    Common { size: u64, alignment: u8 },
    Atom(usize),
}
/// Only names that participate in linkage. Local names are not transmitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub definition: Definition,
    pub private: bool,
    pub weak_definition: bool,
    pub weak_reference: bool,
    pub weak_can_be_hidden: bool,
    pub no_dead_strip: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Numeric reference to a linkage symbol or local atom. Cache IDs use 31 bits.
pub enum Target {
    Symbol(u32),
    Atom(u32),
}
/// One logical relocation. Data addends are explicit, never hidden in payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixup {
    pub offset: u32,
    pub target: Target,
    pub addend: i64,
    pub kind: FixupKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixupKind {
    Pointer64,
    Branch26,
    Page21,
    PageOff12,
    GotPage21,
    GotPageOff12,
    PointerToGot,
    TlvpPage21,
    TlvpPageOff12,
    Difference32 { minus: Target },
    Difference64 { minus: Target },
    PagePair { low_offset: u32 },
    GotPagePair { low_offset: u32 },
    TlvpPagePair { low_offset: u32 },
}
impl FixupKind {
    pub fn width(self) -> usize {
        match self {
            Self::Pointer64 | Self::Difference64 { .. } => 8,
            _ => 4,
        }
    }
    pub fn low_offset(self) -> Option<u32> {
        match self {
            Self::PagePair { low_offset }
            | Self::GotPagePair { low_offset }
            | Self::TlvpPagePair { low_offset } => Some(low_offset),
            _ => None,
        }
    }
}
/// Compact unwind semantics. The owning atom always means this object's copy,
/// even if a weak linkage name selects another object. No function-pointer
/// relocation or synthetic compact-unwind payload is transmitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unwind {
    /// Atom at the start of the function, including any source pointer addend.
    pub owner: u32,
    pub length: u32,
    pub encoding: u32,
    pub personality: Option<Target>,
    pub lsda: Option<Target>,
}
impl Object {
    /// Encode a versioned little-endian cache entry.
    ///
    /// # Errors
    /// Returns an error when a value exceeds the format limits.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        cache::encode(self)
    }
    /// Materialize an owned unit from a validated cache view.
    ///
    /// # Errors
    /// Returns an error for malformed or incompatible input.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Cache::parse(bytes)?.to_object())
    }
}
