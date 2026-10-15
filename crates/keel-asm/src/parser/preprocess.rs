//! Optional, bounded textual assembly expansion. Entered only after the ordinary
//! parser encounters a preprocessing directive; no filesystem access or global state.
//!
//! Supported: plain named macro parameters, comma-separated positional calls,
//! `\name` substitution and `\()` concatenation; nested literal `.rept`;
//! literal `.if`/`.ifeq`/`.ifne`/`.ifgt`/`.ifge`/`.iflt`/`.ifle` with
//! `.elseif`, `.else`, `.endif`. Expressions and symbol-dependent tests, defaults,
//! keyword/variadic/required parameters and Darwin positional operators are rejected.
//! Macro calls/directives must occupy their own line (labels can precede them
//! on a separate line). Diagnostics/metrics name definition lines; expanded
//! diagnostics also identify the outer invocation. Bounds apply per assembly.
use super::{Error, scanning};
use std::collections::BTreeMap;

const MAX_DEPTH: usize = 64;
const MAX_STEPS: usize = 100_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn required(error: &Error) -> bool {
    error
        .message
        .strip_prefix("unsupported directive `")
        .and_then(|s| s.strip_suffix('`'))
        .is_some_and(|op| {
            matches!(
                op,
                ".macro"
                    | ".endm"
                    | ".endmacro"
                    | ".rept"
                    | ".endr"
                    | ".if"
                    | ".ifeq"
                    | ".ifne"
                    | ".ifgt"
                    | ".ifge"
                    | ".iflt"
                    | ".ifle"
                    | ".else"
                    | ".elseif"
                    | ".endif"
            )
        })
}

#[derive(Clone)]
struct Line {
    text: String,
    line: usize,
    invocation: Option<usize>,
}
impl Line {
    fn error(&self, message: impl Into<String>) -> Error {
        let mut message = message.into();
        if let Some(line) = self.invocation {
            message.push_str(&format!(" (expanded at line {line})"));
        }
        Error::new(self.line, message)
    }
}

pub(super) struct Expanded {
    pub(super) source: String,
    origins: Vec<Line>,
}
impl Expanded {
    pub(super) fn original_line(&self, line: usize) -> usize {
        self.origins
            .get(line.wrapping_sub(1))
            .map_or(line, |l| l.line)
    }
    pub(super) fn remap(&self, error: Error) -> Error {
        self.origins
            .get(error.line.wrapping_sub(1))
            .map_or_else(|| error.clone(), |line| line.error(error.message.clone()))
    }
}

#[derive(Clone)]
struct Macro {
    parameters: Vec<String>,
    body: Vec<Line>,
}
#[derive(Default)]
struct Processor {
    macros: BTreeMap<String, Macro>,
    output: String,
    origins: Vec<Line>,
    steps: usize,
}

pub(super) fn expand(source: &str) -> Result<Expanded, Error> {
    let lines: Vec<_> = scanning::Lines::new(source)
        .enumerate()
        .map(|(i, text)| Line {
            text: text.to_owned(),
            line: i + 1,
            invocation: None,
        })
        .collect();
    let mut processor = Processor::default();
    processor.run(&lines, 0)?;
    Ok(Expanded {
        source: processor.output,
        origins: processor.origins,
    })
}

fn parts(text: &str) -> (&str, &str) {
    let text = text.trim();
    text.split_once(char::is_whitespace)
        .map_or((text, ""), |(op, args)| (op, args.trim()))
}
fn identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn literal(text: &str, line: &Line) -> Result<i64, Error> {
    if text.starts_with('#') {
        return Err(line.error("preprocessing requires an integer literal without #"));
    }
    super::integer(text).map_err(|_| line.error("preprocessing requires an integer literal; symbolic/arithmetic expressions are unsupported"))
}

