//! Advanced SIMD register arithmetic, reductions, and modified immediates.
use super::count;
use super::storage::{encode_named, lower, upper};
use crate::FieldId;
use crate::encode;

fn number(text: &str, prefix: char) -> Result<u32, String> {
    let digits = text
        .strip_prefix(prefix)
        .ok_or("invalid SIMD register class")?;
    let number = digits
        .parse::<u32>()
        .map_err(|_| format!("invalid SIMD register `{text}`"))?;
    if number > 31 {
        return Err(format!("invalid SIMD register `{text}`"));
    }
    Ok(number)
}
/// Bytewise register operations. MOV aliases of other arrangements are
/// handled by the lane/permutation module using the same ORR record.
fn logical(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    let unary = matches!(op, "not" | "mvn");
    if !unary
        && !matches!(
            op,
            "and" | "orr" | "orn" | "eor" | "bic" | "bsl" | "bit" | "bif"
        )
        || !args.first().is_some_and(|s| s.starts_with('v'))
    {
        return Ok(None);
    }
    count(args, if unary { 2 } else { 3 }, if unary { 2 } else { 3 })?;
    let d = Shape::parse(args[0])?;
    let n = Shape::parse(args[1])?;
    d.valid_vector()?;
    d.same(n)?;
    if d.size != 0 {
        return Err("SIMD logical forms require 8B or 16B".into());
    }
    let mut fields = vec![
        (FieldId::Rd, d.number),
        (FieldId::Rn, n.number),
        (FieldId::Q, d.q),
    ];
    if !unary {
        let m = Shape::parse(args[2])?;
        d.same(m)?;
        fields.push((FieldId::Rm, m.number));
    }
    let root = if unary { "NOT" } else { op };
    let form = if unary {
        "asimdmisc_R"
    } else {
        "asimdsame_only"
    };
    encode(&format!("{}_{form}", root.to_ascii_uppercase()), &fields).map(Some)
}

/// Advanced SIMD modified immediates: byte replication, shifted halfwords /
/// words, low-one MSL words, and one immediate bit per 00/FF byte in 64-bit lanes.
pub(super) fn move_immediate(args: &[&str]) -> Result<u32, String> {
    modified_immediate("movi", args)
}
fn modified_immediate(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 2, 3)?;
    let dest = lower(args[0]);
    let (rd, arrangement, q) = if dest.starts_with('d') {
        (number(&dest, 'd')?, "d", 0)
    } else {
        let (name, arrangement) = dest
            .split_once('.')
            .ok_or("movi requires a lane arrangement")?;
        let q = match arrangement {
            "8b" | "4h" | "2s" => 0,
            "16b" | "8h" | "4s" | "2d" => 1,
            _ => return Err("invalid movi lane arrangement".into()),
        };
        (number(name, 'v')?, arrangement, q)
    };
    let wide = matches!(arrangement, "d" | "2d");
    let imm = if wide {
        let expr = super::data::expression(args[1].strip_prefix('#').unwrap_or(args[1]))?;
        if expr.add.is_some() || !(-(1i128 << 63)..(1i128 << 64)).contains(&expr.constant) {
            return Err("invalid movi 64-bit byte mask".into());
        }
        let value = expr.constant as u64;
        let mut imm = 0;
        for i in 0..8 {
            match (value >> (8 * i)) & 255 {
                0 => {}
                255 => imm |= 1 << i,
                _ => return Err("movi D immediates require each byte to be 00 or FF".into()),
            }
        }
        imm
    } else {
        super::unsigned(args[1], 255)?
    };
    let shift = if args.len() == 3 {
        super::shift(args[2])?
    } else {
        ("lsl", 0)
    };
    let (form, cmode) = match (arrangement, shift) {
        ("8b" | "16b", ("lsl", 0)) if args.len() == 2 => ("N_b", None),
        ("4h" | "8h", ("lsl", n)) if matches!(n, 0 | 8) => ("L_hl", Some(8 + n / 4)),
        ("2s" | "4s", ("lsl", n)) if matches!(n, 0 | 8 | 16 | 24) => ("L_sl", Some(n / 4)),
        ("2s" | "4s", ("msl", n)) if matches!(n, 8 | 16) => ("M_sm", Some(11 + n / 8)),
        ("d", ("lsl", 0)) if args.len() == 2 => ("D_ds", None),
        ("2d", ("lsl", 0)) if args.len() == 2 => ("D2_d", None),
        _ => return Err("invalid movi shift for lane arrangement".into()),
    };
    if op != "movi" && (wide || matches!(arrangement, "8b" | "16b"))
        || matches!(op, "orr" | "bic") && form == "M_sm"
    {
        return Err("invalid modified-immediate arrangement or shift".into());
    }
    let cmode = cmode.map(|v| {
        if matches!(op, "orr" | "bic") {
            v | 1
        } else {
            v
        }
    });
    let mut fields = vec![(FieldId::Rd, rd)];

    if !wide {
        fields.push((FieldId::Q, q));
    }
    if let Some(cmode) = cmode {
        fields.push((FieldId::cmode, cmode));
    }
    for (i, name) in [
        FieldId::a,
        FieldId::b,
        FieldId::c,
        FieldId::d,
        FieldId::e,
        FieldId::f,
        FieldId::g,
        FieldId::h,
    ]
    .into_iter()
    .enumerate()
    {
        fields.push((name, (imm >> (7 - i)) & 1));
    }
    encode_named(format_args!("{}_asimdimm_{form}", upper(op)), &fields)
}

