//! A64 instructions sharing the ELF metadata, layout, DWARF and object writer.
use super::*;
use keel_asm_ir::{PageKind, SymbolicInstruction as S};

pub(super) fn relocation(kind: u32) -> Result<u32, String> {
    match kind {
        1 => Ok(257),
        10 => Ok(258),
        12 => Ok(259),
        2 => Ok(261),
        257..=573 => Ok(kind),
        _ => Err(format!("unsupported AArch64 ELF relocation {kind}")),
    }
}
pub(super) fn resolve(kind: u32, delta: i128, bytes: &mut [u8]) -> Result<(), String> {
    let (bits, scale, shift) = match kind {
        273 | 280 => (19, 2, 5),
        274 => (21, 0, 0),
        279 => (14, 2, 5),
        _ => (26, 2, 0),
    };
    if delta % (1 << scale) != 0
        || delta < -(1i128 << (bits + scale - 1))
        || delta >= 1i128 << (bits + scale - 1)
    {
        return Err("AArch64 local branch/address overflow or misalignment".into());
    }
    let imm = ((delta >> scale) & ((1 << bits) - 1)) as u32;
    let mut word = u32::from_le_bytes(bytes.try_into().unwrap());
    if kind == 274 {
        word = (word & !((3 << 29) | (0x7ffff << 5))) | (imm & 3) << 29 | (imm >> 2) << 5;
    } else {
        word = (word & !(((1u32 << bits) - 1) << shift)) | imm << shift;
    }
    bytes.copy_from_slice(&word.to_le_bytes());
    Ok(())
}
pub(super) fn word(
    a: &mut Assembler,
    w: &crate::parser::elf::Word,
    line: usize,
) -> Result<(), String> {
    if !a.offset().is_multiple_of(4)
        || a.sections[a.current].nobits
        || a.sections[a.current].flags & 4 == 0
    {
        return Err("AArch64 instruction requires aligned executable PROGBITS data".into());
    }
    if let Some((kind, name, addend)) = &w.relocation {
        let id = a.symbols.intern(name);
        let expr = Expr {
            add: Some(id),
            subtract: None,
            constant: *addend as i128,
        };
        a.fixes.push(Fix {
            section: a.current,
            offset: a.offset(),
            width: 4,
            modifier: Modifier::None,
            expr,
            kind: *kind,
            line,
        });
    }
    a.encoded(&w.word.to_le_bytes())
}
pub(super) fn item(a: &mut Assembler, item: &Item, line: usize) -> Result<(), String> {
    let features = a.arm.unwrap();
    let (instruction, relocation) = match item {
        Item::Instruction(i) => {
            return word(
                a,
                &crate::parser::elf::Word {
                    word: crate::encode_instruction(i, features)?,
                    relocation: None,
                },
                line,
            );
        }
        Item::Branch { link, target } => {
            return word(
                a,
                &crate::parser::elf::Word {
                    word: if *link { 0x94000000 } else { 0x14000000 },
                    relocation: Some((if *link { 283 } else { 282 }, target.clone(), 0)),
                },
                line,
            );
        }
        Item::Symbolic(s) => {
            s.validate()?;
            let kind = match s {
                S::Conditional { .. } | S::Compare { .. } => 280,
                S::Address { .. } => 274,
                S::PageAddress { kind, .. } => match kind {
                    PageKind::Direct => 275,
                    PageKind::Got => 311,
                    PageKind::ThreadLocal => 541,
                },
                S::PageOffset { kind, load, .. } => match kind {
                    PageKind::Direct => {
                        if *load {
                            286
                        } else {
                            277
                        }
                    }
                    PageKind::Got => 312,
                    PageKind::ThreadLocal => 542,
                },
            };
            (s.zero_instruction(), Some((kind, s.target().to_owned(), 0)))
        }
        _ => return Err("non-AArch64 instruction in ARM ELF program".into()),
    };
    word(
        a,
        &crate::parser::elf::Word {
            word: crate::encode_instruction(&instruction, features)?,
            relocation,
        },
        line,
    )
}
pub(super) fn parse(items: &mut Items, source: &str, first: usize) -> Result<(), Error> {
    let mut block = false;
    for (index, raw) in source.lines().enumerate() {
        let line = first + index;
        let mut text = String::new();
        let (mut quoted, mut escaped) = (false, false);
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if block {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    block = false;
                    text.push(' ');
                }
                continue;
            }
            if !quoted && c == '/' {
                if chars.peek() == Some(&'/') {
                    break;
                }
                if chars.peek() == Some(&'*') {
                    chars.next();
                    block = true;
                    text.push(' ');
                    continue;
                }
            }
            if c == '"' && !escaped {
                quoted = !quoted;
            }
            text.push(c);
            escaped = quoted && c == '\\' && !escaped;
        }
        let (mut quoted, mut escaped, mut start) = (false, false, 0);
        let mut statements = Vec::new();
        for (i, c) in text.char_indices() {
            if c == '"' && !escaped {
                quoted = !quoted;
            }
            if c == ';' && !quoted {
                statements.push(&text[start..i]);
                start = i + 1;
            }
            escaped = quoted && c == '\\' && !escaped;
        }
        statements.push(&text[start..]);
        for statement in statements {
            let result = (|| -> Result<(), String> {
                let mut text = statement.trim();
                if text.is_empty() {
                    return Ok(());
                }
                if let Some(i) = text.find(':')
                    && !text[..i].contains(char::is_whitespace)
                {
                    let label = x86::symbol(&text[..i])?;
                    items.item(Item::Label(label), line);
                    text = text[i + 1..].trim();
                    if text.is_empty() {
                        return Ok(());
                    }
                }
                let (op, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
                if op.starts_with('.') {
                    let width = match op {
                        ".byte" => Some(1),
                        ".hword" | ".short" => Some(2),
                        ".word" | ".long" => Some(4),
                        ".xword" | ".quad" => Some(8),
                        _ => None,
                    };
                    if let Some(width) = width {
                        for arg in x86::split(rest, ',') {
                            if arg
                                .trim_start_matches(['#', '-', '+'])
                                .starts_with(|c: char| c.is_ascii_digit())
                            {
                                crate::parser::elf::data_integer(arg, width)?;
                            }
                        }
                    }
                    let op = match op {
                        ".word" => ".long",
                        ".hword" => ".short",
                        ".xword" => ".quad",
                        ".align" => ".p2align",
                        _ => op,
                    };
                    items
                        .parse(&format!("{op} {rest}"), line, &mut inputs::unavailable)
                        .map_err(|e| e.message)?;
                } else {
                    let word = crate::parser::elf::instruction(text, items.arm.unwrap())?;
                    let index = index32(items.arm_words.len());
                    items.arm_words.push(word);
                    items.push(Entry::ArmWord {
                        index,
                        line: line32(line),
                    });
                }
                Ok(())
            })();
            result.map_err(|e| Error::new(line, e))?;
        }
    }
    if block {
        return Err(Error::new(first, "unterminated block comment"));
    }
    Ok(())
}
