//! Symbol expressions and Darwin instruction relocations. There is no
//! general expression evaluator: one symbol, an optional modifier, and
//! integer addends are enough for Keel's address and call sequences.
use super::text::Trim;
use super::{count, integer, is_integer, reg, symbol};
use crate::FieldId;
use crate::{Relocation, RelocationKind};

pub(super) struct Expression<'a> {
    pub name: &'a str,
    pub modifier: Option<String>,
    pub addend: i64,
}
fn addend(text: &str) -> Result<i64, String> {
    let text = text.trim_fast();
    let sign = text.chars().next().ok_or("missing symbol addend")?;
    if !['+', '-'].contains(&sign) {
        return Err("symbol addend requires + or -".into());
    }
    integer(&format!("{sign}{}", text[1..].trim_fast()))
}
fn plain(text: &str) -> Result<(&str, i64), String> {
    let (name, tail) = match text.find(['+', '-']) {
        Some(index) => (&text[..index], &text[index..]),
        None => (text, ""),
    };
    let name = name.trim_fast();
    symbol(name)?;
    Ok((name, if tail.is_empty() { 0 } else { addend(tail)? }))
}
pub(super) fn expression(text: &str) -> Result<Expression<'_>, String> {
    let (name, modifier, value) = if let Some((base, tail)) = text.split_once('@') {
        let (name, before) = plain(base)?;
        let (modifier, after) = match tail.find(['+', '-']) {
            Some(index) => (&tail[..index], addend(&tail[index..])?),
            None => (tail, 0),
        };
        let modifier = modifier.trim_fast().to_ascii_uppercase();
        if ![
            "PAGE",
            "PAGEOFF",
            "GOTPAGE",
            "GOTPAGEOFF",
            "TLVPPAGE",
            "TLVPPAGEOFF",
        ]
        .contains(&modifier.as_str())
        {
            return Err(format!("unsupported symbol modifier `@{modifier}`"));
        }
        (
            name,
            Some(modifier),
            before.checked_add(after).ok_or("symbol addend overflow")?,
        )
    } else {
        let (name, value) = plain(text)?;
        (name, None, value)
    };
    if !(-(1i64 << 23)..(1i64 << 23)).contains(&value) {
        return Err("symbol addend must fit signed 24 bits".into());
    }
    Ok(Expression {
        name,
        modifier,
        addend: value,
    })
}
fn relocation(expr: Expression<'_>, kind: RelocationKind) -> Result<Relocation, String> {
    if !kind.accepts_addend() && expr.addend != 0 {
        return Err("GOT/TLVP relocations do not support addends".into());
    }
    Ok(Relocation {
        section: 0,
        size: 4,
        subtract_symbol: None,
        offset: 0,
        symbol: expr.name.to_string(),
        kind,
        addend: expr.addend,
    })
}

pub(super) fn address_word(op: &str, destination: &str, delta: i64) -> Result<u32, String> {
    let d = reg(destination, false, true)?;
    if d.bits != 64 {
        return Err("adr/adrp require an X register".into());
    }
    address_word_register(op, d.number, delta)
}
pub(super) fn address_word_register(op: &str, destination: u32, delta: i64) -> Result<u32, String> {
    if destination > 31 {
        return Err("invalid address register".into());
    }
    let immediate = if op == "adrp" {
        if delta % 4096 != 0 {
            return Err("adrp displacement must be a multiple of 4096".into());
        }
        delta / 4096
    } else {
        delta
    };
    if !(-(1i64 << 20)..(1i64 << 20)).contains(&immediate) {
        return Err(format!("{op} displacement is out of range"));
    }
    let bits = (immediate as u32) & 0x1f_ffff;
    crate::encode_instruction(
        &keel_asm_ir::Instruction::new(
            if op == "adrp" {
                keel_asm_ir::Opcode::ADRP_only_pcreladdr
            } else {
                keel_asm_ir::Opcode::ADR_only_pcreladdr
            },
            [
                (FieldId::Rd, destination),
                (FieldId::immlo, bits & 3),
                (FieldId::immhi, bits >> 2),
            ],
        ),
        crate::TargetFeatures::default(),
    )
}

/// A relocation-producing instruction. Validation reuses ordinary operand
/// encoders with a zero immediate; the linker supplies the actual immediate.
pub(super) fn instruction(op: &str, args: &[&str]) -> Result<Option<(u32, Relocation)>, String> {
    if op == "adrp" {
        count(args, 2, 2)?;
        if is_integer(args[1]) {
            return Ok(None);
        }
        let expr = expression(args[1])?;
        let kind = match expr.modifier.as_deref() {
            Some("PAGE") => RelocationKind::Page21,
            Some("GOTPAGE") => RelocationKind::GotPage21,
            Some("TLVPPAGE") => RelocationKind::TlvpPage21,
            _ => return Err("adrp symbol requires @PAGE, @GOTPAGE, or @TLVPPAGE".into()),
        };
        return Ok(Some((
            address_word("adrp", args[0], 0)?,
            relocation(expr, kind)?,
        )));
    }
    if op == "add" && args.get(2).is_some_and(|s| s.contains('@')) {
        count(args, 3, 3)?;
        let expr = expression(args[2])?;
        if expr.modifier.as_deref() != Some("PAGEOFF") {
            return Err("symbolic add requires @PAGEOFF".into());
        }
        let word = super::instruction("add", &[args[0], args[1], "#0"])?;
        return Ok(Some((word, relocation(expr, RelocationKind::PageOff12)?)));
    }
    if matches!(
        op,
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" | "ldrsb" | "ldrsh" | "ldrsw"
    ) && args.get(1).is_some_and(|s| s.contains('@'))
    {
        count(args, 2, 2)?;
        let inner = args[1]
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or("symbolic memory addresses do not support writeback")?;
        let parts: Vec<_> = inner.split(',').map(str::trim).collect();
        count(&parts, 2, 2)?;
        let expr = expression(parts[1])?;
        let kind = match expr.modifier.as_deref() {
            Some("PAGEOFF") => RelocationKind::PageOff12,
            Some("GOTPAGEOFF") => RelocationKind::GotPageOff12,
            Some("TLVPPAGEOFF") => RelocationKind::TlvpPageOff12,
            _ => return Err("symbolic memory operand requires a page-offset modifier".into()),
        };
        if kind != RelocationKind::PageOff12
            && (op != "ldr" || reg(args[0], false, true)?.bits != 64)
        {
            return Err("GOT/TLVP page offsets require ldr with an X register".into());
        }
        let address = format!("[{}, #0]", parts[0]);
        let word = super::memory::instruction(op, &[args[0], &address])?;
        return Ok(Some((word, relocation(expr, kind)?)));
    }
    Ok(None)
}
