//! The YAML subset that TAPI writes for text-based stubs: documents of block
//! mappings and block sequences, flow sequences and mappings, plain and quoted
//! scalars, and comments. Scalars borrow from the input. Anchors, aliases,
//! tags other than a document tag, block scalars, multi-line scalars and
//! escapes in double-quoted scalars are rejected rather than misread.
//!
//! Plain scalars resolve to types the way `serde_yaml` resolves them: null,
//! booleans, integers and floats are not strings.
use std::borrow::Cow;

#[derive(Debug)]
pub(super) enum Node<'a> {
    /// An unquoted null, boolean or number.
    Plain(&'a str),
    /// A quoted scalar or an unquoted string.
    Str(Cow<'a, str>),
    Seq(Vec<Node<'a>>),
    Map(Vec<(Cow<'a, str>, Node<'a>)>),
}

static NULL: Node<'static> = Node::Plain("");

/// Resolve an unquoted scalar's type.
fn plain(text: &str) -> Node<'_> {
    if plain_string(text) {
        Node::Str(Cow::Borrowed(text))
    } else {
        Node::Plain(text)
    }
}

impl<'a> Node<'a> {
    /// The value of `key` in a mapping, or null.
    pub(super) fn get(&self, key: &str) -> &Node<'a> {
        match self {
            Node::Map(entries) => entries
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }
    pub(super) fn is_null(&self) -> bool {
        matches!(self, Node::Plain(s) if null(s))
    }
    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Node::Str(s) => Some(s),
            _ => None,
        }
    }
    pub(super) fn as_seq(&self) -> Option<&[Node<'a>]> {
        match self {
            Node::Seq(items) => Some(items),
            _ => None,
        }
    }
    pub(super) fn as_u64(&self) -> Option<u64> {
        match self {
            Node::Plain(s) => unsigned(s, u64::from_str_radix),
            _ => None,
        }
    }
}

fn null(s: &str) -> bool {
    matches!(s, "" | "~" | "null" | "Null" | "NULL")
}

fn plain_string(s: &str) -> bool {
    if null(s) || matches!(s, "true" | "True" | "TRUE" | "false" | "False" | "FALSE") {
        return false;
    }
    // Every numeric form starts with a digit, a sign or a dot.
    if !s.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.')) {
        return true;
    }
    unsigned(s, u128::from_str_radix).is_none()
        && negative(s).is_none()
        && (leading_zero_digits(s) || float(s).is_none())
}

/// Leading zeros followed by digits form a string in YAML 1.2.
fn leading_zero_digits(s: &str) -> bool {
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    s.len() > 1 && s.starts_with('0') && s[1..].bytes().all(|b| b.is_ascii_digit())
}

fn unsigned<T>(s: &str, radix: fn(&str, u32) -> Result<T, std::num::ParseIntError>) -> Option<T> {
    let unsigned = s.strip_prefix('+').unwrap_or(s);
    for (prefix, base) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(rest) = unsigned.strip_prefix(prefix) {
            if rest.starts_with(['+', '-']) {
                return None;
            }
            if let Ok(value) = radix(rest, base) {
                return Some(value);
            }
        }
    }
    if unsigned.starts_with(['+', '-']) || leading_zero_digits(s) {
        return None;
    }
    radix(unsigned, 10).ok()
}

fn negative(s: &str) -> Option<i128> {
    for (prefix, base) in [("-0x", 16), ("-0o", 8), ("-0b", 2)] {
        if let Some(rest) = s.strip_prefix(prefix)
            && let Ok(value) = i128::from_str_radix(&format!("-{rest}"), base)
        {
            return Some(value);
        }
    }
    if leading_zero_digits(s) {
        return None;
    }
    s.parse().ok()
}

fn float(s: &str) -> Option<f64> {
    let unsigned = match s.strip_prefix('+') {
        Some(rest) if rest.starts_with(['+', '-']) => return None,
        Some(rest) => rest,
        None => s,
    };
    if matches!(unsigned, ".inf" | ".Inf" | ".INF")
        || matches!(s, "-.inf" | "-.Inf" | "-.INF" | ".nan" | ".NaN" | ".NAN")
    {
        return Some(0.0);
    }
    unsigned.parse::<f64>().ok().filter(|f| f.is_finite())
}

