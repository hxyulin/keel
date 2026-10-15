//! Advanced SIMD register arithmetic, reductions, and modified immediates.
use super::count;
use super::storage::{encode_named, lower};
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
fn vector(text: &str) -> Result<(u32, &str, u32, u32), String> {
    let (name, arrangement) = text
        .split_once('.')
        .ok_or("vector register requires an arrangement")?;
    let register = number(name, 'v')?;
    let (q, size) = match arrangement {
        "8b" => (0, 0),
        "16b" => (1, 0),
        "4h" => (0, 1),
        "8h" => (1, 1),
        "2s" => (0, 2),
        "4s" => (1, 2),
        "2d" => (1, 3),
        _ => return Err("unsupported SIMD arrangement".into()),
    };
    Ok((register, arrangement, q, size))
}
pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 2, 2)?;
    let d = lower(args[0]);
    let n = lower(args[1]);
    let (rn, arrangement, q, size) = vector(&n)?;
    let rd = if op == "cnt" {
        let (rd, dest_arrangement, _, _) = vector(&d)?;
        if size != 0 || arrangement != dest_arrangement {
            return Err("cnt requires matching 8B or 16B arrangements".into());
        }
        rd
    } else {
        if size == 3 || (size == 2 && q == 0) {
            return Err("invalid SIMD reduction arrangement".into());
        }
        number(
            &d,
            match size {
                0 => 'b',
                1 => 'h',
                _ => 's',
            },
        )?
    };
    encode(
        if op == "cnt" {
            "CNT_asimdmisc_R"
        } else {
            "ADDV_asimdall_only"
        },
        &[
            (FieldId::Rd, rd),
            (FieldId::Rn, rn),
            (FieldId::Q, q),
            (FieldId::size, size),
        ],
    )
}

/// Register-only same-width forms. Restrict each family before encoding:
/// the machine-readable bit layout alone does not exclude every reserved size.
pub(super) fn arithmetic(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    if !args.first().is_some_and(|s| s.starts_with(['v', 'V'])) {
        return Ok(None);
    }
    let (root, unary, kind) = match op {
        "mov" => ("orr", true, "logical"),
        "mvn" => ("not", true, "logical"),
        "not" => (op, true, "logical"),
        "and" | "orr" | "orn" | "eor" | "bic" | "bsl" | "bit" | "bif" => (op, false, "logical"),
        "add" | "sub" | "cmeq" | "cmgt" | "cmge" | "cmhi" | "cmhs" => (op, false, "integer"),
        "abs" | "neg" => (op, true, "integer"),
        "mul" | "mla" | "mls" | "smax" | "smin" | "umax" | "umin" => (op, false, "small"),
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmla" | "fmls" | "fmin" | "fmax" | "fminnm"
        | "fmaxnm" => (op, false, "fp"),
        "fabs" | "fneg" | "fsqrt" => (op, true, "fp"),
        _ => return Ok(None),
    };
    let arity = if unary { 2 } else { 3 };
    count(args, arity, arity)?;
    let dest = args[0].to_ascii_lowercase();
    let (rd, arrangement, q, size) = vector(&dest)?;
    match kind {
        "logical" if size != 0 && op != "mov" => {
            return Err("SIMD logical forms require 8B or 16B".into());
        }
        "small" if size == 3 => {
            return Err("this SIMD arithmetic form does not support D lanes".into());
        }
        "fp" if !matches!(arrangement, "2s" | "4s" | "2d") => {
            return Err("SIMD FP requires 2S, 4S, or 2D".into());
        }
        _ => {}
    }
    let mut registers = Vec::new();
    for arg in &args[1..] {
        let text = arg.to_ascii_lowercase();
        let (r, a, _, _) = vector(&text)?;
        if a != arrangement {
            return Err("SIMD operands require matching arrangements".into());
        }
        registers.push(r);
    }
    let mut fields = vec![("Rd", rd), ("Rn", registers[0]), ("Q", q)];
    if !unary || op == "mov" {
        fields.push(("Rm", *registers.last().unwrap()));
    }
    if kind == "fp" {
        fields.push(("sz", size - 2));
    } else if kind != "logical" {
        fields.push(("size", size));
    }
    let form = if unary && op != "mov" {
        "asimdmisc_R"
    } else {
        "asimdsame_only"
    };
    encode(&format!("{}_{form}", root.to_ascii_uppercase()), &fields).map(Some)
}

/// Advanced SIMD modified immediates: byte replication, shifted halfwords /
/// words, low-one MSL words, and one immediate bit per 00/FF byte in 64-bit lanes.
pub(super) fn move_immediate(args: &[&str]) -> Result<u32, String> {
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
    encode_named(format_args!("MOVI_asimdimm_{form}"), &fields)
}

mod integer;
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
    if let Some(word) = integer::shift(op, &args)? {
        return Ok(Some(word));
    }
    integer::instruction(op, &args)
}

/// Shared operand rules may describe fields fixed in the scalar variant.
/// Each caller validates legal shapes before omitting those fixed fields.
fn finish(name: &str, values: &[(&str, u32)]) -> Result<u32, String> {
    let encoding = crate::generated::encoding(name)
        .ok_or_else(|| format!("unsupported SIMD form `{name}`"))?;
    let fields: Vec<_> = values
        .iter()
        .copied()
        .filter(|(n, _)| encoding.fields.iter().any(|f| f.name == *n))
        .collect();
    encode(name, &fields)
}
