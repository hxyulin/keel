//! Adapter for source and structured inputs sharing layout/fixup/object logic.
use super::{Error, scanning};
use bumpalo::Bump;
use keel_asm_ir::{Instruction, Item, Program};

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
    /// An instruction encoded when it entered a [`Compact`] program.
    Word(u32),
    Directive(&'a keel_asm_ir::Directive),
    Symbolic(&'a keel_asm_ir::SymbolicInstruction),
    Branch(bool, &'a str),
}
impl<'a> Input<'a> {
    pub(super) fn contains(self, needle: &str) -> bool {
        match self {
            Self::Text(s) => s.contains(needle),
            Self::Program(Program { items }) | Self::Compact(Compact { items, .. }) => {
                // One searcher for the many short names, not one per name.
                let finder = memchr::memmem::Finder::new(needle);
                let found = |s: &str| finder.find(s.as_bytes()).is_some();
                items.iter().any(|item| match item {
                    Item::Text(s) | Item::Label(s) | Item::Branch { target: s, .. } => found(s),
                    Item::Instruction(_) | Item::X86Instruction(_) => false,
                    Item::Directive(d) => d.contains(needle),
                    Item::Symbolic(s) => found(s.target()),
                })
            }
        }
    }
    /// The labels and branches a structured input declares, so a pass can
    /// reserve their tables at once rather than leave outgrown copies in its
    /// arena. Text input reserves nothing.
    pub(super) fn reservations(self) -> (usize, usize) {
        use keel_asm_ir::SymbolicInstruction as S;
        match self {
            Self::Text(_) => (0, 0),
            Self::Program(Program { items }) | Self::Compact(Compact { items, .. }) => items
                .iter()
                .fold((0, 0), |(labels, branches), item| match item {
                    Item::Label(_) | Item::Directive(keel_asm_ir::Directive::ZeroFill { .. }) => {
                        (labels + 1, branches)
                    }
                    Item::Branch { .. }
                    | Item::Symbolic(S::Conditional { .. } | S::Compare { .. }) => {
                        (labels, branches + 1)
                    }
                    _ => (labels, branches),
                }),
        }
    }
    pub(super) fn lines(self, arena: &'a Bump) -> Lines<'a> {
        match self {
            Self::Text(s) => Lines {
                current: Some((0, scanning::Lines::new(s, arena))),
                items: [].iter(),
                gaps: [].iter(),
                words: [].iter(),
                pending: 0,
                arena,
                next_line: 1,
            },
            Self::Program(p) => Lines {
                current: None,
                items: p.items.iter(),
                gaps: [].iter(),
                words: [].iter(),
                pending: 0,
                arena,
                next_line: 1,
            },
            Self::Compact(c) => {
                let mut gaps = c.gaps.iter();
                Lines {
                    current: None,
                    items: c.items.iter(),
                    pending: gaps.next().copied().unwrap_or(0),
                    gaps,
                    words: c.words.iter(),
                    arena,
                    next_line: 1,
                }
            }
        }
    }
}

/// A structured program whose plain instructions were encoded as they arrived,
/// retaining four bytes instead of a whole item for each. Every other item is
/// kept unchanged, as is every statement position used by diagnostics.
pub(crate) struct Compact {
    items: Vec<Item>,
    /// `gaps[i]` words precede `items[i]`; the last entry counts trailing words.
    gaps: Vec<u32>,
    words: Vec<u32>,
    features: crate::TargetFeatures,
}
impl Compact {
    pub(crate) fn new(features: crate::TargetFeatures) -> Self {
        Self {
            items: Vec::new(),
            gaps: vec![0],
            words: Vec::new(),
            features,
        }
    }
    pub(crate) fn features(&self) -> crate::TargetFeatures {
        self.features
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
            self.items.push(item);
            self.gaps.push(0);
        }
    }
}

pub(super) struct Lines<'a> {
    current: Option<(usize, scanning::Lines<'a>)>,
    items: std::slice::Iter<'a, Item>,
    /// Compact input: the word count following each remaining item.
    gaps: std::slice::Iter<'a, u32>,
    words: std::slice::Iter<'a, u32>,
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
                self.pending -= 1;
                self.next_line += 1;
                return Some(Ok((line, Entry::Word(*self.words.next()?))));
            }
            let item = self.items.next()?;
            self.pending = self.gaps.next().copied().unwrap_or(0);
            self.next_line += 1;
            match item {
                Item::X86Instruction(_) => {
                    return Some(Err(Error::new(line, "x86 instruction in AArch64 program")));
                }
                Item::Directive(d) => return Some(Ok((line, Entry::Directive(d)))),
                Item::Symbolic(s) => return Some(Ok((line, Entry::Symbolic(s)))),
                Item::Instruction(i) => return Some(Ok((line, Entry::Instruction(i)))),
                Item::Branch { link, target } => {
                    return Some(Ok((line, Entry::Branch(*link, target))));
                }
                Item::Label(name) => return Some(Ok((line, Entry::Label(name)))),
                Item::Text(text) => {
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
    let (items, gaps, mut words): (&[Item], &[u32], &[u32]) = match input {
        Input::Text(_) => unreachable!("only structured programs are materialized"),
        Input::Program(p) => (&p.items, &[], &[]),
        Input::Compact(c) => (&c.items, &c.gaps, &c.words),
    };
    let mut text = String::new();
    let mut gaps = gaps.iter();
    for index in 0..=items.len() {
        // Encoded words keep the spelling of an instruction item below.
        let (before, rest) = words.split_at(gaps.next().map_or(0, |&n| n as usize));
        for word in before {
            writeln!(text, ".long {word}").unwrap();
        }
        words = rest;
        let Some(item) = items.get(index) else {
            break;
        };
        match item {
            Item::X86Instruction(_) => {
                return Err(Error::new(0, "x86 instruction in AArch64 program"));
            }
            Item::Text(s) => {
                text.push_str(s);
                if !s.ends_with('\n') {
                    text.push('\n');
                }
            }
            Item::Label(s) => {
                super::symbol(s).map_err(|e| Error::new(0, e))?;
                writeln!(text, "{s}:").unwrap();
            }
            Item::Directive(d) => {
                d.validate().map_err(|e| Error::new(0, e))?;
                writeln!(text, "{d}").unwrap();
            }
            Item::Symbolic(s) => {
                s.validate().map_err(|e| Error::new(0, e))?;
                super::symbol(s.target()).map_err(|e| Error::new(0, e))?;
                crate::encode_instruction(&s.zero_instruction(), features)
                    .map_err(|e| Error::new(0, e))?;
                writeln!(text, "{s}").unwrap();
            }
            Item::Instruction(i) => {
                let word = crate::encode_instruction(i, features).map_err(|e| Error::new(0, e))?;
                writeln!(text, ".long {word}").unwrap();
            }
            Item::Branch { link, target } => {
                super::symbol(target).map_err(|e| Error::new(0, e))?;
                writeln!(text, "{} {target}", if *link { "bl" } else { "b" }).unwrap();
            }
        }
    }
    Ok(text)
}
