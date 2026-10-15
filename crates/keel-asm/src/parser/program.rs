//! Adapter for source and structured inputs sharing layout/fixup/object logic.
use super::{Error, scanning};
use bumpalo::Bump;
use keel_asm_ir::{Instruction, Item, Program};

#[derive(Clone, Copy)]
pub(super) enum Input<'a> {
    Text(&'a str),
    Program(&'a Program),
}
pub(super) enum Entry<'a> {
    Text(&'a str),
    Label(&'a str),
    Instruction(&'a Instruction),
    Branch(bool, &'a str),
}
impl<'a> Input<'a> {
    pub(super) fn contains(self, needle: &str) -> bool {
        match self {
            Self::Text(s) => s.contains(needle),
            Self::Program(p) => p.items.iter().any(|item| match item {
                Item::Text(s) | Item::Label(s) | Item::Branch { target: s, .. } => {
                    s.contains(needle)
                }
                Item::Instruction(_) => false,
            }),
        }
    }
    pub(super) fn lines(self, arena: &'a Bump) -> Lines<'a> {
        match self {
            Self::Text(s) => Lines {
                current: Some((0, scanning::Lines::new(s, arena))),
                items: [].iter(),
                arena,
                next_line: 1,
            },
            Self::Program(p) => Lines {
                current: None,
                items: p.items.iter(),
                arena,
                next_line: 1,
            },
        }
    }
}
pub(super) struct Lines<'a> {
    current: Option<(usize, scanning::Lines<'a>)>,
    items: std::slice::Iter<'a, Item>,
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
            let item = self.items.next()?;
            let line = self.next_line;
            self.next_line += 1;
            match item {
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
    program: &Program,
    features: crate::TargetFeatures,
) -> Result<String, Error> {
    use std::fmt::Write;
    let mut text = String::new();
    for item in &program.items {
        match item {
            Item::Text(s) => {
                text.push_str(s);
                if !s.ends_with('\n') {
                    text.push('\n');
                }
            }
            Item::Label(s) => {
                writeln!(text, "{s}:").unwrap();
            }
            Item::Instruction(i) => {
                let word = crate::encode_instruction(i, features).map_err(|e| Error::new(0, e))?;
                writeln!(text, ".long {word}").unwrap();
            }
            Item::Branch { link, target } => {
                writeln!(text, "{} {target}", if *link { "bl" } else { "b" }).unwrap();
            }
        }
    }
    Ok(text)
}
