//! Native x86-64 ELF assembler. Intel/AT&T source is parsed once into `keel-asm-ir`.
//! Parsed items are laid out until optimistic short branches fit their final layout.
mod arm;
mod dwarf;
mod encode;
mod inputs;
mod metadata;
mod relax;
mod sections;
pub(crate) mod source;
mod symbolic;
use crate::Error;
use crate::fxhash::{FxHashSet, FxHasher};
pub use inputs::BinaryInputResolver;
pub use keel_asm_ir::x86::{CpuLevel, Features, Syntax};
use keel_asm_ir::{
    Directive, Expression, Item, Program,
    x86::{self, Address, Modifier},
};
use object::write::{Comdat, Object, Relocation, Symbol, SymbolSection};
use object::{
    Architecture, BinaryFormat, ComdatKind, Endianness, RelocationFlags, SectionFlags, SectionKind,
    SymbolFlags, SymbolKind, SymbolScope,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hash::Hasher;

struct Section {
    name: String,
    flags: u64,
    nobits: bool,
    entry_size: u64,
    group: Option<String>,
    section_type: u32,
    unique: Option<u64>,
    align: u64,
    data: Vec<u8>,
    size: u64,
}
#[derive(Clone)]
struct Sym {
    section: Option<usize>,
    value: u64,
    global: bool,
    local: bool,
    weak: bool,
    hidden: bool,
    kind: SymbolKind,
    // Boxed: most symbols are plain labels, and one table entry exists per name.
    size: Option<Box<Expr>>,
    alias: Option<Box<Expr>>,
    common: Option<(u64, u64)>,
}
impl Default for Sym {
    fn default() -> Self {
        Self {
            section: None,
            value: 0,
            global: false,
            local: false,
            weak: false,
            hidden: false,
            kind: SymbolKind::Unknown,
            size: None,
            alias: None,
            common: None,
        }
    }
}
/// An interned symbol name: an index into [`Names`] and the symbol table.
type SymId = u32;
/// Symbol names, interned once to dense ids. The text lives in one buffer
/// and the open-addressed index holds only ids and hash fingerprints, so
/// layout passes compare and look up symbols as small integers rather than
/// hashing names, and a lookup rarely touches a name it does not match.
#[derive(Default)]
struct Names {
    text: String,
    /// Start and length of each name in `text`.
    spans: Vec<(u32, u32)>,
    /// Zero, or a name's 32-bit [`Self::hash`] above its `id + 1`. The hash
    /// also picks the home slot. The length is zero or a power of two.
    slots: Vec<u64>,
}
impl Names {
    fn hash(name: &str) -> u32 {
        let mut h = FxHasher::default();
        h.write(name.as_bytes());
        // Fx's final multiply carries a difference in a word's high byte
        // (the last character of an eight-byte name) only into the top
        // bits; fold those down so slot indices see it too.
        let h = h.finish();
        let h = (h ^ h >> 32).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        (h >> 32) as u32
    }
    fn len(&self) -> usize {
        self.spans.len()
    }
    fn name(&self, id: SymId) -> &str {
        let (start, len) = self.spans[id as usize];
        &self.text[start as usize..(start + len) as usize]
    }
    /// The slot holding `name`, or the empty slot where it belongs.
    fn find(&self, name: &str, hash: u32) -> usize {
        let mask = self.slots.len() - 1;
        let mut i = hash as usize & mask;
        loop {
            let slot = self.slots[i];
            if slot == 0 || ((slot >> 32) as u32 == hash && self.name(slot as u32 - 1) == name) {
                return i;
            }
            i = (i + 1) & mask;
        }
    }
    fn get(&self, name: &str) -> Option<SymId> {
        if self.slots.is_empty() {
            return None;
        }
        (self.slots[self.find(name, Self::hash(name))] as u32).checked_sub(1)
    }
    fn intern(&mut self, name: &str) -> SymId {
        if self.spans.len() * 2 >= self.slots.len() {
            let n = (self.slots.len() * 2).max(64);
            let old = std::mem::replace(&mut self.slots, vec![0; n]);
            for slot in old.into_iter().filter(|&s| s != 0) {
                let mut i = (slot >> 32) as usize & (n - 1);
                while self.slots[i] != 0 {
                    i = (i + 1) & (n - 1);
                }
                self.slots[i] = slot;
            }
        }
        let hash = Self::hash(name);
        let slot = self.find(name, hash);
        if let Some(id) = (self.slots[slot] as u32).checked_sub(1) {
            return id;
        }
        let id = self.spans.len() as SymId;
        let start = u32::try_from(self.text.len()).expect("symbol names exceed 4 GiB");
        self.text.push_str(name);
        self.spans.push((start, name.len() as u32));
        self.slots[slot] = u64::from(hash) << 32 | u64::from(id + 1);
        id
    }
}
/// A symbol expression over interned names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Expr {
    add: Option<SymId>,
    subtract: Option<SymId>,
    constant: i128,
}
impl Expr {
    fn constant(constant: i128) -> Self {
        Self {
            constant,
            ..Self::default()
        }
    }
    fn symbol(id: SymId) -> Self {
        Self {
            add: Some(id),
            ..Self::default()
        }
    }
    fn symbolic(&self) -> bool {
        self.add.is_some() || self.subtract.is_some()
    }
}
/// Symbols by interned id, in a dense table. Layout passes declare the same
/// names, so [`Self::clear`] keeps names and slots: a slot is visible once
/// declared in the current pass. Collection only interns names; the table
/// grows to cover them when layout first declares a symbol.
struct Symbols {
    names: Names,
    table: Vec<Sym>,
    /// The pass that last declared each slot. Passes count from one, so a
    /// zero (new) slot is undeclared.
    declared: Vec<u32>,
    pass: u32,
    len: usize,
}
impl Default for Symbols {
    fn default() -> Self {
        Self {
            names: Names::default(),
            table: Vec::new(),
            declared: Vec::new(),
            pass: 1,
            len: 0,
        }
    }
}
impl Symbols {
    fn len(&self) -> usize {
        self.len
    }
    fn intern(&mut self, name: &str) -> SymId {
        self.names.intern(name)
    }
    fn name(&self, id: SymId) -> &str {
        self.names.name(id)
    }
    fn expr(&mut self, e: &Expression) -> Expr {
        Expr {
            add: e.add.as_deref().map(|n| self.intern(n)),
            subtract: e.subtract.as_deref().map(|n| self.intern(n)),
            constant: e.constant,
        }
    }
    fn get(&self, id: SymId) -> Option<&Sym> {
        let i = id as usize;
        (self.declared.get(i) == Some(&self.pass)).then(|| &self.table[i])
    }
    fn get_mut(&mut self, id: SymId) -> Option<&mut Sym> {
        let i = id as usize;
        (self.declared.get(i) == Some(&self.pass)).then(|| &mut self.table[i])
    }
    /// The symbol, declared with default attributes on first use.
    fn entry(&mut self, id: SymId) -> &mut Sym {
        let i = id as usize;
        if i >= self.table.len() {
            self.table.resize_with(self.names.len(), Sym::default);
            self.declared.resize(self.names.len(), 0);
        }
        if self.declared[i] != self.pass {
            self.declared[i] = self.pass;
            self.table[i] = Sym::default();
            self.len += 1;
        }
        &mut self.table[i]
    }
    fn insert(&mut self, id: SymId, sym: Sym) {
        *self.entry(id) = sym;
    }
    /// Declared symbols in id order.
    fn iter(&self) -> impl Iterator<Item = (SymId, &Sym)> {
        self.table
            .iter()
            .zip(&self.declared)
            .enumerate()
            .filter(|&(_, (_, &pass))| pass == self.pass)
            .map(|(id, (sym, _))| (id as SymId, sym))
    }
    fn values(&self) -> impl Iterator<Item = &Sym> {
        self.iter().map(|(_, sym)| sym)
    }
    /// Begin the next pass with no symbols declared.
    fn clear(&mut self) {
        self.pass += 1;
        self.len = 0;
    }
}
impl std::ops::Index<SymId> for Symbols {
    type Output = Sym;
    fn index(&self, id: SymId) -> &Sym {
        self.get(id).expect("no such symbol")
    }
}
enum AlignFill {
    Default,
    Byte(u8),
    Nop,
}
struct Fix {
    section: usize,
    offset: u64,
    width: u8,
    modifier: Modifier,
    expr: Expr,
    kind: u32,
    line: usize,
}
struct Space {
    section: usize,
    offset: u64,
    expression: x86::expression::Expr,
    line: usize,
    element_size: u8,
    negative_is_zero: bool,
}
#[derive(Default)]
struct Assembler {
    arm: Option<crate::TargetFeatures>,
    sections: Vec<Section>,
    symbols: Symbols,
    /// Aliases declared so far in this pass.
    aliases: usize,
    fixes: Vec<Fix>,
    current: usize,
    previous: Option<usize>,
    section_stack: Vec<usize>,
    cfi: Option<metadata::Frame>,
    frames: Vec<metadata::Frame>,
    debug: dwarf::Debug,
    source_files: Vec<Vec<u8>>,
    addrsig: Option<Vec<String>>,
    leb_widths: Vec<usize>,
    lebs: Vec<(usize, u64, Expr, bool, usize)>,
    space_widths: Vec<u64>,
    spaces: Vec<Space>,
    pref_powers: Vec<u32>,
    /// Section sizes from the previous layout pass, which declares the same
    /// sections in the same order: reserve them instead of regrowing.
    size_hints: Vec<u64>,
    prefs: Vec<(usize, u64, u64, u32, SymId, usize)>,
}

/// Assemble x86 source (initially Intel) as a Linux relocatable ELF64 object.
pub fn assemble_object(source: &str) -> Result<Vec<u8>, Error> {
    assemble_object_with_features(source, x86::Features::default())
}
pub fn assemble_object_with_features(
    source: &str,
    features: x86::Features,
) -> Result<Vec<u8>, Error> {
    assemble_object_with_syntax_and_features(source, Syntax::Intel, features)
}
/// Assemble source with an explicit initial dialect. In-source syntax directives
/// override this selection. Existing APIs retain their Intel default.
pub fn assemble_object_with_syntax_and_features(
    source: &str,
    syntax: Syntax,
    features: x86::Features,
) -> Result<Vec<u8>, Error> {
    assemble_object_with_binary_inputs(source, syntax, features, &mut inputs::unavailable)
}

/// Assemble with caller-supplied binary inputs for `.incbin`. The resolver owns
/// path lookup and I/O; resolved bytes are retained before layout begins.
///
/// # Errors
/// Returns an error for unresolved inputs, invalid ranges, or invalid assembly.
pub fn assemble_object_with_binary_inputs(
    source: &str,
    syntax: Syntax,
    features: x86::Features,
    resolver: &mut BinaryInputResolver<'_>,
) -> Result<Vec<u8>, Error> {
    let mut items = Items::new(features);
    items.parser = x86::SourceParser::new(syntax);
    items.register_aliases = source::RegisterAliases::new(syntax == Syntax::Att);
    items.parse(source, 1, resolver)?;
    assemble_items(items.finish()?)
}
/// Consume x86 instructions and shared directives. Diagnostic positions refer
/// to program items; positions inside explicit text refer to its source lines.
pub fn assemble_program_object(program: &Program) -> Result<Vec<u8>, Error> {
    assemble_program_object_with_features(program, x86::Features::default())
}
pub fn assemble_program_object_with_features(
    program: &Program,
    features: x86::Features,
) -> Result<Vec<u8>, Error> {
    assemble_program_object_with_binary_inputs(program, features, &mut inputs::unavailable)
}

