//! Data sections and deferred symbol expressions. Zero-fill reserves virtual
//! space without allocating bytes; all offsets remain relative to a section.
use super::text::Trim;
use super::{count, symbol, unsigned};
use crate::{Assembly, Section, SectionContents, SectionKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Location {
    pub section: usize,
    pub offset: u64,
}
pub(super) fn position(a: &Assembly, section: usize) -> u64 {
    if section == 0 {
        a.text.len() as u64
    } else {
        a.sections[section - 1].size()
    }
}
pub(super) fn select(
    a: &mut Assembly,
    segment: &str,
    name: &str,
    kind: SectionKind,
) -> Result<usize, String> {
    if segment == "__TEXT" && name == "__text" {
        return Ok(0);
    }
    if let Some(index) = a
        .sections
        .iter()
        .position(|s| s.segment == segment && s.name == name)
    {
        if a.sections[index].kind != kind {
            return Err("section redeclared with a different type".into());
        }
        return Ok(index + 1);
    }
    if a.sections.len() >= 254 {
        return Err("Mach-O supports at most 255 sections".into());
    }
    a.sections.push(Section {
        segment: segment.into(),
        name: name.into(),
        alignment: 0,
        kind,
        contents: if kind.is_zero_fill() {
            SectionContents::ZeroFill(0)
        } else {
            SectionContents::Bytes(Vec::new())
        },
    });
    Ok(a.sections.len())
}
pub(super) fn section(a: &mut Assembly, args: &[&str]) -> Result<usize, String> {
    count(args, 2, 4)?;
    let kind = match (args[0], args[1]) {
        ("__TEXT", "__text")
            if args.len() == 2 || args == ["__TEXT", "__text", "regular", "pure_instructions"] =>
        {
            return Ok(0);
        }
        ("__TEXT", "__const" | "__gcc_except_tab") | ("__DATA", "__data" | "__const") => {
            SectionKind::Data
        }
        ("__TEXT", "__cstring") => SectionKind::CString,
        ("__TEXT", "__literal4") => SectionKind::Literal4,
        ("__TEXT", "__literal8") => SectionKind::Literal8,
        ("__TEXT", "__literal16") => SectionKind::Literal16,
        ("__DATA", "__bss") => SectionKind::ZeroFill,
        ("__DATA", "__thread_data") => SectionKind::ThreadData,
        ("__DATA", "__thread_bss") => SectionKind::ThreadZeroFill,
        ("__DATA", "__thread_vars") => SectionKind::ThreadVariables,
        (
            "__DWARF",
            "__debug_info" | "__debug_abbrev" | "__debug_line" | "__debug_ranges" | "__debug_str"
            | "__debug_loc",
        ) => SectionKind::Debug,
        ("__TEXT", "__eh_frame") => SectionKind::EhFrame,
        _ => return Err("unsupported Mach-O section".into()),
    };
    let expected = match kind {
        SectionKind::Data | SectionKind::Debug => "regular",
        SectionKind::EhFrame => "coalesced",
        SectionKind::CString => "cstring_literals",
        SectionKind::Literal4 => "4byte_literals",
        SectionKind::Literal8 => "8byte_literals",
        SectionKind::Literal16 => "16byte_literals",
        SectionKind::ZeroFill => "zerofill",
        SectionKind::ThreadData => "thread_local_regular",
        SectionKind::ThreadZeroFill => "thread_local_zerofill",
        SectionKind::ThreadVariables => "thread_local_variables",
    };
    if kind == SectionKind::Debug && args == [args[0], args[1], "regular", "debug"] {
        return select(a, args[0], args[1], kind);
    }
    if args.len() > 3 || (args.len() == 3 && args[2] != expected) {
        return Err("unsupported section type or attributes".into());
    }
    select(a, args[0], args[1], kind)
}
pub(super) fn bytes_mut(a: &mut Assembly, section: usize) -> Result<&mut Vec<u8>, String> {
    if section == 0 {
        return Ok(&mut a.text);
    }
    match &mut a.sections[section - 1].contents {
        SectionContents::Bytes(bytes) => Ok(bytes),
        SectionContents::ZeroFill(_) => Err("relocations require a file-backed section".into()),
    }
}
pub(super) fn append(a: &mut Assembly, section: usize, bytes: &[u8]) -> Result<(), String> {
    if section != 0
        && let SectionContents::ZeroFill(size) = &mut a.sections[section - 1].contents
    {
        if bytes.iter().any(|b| *b != 0) {
            return Err("zero-fill sections cannot contain nonzero initializers".into());
        }
        *size = size
            .checked_add(bytes.len() as u64)
            .ok_or("zero-fill size overflow")?;
    } else {
        bytes_mut(a, section)?.extend_from_slice(bytes);
    }
    Ok(())
}
pub(super) fn reserve(
    a: &mut Assembly,
    section: usize,
    amount: u64,
    fill: u8,
) -> Result<(), String> {
    if section != 0
        && let SectionContents::ZeroFill(size) = &mut a.sections[section - 1].contents
    {
        if fill != 0 {
            return Err("zero-fill sections require zero padding".into());
        }
        *size = size.checked_add(amount).ok_or("zero-fill size overflow")?;
    } else {
        let bytes = bytes_mut(a, section)?;
        let amount = usize::try_from(amount).map_err(|_| "data size overflow")?;
        let len = bytes
            .len()
            .checked_add(amount)
            .ok_or("data size overflow")?;
        if len > u32::MAX as usize {
            return Err("file-backed section exceeds 32 bits".into());
        }
        bytes
            .try_reserve(amount)
            .map_err(|_| "cannot allocate data section")?;
        bytes.resize(len, fill);
    }
    Ok(())
}
pub(super) fn align(a: &mut Assembly, section: usize, power: u32) -> Result<(), String> {
    align_with(a, section, power, None, None)
}
pub(super) fn align_with(
    a: &mut Assembly,
    section: usize,
    power: u32,
    fill: Option<u8>,
    maximum: Option<u64>,
) -> Result<(), String> {
    if section == 0 {
        a.text_alignment = a.text_alignment.max(power);
    } else {
        a.sections[section - 1].alignment = a.sections[section - 1].alignment.max(power);
    }
    let alignment = 1u64 << power;
    let padding = (alignment - position(a, section) % alignment) % alignment;
    if maximum.is_some_and(|limit| padding > limit) {
        return Ok(());
    }
    if let Some(fill) = fill {
        return reserve(a, section, padding, fill);
    }
    if section == 0 {
        let partial = padding % 4;
        reserve(a, section, partial, 0)?;
        let nop = crate::encode("NOP_HI_hints", &[])?.to_le_bytes();
        for _ in 0..padding / 4 {
            a.text.extend_from_slice(&nop);
        }
    } else {
        reserve(a, section, padding, 0)?;
    }
    Ok(())
}

pub(super) struct Expression<'a> {
    pub add: Option<&'a str>,
    pub subtract: Option<&'a str>,
    pub constant: i128,
}
pub(super) fn expression(text: &str) -> Result<Expression<'_>, String> {
    let mut result = Expression {
        add: None,
        subtract: None,
        constant: 0,
    };
    let mut rest = text.trim_fast();
    let mut first = true;
    while !rest.is_empty() {
        let sign = match rest.as_bytes()[0] {
            b'+' => {
                rest = rest[1..].trim_start();
                1i128
            }
            b'-' => {
                rest = rest[1..].trim_start();
                -1
            }
            _ if first => 1,
            _ => return Err("expected + or - in data expression".into()),
        };
        let end = rest.find(['+', '-']).unwrap_or(rest.len());
        let term = rest[..end].trim_fast();
        if term.is_empty() {
            return Err("missing data expression term".into());
        }
        if term.as_bytes()[0].is_ascii_digit() {
            let (digits, radix) = super::text::integer_radix(term);
            let value = i128::from_str_radix(digits, radix).map_err(|_| "invalid data integer")?;
            result.constant = result
                .constant
                .checked_add(sign.checked_mul(value).ok_or("data integer overflow")?)
                .ok_or("data integer overflow")?;
        } else {
            symbol(term)?;
            let slot = if sign == 1 {
                &mut result.add
            } else {
                &mut result.subtract
            };
            if slot.replace(term).is_some() {
                return Err(
                    "data expressions allow at most one added and one subtracted symbol".into(),
                );
            }
        }
        rest = rest[end..].trim_start();
        first = false;
    }
    if first || (result.subtract.is_some() && result.add.is_none()) {
        return Err("invalid data expression".into());
    }
    Ok(result)
}
pub(super) fn value(constant: i128, width: u8) -> Result<Vec<u8>, String> {
    let bits = u32::from(width) * 8;
    if constant < -(1i128 << (bits - 1)) || constant >= 1i128 << bits {
        return Err(format!("data value does not fit {bits} bits"));
    }
    Ok((constant as u64).to_le_bytes()[..usize::from(width)].to_vec())
}
pub(super) fn width(op: &str) -> Option<u8> {
    match op {
        ".byte" => Some(1),
        ".short" => Some(2),
        ".long" => Some(4),
        ".quad" => Some(8),
        _ => None,
    }
}
pub(super) fn allocation<'a>(args: &[&'a str], tls: bool) -> Result<(&'a str, u64, u32), String> {
    count(args, if tls { 3 } else { 5 }, if tls { 3 } else { 5 })?;
    let values = if tls {
        args
    } else {
        if args[..2] != ["__DATA", "__bss"] {
            return Err("zerofill requires __DATA,__bss".into());
        }
        &args[2..]
    };
    symbol(values[0])?;
    let size = u64::from(unsigned(values[1], u32::MAX as i64)?);
    let alignment = unsigned(values[2], 16)?;
    Ok((values[0], size, alignment))
}

