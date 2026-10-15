//! Token-based C preprocessing using Keel's target environment and headers.

use crate::Diagnostic;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct PreprocessOptions {
    /// Searched after the including file's directory, in order.
    pub include_dirs: Vec<PathBuf>,
    /// Explicit system search directories, after -I and before compiler headers.
    pub system_include_dirs: Vec<PathBuf>,
    /// Root of the target's SDK/filesystem. User -I/-isystem paths are not rebased.
    pub sysroot: Option<PathBuf>,
    /// macOS minimum deployment version, independent of SDK version (native default 11.0).
    pub darwin_deployment: Option<String>,
    /// Definitions/undefinitions applied in order, after predefined macros.
    pub defines: Vec<(String, Option<String>)>,
    pub target: keel_ir::Target,
    /// Return the final macro definitions instead of preprocessed source (-dM).
    pub dump_macros: bool,
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    space: bool,
    raw_spelling: Option<String>,
    file: String,
    line: u32,
    col: u32,
    hide: HashSet<String>,
}
impl Token {
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            file: self.file.clone(),
            line: self.line,
            col: self.col,
            message: message.into(),
        }
    }
    fn with_text(&self, text: impl Into<String>) -> Self {
        let mut t = self.clone();
        t.text = text.into();
        t.raw_spelling = None;
        t
    }
}

#[derive(Clone)]
struct Macro {
    params: Option<Vec<String>>,
    variadic: bool,
    body: Vec<Token>,
}

/// Preprocess raw C to text accepted by [`crate::compile`], including line
/// markers and pragmas. With default options this needs no external compiler.
pub fn preprocess(
    src: &str,
    path: &str,
    options: &PreprocessOptions,
) -> Result<String, Diagnostic> {
    let mut pp = Preprocessor {
        options,
        in_condition: false,
        macros: HashMap::new(),
        undefined: HashSet::new(),
        system_dirs: Vec::new(),
        framework_dirs: Vec::new(),
        include_stack: Vec::new(),
        once: HashSet::new(),
        counter: 0,
        output: String::new(),
    };
    let predefined = crate::predefined_macros(options.target, options.darwin_deployment.as_deref())
        .map_err(|e| pp.error(e))?;
    for line in predefined.lines() {
        pp.define(&scan(line.strip_prefix("#define ").unwrap(), "<built-in>")?)?;
    }
    pp.native_paths();
    for (name, value) in &options.defines {
        if let Some(value) = value {
            let definition = format!("{name} {value}");
            pp.define(&scan(&definition, "<command line>")?)?;
        } else {
            if !ident(name) {
                return Err(pp.error("expected macro name after -U"));
            }
            pp.macros.remove(name);
            pp.undefined.insert(name.clone());
        }
    }
    pp.file(src, path, 0, None)?;
    if options.dump_macros {
        Ok(pp.macro_dump())
    } else {
        Ok(pp.output)
    }
}

struct Preprocessor<'a> {
    options: &'a PreprocessOptions,
    in_condition: bool,
    macros: HashMap<String, Macro>,
    undefined: HashSet<String>,
    system_dirs: Vec<PathBuf>,
    framework_dirs: Vec<PathBuf>,
    include_stack: Vec<(String, Option<usize>)>,
    once: HashSet<PathBuf>,
    counter: u64,
    output: String,
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
fn ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && c.all(|c| c == '_' || c.is_alphanumeric())
}