/// Structured-program counterpart of [`assemble_object_with_binary_inputs`].
///
/// # Errors
/// Returns an error for unresolved inputs, invalid ranges, or an invalid program.
pub fn assemble_program_object_with_binary_inputs(
    program: &Program,
    features: x86::Features,
    resolver: &mut BinaryInputResolver<'_>,
) -> Result<Vec<u8>, Error> {
    let mut items = Items::new(features);
    items.entries.reserve(program.items.len());
    for (index, item) in program.items.iter().enumerate() {
        match item {
            Item::Text(source) => items.parse(source, index + 1, resolver)?,
            Item::X86Instruction(i) => items.x86_instruction(i, index + 1),
            item => items.item(item.clone(), index + 1),
        }
    }
    assemble_items(items.finish()?)
}

/// Incremental form of [`assemble_program_object_with_features`]: programs
/// pushed in order assemble exactly like their concatenation, with the same
/// item positions in diagnostics. Each chunk is consumed as it arrives, so a
/// whole module's structured program never needs to be resident at once.
pub struct ProgramAssembler {
    items: Items,
    next_line: usize,
    error: Option<Error>,
}

impl fmt::Debug for ProgramAssembler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgramAssembler")
            .field("entries", &self.items.entries.len())
            .field("encoded_bytes", &self.items.encoded.len())
            .field("next_line", &self.next_line)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl ProgramAssembler {
    pub fn new(features: x86::Features) -> Self {
        Self {
            items: Items::new(features),
            next_line: 1,
            error: None,
        }
    }
    pub(crate) fn new_aarch64(features: crate::TargetFeatures) -> Self {
        let mut out = Self::new(x86::Features::default());
        out.items.arm = Some(features);
        out
    }
    pub fn push(&mut self, program: Program) {
        self.push_with_binary_inputs(program, &mut inputs::unavailable);
    }

    /// Push a chunk with explicit `.incbin` resolution. Names already resolved
    /// by earlier chunks retain their original bytes.
    /// Errors are retained and returned by [`Self::finish`].
    pub fn push_with_binary_inputs(
        &mut self,
        program: Program,
        resolver: &mut BinaryInputResolver<'_>,
    ) {
        if self.error.is_some() {
            return;
        }
        let items = &mut self.items;
        for item in program.items {
            let line = self.next_line;
            self.next_line += 1;
            match item {
                Item::Text(source) => {
                    if let Err(e) = if items.arm.is_some() {
                        arm::parse(items, &source, line)
                    } else {
                        items.parse(&source, line, resolver)
                    } {
                        self.error = Some(e);
                        return;
                    }
                }
                Item::X86Instruction(i) => items.x86_instruction(&i, line),
                item => items.item(item, line),
            }
        }
    }
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        match self.error {
            Some(e) => Err(e),
            None => assemble_items(self.items.finish()?),
        }
    }
}

/// One layout unit, kept to 16 bytes. Diagnostics use `line`.
enum Entry {
    /// Consecutive symbol-free instructions, encoded once at collection: their
    /// bytes are `encoded[previous run end..end]`. `line` is the first one's;
    /// only it can fail layout, as the section cannot change within a run.
    Encoded {
        end: u32,
        line: u32,
    },
    /// An instruction with symbolic operands: `symbolic.entries[index]`.
    Symbolic {
        index: u32,
        line: u32,
    },
    Label {
        sym: SymId,
        line: u32,
    },
    /// A single, in-range literal data operand, decoded once. Keep bytes rather
    /// than i128 so this variant does not increase Entry's alignment or size.
    LiteralData {
        bytes: [u8; 8],
        width: u8,
        line: u32,
    },
    /// Directive lines from explicit text (`texts[index]`), applied in order.
    Text {
        index: u32,
        line: u32,
    },
    Align {
        power: u8,
        line: u32,
    },
    /// A register-only CFI directive, packed by [`pack_cfi`].
    Cfi {
        op: u8,
        register: u8,
        value: u64,
        line: u32,
    },
    /// ELF STT_FUNC when `function`, otherwise STT_OBJECT.
    Type {
        sym: SymId,
        function: bool,
        line: u32,
    },
    /// `sizes[index]`: a symbol and its size expression.
    Size {
        index: u32,
        line: u32,
    },
    ArmWord {
        index: u32,
        line: u32,
    },
    /// Any other shared item: `items[index]`.
    Item {
        index: u32,
        line: u32,
    },
}
const _: () = assert!(std::mem::size_of::<Entry>() == 16);
/// A register-only CFI directive as an operation, register and value.
fn pack_cfi(c: &keel_asm_ir::Cfi) -> Option<(u8, u8, u64)> {
    use keel_asm_ir::Cfi;
    Some(match *c {
        Cfi::Start => (0, 0, 0),
        Cfi::End => (1, 0, 0),
        Cfi::DefCfaOffset(n) => (2, 0, n),
        Cfi::DefCfaRegister(r) => (3, r, 0),
        Cfi::DefCfa { register, offset } => (4, register, offset),
        Cfi::Offset { register, offset } => (5, register, offset as u64),
        Cfi::Personality(_) | Cfi::Lsda(_) => return None,
    })
}
fn unpack_cfi(op: u8, register: u8, value: u64) -> keel_asm_ir::Cfi {
    use keel_asm_ir::Cfi;
    match op {
        0 => Cfi::Start,
        1 => Cfi::End,
        2 => Cfi::DefCfaOffset(value),
        3 => Cfi::DefCfaRegister(register),
        4 => Cfi::DefCfa {
            register,
            offset: value,
        },
        _ => Cfi::Offset {
            register,
            offset: value as i64,
        },
    }
}
fn line32(line: usize) -> u32 {
    // Diagnostics only; no input has four billion items or lines.
    u32::try_from(line).unwrap_or(u32::MAX)
}
fn index32(len: usize) -> u32 {
    u32::try_from(len).expect("assembly input exceeds 4 GiB")
}
/// Layout input. Most instructions in ordinary assembly are symbol-free and
/// encode identically on every relaxation pass; retaining their parsed
/// operands for each pass would cost far more than their bytes. Symbolic
/// instructions keep a cached encoding and a compact operand record.
struct Items {
    arm: Option<crate::TargetFeatures>,
    arm_words: Vec<crate::parser::elf::Word>,
    entries: Vec<Entry>,
    encoded: Vec<u8>,
    symbolic: symbolic::Cache,
    texts: Vec<Box<str>>,
    sizes: Vec<(SymId, Expr)>,
    items: Vec<Item>,
    /// Names interned during collection; layout passes extend and reuse it.
    symbols: Symbols,
    features: x86::Features,
    parser: x86::SourceParser,
    locals: source::Locals,
    register_aliases: source::RegisterAliases,
    binary_inputs: inputs::Cache,
    preprocessor: crate::parser::preprocess::Processor,
    // Parse errors anywhere take precedence over feature errors.
    unsupported: Option<Error>,
    parsed: usize,
}
impl Items {
    fn new(features: x86::Features) -> Self {
        Self {
            arm: None,
            arm_words: Vec::new(),
            entries: Vec::new(),
            encoded: Vec::new(),
            symbolic: symbolic::Cache::default(),
            texts: Vec::new(),
            sizes: Vec::new(),
            items: Vec::new(),
            symbols: Symbols::default(),
            features,
            parser: x86::SourceParser::default(),
            locals: source::Locals::default(),
            register_aliases: source::RegisterAliases::default(),
            binary_inputs: inputs::Cache::default(),
            preprocessor: crate::parser::preprocess::Processor::default(),
            unsupported: None,
            parsed: 0,
        }
    }
    fn parse(
        &mut self,
        source: &str,
        first: usize,
        resolver: &mut BinaryInputResolver<'_>,
    ) -> Result<(), Error> {
        // Move syntax state out while the callback collects directly into self.
        let statements = source::statements(source)
            .map_err(|mut e| {
                e.line += first - 1;
                e
            })?
            .into_iter()
            .map(|(line, text)| (line + first - 1, text))
            .collect();
        let expanded =
            crate::parser::preprocess::expand_statements(&mut self.preprocessor, statements)?;
        let mut parser = std::mem::take(&mut self.parser);
        let result = expanded
            .source
            .lines()
            .enumerate()
            .try_for_each(|(index, source)| {
                let line = expanded.original_line(index + 1);
                let source = self.locals.rewrite(source, line)?;
                let Some(source) = self
                    .register_aliases
                    .rewrite(&source)
                    .map_err(|e| Error::new(line, e))?
                else {
                    return Ok(());
                };
                let mut input_error = None;
                parser
                    .parse_with(&source, |_, item| match item {
                        x86::SourceItem::Instruction(i) => self.x86_instruction(&i, line),
                        x86::SourceItem::Label(name) => {
                            let sym = self.symbols.intern(&name);
                            self.push(Entry::Label {
                                sym,
                                line: line32(line),
                            })
                        }
                        x86::SourceItem::Text(text) => {
                            match self.binary_inputs.include(text, resolver) {
                                Ok(Some(bytes)) => {
                                    self.item(Item::Directive(Directive::Bytes(bytes)), line);
                                    return;
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    input_error = Some(Error::new(line, error));
                                    return;
                                }
                            }
                            let line = line32(line);
                            let entry = if let Some((bytes, width)) = literal_data(text) {
                                Entry::LiteralData { bytes, width, line }
                            } else {
                                self.texts.push(text.into());
                                Entry::Text {
                                    index: index32(self.texts.len() - 1),
                                    line,
                                }
                            };
                            self.push(entry);
                        }
                    })
                    .map_err(|e| {
                        expanded.remap(Error::new(
                            index + 1,
                            e.strip_prefix("line 1: ").unwrap_or(&e),
                        ))
                    })?;
                input_error.map_or(Ok(()), Err)
            });
        self.parser = parser;
        result
    }
    fn push(&mut self, entry: Entry) {
        self.parsed += 1;
        self.entries.push(entry);
    }
    /// A shared non-instruction item, stored compactly where common.
    fn item(&mut self, item: Item, line: usize) {
        let line = line32(line);
        let entry = match item {
            Item::Label(name) => Entry::Label {
                sym: self.symbols.intern(&name),
                line,
            },
            Item::Directive(Directive::Align(power)) => Entry::Align { power, line },
            Item::Directive(Directive::Cfi(cfi))
                if let Some((op, register, value)) = pack_cfi(&cfi) =>
            {
                Entry::Cfi {
                    op,
                    register,
                    value,
                    line,
                }
            }
            Item::Directive(Directive::ElfType { name, function }) => Entry::Type {
                sym: self.symbols.intern(&name),
                function,
                line,
            },
            Item::Directive(Directive::Size { name, expression }) => {
                let e = self.symbols.expr(&expression);
                self.sizes.push((self.symbols.intern(&name), e));
                Entry::Size {
                    index: index32(self.sizes.len() - 1),
                    line,
                }
            }
            item => {
                self.items.push(item);
                Entry::Item {
                    index: index32(self.items.len() - 1),
                    line,
                }
            }
        };
        self.push(entry);
    }
    fn x86_instruction(&mut self, i: &x86::Instruction, line: usize) {
        let opcode = x86::evex::promote(i.opcode, &i.operands);
        let promoted;
        let i = if opcode != i.opcode {
            promoted = x86::Instruction {
                opcode,
                ..i.clone()
            };
            &promoted
        } else {
            i
        };
        if !self.encode(i, line) {
            let index = self.symbolic.push(i, &mut self.symbols);
            self.push(Entry::Symbolic {
                index,
                line: line32(line),
            });
        }
    }
    /// Encode a symbol-free instruction into the current run, if possible.
    /// Otherwise the caller retains it for per-pass layout.
    fn encode(&mut self, i: &x86::Instruction, line: usize) -> bool {
        let required = i.required_features();
        if !self.features.contains(required) {
            self.unsupported.get_or_insert_with(|| {
                Error::new(
                    line,
                    format!(
                        "{} requires x86 target features: {}",
                        i.opcode,
                        required.names().collect::<Vec<_>>().join(",")
                    ),
                )
            });
            return false;
        }
        if self.unsupported.is_some()
            || !i.operands.iter().all(|o| match o {
                x86::Operand::Register(_) => true,
                x86::Operand::Immediate(a) => symbol_free(a),
                x86::Operand::Memory(m) => symbol_free(&m.displacement),
            })
        {
            return false;
        }
        // Without a symbol nothing folds and no branch is short, so layout
        // would reencode exactly this. Failures are left for layout to report.
        let Ok(e) = encode::encode(i, false) else {
            return false;
        };
        if !e.fixups.is_empty() {
            return false;
        }
        self.parsed += 1;
        self.encoded.extend_from_slice(&e.bytes);
        let end = index32(self.encoded.len());
        match self.entries.last_mut() {
            Some(Entry::Encoded { end: run, .. }) => *run = end,
            _ => self.entries.push(Entry::Encoded {
                end,
                line: line32(line),
            }),
        }
        true
    }
    fn finish(self) -> Result<Self, Error> {
        self.locals.finish()?;
        self.features
            .validate()
            .map_err(|message| Error::new(1, message))?;
        match self.unsupported {
            Some(e) => Err(e),
            None => Ok(self),
        }
    }
}
fn symbol_free(a: &Address) -> bool {
    a.expression.add.is_none() && a.expression.subtract.is_none()
}

