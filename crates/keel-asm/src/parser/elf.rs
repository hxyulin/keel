//! GNU AArch64 instruction parsing for the shared ELF assembler.
use super::*;

fn expression(text: &str) -> Result<(String, i64), String> {
    let text = text.trim().trim_start_matches('#');
    let split = text
        .char_indices()
        .skip(1)
        .find(|(_, c)| matches!(c, '+' | '-'))
        .map(|(i, _)| i);
    let (name, addend) = match split {
        Some(i) => (&text[..i], integer(text[i..].trim())?),
        None => (text, 0),
    };
    symbol(name.trim())?;
    Ok((name.trim().into(), addend))
}
fn operands(text: &str) -> Result<Vec<&str>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let (mut quoted, mut escaped, mut depth, mut start) = (false, false, 0, 0);
    let mut args = Vec::new();
    for (i, c) in text.char_indices() {
        if c == '"' && !escaped {
            quoted = !quoted;
        }
        if !quoted {
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err("unbalanced operands".into());
                    }
                }
                ',' if depth == 0 => {
                    args.push(text[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        escaped = quoted && c == '\\' && !escaped;
    }
    if quoted || depth != 0 {
        return Err("unterminated string or operand".into());
    }
    args.push(text[start..].trim());
    Ok(args)
}
pub(crate) fn data_integer(text: &str, width: u64) -> Result<i128, String> {
    let text = text.trim().trim_start_matches('#');
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches(['-', '+']);
    let (digits, radix) = text::integer_radix(digits);
    let value = i128::from_str_radix(digits, radix).map_err(|_| "invalid data integer")?;
    let value = if negative { -value } else { value };
    if value < -(1i128 << (width * 8 - 1)) || value >= 1i128 << (width * 8) {
        return Err("data integer out of range".into());
    }
    Ok(value)
}
/// One A64 instruction plus an optional ELF RELA reference.
pub(crate) struct Word {
    pub word: u32,
    pub relocation: Option<(u32, String, i64)>,
}
pub(crate) fn instruction(text: &str, features: crate::TargetFeatures) -> Result<Word, String> {
    let (op, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let op = op.to_ascii_lowercase();
    let args = operands(rest.trim())?;
    let arena = Bump::new();
    let (word, relocation) = if let Some(branch) = control::branch(&op, &args, &arena)? {
        let (name, addend) = expression(branch.target)?;
        let mut fields = branch.fields.to_vec();
        fields.push((branch.field, 0));
        let kind = match branch.width {
            26 => {
                if op == "bl" {
                    283
                } else {
                    282
                }
            }
            19 => {
                if op.starts_with("ldr") {
                    273
                } else {
                    280
                }
            }
            14 => 279,
            _ => return Err("unsupported branch relocation".into()),
        };
        (
            crate::encode_record(
                &crate::generated::ENCODINGS[branch.encoding.index()],
                &fields,
                features,
            )?,
            Some((kind, name, addend)),
        )
    } else if op == "adr" || op == "adrp" {
        count(&args, 2, 2)?;
        let r = reg(args[0], false, false)?;
        if r.bits != 64 {
            return Err("ADR/ADRP requires an X register".into());
        }
        let (kind, target) = if op == "adr" {
            (274, args[1])
        } else if let Some(s) = args[1].strip_prefix(":gottprel:") {
            (541, s)
        } else if let Some(s) = args[1].strip_prefix(":got:") {
            (311, s)
        } else {
            (275, args[1])
        };
        let (name, addend) = expression(target)?;
        (
            (if op == "adr" { 0x10000000 } else { 0x90000000 }) | r.number,
            Some((kind, name, addend)),
        )
    } else if let Some((i, operand)) = args.iter().enumerate().find(|(_, s)| {
        s.contains(":lo12:") || s.contains(":got_lo12:") || s.contains(":gottprel_lo12:")
    }) {
        let tls = operand.contains(":gottprel_lo12:");
        let got = operand.contains(":got_lo12:") || tls;
        let marker = if tls {
            ":gottprel_lo12:"
        } else if got {
            ":got_lo12:"
        } else {
            ":lo12:"
        };
        let start = operand.find(marker).unwrap();
        let tail = &operand[start + marker.len()..];
        let target = tail.trim_end_matches(']').trim();
        let (name, addend) = expression(target)?;
        let replacement = format!(
            "{}#0{}",
            &operand[..start],
            if operand.ends_with(']') { "]" } else { "" }
        );
        let mut patched: Vec<&str> = args.to_vec();
        patched[i] = &replacement;
        let word = instruction_with_features(&op, &patched, features)?;
        let kind = if op == "add" && !got {
            277
        } else if op.starts_with("ldr") || op.starts_with("str") {
            if got {
                if word & 0xffc00000 != 0xf9400000 {
                    return Err("GOT load requires ldr X register".into());
                }
                if tls { 542 } else { 312 }
            } else {
                match (word >> 30, word & 0x04800000 == 0x04800000) {
                    (_, true) => 299,
                    (0, _) => 278,
                    (1, _) => 284,
                    (2, _) => 285,
                    _ => 286,
                }
            }
        } else {
            return Err("unsupported low-12 relocation instruction".into());
        };
        (word, Some((kind, name, addend)))
    } else {
        (instruction_with_features(&op, &args, features)?, None)
    };
    Ok(Word { word, relocation })
}