/// Phase 2 splicing precedes phase 3 comments and preprocessing tokens.
fn scan(src: &str, file: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = src.as_bytes();
    let mut input = Vec::new();
    let (mut i, mut line, mut col) = (0, 1, 1);
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && (bytes.get(i + 1) == Some(&b'\n') || bytes.get(i + 1..i + 3) == Some(b"\r\n"))
        {
            i += if bytes[i + 1] == b'\r' { 3 } else { 2 };
            line += 1;
            col = 1;
        } else {
            input.push((bytes[i], line, col));
            if bytes[i] == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }
    }
    let b: Vec<u8> = input.iter().map(|x| x.0).collect();
    let mut out = Vec::new();
    let (mut i, mut space) = (0, false);
    while i < b.len() {
        let start = i;
        let token = |text: String, space| Token {
            text,
            space,
            raw_spelling: None,
            file: file.into(),
            line: input[start].1,
            col: input[start].2,
            hide: HashSet::new(),
        };
        if b[i] == b'\n' {
            out.push(token("\n".into(), true));
            i += 1;
            space = true;
            continue;
        }
        if b[i].is_ascii_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        if b.get(i..i + 2) == Some(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            space = true;
            continue;
        }
        if b.get(i..i + 2) == Some(b"/*") {
            i += 2;
            while i + 1 < b.len() && &b[i..i + 2] != b"*/" {
                if b[i] == b'\n' {
                    out.push(Token {
                        line: input[i].1,
                        col: input[i].2,
                        ..token("\n".into(), true)
                    });
                }
                i += 1;
            }
            if i + 1 >= b.len() {
                return Err(token(String::new(), false).error("unterminated comment"));
            }
            i += 2;
            space = true;
            continue;
        }
        let prefix = if b.get(i..i + 2) == Some(b"u8") && matches!(b.get(i + 2), Some(b'"' | b'\''))
        {
            2
        } else if matches!(b[i], b'L' | b'u' | b'U') && matches!(b.get(i + 1), Some(b'"' | b'\'')) {
            1
        } else {
            0
        };
        if matches!(b.get(i + prefix), Some(b'"' | b'\'')) {
            i += prefix;
            let q = b[i];
            i += 1;
            while i < b.len() && b[i] != q && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if b.get(i) != Some(&q) {
                return Err(token(String::new(), false).error("unterminated literal"));
            }
            i += 1;
        } else if b[i].is_ascii_digit()
            || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i += 1;
            while i < b.len() {
                if b[i].is_ascii_alphanumeric()
                    || b[i] == b'_'
                    || b[i] == b'.'
                    || b[i] >= 128
                    || (matches!(b[i], b'+' | b'-')
                        && matches!(b[i - 1], b'e' | b'E' | b'p' | b'P'))
                {
                    i += 1;
                } else {
                    break;
                }
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' || b[i] >= 128 {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 128) {
                i += 1;
            }
        } else {
            const P: &[&str] = &[
                "%:%:", "<<=", ">>=", "...", "##", "->", "++", "--", "<<", ">>", "<=", ">=", "==",
                "!=", "&&", "||", "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", "<:", ":>", "<%",
                "%>", "%:",
            ];
            i += P
                .iter()
                .find(|p| b[start..].starts_with(p.as_bytes()))
                .map_or(1, |p| p.len());
        }
        let text = String::from_utf8(b[start..i].to_vec())
            .map_err(|_| token(String::new(), false).error("invalid UTF-8 token"))?;
        let text = match text.as_str() {
            "%:%:" => "##",
            "%:" => "#",
            "<:" => "[",
            ":>" => "]",
            "<%" => "{",
            "%>" => "}",
            _ => &text,
        }
        .to_string();
        let raw = String::from_utf8(b[start..i].to_vec()).unwrap();
        let mut t = token(text, space);
        if t.text != raw {
            t.raw_spelling = Some(raw);
        }
        out.push(t);
        space = false;
    }
    Ok(out)
}