/// Clang emits the SDK clause after whitespace, without a comma delimiter.
pub(super) fn build_version(text: &str) -> Result<crate::BuildVersion, String> {
    let (target, sdk) = text
        .split_once("sdk_version")
        .map_or((text, None), |(a, b)| (a, Some(b)));
    let args = super::memory::operands(target)?;
    super::count(&args, 3, 4)?;
    let platform = match args[0] {
        "macos" => 1,
        "ios" => 2,
        "tvos" => 3,
        "watchos" => 4,
        "bridgeos" => 5,
        "macCatalyst" => 6,
        "iossimulator" => 7,
        "tvossimulator" => 8,
        "watchossimulator" => 9,
        "driverkit" => 10,
        "xros" => 11,
        "xrossimulator" => 12,
        _ => return Err("unsupported build-version platform".into()),
    };
    fn version(args: &[&str]) -> Result<u32, String> {
        super::count(args, 2, 3)?;
        Ok((super::unsigned(args[0], 65535)? << 16)
            | (super::unsigned(args[1], 255)? << 8)
            | args.get(2).map_or(Ok(0), |s| super::unsigned(s, 255))?)
    }
    Ok(crate::BuildVersion {
        platform,
        minimum: version(&args[1..])?,
        sdk: sdk
            .map(|s| super::memory::operands(s).and_then(|a| version(&a)))
            .transpose()?
            .unwrap_or(0),
    })
}

/// LLVM MC expressions shift a 64-bit bit pattern logically, including when
/// the unshifted symbol difference is negative.
pub(super) fn shift_value(value: i128, shift: u32) -> Result<i128, String> {
    if shift == 0 {
        return Ok(value);
    }
    if !(-(1i128 << 63)..(1i128 << 64)).contains(&value) {
        return Err("shifted data exceeds 64 bits".into());
    }
    Ok(i128::from((value as u64) >> shift))
}
