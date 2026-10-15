//! Fused source-line and comment scanning. Blocks span instruction boundaries,
//! so short lines reuse the delimiter mask instead of starting another search.

use crate::Error;
use bumpalo::{Bump, collections::String as BumpString};

pub(super) struct Lines<'a> {
    arena: &'a Bump,
    line: usize,
    source: &'a str,
    position: usize,
    line_start: usize,
    block: usize,
    mask: u64,
}

impl<'a> Lines<'a> {
    pub(super) fn new(source: &'a str, arena: &'a Bump) -> Self {
        Self {
            arena,
            line: 1,
            source,
            position: 0,
            line_start: 0,
            block: 0,
            mask: 0,
        }
    }

    fn delimiter(&mut self) -> Option<usize> {
        while self.mask == 0 {
            if self.position == self.source.len() {
                return None;
            }
            self.block = self.position;
            let bytes = &self.source.as_bytes()[self.position..];
            let length = bytes.len().min(16);
            self.mask = delimiters(bytes);
            self.position += length;
        }
        let shift = self.mask.trailing_zeros();
        self.mask &= !(0xf_u64 << shift);
        Some(self.block + shift as usize / 4)
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Result<(usize, &'a str), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut start = self.line_start;
        if start == self.source.len() {
            return None;
        }
        let mut line = self.line;
        let bytes = self.source.as_bytes();
        let mut scratch = None;
        // Compiler full-line comments skip their bodies with a SIMD search.
        if bytes[start] == b';' || (bytes[start] == b'/' && bytes.get(start + 1) == Some(&b'/')) {
            let text = self.comment(start, start);
            return Some(Ok((line, text)));
        }
        let mut quoted = false;
        let mut escaped = None;
        while let Some(index) = self.delimiter() {
            match bytes[index] {
                b'\n' | b'\r' => {
                    let next = self.after_newline(index);
                    self.seek(next);
                    let text = self.finish_line(start, index, next);
                    return Some(Ok((line, finish(scratch, text))));
                }
                _ if escaped == Some(index) => {}
                b'\\' if quoted => escaped = Some(index + 1),
                b'"' => quoted = !quoted,
                b';' if !quoted => {
                    let text = self.comment(start, index);
                    return Some(Ok((line, finish(scratch, text))));
                }
                b'/' if !quoted && bytes.get(index + 1) == Some(&b'/') => {
                    let text = self.comment(start, index);
                    return Some(Ok((line, finish(scratch, text))));
                }
                b'/' if !quoted && bytes.get(index + 1) == Some(&b'*') => {
                    let Some(offset) = memchr::memmem::find(&bytes[index + 2..], b"*/") else {
                        self.seek(bytes.len());
                        self.line_start = bytes.len();
                        return Some(Err(Error::new(self.line, "unterminated block comment")));
                    };
                    let end = index + 2 + offset + 2;
                    let out = scratch.get_or_insert_with(|| BumpString::new_in(self.arena));
                    out.push_str(&self.source[start..index]);
                    out.push(' ');
                    // A block comment is whitespace even across physical lines;
                    // keep those lines in diagnostics, not statement boundaries.
                    self.line += newline_count(&bytes[index..end]);
                    if out.trim().is_empty() {
                        line = self.line;
                    }
                    start = end;
                    self.seek(end);
                }
                _ => {}
            }
        }
        let text = self.finish_line(start, bytes.len(), bytes.len());
        Some(Ok((line, finish(scratch, text))))
    }
}

fn finish<'a>(scratch: Option<BumpString<'a>>, tail: &'a str) -> &'a str {
    if let Some(mut text) = scratch {
        text.push_str(tail);
        text.into_bump_str()
    } else {
        tail
    }
}

fn newline_count(bytes: &[u8]) -> usize {
    memchr::memchr2_iter(b'\r', b'\n', bytes)
        .filter(|&index| bytes[index] == b'\r' || index == 0 || bytes[index - 1] != b'\r')
        .count()
}

impl<'a> Lines<'a> {
    fn after_newline(&self, index: usize) -> usize {
        index
            + 1
            + usize::from(
                self.source.as_bytes()[index] == b'\r'
                    && self.source.as_bytes().get(index + 1) == Some(&b'\n'),
            )
    }

    fn seek(&mut self, next: usize) {
        if next >= self.position {
            self.position = next;
            self.mask = 0;
        } else {
            let consumed = (next - self.block) * 4;
            self.mask &= u64::MAX << consumed;
        }
    }

    fn finish_line(&mut self, start: usize, end: usize, next: usize) -> &'a str {
        self.line_start = next;
        if next > end {
            self.line += 1;
        }
        &self.source[start..end]
    }

    fn comment(&mut self, start: usize, end: usize) -> &'a str {
        let newline = memchr::memchr2(b'\r', b'\n', &self.source.as_bytes()[end..])
            .map(|offset| end + offset);
        let next = newline.map_or(self.source.len(), |index| self.after_newline(index));
        self.seek(next);
        self.line_start = next;
        self.line += usize::from(newline.is_some());
        &self.source[start..end]
    }
}

