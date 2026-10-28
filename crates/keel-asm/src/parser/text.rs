//! ASCII whitespace fast path with the existing Unicode trim semantics.
pub(super) trait Trim {
    fn trim_fast(&self) -> &str;
}
impl Trim for str {
    #[inline]
    fn trim_fast(&self) -> &str {
        let text = self.trim_ascii();
        // Rust's ASCII whitespace excludes vertical tab, while str::trim
        // includes it. Unicode and vertical-tab boundaries use the full trim.
        let needs_unicode = |b: u8| b >= 0x80 || b == 0x0b;
        if text.as_bytes().first().copied().is_some_and(needs_unicode)
            || text.as_bytes().last().copied().is_some_and(needs_unicode)
        {
            text.trim()
        } else {
            text
        }
    }
}

/// LLVM/GNU assembler integer spellings; signs are handled by the caller.
pub(super) fn integer_radix(digits: &str) -> (&str, u32) {
    if let Some(value) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (value, 16)
    } else if let Some(value) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (value, 2)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (&digits[1..], 8)
    } else {
        (digits, 10)
    }
}
#[cfg(test)]
mod tests {
    use super::Trim;
    #[test]
    fn trim_matches_unicode_semantics() {
        for edge in [
            "", " ", "\t\r\n", "\u{b}", "\u{85}", "\u{a0}", "\u{1680}", "\u{2000}", "\u{2028}",
            "\u{2029}", "\u{202f}", "\u{205f}", "\u{3000}", "λ", "中",
        ] {
            for middle in ["", "ret", " λ中 ", "\u{a0}\u{b}"] {
                let text = format!(" \t{edge}{middle}{edge}\n ");
                assert_eq!(text.trim_fast(), text.trim(), "{text:?}");
            }
        }
        for b in 0..=127u8 {
            let text = format!("{0}ret{0}", char::from(b));
            assert_eq!(text.trim_fast(), text.trim());
        }
    }
}