/// Parse every document in `text`. A document's top-level tag is dropped.
pub(super) fn documents(text: &str) -> Result<Vec<Node<'_>>, String> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.documents().map_err(|e| {
        let line = 1 + text.as_bytes()[..parser.pos.min(text.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        format!("line {line}: {e}")
    })
}

/// `text` without trailing spaces, tabs and carriage returns.
fn trim(text: &str) -> &str {
    let mut end = text.len();
    while end > 0 && matches!(text.as_bytes()[end - 1], b' ' | b'\t' | b'\r') {
        end -= 1;
    }
    &text[..end]
}

/// Bytes that can end a plain scalar in a flow collection.
static FLOW_STOP: [bool; 256] = {
    let mut stop = [false; 256];
    let mut i = 0;
    let delimiters = b"\n,[]{}:#";
    while i < delimiters.len() {
        stop[delimiters[i] as usize] = true;
        i += 1;
    }
    stop
};

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

type Parse<T> = Result<T, String>;

fn unsupported<T>(what: &str) -> Parse<T> {
    Err(format!("unsupported YAML {what}"))
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }
    fn at_line_end(&self) -> bool {
        matches!(self.peek(), None | Some(b'\n'))
    }
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r')) {
            self.pos += 1;
        }
    }
    fn skip_to_line_end(&mut self) {
        while !self.at_line_end() {
            self.pos += 1;
        }
    }
    /// After a value: allow trailing spaces and a comment, then consume the newline.
    fn finish_line(&mut self) -> Parse<()> {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            self.skip_to_line_end();
        }
        if !self.at_line_end() {
            return Err("unexpected content after value".into());
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
        Ok(())
    }
    /// Move to the start of the next line with content, and return its
    /// indentation, or `None` at the end of input.
    fn next_line(&mut self) -> Parse<Option<usize>> {
        loop {
            let start = self.pos;
            if start >= self.bytes.len() {
                return Ok(None);
            }
            let mut i = start;
            while self.bytes.get(i) == Some(&b' ') {
                i += 1;
            }
            match self.bytes.get(i) {
                None => {
                    self.pos = i;
                    return Ok(None);
                }
                Some(b'\n') => self.pos = i + 1,
                Some(b'\r') if self.bytes.get(i + 1) == Some(&b'\n') => self.pos = i + 2,
                Some(b'#') => {
                    self.pos = i;
                    self.skip_to_line_end();
                }
                Some(b'\t') => {
                    self.pos = i;
                    return unsupported("tab indentation");
                }
                Some(_) => {
                    self.pos = i;
                    return Ok(Some(i - start));
                }
            }
        }
    }
    /// A `---` or `...` marker at the current line start.
    fn marker(&self, marker: &[u8]) -> bool {
        self.bytes[self.pos..].starts_with(marker)
            && matches!(self.at(3), None | Some(b' ' | b'\t' | b'\r' | b'\n'))
    }

    fn documents(&mut self) -> Parse<Vec<Node<'a>>> {
        let mut documents = Vec::new();
        if self.text.starts_with('\u{feff}') {
            self.pos = 3;
        }
        while let Some(indent) = self.next_line()? {
            if indent == 0 && self.peek() == Some(b'%') {
                return unsupported("directive");
            }
            if indent == 0 && self.marker(b"---") {
                self.pos += 3;
                self.skip_spaces();
                if self.peek() == Some(b'!') {
                    while !matches!(self.peek(), None | Some(b' ' | b'\t' | b'\r' | b'\n')) {
                        self.pos += 1;
                    }
                }
                self.finish_line()?;
            }
            documents.push(self.block(-1, false)?);
            match self.next_line()? {
                None => break,
                Some(0) if self.marker(b"...") => {
                    self.pos += 3;
                    self.finish_line()?;
                }
                Some(0) if self.marker(b"---") => {}
                Some(_) => return Err("unexpected content after document".into()),
            }
        }
        if documents.is_empty() {
            // An empty stream holds one null document.
            documents.push(Node::Plain(""));
        }
        Ok(documents)
    }

    /// A block node more indented than `parent`, or a block sequence at the
    /// parent's indentation when `sequence_at_parent` (a mapping value).
    /// Null when no such node follows.
    fn block(&mut self, parent: isize, sequence_at_parent: bool) -> Parse<Node<'a>> {
        let Some(indent) = self.next_line()? else {
            return Ok(Node::Plain(""));
        };
        let line = self.pos - indent;
        if indent == 0 && (self.marker(b"---") || self.marker(b"...")) {
            self.pos = line;
            return Ok(Node::Plain(""));
        }
        let sequence = self.sequence_item();
        if (indent as isize) > parent
            || (sequence_at_parent && sequence && indent as isize == parent)
        {
            if sequence {
                return self.sequence(indent);
            }
            if self.mapping_key()?.is_some() {
                return self.mapping(indent);
            }
            let node = self.inline()?;
            self.finish_line()?;
            return Ok(node);
        }
        self.pos = line;
        Ok(Node::Plain(""))
    }

    fn sequence_item(&self) -> bool {
        self.peek() == Some(b'-') && matches!(self.at(1), None | Some(b' ' | b'\r' | b'\n'))
    }

    /// The plain key at the current position, if the line holds `key:`.
    /// Leaves the position unchanged.
    fn mapping_key(&self) -> Parse<Option<(&'a str, usize)>> {
        if matches!(
            self.peek(),
            None | Some(
                b'[' | b']'
                    | b'{'
                    | b'}'
                    | b','
                    | b'\''
                    | b'"'
                    | b'!'
                    | b'&'
                    | b'*'
                    | b'|'
                    | b'>'
                    | b'%'
                    | b'@'
                    | b'`'
                    | b'#'
                    | b'?'
            )
        ) {
            return Ok(None);
        }
        let mut i = self.pos;
        while let Some(&b) = self.bytes.get(i) {
            match b {
                b'\n' => break,
                b'#' if matches!(self.bytes[i - 1], b' ' | b'\t') => break,
                b':' if matches!(
                    self.bytes.get(i + 1),
                    None | Some(b' ' | b'\t' | b'\r' | b'\n')
                ) =>
                {
                    let key = trim(&self.text[self.pos..i]);
                    return Ok(Some((key, i + 1)));
                }
                _ => i += 1,
            }
        }
        Ok(None)
    }

    fn mapping(&mut self, indent: usize) -> Parse<Node<'a>> {
        let mut entries: Vec<(Cow<'a, str>, Node<'a>)> = Vec::new();
        loop {
            let key = if self.sequence_item() {
                None
            } else {
                self.mapping_key()?
            };
            let Some((key, after)) = key else {
                return Err("expected a mapping key".into());
            };
            if entries.iter().any(|(k, _)| k == key) {
                return Err(format!("duplicate mapping key `{key}`"));
            }
            self.pos = after;
            self.skip_spaces();
            let value = if matches!(self.peek(), None | Some(b'\n' | b'#')) {
                self.finish_line()?;
                self.block(indent as isize, true)?
            } else {
                let value = self.inline()?;
                self.finish_line()?;
                value
            };
            entries.push((Cow::Borrowed(key), value));
            let Some(next) = self.next_line()? else {
                break;
            };
            if next < indent || (next == 0 && (self.marker(b"---") || self.marker(b"..."))) {
                self.pos -= next;
                break;
            }
            if next > indent {
                return unsupported("indentation or multi-line scalar");
            }
        }
        Ok(Node::Map(entries))
    }

    fn sequence(&mut self, indent: usize) -> Parse<Node<'a>> {
        let mut items = Vec::new();
        loop {
            self.pos += 1;
            self.skip_spaces();
            let column = self.pos - self.line_start();
            let item = if matches!(self.peek(), None | Some(b'\n' | b'#')) {
                self.finish_line()?;
                self.block(indent as isize, false)?
            } else if self.sequence_item() {
                return unsupported("compact nested sequence");
            } else if self.mapping_key()?.is_some() {
                self.mapping(column)?
            } else {
                let item = self.inline()?;
                self.finish_line()?;
                item
            };
            items.push(item);
            let Some(next) = self.next_line()? else {
                break;
            };
            if next == indent && self.sequence_item() {
                continue;
            }
            if next > indent {
                return unsupported("indentation or multi-line scalar");
            }
            self.pos -= next;
            break;
        }
        Ok(Node::Seq(items))
    }

    fn line_start(&self) -> usize {
        self.bytes[..self.pos]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1)
    }

    /// A flow collection, quoted scalar or plain scalar on the current line.
    fn inline(&mut self) -> Parse<Node<'a>> {
        match self.peek() {
            Some(b'[' | b'{' | b'\'' | b'"') => self.flow(),
            Some(b'!' | b'&' | b'*' | b'|' | b'>' | b'%' | b'@' | b'`') => {
                unsupported("anchor, alias, tag or block scalar")
            }
            _ => {
                let start = self.pos;
                while !self.at_line_end()
                    && !(self.peek() == Some(b'#')
                        && matches!(self.bytes[self.pos - 1], b' ' | b'\t'))
                {
                    self.pos += 1;
                }
                let text = trim(&self.text[start..self.pos]);
                if text.contains(": ") || text.ends_with(':') {
                    return unsupported("mapping in a plain scalar");
                }
                Ok(plain(text))
            }
        }
    }

    /// Spaces, line breaks and comments inside a flow collection.
    #[inline]
    fn skip_flow_space(&mut self) {
        loop {
            // Continuation lines of long sequences start with runs of spaces.
            while self.bytes.get(self.pos..self.pos + 8) == Some(b"        ") {
                self.pos += 8;
            }
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.pos += 1,
                Some(b'#') => self.skip_to_line_end(),
                _ => return,
            }
        }
    }

    fn flow(&mut self) -> Parse<Node<'a>> {
        match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_flow_space();
                    if self.peek() == Some(b']') {
                        self.pos += 1;
                        return Ok(Node::Seq(items));
                    }
                    items.push(match self.peek() {
                        Some(b'[' | b'{') => self.flow()?,
                        _ => self.flow_scalar()?,
                    });
                    self.skip_flow_space();
                    if self.peek() == Some(b':') {
                        return unsupported("mapping in a flow sequence");
                    }
                    self.flow_separator(b']')?;
                    if self.bytes[self.pos - 1] == b']' {
                        return Ok(Node::Seq(items));
                    }
                }
            }
            Some(b'{') => {
                self.pos += 1;
                let mut entries: Vec<(Cow<'a, str>, Node<'a>)> = Vec::new();
                loop {
                    self.skip_flow_space();
                    if self.peek() == Some(b'}') {
                        self.pos += 1;
                        return Ok(Node::Map(entries));
                    }
                    let key = match self.flow()? {
                        Node::Plain(key) => Cow::Borrowed(key),
                        Node::Str(key) => key,
                        _ => return unsupported("collection key"),
                    };
                    if entries.iter().any(|(k, _)| *k == key) {
                        return Err(format!("duplicate mapping key `{key}`"));
                    }
                    self.skip_flow_space();
                    if self.peek() != Some(b':') {
                        return Err("expected `:` in a flow mapping".into());
                    }
                    self.pos += 1;
                    self.skip_flow_space();
                    let value = if matches!(self.peek(), Some(b',' | b'}')) {
                        Node::Plain("")
                    } else {
                        self.flow()?
                    };
                    entries.push((key, value));
                    self.skip_flow_space();
                    self.flow_separator(b'}')?;
                    if self.bytes[self.pos - 1] == b'}' {
                        return Ok(Node::Map(entries));
                    }
                }
            }
            _ => self.flow_scalar(),
        }
    }

    /// A quoted or plain scalar inside a flow collection.
    fn flow_scalar(&mut self) -> Parse<Node<'a>> {
        match self.peek() {
            Some(b'\'') => {
                self.pos += 1;
                let mut start = self.pos;
                let mut owned: Option<String> = None;
                loop {
                    self.pos += self.bytes[self.pos..]
                        .iter()
                        .position(|&b| b == b'\'' || b == b'\n')
                        .unwrap_or(self.bytes.len() - self.pos);
                    if self.peek() != Some(b'\'') {
                        return unsupported("multi-line quoted scalar");
                    }
                    if self.at(1) == Some(b'\'') {
                        // `''` is an escaped quote: keep one.
                        owned
                            .get_or_insert_with(String::new)
                            .push_str(&self.text[start..=self.pos]);
                        self.pos += 2;
                        start = self.pos;
                        continue;
                    }
                    let rest = &self.text[start..self.pos];
                    self.pos += 1;
                    return Ok(Node::Str(match owned {
                        Some(mut text) => {
                            text.push_str(rest);
                            Cow::Owned(text)
                        }
                        None => Cow::Borrowed(rest),
                    }));
                }
            }
            Some(b'"') => {
                self.pos += 1;
                let start = self.pos;
                loop {
                    match self.peek() {
                        None | Some(b'\n') => return unsupported("multi-line quoted scalar"),
                        Some(b'\\') => return unsupported("escape sequence"),
                        Some(b'"') => {
                            self.pos += 1;
                            return Ok(Node::Str(Cow::Borrowed(&self.text[start..self.pos - 1])));
                        }
                        Some(_) => self.pos += 1,
                    }
                }
            }
            Some(b'!' | b'&' | b'*' | b'|' | b'>' | b'%' | b'@' | b'`') => {
                unsupported("anchor, alias, tag or block scalar")
            }
            _ => {
                let start = self.pos;
                loop {
                    // Skip ordinary bytes without matching each delimiter.
                    self.pos += self.bytes[self.pos..]
                        .iter()
                        .position(|&b| FLOW_STOP[usize::from(b)])
                        .unwrap_or(self.bytes.len() - self.pos);
                    match self.peek() {
                        None | Some(b'\n' | b',' | b'[' | b']' | b'{' | b'}') => break,
                        Some(b':')
                            if matches!(
                                self.at(1),
                                None | Some(b' ' | b'\t' | b'\r' | b'\n' | b',' | b']' | b'}')
                            ) =>
                        {
                            break;
                        }
                        Some(b'#') if matches!(self.bytes[self.pos - 1], b' ' | b'\t') => break,
                        Some(_) => self.pos += 1,
                    }
                }
                let text = trim(&self.text[start..self.pos]);
                if text.is_empty() {
                    return Err("expected a flow value".into());
                }
                if self.peek() == Some(b'\n') {
                    // A plain scalar continued on the next line is folded by YAML.
                    let end = self.pos;
                    self.skip_flow_space();
                    let next = self.peek();
                    self.pos = end;
                    if !matches!(next, Some(b',' | b']' | b'}' | b':')) {
                        return unsupported("multi-line plain scalar");
                    }
                }
                Ok(plain(text))
            }
        }
    }

    /// Consume `,` or the collection's `close` byte.
    fn flow_separator(&mut self, close: u8) -> Parse<()> {
        match self.peek() {
            Some(b',') => {
                self.pos += 1;
                Ok(())
            }
            Some(b) if b == close => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(format!("expected `,` or `{}`", close as char)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;
    use std::path::{Path, PathBuf};

    /// Whether `node` reads as `value` through every accessor the TBD reader uses.
    fn same(node: &Node<'_>, value: &Value) -> bool {
        match value {
            Value::Tagged(_) => false,
            Value::Sequence(items) => node.as_seq().is_some_and(|nodes| {
                nodes.len() == items.len() && nodes.iter().zip(items).all(|(n, v)| same(n, v))
            }),
            Value::Mapping(map) => match node {
                Node::Map(entries) => {
                    entries.len() == map.len()
                        && map.iter().all(|(k, v)| {
                            k.as_str()
                                .is_some_and(|k| entries.iter().any(|(nk, _)| nk == k))
                                && same(node.get(k.as_str().unwrap()), v)
                        })
                }
                _ => false,
            },
            _ => {
                !matches!(node, Node::Seq(_) | Node::Map(_))
                    && node.as_str() == value.as_str()
                    && node.is_null() == value.is_null()
                    && node.as_u64() == value.as_u64()
                    && matches!(node, Node::Plain(s)
                        if matches!(*s, "true" | "True" | "TRUE"))
                        == (value.as_bool() == Some(true))
            }
        }
    }

    /// Compare one file's documents with serde_yaml's reading of them.
    fn check(text: &str) -> Result<(), String> {
        let oracle: Result<Vec<Value>, _> = serde_yaml::Deserializer::from_str(text)
            .map(|document| {
                <Value as serde::Deserialize>::deserialize(document).map(|value| match value {
                    Value::Tagged(tagged) => tagged.value,
                    value => value,
                })
            })
            .collect();
        match (documents(text), oracle) {
            (Ok(nodes), Ok(values)) => {
                if nodes.len() != values.len() {
                    return Err(format!(
                        "{} documents, oracle {}",
                        nodes.len(),
                        values.len()
                    ));
                }
                for (i, (node, value)) in nodes.iter().zip(&values).enumerate() {
                    if !same(node, value) {
                        return Err(format!("document {i} differs: {node:?}"));
                    }
                }
                Ok(())
            }
            (Err(_), Err(_)) => Ok(()),
            (Ok(_), Err(e)) => Err(format!("accepted; oracle error {e}")),
            (Err(e), Ok(_)) => Err(format!("rejected ({e}); oracle accepted")),
        }
    }

    #[test]
    fn subset_matches_yaml_oracle() {
        for text in [
            "",
            "--- !tapi-tbd\ntbd-version: 4\n...\n",
            "a: [ x, 'y''z', \"q\", 42, 0x10, 010, -3, 1.5, .inf, ~, null, true, False ]\n",
            "a:\n  - b: [ c,\n       d ]\n    e: f\n  - g: ~\nh:\n- i\n- 'j'\n",
            "a: { b: [ c ], d: 1 }\nk: v # comment\n# full line\n",
            "--- !tapi-tbd\na: 1\n--- !tapi-tbd\nb: 2\n...\n",
            "a: []\nb:\nc: ''\n",
        ] {
            check(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        }
        for text in [
            "a: &x 1\n",
            "a: |\n  text\n",
            "a: b\n  c\n",
            "a: [ b\n  c ]\n",
            "a: 'b\n  c'\n",
            "a: \"\\n\"\n",
            "a: 1\na: 2\n",
            "\ta: 1\n",
        ] {
            assert!(documents(text).is_err(), "{text:?} was accepted");
        }
    }

    fn sdk() -> Option<PathBuf> {
        if let Some(root) = std::env::var_os("SDKROOT").filter(|s| !s.is_empty()) {
            return Some(PathBuf::from(root));
        }
        let out = std::process::Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .ok()
            .filter(|out| out.status.success())?;
        Some(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
    }

    fn tbd_files(directory: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                tbd_files(&path, files);
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "tbd") {
                files.push(path);
            }
        }
    }

    /// Check `files` on every core, and report each mismatch.
    fn check_files(files: &[PathBuf]) {
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
        let chunk = files.len().div_ceil(workers).max(1);
        let failures: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|files| {
                    scope.spawn(move || {
                        files
                            .iter()
                            .filter_map(|path| {
                                let text = std::fs::read_to_string(path).ok()?;
                                check(&text)
                                    .err()
                                    .map(|e| format!("{}: {e}", path.display()))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect()
        });
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn sdk_libsystem_matches_yaml_oracle() {
        let Some(sdk) = sdk() else {
            keel_test_support::skip!("no macOS SDK; set SDKROOT");
        };
        let path = sdk.join("usr/lib/libSystem.B.tbd");
        if !path.is_file() {
            keel_test_support::skip!("no libSystem.B.tbd in {}", sdk.display());
        }
        check_files(&[path]);
    }

    #[test]
    fn exhaustive_sdk_tbd_files_match_yaml_oracle() {
        let Some(sdk) = sdk().and_then(|sdk| sdk.canonicalize().ok()) else {
            keel_test_support::skip!("no macOS SDK; set SDKROOT");
        };
        let mut files = Vec::new();
        tbd_files(&sdk, &mut files);
        if files.is_empty() {
            keel_test_support::skip!("no .tbd files in {}", sdk.display());
        }
        check_files(&files);
    }
}
