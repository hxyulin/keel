//! Symbol spelling for AArch64 Mach-O assembly.

use std::fmt::{self, Write};

/// Whether a symbol can be written without quotes in assembly.
#[inline]
pub fn is_bare_symbol(name: &str) -> bool {
    let valid = |b: u8| {
        (b | 0x20).wrapping_sub(b'a') < 26
            || b.wrapping_sub(b'0') < 10
            || matches!(b, b'_' | b'.' | b'$')
    };
    match name.as_bytes() {
        [first, rest @ ..] => {
            valid(*first)
                && !first.is_ascii_digit()
                && rest.iter().fold(true, |all, &b| all & valid(b))
        }
        [] => false,
    }
}

/// Check a decoded symbol name before assembling a Mach-O object.
///
/// # Errors
/// Returns an error for empty names, NUL bytes, or line endings.
pub fn validate_symbol(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains(['\0', '\r', '\n']) {
        return Err(format!("invalid symbol `{name}`"));
    }
    Ok(())
}

/// Write an assembly token for a decoded symbol, quoting it when needed.
///
/// # Errors
/// Returns any formatting error from the output sink.
pub fn write_symbol(out: &mut (impl Write + ?Sized), name: &str) -> fmt::Result {
    if is_bare_symbol(name) {
        return out.write_str(name);
    }
    out.write_char('"')?;
    let mut start = 0;
    for (index, ch) in name.match_indices(['"', '\\']) {
        out.write_str(&name[start..index])?;
        out.write_char('\\')?;
        out.write_str(ch)?;
        start = index + 1;
    }
    out.write_str(&name[start..])?;
    out.write_char('"')
}

/// Format a decoded symbol as an assembly token.
#[derive(Clone, Copy, Debug)]
pub struct Symbol<'a>(pub &'a str);

impl fmt::Display for Symbol<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_symbol(f, self.0)
    }
}
