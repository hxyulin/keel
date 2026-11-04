//! Pieces of the DWARF line table that the Mach-O builder (parser/debug.rs)
//! and the ELF builder (x86/dwarf.rs) emit identically.
//!
//! The two builders still differ in `.file` and `.loc` semantics and in the
//! order of the line-program opcodes for one row, so each keeps its own row
//! encoder and DWARF 5 tables.
use keel_util::leb128;

/// The line-program header fields after `header_length`: a minimum
/// instruction length of 1, one operation per instruction, `default_is_stmt`
/// set, `line_base` -5, `line_range` 14, `opcode_base` 13, and the operand
/// counts of standard opcodes 1 through 12.
pub(crate) const PROGRAM_PARAMETERS: [u8; 18] = [
    1,
    1,
    1,
    (-5i8) as u8,
    14,
    13,
    0,
    1,
    1,
    1,
    1,
    0,
    0,
    0,
    1,
    0,
    0,
    1,
];

/// The DWARF 4 `include_directories` and `file_names` tables. Each file is a
/// name and a directory index; modification time and length are zero.
pub(crate) fn v4_tables<'a>(
    out: &mut Vec<u8>,
    directories: &[Vec<u8>],
    files: impl IntoIterator<Item = (&'a [u8], u32)>,
) {
    for directory in directories {
        out.extend_from_slice(directory);
        out.push(0);
    }
    out.push(0);
    for (name, directory) in files {
        out.extend_from_slice(name);
        out.push(0);
        leb128::unsigned(out, directory.into());
        out.extend_from_slice(&[0, 0]);
    }
    out.push(0);
}

/// Split one leading quoted string from `s`, honoring backslash escapes.
/// Returns the string with its quotes, and the rest of `s`.
pub(crate) fn quoted(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return Err("expected quoted string".into());
    }
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Ok((&s[..=i], &s[i + 1..])),
            _ => i += 1,
        }
    }
    Err("unterminated string".into())
}