/// Recognize only single literals whose address parsing and width check would
/// succeed. Every other spelling stays on the text path, including errors:
/// collection must not change parse/feature/layout diagnostic precedence.
fn literal_data(text: &str) -> Option<([u8; 8], u8)> {
    let (op, args) = text.trim().split_once(char::is_whitespace)?;
    let width = match op {
        ".byte" => 1,
        ".short" | ".word" => 2,
        ".long" | ".int" => 4,
        ".quad" => 8,
        _ => return None,
    };
    let args = args.trim();
    let (negative, digits) = if let Some(s) = args.strip_prefix('-') {
        (true, s)
    } else {
        (false, args.strip_prefix('+').unwrap_or(args))
    };
    // Match x86::integer's radix rules, but use a nonallocating failure path.
    // Do not call integer(...).ok(): symbolic/multioperand probes would format
    // discarded diagnostics. Additional signs and expression terms fall back.
    let (radix, digits) = if let Some(s) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, s)
    } else if let Some(s) = digits.strip_prefix("0b") {
        (2, s)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    if digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = u64::from_str_radix(digits, radix).ok()?;
    let value = if negative {
        -i128::from(magnitude)
    } else {
        i128::from(magnitude)
    };
    let bits = u32::from(width) * 8;
    if value < -(1i128 << (bits - 1)) || value >= 1i128 << bits {
        return None;
    }
    Some(((value as u64).to_le_bytes(), width))
}