mod fp;
mod indexed;
mod integer;
mod memory;
mod numeric;
mod permute;

#[derive(Debug, Clone, Copy)]
struct Shape {
    number: u32,
    size: u32,
    q: u32,
    scalar: bool,
}
impl Shape {
    fn parse(text: &str) -> Result<Self, String> {
        if text.starts_with('v') {
            let (name, arrangement) = text
                .split_once('.')
                .ok_or("vector register requires arrangement")?;
            let (q, size) = match arrangement {
                "8b" => (0, 0),
                "16b" => (1, 0),
                "4h" => (0, 1),
                "8h" => (1, 1),
                "2s" => (0, 2),
                "4s" => (1, 2),
                "1d" => (0, 3),
                "2d" => (1, 3),
                _ => return Err("invalid SIMD arrangement".into()),
            };
            Ok(Self {
                number: number(name, 'v')?,
                size,
                q,
                scalar: false,
            })
        } else {
            let prefix = text.chars().next().ok_or("missing SIMD register")?;
            let size = match prefix {
                'b' => 0,
                'h' => 1,
                's' => 2,
                'd' => 3,
                _ => return Err("invalid SIMD register class".into()),
            };
            Ok(Self {
                number: number(text, prefix)?,
                size,
                q: 0,
                scalar: true,
            })
        }
    }
    fn bits(self) -> u32 {
        8 << self.size
    }
    fn lanes(self) -> u32 {
        if self.scalar {
            1
        } else {
            (8 << self.q) >> self.size
        }
    }
    fn same(self, other: Self) -> Result<(), String> {
        if self.size != other.size || self.q != other.q || self.scalar != other.scalar {
            return Err("SIMD operands require matching arrangements".into());
        }
        Ok(())
    }
    fn valid_vector(self) -> Result<(), String> {
        if self.scalar || (self.q == 0 && self.size == 3) {
            return Err("invalid vector arrangement for this instruction".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy)]
struct Lane {
    number: u32,
    size: u32,
    index: u32,
}
impl Lane {
    fn parse(text: &str) -> Result<Self, String> {
        let (name, index) = text.split_once('[').ok_or("expected SIMD lane index")?;
        let index = index
            .strip_suffix(']')
            .ok_or("unclosed SIMD lane index")?
            .parse::<u32>()
            .map_err(|_| "invalid SIMD lane index")?;
        let (name, element) = name
            .split_once('.')
            .ok_or("SIMD lane requires element size")?;
        let size = match element {
            "b" => 0,
            "h" => 1,
            "s" => 2,
            "d" => 3,
            _ => return Err("invalid SIMD lane element".into()),
        };
        if index >= 16 >> size {
            return Err("SIMD lane index out of range".into());
        }
        Ok(Self {
            number: number(name, 'v')?,
            size,
            index,
        })
    }
    fn imm5(self) -> u32 {
        (self.index << (self.size + 1)) | (1 << self.size)
    }
}

/// Lists are consecutive modulo 32, as in the architectural register list.
/// Both explicit lists and the assembler's start-end shorthand are accepted.
fn register_list(text: &str) -> Result<Vec<Shape>, String> {
    let text = text
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected SIMD register list in braces")?
        .trim();
    let regs = if let Some((start, end)) = text.split_once('-') {
        let start = Shape::parse(start.trim())?;
        let end = Shape::parse(end.trim())?;
        start.same(end)?;
        let count = (end.number + 32 - start.number) % 32 + 1;
        if count > 4 {
            return Err("SIMD register lists require 1..4 registers".into());
        }
        (0..count)
            .map(|i| Shape {
                number: (start.number + i) % 32,
                ..start
            })
            .collect::<Vec<_>>()
    } else {
        text.split(',')
            .map(|s| Shape::parse(s.trim()))
            .collect::<Result<Vec<_>, _>>()?
    };
    if regs.is_empty() || regs.len() > 4 || regs[0].scalar {
        return Err("SIMD register lists require 1..4 vectors".into());
    }
    for (i, r) in regs.iter().enumerate() {
        regs[0].same(*r)?;
        if r.number != (regs[0].number + i as u32) % 32 {
            return Err("SIMD register list must be consecutive".into());
        }
    }
    Ok(regs)
}

pub(super) fn extra(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    if args.first().is_none_or(|s| {
        s.eq_ignore_ascii_case("sp")
            || !s.starts_with(['v', 'V', 'b', 'B', 'h', 'H', 's', 'S', 'd', 'D', '{'])
                && !matches!(op, "smov" | "umov")
                && !(op == "mov" && args.get(1).is_some_and(|s| s.starts_with(['v', 'V'])))
    }) {
        return Ok(None);
    }
    let owned: Vec<_> = args.iter().map(|s| s.to_ascii_lowercase()).collect();
    let args: Vec<_> = owned.iter().map(String::as_str).collect();
    if let Some(word) = permute::instruction(op, &args)? {
        return Ok(Some(word));
    }
    if let Some(word) = memory::instruction(op, &args)? {
        return Ok(Some(word));
    }
    if let Some(word) = indexed::instruction(op, &args)? {
        return Ok(Some(word));
    }
    if let Some(word) = integer::shift(op, &args)? {
        return Ok(Some(word));
    }
    if let Some(word) = integer::instruction(op, &args)? {
        return Ok(Some(word));
    }
    if let Some(word) = fp::reduction(op, &args)? {
        return Ok(Some(word));
    }
    if matches!(op, "mvni" | "orr" | "bic")
        && args.get(1).is_some_and(|s| s.starts_with('#'))
        && args[0].starts_with('v')
    {
        return modified_immediate(op, &args).map(Some);
    }
    if let Some(word) = fp::instruction(op, &args)? {
        return Ok(Some(word));
    }
    logical(op, &args)
}

pub(super) fn numeric(
    op: &str,
    args: &[&str],
    features: crate::TargetFeatures,
) -> Result<Option<u32>, String> {
    numeric::instruction(op, args, features)
}

/// Shared operand rules may describe fields fixed in the scalar variant.
/// Each caller validates legal shapes before omitting those fixed fields.
fn finish(name: &str, values: &[(FieldId, u32)]) -> Result<u32, String> {
    finish_with_features(name, values, crate::TargetFeatures::default())
}
fn finish_with_features(
    name: &str,
    values: &[(FieldId, u32)],
    features: crate::TargetFeatures,
) -> Result<u32, String> {
    let encoding = crate::generated::encoding(name)
        .ok_or_else(|| format!("unsupported SIMD form `{name}`"))?;
    let fields: Vec<_> = values
        .iter()
        .copied()
        .filter(|(n, _)| encoding.fields.iter().any(|f| f.id == *n))
        .collect();
    crate::encode_with_features(name, &fields, features)
}

fn immediate_fields(imm: u32) -> Vec<(FieldId, u32)> {
    [
        FieldId::a,
        FieldId::b,
        FieldId::c,
        FieldId::d,
        FieldId::e,
        FieldId::f,
        FieldId::g,
        FieldId::h,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, n)| (n, (imm >> (7 - i)) & 1))
    .collect()
}
