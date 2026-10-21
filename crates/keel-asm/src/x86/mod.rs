//! Native x86-64 ELF assembler. Intel/AT&T source is parsed once into `keel-asm-ir`.
//! Parsed items are laid out until optimistic short branches fit their final layout.
mod dwarf;
mod encode;
mod metadata;
mod sections;
use crate::Error;
use crate::fxhash::{FxHashMap, FxHashSet};
pub use keel_asm_ir::x86::{Features, Syntax};
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
    size: Option<Box<Expression>>,
    alias: Option<Box<Expression>>,
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
/// Symbols by name. The hash index holds only names and positions in a
/// dense table, keeping its power-of-two (often half-empty) buckets small.
/// Layout passes declare the same names, so [`Self::clear`] keeps the index
/// and its slots: a slot is visible once declared in the current pass.
#[derive(Default)]
struct Symbols {
    index: FxHashMap<Box<str>, u32>,
    table: Vec<Sym>,
    /// The pass that last declared each slot.
    declared: Vec<u32>,
    pass: u32,
    len: usize,
}
impl Symbols {
    fn len(&self) -> usize {
        self.len
    }
    fn slot(&self, name: &str) -> Option<usize> {
        let i = *self.index.get(name)? as usize;
        (self.declared[i] == self.pass).then_some(i)
    }
    fn get(&self, name: &str) -> Option<&Sym> {
        self.slot(name).map(|i| &self.table[i])
    }
    fn get_mut(&mut self, name: &str) -> Option<&mut Sym> {
        self.slot(name).map(|i| &mut self.table[i])
    }
    /// The named symbol, declared with default attributes on first use.
    fn entry(&mut self, name: &str) -> &mut Sym {
        let i = match self.index.get(name) {
            Some(&i) => {
                let i = i as usize;
                if self.declared[i] != self.pass {
                    self.declared[i] = self.pass;
                    self.table[i] = Sym::default();
                    self.len += 1;
                }
                i
            }
            None => {
                let i = self.table.len();
                self.index.insert(name.into(), i as u32);
                self.table.push(Sym::default());
                self.declared.push(self.pass);
                self.len += 1;
                i
            }
        };
        &mut self.table[i]
    }
    fn insert(&mut self, name: &str, sym: Sym) {
        *self.entry(name) = sym;
    }
    fn iter(&self) -> impl Iterator<Item = (&str, &Sym)> {
        self.index
            .iter()
            .filter(|&(_, &i)| self.declared[i as usize] == self.pass)
            .map(|(name, &i)| (&**name, &self.table[i as usize]))
    }
    fn values(&self) -> impl Iterator<Item = &Sym> {
        self.table
            .iter()
            .zip(&self.declared)
            .filter(|&(_, &pass)| pass == self.pass)
            .map(|(sym, _)| sym)
    }
    /// Begin the next pass with no symbols declared.
    fn clear(&mut self) {
        self.pass += 1;
        self.len = 0;
    }
}
impl std::ops::Index<&str> for Symbols {
    type Output = Sym;
    fn index(&self, name: &str) -> &Sym {
        self.get(name).expect("no such symbol")
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
    address: Address,
    kind: u32,
    line: usize,
}
#[derive(Default)]
struct Assembler {
    sections: Vec<Section>,
    symbols: Symbols,
    fixes: Vec<Fix>,
    current: usize,
    cfi: Option<metadata::Frame>,
    frames: Vec<metadata::Frame>,
    debug: dwarf::Debug,
    source_files: Vec<Vec<u8>>,
    addrsig: Option<Vec<String>>,
    leb_widths: Vec<usize>,
    lebs: Vec<(usize, u64, Expression, bool, usize)>,
    pref_powers: Vec<u32>,
    /// Section sizes from the previous layout pass, which declares the same
    /// sections in the same order: reserve them instead of regrowing.
    size_hints: Vec<u64>,
    prefs: Vec<(usize, u64, u64, u32, String, usize)>,
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
    let mut items = Items::new(features);
    items.parser = x86::SourceParser::new(syntax);
    items.parse(source, 1)?;
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
    let mut items = Items::new(features);
    items.entries.reserve(program.items.len());
    for (index, item) in program.items.iter().enumerate() {
        match item {
            Item::Text(source) => items.parse(source, index + 1)?,
            Item::X86Instruction(i) => {
                if !items.encode(i, index + 1) {
                    items.instruction(Ref::Borrowed(i), index + 1)
                }
            }
            _ => items.push(Entry::Item {
                item: Ref::Borrowed(item),
                line: index + 1,
            }),
        }
    }
    assemble_items(items.finish()?)
}

/// Incremental form of [`assemble_program_object_with_features`]: programs
/// pushed in order assemble exactly like their concatenation, with the same
/// item positions in diagnostics. Each chunk is consumed as it arrives, so a
/// whole module's structured program never needs to be resident at once.
pub struct ProgramAssembler {
    items: Items<'static>,
    next_line: usize,
    error: Option<Error>,
}
impl ProgramAssembler {
    pub fn new(features: x86::Features) -> Self {
        Self {
            items: Items::new(features),
            next_line: 1,
            error: None,
        }
    }
    pub fn push(&mut self, program: Program) {
        if self.error.is_some() {
            return;
        }
        let items = &mut self.items;
        for item in program.items {
            let line = self.next_line;
            self.next_line += 1;
            match item {
                Item::Text(source) => {
                    if let Err(e) = items.parse(&source, line) {
                        self.error = Some(e);
                        return;
                    }
                }
                Item::X86Instruction(i) => {
                    if !items.encode(&i, line) {
                        items.instruction(Ref::Owned(Box::new(i)), line)
                    }
                }
                // Equivalent to the borrowed label item, without its box.
                Item::Label(name) => items.push(Entry::Label {
                    name: name.into_boxed_str(),
                    line,
                }),
                item => items.push(Entry::Item {
                    item: Ref::Owned(Box::new(item)),
                    line,
                }),
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

/// Borrowed from a caller's program, or owned after parsing explicit text.
enum Ref<'a, T> {
    Borrowed(&'a T),
    Owned(Box<T>),
}
impl<T> std::ops::Deref for Ref<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Borrowed(t) => t,
            Self::Owned(t) => t,
        }
    }
}
/// One layout unit. Diagnostics use `line`.
enum Entry<'a> {
    /// Consecutive symbol-free instructions, encoded once at collection: their
    /// bytes are `encoded[previous run end..end]`. `line` is the first one's;
    /// only it can fail layout, as the section cannot change within a run.
    Encoded {
        end: usize,
        line: usize,
    },
    /// `short` is a direct jump still assumed to fit rel8.
    Instruction {
        instruction: Ref<'a, x86::Instruction>,
        line: usize,
        short: bool,
    },
    Label {
        name: Box<str>,
        line: usize,
    },
    /// A single, in-range literal data operand, decoded once. Keep bytes rather
    /// than i128 so this variant does not increase Entry's alignment or size.
    LiteralData {
        bytes: [u8; 8],
        width: u8,
        line: usize,
    },
    /// Directive lines from explicit text, applied in order.
    Text {
        text: Box<str>,
        line: usize,
    },
    Item {
        item: Ref<'a, Item>,
        line: usize,
    },
}
/// Layout input. Most instructions in ordinary assembly are symbol-free and
/// encode identically on every relaxation pass; retaining their parsed
/// operands for each pass would cost far more than their bytes.
struct Items<'a> {
    entries: Vec<Entry<'a>>,
    encoded: Vec<u8>,
    features: x86::Features,
    parser: x86::SourceParser,
    // Parse errors anywhere take precedence over feature errors.
    unsupported: Option<Error>,
    parsed: usize,
}
impl<'a> Items<'a> {
    fn new(features: x86::Features) -> Self {
        Self {
            entries: Vec::new(),
            encoded: Vec::new(),
            features,
            parser: x86::SourceParser::default(),
            unsupported: None,
            parsed: 0,
        }
    }
    fn parse(&mut self, source: &str, first: usize) -> Result<(), Error> {
        // Move syntax state out while the callback collects directly into self.
        let mut parser = std::mem::take(&mut self.parser);
        let result = source.lines().enumerate().try_for_each(|(index, source)| {
            let line = first + index;
            parser
                .parse_with(source, |_, item| match item {
                    x86::SourceItem::Instruction(mut i) => {
                        if !self.encode(&i, line) {
                            i.operands.shrink_to_fit();
                            self.instruction(Ref::Owned(Box::new(i)), line)
                        }
                    }
                    x86::SourceItem::Label(name) => self.push(Entry::Label {
                        name: name.into_boxed_str(),
                        line,
                    }),
                    x86::SourceItem::Text(text) => {
                        self.push(if let Some((bytes, width)) = literal_data(text) {
                            Entry::LiteralData { bytes, width, line }
                        } else {
                            Entry::Text {
                                text: text.into(),
                                line,
                            }
                        });
                    }
                })
                .map_err(|e| Error::new(line, e.strip_prefix("line 1: ").unwrap_or(&e)))
        });
        self.parser = parser;
        result
    }
    fn push(&mut self, entry: Entry<'a>) {
        self.parsed += 1;
        self.entries.push(entry);
    }
    /// Encode a symbol-free instruction into the current run, if possible.
    /// Otherwise the caller retains it for per-pass layout.
    fn encode(&mut self, i: &x86::Instruction, line: usize) -> bool {
        let required = i.opcode.required_features();
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
        let end = self.encoded.len();
        match self.entries.last_mut() {
            Some(Entry::Encoded { end: run, .. }) => *run = end,
            _ => self.entries.push(Entry::Encoded { end, line }),
        }
        true
    }
    fn instruction(&mut self, i: Ref<'a, x86::Instruction>, line: usize) {
        let short = matches!(i.opcode, x86::Opcode::Jmp | x86::Opcode::Jcc(_))
            && matches!(i.operands.as_slice(), [x86::Operand::Immediate(address)]
                if address.modifier == Modifier::None && address.expression.add.is_some()
                    && address.expression.subtract.is_none());
        self.push(Entry::Instruction {
            instruction: i,
            line,
            short,
        });
    }
    fn finish(self) -> Result<Self, Error> {
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
    items.parse(source, 1)?;
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
    // Brent's cycle detector retains one directive state, not a history per
    // pass. A branch widening changes the transition function, so reset it.
    let mut checkpoint = None;
    let (mut period, mut distance) = (1usize, 0usize);
    let mut variable_work = 0usize;
    // Every pass defines the same symbols and fixups: later passes reuse the
    // previous pass's (cleared) tables, and size their sections from it.
    let mut recycled = (Symbols::default(), Vec::new());
    let mut size_hints = Vec::new();
    loop {
        let start = PROFILE.then(std::time::Instant::now);
        let mut a = Assembler::new()?;
        (a.symbols, a.fixes) = std::mem::take(&mut recycled);
        a.leb_widths = leb_widths.clone();
        a.pref_powers = pref_powers.clone();
        a.size_hints = std::mem::take(&mut size_hints);
        let mut branches = Vec::new();
        let mut run = 0;
        for (index, entry) in items.entries.iter().enumerate() {
            let fix = a.fixes.len();
            let (result, line) = match entry {
                Entry::Encoded { end, line } => {
                    let bytes = &items.encoded[run..*end];
                    run = *end;
                    (a.encoded(bytes), line)
                }
                Entry::Instruction {
                    instruction,
                    line,
                    short,
                } => {
                    if *short {
                        branches.push((index, fix));
                    }
                    (a.instruction(instruction, *line, *short), line)
                }
                Entry::Label { name, line } => (a.label(name), line),
                Entry::LiteralData { bytes, width, line } => {
                    (a.data_bytes(&bytes[..usize::from(*width)]), line)
                }
                Entry::Text { text, line } => (
                    text.lines()
                        .try_for_each(|directive| a.directive(directive, *line)),
                    line,
                ),
                Entry::Item { item, line } => (a.item(item, *line), line),
            };
            result.map_err(|e| Error::new(*line, e))?;
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
            let name = f.address.expression.add.as_deref().unwrap();
            let fits = a.local_symbol(name, 0)
                && a.try_eval(&f.address.expression, 0)
                    .is_some_and(|(section, value)| {
                        section == Some(f.section)
                            && (-128..=127).contains(&(value - i128::from(f.offset)))
                    });
            if !fits && let Entry::Instruction { short, .. } = &mut items.entries[index] {
                *short = false;
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
        for (i, (section, start, padding, preferred, end, line)) in a.prefs.iter().enumerate() {
            let (sec, value) = a.eval_symbol(end, 0).map_err(|e| Error::new(*line, e))?;
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
            let object = a.object()?;
            if let Some(start) = start {
                profile.object_ms = start.elapsed().as_secs_f64() * 1000.0;
            }
            return Ok(object);
        }
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
            if checkpoint
                .as_ref()
                .is_some_and(|(lebs, prefs)| lebs == &leb_widths && prefs == &pref_powers)
            {
                return Err(Error::new(0, "cyclic variable-length directive layout"));
            }
            if checkpoint.is_none() || distance == period {
                checkpoint = Some((leb_widths.clone(), pref_powers.clone()));
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
        Ok(a)
    }
    // Binding-sensitive expressions must reach the linker, including aliases
    // whose underlying definition is interposable.
    fn local_symbol(&self, name: &str, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        self.symbols.get(name).is_some_and(|s| {
            !s.global
                && !s.weak
                && s.alias.as_ref().is_none_or(|e| {
                    e.add
                        .as_deref()
                        .is_none_or(|n| self.local_symbol(n, depth + 1))
                        && e.subtract
                            .as_deref()
                            .is_none_or(|n| self.local_symbol(n, depth + 1))
                })
        })
    }
    fn item(&mut self, item: &Item, line: usize) -> Result<(), String> {
        match item {
            Item::Label(n) => self.label(n),
            Item::Directive(d) => self.shared_directive(d, line),
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
        self.section_full(name, flags, nobits, entry_size, group, None, None)
    }
    fn section_full(
        &mut self,
        name: &str,
        flags: Option<u64>,
        nobits: bool,
        entry_size: u64,
        group: Option<String>,
        section_type: Option<u32>,
        unique: Option<u64>,
    ) -> Result<(), String> {
        let nobits = nobits || matches!(name, ".bss" | ".tbss");
        let section_type = section_type.unwrap_or(if nobits {
            8
        } else if name == ".eh_frame" {
            0x7000_0001
        } else {
            1
        });
        if let Some(i) = self
            .sections
            .iter()
            .position(|s| s.name == name && s.group == group && s.unique == unique)
        {
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
            self.current = i;
            return Ok(());
        }
        let flags = flags.unwrap_or_else(|| sections::default_flags(name));
        sections::validate(name, flags, nobits, entry_size, group.as_deref())?;
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
    fn label(&mut self, name: &str) -> Result<(), String> {
        let value = self.offset();
        let sym = self.symbols.entry(name);
        if sym.section.is_some() || sym.alias.is_some() || sym.common.is_some() {
            return Err(format!("duplicate symbol `{name}`"));
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
            if address.modifier == Modifier::None
                && address.expression.add.is_some()
                && let Some((None, value)) = self.try_eval(&address.expression, 0)
            {
                if matches!(
                    i.opcode,
                    x86::Opcode::Call | x86::Opcode::Jmp | x86::Opcode::Jcc(_)
                ) && matches!(operand, x86::Operand::Immediate(_))
                {
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
            self.fixes.push(Fix {
                section: self.current,
                offset: at + f.offset as u64,
                width: f.width,
                address: f.address,
                kind: f.kind,
                line,
            })
        }
        self.bytes(&e.bytes)
    }
    fn capture_dot(&mut self, mut expression: Expression) -> Result<Expression, String> {
        if expression.add.as_deref() == Some(".") || expression.subtract.as_deref() == Some(".") {
            let name = format!(".Lkeel.dot.{}", self.symbols.len());
            self.label(&name)?;
            if expression.add.as_deref() == Some(".") {
                expression.add = Some(name.clone())
            }
            if expression.subtract.as_deref() == Some(".") {
                expression.subtract = Some(name)
            }
        }
        Ok(expression)
    }
    fn integer(&mut self, width: u8, mut address: Address, line: usize) -> Result<(), String> {
        self.data_location(u64::from(width));
        address.expression = self.capture_dot(address.expression)?;
        if address.expression.add.is_none() && address.expression.subtract.is_none() {
            return self.bytes(&address.expression.constant.to_le_bytes()[..width as usize]);
        }
        if self.sections[self.current].nobits {
            return Err("symbolic initializer in NOBITS section".into());
        }
        let kind = match (address.modifier, width) {
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
            address,
            kind,
            line,
        });
        self.zero(u64::from(width), 0)
    }
    fn shared_directive(&mut self, d: &Directive, line: usize) -> Result<(), String> {
        match d {
            Directive::X86Integer { width, address } if matches!(width, 1 | 2 | 4 | 8) => {
                self.integer(*width, address.clone(), line)
            }
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
                self.symbols.entry(name).kind = if *function {
                    SymbolKind::Text
                } else {
                    SymbolKind::Data
                };
                Ok(())
            }
            Directive::Size { name, expression } => {
                let e = self.capture_dot(expression.clone())?;
                self.symbols.entry(name).size = Some(Box::new(e));
                Ok(())
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
                self.section(if *thread_local { ".tbss" } else { ".bss" }, None, true)?;
                self.align(u32::from(*alignment), None)?;
                self.label(symbol)?;
                self.symbols.get_mut(symbol).unwrap().size =
                    Some(Box::new(Expression::constant(i128::from(*size))));
                self.zero(*size, 0)?;
                self.current = old;
                Ok(())
            }
            Directive::Global(s) => self.global(s),
            Directive::Bytes(b) => self.data_bytes(b),
            Directive::CString(b) => {
                self.data_bytes(b)?;
                self.data_bytes(&[0])
            }
            Directive::Integer {
                width,
                expression,
                got: false,
            } if matches!(width, 1 | 2 | 4 | 8) => self.integer(
                *width,
                Address {
                    expression: expression.clone(),
                    modifier: Modifier::None,
                },
                line,
            ),
            Directive::Space(n) => self.data_zero(*n, 0),
            Directive::Align(n) => self.align(u32::from(*n), None),
            Directive::Set { name, expression } => {
                let e = self.capture_dot(expression.clone())?;
                self.set(name, e)
            }
            Directive::Leb { signed, expression } => {
                self.leb_expression(expression.clone(), *signed, line)
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
    fn global(&mut self, name: &str) -> Result<(), String> {
        let s = self.symbols.entry(name);
        if s.weak {
            return Err("weak symbol changed binding to global".into());
        }
        s.local = false;
        s.global = true;
        Ok(())
    }
    fn set(&mut self, name: &str, e: Expression) -> Result<(), String> {
        let sym = self.symbols.entry(name);
        if sym.section.is_some() || sym.alias.is_some() {
            return Err(format!("duplicate symbol `{name}`"));
        }
        sym.alias = Some(Box::new(e));
        Ok(())
    }
    fn leb(&mut self, n: i128, signed: bool) -> Result<(), String> {
        let mut b = Vec::new();
        metadata::leb(&mut b, n, signed)?;
        self.bytes(&b)
    }
    fn leb_expression(&mut self, e: Expression, signed: bool, line: usize) -> Result<(), String> {
        self.data_location(1);
        if e.add.is_none() && e.subtract.is_none() {
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
            let expression = self.capture_dot(x86::address(assignment[1])?.expression)?;
            return self.set(&name, expression);
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
            ".section" => {
                let section = sections::parse(&parts)?;
                self.section_full(
                    &section.name,
                    section.flags,
                    section.nobits,
                    section.entry_size,
                    section.group,
                    section.section_type,
                    section.unique,
                )
            }
            ".globl" | ".global" | ".local" | ".weak" | ".hidden" => {
                for name in parts {
                    let name = x86::symbol(name)?;
                    if matches!(op, ".globl" | ".global") {
                        self.global(&name)?;
                        continue;
                    }
                    let s = self.symbols.entry(&name);
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
            ".type" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let kind = match parts[1] {
                    "@function" | "%function" => SymbolKind::Text,
                    "@object" | "%object" => SymbolKind::Data,
                    "@tls_object" => SymbolKind::Tls,
                    _ => return Err("unsupported ELF symbol type".into()),
                };
                self.symbols.entry(&name).kind = kind;
                Ok(())
            }
            ".size" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let expr = self.capture_dot(x86::address(parts[1])?.expression)?;
                self.symbols.entry(&name).size = Some(Box::new(expr));
                Ok(())
            }
            ".set" | ".equ" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let expr = self.capture_dot(x86::address(parts[1])?.expression)?;
                self.set(&name, expr)
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
                let sym = self.symbols.entry(&name);
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
                    self.section(".bss", None, true)?;
                    self.align(align.trailing_zeros(), None)?;
                    self.symbols.get_mut(&name).unwrap().common = None;
                    self.label(&name)?;
                    self.symbols.get_mut(&name).unwrap().size =
                        Some(Box::new(Expression::constant(size.into())));
                    self.zero(size, 0)?;
                    self.current = old
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
            ".byte" | ".short" | ".word" | ".long" | ".int" | ".quad" => {
                let width = match op {
                    ".byte" => 1,
                    ".short" | ".word" => 2,
                    ".long" | ".int" => 4,
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
                    self.integer(width, address, line)?
                }
                Ok(())
            }
            ".zero" | ".space" | ".skip" => {
                if parts.len() > 2 {
                    return Err("too many space operands".into());
                }
                let n = num(parts[0])?;
                let fill = parts.get(1).map(|s| num(s)).transpose()?.unwrap_or(0);
                self.data_zero(n, u8::try_from(fill).map_err(|_| "fill exceeds a byte")?)
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
                    self.leb_expression(address.expression, op == ".sleb128", line)?
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
                self.section_metadata(".comment", Some(0x30), false, 1, None)?;
                if self.offset() == 0 {
                    self.bytes(&[0])?;
                }
                self.bytes(&value)?;
                self.bytes(&[0])?;
                self.current = old;
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
                let end = x86::symbol(parts[1])?;
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
    fn try_eval_symbol(&self, name: &str, depth: usize) -> Option<(Option<usize>, i128)> {
        if depth > 64 {
            return None;
        }
        let s = self.symbols.get(name)?;
        if let Some(e) = &s.alias {
            return self.try_eval(e, depth + 1);
        }
        Some((Some(s.section?), s.value.into()))
    }
    fn try_eval(&self, e: &Expression, depth: usize) -> Option<(Option<usize>, i128)> {
        let (mut section, mut value) = (None, e.constant);
        if let Some(n) = &e.add {
            let (s, v) = self.try_eval_symbol(n, depth)?;
            section = s;
            value += v
        }
        if let Some(n) = &e.subtract {
            let (s, v) = self.try_eval_symbol(n, depth)?;
            if s != section {
                return None;
            }
            section = None;
            value -= v
        }
        Some((section, value))
    }
    fn eval_symbol(&self, name: &str, depth: usize) -> Result<(Option<usize>, i128), String> {
        if depth > 64 {
            return Err("cyclic or excessively nested symbol alias".into());
        }
        let s = self
            .symbols
            .get(name)
            .ok_or_else(|| format!("undefined symbol `{name}` in expression"))?;
        if let Some(e) = &s.alias {
            return self.eval(e, depth + 1);
        }
        if let Some(section) = s.section {
            Ok((Some(section), s.value.into()))
        } else {
            Err(format!("undefined symbol `{name}` in expression"))
        }
    }
    fn eval(&self, e: &Expression, depth: usize) -> Result<(Option<usize>, i128), String> {
        let (mut section, mut value) = (None, e.constant);
        if let Some(n) = &e.add {
            let (s, v) = self.eval_symbol(n, depth)?;
            section = s;
            value += v
        }
        if let Some(n) = &e.subtract {
            let (s, v) = self.eval_symbol(n, depth)?;
            if s != section {
                return Err("symbol difference requires the same section".into());
            }
            section = None;
            value -= v
        }
        Ok((section, value))
    }
    fn symbol_attributes(&self, name: &str, depth: usize) -> (SymbolKind, Option<&Expression>) {
        let Some(s) = self.symbols.get(name) else {
            return (SymbolKind::Unknown, None);
        };
        let inherited = if depth < 64 {
            s.alias
                .as_deref()
                .filter(|e| e.subtract.is_none())
                .and_then(|e| e.add.as_deref())
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
        for f in &self.fixes {
            if let Some(n) = &f.address.expression.add {
                let section = self.sections.iter().position(|s| s.name == *n);
                let sym = self.symbols.entry(n);
                if sym.section.is_none()
                    && sym.alias.is_none()
                    && sym.common.is_none()
                    && let Some(i) = section
                {
                    sym.section = Some(i);
                    sym.kind = SymbolKind::Section;
                }
                if matches!(
                    f.address.modifier,
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
                .map(|f| &f.address.expression)
                .chain(
                    self.symbols
                        .values()
                        .flat_map(|s| s.alias.iter().chain(s.size.iter()))
                        .map(|e| &**e),
                )
                .flat_map(|e| e.add.iter().chain(e.subtract.iter()))
                .map(String::as_str)
                .collect()
        };
        let synthetic_signatures: BTreeSet<_> = groups
            .keys()
            .filter(|name| {
                !ordinary_references.contains(name.as_str())
                    && self.symbols.get(*name).is_none_or(|s| {
                        s.section.is_none() && s.alias.is_none() && s.common.is_none()
                    })
            })
            .cloned()
            .collect();
        for signature in groups.keys() {
            self.symbols.entry(signature);
        }
        let mut object = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
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
                    || f.kind == 10
                        && f.width == 4
                        && f.address
                            .expression
                            .subtract
                            .as_deref()
                            .is_some_and(|name| {
                                self.try_eval_symbol(name, 0)
                                    .is_some_and(|(section, _)| section == Some(f.section))
                            });
                let merge_target = f.address.expression.add.as_deref().is_some_and(|name| {
                    self.try_eval_symbol(name, 0).is_some_and(|(section, _)| {
                        section.is_some_and(|i| self.sections[i].flags & 0x12 == 0x12)
                    })
                });
                merge_target
                    || f.address.modifier != Modifier::None
                    || pc_relative
                        && f.address.expression.add.as_deref().is_some_and(|name| {
                            self.try_eval_symbol(name, 0)
                                .is_some_and(|(section, _)| section.is_none())
                        })
            })
            .filter_map(|f| f.address.expression.add.as_deref())
            .collect();
        let mut retained = identity.clone();
        if let Some(names) = &self.addrsig {
            retained.extend(names.iter().map(String::as_str));
        }
        let mut symbols = FxHashMap::default();
        symbols.reserve(self.symbols.len());
        // Lookup order is irrelevant during layout; ELF symbol order is stable.
        // Temporary local labels without an ELF symbol table entry.
        let discard = |name: &str, s: &Sym, section: Option<usize>, tls: bool| {
            name.starts_with(".L")
                && !s.global
                && !s.weak
                && !s.hidden
                && s.common.is_none()
                && !tls
                && !retained.contains(name)
                && !groups.contains_key(name)
                && (section.is_some() || s.alias.is_some())
                && self.local_symbol(name, 0)
        };
        // Most symbols are plain local labels that cannot fail evaluation
        // below and are discarded there; only the rest need ordering.
        let mut entries: Vec<_> = self
            .symbols
            .iter()
            .filter(|(name, s)| {
                let tls = s.kind == SymbolKind::Tls
                    || s.section
                        .is_some_and(|i| self.sections[i].flags & 0x400 != 0);
                s.kind == SymbolKind::Section
                    || s.alias.is_some()
                    || s.size.is_some()
                    || !discard(name, s, s.section, tls)
            })
            .collect();
        entries.sort_unstable_by_key(|(name, _)| *name);
        for (name, s) in entries {
            if s.kind == SymbolKind::Section {
                symbols.insert(name, object.section_symbol(sections[s.section.unwrap()]));
                continue;
            }
            let (section, value) = if s.alias.is_some() {
                let (sec, value) = self.eval_symbol(name, 0).map_err(err)?;
                (
                    sec,
                    u64::try_from(value).map_err(|_| err("alias value out of range".into()))?,
                )
            } else {
                (s.section, s.value)
            };
            let (kind, size_expression) = self.symbol_attributes(name, 0);
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
            if discard(name, s, section, tls) {
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
            let id = if let Some((size, align)) = s.common {
                // ELF common symbols store alignment in st_value. The writer's
                // add_common_symbol API only applies `align` for Mach-O BSS.
                let mut symbol = symbol;
                symbol.value = align;
                object.add_common_symbol(symbol, size, align)
            } else {
                object.add_symbol(symbol)
            };
            symbols.insert(name, id);
        }
        for (signature, members) in &groups {
            object.add_comdat(Comdat {
                kind: ComdatKind::Any,
                symbol: symbols[signature.as_str()],
                sections: members.iter().map(|&i| sections[i]).collect(),
            });
        }
        for f in &self.fixes {
            let expr = &f.address.expression;
            let mut kind = f.kind;
            let mut addend = expr.constant;
            let name = expr
                .add
                .as_deref()
                .ok_or_else(|| Error::new(f.line, "relocation requires a positive symbol"))?;
            let defined = self.try_eval_symbol(name, 0);
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
                && self.local_symbol(&name, 0)
            {
                defined.map(|(_, n)| n + addend - i128::from(f.offset))
            } else {
                None
            };
            if let Some(value) = absolute {
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
            if let Some(sub) = &expr.subtract {
                let (sec, value) = self
                    .eval_symbol(sub, 0)
                    .map_err(|e| Error::new(f.line, e))?;
                if sec != Some(f.section) || f.width != 4 || kind != 10 {
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
                && (debug_reference || !identity.contains(name))
                && f.address.modifier == Modifier::None
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
                    symbols[name]
                }
            } else {
                symbols[name]
            };
            object
                .add_relocation(
                    sections[f.section],
                    Relocation {
                        offset: f.offset,
                        symbol,
                        addend: i64::try_from(addend)
                            .map_err(|_| Error::new(f.line, "relocation addend exceeds i64"))?,
                        flags: RelocationFlags::Elf { r_type: kind },
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