/// In-process diagnostic timings. Normal assembly compiles out stage timing.
#[derive(Default, Debug)]
pub struct Profile {
    pub parse_ms: f64,
    pub layout_ms: Vec<f64>,
    pub branch_check_ms: Vec<f64>,
    pub discard_ms: Vec<f64>,
    pub widened: Vec<usize>,
    pub object_ms: f64,
    /// Parsed instructions, labels and retained directives before layout.
    /// Each inline directive counts separately, just like newline-separated
    /// directives. Syntax-mode directives consumed by the source parser do not
    /// contribute. Older versions coalesced adjacent inline directive texts.
    pub items: usize,
}
/// Assemble once and collect stage timings, excluding file I/O.
pub fn profile_object(source: &str) -> Result<(Vec<u8>, Profile), Error> {
    profile_object_with_features(source, Features::default())
}
pub fn profile_object_with_features(
    source: &str,
    features: Features,
) -> Result<(Vec<u8>, Profile), Error> {
    let mut profile = Profile::default();
    let start = std::time::Instant::now();
    let mut items = Items::new(features);
    items.parse(source, 1, &mut inputs::unavailable)?;
    let mut items = items.finish()?;
    profile.parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    profile.items = items.parsed;
    let object = assemble_items_inner::<true>(&mut items, &mut profile)?;
    Ok((object, profile))
}
fn assemble_items(mut items: Items) -> Result<Vec<u8>, Error> {
    assemble_items_inner::<false>(&mut items, &mut Profile::default())
}
fn assemble_items_inner<const PROFILE: bool>(
    items: &mut Items,
    profile: &mut Profile,
) -> Result<Vec<u8>, Error> {
    // Start optimistically short, then only widen. Alignment (especially max-skip)
    // is not monotonic: recheck *every* remaining short branch after each layout.
    // Branches only widen. Variable-size directives may need extra passes;
    // diagnose a cyclic directive layout without capping branch relaxation.
    let mut leb_widths = Vec::new();
    let mut pref_powers = Vec::new();
    let mut space_widths = Vec::new();
    // Brent's cycle detector retains one directive state, not a history per
    // pass. A branch widening changes the transition function, so reset it.
    let mut checkpoint = None;
    let (mut period, mut distance) = (1usize, 0usize);
    let mut variable_work = 0usize;
    // Every pass defines the same symbols and fixups: later passes reuse the
    // previous pass's (cleared) tables, and size their sections from it.
    let mut recycled = (std::mem::take(&mut items.symbols), Vec::new());
    let mut size_hints = Vec::new();
    let mut first = true;
    loop {
        let start = PROFILE.then(std::time::Instant::now);
        let mut a = Assembler::new()?;
        a.arm = items.arm;
        (a.symbols, a.fixes) = std::mem::take(&mut recycled);
        a.leb_widths = leb_widths.clone();
        a.pref_powers = pref_powers.clone();
        a.space_widths = space_widths.clone();
        a.size_hints = std::mem::take(&mut size_hints);
        let mut branches = Vec::new();
        let mut run = 0;
        // Only the first pass records positions for offset-only relaxation.
        let mut trace = first.then(|| relax::Trace::new(&items.symbolic, a.symbols.names.len()));
        for entry in &items.entries {
            let (result, line) = match *entry {
                Entry::Encoded { end, line } => {
                    let bytes = &items.encoded[run..end as usize];
                    run = end as usize;
                    (a.encoded(bytes), line)
                }
                Entry::Symbolic { index, line } => {
                    if items.symbolic.entries[index as usize].short {
                        branches.push((index, a.fixes.len()));
                    }
                    if let Some(trace) = &mut trace {
                        trace.symbolic(&items.symbolic, index, a.current, a.offset());
                    }
                    (a.symbolic(&items.symbolic, index, line as usize), line)
                }
                Entry::Label { sym, line } => {
                    if let Some(trace) = &mut trace {
                        trace.label(sym, a.current, a.offset());
                    }
                    (a.label(sym), line)
                }
                Entry::LiteralData { bytes, width, line } => {
                    (a.data_bytes(&bytes[..usize::from(width)]), line)
                }
                Entry::Text { index, line } => {
                    if let Some(trace) = &mut trace {
                        trace.invalidate();
                    }
                    (
                        items.texts[index as usize]
                            .lines()
                            .try_for_each(|directive| a.directive(directive, line as usize)),
                        line,
                    )
                }
                Entry::Align { power, line } => {
                    let at = a.offset();
                    let result = a.align(power.into(), None);
                    if let Some(trace) = &mut trace {
                        trace.align(power, a.current, at, a.offset() - at);
                    }
                    (result, line)
                }
                Entry::Cfi {
                    op,
                    register,
                    value,
                    line,
                } => (
                    metadata::shared_cfi(&mut a, &unpack_cfi(op, register, value)),
                    line,
                ),
                Entry::Type {
                    sym,
                    function,
                    line,
                } => {
                    a.symbols.entry(sym).kind = if function {
                        SymbolKind::Text
                    } else {
                        SymbolKind::Data
                    };
                    (Ok(()), line)
                }
                Entry::Size { index, line } => {
                    let (sym, e) = items.sizes[index as usize];
                    (a.size(sym, e), line)
                }
                Entry::ArmWord { index, line } => (
                    arm::word(&mut a, &items.arm_words[index as usize], line as usize),
                    line,
                ),
                Entry::Item { index, line } => {
                    (a.item(&items.items[index as usize], line as usize), line)
                }
            };
            result.map_err(|e| Error::new(line as usize, e))?;
        }
        if let Some(start) = start {
            profile
                .layout_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let start = PROFILE.then(std::time::Instant::now);
        size_hints = a.sections.iter().map(|s| s.data.len() as u64).collect();
        let mut changed = 0;
        for (index, fix) in branches {
            let f = &a.fixes[fix];
            let fits = a.local_symbol(f.expr.add.unwrap(), 0)
                && a.try_eval(&f.expr, 0).is_some_and(|(section, value)| {
                    section == Some(f.section)
                        && (-128..=127).contains(&(value - i128::from(f.offset)))
                });
            if !fits {
                items.symbolic.widen(index, &mut a.symbols);
                changed += 1;
            }
        }
        if let Some(start) = start {
            profile
                .branch_check_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
            profile.widened.push(changed);
        }
        let start = PROFILE.then(std::time::Instant::now);
        let mut variable_changed = false;
        let mut pending_leb_error = None;
        for (i, (section, offset, expression, signed, line)) in a.lebs.iter().enumerate() {
            let (sec, value) = a.eval(expression, 0).map_err(|e| Error::new(*line, e))?;
            if sec.is_some() {
                return Err(Error::new(
                    *line,
                    "LEB128 requires an absolute same-section difference",
                ));
            }
            let mut bytes = Vec::new();
            let width = a.leb_widths.get(i).copied().unwrap_or(1);
            if leb_widths.len() <= i {
                leb_widths.resize(i + 1, 1);
            }
            // Unsigned values may be provisionally negative while another
            // directive or branch still changes layout. Keep this width until
            // the layout is stable before reporting the domain error.
            if let Err(e) = metadata::leb(&mut bytes, value, *signed) {
                pending_leb_error.get_or_insert_with(|| Error::new(*line, e));
                continue;
            }
            leb_widths[i] = bytes.len();
            if width != bytes.len() {
                variable_changed = true;
            } else {
                let at = *offset as usize;
                a.sections[*section].data[at..at + width].copy_from_slice(&bytes);
            }
        }
        for (i, space) in a.spaces.iter().enumerate() {
            let Space {
                section,
                offset,
                expression,
                line,
                ..
            } = space;
            let value = expression
                .eval(&mut |name| {
                    if name == "." {
                        return Ok(x86::expression::Value {
                            section: Some(*section),
                            number: i128::from(*offset),
                        });
                    }
                    let name = x86::symbol(name)?;
                    let id =
                        a.symbols.names.get(&name).ok_or_else(|| {
                            format!("undefined symbol `{name}` in space expression")
                        })?;
                    let (section, number) = a.eval_symbol(id, 0)?;
                    Ok(x86::expression::Value { section, number })
                })
                .map_err(|e| Error::new(*line, e))?;
            if value.section.is_some() {
                return Err(Error::new(
                    *line,
                    "space size requires an absolute expression",
                ));
            }
            let number = if space.negative_is_zero {
                value.number.max(0)
            } else {
                value.number
            };
            let Ok(count) = u64::try_from(number) else {
                pending_leb_error.get_or_insert_with(|| {
                    Error::new(*line, "space size requires a nonnegative value")
                });
                continue;
            };
            let size = count
                .checked_mul(u64::from(space.element_size))
                .ok_or_else(|| Error::new(*line, "space size overflow"))?;
            if size > 64 * 1024 * 1024 {
                return Err(Error::new(*line, "deferred space exceeds 64 MiB"));
            }
            if space_widths.len() <= i {
                space_widths.resize(i + 1, 0);
            }
            space_widths[i] = size;
            variable_changed |= a.space_widths.get(i).copied().unwrap_or(0) != size;
        }
        for (i, (section, start, padding, preferred, end, line)) in a.prefs.iter().enumerate() {
            let (sec, value) = a.eval_symbol(*end, 0).map_err(|e| Error::new(*line, e))?;
            if sec != Some(*section) || value < i128::from(start + padding) {
                return Err(Error::new(
                    *line,
                    ".prefalign end must follow its body in the same section",
                ));
            }
            let size = u64::try_from(value - i128::from(start + padding))
                .map_err(|_| Error::new(*line, ".prefalign body too large"))?;
            let power = size
                .min(1u64 << preferred)
                .next_power_of_two()
                .trailing_zeros();
            if pref_powers.len() <= i {
                pref_powers.resize(i + 1, 0);
            }
            pref_powers[i] = power;
            variable_changed |= a.pref_powers.get(i).copied().unwrap_or(0) != power;
        }
        if changed == 0 && !variable_changed {
            if let Some(e) = pending_leb_error {
                return Err(e);
            }
            // The layout is final: release the collected program before
            // the object writer allocates its own tables and output.
            *items = Items::new(items.features);
            let object = a.object()?;
            if let Some(start) = start {
                profile.object_ms = start.elapsed().as_secs_f64() * 1000.0;
            }
            return Ok(object);
        }
        // Settle the remaining relaxation over the first pass's positions,
        // leaving one layout pass to confirm it, rather than a pass per step.
        if changed != 0
            && !variable_changed
            && let Some(trace) = trace
        {
            relax::relax(&trace, &mut a, &mut items.symbolic);
        }
        first = false;
        if changed != 0 {
            checkpoint = None;
            period = 1;
            distance = 0;
        } else if variable_changed {
            // Bound repeated layout work explicitly, rather than treating a
            // pass count as proof of a cycle. Ordinary fixed layouts pay none
            // of this state-storage cost. This permits long acyclic chains.
            variable_work = variable_work.saturating_add(items.entries.len());
            if variable_work > 100_000_000 {
                return Err(Error::new(
                    0,
                    "variable-length directive layout resource limit (100 million item visits)",
                ));
            }
            distance += 1;
            if checkpoint.as_ref().is_some_and(|(lebs, prefs, spaces)| {
                lebs == &leb_widths && prefs == &pref_powers && spaces == &space_widths
            }) {
                return Err(Error::new(0, "cyclic variable-length directive layout"));
            }
            if checkpoint.is_none() || distance == period {
                checkpoint = Some((
                    leb_widths.clone(),
                    pref_powers.clone(),
                    space_widths.clone(),
                ));
                period = period.saturating_mul(2);
                distance = 0;
            }
        }
        a.symbols.clear();
        a.fixes.clear();
        recycled = (std::mem::take(&mut a.symbols), std::mem::take(&mut a.fixes));
        drop(a);
        if let Some(start) = start {
            profile
                .discard_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
impl Assembler {
    fn new() -> Result<Self, Error> {
        let mut a = Self::default();
        a.section(".text", Some(6), false)
            .map_err(|e| Error::new(0, e))?;
        a.previous = None;
        Ok(a)
    }
    // Binding-sensitive expressions must reach the linker, including aliases
    // whose underlying definition is interposable.
    fn local_symbol(&self, id: SymId, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        self.symbols.get(id).is_some_and(|s| {
            !s.global
                && !s.weak
                && s.alias.as_ref().is_none_or(|e| {
                    e.add.is_none_or(|n| self.local_symbol(n, depth + 1))
                        && e.subtract.is_none_or(|n| self.local_symbol(n, depth + 1))
                })
        })
    }
    fn item(&mut self, item: &Item, line: usize) -> Result<(), String> {
        match item {
            Item::Label(n) => {
                let id = self.symbols.intern(n);
                self.label(id)
            }
            Item::Directive(d) => self.shared_directive(d, line),
            _ if self.arm.is_some() => arm::item(self, item, line),
            _ => Err("AArch64 instruction/branch in x86 program".into()),
        }
    }
    fn object(mut self) -> Result<Vec<u8>, Error> {
        if self.cfi.is_some() {
            return Err(Error::new(0, "unterminated .cfi_startproc"));
        }
        metadata::finish(&mut self).map_err(|e| Error::new(0, e))?;
        self.finish()
    }
    fn offset(&self) -> u64 {
        self.sections[self.current].size
    }
    fn section(&mut self, name: &str, flags: Option<u64>, nobits: bool) -> Result<(), String> {
        self.section_metadata(name, flags, nobits, 0, None)
    }
    fn section_metadata(
        &mut self,
        name: &str,
        flags: Option<u64>,
        nobits: bool,
        entry_size: u64,
        group: Option<String>,
    ) -> Result<(), String> {
        self.section_full(sections::Declaration {
            name,
            flags,
            nobits,
            entry_size,
            group,
            section_type: None,
            unique: None,
        })
    }
    fn section_full(
        &mut self,
        declaration: sections::Declaration<impl AsRef<str>>,
    ) -> Result<(), String> {
        let sections::Declaration {
            name,
            flags,
            nobits,
            entry_size,
            group,
            section_type,
            unique,
        } = declaration;
        let name = name.as_ref();
        let nobits = nobits || matches!(name, ".bss" | ".tbss");
        let section_type = section_type.unwrap_or(if nobits {
            8
        } else if name == ".eh_frame" && self.arm.is_none() {
            0x7000_0001
        } else {
            1
        });
        // GNU as gives retained and ordinary contributions distinct input
        // sections, even when their names match (including the default .text).
        if let Some(i) = self.sections.iter().position(|s| {
            s.name == name
                && s.group == group
                && s.unique == unique
                && flags.is_none_or(|f| (s.flags ^ f) & 0x200000 == 0)
        }) {
            let s = &self.sections[i];
            // A bare .section name switches back without redeclaring attributes.
            if flags.is_some_and(|f| {
                f != s.flags
                    || nobits != s.nobits
                    || entry_size != s.entry_size
                    || section_type != s.section_type
            }) {
                return Err(format!("conflicting section attributes for {name}"));
            }
            self.previous = Some(self.current);
            self.current = i;
            return Ok(());
        }
        let flags = flags.unwrap_or_else(|| sections::default_flags(name));
        sections::validate(name, flags, nobits, entry_size, group.as_deref())?;
        self.previous = Some(self.current);
        self.current = self.sections.len();
        self.sections.push(Section {
            name: name.into(),
            flags,
            nobits,
            entry_size,
            group,
            section_type,
            unique,
            align: 1,
            data: Vec::with_capacity(
                self.size_hints
                    .get(self.sections.len())
                    .map_or(0, |&n| n as usize),
            ),
            size: 0,
        });
        Ok(())
    }
    // Explicit data in executable sections can contain instructions. Padding
    // uses the raw byte/fill helpers so it does not consume a pending .loc.
    fn data_location(&mut self, size: u64) {
        if size != 0 && self.sections[self.current].flags & 4 != 0 {
            self.debug.emit(self.current, self.offset());
        }
    }
    fn data_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        self.data_location(data.len() as u64);
        self.bytes(data)
    }
    fn data_zero(&mut self, size: u64, fill: u8) -> Result<(), String> {
        self.data_location(size);
        self.zero(size, fill)
    }
    fn data_fill(&mut self, size: u64, element_size: u8, value: i128) -> Result<(), String> {
        if size == 0 || element_size == 0 {
            return Ok(());
        }
        let pattern = u64::from(value as u32).to_le_bytes();
        let pattern = &pattern[..usize::from(element_size)];
        if self.sections[self.current].nobits && pattern.iter().any(|&b| b != 0) {
            return Err("nonzero fill in NOBITS section".into());
        }
        let start = self.sections[self.current].data.len();
        self.data_zero(size, 0)?;
        if !self.sections[self.current].nobits {
            for chunk in self.sections[self.current].data[start..].chunks_exact_mut(pattern.len()) {
                chunk.copy_from_slice(pattern);
            }
        }
        Ok(())
    }
    fn bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let s = &mut self.sections[self.current];
        if s.nobits && data.iter().any(|b| *b != 0) {
            return Err("nonzero initializer in NOBITS section".into());
        }
        s.size = s
            .size
            .checked_add(data.len() as u64)
            .ok_or("section size overflow")?;
        if !s.nobits {
            s.data.extend_from_slice(data)
        }
        Ok(())
    }
    fn zero(&mut self, n: u64, fill: u8) -> Result<(), String> {
        let s = &mut self.sections[self.current];
        if s.nobits && fill != 0 {
            return Err("nonzero fill in NOBITS section".into());
        }
        s.size = s.size.checked_add(n).ok_or("section size overflow")?;
        if !s.nobits {
            let len = usize::try_from(s.size).map_err(|_| "section too large")?;
            s.data
                .try_reserve(len - s.data.len())
                .map_err(|_| "section too large")?;
            s.data.resize(len, fill)
        }
        Ok(())
    }
    fn label(&mut self, id: SymId) -> Result<(), String> {
        let value = self.offset();
        let sym = self.symbols.entry(id);
        if sym.section.is_some() || sym.alias.is_some() || sym.common.is_some() {
            return Err(format!("duplicate symbol `{}`", self.symbols.name(id)));
        }
        sym.section = Some(self.current);
        sym.value = value;
        Ok(())
    }
    fn encoded(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.sections[self.current].nobits {
            return Err("instruction in NOBITS section".into());
        }
        self.debug.emit(self.current, self.offset());
        self.bytes(bytes)
    }
    /// Lay out a symbolic instruction from its cached encoding, unless an
    /// operand folds to a constant on this pass.
    fn symbolic(&mut self, cache: &symbolic::Cache, index: u32, line: usize) -> Result<(), String> {
        if let Some((bytes, fixups)) = cache.cached(index)
            && !fixups.iter().any(|f| {
                f.may_fold(self.aliases) && matches!(self.try_eval(&f.expr(), 0), Some((None, _)))
            })
        {
            if self.sections[self.current].nobits {
                return Err("instruction in NOBITS section".into());
            }
            self.debug.emit(self.current, self.offset());
            let at = self.offset();
            for f in fixups {
                self.fixes.push(Fix {
                    section: self.current,
                    offset: at + u64::from(f.offset),
                    width: f.width,
                    modifier: f.modifier,
                    expr: f.expr(),
                    kind: f.kind.into(),
                    line,
                })
            }
            return self.bytes(bytes);
        }
        let short = cache.entries[index as usize].short;
        let i = cache.instruction(index, &self.symbols);
        self.instruction(&i, line, short)
    }
    fn instruction(
        &mut self,
        i: &x86::Instruction,
        line: usize,
        short: bool,
    ) -> Result<(), String> {
        if self.sections[self.current].nobits {
            return Err("instruction in NOBITS section".into());
        }
        self.debug.emit(self.current, self.offset());
        // Intel operands known to be absolute at this point are constants.
        // Forward references remain symbolic, including later absolute aliases.
        let mut folded: Option<x86::Instruction> = None;
        for (index, operand) in i.operands.iter().enumerate() {
            let address = match operand {
                x86::Operand::Immediate(address) => address,
                x86::Operand::Memory(memory) => &memory.displacement,
                _ => continue,
            };
            // Only a difference or an alias (none yet, usually) is absolute.
            if address.modifier == Modifier::None
                && address.expression.add.is_some()
                && (self.aliases != 0 || address.expression.subtract.is_some())
                && let Some((None, value)) = {
                    let e = self.symbols.expr(&address.expression);
                    self.try_eval(&e, 0)
                }
            {
                if i.opcode.is_direct_branch() && matches!(operand, x86::Operand::Immediate(_)) {
                    return Err("direct branch target cannot be absolute".into());
                }
                let operand = &mut folded.get_or_insert_with(|| i.clone()).operands[index];
                let address = match operand {
                    x86::Operand::Immediate(address) => address,
                    x86::Operand::Memory(memory) => &mut memory.displacement,
                    _ => unreachable!(),
                };
                address.expression = Expression::constant(value);
            }
        }
        // Symbolic instructions are reencoded on each pass: folded absolute
        // expressions and short branches can depend on layout.
        let e = encode::encode(folded.as_ref().unwrap_or(i), short)?;
        let at = self.offset();
        for f in e.fixups {
            let mut expr = self.symbols.expr(&f.address.expression);
            expr.constant += f.adjust;
            self.fixes.push(Fix {
                section: self.current,
                offset: at + f.offset as u64,
                width: f.width,
                modifier: f.address.modifier,
                expr,
                kind: f.kind,
                line,
            })
        }
        self.bytes(&e.bytes)
    }
    /// Replace `.` with a label at the current position.
    fn capture_dot(&mut self, mut e: Expr) -> Result<Expr, String> {
        let Some(dot) = self.symbols.names.get(".") else {
            return Ok(e);
        };
        if e.add == Some(dot) || e.subtract == Some(dot) {
            let name = format!(".Lkeel.dot.{}", self.symbols.len());
            let id = self.symbols.intern(&name);
            self.label(id)?;
            if e.add == Some(dot) {
                e.add = Some(id)
            }
            if e.subtract == Some(dot) {
                e.subtract = Some(id)
            }
        }
        Ok(e)
    }
    fn integer(
        &mut self,
        width: u8,
        e: Expr,
        modifier: Modifier,
        line: usize,
    ) -> Result<(), String> {
        self.data_location(u64::from(width));
        let e = self.capture_dot(e)?;
        if !e.symbolic() {
            return self.bytes(&e.constant.to_le_bytes()[..width as usize]);
        }
        if self.sections[self.current].nobits {
            return Err("symbolic initializer in NOBITS section".into());
        }
        let kind = match (modifier, width) {
            (Modifier::None, 8) => 1,
            (Modifier::None, 4) => 10,
            (Modifier::None, 2) => 12,
            (Modifier::None, 1) => 14,
            (Modifier::DtpOff, 8) => 17,
            (Modifier::TpOff, 4) => 23,
            _ => return Err("unsupported data relocation".into()),
        };
        self.fixes.push(Fix {
            section: self.current,
            offset: self.offset(),
            width,
            modifier,
            expr: e,
            kind,
            line,
        });
        self.zero(u64::from(width), 0)
    }
    fn shared_directive(&mut self, d: &Directive, line: usize) -> Result<(), String> {
        match d {
            Directive::X86Integer { width, address } if matches!(width, 1 | 2 | 4 | 8) => {
                let e = self.symbols.expr(&address.expression);
                self.integer(*width, e, address.modifier, line)
            }
            Directive::ElfLifecycleArray { fini } => self.section_full(sections::Declaration {
                name: if *fini { ".fini_array" } else { ".init_array" },
                flags: Some(3),
                nobits: false,
                entry_size: 0,
                group: None,
                section_type: Some(if *fini { 15 } else { 14 }),
                unique: None,
            }),
            Directive::ElfSection {
                name,
                flags,
                nobits,
            } => self.section(name, Some(*flags), *nobits),
            Directive::ElfSectionMetadata {
                name,
                flags,
                nobits,
                entry_size,
                group,
            } => self.section_metadata(name, Some(*flags), *nobits, *entry_size, group.clone()),
            Directive::ElfType { name, function } => {
                let id = self.symbols.intern(name);
                self.symbols.entry(id).kind = if *function {
                    SymbolKind::Text
                } else {
                    SymbolKind::Data
                };
                Ok(())
            }
            Directive::ElfWeak(name) => {
                let id = self.symbols.intern(name);
                let s = self.symbols.entry(id);
                s.weak = true;
                s.local = false;
                s.global = true;
                Ok(())
            }
            Directive::ElfHidden(name) => {
                let id = self.symbols.intern(name);
                self.symbols.entry(id).hidden = true;
                Ok(())
            }
            Directive::Size { name, expression } => {
                let e = self.symbols.expr(expression);
                let id = self.symbols.intern(name);
                self.size(id, e)
            }

            Directive::Section {
                segment,
                name,
                kind,
            } if segment.is_empty() => {
                use keel_asm_ir::SectionKind::*;
                let (flags, nobits) = match kind {
                    Debug => (0, false),
                    ThreadData => (0x403, false),
                    ThreadZeroFill => (0x403, true),
                    ZeroFill => (3, true),
                    EhFrame => (2, false),
                    Data => (
                        if name == ".text" {
                            6
                        } else if name == ".rodata" {
                            2
                        } else {
                            3
                        },
                        false,
                    ),
                    _ => return Err("unsupported shared ELF section kind".into()),
                };
                self.section(name, Some(flags), nobits)
            }
            Directive::File { index, path } => {
                self.debug.file(*index, None, path.clone(), None, None)
            }
            Directive::Loc {
                file,
                line,
                column,
                prologue_end,
            } => self.debug.loc(
                self.current,
                self.offset(),
                &format!(
                    "{file} {line} {column}{}",
                    if *prologue_end { " prologue_end" } else { "" }
                ),
            ),
            Directive::Cfi(cfi) => metadata::shared_cfi(self, cfi),
            Directive::ZeroFill {
                symbol,
                size,
                alignment,
                thread_local,
            } => {
                let old = self.current;
                let previous = self.previous;
                self.section(if *thread_local { ".tbss" } else { ".bss" }, None, true)?;
                self.align(u32::from(*alignment), None)?;
                let id = self.symbols.intern(symbol);
                self.label(id)?;
                self.symbols.get_mut(id).unwrap().size =
                    Some(Box::new(Expr::constant(i128::from(*size))));
                self.zero(*size, 0)?;
                self.current = old;
                self.previous = previous;
                Ok(())
            }
            Directive::Global(s) => {
                let id = self.symbols.intern(s);
                self.global(id)
            }
            Directive::Bytes(b) => self.data_bytes(b),
            Directive::CString(b) => {
                self.data_bytes(b)?;
                self.data_bytes(&[0])
            }
            Directive::Integer {
                width,
                expression,
                got: false,
            } if matches!(width, 1 | 2 | 4 | 8) => {
                let e = self.symbols.expr(expression);
                self.integer(*width, e, Modifier::None, line)
            }
            Directive::Space(n) => self.data_zero(*n, 0),
            Directive::Align(n) => self.align(u32::from(*n), None),
            Directive::Set { name, expression } => {
                let e = self.symbols.expr(expression);
                let e = self.capture_dot(e)?;
                let id = self.symbols.intern(name);
                self.set(id, e)
            }
            Directive::Leb { signed, expression } => {
                let e = self.symbols.expr(expression);
                self.leb_expression(e, *signed, line)
            }
            _ => Err(format!(
                "unsupported shared directive for ELF: {d:?}; use ELF text spelling"
            )),
        }
    }
    fn align(&mut self, power: u32, max: Option<u64>) -> Result<(), String> {
        self.align_fill(power, max, AlignFill::Default)
    }
    fn align_fill(&mut self, power: u32, max: Option<u64>, fill: AlignFill) -> Result<(), String> {
        if power > 30 {
            return Err("alignment exceeds 2^30".into());
        }
        let align = 1u64 << power;
        let n = self.offset().wrapping_neg() & (align - 1);
        let section = &mut self.sections[self.current];
        section.align = section.align.max(align);
        let nops = match fill {
            AlignFill::Default => !section.nobits && section.flags & 4 != 0,
            AlignFill::Nop => true,
            AlignFill::Byte(_) => false,
        };
        if max.is_none_or(|m| n <= m) {
            if nops {
                self.nops(n)?;
            } else {
                self.zero(
                    n,
                    match fill {
                        AlignFill::Byte(byte) => byte,
                        _ => 0,
                    },
                )?;
            }
        }
        Ok(())
    }
    fn nops(&mut self, mut n: u64) -> Result<(), String> {
        if self.arm.is_some() {
            // Zero bytes before the next aligned instruction, then A64 NOPs.
            let prefix = ((4 - self.offset() % 4) % 4).min(n);
            self.zero(prefix, 0)?;
            n -= prefix;
            while n >= 4 {
                self.bytes(&0xd503201fu32.to_le_bytes())?;
                n -= 4;
            }
            return self.zero(n, 0);
        }
        // LLVM's generic x86-64 backend uses NOPs of at most ten bytes.
        const NOPS: [&[u8]; 11] = [
            &[],
            &[0x90],
            &[0x66, 0x90],
            &[0x0f, 0x1f, 0x00],
            &[0x0f, 0x1f, 0x40, 0x00],
            &[0x0f, 0x1f, 0x44, 0x00, 0x00],
            &[0x66, 0x0f, 0x1f, 0x44, 0x00, 0x00],
            &[0x0f, 0x1f, 0x80, 0, 0, 0, 0],
            &[0x0f, 0x1f, 0x84, 0, 0, 0, 0, 0],
            &[0x66, 0x0f, 0x1f, 0x84, 0, 0, 0, 0, 0],
            &[0x66, 0x2e, 0x0f, 0x1f, 0x84, 0, 0, 0, 0, 0],
        ];
        while n > 0 {
            let size = n.min(10) as usize;
            self.bytes(NOPS[size])?;
            n -= size as u64;
        }
        Ok(())
    }
    fn size(&mut self, id: SymId, e: Expr) -> Result<(), String> {
        let e = self.capture_dot(e)?;
        self.symbols.entry(id).size = Some(Box::new(e));
        Ok(())
    }
    fn global(&mut self, id: SymId) -> Result<(), String> {
        let s = self.symbols.entry(id);
        if s.weak {
            return Err("weak symbol changed binding to global".into());
        }
        s.local = false;
        s.global = true;
        Ok(())
    }
    fn set(&mut self, id: SymId, e: Expr) -> Result<(), String> {
        let depends = |e: &Expr| e.add == Some(id) || e.subtract == Some(id);
        let referenced = self.fixes.iter().any(|f| depends(&f.expr))
            || self.symbols.iter().any(|(_, s)| {
                s.alias.as_deref().is_some_and(depends) || s.size.as_deref().is_some_and(depends)
            });
        let sym = self.symbols.entry(id);
        if sym.alias.is_some() && referenced {
            return Err("cannot redefine an alias referenced by a deferred expression".into());
        }
        if sym.section.is_some()
            || sym
                .alias
                .as_ref()
                .is_some_and(|old| old.symbolic() || e.symbolic())
        {
            return Err(format!("duplicate symbol `{}`", self.symbols.name(id)));
        }
        sym.alias = Some(Box::new(e));
        self.aliases += 1;
        Ok(())
    }
    fn leb(&mut self, n: i128, signed: bool) -> Result<(), String> {
        let mut b = Vec::new();
        metadata::leb(&mut b, n, signed)?;
        self.bytes(&b)
    }
    fn leb_expression(&mut self, e: Expr, signed: bool, line: usize) -> Result<(), String> {
        self.data_location(1);
        if !e.symbolic() {
            return self.leb(e.constant, signed);
        }
        if self.sections[self.current].nobits {
            return Err("symbolic LEB128 in NOBITS section".into());
        }
        let e = self.capture_dot(e)?;
        let width = self.leb_widths.get(self.lebs.len()).copied().unwrap_or(1);
        self.lebs
            .push((self.current, self.offset(), e, signed, line));
        self.zero(width as u64, 0)
    }
    fn directive(&mut self, s: &str, line: usize) -> Result<(), String> {
        let assignment = x86::split_inline(s, '=');
        if assignment.len() == 2 {
            let name = x86::symbol(assignment[0])?;
            let e = self.symbols.expr(&x86::address(assignment[1])?.expression);
            let e = self.capture_dot(e)?;
            let id = self.symbols.intern(&name);
            return self.set(id, e);
        }
        let (op, args) = s
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((s.trim(), ""));
        let args = args.trim();
        let parts = x86::split_inline(args, ',');
        let num = |s: &str| -> Result<u64, String> {
            u64::try_from(x86::integer(s)?).map_err(|_| "expected nonnegative integer".into())
        };
        match op {
            "" => Ok(()),
            ".intel_syntax" if args == "noprefix" => Ok(()),
            ".text" | ".data" | ".bss" if args.is_empty() => self.section(op, None, op == ".bss"),
            ".code64" if args.is_empty() && self.arm.is_none() => Ok(()),
            ".code16" | ".code32" | ".code16gcc" => Err(format!(
                "{op} is unsupported; only 64-bit x86 assembly is supported"
            )),
            ".popsection" if args.is_empty() => {
                let section = self
                    .section_stack
                    .pop()
                    .ok_or(".popsection without .pushsection")?;
                self.previous = Some(self.current);
                self.current = section;
                Ok(())
            }
            ".previous" if args.is_empty() => {
                let section = self
                    .previous
                    .ok_or(".previous without a previous section")?;
                self.previous = Some(self.current);
                self.current = section;
                Ok(())
            }
            ".section" | ".pushsection" => {
                if op == ".pushsection" {
                    self.section_stack.push(self.current);
                }
                let section = sections::parse(&parts)?;
                self.section_full(section)
            }
            ".globl" | ".global" | ".local" | ".weak" | ".hidden" => {
                for name in parts {
                    let id = self.symbols.intern(&x86::symbol(name)?);
                    if matches!(op, ".globl" | ".global") {
                        self.global(id)?;
                        continue;
                    }
                    let s = self.symbols.entry(id);
                    match op {
                        ".local" => {
                            if s.weak || s.global || s.common.is_some() {
                                return Err("symbol changed binding to local".into());
                            }
                            s.global = false;
                            s.local = true;
                        }
                        ".hidden" => s.hidden = true,
                        ".weak" => {
                            s.weak = true;
                            s.local = false;
                            s.global = true
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(())
            }
            ".type" => {
                let (name, kind) = match parts.as_slice() {
                    [name, kind] => Some((*name, *kind)),
                    [args] => args.rsplit_once(char::is_whitespace),
                    _ => None,
                }
                .ok_or(".type expects a symbol and a type")?;
                let name = x86::symbol(name)?;
                let kind = match kind.trim() {
                    "@function" | "%function" | "STT_FUNC" => SymbolKind::Text,
                    "@object" | "%object" | "STT_OBJECT" => SymbolKind::Data,
                    "@tls_object" | "STT_TLS" => SymbolKind::Tls,
                    "@notype" | "%notype" | "STT_NOTYPE" => SymbolKind::Unknown,
                    _ => return Err("unsupported ELF symbol type".into()),
                };
                let id = self.symbols.intern(&name);
                self.symbols.entry(id).kind = kind;
                Ok(())
            }
            ".size" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let e = self.symbols.expr(&x86::address(parts[1])?.expression);
                let e = self.capture_dot(e)?;
                let id = self.symbols.intern(&name);
                self.symbols.entry(id).size = Some(Box::new(e));
                Ok(())
            }
            ".set" | ".equ" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let e = self.symbols.expr(&x86::address(parts[1])?.expression);
                let e = self.capture_dot(e)?;
                let id = self.symbols.intern(&name);
                self.set(id, e)
            }
            ".comm" | ".lcomm" if parts.len() == 2 || parts.len() == 3 => {
                let name = x86::symbol(parts[0])?;
                let size = num(parts[1])?;
                let align = if parts.len() == 3 {
                    num(parts[2])?
                } else {
                    size.checked_next_power_of_two().unwrap_or(16).min(16)
                };
                if !align.is_power_of_two() {
                    return Err("common alignment must be a power of two".into());
                }
                let id = self.symbols.intern(&name);
                let sym = self.symbols.entry(id);
                if sym.section.is_some() || sym.common.is_some() {
                    return Err("duplicate common symbol".into());
                }
                sym.common = Some((size, align));
                sym.kind = SymbolKind::Data;
                let local = sym.local || op == ".lcomm";
                if local && sym.weak {
                    return Err("weak local common symbols are unsupported".into());
                }
                sym.global = !local;
                if local {
                    let old = self.current;
                    let previous = self.previous;
                    self.section(".bss", None, true)?;
                    self.align(align.trailing_zeros(), None)?;
                    self.symbols.get_mut(id).unwrap().common = None;
                    self.label(id)?;
                    self.symbols.get_mut(id).unwrap().size =
                        Some(Box::new(Expr::constant(size.into())));
                    self.zero(size, 0)?;
                    self.current = old;
                    self.previous = previous;
                }
                Ok(())
            }
            ".p2align" | ".align" | ".balign" => {
                if parts.len() > 3 {
                    return Err("too many alignment operands".into());
                }
                let n = num(parts[0])?;
                let power = if op == ".p2align" {
                    u32::try_from(n).map_err(|_| "alignment too large")?
                } else {
                    if !n.is_power_of_two() {
                        return Err("alignment must be a power of two".into());
                    }
                    n.trailing_zeros()
                };
                let max = parts.get(2).map(|s| num(s)).transpose()?;
                let fill = parts
                    .get(1)
                    .filter(|s| !s.is_empty())
                    .map(|s| x86::integer(s).map(|n| n as u8))
                    .transpose()?;
                self.align_fill(power, max, fill.map_or(AlignFill::Default, AlignFill::Byte))
            }
            ".byte" | ".short" | ".word" | ".value" | ".2byte" | ".long" | ".int" | ".4byte"
            | ".quad" | ".8byte" => {
                let width = match op {
                    ".byte" => 1,
                    ".short" | ".word" | ".value" | ".2byte" => 2,
                    ".long" | ".int" | ".4byte" => 4,
                    _ => 8,
                };
                for p in parts {
                    let address = x86::address(p)?;
                    let e = &address.expression;
                    // LLVM accepts a literal that fits the width as either signed or
                    // unsigned. Values it would wrap at 64 bits are rejected here.
                    let bits = u32::from(width) * 8;
                    if e.add.is_none()
                        && e.subtract.is_none()
                        && (e.constant < -(1i128 << (bits - 1)) || e.constant >= 1i128 << bits)
                    {
                        return Err(format!("out of range literal value for `{op}`"));
                    }
                    let e = self.symbols.expr(e);
                    self.integer(width, e, address.modifier, line)?
                }
                Ok(())
            }
            ".org" if parts.len() == 1 || parts.len() == 2 => {
                let address = x86::address(parts[0])?;
                if address.modifier != Modifier::None {
                    return Err(".org relocation modifiers are unsupported".into());
                }
                let e = self.symbols.expr(&address.expression);
                let e = self.capture_dot(e)?;
                // ELF section names denote their section's origin, even before
                // a relocation has caused a section symbol to be materialized.
                for id in e.add.into_iter().chain(e.subtract) {
                    if self.symbols.get(id).is_none()
                        && let Some(section) = self
                            .sections
                            .iter()
                            .position(|s| s.name == self.symbols.name(id))
                    {
                        let sym = self.symbols.entry(id);
                        sym.section = Some(section);
                        sym.kind = SymbolKind::Section;
                    }
                }
                let (section, value) = self.eval(&e, 0)?;
                if section.is_some_and(|s| s != self.current) {
                    return Err(".org requires the current section or an absolute offset".into());
                }
                let target =
                    u64::try_from(value).map_err(|_| ".org requires a nonnegative offset")?;
                let size = target
                    .checked_sub(self.offset())
                    .ok_or(".org cannot move backwards")?;
                let fill = parts
                    .get(1)
                    .map(|s| x86::integer(s))
                    .transpose()?
                    .unwrap_or(0) as u8;
                // GNU ignores an .org fill in NOBITS sections.
                self.zero(
                    size,
                    if self.sections[self.current].nobits {
                        0
                    } else {
                        fill
                    },
                )
            }
            ".fill" => {
                if parts.is_empty() || parts.len() > 3 {
                    return Err(".fill expects repeat[, size[, value]]".into());
                }
                let size = parts
                    .get(1)
                    .map(|s| x86::integer(s))
                    .transpose()?
                    .unwrap_or(1)
                    .clamp(0, 8) as u8;
                let value = parts
                    .get(2)
                    .map(|s| x86::integer(s))
                    .transpose()?
                    .unwrap_or(0);
                if size == 0 {
                    return Ok(());
                }
                if let Ok(count) = x86::integer(parts[0]) {
                    let total = u64::try_from(count.max(0))
                        .ok()
                        .and_then(|n| n.checked_mul(u64::from(size)))
                        .filter(|&n| n <= 64 * 1024 * 1024)
                        .ok_or("fill size exceeds 64 MiB")?;
                    return self.data_fill(total, size, value);
                }
                let expression = x86::expression::parse(parts[0])?;
                let width = self
                    .space_widths
                    .get(self.spaces.len())
                    .copied()
                    .unwrap_or(0);
                self.spaces.push(Space {
                    section: self.current,
                    offset: self.offset(),
                    expression,
                    line,
                    element_size: size,
                    negative_is_zero: true,
                });
                self.data_fill(width, size, value)
            }
            ".zero" | ".space" | ".skip" => {
                if parts.len() > 2 {
                    return Err("too many space operands".into());
                }
                let fill = parts.get(1).map(|s| num(s)).transpose()?.unwrap_or(0);
                let fill = u8::try_from(fill).map_err(|_| "fill exceeds a byte")?;
                if let Ok(n) = num(parts[0]) {
                    if n > 64 * 1024 * 1024 {
                        return Err("space size exceeds 64 MiB".into());
                    }
                    return self.data_zero(n, fill);
                }
                let expression = x86::expression::parse(parts[0])?;
                let width = self
                    .space_widths
                    .get(self.spaces.len())
                    .copied()
                    .unwrap_or(0);
                self.spaces.push(Space {
                    section: self.current,
                    offset: self.offset(),
                    expression,
                    line,
                    element_size: 1,
                    negative_is_zero: false,
                });
                self.data_zero(width, fill)
            }
            ".ascii" | ".asciz" | ".string" => {
                for p in parts {
                    self.data_bytes(&x86::unquote(p)?)?;
                    if op != ".ascii" {
                        self.data_bytes(&[0])?
                    }
                }
                Ok(())
            }
            ".uleb128" | ".sleb128" => {
                for p in parts {
                    let address = x86::address(p)?;
                    if address.modifier != Modifier::None {
                        return Err("LEB128 relocation modifiers are unsupported".into());
                    }
                    let e = self.symbols.expr(&address.expression);
                    self.leb_expression(e, op == ".sleb128", line)?
                }
                Ok(())
            }
            ".file" if args.starts_with('"') => {
                let file = x86::unquote(args)?;
                if file.contains(&0) {
                    return Err("file name contains NUL".into());
                }
                self.source_files.push(file);
                Ok(())
            }
            ".file" => self.debug.parse_file(args),
            ".loc" => self.debug.loc(self.current, self.offset(), args),
            ".ident" => {
                let value = x86::unquote(args)?;
                let old = self.current;
                let previous = self.previous;
                self.section_metadata(".comment", Some(0x30), false, 1, None)?;
                if self.offset() == 0 {
                    self.bytes(&[0])?;
                }
                self.bytes(&value)?;
                self.bytes(&[0])?;
                self.current = old;
                self.previous = previous;
                Ok(())
            }
            ".addrsig" if args.is_empty() => {
                self.addrsig.get_or_insert_with(Vec::new);
                Ok(())
            }
            ".addrsig_sym" => {
                let name = x86::symbol(args)?;
                self.addrsig.get_or_insert_with(Vec::new).push(name);
                Ok(())
            }
            ".prefalign" if parts.len() == 3 => {
                let preferred = u32::try_from(num(parts[0])?).map_err(|_| "alignment too large")?;
                if preferred > 30 {
                    return Err("alignment exceeds 2^30".into());
                }
                let end = self.symbols.intern(&x86::symbol(parts[1])?);
                let fill = if parts[2] == "nop" {
                    AlignFill::Nop
                } else {
                    AlignFill::Byte(
                        u8::try_from(num(parts[2])?).map_err(|_| "fill exceeds a byte")?,
                    )
                };
                let power = self.pref_powers.get(self.prefs.len()).copied().unwrap_or(0);
                let start = self.offset();
                self.align_fill(power, None, fill)?;
                self.prefs.push((
                    self.current,
                    start,
                    self.offset() - start,
                    preferred,
                    end,
                    line,
                ));
                Ok(())
            }
            ".cfi_startproc"
            | ".cfi_endproc"
            | ".cfi_def_cfa_offset"
            | ".cfi_def_cfa_register"
            | ".cfi_def_cfa"
            | ".cfi_offset"
            | ".cfi_restore"
            | ".cfi_remember_state"
            | ".cfi_restore_state"
            | ".cfi_adjust_cfa_offset"
            | ".cfi_personality"
            | ".cfi_lsda" => metadata::cfi(self, op, args),
            _ => Err(format!("unsupported ELF directive `{op}`")),
        }
    }
    /// [`Self::eval_symbol`] for callers that discard why it fails, without
    /// formatting a diagnostic: forward references fail on every early pass.
    fn try_eval_symbol(&self, id: SymId, depth: usize) -> Option<(Option<usize>, i128)> {
        if depth > 64 {
            return None;
        }
        let s = self.symbols.get(id)?;
        if let Some(e) = &s.alias {
            return self.try_eval(e, depth + 1);
        }
        Some((Some(s.section?), s.value.into()))
    }
    fn try_eval(&self, e: &Expr, depth: usize) -> Option<(Option<usize>, i128)> {
        let (mut section, mut value) = (None, e.constant);
        if let Some(n) = e.add {
            let (s, v) = self.try_eval_symbol(n, depth)?;
            section = s;
            value += v
        }
        if let Some(n) = e.subtract {
            let (s, v) = self.try_eval_symbol(n, depth)?;
            if s != section {
                return None;
            }
            section = None;
            value -= v
        }
        Some((section, value))
    }
    fn eval_symbol(&self, id: SymId, depth: usize) -> Result<(Option<usize>, i128), String> {
        if depth > 64 {
            return Err("cyclic or excessively nested symbol alias".into());
        }
        let undefined = || format!("undefined symbol `{}` in expression", self.symbols.name(id));
        let s = self.symbols.get(id).ok_or_else(undefined)?;
        if let Some(e) = &s.alias {
            return self.eval(e, depth + 1);
        }
        if let Some(section) = s.section {
            Ok((Some(section), s.value.into()))
        } else {
            Err(undefined())
        }
    }
    fn eval(&self, e: &Expr, depth: usize) -> Result<(Option<usize>, i128), String> {
        let (mut section, mut value) = (None, e.constant);
        if let Some(n) = e.add {
            let (s, v) = self.eval_symbol(n, depth)?;
            section = s;
            value += v
        }
        if let Some(n) = e.subtract {
            let (s, v) = self.eval_symbol(n, depth)?;
            if s != section {
                return Err("symbol difference requires the same section".into());
            }
            section = None;
            value -= v
        }
        Ok((section, value))
    }
    fn symbol_attributes(&self, id: SymId, depth: usize) -> (SymbolKind, Option<&Expr>) {
        let Some(s) = self.symbols.get(id) else {
            return (SymbolKind::Unknown, None);
        };
        let inherited = if depth < 64 {
            s.alias
                .as_deref()
                .filter(|e| e.subtract.is_none())
                .and_then(|e| e.add)
                .map(|n| self.symbol_attributes(n, depth + 1))
                .unwrap_or((SymbolKind::Unknown, None))
        } else {
            (SymbolKind::Unknown, None)
        };
        (
            if s.kind == SymbolKind::Unknown {
                inherited.0
            } else {
                s.kind
            },
            s.size.as_deref().or(inherited.1),
        )
    }
    fn finish(mut self) -> Result<Vec<u8>, Error> {
        let err = |s: String| Error::new(0, s);
        // Ensure even undefined references receive symbol table entries.
        let section_names: Vec<_> = self
            .sections
            .iter()
            .map(|s| self.symbols.names.get(&s.name))
            .collect();
        for f in &self.fixes {
            if let Some(n) = f.expr.add {
                let section = section_names.iter().position(|&s| s == Some(n));
                let sym = self.symbols.entry(n);
                if sym.section.is_none()
                    && sym.alias.is_none()
                    && sym.common.is_none()
                    && let Some(i) = section
                {
                    sym.section = Some(i);
                    sym.kind = SymbolKind::Section;
                }
                if self.arm.is_some() && matches!(f.kind, 541 | 542) {
                    sym.kind = SymbolKind::Tls;
                }
                if matches!(
                    f.modifier,
                    Modifier::TpOff | Modifier::GotTpOff | Modifier::DtpOff
                ) {
                    sym.kind = SymbolKind::Tls;
                }
            }
        }
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, section) in self.sections.iter().enumerate() {
            if let Some(signature) = &section.group {
                groups.entry(signature.clone()).or_default().push(i);
            }
        }
        // A signature may also be an ordinary external relocation target.
        // Only group-only names may receive a synthetic .group definition.
        let ordinary_references: FxHashSet<_> = if groups.is_empty() {
            FxHashSet::default()
        } else {
            self.fixes
                .iter()
                .map(|f| &f.expr)
                .chain(
                    self.symbols
                        .values()
                        .flat_map(|s| s.alias.iter().chain(s.size.iter()))
                        .map(|e| &**e),
                )
                .flat_map(|e| e.add.into_iter().chain(e.subtract))
                .collect()
        };
        let synthetic_signatures: BTreeSet<_> = groups
            .keys()
            .filter(|name| {
                self.symbols.names.get(name).is_none_or(|id| {
                    !ordinary_references.contains(&id)
                        && self.symbols.get(id).is_none_or(|s| {
                            s.section.is_none() && s.alias.is_none() && s.common.is_none()
                        })
                })
            })
            .cloned()
            .collect();
        for signature in groups.keys() {
            let id = self.symbols.intern(signature);
            self.symbols.entry(id);
        }
        let mut object = Object::new(
            BinaryFormat::Elf,
            if self.arm.is_some() {
                Architecture::Aarch64
            } else {
                Architecture::X86_64
            },
            Endianness::Little,
        );
        for file in &self.source_files {
            object.add_file_symbol(file.clone());
        }
        let mut sections = Vec::new();
        for s in &self.sections {
            let kind = if s.nobits {
                if s.flags & 0x400 != 0 {
                    SectionKind::UninitializedTls
                } else {
                    SectionKind::UninitializedData
                }
            } else if s.flags & 4 != 0 {
                SectionKind::Text
            } else if s.flags & 0x400 != 0 {
                SectionKind::Tls
            } else if s.flags & 1 != 0 {
                SectionKind::Data
            } else if s.flags & 2 != 0 {
                SectionKind::ReadOnlyData
            } else {
                SectionKind::Other
            };
            let id = object.add_section(Vec::new(), s.name.as_bytes().to_vec(), kind);
            object.section_mut(id).flags = SectionFlags::Elf { sh_flags: s.flags };
            // Initialized data moves in after relocation patching below.
            if s.nobits {
                object.append_section_bss(id, s.size, s.align);
            }
            sections.push(id);
        }
        let identity: FxHashSet<_> = self
            .fixes
            .iter()
            .filter(|f| {
                // A 32-bit data difference against this section becomes PC32
                // below. An absolute positive symbol still needs its identity
                // in that relocation, just like an explicit PC-relative fixup.
                let pc_relative = matches!(f.kind, 2 | 4)
                    || matches!(f.kind, 10 | 11)
                        && f.width == 4
                        && f.expr.subtract.is_some_and(|id| {
                            self.try_eval_symbol(id, 0)
                                .is_some_and(|(section, _)| section == Some(f.section))
                        });
                let merge_target = f.expr.add.is_some_and(|id| {
                    self.try_eval_symbol(id, 0).is_some_and(|(section, _)| {
                        section.is_some_and(|i| self.sections[i].flags & 0x12 == 0x12)
                    })
                });
                merge_target
                    || self.arm.is_some() && matches!(f.kind, 311 | 312 | 541 | 542)
                    || f.modifier != Modifier::None
                    || pc_relative
                        && f.expr.add.is_some_and(|id| {
                            self.try_eval_symbol(id, 0)
                                .is_some_and(|(section, _)| section.is_none())
                        })
            })
            .filter_map(|f| f.expr.add)
            .collect();
        let mut retained = identity.clone();
        if let Some(names) = &self.addrsig {
            retained.extend(names.iter().filter_map(|n| self.symbols.names.get(n)));
        }
        let mut symbols = vec![None; self.symbols.names.len()];
        // Temporary local labels without an ELF symbol table entry.
        let discard = |id: SymId, name: &str, s: &Sym, section: Option<usize>, tls: bool| {
            name.starts_with(".L")
                && !s.global
                && !s.weak
                && !s.hidden
                && s.common.is_none()
                && !tls
                && !retained.contains(&id)
                && !groups.contains_key(name)
                && (section.is_some() || s.alias.is_some())
                && self.local_symbol(id, 0)
        };
        // Most symbols are plain local labels that cannot fail evaluation
        // below and are discarded there; only the rest need ordering.
        let mut entries: Vec<_> = self
            .symbols
            .iter()
            .map(|(id, s)| (self.symbols.name(id), id, s))
            .filter(|&(name, id, s)| {
                let tls = s.kind == SymbolKind::Tls
                    || s.section
                        .is_some_and(|i| self.sections[i].flags & 0x400 != 0);
                s.kind == SymbolKind::Section
                    || s.alias.is_some()
                    || s.size.is_some()
                    || !discard(id, name, s, s.section, tls)
            })
            .collect();
        entries.sort_unstable_by_key(|&(name, _, _)| name);
        for (name, id, s) in entries {
            if s.kind == SymbolKind::Section {
                symbols[id as usize] = Some(object.section_symbol(sections[s.section.unwrap()]));
                continue;
            }
            let (section, value) = if s.alias.is_some() {
                let (sec, value) = self.eval_symbol(id, 0).map_err(err)?;
                // An absolute ELF64 symbol may spell a negative machine word.
                // Section offsets and values wider than one word still fail.
                let value = if sec.is_none()
                    && (i128::from(i64::MIN)..=i128::from(u64::MAX)).contains(&value)
                {
                    value as u64
                } else {
                    u64::try_from(value).map_err(|_| err("alias value out of range".into()))?
                };
                (sec, value)
            } else {
                (s.section, s.value)
            };
            let (kind, size_expression) = self.symbol_attributes(id, 0);
            let size = if let Some(e) = size_expression {
                let (sec, n) = self.eval(e, 0).map_err(err)?;
                if sec.is_some() {
                    return Err(err("symbol size must be absolute".into()));
                }
                u64::try_from(n).map_err(|_| err("symbol size out of range".into()))?
            } else {
                0
            };
            let tls = kind == SymbolKind::Tls
                || section.is_some_and(|i| self.sections[i].flags & 0x400 != 0);
            if discard(id, name, s, section, tls) {
                continue;
            }
            let synthetic = synthetic_signatures.contains(name);
            let undefined = section.is_none() && s.alias.is_none() && !synthetic;
            let symbol = Symbol {
                name: name.as_bytes().to_vec(),
                value,
                size,
                kind: if tls { SymbolKind::Tls } else { kind },
                scope: if s.global || s.weak || undefined {
                    SymbolScope::Linkage
                } else {
                    SymbolScope::Compilation
                },
                weak: s.weak,
                section: match section {
                    Some(i) => SymbolSection::Section(sections[i]),
                    // A temporary placeholder, patched to the generated .group
                    // section after serialization. Keep it out of layout/evaluation.
                    None if synthetic || s.alias.is_some() => SymbolSection::Absolute,
                    None => SymbolSection::Undefined,
                },
                flags: SymbolFlags::Elf {
                    st_info: ((if s.weak {
                        2
                    } else if s.global || undefined {
                        1
                    } else {
                        0
                    }) << 4)
                        | (if tls {
                            6
                        } else {
                            match kind {
                                SymbolKind::Text => 2,
                                SymbolKind::Data => 1,
                                _ => 0,
                            }
                        }),
                    st_other: if s.hidden { 2 } else { 0 },
                },
            };
            let symbol_id = if let Some((size, align)) = s.common {
                // ELF common symbols store alignment in st_value. The writer's
                // add_common_symbol API only applies `align` for Mach-O BSS.
                let mut symbol = symbol;
                symbol.value = align;
                object.add_common_symbol(symbol, size, align)
            } else {
                object.add_symbol(symbol)
            };
            symbols[id as usize] = Some(symbol_id);
        }
        let symbols = |id: SymId| symbols[id as usize].expect("no such symbol");
        for (signature, members) in &groups {
            object.add_comdat(Comdat {
                kind: ComdatKind::Any,
                symbol: symbols(self.symbols.names.get(signature).unwrap()),
                sections: members.iter().map(|&i| sections[i]).collect(),
            });
        }
        for f in &self.fixes {
            let expr = &f.expr;
            let mut kind = f.kind;
            let mut addend = expr.constant;
            let name = expr
                .add
                .ok_or_else(|| Error::new(f.line, "relocation requires a positive symbol"))?;
            let defined = self.try_eval_symbol(name, 0);
            if self.arm.is_some()
                && matches!(kind, 273 | 274 | 279 | 280 | 282 | 283)
                && defined.is_some_and(|(s, _)| s == Some(f.section))
                && self.local_symbol(name, 0)
            {
                let delta = defined.unwrap().1 + addend - i128::from(f.offset);
                let bytes =
                    &mut self.sections[f.section].data[f.offset as usize..f.offset as usize + 4];
                arm::resolve(kind, delta, bytes).map_err(|e| Error::new(f.line, e))?;
                continue;
            }
            let absolute = if expr.subtract.is_some() {
                self.try_eval(expr, 0)
                    .filter(|(s, _)| s.is_none())
                    .map(|(_, n)| n)
            } else if matches!(kind, 1 | 10 | 11 | 12 | 14)
                && defined.is_some_and(|(s, _)| s.is_none())
            {
                defined.map(|(_, n)| n + addend)
            } else if matches!(kind, 2 | 4 | 15)
                && defined.is_some_and(|(s, _)| s == Some(f.section))
                && self.local_symbol(name, 0)
            {
                defined.map(|(_, n)| n + addend - i128::from(f.offset))
            } else {
                None
            };
            if let Some(value) = absolute {
                let value = if kind == 11 {
                    encode::signed_word64(value)
                } else {
                    value
                };
                let bits = u32::from(f.width) * 8;
                let signed = matches!(kind, 2 | 4 | 15 | 11 | 23);
                if value < -(1i128 << (bits - 1)) || value >= (1i128 << (bits - u32::from(signed)))
                {
                    return Err(Error::new(
                        f.line,
                        "resolved value does not fit relocation field",
                    ));
                }
                let offset = usize::try_from(f.offset)
                    .map_err(|_| Error::new(f.line, "offset too large"))?;
                self.sections[f.section].data[offset..offset + usize::from(f.width)]
                    .copy_from_slice(&value.to_le_bytes()[..usize::from(f.width)]);
                continue;
            }
            if let Some(sub) = expr.subtract {
                let (sec, value) = self
                    .eval_symbol(sub, 0)
                    .map_err(|e| Error::new(f.line, e))?;
                if sec != Some(f.section) || f.width != 4 || !matches!(kind, 10 | 11) {
                    return Err(Error::new(
                        f.line,
                        "unsupported cross-section symbol difference",
                    ));
                }
                kind = 2;
                addend += i128::from(f.offset) - value;
            }
            // Relocations against section-relative aliases use their underlying section symbol.
            let debug_reference = self.sections[f.section].name.starts_with(".debug_");
            let symbol = if defined.is_some()
                && (debug_reference || !identity.contains(&name))
                && f.modifier == Modifier::None
                && self.local_symbol(name, 0)
                && self.symbols[name].kind != SymbolKind::Tls
                && !defined.is_some_and(|(section, _)| {
                    section.is_some_and(|sec| self.sections[sec].flags & 0x400 != 0)
                }) {
                let (sec, value) = defined.ok_or_else(|| Error::new(f.line, "unresolved alias"))?;
                if let Some(sec) = sec {
                    addend += value;
                    object.section_symbol(sections[sec])
                } else {
                    // GOT/TLS modifiers still require a linker relocation
                    // even when the referenced symbol is absolute.
                    symbols(name)
                }
            } else {
                symbols(name)
            };
            object
                .add_relocation(
                    sections[f.section],
                    Relocation {
                        offset: f.offset,
                        symbol,
                        addend: i64::try_from(addend)
                            .map_err(|_| Error::new(f.line, "relocation addend exceeds i64"))?,
                        flags: RelocationFlags::Elf {
                            r_type: if self.arm.is_some() {
                                arm::relocation(kind).map_err(|e| Error::new(f.line, e))?
                            } else {
                                kind
                            },
                        },
                    },
                )
                .map_err(|e| Error::new(f.line, e.to_string()))?;
        }
        // Move rather than copy the patched data: large sections would
        // otherwise be resident twice before serialization makes a third copy.
        for (i, s) in self.sections.iter_mut().enumerate() {
            if !s.nobits {
                object.set_section_data(sections[i], std::mem::take(&mut s.data), s.align);
            }
        }
        let mut bytes = object.write().map_err(|e| err(e.to_string()))?;
        sections::finalize(
            &mut bytes,
            &self.sections,
            &groups,
            &synthetic_signatures,
            self.addrsig.as_deref(),
        )
        .map_err(err)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_probe_only_retains_valid_single_operands() {
        for (op, width) in [
            (".byte", 1),
            (".short", 2),
            (".word", 2),
            (".long", 4),
            (".int", 4),
            (".quad", 8),
        ] {
            for literal in ["0", "+1", "-1", "010", "0Xf", "0b11"] {
                let (bytes, actual_width) = literal_data(&format!("{op} {literal}\n")).unwrap();
                let address = x86::address(literal).unwrap();
                assert_eq!(actual_width, width);
                assert_eq!(
                    &bytes[..usize::from(width)],
                    &address.expression.constant.to_le_bytes()[..usize::from(width)]
                );
            }
        }
        for text in [
            ".byte symbol",
            ".byte .",
            ".byte 1,2",
            ".byte 1+0",
            ".byte --1",
            ".byte + 1",
            ".byte 0x+1",
            ".byte 08",
            ".byte 0B1",
            ".byte 256",
            ".byte -129",
            ".short -32769",
            ".quad 18446744073709551616",
            ".quad -9223372036854775809",
            ".byte 1\n.byte 2\n",
            ".byte",
            ".ascii \"a\"",
        ] {
            assert!(literal_data(text).is_none(), "{text}");
        }
    }
}
