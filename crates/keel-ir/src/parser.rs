//! Parser for the text format produced by `Module`'s `Display` impl.
//!
//! A file is one module: `decl` lines for external functions and data,
//! `data` definitions and `func` definitions, in any order, and `file`
//! lines naming the source files instruction locations refer to, each
//! before its first use. A call or a
//! `symbol_addr` may name something defined later, so functions are parsed
//! first and built once every name is known.
//!
//! The grammar is line-based: a block header or an instruction per line.
//! `;` starts a comment. Values may be used before the line that defines them
//! (a block printed early can use a value from a block that dominates it), so
//! parsing happens in two steps:
//!
//! 1. Parse each instruction into `InstData` with temporary value handles,
//!    one per distinct name.
//! 2. Number the values in the order the function will create them (block
//!    parameters and results, in text order), infer result types, and build.

use crate::{
    AbiParam, ArgExt, BinaryOp, Block, BlockCall, CallConv, Clause, ConvertOp, DataInit, DataItem,
    DataObject, DataRef, EntityRef, FloatCC, FuncDecl, FuncRef, Function, InlineAsm, InstData,
    IntCC, Intrinsic, Layout, Module, Ordering, OverflowOp, Purpose, RmwOp, SigRef, Signature,
    SourceFile, SourceLoc, StackSlot, StackSlotData, Symbol, Type, UnaryOp, Value,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

/// Parses a module: declarations, data and functions.
pub fn parse(src: &str) -> Result<Module, ParseError> {
    parse_module(src, None)
}

/// Like [`parse`], but if the module names no source files, every
/// instruction gets its own place in `src` as its location, with `path` as
/// the file: debug info for stepping through the IR itself.
pub fn parse_with_ir_lines(src: &str, path: &str) -> Result<Module, ParseError> {
    parse_module(src, Some(path))
}

fn parse_module(src: &str, ir_path: Option<&str>) -> Result<Module, ParseError> {
    let tokens = lex(src)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        files: HashMap::new(),
    };
    let mut files = Vec::new();
    let mut decls = Vec::new();
    let mut data = Vec::new();
    let mut parsed = Vec::new();
    parser.skip_newlines();
    while !parser.at(&Tok::Eof) {
        let (kw, loc) = parser.ident("`func`, `data`, `decl` or `file`")?;
        match kw.as_str() {
            "func" => parsed.push(parser.function()?),
            "file" => {
                let (name, name_loc) = parser.ident("a file name like `f0`")?;
                parser.expect(Tok::Punct('='))?;
                let path = match parser.next() {
                    (Tok::Str(path), _) => path,
                    _ => {
                        parser.pos -= 1;
                        return Err(parser.unexpected("a path like `\"main.c\"`"));
                    }
                };
                parser.expect(Tok::Newline)?;
                let file = SourceFile::new(files.len());
                if parser.files.insert(name.clone(), file).is_some() {
                    return Err(error(name_loc, format!("file `{name}` is declared twice")));
                }
                files.push(path);
            }
            "data" => data.push(parser.data_definition()?),
            "decl" if parser.eat(&Tok::Ident("data".into())) => {
                let thread_local = parser.eat(&Tok::Ident("thread_local".into()));
                let name = parser.global("a data name like `@environ`")?;
                parser.expect(Tok::Newline)?;
                data.push(DataObject {
                    name,
                    thread_local,
                    local: false,
                    init: None,
                });
            }
            "decl" => {
                let name = parser.global("a function name like `@putchar`")?;
                let sig = parser.signature()?;
                parser.expect(Tok::Newline)?;
                decls.push(FuncDecl { name, sig });
            }
            _ => {
                return Err(error(
                    loc,
                    format!("expected `func`, `data`, `decl` or `file`, found `{kw}`"),
                ));
            }
        }
        parser.skip_newlines();
    }

    // Duplicate names are left for the verifier; calls bind to the first.
    let mut sigs: HashMap<&str, &Signature> = HashMap::new();
    for decl in &decls {
        sigs.entry(&decl.name).or_insert(&decl.sig);
    }
    for func in &parsed {
        sigs.entry(&func.name).or_insert(&func.sig);
    }
    let data_names: HashSet<&str> = data.iter().map(|d| d.name.as_str()).collect();
    let ir_file = match ir_path {
        Some(path) if files.is_empty() => {
            files.push(path.to_string());
            Some(SourceFile::new(0))
        }
        _ => None,
    };
    let functions = parsed
        .iter()
        .map(|func| build(func, &sigs, &data_names, ir_file))
        .collect::<Result<_, _>>()?;
    Ok(Module {
        decls,
        data,
        functions,
        files,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Global(String),
    Int(i64),
    /// `0x...`: raw bits.
    Hex(u64),
    /// A decimal float, as written, so it can be parsed at the right width.
    Float(String),
    /// `b"..."`, with escapes already decoded.
    Bytes(Vec<u8>),
    /// `"..."`, with escapes already decoded.
    Str(String),
    Punct(char),
    Arrow,
    /// `...`
    Ellipsis,
    Newline,
    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Ident(s) => write!(f, "`{s}`"),
            Tok::Global(s) => write!(f, "`@{s}`"),
            Tok::Int(n) => write!(f, "`{n}`"),
            Tok::Hex(n) => write!(f, "`{n:#x}`"),
            Tok::Float(s) => write!(f, "`{s}`"),
            Tok::Bytes(_) => f.write_str("a byte string"),
            Tok::Str(_) => f.write_str("a string"),
            Tok::Punct(c) => write!(f, "`{c}`"),
            Tok::Arrow => f.write_str("`->`"),
            Tok::Ellipsis => f.write_str("`...`"),
            Tok::Newline => f.write_str("end of line"),
            Tok::Eof => f.write_str("end of file"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct Loc {
    line: u32,
    col: u32,
}

fn lex(src: &str) -> Result<Vec<(Tok, Loc)>, ParseError> {
    let mut tokens = Vec::new();
    for (line_idx, line) in src.lines().enumerate() {
        let line_no = line_idx as u32 + 1;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let loc = Loc {
                line: line_no,
                col: i as u32 + 1,
            };
            let ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';
            if c == ';' {
                // A comment runs to the end of the line.
                break;
            } else if c.is_whitespace() {
                i += 1;
            } else if c == 'b' && chars.get(i + 1) == Some(&'"') {
                let (bytes, end) = lex_bytes(&chars, i + 2, line_no)?;
                tokens.push((Tok::Bytes(bytes), loc));
                i = end;
            } else if c == '"' {
                let (bytes, end) = lex_bytes(&chars, i + 1, line_no)?;
                let text = String::from_utf8(bytes).map_err(|_| ParseError {
                    line: line_no,
                    col: loc.col,
                    message: "a string must be UTF-8".into(),
                })?;
                tokens.push((Tok::Str(text), loc));
                i = end;
            } else if chars[i..].starts_with(&['.', '.', '.']) {
                tokens.push((Tok::Ellipsis, loc));
                i += 3;
            } else if c == '-' && chars.get(i + 1) == Some(&'>') {
                tokens.push((Tok::Arrow, loc));
                i += 2;
            } else if c == '-' || c.is_ascii_digit() {
                let (tok, end) = lex_number(&chars, i, line_no)?;
                tokens.push((tok, loc));
                i = end;
            } else if c == '@' || c.is_ascii_alphabetic() || c == '_' {
                let start = if c == '@' { i + 1 } else { i };
                i = start;
                while i < chars.len() && ident_char(chars[i]) {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                tokens.push((
                    if c == '@' {
                        Tok::Global(text)
                    } else {
                        Tok::Ident(text)
                    },
                    loc,
                ));
            } else if "(),:={}+[]!".contains(c) {
                tokens.push((Tok::Punct(c), loc));
                i += 1;
            } else {
                return Err(ParseError {
                    line: loc.line,
                    col: loc.col,
                    message: format!("unexpected character `{c}`"),
                });
            }
        }
        tokens.push((
            Tok::Newline,
            Loc {
                line: line_no,
                col: i as u32 + 1,
            },
        ));
    }
    let last_line = src.lines().count() as u32 + 1;
    tokens.push((
        Tok::Eof,
        Loc {
            line: last_line,
            col: 1,
        },
    ));
    Ok(tokens)
}

/// Lexes a number starting at `chars[start]`: a decimal integer, `0x` hex
/// bits, or a float (`1.5`, `-2e-3`, `-inf`; `inf` and `nan` alone are
/// identifiers). Returns the token and the index after it.
fn lex_number(chars: &[char], start: usize, line: u32) -> Result<(Tok, usize), ParseError> {
    let err = |message: String| ParseError {
        line,
        col: start as u32 + 1,
        message,
    };
    let text = |end: usize| chars[start..end].iter().collect::<String>();
    let digits = |mut i: usize, radix: u32| {
        while i < chars.len() && chars[i].is_digit(radix) {
            i += 1;
        }
        i
    };
    let negative = chars[start] == '-';
    let mut i = start + usize::from(negative);
    if negative && chars[i..].starts_with(&['i', 'n', 'f']) {
        return Ok((Tok::Float("-inf".into()), i + 3));
    }
    if chars[i..].starts_with(&['0', 'x']) {
        let end = digits(i + 2, 16);
        let hex: String = chars[i + 2..end].iter().collect();
        if negative {
            return Err(err("hex literals are bits and cannot be negative".into()));
        }
        let bits = u64::from_str_radix(&hex, 16)
            .map_err(|_| err(format!("invalid hex literal `{}`", text(end))))?;
        return Ok((Tok::Hex(bits), end));
    }
    let int_end = digits(i, 10);
    if int_end == i {
        return Err(err(format!("invalid number `{}`", text(int_end))));
    }
    i = int_end;
    let mut is_float = false;
    if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
        i = digits(i + 1, 10);
        is_float = true;
    }
    if matches!(chars.get(i), Some('e' | 'E')) {
        let mut j = i + 1;
        if matches!(chars.get(j), Some('+' | '-')) {
            j += 1;
        }
        if chars.get(j).is_some_and(char::is_ascii_digit) {
            i = digits(j, 10);
            is_float = true;
        }
    }
    let text = text(i);
    if is_float {
        return Ok((Tok::Float(text), i));
    }
    let n = text
        .parse()
        .map_err(|_| err(format!("invalid integer `{text}`")))?;
    Ok((Tok::Int(n), i))
}

/// Decodes the body of a `b"..."` or `"..."` string starting at
/// `chars[start]`, up to the closing quote. Returns the bytes and the index
/// after the quote.
fn lex_bytes(chars: &[char], start: usize, line: u32) -> Result<(Vec<u8>, usize), ParseError> {
    let err = |i: usize, message: String| ParseError {
        line,
        col: i as u32 + 1,
        message,
    };
    let mut bytes = Vec::new();
    let mut i = start;
    loop {
        let Some(&c) = chars.get(i) else {
            return Err(err(start - 1, "unterminated string".into()));
        };
        i += 1;
        match c {
            '"' => return Ok((bytes, i)),
            '\\' => {
                let escape = chars.get(i).copied();
                i += 1;
                bytes.push(match escape {
                    Some('n') => b'\n',
                    Some('t') => b'\t',
                    Some('r') => b'\r',
                    Some('0') => 0,
                    Some('\\') => b'\\',
                    Some('"') => b'"',
                    Some('x') => {
                        let hex: String = chars.get(i..i + 2).unwrap_or(&[]).iter().collect();
                        i += 2;
                        u8::from_str_radix(&hex, 16).map_err(|_| {
                            err(i - 4, "`\\x` needs two hex digits, like `\\x0a`".into())
                        })?
                    }
                    other => {
                        let shown = other.map_or(String::new(), String::from);
                        return Err(err(i - 2, format!("unknown escape `\\{shown}`")));
                    }
                });
            }
            c if c.is_ascii() => bytes.push(c as u8),
            // Non-ASCII text would need an encoding; spell it with `\x`.
            c => {
                return Err(err(
                    i - 1,
                    format!("non-ASCII character `{c}`; use `\\x` escapes"),
                ));
            }
        }
    }
}

struct Parser {
    tokens: Vec<(Tok, Loc)>,
    pos: usize,
    /// Source files declared so far, by name.
    files: HashMap<String, SourceFile>,
}

/// One block as written, with temporary value and block handles.
struct ParsedBlock {
    block: Block,
    params: Vec<(Value, Type)>,
    insts: Vec<ParsedInst>,
}

struct ParsedInst {
    results: Vec<Value>,
    data: InstData,
    loc: Loc,
    /// Written as `!f0:12:5` at the end of the line.
    srcloc: Option<SourceLoc>,
}

/// A function as written, built once all signatures in the module are known.
struct ParsedFunction {
    name: String,
    sig: Signature,
    local: bool,
    personality: Option<String>,
    slots: Vec<StackSlotData>,
    blocks: Vec<ParsedBlock>,
    names: Names,
}

/// Temporary handles for one function: each distinct name gets one,
/// in order of first appearance.
#[derive(Default)]
struct Names {
    values: HashMap<String, Value>,
    /// Where each temporary value was first used, for "undefined" errors.
    value_first_use: Vec<(String, Loc)>,
    /// Definition order of temporary values; the index is the final number.
    value_defs: Vec<Value>,
    value_defined: Vec<bool>,
    blocks: HashMap<String, Block>,
    block_first_use: Vec<(String, Loc)>,
    block_defined: Vec<bool>,
    /// Stack slots, declared before the first block.
    slots: HashMap<String, StackSlot>,
    /// Called functions, by name; resolved against the module when building.
    callees: HashMap<String, FuncRef>,
    callee_first_use: Vec<(String, Loc)>,
    /// Names used by `symbol_addr`, which may be functions or data; the
    /// module decides when building.
    symbols: HashMap<String, usize>,
    symbol_first_use: Vec<(String, Loc)>,
    /// Indirect-call signatures without duplicates; the index is the
    /// final `SigRef`.
    signatures: Vec<Signature>,
}

impl Names {
    fn value(&mut self, name: &str, loc: Loc) -> Value {
        if let Some(&v) = self.values.get(name) {
            return v;
        }
        let v = Value::new(self.value_first_use.len());
        self.values.insert(name.to_string(), v);
        self.value_first_use.push((name.to_string(), loc));
        self.value_defined.push(false);
        v
    }

    fn define_value(&mut self, name: &str, loc: Loc) -> Result<Value, ParseError> {
        let v = self.value(name, loc);
        if std::mem::replace(&mut self.value_defined[v.index()], true) {
            return Err(error(loc, format!("value `{name}` is defined twice")));
        }
        self.value_defs.push(v);
        Ok(v)
    }

    fn callee(&mut self, name: &str, loc: Loc) -> FuncRef {
        if let Some(&f) = self.callees.get(name) {
            return f;
        }
        let f = FuncRef::new(self.callee_first_use.len());
        self.callees.insert(name.to_string(), f);
        self.callee_first_use.push((name.to_string(), loc));
        f
    }

    /// A temporary handle for a `symbol_addr` operand, stored as a `DataRef`
    /// until the module says what the name is.
    fn symbol(&mut self, name: &str, loc: Loc) -> Symbol {
        let n = self.symbols.len();
        let n = *self.symbols.entry(name.to_string()).or_insert_with(|| {
            self.symbol_first_use.push((name.to_string(), loc));
            n
        });
        Symbol::Data(DataRef::new(n))
    }

    fn signature(&mut self, sig: Signature) -> SigRef {
        let n = match self.signatures.iter().position(|s| *s == sig) {
            Some(n) => n,
            None => {
                self.signatures.push(sig);
                self.signatures.len() - 1
            }
        };
        SigRef::new(n)
    }

    fn block(&mut self, name: &str, loc: Loc) -> Block {
        if let Some(&b) = self.blocks.get(name) {
            return b;
        }
        let b = Block::new(self.block_first_use.len());
        self.blocks.insert(name.to_string(), b);
        self.block_first_use.push((name.to_string(), loc));
        self.block_defined.push(false);
        b
    }
}

fn error(loc: Loc, message: String) -> ParseError {
    ParseError {
        line: loc.line,
        col: loc.col,
        message,
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].0
    }

    fn loc(&self) -> Loc {
        self.tokens[self.pos].1
    }

    fn at(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn next(&mut self) -> (Tok, Loc) {
        let tok = self.tokens[self.pos].clone();
        if tok.0 != Tok::Eof {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok) -> Result<(), ParseError> {
        if self.eat(&tok) {
            Ok(())
        } else {
            Err(self.unexpected(&tok.to_string()))
        }
    }

    fn unexpected(&self, wanted: &str) -> ParseError {
        error(
            self.loc(),
            format!("expected {wanted}, found {}", self.peek()),
        )
    }

    fn skip_newlines(&mut self) {
        while self.eat(&Tok::Newline) {}
    }

    fn ident(&mut self, wanted: &str) -> Result<(String, Loc), ParseError> {
        match self.next() {
            (Tok::Ident(s), loc) => Ok((s, loc)),
            (_, _) => {
                self.pos -= 1;
                Err(self.unexpected(wanted))
            }
        }
    }

    fn ty(&mut self) -> Result<Type, ParseError> {
        let (name, loc) = self.ident("a type")?;
        parse_type(&name).ok_or_else(|| error(loc, format!("unknown type `{name}`")))
    }

    fn global(&mut self, wanted: &str) -> Result<String, ParseError> {
        match self.next() {
            (Tok::Global(name), _) => Ok(name),
            _ => {
                self.pos -= 1;
                Err(self.unexpected(wanted))
            }
        }
    }

    /// `(i32, ptr) -> i64 cc(c) returns_twice`; the return list, `cc(..)`
    /// and `returns_twice` are optional, and the parameters may end in
    /// `...`.
    fn signature(&mut self) -> Result<Signature, ParseError> {
        let mut params = Vec::new();
        let mut variadic = false;
        self.expect(Tok::Punct('('))?;
        if !self.eat(&Tok::Punct(')')) {
            loop {
                if self.eat(&Tok::Ellipsis) {
                    variadic = true;
                    self.expect(Tok::Punct(')'))?;
                    break;
                }
                params.push(self.abi_param()?);
                if self.eat(&Tok::Punct(')')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
            }
        }
        let mut returns = Vec::new();
        if self.eat(&Tok::Arrow) {
            if self.eat(&Tok::Punct('(')) {
                loop {
                    returns.push(self.abi_param()?);
                    if self.eat(&Tok::Punct(')')) {
                        break;
                    }
                    self.expect(Tok::Punct(','))?;
                }
            } else {
                returns.push(self.abi_param()?);
            }
        }
        let mut cc = CallConv::C;
        if self.at(&Tok::Ident("cc".into())) {
            self.pos += 1;
            self.expect(Tok::Punct('('))?;
            let (name, loc) = self.ident("a calling convention")?;
            cc = match name.as_str() {
                "c" => CallConv::C,
                _ => return Err(error(loc, format!("unknown calling convention `{name}`"))),
            };
            self.expect(Tok::Punct(')'))?;
        }
        let returns_twice = self.eat(&Tok::Ident("returns_twice".into()));
        Ok(Signature {
            params,
            returns,
            cc,
            variadic,
            returns_twice,
        })
    }

    /// A type with optional ABI attributes: `i8 sext`, `ptr byval {i32}`.
    fn abi_param(&mut self) -> Result<AbiParam, ParseError> {
        let mut param = AbiParam::new(self.ty()?);
        if self.eat(&Tok::Ident("sext".into())) {
            param.ext = ArgExt::Sext;
        } else if self.eat(&Tok::Ident("uext".into())) {
            param.ext = ArgExt::Uext;
        } else if self.eat(&Tok::Ident("byval".into())) {
            param.purpose = Purpose::ByVal(self.layout()?);
        } else if self.eat(&Tok::Ident("sret".into())) {
            param.purpose = Purpose::Sret(self.layout()?);
        }
        Ok(param)
    }

    /// `{layout, ...}`, `[n x layout]` or a type.
    fn layout(&mut self) -> Result<Layout, ParseError> {
        if self.eat(&Tok::Punct('{')) {
            let mut fields = Vec::new();
            if !self.eat(&Tok::Punct('}')) {
                loop {
                    fields.push(self.layout()?);
                    if self.eat(&Tok::Punct('}')) {
                        break;
                    }
                    self.expect(Tok::Punct(','))?;
                }
            }
            return Ok(Layout::Struct(fields));
        }
        if self.eat(&Tok::Punct('[')) {
            let n = self.u32("an element count")?;
            let (x, loc) = self.ident("`x`")?;
            if x != "x" {
                return Err(error(
                    loc,
                    format!("expected `x` as in `[4 x f32]`, found `{x}`"),
                ));
            }
            let elem = self.layout()?;
            self.expect(Tok::Punct(']'))?;
            return Ok(Layout::Array(Box::new(elem), n));
        }
        Ok(Layout::Scalar(self.ty()?))
    }

    /// Everything after `func`.
    fn function(&mut self) -> Result<ParsedFunction, ParseError> {
        let name = self.global("a function name like `@main`")?;
        let sig = self.signature()?;
        let local = self.eat(&Tok::Ident("local".into()));
        let personality = if self.eat(&Tok::Ident("personality".into())) {
            Some(self.global("a personality function like `@__gxx_personality_v0`")?)
        } else {
            None
        };
        self.expect(Tok::Punct('{'))?;
        self.expect(Tok::Newline)?;

        let mut names = Names::default();
        let mut slots = Vec::new();
        self.skip_newlines();
        while matches!(self.peek(), Tok::Ident(_)) && self.tokens[self.pos + 1].0 == Tok::Punct('=')
        {
            slots.push(self.stack_slot(&mut names)?);
            self.skip_newlines();
        }
        let mut blocks = Vec::new();
        while !self.eat(&Tok::Punct('}')) {
            if self.at(&Tok::Eof) {
                return Err(self.unexpected("`}`"));
            }
            blocks.push(self.block(&mut names)?);
            self.skip_newlines();
        }
        Ok(ParsedFunction {
            name,
            sig,
            local,
            personality,
            slots,
            blocks,
            names,
        })
    }

    /// `ss0 = stack_slot <size>, align <align>`
    fn stack_slot(&mut self, names: &mut Names) -> Result<StackSlotData, ParseError> {
        let (name, loc) = self.ident("a stack slot name")?;
        self.expect(Tok::Punct('='))?;
        let (kw, kw_loc) = self.ident("`stack_slot`")?;
        if kw != "stack_slot" {
            return Err(error(
                kw_loc,
                format!("expected `stack_slot`, found `{kw}`"),
            ));
        }
        let size = self.u32("a size in bytes")?;
        self.expect(Tok::Punct(','))?;
        let (kw, kw_loc) = self.ident("`align`")?;
        if kw != "align" {
            return Err(error(kw_loc, format!("expected `align`, found `{kw}`")));
        }
        let align = self.u32("an alignment")?;
        self.expect(Tok::Newline)?;
        let slot = StackSlot::new(names.slots.len());
        if names.slots.insert(name.clone(), slot).is_some() {
            return Err(error(loc, format!("stack slot `{name}` is defined twice")));
        }
        Ok(StackSlotData { size, align })
    }

    /// Everything after `data`:
    /// `@name = [local] [thread_local] [const] align <n> { item, ... }`, where an item is
    /// `b"..."`, `<int type> <n>`, `zero <n>` or `ptr @sym [+ <n>]`. Items
    /// may continue over several lines.
    fn data_definition(&mut self) -> Result<DataObject, ParseError> {
        let name = self.global("a data name like `@msg`")?;
        self.expect(Tok::Punct('='))?;
        let local = self.eat(&Tok::Ident("local".into()));
        let thread_local = self.eat(&Tok::Ident("thread_local".into()));
        let constant = self.eat(&Tok::Ident("const".into()));
        let (kw, kw_loc) = self.ident("`align`")?;
        if kw != "align" {
            return Err(error(kw_loc, format!("expected `align`, found `{kw}`")));
        }
        let align = self.u32("an alignment")?;
        self.expect(Tok::Punct('{'))?;
        let mut items = Vec::new();
        self.skip_newlines();
        if !self.eat(&Tok::Punct('}')) {
            loop {
                items.push(self.data_item()?);
                self.skip_newlines();
                if self.eat(&Tok::Punct('}')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
                self.skip_newlines();
            }
        }
        self.expect(Tok::Newline)?;
        Ok(DataObject {
            name,
            thread_local,
            local,
            init: Some(DataInit {
                constant,
                align,
                items,
            }),
        })
    }

    fn data_item(&mut self) -> Result<DataItem, ParseError> {
        if let (Tok::Bytes(bytes), _) = self.next() {
            return Ok(DataItem::Bytes(bytes));
        }
        self.pos -= 1;
        let (kw, loc) = self.ident("a data item like `i32 7`, `b\"..\"`, `zero 8` or `ptr @f`")?;
        match kw.as_str() {
            "zero" => Ok(DataItem::Zero(self.u32("a byte count")?)),
            "ptr" => {
                let symbol = self.global("a symbol like `@main`")?;
                let addend = if self.eat(&Tok::Punct('+')) {
                    self.u32("an offset in bytes")?
                } else {
                    0
                };
                Ok(DataItem::Ptr { symbol, addend })
            }
            _ => {
                let ty = parse_type(&kw)
                    .ok_or_else(|| error(loc, format!("expected a data item, found `{kw}`")))?;
                let value = self.int("an integer")?;
                Ok(DataItem::Int { ty, value })
            }
        }
    }

    fn u32(&mut self, wanted: &str) -> Result<u32, ParseError> {
        let loc = self.loc();
        let n = self.int(wanted)?;
        u32::try_from(n).map_err(|_| error(loc, format!("{n} is out of range")))
    }

    /// A decimal integer, or hex bits taken as a 64-bit two's complement
    /// number.
    fn int(&mut self, wanted: &str) -> Result<i64, ParseError> {
        match self.next() {
            (Tok::Int(n), _) => Ok(n),
            (Tok::Hex(bits), _) => Ok(bits as i64),
            _ => {
                self.pos -= 1;
                Err(self.unexpected(wanted))
            }
        }
    }

    /// The bits of a float literal of type `ty`: a decimal, `inf`, `nan`,
    /// or hex bits.
    fn float_bits(&mut self, ty: Type) -> Result<u64, ParseError> {
        let (tok, loc) = self.next();
        let text = match tok {
            Tok::Hex(bits) if ty == Type::F32 && bits > u64::from(u32::MAX) => {
                return Err(error(loc, format!("{bits:#x} is too wide for f32 bits")));
            }
            Tok::Hex(bits) => return Ok(bits),
            Tok::Float(s) => s,
            Tok::Int(n) => n.to_string(),
            Tok::Ident(s) if s == "inf" || s == "nan" => s,
            _ => {
                self.pos -= 1;
                return Err(
                    self.unexpected("a float like `1.5`, `inf`, `nan` or `0x3ff0000000000000`")
                );
            }
        };
        let bad = || error(loc, format!("invalid {ty} `{text}`"));
        match ty {
            Type::F32 => Ok(u64::from(text.parse::<f32>().map_err(|_| bad())?.to_bits())),
            _ => Ok(text.parse::<f64>().map_err(|_| bad())?.to_bits()),
        }
    }

    fn block(&mut self, names: &mut Names) -> Result<ParsedBlock, ParseError> {
        let (name, loc) = self.ident("a block name")?;
        let block = names.block(&name, loc);
        if std::mem::replace(&mut names.block_defined[block.index()], true) {
            return Err(error(loc, format!("block `{name}` is defined twice")));
        }
        let mut params = Vec::new();
        if self.eat(&Tok::Punct('(')) {
            loop {
                let (pname, ploc) = self.ident("a parameter name")?;
                self.expect(Tok::Punct(':'))?;
                let ty = self.ty()?;
                params.push((names.define_value(&pname, ploc)?, ty));
                if self.eat(&Tok::Punct(')')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
            }
        }
        self.expect(Tok::Punct(':'))?;
        self.expect(Tok::Newline)?;

        let mut insts = Vec::new();
        loop {
            self.skip_newlines();
            // A new block header is `name:` or `name(`; `}` ends the function.
            let next_is_header = matches!(self.peek(), Tok::Ident(_))
                && matches!(
                    self.tokens[self.pos + 1].0,
                    Tok::Punct(':') | Tok::Punct('(')
                );
            if next_is_header || self.at(&Tok::Punct('}')) || self.at(&Tok::Eof) {
                break;
            }
            insts.push(self.inst(names)?);
        }
        Ok(ParsedBlock {
            block,
            params,
            insts,
        })
    }

    fn inst(&mut self, names: &mut Names) -> Result<ParsedInst, ParseError> {
        // `v1 = ...` or, for calls, `v1, v2 = ...`.
        let mut results = Vec::new();
        if matches!(
            self.tokens[self.pos + 1].0,
            Tok::Punct('=') | Tok::Punct(',')
        ) {
            loop {
                results.push(self.ident("a result name")?);
                if self.eat(&Tok::Punct('=')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
            }
        }
        let (opcode, op_loc) = self.ident("an opcode")?;
        let (base, suffix) = match opcode.split_once('.') {
            Some((b, s)) => (b, Some(s)),
            None => (opcode.as_str(), None),
        };
        let suffix_type = |what: &str| -> Result<Type, ParseError> {
            let s = suffix.ok_or_else(|| {
                error(
                    op_loc,
                    format!("`{base}` needs a {what} suffix like `{base}.i32`"),
                )
            })?;
            parse_type(s).ok_or_else(|| error(op_loc, format!("unknown type `{s}`")))
        };
        let no_suffix = || -> Result<(), ParseError> {
            match suffix {
                Some(s) => Err(error(op_loc, format!("`{base}` takes no `.{s}` suffix"))),
                None => Ok(()),
            }
        };

        let data = if let Some(op) = parse_unary_op(base) {
            no_suffix()?;
            InstData::Unary {
                op,
                arg: self.value(names)?,
            }
        } else if let Some(op) = parse_binary_op(base) {
            no_suffix()?;
            let a = self.value(names)?;
            self.expect(Tok::Punct(','))?;
            InstData::Binary {
                op,
                args: [a, self.value(names)?],
            }
        } else if let Some(op) = parse_overflow_op(base) {
            no_suffix()?;
            let a = self.value(names)?;
            self.expect(Tok::Punct(','))?;
            InstData::Overflow {
                op,
                args: [a, self.value(names)?],
            }
        } else if let Some(op) = parse_convert_op(base) {
            InstData::Convert {
                op,
                ty: suffix_type("type")?,
                arg: self.value(names)?,
            }
        } else {
            match base {
                "iconst" => {
                    let ty = suffix_type("type")?;
                    let imm = self.int("an integer")?;
                    InstData::Iconst { ty, imm }
                }
                "fma" => {
                    no_suffix()?;
                    let a = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    let b = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Fma {
                        args: [a, b, self.value(names)?],
                    }
                }
                "fconst" => {
                    let ty = suffix_type("type")?;
                    if !ty.is_float() {
                        return Err(error(
                            op_loc,
                            format!("`fconst` makes f32 or f64, not {ty}"),
                        ));
                    }
                    InstData::Fconst {
                        ty,
                        bits: self.float_bits(ty)?,
                    }
                }
                "call" | "return_call" => {
                    no_suffix()?;
                    let (name, loc) = match self.next() {
                        (Tok::Global(name), loc) => (name, loc),
                        _ => {
                            self.pos -= 1;
                            return Err(self.unexpected("a function name like `@putchar`"));
                        }
                    };
                    let func = names.callee(&name, loc);
                    let args = self.arg_list(names)?;
                    if base == "call" {
                        InstData::Call { func, args }
                    } else {
                        InstData::ReturnCall { func, args }
                    }
                }
                "invoke" => {
                    no_suffix()?;
                    let (name, loc) = match self.next() {
                        (Tok::Global(name), loc) => (name, loc),
                        _ => {
                            self.pos -= 1;
                            return Err(self.unexpected("a function name like `@may_throw`"));
                        }
                    };
                    let func = names.callee(&name, loc);
                    let args = self.arg_list(names)?;
                    self.expect(Tok::Punct(','))?;
                    let normal = self.block_call(names)?;
                    self.expect(Tok::Punct(','))?;
                    let unwind = self.block_call(names)?;
                    InstData::Invoke {
                        func,
                        args,
                        dests: [normal, unwind],
                    }
                }
                "landingpad" => {
                    no_suffix()?;
                    let mut clauses = Vec::new();
                    loop {
                        let (kw, kw_loc) = self.ident("`catch` or `cleanup`")?;
                        clauses.push(match kw.as_str() {
                            "cleanup" => Clause::Cleanup,
                            "catch" if self.eat(&Tok::Ident("null".into())) => Clause::Catch(None),
                            "catch" => {
                                let loc = self.loc();
                                let name = self.global("a typeinfo like `@_ZTIi`, or `null`")?;
                                let Symbol::Data(temp) = names.symbol(&name, loc) else {
                                    unreachable!("the parser stores symbols as temporary DataRefs")
                                };
                                Clause::Catch(Some(temp))
                            }
                            _ => {
                                return Err(error(
                                    kw_loc,
                                    format!("expected `catch` or `cleanup`, found `{kw}`"),
                                ));
                            }
                        });
                        if !self.eat(&Tok::Punct(',')) {
                            break;
                        }
                    }
                    InstData::LandingPad { clauses }
                }
                "eh_typeid" => {
                    no_suffix()?;
                    let loc = self.loc();
                    let name = self.global("a typeinfo like `@_ZTIi`")?;
                    let Symbol::Data(typeinfo) = names.symbol(&name, loc) else {
                        unreachable!("the parser stores symbols as temporary DataRefs")
                    };
                    InstData::EhTypeid { typeinfo }
                }
                "return_call_indirect" => {
                    no_suffix()?;
                    let callee = self.value(names)?;
                    let args = self.arg_list(names)?;
                    self.expect(Tok::Punct(':'))?;
                    let sig = self.signature()?;
                    InstData::ReturnCallIndirect {
                        sig: names.signature(sig),
                        callee,
                        args,
                    }
                }
                "call_indirect" => {
                    no_suffix()?;
                    let callee = self.value(names)?;
                    let args = self.arg_list(names)?;
                    self.expect(Tok::Punct(':'))?;
                    let sig = self.signature()?;
                    if sig.returns.len() != results.len() {
                        return Err(error(
                            op_loc,
                            format!(
                                "the signature returns {} value(s), but {} result(s) are named",
                                sig.returns.len(),
                                results.len()
                            ),
                        ));
                    }
                    InstData::CallIndirect {
                        sig: names.signature(sig),
                        callee,
                        args,
                    }
                }
                "symbol_addr" => {
                    no_suffix()?;
                    let (Tok::Global(name), loc) = self.next() else {
                        self.pos -= 1;
                        return Err(self.unexpected("a symbol like `@msg`"));
                    };
                    InstData::SymbolAddr {
                        symbol: names.symbol(&name, loc),
                    }
                }
                "intrinsic" => {
                    no_suffix()?;
                    let (name, loc) = self.ident("an intrinsic name like `memcpy`")?;
                    let id = Intrinsic::from_name(&name)
                        .ok_or_else(|| error(loc, format!("unknown intrinsic `{name}`")))?;
                    InstData::Intrinsic {
                        id,
                        args: self.arg_list(names)?,
                    }
                }
                "trap" => {
                    no_suffix()?;
                    InstData::Trap
                }
                "asm" => {
                    no_suffix()?;
                    let template = self.string("a template string")?;
                    self.expect(Tok::Punct(','))?;
                    let constraints_loc = self.loc();
                    let constraints = self.string("a constraint string like `\"=r,r\"`")?;
                    let args = self.arg_list(names)?;
                    let mut types = Vec::new();
                    if self.eat(&Tok::Arrow) {
                        if self.eat(&Tok::Punct('(')) {
                            loop {
                                types.push(self.ty()?);
                                if self.eat(&Tok::Punct(')')) {
                                    break;
                                }
                                self.expect(Tok::Punct(','))?;
                            }
                        } else {
                            types.push(self.ty()?);
                        }
                    }
                    if types.len() != results.len() {
                        return Err(error(
                            op_loc,
                            format!(
                                "`asm` returns {} value(s), but {} result(s) are named",
                                types.len(),
                                results.len()
                            ),
                        ));
                    }
                    let asm = InlineAsm::new(&template, &constraints, &types)
                        .map_err(|message| error(constraints_loc, message))?;
                    if asm.inputs.len() != args.len() {
                        return Err(error(
                            constraints_loc,
                            format!(
                                "{} input constraint(s) but {} argument(s)",
                                asm.inputs.len(),
                                args.len()
                            ),
                        ));
                    }
                    InstData::InlineAsm {
                        asm: Box::new(asm),
                        args,
                    }
                }
                "stack_addr" => {
                    no_suffix()?;
                    let (name, loc) = self.ident("a stack slot")?;
                    let slot = *names.slots.get(&name).ok_or_else(|| {
                        error(loc, format!("stack slot `{name}` is not declared"))
                    })?;
                    InstData::StackAddr { slot }
                }
                "select" => {
                    no_suffix()?;
                    let cond = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    let if_true = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Select {
                        args: [cond, if_true, self.value(names)?],
                    }
                }
                "icmp" => {
                    let s = suffix.ok_or_else(|| {
                        error(op_loc, "`icmp` needs a condition like `icmp.eq`".into())
                    })?;
                    let cond = parse_cond(s)
                        .ok_or_else(|| error(op_loc, format!("unknown condition `{s}`")))?;
                    let a = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Icmp {
                        cond,
                        args: [a, self.value(names)?],
                    }
                }
                "fcmp" => {
                    let s = suffix.ok_or_else(|| {
                        error(op_loc, "`fcmp` needs a condition like `fcmp.lt`".into())
                    })?;
                    let cond = parse_float_cond(s)
                        .ok_or_else(|| error(op_loc, format!("unknown condition `{s}`")))?;
                    let a = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Fcmp {
                        cond,
                        args: [a, self.value(names)?],
                    }
                }
                "va_arg" => InstData::VaArg {
                    ty: suffix_type("type")?,
                    list: self.value(names)?,
                },
                "load" => InstData::Load {
                    ty: suffix_type("type")?,
                    addr: self.value(names)?,
                },
                "atomic_load" => InstData::AtomicLoad {
                    ty: suffix_type("type")?,
                    ordering: self.ordering()?,
                    addr: self.value(names)?,
                },
                "atomic_store" => {
                    no_suffix()?;
                    let ordering = self.ordering()?;
                    let value = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::AtomicStore {
                        ordering,
                        value,
                        addr: self.value(names)?,
                    }
                }
                "atomic_rmw" => {
                    let s = suffix.ok_or_else(|| {
                        error(
                            op_loc,
                            "`atomic_rmw` needs an operation like `atomic_rmw.add`".into(),
                        )
                    })?;
                    let op = parse_rmw_op(s).ok_or_else(|| {
                        error(op_loc, format!("unknown atomic_rmw operation `{s}`"))
                    })?;
                    let ordering = self.ordering()?;
                    let addr = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::AtomicRmw {
                        op,
                        ordering,
                        args: [addr, self.value(names)?],
                    }
                }
                "atomic_cas" => {
                    no_suffix()?;
                    let ordering = self.ordering()?;
                    let addr = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    let expected = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::AtomicCas {
                        ordering,
                        args: [addr, expected, self.value(names)?],
                    }
                }
                "fence" => {
                    no_suffix()?;
                    InstData::Fence {
                        ordering: self.ordering()?,
                    }
                }
                "store" => {
                    no_suffix()?;
                    let value = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Store {
                        value,
                        addr: self.value(names)?,
                    }
                }
                "jump" => {
                    no_suffix()?;
                    InstData::Jump {
                        dest: self.block_call(names)?,
                    }
                }
                "brif" => {
                    no_suffix()?;
                    let cond = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    let then_dest = self.block_call(names)?;
                    self.expect(Tok::Punct(','))?;
                    InstData::Brif {
                        cond,
                        dests: [then_dest, self.block_call(names)?],
                    }
                }
                "switch" => {
                    no_suffix()?;
                    let arg = self.value(names)?;
                    self.expect(Tok::Punct(','))?;
                    let mut dests = vec![self.block_call(names)?];
                    let mut cases = Vec::new();
                    self.expect(Tok::Punct(','))?;
                    self.expect(Tok::Punct('['))?;
                    if !self.eat(&Tok::Punct(']')) {
                        loop {
                            cases.push(self.int("a case value")?);
                            self.expect(Tok::Punct(':'))?;
                            dests.push(self.block_call(names)?);
                            if self.eat(&Tok::Punct(']')) {
                                break;
                            }
                            self.expect(Tok::Punct(','))?;
                        }
                    }
                    InstData::Switch { arg, dests, cases }
                }
                "return" => {
                    no_suffix()?;
                    let mut args = Vec::new();
                    if !self.at(&Tok::Newline) && !self.at(&Tok::Punct('!')) {
                        args.push(self.value(names)?);
                        while self.eat(&Tok::Punct(',')) {
                            args.push(self.value(names)?);
                        }
                    }
                    InstData::Return { args }
                }
                _ => return Err(error(op_loc, format!("unknown opcode `{opcode}`"))),
            }
        };
        let srcloc = if self.eat(&Tok::Punct('!')) {
            Some(self.srcloc()?)
        } else {
            None
        };
        self.expect(Tok::Newline)?;

        // A call's result count depends on the callee's signature, which is
        // checked when building; every other instruction has 0 or 1 results.
        let expected = match &data {
            InstData::Call { .. }
            | InstData::CallIndirect { .. }
            | InstData::Invoke { .. }
            | InstData::InlineAsm { .. } => None,
            InstData::LandingPad { .. } => Some(2),
            InstData::Intrinsic { id, .. } => Some(id.info().returns.len()),
            InstData::Overflow { .. } => Some(2),
            InstData::Store { .. }
            | InstData::AtomicStore { .. }
            | InstData::Fence { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Switch { .. }
            | InstData::Return { .. }
            | InstData::ReturnCall { .. }
            | InstData::ReturnCallIndirect { .. }
            | InstData::Trap => Some(0),
            _ => Some(1),
        };
        // Name intrinsics by themselves, not by the `intrinsic` keyword.
        let what = match &data {
            InstData::Intrinsic { id, .. } => id.to_string(),
            _ => base.to_string(),
        };
        match (expected, results.len()) {
            (Some(0), n) if n > 0 => return Err(error(op_loc, format!("`{what}` has no result"))),
            (Some(1), 0) => {
                return Err(error(
                    op_loc,
                    format!("`{base}` produces a result; write `vN = {base} ...`"),
                ));
            }
            (Some(1), n) if n > 1 => return Err(error(op_loc, format!("`{base}` has one result"))),
            (Some(2), n) if n != 2 => {
                return Err(error(
                    op_loc,
                    format!("`{base}` has two results; write `vN, vM = {base} ...`"),
                ));
            }
            _ => {}
        }
        let results = results
            .into_iter()
            .map(|(name, loc)| names.define_value(&name, loc))
            .collect::<Result<_, _>>()?;
        Ok(ParsedInst {
            results,
            data,
            loc: op_loc,
            srcloc,
        })
    }

    /// `relaxed`, `acquire`, `release`, `acq_rel` or `seq_cst`.
    fn ordering(&mut self) -> Result<Ordering, ParseError> {
        let (name, loc) = self.ident("an ordering like `seq_cst`")?;
        Ok(match name.as_str() {
            "relaxed" => Ordering::Relaxed,
            "acquire" => Ordering::Acquire,
            "release" => Ordering::Release,
            "acq_rel" => Ordering::AcqRel,
            "seq_cst" => Ordering::SeqCst,
            _ => {
                return Err(error(
                    loc,
                    format!(
                        "expected an ordering (`relaxed`, `acquire`, `release`, `acq_rel` or `seq_cst`), found `{name}`"
                    ),
                ));
            }
        })
    }

    /// `f0:12:5`, after the `!`.
    fn srcloc(&mut self) -> Result<SourceLoc, ParseError> {
        let (name, loc) = self.ident("a file name like `f0`")?;
        let file = *self
            .files
            .get(&name)
            .ok_or_else(|| error(loc, format!("file `{name}` is not declared")))?;
        self.expect(Tok::Punct(':'))?;
        let line = self.u32("a line number")?;
        self.expect(Tok::Punct(':'))?;
        let col = self.u32("a column number")?;
        Ok(SourceLoc { file, line, col })
    }

    /// `(v0, v1, ...)`
    fn arg_list(&mut self, names: &mut Names) -> Result<Vec<Value>, ParseError> {
        self.expect(Tok::Punct('('))?;
        let mut args = Vec::new();
        if !self.eat(&Tok::Punct(')')) {
            loop {
                args.push(self.value(names)?);
                if self.eat(&Tok::Punct(')')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
            }
        }
        Ok(args)
    }

    fn string(&mut self, wanted: &str) -> Result<String, ParseError> {
        match self.next() {
            (Tok::Str(s), _) => Ok(s),
            _ => {
                self.pos -= 1;
                Err(self.unexpected(wanted))
            }
        }
    }

    fn value(&mut self, names: &mut Names) -> Result<Value, ParseError> {
        let (name, loc) = self.ident("a value")?;
        Ok(names.value(&name, loc))
    }

    fn block_call(&mut self, names: &mut Names) -> Result<BlockCall, ParseError> {
        let (name, loc) = self.ident("a block name")?;
        let block = names.block(&name, loc);
        let mut args = Vec::new();
        if self.eat(&Tok::Punct('(')) && !self.eat(&Tok::Punct(')')) {
            loop {
                args.push(self.value(names)?);
                if self.eat(&Tok::Punct(')')) {
                    break;
                }
                self.expect(Tok::Punct(','))?;
            }
        }
        Ok(BlockCall { block, args })
    }
}

/// Renumbers temporary handles, infers result types and builds the function.
fn build(
    parsed: &ParsedFunction,
    sigs: &HashMap<&str, &Signature>,
    data_names: &HashSet<&str>,
    ir_file: Option<SourceFile>,
) -> Result<Function, ParseError> {
    let ParsedFunction {
        name,
        sig,
        local,
        personality,
        slots,
        blocks,
        names,
    } = parsed;
    if let Some(i) = names.block_defined.iter().position(|&d| !d) {
        let (name, loc) = &names.block_first_use[i];
        return Err(error(*loc, format!("block `{name}` is never defined")));
    }
    if let Some(i) = names.value_defined.iter().position(|&d| !d) {
        let (name, loc) = &names.value_first_use[i];
        return Err(error(*loc, format!("value `{name}` is never defined")));
    }

    // Final numbering: blocks and values are created in text order.
    let mut block_map = vec![Block::new(0); blocks.len()];
    for (i, parsed) in blocks.iter().enumerate() {
        block_map[parsed.block.index()] = Block::new(i);
    }
    let mut value_map = vec![Value::new(0); names.value_defs.len()];
    for (i, &v) in names.value_defs.iter().enumerate() {
        value_map[v.index()] = Value::new(i);
    }

    // Result types, by temporary handle. Binary results take their first
    // operand's type, which may be defined later in the text, so iterate.
    let mut types: Vec<Option<Type>> = vec![None; value_map.len()];
    for parsed in blocks {
        for &(v, ty) in &parsed.params {
            types[v.index()] = Some(ty);
        }
    }

    // Callees, in order of first use, which is their temporary numbering.
    let mut callee_sigs = Vec::new();
    for (callee, loc) in &names.callee_first_use {
        let sig = sigs
            .get(callee.as_str())
            .ok_or_else(|| error(*loc, format!("function `@{callee}` is not declared")))?;
        callee_sigs.push((callee, (*sig).clone()));
    }
    // `symbol_addr` names, in order of first use: functions (by signature)
    // or data.
    let mut symbols = Vec::new();
    for (symbol, loc) in &names.symbol_first_use {
        let sig = sigs.get(symbol.as_str());
        if sig.is_none() && !data_names.contains(symbol.as_str()) {
            return Err(error(*loc, format!("symbol `@{symbol}` is not declared")));
        }
        symbols.push((symbol, sig.map(|s| (*s).clone())));
    }
    for inst in blocks.iter().flat_map(|b| &b.insts) {
        if let InstData::CallIndirect { sig, .. } = &inst.data {
            let returns = names.signatures[sig.index()].return_types();
            for (&v, &ty) in inst.results.iter().zip(&returns) {
                types[v.index()] = Some(ty);
            }
        }
        if let InstData::Intrinsic { id, .. } = &inst.data {
            for (&v, &ty) in inst.results.iter().zip(id.info().returns) {
                types[v.index()] = Some(ty);
            }
        }
        if let InstData::InlineAsm { asm, .. } = &inst.data {
            for (&v, &(_, ty)) in inst.results.iter().zip(&asm.outputs) {
                types[v.index()] = Some(ty);
            }
        }
        if let InstData::LandingPad { .. } = &inst.data
            && let [exception, selector] = inst.results[..]
        {
            types[exception.index()] = Some(Type::Ptr);
            types[selector.index()] = Some(Type::I32);
        }
        if let InstData::Call { func, .. } | InstData::Invoke { func, .. } = &inst.data {
            let (callee, sig) = &callee_sigs[func.index()];
            if sig.returns.len() != inst.results.len() {
                return Err(error(
                    inst.loc,
                    format!(
                        "`@{callee}` returns {} value(s), but {} result(s) are named",
                        sig.returns.len(),
                        inst.results.len()
                    ),
                ));
            }
            for (&v, &ty) in inst.results.iter().zip(&sig.return_types()) {
                types[v.index()] = Some(ty);
            }
        }
    }

    loop {
        let mut changed = false;
        for inst in blocks.iter().flat_map(|b| &b.insts) {
            if let (InstData::Overflow { args, .. }, [value, flag]) =
                (&inst.data, &inst.results[..])
                && types[value.index()].is_none()
                && types[args[0].index()].is_some()
            {
                types[value.index()] = types[args[0].index()];
                types[flag.index()] = Some(Type::I8);
                changed = true;
            }
            let [result] = inst.results[..] else { continue };
            if types[result.index()].is_some() {
                continue;
            }
            let ty = match &inst.data {
                InstData::Iconst { ty, .. }
                | InstData::Fconst { ty, .. }
                | InstData::Convert { ty, .. }
                | InstData::Load { ty, .. }
                | InstData::AtomicLoad { ty, .. }
                | InstData::VaArg { ty, .. } => Some(*ty),
                InstData::AtomicRmw { args, .. } => types[args[1].index()],
                InstData::AtomicCas { args, .. } => types[args[1].index()],
                InstData::StackAddr { .. } | InstData::SymbolAddr { .. } => Some(Type::Ptr),
                InstData::Icmp { .. } | InstData::Fcmp { .. } => Some(Type::I8),
                InstData::Unary { arg, .. } => types[arg.index()],
                InstData::Binary { args, .. } => types[args[0].index()],
                InstData::Select { args } => types[args[1].index()],
                InstData::Fma { args } => types[args[0].index()],
                InstData::EhTypeid { .. } => Some(Type::I32),
                _ => None,
            };
            if ty.is_some() {
                types[result.index()] = ty;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut func = Function::new(name.clone(), sig.clone());
    func.personality = personality.clone();
    func.local = *local;
    // Slots and callees were numbered in declaration order, which is
    // creation order.
    for slot in slots {
        func.create_stack_slot(slot.size, slot.align);
    }
    for (callee, sig) in callee_sigs {
        func.import_function(callee, sig);
    }
    // After the callees, so a function named by both a call and a
    // `symbol_addr` reuses the call's `FuncRef`.
    let symbols: Vec<Symbol> = symbols
        .into_iter()
        .map(|(name, sig)| match sig {
            Some(sig) => Symbol::Func(func.import_function(name, sig)),
            None => Symbol::Data(func.import_data(name)),
        })
        .collect();
    for sig in &names.signatures {
        func.import_signature(sig.clone());
    }
    for _ in blocks {
        func.create_block();
    }
    for parsed in blocks {
        let block = block_map[parsed.block.index()];
        for &(v, ty) in &parsed.params {
            let created = func.append_block_param(block, ty);
            debug_assert_eq!(created, value_map[v.index()]);
        }
        for inst in &parsed.insts {
            let mut data = inst.data.clone();
            data.for_each_operand_mut(|v| *v = value_map[v.index()]);
            for dest in data.branch_dests_mut() {
                dest.block = block_map[dest.block.index()];
            }
            if let InstData::SymbolAddr { symbol } = &mut data {
                let Symbol::Data(temp) = *symbol else {
                    unreachable!("the parser stores symbols as temporary DataRefs")
                };
                *symbol = symbols[temp.index()];
            }
            // Typeinfos are data objects.
            let typeinfo = |temp: DataRef| match symbols[temp.index()] {
                Symbol::Data(data) => Ok(data),
                Symbol::Func(_) => {
                    let (name, loc) = &names.symbol_first_use[temp.index()];
                    Err(error(
                        *loc,
                        format!("typeinfo `@{name}` must be data, not a function"),
                    ))
                }
            };
            match &mut data {
                InstData::LandingPad { clauses } => {
                    for clause in clauses {
                        if let Clause::Catch(Some(temp)) = clause {
                            *temp = typeinfo(*temp)?;
                        }
                    }
                }
                InstData::EhTypeid { typeinfo: temp } => *temp = typeinfo(*temp)?,
                _ => {}
            }
            let result_tys = inst
                .results
                .iter()
                .map(|&v| {
                    types[v.index()].ok_or_else(|| {
                        let (name, loc) = &names.value_first_use[v.index()];
                        error(
                            *loc,
                            format!("cannot infer the type of `{name}`: it depends on itself"),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let created = func.append_inst_with_results(block, data, &result_tys);
            let ir_loc = ir_file.map(|file| SourceLoc {
                file,
                line: inst.loc.line,
                col: inst.loc.col,
            });
            func.set_inst_loc(created, inst.srcloc.or(ir_loc));
            debug_assert!(
                func.inst_results(created)
                    .iter()
                    .eq(inst.results.iter().map(|v| &value_map[v.index()]))
            );
        }
    }
    Ok(func)
}

fn parse_type(s: &str) -> Option<Type> {
    Some(match s {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "i128" => Type::I128,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "ptr" => Type::Ptr,
        _ => return None,
    })
}

fn parse_binary_op(s: &str) -> Option<BinaryOp> {
    Some(match s {
        "iadd" => BinaryOp::Iadd,
        "isub" => BinaryOp::Isub,
        "imul" => BinaryOp::Imul,
        "udiv" => BinaryOp::Udiv,
        "sdiv" => BinaryOp::Sdiv,
        "urem" => BinaryOp::Urem,
        "srem" => BinaryOp::Srem,
        "and" => BinaryOp::And,
        "or" => BinaryOp::Or,
        "xor" => BinaryOp::Xor,
        "shl" => BinaryOp::Shl,
        "ushr" => BinaryOp::Ushr,
        "sshr" => BinaryOp::Sshr,
        "rotl" => BinaryOp::Rotl,
        "rotr" => BinaryOp::Rotr,
        "umulhi" => BinaryOp::Umulhi,
        "smulhi" => BinaryOp::Smulhi,
        "ptradd" => BinaryOp::Ptradd,
        "fadd" => BinaryOp::Fadd,
        "fsub" => BinaryOp::Fsub,
        "fmul" => BinaryOp::Fmul,
        "fdiv" => BinaryOp::Fdiv,
        "fmin" => BinaryOp::Fmin,
        "fmax" => BinaryOp::Fmax,
        "fcopysign" => BinaryOp::Fcopysign,
        _ => return None,
    })
}

fn parse_unary_op(s: &str) -> Option<UnaryOp> {
    Some(match s {
        "ineg" => UnaryOp::Ineg,
        "bnot" => UnaryOp::Bnot,
        "clz" => UnaryOp::Clz,
        "ctz" => UnaryOp::Ctz,
        "popcnt" => UnaryOp::Popcnt,
        "bswap" => UnaryOp::Bswap,
        "fneg" => UnaryOp::Fneg,
        "fabs" => UnaryOp::Fabs,
        "fsqrt" => UnaryOp::Fsqrt,
        "fceil" => UnaryOp::Fceil,
        "ffloor" => UnaryOp::Ffloor,
        "ftrunc" => UnaryOp::Ftrunc,
        "fnearest" => UnaryOp::Fnearest,
        _ => return None,
    })
}

fn parse_rmw_op(s: &str) -> Option<RmwOp> {
    Some(match s {
        "add" => RmwOp::Add,
        "sub" => RmwOp::Sub,
        "and" => RmwOp::And,
        "or" => RmwOp::Or,
        "xor" => RmwOp::Xor,
        "xchg" => RmwOp::Xchg,
        "umin" => RmwOp::Umin,
        "umax" => RmwOp::Umax,
        "smin" => RmwOp::Smin,
        "smax" => RmwOp::Smax,
        _ => return None,
    })
}

fn parse_overflow_op(s: &str) -> Option<OverflowOp> {
    Some(match s {
        "uadd_overflow" => OverflowOp::Uadd,
        "sadd_overflow" => OverflowOp::Sadd,
        "usub_overflow" => OverflowOp::Usub,
        "ssub_overflow" => OverflowOp::Ssub,
        "umul_overflow" => OverflowOp::Umul,
        "smul_overflow" => OverflowOp::Smul,
        _ => return None,
    })
}

fn parse_convert_op(s: &str) -> Option<ConvertOp> {
    Some(match s {
        "sext" => ConvertOp::Sext,
        "uext" => ConvertOp::Uext,
        "trunc" => ConvertOp::Trunc,
        "ptrtoint" => ConvertOp::PtrToInt,
        "inttoptr" => ConvertOp::IntToPtr,
        "fpext" => ConvertOp::Fpext,
        "fptrunc" => ConvertOp::Fptrunc,
        "fptosi" => ConvertOp::Fptosi,
        "fptoui" => ConvertOp::Fptoui,
        "sitofp" => ConvertOp::Sitofp,
        "uitofp" => ConvertOp::Uitofp,
        "bitcast" => ConvertOp::Bitcast,
        _ => return None,
    })
}

fn parse_cond(s: &str) -> Option<IntCC> {
    Some(match s {
        "eq" => IntCC::Eq,
        "ne" => IntCC::Ne,
        "slt" => IntCC::Slt,
        "sle" => IntCC::Sle,
        "sgt" => IntCC::Sgt,
        "sge" => IntCC::Sge,
        "ult" => IntCC::Ult,
        "ule" => IntCC::Ule,
        "ugt" => IntCC::Ugt,
        "uge" => IntCC::Uge,
        _ => return None,
    })
}

fn parse_float_cond(s: &str) -> Option<FloatCC> {
    Some(match s {
        "eq" => FloatCC::Eq,
        "ne" => FloatCC::Ne,
        "lt" => FloatCC::Lt,
        "le" => FloatCC::Le,
        "gt" => FloatCC::Gt,
        "ge" => FloatCC::Ge,
        "ord" => FloatCC::Ord,
        "uno" => FloatCC::Uno,
        _ => return None,
    })
}
