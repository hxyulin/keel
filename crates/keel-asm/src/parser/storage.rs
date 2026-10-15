//! Small, borrowed storage for the common parser path. No syntax limits are
//! imposed by the inline capacities: unusually long inputs fall back to Vec.
use crate::FieldId;
use std::{borrow::Cow, fmt, fmt::Write, ops::Deref};

pub(super) fn lower(text: &str) -> Cow<'_, str> {
    if text.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(text.to_ascii_lowercase())
    } else {
        Cow::Borrowed(text)
    }
}

pub(super) struct Upper<T>(T);
pub(super) fn upper<T: AsRef<str>>(text: T) -> Upper<T> {
    Upper(text)
}
impl<T: AsRef<str>> fmt::Display for Upper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.0.as_ref();
        let mut bytes = [0; 128];
        if let Some(out) = bytes.get_mut(..text.len()) {
            out.copy_from_slice(text.as_bytes());
            out.make_ascii_uppercase();
            // SAFETY: `text` was valid UTF-8 and ASCII case conversion only
            // changes single-byte ASCII characters, preserving UTF-8 boundaries.
            return f.write_str(unsafe { std::str::from_utf8_unchecked(out) });
        }
        for c in text.chars() {
            f.write_char(c.to_ascii_uppercase())?;
        }
        Ok(())
    }
}

/// Encoding names are short and internal. Keep the existing encoding lookup
/// and all its field validation; only avoid allocating its temporary name.
pub(super) fn encode_named(
    name: fmt::Arguments<'_>,
    fields: &[(FieldId, u32)],
) -> Result<u32, String> {
    encode_named_features(name, fields, crate::TargetFeatures::default())
}
pub(super) fn encode_named_features(
    name: fmt::Arguments<'_>,
    fields: &[(FieldId, u32)],
    features: crate::TargetFeatures,
) -> Result<u32, String> {
    struct Name {
        bytes: [u8; 128],
        len: usize,
    }
    impl Write for Name {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            let end = self.len + text.len();
            let out = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
            out.copy_from_slice(text.as_bytes());
            self.len = end;
            Ok(())
        }
    }
    let mut buffer = Name {
        bytes: [0; 128],
        len: 0,
    };
    if buffer.write_fmt(name).is_err() {
        return crate::encode_with_features(&name.to_string(), fields, features);
    }
    // SAFETY: Name::write_str appends only complete &str fragments and updates
    // len after each successful copy. Failed formatting uses the fallback above.
    let name = unsafe { std::str::from_utf8_unchecked(&buffer.bytes[..buffer.len]) };
    crate::encode_with_features(name, fields, features)
}

pub(super) struct Operands<'a> {
    inline: [&'a str; 8],
    len: usize,
    overflow: Vec<&'a str>,
}
impl<'a> Operands<'a> {
    pub(super) fn new() -> Self {
        Self {
            inline: [""; 8],
            len: 0,
            overflow: Vec::new(),
        }
    }
    pub(super) fn push(&mut self, value: &'a str) {
        if self.len < self.inline.len() {
            self.inline[self.len] = value;
        } else {
            if self.overflow.is_empty() {
                self.overflow.extend_from_slice(&self.inline);
            }
            self.overflow.push(value);
        }
        self.len += 1;
    }
    pub(super) fn collect(iter: impl Iterator<Item = &'a str>) -> Self {
        let mut result = Self::new();
        for value in iter {
            result.push(value);
        }
        result
    }
}
impl<'a> Deref for Operands<'a> {
    type Target = [&'a str];
    fn deref(&self) -> &Self::Target {
        if self.len <= self.inline.len() {
            &self.inline[..self.len]
        } else {
            &self.overflow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_borrows_lowercase_and_preserves_non_ascii() {
        for text in ["x0", "", "λé中"] {
            assert!(matches!(lower(text), Cow::Borrowed(_)));
        }
        assert_eq!(lower("XÉλ"), "xÉλ");
        for text in ["add", "XÉλ", &"aλZ".repeat(100)] {
            assert_eq!(upper(text).to_string(), text.to_ascii_uppercase());
        }
    }

    #[test]
    fn encoding_name_storage_keeps_lookup_errors_and_validation() {
        assert_eq!(
            encode_named(format_args!("{}_HI_hints", upper("nop")), &[]),
            crate::encode("NOP_HI_hints", &[])
        );
        // Include multibyte text at the boundary and exercise heap fallback.
        for name in ["é".repeat(64), "é".repeat(65)] {
            assert_eq!(
                encode_named(format_args!("{name}"), &[]),
                crate::encode(&name, &[])
            );
        }
        assert_eq!(
            encode_named(
                format_args!("MOVZ_{}_movewide", 32),
                &[(FieldId::Rd, 32), (FieldId::imm16, 0), (FieldId::hw, 0)]
            ),
            crate::encode(
                "MOVZ_32_movewide",
                &[(FieldId::Rd, 32), (FieldId::imm16, 0), (FieldId::hw, 0)]
            )
        );
    }

    #[test]
    fn operand_storage_preserves_inline_and_spilled_lists() {
        for len in [0, 1, 7, 8, 9, 16, 100] {
            let expected = vec!["λ"; len];
            let operands = Operands::collect(expected.iter().copied());
            assert_eq!(&*operands, expected.as_slice());
            assert_eq!(operands.overflow.is_empty(), len <= 8);
        }
    }
}
