//! Adapter for source and structured inputs sharing layout/fixup/object logic.
use super::{Error, scanning};
use bumpalo::Bump;
use keel_asm_ir::{Directive, Instruction, Item, Program, SymbolicInstruction};

#[derive(Clone, Copy)]
pub(super) enum Input<'a> {
    Text(&'a str),
    Program(&'a Program),
    Compact(&'a Compact),
}
pub(super) enum Entry<'a> {
    Text(&'a str),
    Label(&'a str),
    Instruction(&'a Instruction),
    /// Consecutive instructions encoded when they entered a [`Compact`]
    /// program, one statement each.
    Words(&'a [u32]),
    Directive(&'a Directive),
    Symbolic(&'a SymbolicInstruction),
    Branch(bool, &'a str),
}
/// A structured item, borrowed from a [`Program`] or a [`Compact`] program.
#[derive(Clone, Copy)]
enum ItemRef<'a> {
    Label(&'a str),
    Branch(bool, &'a str),
    Symbolic(&'a SymbolicInstruction),
    Directive(&'a Directive),
    Instruction(&'a Instruction),
    Text(&'a str),
    X86Instruction,
}
impl<'a> ItemRef<'a> {
    fn of(item: &'a Item) -> Self {
        match item {
            Item::Label(s) => Self::Label(s),
            Item::Branch { link, target } => Self::Branch(*link, target),
            Item::Symbolic(s) => Self::Symbolic(s),
            Item::Directive(d) => Self::Directive(d),
            Item::Instruction(i) => Self::Instruction(i),
            Item::Text(s) => Self::Text(s),
            Item::X86Instruction(_) => Self::X86Instruction,
        }
    }
}
/// A structured input's items in order, with any encoded words before each.
#[derive(Clone)]
enum Items<'a> {
    Program(std::slice::Iter<'a, Item>),
    Compact(&'a Compact, std::slice::Iter<'a, Stored>),
}
impl<'a> Iterator for Items<'a> {
    type Item = ItemRef<'a>;
    #[inline]
    fn next(&mut self) -> Option<ItemRef<'a>> {
        match self {
            Self::Program(items) => items.next().map(ItemRef::of),
            Self::Compact(c, items) => items.next().map(|s| c.item(s)),
        }
    }
}
impl<'a> Input<'a> {
    fn items(self) -> Items<'a> {
        match self {
            Self::Text(_) => Items::Program([].iter()),
            Self::Program(p) => Items::Program(p.items.iter()),
            Self::Compact(c) => Items::Compact(c, c.items.iter()),
        }
    }
    pub(super) fn contains(self, needle: &str) -> bool {
        match self {
            Self::Text(s) => s.contains(needle),
            Self::Compact(c) if !needle.contains(SEPARATOR) => c.contains(needle),
            Self::Program(_) | Self::Compact(_) => {
                // One searcher for the many short names, not one per name.
                let finder = memchr::memmem::Finder::new(needle);
                let found = |s: &str| finder.find(s.as_bytes()).is_some();
                self.items().any(|item| match item {
                    ItemRef::Text(s) | ItemRef::Label(s) | ItemRef::Branch(_, s) => found(s),
                    ItemRef::Instruction(_) | ItemRef::X86Instruction => false,
                    ItemRef::Directive(d) => d.contains(needle),
                    ItemRef::Symbolic(s) => found(s.target()),
                })
            }
        }
    }
    /// The labels, branches and CFI procedures a structured input declares,
    /// so a pass can reserve their tables at once rather than regrow them or
    /// leave outgrown copies in its arena. Text input reserves nothing.
    pub(super) fn reservations(self) -> Reservations {
        if let Self::Compact(c) = self {
            return c.reservations;
        }
        let mut r = Reservations::default();
        self.items().for_each(|item| r.count(item));
        r
    }
    pub(super) fn lines(self, arena: &'a Bump) -> Lines<'a> {
        match self {
            Self::Text(s) => Lines {
                current: Some((0, scanning::Lines::new(s, arena))),
                items: self.items(),
                gaps: [].iter(),
                words: &[],
                pending: 0,
                arena,
                next_line: 1,
            },
            Self::Program(_) => Lines {
                current: None,
                items: self.items(),
                gaps: [].iter(),
                words: &[],
                pending: 0,
                arena,
                next_line: 1,
            },
            Self::Compact(c) => {
                let mut gaps = c.gaps.iter();
                Lines {
                    current: None,
                    items: self.items(),
                    pending: gaps.next().copied().unwrap_or(0),
                    gaps,
                    words: &c.words,
                    arena,
                    next_line: 1,
                }
            }
        }
    }
}

/// A structured program whose plain instructions were encoded as they arrived,
/// retaining four bytes instead of a whole item for each. Every other item is
/// kept, as is every statement position used by diagnostics: labels and
/// branch targets as spans of one name buffer, other items in typed tables.
pub(crate) struct Compact {
    items: Vec<Stored>,
    /// `gaps[i]` words precede `items[i]`; the last entry counts trailing words.
    gaps: Vec<u32>,
    words: Vec<u32>,
    /// Label and branch names, each followed by [`SEPARATOR`] so that a
    /// search of the whole buffer finds only matches within one name.
    names: String,
    symbolic: Vec<SymbolicInstruction>,
    directives: Vec<Directive>,
    other: Vec<Item>,
    /// [`Input::reservations`], counted as items arrive.
    reservations: Reservations,
    features: crate::TargetFeatures,
}
const SEPARATOR: char = '\0';
#[derive(Clone, Copy, Default)]
pub(super) struct Reservations {
    pub labels: usize,
    pub branches: usize,
    /// `.cfi_startproc` procedures: each gains at least one synthesized
    /// unwind label.
    pub frames: usize,
}
impl Reservations {
    fn count(&mut self, item: ItemRef<'_>) {
        use keel_asm_ir::{Cfi, SymbolicInstruction as S};
        match item {
            ItemRef::Label(_) | ItemRef::Directive(Directive::ZeroFill { .. }) => self.labels += 1,
            ItemRef::Branch(..) | ItemRef::Symbolic(S::Conditional { .. } | S::Compare { .. }) => {
                self.branches += 1
            }
            ItemRef::Directive(Directive::Cfi(Cfi::Start)) => self.frames += 1,
            _ => {}
        }
    }
}
/// A [`Compact`] item in twelve bytes.
enum Stored {
    /// `names[start..start + len]`.
    Label { start: u32, len: u32 },
    Branch { link: bool, start: u32, len: u32 },
    Symbolic(u32),
    Directive(u32),
    Other(u32),
}
const _: () = assert!(std::mem::size_of::<Stored>() == 12);
fn index(len: usize) -> u32 {
    u32::try_from(len).expect("structured program exceeds 4 GiB")
}
impl Compact {
    pub(crate) fn new(features: crate::TargetFeatures) -> Self {
        Self {
            items: Vec::new(),
            gaps: vec![0],
            words: Vec::new(),
            names: String::new(),
            symbolic: Vec::new(),
            directives: Vec::new(),
            other: Vec::new(),
            reservations: Reservations::default(),
            features,
        }
    }
    pub(crate) fn features(&self) -> crate::TargetFeatures {
        self.features
    }
    fn name(&mut self, name: &str) -> (u32, u32) {
        let start = index(self.names.len());
        self.names.push_str(name);
        self.names.push(SEPARATOR);
        (start, index(name.len()))
    }
    /// Whether any item mentions `needle`, which must not contain [`SEPARATOR`].
    fn contains(&self, needle: &str) -> bool {
        let finder = memchr::memmem::Finder::new(needle);
        let found = |s: &str| finder.find(s.as_bytes()).is_some();
        found(&self.names)
            || self.symbolic.iter().any(|s| found(s.target()))
            || self.directives.iter().any(|d| d.contains(needle))
            || self.other.iter().any(|item| matches!(item, Item::Text(s) if found(s)))
    }
    #[inline]
    fn item(&self, stored: &Stored) -> ItemRef<'_> {
        let name = |start: u32, len: u32| &self.names[start as usize..(start + len) as usize];
        match *stored {
            Stored::Label { start, len } => ItemRef::Label(name(start, len)),
            Stored::Branch { link, start, len } => ItemRef::Branch(link, name(start, len)),
            Stored::Symbolic(i) => ItemRef::Symbolic(&self.symbolic[i as usize]),
            Stored::Directive(i) => ItemRef::Directive(&self.directives[i as usize]),
            Stored::Other(i) => ItemRef::of(&self.other[i as usize]),
        }
    }
    /// Encoding failures keep their item, to be diagnosed at its position.
    pub(crate) fn push(&mut self, program: Program) {
        for item in program.items {
            if let Item::Instruction(i) = &item
                && let Ok(word) = crate::encode_instruction(i, self.features)
            {
                self.words.push(word);
                *self.gaps.last_mut().unwrap() += 1;
                continue;
            }
            self.reservations.count(ItemRef::of(&item));
            let stored = match item {
                Item::Label(name) => {
                    let (start, len) = self.name(&name);
                    Stored::Label { start, len }
                }
                Item::Branch { link, target } => {
                    let (start, len) = self.name(&target);
                    Stored::Branch { link, start, len }
                }
                Item::Symbolic(s) => {
                    self.symbolic.push(s);
                    Stored::Symbolic(index(self.symbolic.len() - 1))
                }
                Item::Directive(d) => {
                    self.directives.push(d);
                    Stored::Directive(index(self.directives.len() - 1))
                }
                item => {
                    self.other.push(item);
                    Stored::Other(index(self.other.len() - 1))
                }
            };
            self.items.push(stored);
            self.gaps.push(0);
        }
    }
}

pub(super) struct Lines<'a> {
    current: Option<(usize, scanning::Lines<'a>)>,
    items: Items<'a>,
    /// Compact input: the word count following each remaining item.
    gaps: std::slice::Iter<'a, u32>,
    words: &'a [u32],
    /// Words to yield before the next item.
    pending: u32,
    arena: &'a Bump,
    next_line: usize,
}
impl<'a> Iterator for Lines<'a> {
    type Item = Result<(usize, Entry<'a>), Error>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((base, lines)) = &mut self.current {
                if let Some(result) = lines.next() {
                    return Some(
                        result
                            .map(|(line, text)| (line + *base, Entry::Text(text)))
                            .map_err(|mut e| {
                                e.line += *base;
                                e
                            }),
                    );
                }
                self.current = None;
            }
            let line = self.next_line;
            if self.pending > 0 {
                let (words, rest) = self.words.split_at_checked(self.pending as usize)?;
                self.words = rest;
                self.next_line += self.pending as usize;
                self.pending = 0;
                return Some(Ok((line, Entry::Words(words))));
            }
            let item = self.items.next()?;
            self.pending = self.gaps.next().copied().unwrap_or(0);
            self.next_line += 1;
            match item {
                ItemRef::Directive(d) => return Some(Ok((line, Entry::Directive(d)))),
                ItemRef::Symbolic(s) => return Some(Ok((line, Entry::Symbolic(s)))),
                ItemRef::Branch(link, target) => {
                    return Some(Ok((line, Entry::Branch(link, target))));
                }
                ItemRef::Label(name) => return Some(Ok((line, Entry::Label(name)))),
                ItemRef::X86Instruction => {
                    return Some(Err(Error::new(line, "x86 instruction in AArch64 program")));
                }
                ItemRef::Instruction(i) => return Some(Ok((line, Entry::Instruction(i)))),
                ItemRef::Text(text) => {
                    let bytes = text.as_bytes();
                    let endings = memchr::memchr2_iter(b'\r', b'\n', bytes)
                        .filter(|&i| bytes[i] == b'\r' || i == 0 || bytes[i - 1] != b'\r')
                        .count();
                    let unterminated = usize::from(
                        !bytes.is_empty() && !matches!(bytes.last(), Some(b'\r' | b'\n')),
                    );
                    self.next_line = line + endings + unterminated;
                    self.current = Some((line - 1, scanning::Lines::new(text, self.arena)));
                }
            }
        }
    }
}

pub(super) fn materialize(
    input: Input<'_>,
    features: crate::TargetFeatures,
) -> Result<String, Error> {
    use std::fmt::Write;
    let (gaps, mut words): (&[u32], &[u32]) = match input {
        Input::Text(_) => unreachable!("only structured programs are materialized"),
        Input::Program(_) => (&[], &[]),
        Input::Compact(c) => (&c.gaps, &c.words),
    };
    let mut text = String::new();
    let mut gaps = gaps.iter();
    let mut items = input.items();
    loop {
        // Encoded words keep the spelling of an instruction item below.
        let (before, rest) = words.split_at(gaps.next().map_or(0, |&n| n as usize));
        for word in before {
            writeln!(text, ".long {word}").unwrap();
        }
        words = rest;
        let Some(item) = items.next() else {
            break;
        };
        match item {
            ItemRef::X86Instruction => {
                return Err(Error::new(0, "x86 instruction in AArch64 program"));
            }
            ItemRef::Text(s) => {
                text.push_str(s);
                if !s.ends_with('\n') {
                    text.push('\n');
                }
            }
            ItemRef::Label(s) => {
                super::symbol(s).map_err(|e| Error::new(0, e))?;
                writeln!(text, "{s}:").unwrap();
            }
            ItemRef::Directive(d) => {
                d.validate().map_err(|e| Error::new(0, e))?;
                writeln!(text, "{d}").unwrap();
            }
            ItemRef::Symbolic(s) => {
                s.validate().map_err(|e| Error::new(0, e))?;
                super::symbol(s.target()).map_err(|e| Error::new(0, e))?;
                crate::encode_instruction(&s.zero_instruction(), features)
                    .map_err(|e| Error::new(0, e))?;
                writeln!(text, "{s}").unwrap();
            }
            ItemRef::Instruction(i) => {
                let word = crate::encode_instruction(i, features).map_err(|e| Error::new(0, e))?;
                writeln!(text, ".long {word}").unwrap();
            }
            ItemRef::Branch(link, target) => {
                super::symbol(target).map_err(|e| Error::new(0, e))?;
                writeln!(text, "{} {target}", if link { "bl" } else { "b" }).unwrap();
            }
        }
    }
    Ok(text)
}
