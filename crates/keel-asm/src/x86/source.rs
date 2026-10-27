//! GNU statement scanning and numeric local names, before instruction parsing.
use crate::Error;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

/// Preserve physical lines while removing comments and separating statements.
pub(crate) fn statements(source: &str) -> Result<Vec<(usize, String)>, Error> {
    let mut result = Vec::new();
    let mut text = String::new();
    let (mut line, mut start) = (1, 1);
    let (mut quoted, mut escaped, mut block, mut comment) = (false, false, false, false);
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if !block {
                push_statement(&mut result, start, std::mem::take(&mut text));
            }
            line += 1;
            if text.trim().is_empty() {
                start = line;
            }
            comment = false;
            continue;
        }
        if block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block = false;
            }
            continue;
        }
        if comment {
            continue;
        }
        if escaped {
            text.push(c);
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            text.push(c);
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted {
            if c == '#' || (c == '/' && chars.peek() == Some(&'/')) {
                comment = true;
                continue;
            }
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block = true;
                text.push(' ');
                continue;
            }
            if c == ';' {
                push_statement(&mut result, start, std::mem::take(&mut text));
                start = line;
                continue;
            }
        }
        text.push(c);
    }
    if block {
        return Err(Error::new(line, "unterminated block comment"));
    }
    push_statement(&mut result, start, text);
    Ok(result)
}

// Split leading labels before expansion as well: a GNU macro invocation may
// follow a label, and multiple labels can name the same instruction.
fn push_statement(result: &mut Vec<(usize, String)>, line: usize, text: String) {
    let mut rest = text.trim();
    loop {
        let labels = keel_asm_ir::x86::split_inline(rest, ':');
        if labels.len() <= 1
            || (!labels[0].starts_with('"') && labels[0].trim().contains(char::is_whitespace))
        {
            break;
        }
        let prefix = labels[0].len();
        let end = prefix + rest[prefix..].find(':').unwrap() + 1;
        result.push((line, rest[..end].to_owned()));
        rest = rest[end..].trim();
    }
    result.push((line, rest.to_owned()));
}

/// GNU register symbols are lexical operand aliases, not ELF data symbols.
/// Keep their definitions between chunks, just like macro definitions.
#[derive(Default)]
pub(super) struct RegisterAliases {
    names: BTreeMap<String, keel_asm_ir::x86::Register>,
    ordinary: BTreeSet<String>,
    prefix: bool,
}
impl RegisterAliases {
    pub fn new(prefix: bool) -> Self {
        Self {
            prefix,
            ..Self::default()
        }
    }
    pub fn rewrite<'s>(&mut self, source: &'s str) -> Result<Option<Cow<'s, str>>, String> {
        let source = source.trim();
        let (op, args) = source
            .split_once(char::is_whitespace)
            .unwrap_or((source, ""));
        if let Some(label) = source.strip_suffix(':') {
            let name = keel_asm_ir::x86::symbol(label)?;
            self.names.remove(&name);
            self.ordinary.insert(name);
            return Ok(Some(Cow::Borrowed(source)));
        }
        if matches!(
            op,
            ".globl"
                | ".global"
                | ".weak"
                | ".local"
                | ".hidden"
                | ".protected"
                | ".type"
                | ".size"
        ) {
            let names = keel_asm_ir::x86::split_inline(args, ',');
            let names = if matches!(op, ".type" | ".size") {
                &names[..names.len().min(1)]
            } else {
                &names
            };
            for name in names {
                if let Ok(name) = keel_asm_ir::x86::symbol(name) {
                    if self.names.contains_key(&name) {
                        return Err("register aliases cannot carry ELF symbol attributes".into());
                    }
                    self.ordinary.insert(name);
                }
            }
        }
        if matches!(op, ".att_syntax" | ".intel_syntax") {
            self.prefix = match args.trim() {
                "prefix" => true,
                "noprefix" => false,
                _ => op == ".att_syntax",
            };
        }
        if matches!(op, ".set" | ".equ")
            && let Some((name, value)) = args.split_once(',')
        {
            let name = keel_asm_ir::x86::symbol(name)?;
            let value = value.trim();
            let target = value
                .strip_prefix('%')
                .or((!self.prefix).then_some(value))
                .and_then(keel_asm_ir::x86::Register::parse)
                .or_else(|| self.names.get(value).copied());
            if let Some(target) = target {
                if self.ordinary.contains(&name) {
                    return Err(
                        "redefining an ordinary symbol as a register alias is unsupported".into(),
                    );
                }
                self.names.insert(name, target);
                return Ok(None);
            }
            self.names.remove(&name);
            self.ordinary.insert(name);
        }
        if op.starts_with('.') || source.ends_with(':') || args.is_empty() || self.names.is_empty()
        {
            return Ok(Some(Cow::Borrowed(source)));
        }
        let mut output = String::with_capacity(source.len());
        output.push_str(op);
        output.push(' ');
        let mut chars = args.char_indices().peekable();
        let mut quoted = false;
        let mut escaped = false;
        let mut previous = None;
        while let Some((start, c)) = chars.next() {
            if escaped {
                output.push(c);
                escaped = false;
            } else if quoted && c == '\\' {
                output.push(c);
                escaped = true;
            } else if c == '"' {
                output.push(c);
                quoted = !quoted;
            } else if !quoted && (c.is_ascii_alphabetic() || matches!(c, '_' | '.' | '$')) {
                let mut end = start + c.len_utf8();
                while let Some((i, next)) = chars.peek() {
                    if !next.is_ascii_alphanumeric() && !matches!(next, '_' | '.' | '$') {
                        break;
                    }
                    end = i + next.len_utf8();
                    chars.next();
                }
                let name = &args[start..end];
                if previous != Some('%')
                    && let Some(target) = self.names.get(name)
                {
                    if self.prefix {
                        output.push('%');
                    }
                    output.push_str(&target.to_string());
                } else {
                    output.push_str(name);
                }
            } else {
                output.push(c);
            }
            previous = Some(c);
        }
        Ok(Some(Cow::Owned(output)))
    }
}