impl Preprocessor<'_> {
    fn macro_dump(&self) -> String {
        let mut names: Vec<_> = self.macros.keys().collect();
        names.sort();
        let mut out = String::new();
        for name in names {
            let m = &self.macros[name];
            out.push_str("#define ");
            out.push_str(name);
            if let Some(params) = &m.params {
                let mut params = params.clone();
                if m.variadic {
                    let last = params.last_mut().unwrap();
                    if last == "__VA_ARGS__" {
                        *last = "...".into();
                    } else {
                        last.push_str("...");
                    }
                }
                out.push('(');
                out.push_str(&params.join(","));
                out.push(')');
            }
            out.push(' ');
            out.push_str(&spelling(&m.body));
            out.push('\n');
        }
        out
    }
    fn native_paths(&mut self) {
        // With an explicit root, never consult host system include directories.
        let root = self.options.sysroot.clone().or_else(|| {
            (self.options.target == keel_ir::Target::X86_64LinuxGnu
                && keel_ir::Target::host() == Some(self.options.target))
            .then(|| PathBuf::from("/"))
        });
        let Some(root) = root else {
            return;
        };
        if self.options.target == keel_ir::Target::X86_64LinuxGnu {
            self.system_dirs
                .push(root.join("usr/include/x86_64-linux-gnu"));
        }
        self.system_dirs.push(root.join("usr/include"));
        if self.options.target == keel_ir::Target::Aarch64AppleDarwin {
            self.framework_dirs
                .push(root.join("System/Library/Frameworks"));
        }
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            file: "<preprocessor>".into(),
            line: 1,
            col: 1,
            message: message.into(),
        }
    }
    fn define(&mut self, ts: &[Token]) -> Result<(), Diagnostic> {
        let Some(name) = ts.first() else {
            return Err(self.error("expected macro name"));
        };
        if !ident(&name.text) {
            return Err(name.error("expected macro name"));
        }
        let mut i = 1;
        let mut params = None;
        let mut variadic = false;
        if ts.get(i).is_some_and(|t| t.text == "(" && !t.space) {
            i += 1;
            let mut names = Vec::new();
            while ts.get(i).is_some_and(|t| t.text != ")") {
                let p = &ts[i];
                if p.text == "..." {
                    variadic = true;
                    names.push("__VA_ARGS__".into());
                    i += 1;
                } else if ident(&p.text) && !names.contains(&p.text) {
                    names.push(p.text.clone());
                    i += 1;
                    if ts.get(i).is_some_and(|t| t.text == "...") {
                        variadic = true;
                        i += 1;
                    }
                } else {
                    return Err(p.error("invalid or duplicate macro parameter"));
                }
                if variadic || ts.get(i).is_some_and(|t| t.text == ")") {
                    break;
                }
                if !ts.get(i).is_some_and(|t| t.text == ",") {
                    return Err(p.error("expected ',' in macro parameters"));
                }
                i += 1;
                if ts.get(i).is_some_and(|t| t.text == ")") {
                    return Err(p.error("expected macro parameter after ','"));
                }
            }
            if !ts.get(i).is_some_and(|t| t.text == ")") {
                return Err(name.error("unterminated macro parameters"));
            }
            i += 1;
            params = Some(names);
        }
        let body = ts[i..].to_vec();
        if body.first().is_some_and(|t| t.text == "##")
            || body.last().is_some_and(|t| t.text == "##")
        {
            return Err(name.error("'##' cannot appear at either end of a macro"));
        }
        if let Some(params) = &params {
            for (i, t) in body.iter().enumerate() {
                if t.text == "#" && !body.get(i + 1).is_some_and(|t| params.contains(&t.text)) {
                    return Err(t.error("'#' must precede a macro parameter"));
                }
            }
        }
        self.macros.insert(
            name.text.clone(),
            Macro {
                params,
                variadic,
                body,
            },
        );
        Ok(())
    }

    fn emit(&mut self, ts: &[Token]) {
        let mut prev: Option<(&str, u32)> = None;
        for t in ts {
            if t.text.is_empty() {
                continue;
            }
            if prev != Some((&t.file, t.line)) {
                if prev.is_some() {
                    self.output.push('\n');
                }
                self.output
                    .push_str(&format!("# {} {}\n", t.line, quote(&t.file)));
                prev = Some((&t.file, t.line));
            }
            self.output.push_str(&t.text);
            self.output.push(' ');
            if t.text.contains('\n') {
                prev = None;
            }
        }
        if prev.is_some() {
            self.output.push('\n');
        }
    }
    fn flush(&mut self, pending: &mut Vec<Token>) -> Result<(), Diagnostic> {
        let expanded = self.expand(std::mem::take(pending), 0)?;
        self.emit(&expanded);
        Ok(())
    }
    fn file(
        &mut self,
        src: &str,
        path: &str,
        depth: usize,
        origin: Option<usize>,
    ) -> Result<(), Diagnostic> {
        if depth > 200 {
            return Err(self.error("include nesting exceeds 200"));
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        if self.once.contains(&canonical) {
            return Ok(());
        }
        self.include_stack.push((path.into(), origin));
        let ts = scan(src, path)?;
        let mut pending = Vec::new();
        let mut stack: Vec<Conditional> = Vec::new();
        let mut logical_file = path.to_string();
        let mut line_delta: i64 = 0;
        for raw_line in ts.split(|t| t.text == "\n") {
            let mut line = raw_line.to_vec();
            for t in &mut line {
                t.file.clone_from(&logical_file);
                t.line = (t.line as i64 + line_delta) as u32;
            }
            let active = stack.last().is_none_or(|c| c.active);
            if !line.first().is_some_and(|t| t.text == "#") {
                if active {
                    if let Some(first) = line.first_mut() {
                        first.space = true;
                    }
                    pending.extend(line);
                }
                continue;
            }
            self.flush(&mut pending)?;
            let Some(directive) = line.get(1) else {
                continue;
            };
            let args = &line[2..];
            match directive.text.as_str() {
                "if" | "ifdef" | "ifndef" => {
                    let yes = if !active {
                        false
                    } else if directive.text == "if" {
                        self.condition(args)?
                    } else {
                        if args.len() != 1 || !ident(&args[0].text) {
                            return Err(directive.error("expected one macro name"));
                        }
                        self.is_defined(&args[0].text) != (directive.text == "ifndef")
                    };
                    stack.push(Conditional {
                        parent: active,
                        active: active && yes,
                        taken: yes,
                        otherwise: false,
                        start: directive.clone(),
                    });
                }
                "elif" | "else" => {
                    let Some(c) = stack.last_mut() else {
                        return Err(directive.error("conditional without #if"));
                    };
                    if c.otherwise {
                        return Err(directive.error("conditional after #else"));
                    }
                    let yes = if directive.text == "else" {
                        c.otherwise = true;
                        true
                    } else if c.parent && !c.taken {
                        self.condition(args)?
                    } else {
                        false
                    };
                    c.active = c.parent && !c.taken && yes;
                    c.taken |= c.active;
                }
                "endif" => {
                    if stack.pop().is_none() {
                        return Err(directive.error("#endif without #if"));
                    }
                }
                _ if !active => {}
                "define" => self.define(args)?,
                "undef" => {
                    if args.len() != 1 || !ident(&args[0].text) {
                        return Err(directive.error("expected one macro name"));
                    }
                    self.macros.remove(&args[0].text);
                    self.undefined.insert(args[0].text.clone());
                }
                "include" | "include_next" => {
                    let expanded = if args
                        .first()
                        .is_some_and(|t| t.text == "<" || t.text.starts_with('"'))
                    {
                        args.to_vec()
                    } else {
                        self.expand(args.to_vec(), 0)?
                    };
                    let (name, quoted) =
                        header(&expanded).ok_or_else(|| directive.error("expected header name"))?;
                    let include = self
                        .find_header(&name, quoted, directive.text == "include_next")
                        .ok_or_else(|| directive.error(format!("header not found: {name}")))?;
                    let contents = match include.contents {
                        Some(contents) => contents.to_string(),
                        None => std::fs::read_to_string(&include.path).map_err(|e| {
                            directive.error(format!("{}: {e}", include.path.display()))
                        })?,
                    };
                    self.file(
                        &contents,
                        &include.path.to_string_lossy(),
                        depth + 1,
                        include.origin,
                    )?;
                }
                "pragma" => {
                    if args.first().is_some_and(|t| t.text == "once") {
                        self.once.insert(canonical.clone());
                    } else {
                        self.output.push_str("#pragma ");
                        self.output.push_str(&spelling(args));
                        self.output.push('\n');
                    }
                }
                "line" => {
                    let expanded = self.expand(args.to_vec(), 0)?;
                    let n = expanded
                        .first()
                        .and_then(|t| t.text.parse::<u32>().ok())
                        .filter(|&n| n > 0)
                        .ok_or_else(|| directive.error("invalid #line number"))?;
                    // The directive's physical line, before any previous #line adjustment.
                    line_delta = n as i64 - raw_line[1].line as i64 - 1;
                    if let Some(t) = expanded.get(1) {
                        logical_file =
                            unquote(&t.text).ok_or_else(|| t.error("expected #line file name"))?;
                    }
                }
                "error" => return Err(directive.error(spelling(args))),
                "warning" => {} // Warnings are currently non-fatal; no warning sink yet.
                _ => {
                    return Err(directive.error(format!(
                        "unsupported preprocessing directive #{}",
                        directive.text
                    )));
                }
            }
        }
        if let Some(c) = stack.last() {
            return Err(c.start.error("unterminated conditional directive"));
        }
        self.flush(&mut pending)?;
        self.include_stack.pop();
        Ok(())
    }
    fn find_header(&self, name: &str, quoted: bool, next: bool) -> Option<Include> {
        let (path, origin) = self.include_stack.last()?;
        let mut candidates = Vec::new();
        if quoted && !next {
            candidates.push(Include {
                path: Path::new(path)
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(name),
                origin: None,
                contents: None,
            });
        }
        let mut dirs = self.options.include_dirs.clone();
        dirs.extend(self.options.system_include_dirs.iter().cloned());
        let builtin_index = dirs.len();
        dirs.push(crate::headers::ROOT.into());
        dirs.extend(self.system_dirs.iter().cloned());
        for (i, dir) in dirs.iter().enumerate() {
            if next && origin.is_some_and(|origin| i <= origin) {
                continue;
            }
            let contents = if i == builtin_index {
                crate::headers::get(Path::new(name))
            } else {
                None
            };
            candidates.push(Include {
                path: dir.join(name),
                origin: Some(i),
                contents,
            });
        }
        if let Some((framework, rest)) = name.split_once('/') {
            for (i, dir) in self.framework_dirs.iter().enumerate() {
                let i = i + dirs.len();
                if !next || origin.is_none_or(|origin| i > origin) {
                    candidates.push(Include {
                        path: dir.join(format!("{framework}.framework/Headers/{rest}")),
                        origin: Some(i),
                        contents: None,
                    });
                }
            }
        }
        candidates
            .into_iter()
            .find(|include| include.contents.is_some() || include.path.is_file())
    }
    fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
            || (!self.undefined.contains(name)
                && (matches!(
                    name,
                    "__LINE__"
                        | "__FILE__"
                        | "__BASE_FILE__"
                        | "__FILE_NAME__"
                        | "__INCLUDE_LEVEL__"
                        | "__COUNTER__"
                        | "_Pragma"
                        | "__has_include"
                        | "__has_include_next"
                ) || query_builtin(name)))
    }
}
struct Include {
    path: PathBuf,
    origin: Option<usize>,
    contents: Option<&'static str>,
}
struct Conditional {
    parent: bool,
    active: bool,
    taken: bool,
    otherwise: bool,
    start: Token,
}
fn spelling(ts: &[Token]) -> String {
    let mut s = String::new();
    for t in ts {
        if t.space && !s.is_empty() {
            s.push(' ');
        }
        s.push_str(t.raw_spelling.as_deref().unwrap_or(&t.text));
    }
    s
}
fn unquote(s: &str) -> Option<String> {
    let s = s.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' { chars.next()? } else { c });
    }
    Some(out)
}
fn header(ts: &[Token]) -> Option<(String, bool)> {
    if ts.len() == 1 {
        return unquote(&ts[0].text).map(|s| (s, true));
    }
    if ts.first()?.text == "<" && ts.last()?.text == ">" {
        return Some((
            ts[1..ts.len() - 1]
                .iter()
                .map(|t| t.text.as_str())
                .collect(),
            false,
        ));
    }
    None
}
fn query_builtin(name: &str) -> bool {
    matches!(
        name,
        "__building_module"
            | "__has_builtin"
            | "__has_feature"
            | "__has_extension"
            | "__has_attribute"
            | "__has_c_attribute"
            | "__has_declspec_attribute"
            | "__is_identifier"
            | "__is_target_arch"
            | "__is_target_vendor"
            | "__is_target_os"
            | "__is_target_environment"
            | "__is_target_variant_os"
            | "__is_target_variant_environment"
    )
}

