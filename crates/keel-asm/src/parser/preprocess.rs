//! Optional, bounded textual assembly expansion. Entered only after the ordinary
//! parser encounters a preprocessing directive; no filesystem access or global state.
//!
//! GNU x86 enables optional/default/keyword arguments, iteration and bounded
//! integer expressions. AArch64 retains its existing restricted macro grammar.
//! Expansion never assumes values for unresolved symbols or layout positions.
use super::{Error, scanning};
use std::collections::BTreeMap;

const MAX_DEPTH: usize = 64;
const MAX_STEPS: usize = 100_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn required(error: &Error) -> bool {
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

pub(crate) struct Expanded {
    pub(crate) source: String,
    origins: Vec<Line>,
}
impl Expanded {
    pub(crate) fn original_line(&self, line: usize) -> usize {
        self.origins
            .get(line.wrapping_sub(1))
            .map_or(line, |l| l.line)
    }
    pub(crate) fn remap(&self, error: Error) -> Error {
        self.origins
            .get(error.line.wrapping_sub(1))
            .map_or_else(|| error.clone(), |line| line.error(error.message.clone()))
    }
}

#[derive(Clone)]
struct Macro {
    parameters: Vec<String>,
    required: Vec<bool>,
    body: Vec<Line>,
    defaults: Vec<String>,
}
#[derive(Default)]
pub(crate) struct Processor {
    macros: BTreeMap<String, Macro>,
    constants: BTreeMap<String, i128>,
    gnu: bool,
    output: String,
    origins: Vec<Line>,
    steps: usize,
    invocations: usize,
    emitted_bytes: usize,
}

pub(crate) fn expand(source: &str) -> Result<Expanded, Error> {
    let arena = bumpalo::Bump::new();
    let lines: Vec<_> = scanning::Lines::new(source, &arena)
        .map(|result| {
            result.map(|(line, text)| Line {
                text: text.to_owned(),
                line,
                invocation: None,
            })
        })
        .collect::<Result<_, Error>>()?;
    process(lines)
}

// The x86 lexer supplies statements because GNU x86 uses semicolons as
// separators and # as comments, unlike the shared AArch64 scanner.
pub(crate) fn expand_statements(
    processor: &mut Processor,
    lines: Vec<(usize, String)>,
) -> Result<Expanded, Error> {
    processor.gnu = true;
    let lines: Vec<_> = lines
        .into_iter()
        .map(|(line, text)| Line {
            text,
            line,
            invocation: None,
        })
        .collect();
    if !processor.macros.is_empty()
        || !processor.constants.is_empty()
        || lines.iter().any(|l| {
            l.text.split_once('=').is_some_and(|(name, rhs)| {
                keel_asm_ir::x86::symbol(name).is_ok() && !rhs.starts_with('=')
            }) || matches!(
                parts(&l.text).0,
                ".macro"
                    | ".endm"
                    | ".endmacro"
                    | ".irp"
                    | ".irpc"
                    | ".ifc"
                    | ".ifnc"
                    | ".purgem"
                    | ".set"
                    | ".equ"
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
    {
        processor.expand_lines(&lines)
    } else {
        // Ordinary inputs have no expansion budget and need no macro lookup.
        let mut source = String::new();
        for line in &lines {
            source.push_str(&line.text);
            source.push('\n');
        }
        Ok(Expanded {
            source,
            origins: lines,
        })
    }
}

fn process(lines: Vec<Line>) -> Result<Expanded, Error> {
    let mut processor = Processor::default();
    processor.expand_lines(&lines)
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
// GNU x86 also permits whitespace separators outside parenthesized operands.
fn arguments(text: &str, line: &Line, declaration: bool, gnu: bool) -> Result<Vec<String>, Error> {
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
            '=' if !declaration && !gnu => {
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
    } else if declaration || gnu {
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
    if gnu && comma_mode {
        let mut mixed = Vec::new();
        for arg in result {
            if arg.is_empty() {
                mixed.push(arg);
            } else {
                mixed.extend(arguments(&arg, line, declaration, gnu)?);
            }
        }
        return Ok(mixed);
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
            } else if !gnu && arg.chars().any(char::is_whitespace) {
                return Err(line.error("macro arguments containing whitespace must be quoted"));
            }
        }
    }
    Ok(result)
}

impl Processor {
    fn number(&self, text: &str, line: &Line) -> Result<i128, Error> {
        if !self.gnu {
            return literal(text, line).map(i128::from);
        }
        // Kernel unwind macros compare register spellings. Registers are not
        // integer constants, so support equality without inventing numeric IDs.
        for op in ["==", "!="] {
            if let Some((a, b)) = text.split_once(op)
                && (a.trim().starts_with('%') || b.trim().starts_with('%'))
            {
                let register = |s: &str| {
                    s.trim()
                        .strip_prefix('%')
                        .and_then(keel_asm_ir::x86::Register::parse)
                        .ok_or_else(|| {
                            line.error("register conditional requires two registers and == or !=")
                        })
                };
                return Ok(-i128::from((register(a)? == register(b)?) == (op == "==")));
            }
        }
        keel_asm_ir::x86::expression::parse(text)
            .and_then(|e| {
                e.eval(&mut |s| {
                    self.constants
                        .get(s)
                        .copied()
                        .map(keel_asm_ir::x86::expression::Value::absolute)
                        .ok_or_else(|| {
                            format!(
                                "undefined or layout-dependent symbol `{s}` in assembly conditional"
                            )
                        })
                })
            })
            .map(|v| v.number)
            .map_err(|e| line.error(e))
    }
    fn string_test(&self, args: &str, line: &Line) -> Result<bool, Error> {
        let values = keel_asm_ir::x86::split_inline(args, ',');
        if values.len() != 2 {
            return Err(line.error("string conditional requires two comma-separated arguments"));
        }
        let normalize = |s: &str| {
            s.trim()
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(s.trim())
                .to_owned()
        };
        Ok(normalize(values[0]) == normalize(values[1]))
    }
    fn gnu_line(&mut self, line: &Line) -> Result<Line, Error> {
        let (op, args) = parts(&line.text);
        let assignment = if matches!(op, ".set" | ".equ") {
            args.split_once(',')
        } else {
            line.text.split_once('=').filter(|(name, rhs)| {
                keel_asm_ir::x86::symbol(name).is_ok() && !rhs.starts_with('=')
            })
        };
        if let Some((name, expr)) = assignment {
            let name = name.trim();
            keel_asm_ir::x86::symbol(name).map_err(|e| line.error(e))?;
            let text = replace_constants(expr, &self.constants, false);
            if let Ok(value) = keel_asm_ir::x86::integer(&text) {
                self.constants.insert(name.into(), value);
                return Ok(Line {
                    text: format!(".set {name}, {value}"),
                    ..line.clone()
                });
            }
            self.constants.remove(name);
            return Ok(Line {
                text: format!(".set {name}, {text}"),
                ..line.clone()
            });
        }
        // Only operand expressions are substituted. Names in labels, section
        // declarations and symbol attributes retain their identity.
        if !op.ends_with(':')
            && (!op.starts_with('.')
                || matches!(
                    op,
                    ".byte"
                        | ".word"
                        | ".short"
                        | ".long"
                        | ".int"
                        | ".quad"
                        | ".4byte"
                        | ".8byte"
                        | ".2byte"
                        | ".skip"
                        | ".fill"
                        | ".incbin"
                        | ".space"
                        | ".zero"
                        | ".org"
                        | ".rept"
                        | ".uleb128"
                        | ".sleb128"
                        | ".align"
                        | ".balign"
                        | ".p2align"
                ))
        {
            let operands = if !op.starts_with('.') {
                // A positive absolute symbol in S-A may be the identity used
                // by a PC-relative relocation. Do not turn it into a pure
                // constant minus a section symbol, which ELF cannot represent
                // through the public linear expression model.
                let protected: Vec<_> = keel_asm_ir::x86::split_inline(args, ',')
                    .iter()
                    .filter_map(|arg| arg.trim().strip_prefix('$'))
                    .filter_map(|arg| keel_asm_ir::x86::address(arg).ok())
                    .filter(|a| a.expression.add.is_some() && a.expression.subtract.is_some())
                    .flat_map(|a| [a.expression.add.unwrap(), a.expression.subtract.unwrap()])
                    .collect();
                if protected.is_empty() {
                    replace_constants(args, &self.constants, true)
                } else {
                    let mut constants = self.constants.clone();
                    for name in protected {
                        constants.remove(&name);
                    }
                    replace_constants(args, &constants, true)
                }
            } else {
                keel_asm_ir::x86::split_inline(args, ',')
                    .iter()
                    .map(|arg| {
                        if keel_asm_ir::x86::address(arg).is_ok_and(|a| {
                            a.expression.add.is_some() && a.expression.subtract.is_some()
                        }) {
                            (*arg).to_owned()
                        } else {
                            replace_constants(arg, &self.constants, false)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            return Ok(Line {
                text: format!("{op} {operands}"),
                ..line.clone()
            });
        }
        Ok(line.clone())
    }
    fn expand_lines(&mut self, lines: &[Line]) -> Result<Expanded, Error> {
        self.run(lines, 0)?;
        Ok(Expanded {
            source: std::mem::take(&mut self.output),
            origins: std::mem::take(&mut self.origins),
        })
    }
    fn step(&mut self, line: &Line) -> Result<(), Error> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(line.error("assembly expansion exceeds 100000 steps"));
        }
        Ok(())
    }
    fn emit(&mut self, line: &Line) -> Result<(), Error> {
        self.emitted_bytes = self.emitted_bytes.saturating_add(line.text.len() + 1);
        if self.emitted_bytes > MAX_BYTES {
            return Err(line.error("assembly expansion exceeds 8 MiB"));
        }
        self.output.push_str(&line.text);
        self.output.push('\n');
        self.origins.push(line.clone());
        Ok(())
    }
    fn rescan(&self, lines: Vec<Line>) -> Result<Vec<Line>, Error> {
        if !self.gnu {
            return Ok(lines);
        }
        let mut out = Vec::new();
        for line in lines {
            let statements =
                crate::x86::source::statements(&line.text).map_err(|e| line.error(e.message))?;
            for (_, text) in statements {
                out.push(Line {
                    text,
                    ..line.clone()
                });
            }
        }
        Ok(out)
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
                ".ifc" | ".ifnc" | ".irp" | ".irpc" if !self.gnu => {
                    return Err(line.error(format!("unsupported directive `{lower}`")));
                }
                ".if" | ".ifeq" | ".ifne" | ".ifgt" | ".ifge" | ".iflt" | ".ifle" | ".ifc"
                | ".ifnc" => {
                    let yes = if active {
                        let n = if lower == ".ifc" || lower == ".ifnc" {
                            i128::from(self.string_test(args, line)? == (lower == ".ifc"))
                        } else {
                            self.number(args, line)?
                        };
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
                        self.number(args, line)? != 0
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
                ".macro" | ".rept" | ".irp" | ".irpc" => {
                    let end = block_end(lines, i)?;
                    if active {
                        if lower == ".macro" {
                            let (name, params) = parts(args);
                            let name = name.strip_suffix(',').unwrap_or(name);
                            if !identifier(name) {
                                return Err(line.error("macro name must be an identifier"));
                            }
                            let declarations = arguments(params, line, true, self.gnu)?;
                            let mut parameters = Vec::new();
                            let mut required = Vec::new();
                            let mut defaults = Vec::new();
                            let mut seen = std::collections::BTreeSet::new();
                            for declaration in &declarations {
                                let (declaration, default) = if self.gnu {
                                    declaration.split_once('=').unwrap_or((declaration, ""))
                                } else {
                                    (declaration.as_str(), "")
                                };
                                let parameter =
                                    declaration.strip_suffix(":req").unwrap_or(declaration);
                                if !identifier(parameter) || !seen.insert(parameter.to_owned()) {
                                    return Err(line.error("macro parameters must be unique plain names; defaults and :vararg are unsupported"));
                                }
                                if declaration.ends_with(":req") && !default.is_empty() {
                                    return Err(line
                                        .error("required macro parameter cannot have a default"));
                                }
                                defaults.push(
                                    default
                                        .strip_prefix('"')
                                        .and_then(|s| s.strip_suffix('"'))
                                        .unwrap_or(default)
                                        .to_owned(),
                                );
                                parameters.push(parameter.to_owned());
                                required.push(declaration.ends_with(":req"));
                            }
                            if self.macros.contains_key(name) {
                                return Err(line.error(format!("macro `{name}` already defined")));
                            }
                            self.macros.insert(
                                name.into(),
                                Macro {
                                    parameters,
                                    required,
                                    defaults,
                                    body: lines[i + 1..end].to_vec(),
                                },
                            );
                        } else if lower == ".irp" || lower == ".irpc" {
                            let (name, text) = args.split_once(',').unwrap_or((args, ""));
                            let name = name.trim();
                            if !identifier(name) {
                                return Err(line.error("iteration parameter must be an identifier"));
                            }
                            let values = if lower == ".irp" {
                                if text.trim().is_empty() {
                                    vec![String::new()]
                                } else {
                                    arguments(text.trim(), line, false, true)?
                                }
                            } else {
                                let text = text.trim();
                                if text.is_empty() {
                                    return Err(line.error(".irpc requires a character string"));
                                }
                                let text = text
                                    .strip_prefix('"')
                                    .and_then(|s| s.strip_suffix('"'))
                                    .unwrap_or(text);
                                if !text.is_ascii() {
                                    return Err(line.error(".irpc requires ASCII characters"));
                                }
                                text.chars().map(|c| c.to_string()).collect()
                            };
                            for value in values {
                                self.step(line)?;
                                let mut parameters = vec![name.into()];
                                let mut values = vec![value];
                                iteration_bindings(
                                    &lines[i + 1..end],
                                    &mut parameters,
                                    &mut values,
                                );
                                let body = lines[i + 1..end]
                                    .iter()
                                    .map(|body| {
                                        Ok(Line {
                                            text: substitute(
                                                body,
                                                &parameters,
                                                &values,
                                                self.invocations,
                                                self.gnu,
                                            )?,
                                            ..body.clone()
                                        })
                                    })
                                    .collect::<Result<Vec<_>, Error>>()?;
                                let body = self.rescan(body)?;
                                self.run(&body, depth + 1)?;
                            }
                        } else {
                            let count = self.number(args, line)?;
                            if !(0..=MAX_STEPS as i128).contains(&count) {
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
                        let supplied = arguments(args, line, false, self.gnu)?;
                        let values = if self.gnu {
                            let mut values = mac.defaults.clone();
                            let mut seen = vec![false; values.len()];
                            let (mut positional, mut keyword) = (0, false);
                            for value in supplied {
                                let (index, value) =
                                    if let Some((name, value)) = value.split_once('=') {
                                        keyword = true;
                                        let index = mac
                                            .parameters
                                            .iter()
                                            .position(|p| p == name.trim())
                                            .ok_or_else(|| {
                                                line.error(format!(
                                                    "unknown keyword macro argument `{name}`"
                                                ))
                                            })?;
                                        let value = value.trim();
                                        (
                                            index,
                                            value
                                                .strip_prefix('"')
                                                .and_then(|s| s.strip_suffix('"'))
                                                .unwrap_or(value)
                                                .to_owned(),
                                        )
                                    } else {
                                        if keyword {
                                            return Err(line.error(
                                                "positional macro argument after keyword argument",
                                            ));
                                        }
                                        let index = positional;
                                        positional += 1;
                                        (index, value)
                                    };
                                if index >= values.len() {
                                    return Err(line.error(format!(
                                        "macro `{op}` expects at most {} arguments",
                                        values.len()
                                    )));
                                }
                                if seen[index] {
                                    return Err(line.error("duplicate macro argument"));
                                }
                                seen[index] = true;
                                if !value.is_empty() {
                                    values[index] = value;
                                }
                            }
                            values
                        } else {
                            supplied
                        };
                        for (index, required) in mac.required.iter().enumerate() {
                            if *required && values.get(index).is_none_or(|v| v.is_empty()) {
                                return Err(line.error(format!(
                                    "required macro argument `{}` is missing",
                                    mac.parameters[index]
                                )));
                            }
                        }
                        if values.len() != mac.parameters.len() {
                            return Err(line.error(format!(
                                "macro `{op}` expects {} arguments, got {}",
                                mac.parameters.len(),
                                values.len()
                            )));
                        }
                        let invocation = self.invocations;
                        self.invocations += 1;
                        let mut parameters = mac.parameters.clone();
                        let mut values = values;
                        iteration_bindings(&mac.body, &mut parameters, &mut values);
                        let mut body_bytes = 0usize;
                        let body = mac
                            .body
                            .iter()
                            .map(|body| {
                                let text =
                                    substitute(body, &parameters, &values, invocation, self.gnu)?;
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
                        let body = self.rescan(body)?;
                        self.run(&body, depth + 1)?;
                    } else if self.gnu && lower == ".purgem" {
                        if self.macros.remove(args).is_none() {
                            return Err(line.error("undefined macro in .purgem"));
                        }
                    } else {
                        let line = if self.gnu {
                            self.gnu_line(line)?
                        } else {
                            line.clone()
                        };
                        self.emit(&line)?;
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
            ".rept" | ".irp" | ".irpc" => stack.push("rept"),
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

fn substitute(
    line: &Line,
    parameters: &[String],
    values: &[String],
    invocation: usize,
    gnu: bool,
) -> Result<String, Error> {
    if !gnu
        && line
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
        if let Some(tail) = rest.strip_prefix('@') {
            result.push_str(&invocation.to_string());
            rest = tail;
            continue;
        }
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if end == 0 {
            // Preserve string escapes such as \" and \\. Unsupported macro
            // operators must fail instead of being silently reinterpreted.
            if rest.starts_with('+') {
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

// Quoted strings and relocation names are opaque. Parentheses protect negative
// values and preserve precedence when a named absolute constant is substituted.
fn replace_constants(
    text: &str,
    constants: &BTreeMap<String, i128>,
    immediates_only: bool,
) -> String {
    let mut out = String::new();
    let mut chars = text.char_indices().peekable();
    let (mut quoted, mut escaped) = (false, false);
    let (mut immediate, mut depth) = (false, 0usize);
    while let Some((start, c)) = chars.next() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            escaped = true;
            out.push(c);
            continue;
        }
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted {
            if c == '$' {
                immediate = true;
            }
            if c == '(' {
                depth += 1;
            }
            if c == ')' {
                depth = depth.saturating_sub(1);
            }
            if c == ',' && depth == 0 {
                immediate = false;
            }
        }
        if !quoted && (c.is_ascii_alphanumeric() || "_.$".contains(c)) {
            let mut end = start + c.len_utf8();
            while let Some(&(i, c)) = chars.peek() {
                if !(c.is_ascii_alphanumeric() || "_.$".contains(c)) {
                    break;
                }
                chars.next();
                end = i + c.len_utf8();
            }
            let name = &text[start..end];
            if (immediates_only && !immediate) || text.as_bytes().get(end) == Some(&b'@') {
                out.push_str(name);
            } else if let Some(n) = constants.get(name) {
                out.push_str(&format!("({n})"));
            } else if let Some(n) = name.strip_prefix('$').and_then(|name| constants.get(name)) {
                out.push_str(&format!("$({n})"));
            } else {
                out.push_str(name);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn iteration_bindings(body: &[Line], parameters: &mut Vec<String>, values: &mut Vec<String>) {
    for nested in body {
        let (op, args) = parts(&nested.text);
        if matches!(op, ".irp" | ".irpc") {
            let name = args.split(',').next().unwrap().trim();
            if !parameters.iter().any(|p| p == name) {
                parameters.push(name.into());
                values.push(format!("\\{name}"));
            }
        }
    }
}