#[derive(Default)]
pub(super) struct Locals {
    counts: BTreeMap<String, usize>,
    forward: Vec<(String, usize, usize)>,
}
impl Locals {
    pub fn finish(&self) -> Result<(), Error> {
        for (number, occurrence, line) in &self.forward {
            if self.counts.get(number).copied().unwrap_or(0) < *occurrence {
                return Err(Error::new(
                    *line,
                    format!("undefined numeric local label `{number}f`"),
                ));
            }
        }
        Ok(())
    }
    pub fn rewrite(&mut self, source: &str, line: usize) -> Result<String, Error> {
        let mut out = String::new();
        let bytes = source.as_bytes();
        let (mut i, mut quoted, mut escaped) = (0, false, false);
        while i < bytes.len() {
            let c = bytes[i];
            if escaped {
                let c = source[i..].chars().next().unwrap();
                out.push(c);
                escaped = false;
                i += c.len_utf8();
                continue;
            }
            if quoted && c == b'\\' {
                escaped = true;
            }
            if c == b'"' {
                quoted = !quoted;
            }
            if !quoted
                && c.is_ascii_digit()
                && (i == 0
                    || !name_byte(bytes[i - 1])
                    || (bytes[i - 1] == b'$' && (i == 1 || !name_byte(bytes[i - 2]))))
            {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let digits = &source[start..i];
                let number = digits.trim_start_matches('0');
                let number = if number.is_empty() { "0" } else { number };
                let tail = &source[i..];
                let definition =
                    tail.trim_start().starts_with(':') && source[..start].trim().is_empty();
                let direction = bytes
                    .get(i)
                    .copied()
                    .filter(|c| matches!(c, b'b' | b'f'))
                    .filter(|_| bytes.get(i + 1).is_none_or(|c| !name_byte(*c)));
                if definition || direction.is_some() {
                    let count = self.counts.entry(number.to_owned()).or_default();
                    let occurrence = if definition {
                        *count += 1;
                        *count
                    } else if direction == Some(b'f') {
                        *count + 1
                    } else if *count == 0 {
                        return Err(Error::new(
                            line,
                            format!("undefined numeric local label `{number}b`"),
                        ));
                    } else {
                        *count
                    };
                    if direction == Some(b'f') {
                        self.forward.push((number.into(), occurrence, line));
                    }
                    // A quoted control character keeps these names disjoint from
                    // ordinary source identifiers. ELF discards .L local names.
                    out.push_str(&format!("\".Lkeel.numeric.\\001{number}.{occurrence}\""));
                    if direction.is_some() {
                        i += 1;
                    }
                } else {
                    out.push_str(digits);
                }
                continue;
            }
            let c = source[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
        Ok(out)
    }
}
fn name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b"_.$".contains(&c)
}