/// Returns arguments and the index immediately after the closing parenthesis.
fn arguments(ts: &[Token], open: usize) -> Result<(Vec<Vec<Token>>, usize), Diagnostic> {
    let mut args = vec![Vec::new()];
    let mut depth = 0;
    for (i, t) in ts.iter().enumerate().skip(open + 1) {
        match t.text.as_str() {
            "(" => depth += 1,
            ")" if depth == 0 => return Ok((args, i + 1)),
            ")" => depth -= 1,
            "," if depth == 0 => {
                args.push(Vec::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().unwrap().push(t.clone());
    }
    Err(ts[open].error("unterminated macro invocation"))
}

impl Preprocessor<'_> {
    fn expand(&mut self, mut ts: Vec<Token>, depth: usize) -> Result<Vec<Token>, Diagnostic> {
        if depth > 200 {
            return Err(self.error("macro argument nesting exceeds 200"));
        }
        let mut i = 0;
        let mut steps = 0;
        while i < ts.len() {
            steps += 1;
            if steps > 1_000_000 || ts.len() > 1_000_000 {
                return Err(ts[i].error("macro expansion limit exceeded"));
            }
            let t = ts[i].clone();
            let name = t.text.as_str();
            if name == "defined" && self.in_condition {
                let mut end = i + 1;
                let paren = ts.get(end).is_some_and(|t| t.text == "(");
                if paren {
                    end += 1;
                }
                let operand = ts
                    .get(end)
                    .filter(|t| ident(&t.text))
                    .ok_or_else(|| t.error("expected identifier after defined"))?;
                let value = self.is_defined(&operand.text);
                end += 1;
                if paren {
                    if !ts.get(end).is_some_and(|t| t.text == ")") {
                        return Err(t.error("expected ')' after defined"));
                    }
                    end += 1;
                }
                ts.splice(i..end, [t.with_text((value as u8).to_string())]);
                i += 1;
                continue;
            }
            if t.hide.contains(name) {
                i += 1;
                continue;
            }
            let builtin_enabled = !self.macros.contains_key(name) && !self.undefined.contains(name);
            let builtin = if builtin_enabled {
                match name {
                    "__LINE__" => Some(t.line.to_string()),
                    "__FILE__" => Some(quote(&t.file)),
                    "__BASE_FILE__" => Some(quote(&self.include_stack[0].0)),
                    "__FILE_NAME__" => Some(quote(
                        Path::new(&t.file)
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or(""),
                    )),
                    "__INCLUDE_LEVEL__" => Some((self.include_stack.len() - 1).to_string()),
                    "__COUNTER__" => {
                        let n = self.counter;
                        self.counter += 1;
                        Some(n.to_string())
                    }
                    _ => None,
                }
            } else {
                None
            };
            if let Some(value) = builtin {
                ts[i].text = value;
                i += 1;
                continue;
            }
            if (matches!(name, "__has_include" | "__has_include_next") || query_builtin(name))
                && builtin_enabled
            {
                if !ts.get(i + 1).is_some_and(|t| t.text == "(") {
                    return Err(t.error("expected '(' after preprocessing builtin"));
                }
                let (args, end) = arguments(&ts, i + 1)?;
                if args.len() != 1 {
                    return Err(t.error("expected one argument to preprocessing builtin"));
                }
                let value = if matches!(name, "__has_include" | "__has_include_next") {
                    let expanded = self.expand(args[0].clone(), depth + 1)?;
                    let (h, q) = header(&expanded)
                        .ok_or_else(|| t.error("expected header name in __has_include"))?;
                    (self
                        .find_header(&h, q, name == "__has_include_next")
                        .is_some() as u8)
                        .to_string()
                } else {
                    if args[0].len() != 1 || !ident(&args[0][0].text) {
                        return Err(t.error("expected identifier in feature query"));
                    }
                    crate::features::query(name, &args[0][0].text, self.options.target).to_string()
                };
                ts.splice(i..end, [t.with_text(value)]);
                i += 1;
                continue;
            }
            if name == "_Pragma" && builtin_enabled {
                if !ts.get(i + 1).is_some_and(|t| t.text == "(") {
                    return Err(t.error("expected '(' after _Pragma"));
                }
                let (args, end) = arguments(&ts, i + 1)?;
                if args.len() != 1 {
                    return Err(t.error("expected one _Pragma argument"));
                }
                let expanded = self.expand(args[0].clone(), depth + 1)?;
                let value = if expanded.len() == 1 {
                    unquote(&expanded[0].text)
                } else {
                    None
                }
                .ok_or_else(|| t.error("expected string literal in _Pragma"))?;
                // Keep pragmas in the output stream, in token order.
                ts.splice(i..end, [t.with_text(format!("\n#pragma {value}\n"))]);
                i += 1;
                continue;
            }
            let Some(m) = self.macros.get(name).cloned() else {
                i += 1;
                continue;
            };
            let (end, mut replacement, hide) = if let Some(params) = &m.params {
                if !ts.get(i + 1).is_some_and(|t| t.text == "(") {
                    i += 1;
                    continue;
                }
                let (mut args, end) = arguments(&ts, i + 1)?;
                if params.is_empty() && args.len() == 1 && args[0].is_empty() {
                    args.clear();
                }
                let fixed = params.len() - usize::from(m.variadic);
                let omitted = m.variadic && args.len() <= fixed;
                if args.len() < fixed || (!m.variadic && args.len() != fixed) {
                    return Err(t.error(format!("wrong number of arguments to macro {name}")));
                }
                if m.variadic {
                    let extras = args.split_off(fixed);
                    let mut rest = Vec::new();
                    for (n, arg) in extras.into_iter().enumerate() {
                        if n > 0 {
                            rest.push(t.with_text(","));
                        }
                        rest.extend(arg);
                    }
                    args.push(rest);
                }
                let mut expanded = HashMap::new();
                let mut body = Vec::new();
                let mut j = 0;
                while j < m.body.len() {
                    let b = &m.body[j];
                    if b.text == "#" {
                        let p = params
                            .iter()
                            .position(|p| p == &m.body[j + 1].text)
                            .unwrap();
                        body.push((b.with_text(quote(&spelling(&args[p]))), false));
                        j += 2;
                        continue;
                    }
                    // GNU comma elision: only a missing variadic argument
                    // removes the comma; an explicitly empty one preserves it.
                    if m.variadic
                        && b.text == ","
                        && m.body.get(j + 1).is_some_and(|t| t.text == "##")
                        && m.body
                            .get(j + 2)
                            .is_some_and(|t| Some(&t.text) == params.last())
                    {
                        if !omitted {
                            body.push((b.clone(), false));
                            body.extend(
                                self.expand(args.last().unwrap().clone(), depth + 1)?
                                    .into_iter()
                                    .map(|t| (t, false)),
                            );
                        }
                        j += 3;
                        continue;
                    }
                    if let Some(p) = params.iter().position(|p| p == &b.text) {
                        let paste = (j > 0 && m.body[j - 1].text == "##")
                            || m.body.get(j + 1).is_some_and(|t| t.text == "##");
                        let mut value = if paste {
                            args[p].clone()
                        } else {
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                expanded.entry(p)
                            {
                                entry.insert(self.expand(args[p].clone(), depth + 1)?);
                            }
                            expanded[&p].clone()
                        };
                        if value.is_empty() && paste {
                            value.push(b.with_text(""));
                        }
                        if let Some(first) = value.first_mut() {
                            first.space = b.space;
                        }
                        // Argument tokens spelling ## are ordinary tokens,
                        // never operators in this replacement list.
                        body.extend(value.into_iter().map(|t| (t, false)));
                    } else {
                        body.push((b.clone(), b.text == "##"));
                    }
                    j += 1;
                }
                let hide = t
                    .hide
                    .intersection(&ts[end - 1].hide)
                    .cloned()
                    .collect::<HashSet<_>>();
                (end, self.paste(body)?, hide)
            } else {
                (
                    i + 1,
                    self.paste(m.body.iter().map(|t| (t.clone(), t.text == "##")).collect())?,
                    t.hide.clone(),
                )
            };
            for r in &mut replacement {
                r.hide.extend(hide.iter().cloned());
                r.hide.insert(name.into());
                r.file.clone_from(&t.file);
                r.line = t.line;
                r.col = t.col;
            }
            if let Some(first) = replacement.first_mut() {
                first.space = t.space;
            }
            ts.splice(i..end, replacement);
        }
        Ok(ts)
    }
    fn paste(&self, body: Vec<(Token, bool)>) -> Result<Vec<Token>, Diagnostic> {
        let mut out: Vec<Token> = Vec::new();
        let mut body = body.into_iter();
        while let Some((t, operator)) = body.next() {
            if operator {
                let left = out
                    .pop()
                    .ok_or_else(|| t.error("missing left operand to ##"))?;
                let (right, _) = body
                    .next()
                    .ok_or_else(|| t.error("missing right operand to ##"))?;
                let text = format!(
                    "{}{}",
                    left.raw_spelling.as_deref().unwrap_or(&left.text),
                    right.raw_spelling.as_deref().unwrap_or(&right.text)
                );
                let mut result = left.with_text(text.clone());
                if !text.is_empty() {
                    let check = scan(&text, &t.file)?;
                    if check.len() != 1
                        || check[0].raw_spelling.as_deref().unwrap_or(&check[0].text) != text
                    {
                        return Err(t.error(format!(
                            "token paste does not form one preprocessing token: {text}"
                        )));
                    }
                    result.text.clone_from(&check[0].text);
                    result.raw_spelling.clone_from(&check[0].raw_spelling);
                }
                result.hide = left.hide.intersection(&right.hide).cloned().collect();
                out.push(result);
            } else {
                out.push(t);
            }
        }
        out.retain(|t| !t.text.is_empty());
        Ok(out)
    }
    fn condition(&mut self, ts: &[Token]) -> Result<bool, Diagnostic> {
        if ts.is_empty() {
            return Err(self.error("expected #if expression"));
        }
        self.in_condition = true;
        let expanded = self.expand(ts.to_vec(), 0);
        self.in_condition = false;
        let mut tokens = expanded?;
        for t in &mut tokens {
            if ident(&t.text) {
                t.text = "0".into();
            }
        }
        if tokens.is_empty() {
            return Err(ts[0].error("expected #if expression after expansion"));
        }
        let mut expr = Expression {
            ts: &tokens,
            pos: 0,
        };
        let value = expr.eval(0, true)?;
        if expr.pos != tokens.len() {
            return Err(tokens[expr.pos].error("unexpected token in #if expression"));
        }
        Ok(value.bits != 0)
    }
}

#[derive(Clone, Copy)]
struct Integer {
    bits: u64,
    unsigned: bool,
}
impl Integer {
    fn boolean(v: bool) -> Self {
        Self {
            bits: v as u64,
            unsigned: false,
        }
    }
    fn negative(self) -> bool {
        !self.unsigned && (self.bits as i64) < 0
    }
}
struct Expression<'a> {
    ts: &'a [Token],
    pos: usize,
}
impl Expression<'_> {
    fn eval(&mut self, min: u8, live: bool) -> Result<Integer, Diagnostic> {
        let Some(t) = self.ts.get(self.pos) else {
            return Err(self
                .ts
                .last()
                .unwrap()
                .error("expected operand in #if expression"));
        };
        self.pos += 1;
        let mut lhs = match t.text.as_str() {
            "(" => {
                let v = self.eval(0, live)?;
                self.expect(")", t)?;
                v
            }
            "+" | "-" | "!" | "~" => {
                let v = self.eval(13, live)?;
                match t.text.as_str() {
                    "+" => v,
                    "-" => Integer {
                        bits: v.bits.wrapping_neg(),
                        ..v
                    },
                    "~" => Integer { bits: !v.bits, ..v },
                    _ => Integer::boolean(v.bits == 0),
                }
            }
            _ => integer(t)?,
        };
        while let Some(op) = self.ts.get(self.pos) {
            if op.text == "?" && min <= 1 {
                self.pos += 1;
                let yes = self.eval(0, live && lhs.bits != 0)?;
                self.expect(":", op)?;
                let no = self.eval(1, live && lhs.bits == 0)?;
                let bits = if lhs.bits != 0 { yes.bits } else { no.bits };
                lhs = Integer {
                    bits,
                    unsigned: yes.unsigned || no.unsigned,
                };
                continue;
            }
            let prec = match op.text.as_str() {
                "||" => 2,
                "&&" => 3,
                "|" => 4,
                "^" => 5,
                "&" => 6,
                "==" | "!=" => 7,
                "<" | "<=" | ">" | ">=" => 8,
                "<<" | ">>" => 9,
                "+" | "-" => 10,
                "*" | "/" | "%" => 11,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.pos += 1;
            let rhs = self.eval(
                prec + 1,
                live && match op.text.as_str() {
                    "&&" => lhs.bits != 0,
                    "||" => lhs.bits == 0,
                    _ => true,
                },
            )?;
            let (a, b) = (lhs.bits, rhs.bits);
            let unsigned = lhs.unsigned || rhs.unsigned;
            let less = if unsigned {
                a < b
            } else {
                (a as i64) < (b as i64)
            };
            let value = match op.text.as_str() {
                "||" => Integer::boolean(a != 0 || b != 0),
                "&&" => Integer::boolean(a != 0 && b != 0),
                "==" => Integer::boolean(a == b),
                "!=" => Integer::boolean(a != b),
                "<" => Integer::boolean(less),
                ">" => Integer::boolean(!less && a != b),
                "<=" => Integer::boolean(less || a == b),
                ">=" => Integer::boolean(!less),
                _ => {
                    let bits = match op.text.as_str() {
                        "|" => a | b,
                        "^" => a ^ b,
                        "&" => a & b,
                        "+" => a.wrapping_add(b),
                        "-" => a.wrapping_sub(b),
                        "*" => a.wrapping_mul(b),
                        "<<" | ">>" => {
                            if live && (rhs.negative() || b >= 64) {
                                return Err(op.error("invalid shift count in #if"));
                            }
                            if op.text == "<<" {
                                a.wrapping_shl(b as u32)
                            } else if lhs.unsigned {
                                a.wrapping_shr(b as u32)
                            } else {
                                (a as i64).wrapping_shr(b as u32) as u64
                            }
                        }
                        "/" | "%" => {
                            if b == 0 {
                                if live {
                                    return Err(op.error("division by zero in #if"));
                                }
                                0
                            } else if unsigned {
                                if op.text == "/" { a / b } else { a % b }
                            } else if op.text == "/" {
                                (a as i64).wrapping_div(b as i64) as u64
                            } else {
                                (a as i64).wrapping_rem(b as i64) as u64
                            }
                        }
                        _ => unreachable!(),
                    };
                    Integer {
                        bits,
                        unsigned: if matches!(op.text.as_str(), "<<" | ">>") {
                            lhs.unsigned
                        } else {
                            unsigned
                        },
                    }
                }
            };
            lhs = value;
        }
        Ok(lhs)
    }
    fn expect(&mut self, expected: &str, at: &Token) -> Result<(), Diagnostic> {
        if self.ts.get(self.pos).is_some_and(|t| t.text == expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(at.error(format!("expected '{expected}' in #if expression")))
        }
    }
}
fn integer(t: &Token) -> Result<Integer, Diagnostic> {
    if t.text.contains('\'') {
        let mut files = Vec::new();
        let tokens =
            crate::lex::tokenize(&t.text, &t.file, &mut files).map_err(|e| t.error(e.message))?;
        if let crate::lex::Tok::Char(value) = tokens[0].tok {
            return Ok(Integer {
                bits: value as u64,
                unsigned: false,
            });
        }
        return Err(t.error("invalid character constant in #if"));
    }
    let s = &t.text;
    let (radix, start) = if s.starts_with("0x") || s.starts_with("0X") {
        (16, 2)
    } else if s.starts_with("0b") || s.starts_with("0B") {
        (2, 2)
    } else if s.starts_with('0') && s.len() > 1 {
        (8, 0)
    } else {
        (10, 0)
    };
    let end = s[start..]
        .find(|c: char| !c.is_digit(radix))
        .map_or(s.len(), |n| n + start);
    let suffix = s[end..].to_ascii_lowercase();
    if !matches!(
        suffix.as_str(),
        "" | "u" | "l" | "ul" | "lu" | "ll" | "ull" | "llu"
    ) {
        return Err(t.error("invalid integer constant in #if"));
    }
    let bits = u64::from_str_radix(&s[start..end], radix)
        .map_err(|_| t.error("invalid integer constant in #if"))?;
    Ok(Integer {
        bits,
        unsigned: suffix.contains('u') || bits > i64::MAX as u64,
    })
}
