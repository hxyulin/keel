//! Bounded GNU script grammar. Unknown syntax is an error, never a layout hint.
mod layout;
use crate::Result;
pub(super) use layout::link;

#[derive(Clone, Debug)]
enum Expr {
    Number(u64),
    Name(String),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Assert(Box<Expr>, String),
}
impl Expr {
    fn references(&self, name: &str) -> bool {
        match self {
            Self::Name(n) => n == name,
            Self::Unary(_, e) | Self::Assert(e, _) => e.references(name),
            Self::Binary(_, a, b) => a.references(name) || b.references(name),
            Self::Call(_, args) => args.iter().any(|a| a.references(name)),
            Self::Number(_) => false,
        }
    }
}
#[derive(Debug)]
struct Assignment {
    name: String,
    expr: Expr,
    provide: bool,
}
#[derive(Debug)]
enum Command {
    Assign(Assignment),
    Assert(Expr),
    Input(Vec<(String, bool)>),
    Output(Output),
}
#[derive(Debug)]
struct Output {
    name: String,
    address: Option<Expr>,
    load: Option<Expr>,
    body: Vec<Command>,
    phdrs: Vec<String>,
    fill: Option<Expr>,
    info: bool,
}
#[derive(Debug)]
struct Phdr {
    name: String,
    kind: u32,
    flags: u32,
}
#[derive(Debug)]
pub(super) struct Script {
    pub entry: Option<String>,
    commands: Vec<Command>,
    phdrs: Vec<Phdr>,
}
struct Parser {
    tokens: Vec<String>,
    at: usize,
    expression_nodes: usize,
}
impl Parser {
    fn error(&self, message: &str) -> String {
        format!(
            "linker script token {} ({:?}): {message}",
            self.at,
            self.tokens.get(self.at)
        )
    }
    fn peek(&self) -> &str {
        self.tokens.get(self.at).map_or("", String::as_str)
    }
    fn eat(&mut self, token: &str) -> bool {
        if self.peek() == token {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, token: &str) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {token}")))
        }
    }
    fn take(&mut self) -> Result<String> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or_else(|| self.error("unexpected end"))?;
        self.at += 1;
        Ok(token)
    }
    fn name(&mut self) -> Result<String> {
        let name = self.take()?;
        if name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '/') {
            Ok(name)
        } else {
            Err(self.error("expected name"))
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr> {
        self.expression_nodes = 0;
        self.expression(min, 0)
    }
    fn expression(&mut self, min: u8, depth: usize) -> Result<Expr> {
        if depth > 128 {
            return Err(self.error("expression nesting limit exceeded"));
        }
        self.expression_nodes += 1;
        if self.expression_nodes > 512 {
            return Err(self.error("expression node limit exceeded"));
        }
        let token = self.take()?;
        let mut left = if matches!(token.as_str(), "~" | "-" | "+" | "!") {
            Expr::Unary(token, Box::new(self.expression(12, depth + 1)?))
        } else if token == "(" {
            let e = self.expression(0, depth + 1)?;
            self.expect(")")?;
            e
        } else if token.starts_with(|c: char| c.is_ascii_digit()) {
            let (digits, radix) = token
                .strip_prefix("0x")
                .map_or((token.as_str(), 10), |s| (s, 16));
            Expr::Number(
                u64::from_str_radix(digits, radix).map_err(|_| self.error("invalid integer"))?,
            )
        } else if self.eat("(") {
            if token == "ASSERT" {
                let condition = self.expression(0, depth + 1)?;
                self.expect(",")?;
                let message = self.take()?;
                if !message.starts_with('"') {
                    return Err(self.error("ASSERT requires a string"));
                }
                self.expect(")")?;
                Expr::Assert(Box::new(condition), message.trim_matches('"').into())
            } else {
                if !matches!(
                    token.as_str(),
                    "ALIGN" | "ABSOLUTE" | "ADDR" | "LOADADDR" | "SIZEOF" | "MAX" | "MIN"
                ) {
                    return Err(self.error(&format!("unsupported function {token}")));
                }
                let mut args = vec![self.expression(0, depth + 1)?];
                while self.eat(",") {
                    args.push(self.expression(0, depth + 1)?);
                }
                self.expect(")")?;
                let count = args.len();
                if !(count == 1
                    && matches!(
                        token.as_str(),
                        "ALIGN" | "ABSOLUTE" | "ADDR" | "LOADADDR" | "SIZEOF"
                    )
                    || count == 2 && matches!(token.as_str(), "ALIGN" | "MIN" | "MAX"))
                {
                    return Err(self.error("invalid function argument count"));
                }
                Expr::Call(token, args)
            }
        } else {
            if !token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '.') {
                return Err(self.error("expected expression"));
            }
            Expr::Name(token)
        };
        loop {
            let op = self.peek();
            let precedence = match op {
                "||" => 1,
                "&&" => 2,
                "|" => 3,
                "^" => 4,
                "&" => 5,
                "==" | "!=" => 6,
                "<" | ">" | "<=" | ">=" => 7,
                "<<" | ">>" => 8,
                "+" | "-" => 9,
                "*" | "/" | "%" => 10,
                _ => break,
            };
            if precedence < min {
                break;
            }
            self.expression_nodes += 1;
            if self.expression_nodes > 512 {
                return Err(self.error("expression node limit exceeded"));
            }
            let op = self.take()?;
            let right = self.expression(precedence + 1, depth + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn assignment(&mut self, provide: bool) -> Result<Command> {
        if provide {
            self.expect("(")?;
        }
        let name = self.name()?;
        let add = self.eat("+=");
        if !add {
            self.expect("=")?;
        }
        let mut expr = self.expr(0)?;
        if add {
            expr = Expr::Binary(
                "+".into(),
                Box::new(Expr::Name(name.clone())),
                Box::new(expr),
            );
        }
        if provide {
            self.expect(")")?;
        }
        self.expect(";")?;
        Ok(Command::Assign(Assignment {
            name,
            expr,
            provide,
        }))
    }
    fn selector(&mut self) -> Result<Command> {
        let keep = self.eat("KEEP");
        if keep {
            self.expect("(")?;
        }
        self.expect("*")?;
        self.expect("(")?;
        let mut patterns = Vec::new();
        while !self.eat(")") {
            let sort = self.eat("SORT") || self.eat("SORT_BY_NAME");
            if sort {
                self.expect("(")?;
            }
            let mut pattern = self.take()?;
            // Wildcards and '+' are separate expression tokens. Join only
            // adjacent tokens (the lexer records whitespace as pattern breaks).
            while self.eat("@join") {
                pattern.push_str(&self.take()?);
            }
            if !pattern
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._*?+-/".contains(c))
            {
                return Err(self.error("unsupported section wildcard"));
            }
            if sort {
                self.expect(")")?;
            }
            patterns.push((pattern, sort));
        }
        if keep {
            self.expect(")")?;
        }
        if patterns.len() > 1 && patterns.iter().any(|(_, sort)| *sort) {
            return Err(self.error("sorting requires one pattern per input selector"));
        }
        Ok(Command::Input(patterns))
    }
    fn commands(&mut self, body: bool) -> Result<Vec<Command>> {
        let mut commands = Vec::new();
        while self.peek() != "}" && !self.peek().is_empty() {
            if self.eat(";") {
                continue;
            }
            if self.peek() == "ASSERT" {
                commands.push(Command::Assert(self.expr(0)?));
                self.eat(";");
            } else if self.eat("PROVIDE") {
                commands.push(self.assignment(true)?);
            } else if matches!(self.peek(), "*" | "KEEP") {
                commands.push(self.selector()?);
            } else if self.eat("CONSTRUCTORS") {
                // GNU documents this as ignored for ELF. ELF constructor arrays
                // are ordinary input sections and must be selected explicitly.
            } else if self
                .tokens
                .get(self.at + 1)
                .is_some_and(|s| s == "=" || s == "+=")
            {
                commands.push(self.assignment(false)?);
            } else if !body {
                let name = self.name()?;
                let mut info = false;
                let address = if self.peek() == ":" {
                    None
                } else if self.peek() == "("
                    && self.tokens.get(self.at + 1).is_some_and(|s| s == "INFO")
                {
                    self.expect("(")?;
                    self.expect("INFO")?;
                    self.expect(")")?;
                    info = true;
                    None
                } else {
                    Some(self.expr(0)?)
                };
                self.expect(":")?;
                let load = if self.eat("AT") {
                    self.expect("(")?;
                    let e = self.expr(0)?;
                    self.expect(")")?;
                    Some(e)
                } else {
                    None
                };
                self.expect("{")?;
                let body = self.commands(true)?;
                self.expect("}")?;
                let mut phdrs = Vec::new();
                while self.eat(":") {
                    phdrs.push(self.name()?);
                }
                let fill = if self.eat("=") {
                    Some(self.expr(0)?)
                } else {
                    None
                };
                commands.push(Command::Output(Output {
                    name,
                    address,
                    load,
                    body,
                    phdrs,
                    fill,
                    info,
                }));
            } else {
                return Err(self.error("unsupported output-section command"));
            }
        }
        Ok(commands)
    }
}

impl Script {
    pub fn parse(text: &str) -> Result<Self> {
        let mut p = Parser {
            tokens: lex(text)?,
            at: 0,
            expression_nodes: 0,
        };
        // The lexer preserves wildcard adjacency only inside input selectors.
        let mut script = Self {
            entry: None,
            commands: Vec::new(),
            phdrs: Vec::new(),
        };
        while !p.peek().is_empty() {
            match p.peek() {
                "OUTPUT_FORMAT" | "OUTPUT_ARCH" | "ENTRY" => {
                    let command = p.take()?;
                    p.expect("(")?;
                    let mut argument = String::new();
                    while p.peek() != ")" {
                        argument.push_str(&p.take()?);
                    }
                    p.expect(")")?;
                    p.eat(";");
                    match command.as_str() {
                        "OUTPUT_FORMAT" if argument.trim_matches('"') == "elf64-x86-64" => {}
                        "OUTPUT_ARCH" if argument == "i386:x86-64" => {}
                        "ENTRY" if !argument.is_empty() => script.entry = Some(argument),
                        _ => return Err(p.error("unsupported output format or architecture")),
                    }
                }
                "PHDRS" => {
                    p.take()?;
                    p.expect("{")?;
                    while !p.eat("}") {
                        let name = p.name()?;
                        if script.phdrs.iter().any(|h| h.name == name) {
                            return Err(p.error("duplicate PHDR"));
                        }
                        let kind = match p.take()?.as_str() {
                            "PT_LOAD" => 1,
                            "PT_NOTE" => 4,
                            _ => return Err(p.error("unsupported PHDR type")),
                        };
                        p.expect("FLAGS")?;
                        p.expect("(")?;
                        let Expr::Number(flags) = p.expr(0)? else {
                            return Err(p.error("PHDR FLAGS requires a constant"));
                        };
                        if flags > 7 {
                            return Err(p.error("unsupported PHDR flags"));
                        }
                        p.expect(")")?;
                        p.expect(";")?;
                        script.phdrs.push(Phdr {
                            name,
                            kind,
                            flags: flags as u32,
                        });
                    }
                }
                "SECTIONS" => {
                    p.take()?;
                    p.expect("{")?;
                    script.commands.extend(p.commands(false)?);
                    p.expect("}")?;
                }
                "PROVIDE" => {
                    p.take()?;
                    script.commands.push(p.assignment(true)?);
                }
                ";" => {
                    p.take()?;
                }
                _ => script.commands.push(p.assignment(false)?),
            }
        }
        Ok(script)
    }
}

fn lex(text: &str) -> Result<Vec<String>> {
    if !text.is_ascii() || text.len() > 1024 * 1024 {
        return Err("linker scripts require ASCII text of at most 1 MiB".into());
    }
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut tokens: Vec<String> = Vec::new();
    let mut selector_depth = None;
    let mut depth = 0usize;
    let mut adjacent = false;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            adjacent = false;
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            let tail = text[at + 2..]
                .find("*/")
                .ok_or("unterminated script comment")?;
            at += tail + 4;
            adjacent = false;
            continue;
        }
        let start = at;
        if bytes[at] == b'"' {
            at += 1;
            while at < bytes.len() && bytes[at] != b'"' {
                if bytes[at] == b'\\' {
                    return Err("script string escapes are unsupported".into());
                }
                at += 1;
            }
            if at == bytes.len() {
                return Err("unterminated script string".into());
            }
            at += 1;
        } else if bytes[at].is_ascii_alphanumeric() || b"._".contains(&bytes[at]) {
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || b"._".contains(&bytes[at]))
            {
                at += 1;
            }
        } else if bytes[at..].starts_with(b"/DISCARD/") {
            at += 9;
        } else {
            at += 1;
            if at < bytes.len()
                && matches!(
                    &text[start..at + 1],
                    "<<" | ">>" | "<=" | ">=" | "==" | "!=" | "||" | "&&" | "+="
                )
            {
                at += 1;
            }
        }
        let token = &text[start..at];
        if token == "(" {
            if tokens.last().is_some_and(|s| s == "*") {
                selector_depth = Some(depth + 1);
            }
            depth += 1;
        }
        if selector_depth.is_some()
            && adjacent
            && !matches!(token, "(" | ")")
            && tokens
                .last()
                .is_some_and(|s| !matches!(s.as_str(), "(" | ")"))
        {
            tokens.push("@join".into());
        }
        tokens.push(token.to_owned());
        if token == ")" {
            if selector_depth == Some(depth) {
                selector_depth = None;
            }
            depth = depth
                .checked_sub(1)
                .ok_or("unbalanced script parentheses")?;
        }
        adjacent = true;
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_syntax() {
        for text in [
            "MEMORY { ram : ORIGIN = 0, LENGTH = 1 }",
            "SECTIONS { .text : { BYTE(1) } }",
            "OUTPUT_ARCH(aarch64)",
            "SECTIONS { .text : { *(EXCLUDE_FILE(foo) .text) } }",
        ] {
            assert!(Script::parse(text).is_err(), "{text}");
        }
    }
    #[test]
    fn parses_sort_and_globs() {
        let script = Script::parse(
            "SECTIONS { .text : { KEEP(*(SORT(___ksymtab+*))) *(.text .text.*) } = 0xcccccccc }",
        )
        .unwrap();
        let Command::Output(out) = &script.commands[0] else {
            panic!()
        };
        let Command::Input(patterns) = &out.body[0] else {
            panic!()
        };
        assert_eq!(patterns, &[("___ksymtab+*".into(), true)]);
    }
}