fn delimiters_scalar(bytes: &[u8]) -> u64 {
    let mut mask = 0;
    for (index, byte) in bytes.iter().take(16).enumerate() {
        if matches!(byte, b'\n' | b'\r' | b';' | b'/' | b'"' | b'\\') {
            mask |= 0xf_u64 << (index * 4);
        }
    }
    mask
}

#[inline]
fn delimiters(bytes: &[u8]) -> u64 {
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    if bytes.len() >= 16 {
        // SAFETY: This target enables NEON. The length check covers the entire
        // unaligned load; no read crosses the source allocation. Each matched
        // byte becomes one nibble, in source order on little-endian targets.
        #[cfg(target_endian = "little")]
        unsafe {
            use std::arch::aarch64::*;
            let input = vld1q_u8(bytes.as_ptr());
            let mut matches = vceqq_u8(input, vdupq_n_u8(b'\n'));
            for byte in *b"\r;/\"\\" {
                matches = vorrq_u8(matches, vceqq_u8(input, vdupq_n_u8(byte)));
            }
            return vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(
                vreinterpretq_u16_u8(matches),
            )));
        }
    }
    delimiters_scalar(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Original parser semantics, deliberately independent of the block scanner.
    fn uncomment(source: &str) -> &str {
        let bytes = source.as_bytes();
        let mut quoted = false;
        let mut index = 0;
        while index < bytes.len() {
            if quoted && bytes[index] == b'\\' {
                index += 2;
                continue;
            }
            if bytes[index] == b'"' {
                quoted = !quoted;
            }
            if !quoted
                && (bytes[index] == b';'
                    || (bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/')))
            {
                return &source[..index];
            }
            index += 1;
        }
        source
    }

    fn check(source: &str) {
        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        let expected: Vec<_> = normalized.lines().map(uncomment).collect();
        let arena = Bump::new();
        let actual: Vec<_> = Lines::new(source, &arena)
            .map(|line| line.unwrap().1)
            .collect();
        assert_eq!(actual, expected, "{source:?}");
    }

    #[test]
    fn block_comments_cross_vector_boundaries_and_preserve_physical_lines() {
        for offset in 0..16 {
            for length in 0..65 {
                let padding = " ".repeat(offset);
                let body = "λ".repeat(length);
                let source = format!(
                    "{padding}mov x0,/*{body}\r\n; // \"\r*/x1/*tail*/\r\nret// /*ignored\nnop"
                );
                let arena = Bump::new();
                let lines = Lines::new(&source, &arena)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                assert_eq!(
                    lines,
                    [
                        (1, format!("{padding}mov x0, x1 ").as_str()),
                        (4, "ret"),
                        (5, "nop")
                    ]
                );

                let source = format!("{padding}/*{body}\n*/ /*unterminated");
                let arena = Bump::new();
                let error = Lines::new(&source, &arena).next().unwrap().unwrap_err();
                assert_eq!(error.line, 2);
            }
        }
    }

    #[test]
    fn every_byte_and_lane_matches_scalar_mask() {
        for offset in 0..16 {
            for length in 0..65 {
                let mut bytes = vec![b'a'; offset + length];
                for index in offset..bytes.len() {
                    for byte in 0..=255 {
                        bytes[index] = byte;
                        assert_eq!(
                            delimiters(&bytes[offset..]),
                            delimiters_scalar(&bytes[offset..])
                        );
                    }
                    bytes[index] = b'a';
                }
                bytes.fill(b';');
                assert_eq!(
                    delimiters(&bytes[offset..]),
                    delimiters_scalar(&bytes[offset..])
                );
            }
        }
    }

    #[test]
    fn exhaustive_short_lines_match_original() {
        fn visit(source: &mut String, depth: usize) {
            check(source);
            if depth == 0 {
                return;
            }
            for token in ["a", "\n", "\r", ";", "/", "\"", "\\", "é", "界"] {
                let length = source.len();
                source.push_str(token);
                visit(source, depth - 1);
                source.truncate(length);
            }
        }
        visit(&mut String::new(), 5);
    }

    #[test]
    fn delimiters_escapes_and_line_endings_at_block_boundaries() {
        for offset in 0..16 {
            for length in 0..65 {
                for tail in [
                    ";\"ignored\\\nnext\r\n",
                    "//ignored\n\nend\r",
                    "/\nnext",
                    "\"; // quoted\\\"still quoted\";comment\r\nnext",
                    "\"\\\\\"//comment\nnext",
                    "\"\\é\";comment\nnext",
                    "\"\\\n;comment\nnext",
                    "\r\n",
                    "\r",
                    "",
                    "\\",
                    "\"",
                ] {
                    let source = format!("{}{}{tail}", "x".repeat(offset), "a".repeat(length));
                    check(&source[offset..]);
                }
            }
        }
    }

    #[test]
    fn randomized_multiblock_sources_match_original() {
        let mut seed = 42_u64;
        for _ in 0..5000 {
            let mut source = String::new();
            for _ in 0..512 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                source.push_str(match seed % 32 {
                    0 => "\n",
                    1 => "\r",
                    2 => ";",
                    3 => "/",
                    4 => "\"",
                    5 => "\\",
                    6 => "界",
                    7 => "\u{2003}",
                    8 => "\t",
                    _ => "a",
                });
            }
            check(&source);
        }
    }
}