// LLVM protects commas in strings and parentheses, but not A64 address brackets.
// Restrict calls to comma-separated arguments; parameter declarations also accept spaces.
fn arguments(text: &str, line: &Line, declaration: bool) -> Result<Vec<String>, Error> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut quoted = false;
    let mut escaped = false;
    let mut nesting = Vec::new();
    let mut commas = Vec::new();
    let mut spaces = Vec::new();
    for (i, c) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match c {
            '(' => nesting.push(c),
            ')' => {
                let expected = '(';
                if nesting.pop() != Some(expected) {
                    return Err(line.error("unbalanced macro argument"));
                }
            }
            '=' if !declaration => {
                return Err(line.error("keyword macro arguments are unsupported"));
            }
            ',' if nesting.is_empty() => commas.push(i),
            c if c.is_whitespace() && nesting.is_empty() => spaces.push(i),
            _ => {}
        }
    }
    if quoted || !nesting.is_empty() {
        return Err(line.error("unterminated macro argument"));
    }
    let comma_mode = !commas.is_empty();
    let mut result = Vec::new();
    let mut start = 0;
    for end in if comma_mode {
        commas
    } else if declaration {
        spaces
    } else {
        Vec::new()
    } {
        let arg = text[start..end].trim();
        if comma_mode || !arg.is_empty() {
            result.push(arg.to_owned());
        }
        start = end + 1;
    }
    let arg = text[start..].trim();
    if comma_mode || !arg.is_empty() {
        result.push(arg.to_owned());
    }
    if !declaration {
        for arg in &mut result {
            if arg.starts_with('"') && arg.ends_with('"') && arg.len() >= 2 {
                let mut escaped = false;
                let mut quotes = 0;
                for c in arg.chars() {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        quotes += 1;
                    }
                }
                if quotes != 2 {
                    return Err(line.error("mixed quoted macro arguments are unsupported"));
                }
                *arg = arg[1..arg.len() - 1].to_owned();
            } else if arg.chars().any(char::is_whitespace) {
                return Err(line.error("macro arguments containing whitespace must be quoted"));
            }
        }
    }
    Ok(result)
}

impl Processor {
    fn step(&mut self, line: &Line) -> Result<(), Error> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(line.error("assembly expansion exceeds 100000 steps"));
        }
        Ok(())
    }
    fn emit(&mut self, line: &Line) -> Result<(), Error> {
        if self.output.len().saturating_add(line.text.len() + 1) > MAX_BYTES {
            return Err(line.error("assembly expansion exceeds 8 MiB"));
        }
        self.output.push_str(&line.text);
        self.output.push('\n');
        self.origins.push(line.clone());
        Ok(())
    }
    fn run(&mut self, lines: &[Line], depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(lines.first().map_or_else(
                || Error::new(0, "assembly expansion nesting exceeds 64"),
                |l| l.error("assembly expansion nesting exceeds 64"),
            ));
        }
        let mut i = 0;
        // (parent active, any branch taken, current branch active, saw else)
        let mut conditions: Vec<(bool, bool, bool, bool)> = Vec::new();
        while i < lines.len() {
            let line = &lines[i];
            self.step(line)?;
            let (op, args) = parts(&line.text);
            let lower = op.to_ascii_lowercase();
            let active = conditions.last().is_none_or(|c| c.2);
            match lower.as_str() {
                ".if" | ".ifeq" | ".ifne" | ".ifgt" | ".ifge" | ".iflt" | ".ifle" => {
                    let yes = if active {
                        let n = literal(args, line)?;
                        match lower.as_str() {
                            ".ifeq" => n == 0,
                            ".ifgt" => n > 0,
                            ".ifge" => n >= 0,
                            ".iflt" => n < 0,
                            ".ifle" => n <= 0,
                            _ => n != 0,
                        }
                    } else {
                        false
                    };
                    if conditions.len() >= MAX_DEPTH {
                        return Err(line.error("conditional nesting exceeds 64"));
                    }
                    conditions.push((active, yes, active && yes, false));
                }
                ".else" | ".elseif" => {
                    let c = conditions
                        .last_mut()
                        .ok_or_else(|| line.error("unmatched conditional branch"))?;
                    if c.3 {
                        return Err(line.error("conditional branch after .else"));
                    }
                    let yes = if lower == ".else" {
                        if !args.is_empty() {
                            return Err(line.error("unexpected .else operands"));
                        }
                        c.3 = true;
                        true
                    } else if c.0 && !c.1 {
                        literal(args, line)? != 0
                    } else {
                        false
                    };
                    c.2 = c.0 && !c.1 && yes;
                    c.1 |= yes;
                }
                ".endif" => {
                    if !args.is_empty() {
                        return Err(line.error("unexpected .endif operands"));
                    }
                    conditions
                        .pop()
                        .ok_or_else(|| line.error("unmatched .endif"))?;
                }
                ".macro" | ".rept" => {
                    let end = block_end(lines, i)?;
                    if active {
                        if lower == ".macro" {
                            let (name, params) = parts(args);
                            let name = name.strip_suffix(',').unwrap_or(name);
                            if !identifier(name) {
                                return Err(line.error("macro name must be an identifier"));
                            }
                            let parameters = arguments(params, line, true)?;
                            let mut seen = std::collections::BTreeSet::new();
                            for parameter in &parameters {
                                if !identifier(parameter) || !seen.insert(parameter.clone()) {
                                    return Err(line.error("macro parameters must be unique plain names; defaults, :req and :vararg are unsupported"));
                                }
                            }
                            if self.macros.contains_key(name) {
                                return Err(line.error(format!("macro `{name}` already defined")));
                            }
                            self.macros.insert(
                                name.into(),
                                Macro {
                                    parameters,
                                    body: lines[i + 1..end].to_vec(),
                                },
                            );
                        } else {
                            let count = literal(args, line)?;
                            if !(0..=MAX_STEPS as i64).contains(&count) {
                                return Err(line.error(".rept count must be in 0..=100000"));
                            }
                            for _ in 0..count {
                                self.step(line)?;
                                self.run(&lines[i + 1..end], depth + 1)?;
                            }
                        }
                    }
                    i = end;
                }
                ".endm" | ".endmacro" | ".endr" => {
                    return Err(line.error("unmatched expansion terminator"));
                }
                _ if active => {
                    if let Some(mac) = self.macros.get(op).cloned() {
                        let values = arguments(args, line, false)?;
                        if values.len() != mac.parameters.len() {
                            return Err(line.error(format!(
                                "macro `{op}` expects {} arguments, got {}",
                                mac.parameters.len(),
                                values.len()
                            )));
                        }
                        let mut body_bytes = 0usize;
                        let body = mac
                            .body
                            .iter()
                            .map(|body| {
                                let text = substitute(body, &mac.parameters, &values)?;
                                body_bytes = body_bytes.saturating_add(text.len() + 1);
                                if body_bytes > MAX_BYTES {
                                    return Err(line.error("macro substitution exceeds 8 MiB"));
                                }
                                Ok(Line {
                                    text,
                                    line: body.line,
                                    invocation: Some(line.invocation.unwrap_or(line.line)),
                                })
                            })
                            .collect::<Result<Vec<_>, Error>>()?;
                        self.run(&body, depth + 1)?;
                    } else {
                        self.emit(line)?;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if !conditions.is_empty() {
            return Err(lines
                .last()
                .unwrap()
                .error("unterminated conditional assembly"));
        }
        Ok(())
    }
}

fn block_end(lines: &[Line], start: usize) -> Result<usize, Error> {
    let mut stack = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(start) {
        let (op, args) = parts(&line.text);
        match op.to_ascii_lowercase().as_str() {
            ".macro" => stack.push("macro"),
            ".rept" => stack.push("rept"),
            ".endm" | ".endmacro" | ".endr" => {
                let expected = if op.eq_ignore_ascii_case(".endr") {
                    "rept"
                } else {
                    "macro"
                };
                if stack.pop() != Some(expected) {
                    return Err(line.error("mismatched expansion terminator"));
                }
                if !args.is_empty() {
                    return Err(line.error("unexpected expansion terminator operands"));
                }
                if stack.is_empty() {
                    return Ok(i);
                }
            }
            _ => {}
        }
        if stack.len() > MAX_DEPTH {
            return Err(line.error("assembly expansion nesting exceeds 64"));
        }
    }
    Err(lines[start].error("unterminated .macro or .rept block"))
}

fn substitute(line: &Line, parameters: &[String], values: &[String]) -> Result<String, Error> {
    if line
        .text
        .as_bytes()
        .windows(2)
        .any(|s| s[0] == b'$' && s[1].is_ascii_digit())
    {
        return Err(
            line.error("Darwin positional macro parameters are unsupported; use named parameters")
        );
    }
    let mut result = String::new();
    let mut rest = line.text.as_str();
    while let Some(index) = rest.find('\\') {
        result.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        if let Some(tail) = rest.strip_prefix("()") {
            rest = tail;
            continue;
        }
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if end == 0 {
            // Preserve string escapes such as \" and \\. Unsupported macro
            // operators must fail instead of being silently reinterpreted.
            if rest.starts_with(['@', '+']) {
                return Err(line.error("unsupported macro substitution operator"));
            }
            result.push('\\');
            if let Some(c) = rest.chars().next() {
                result.push(c);
                rest = &rest[c.len_utf8()..];
            }
            continue;
        }
        let name = &rest[..end];
        if let Some(index) = parameters.iter().position(|p| p == name) {
            result.push_str(&values[index]);
        } else {
            // LLVM leaves ordinary string escapes alone. Unknown names remain
            // for the parser to diagnose, except known string escape prefixes.
            if !matches!(
                name.as_bytes()[0],
                b'n' | b'r' | b't' | b'b' | b'f' | b'v' | b'x' | b'0'..=b'7'
            ) {
                return Err(line.error(format!("unknown macro parameter `{name}`")));
            }
            result.push('\\');
            result.push_str(name);
        }
        rest = &rest[end..];
        if result.len() > MAX_BYTES {
            return Err(line.error("macro substitution exceeds 8 MiB"));
        }
    }
    result.push_str(rest);
    if result.len() > MAX_BYTES {
        return Err(line.error("macro substitution exceeds 8 MiB"));
    }
    Ok(result)
}
