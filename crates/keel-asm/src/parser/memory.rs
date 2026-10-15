//! Scalar integer / FP / SIMD memory operands. Immediate offsets are byte
//! offsets in source; scaling and signed field packing happen only at encoding.
use super::storage::Operands;
use super::storage::{encode_named, lower, upper};
use super::text::Trim;
use super::{Kind, Reg, count, integer, is_integer, reg, unsigned};
use crate::FieldId;

/// Keep bracketed addresses together when splitting instruction operands.
/// This also catches malformed addresses before they reach an encoder.
pub(super) fn operands(text: &str) -> Result<Operands<'_>, String> {
    if text.is_empty() {
        return Ok(Operands::new());
    }
    let mut result = Operands::new();
    let mut bracket = false;
    let mut brace = false;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        match c {
            '[' if bracket => return Err("nested memory brackets are not supported".into()),
            '[' => bracket = true,
            ']' if !bracket => return Err("unexpected closing memory bracket".into()),
            ']' => bracket = false,
            '{' if brace => return Err("nested register lists are not supported".into()),
            '{' => brace = true,
            '}' if !brace => return Err("unexpected closing register-list brace".into()),
            '}' => brace = false,
            ',' if !bracket && !brace => {
                result.push(text[start..index].trim_fast());

                start = index + 1;
            }
            _ => {}
        }
    }
    if bracket {
        return Err("unclosed memory bracket".into());
    }
    if brace {
        return Err("unclosed register-list brace".into());
    }
    result.push(text[start..].trim_fast());
    Ok(result)
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Offset,
    Pre,
    Post,
}
enum Offset {
    Immediate(i64),
    Register {
        index: Reg,
        option: u32,
        shift: Option<u32>,
    },
}
struct Address {
    base: Reg,
    offset: Offset,
    mode: Mode,
}

fn address(args: &[&str]) -> Result<Address, String> {
    count(args, 1, 2)?;
    let text = args[0].trim_fast();
    let (text, pre) = if let Some(text) = text.strip_suffix('!') {
        (text.trim_end(), true)
    } else {
        (text, false)
    };
    let inner = text
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("expected bracketed memory address")?;
    let args_inner = Operands::collect(inner.split(',').map(str::trim));
    count(&args_inner, 1, 3)?;
    let base = reg(args_inner[0], true, false)?;
    if base.bits != 64 {
        return Err("memory base must be an X register or sp".into());
    }
    if args.len() == 2 {
        if pre || args_inner.len() != 1 {
            return Err("post-index addressing requires [base], #offset".into());
        }
        return Ok(Address {
            base,
            offset: Offset::Immediate(integer(args[1])?),
            mode: Mode::Post,
        });
    }
    let mode = if pre { Mode::Pre } else { Mode::Offset };
    if args_inner.len() == 1 {
        if pre {
            return Err("pre-index addressing requires [base, #offset]!".into());
        }
        return Ok(Address {
            base,
            offset: Offset::Immediate(0),
            mode,
        });
    }
    if is_integer(args_inner[1]) {
        count(&args_inner, 2, 2)?;
        return Ok(Address {
            base,
            offset: Offset::Immediate(integer(args_inner[1])?),
            mode,
        });
    }
    if pre {
        return Err("register offsets do not support writeback".into());
    }
    let index = reg(args_inner[1], false, true)?;
    let (option, amount) = if args_inner.len() == 3 {
        let words = Operands::collect(args_inner[2].split_whitespace());
        count(&words, 1, 2)?;
        let kind = lower(words[0]);
        let option =
            match (kind.as_ref(), index.bits) {
                ("uxtw", 32) => 2,
                ("lsl", 64) => 3,
                ("sxtw", 32) => 6,
                ("sxtx", 64) => 7,
                _ => return Err(
                    "memory index requires uxtw/sxtw for W registers or lsl/sxtx for X registers"
                        .into(),
                ),
            };
        if kind == "lsl" && words.len() != 2 {
            return Err("lsl requires a shift amount".into());
        }
        let amount = words.get(1).map(|s| unsigned(s, 4)).transpose()?;
        (option, amount)
    } else {
        if index.bits != 64 {
            return Err("W register index requires uxtw or sxtw".into());
        }
        (3, None)
    };
    Ok(Address {
        base,
        offset: Offset::Register {
            index,
            option,
            shift: amount,
        },
        mode,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Gpr,
    Fp,
}
struct Transfer {
    number: u32,
    bits: u32,
    class: Class,
    tag: &'static str,
}
fn transfer(s: &str) -> Result<Transfer, String> {
    let s = lower(s);
    let bits = match s.as_bytes().first() {
        Some(b'b') => 8,
        Some(b'h') => 16,
        Some(b's') if s != "sp" => 32,
        Some(b'd') => 64,
        Some(b'q') => 128,
        _ => {
            let r = reg(&s, false, true)?;
            return Ok(Transfer {
                number: r.number,
                bits: r.bits,
                class: Class::Gpr,
                tag: if r.bits == 64 { "64" } else { "32" },
            });
        }
    };
    let number = s[1..]
        .parse::<u32>()
        .map_err(|_| format!("invalid scalar FP/SIMD register `{s}`"))?;
    if number > 31 {
        return Err(format!("invalid scalar FP/SIMD register `{s}`"));
    }
    Ok(Transfer {
        number,
        bits,
        class: Class::Fp,
        tag: match bits {
            8 => "B",
            16 => "H",
            32 => "S",
            64 => "D",
            128 => "Q",
            _ => unreachable!(),
        },
    })
}

fn signed_scaled(offset: i64, scale: i64, bits: u32) -> Result<u32, String> {
    let bound = 1i64 << (bits - 1);
    if offset % scale != 0 || !(-bound * scale..=(bound - 1) * scale).contains(&offset) {
        return Err(format!(
            "memory offset must be a multiple of {scale} in {}..={}",
            -bound * scale,
            (bound - 1) * scale
        ));
    }
    Ok(((offset / scale) as u32) & ((1 << bits) - 1))
}
fn writeback_overlap(t: &Transfer, address: &Address) -> Result<(), String> {
    if address.mode != Mode::Offset
        && t.class == Class::Gpr
        && address.base.kind != Kind::Sp
        && t.number == address.base.number
    {
        return Err("writeback base overlaps the transfer register".into());
    }
    Ok(())
}

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    if matches!(op, "ldp" | "stp") {
        return pair(op, args);
    }
    if matches!(op, "ldar" | "ldarb" | "ldarh" | "stlr" | "stlrb" | "stlrh") {
        return ordered(op, args);
    }
    count(args, 2, 3)?;
    let t = transfer(args[0])?;
    let address = address(&args[1..])?;
    let unscaled = op.starts_with("ldur") || op.starts_with("stur");
    let plain = match op {
        "ldur" => "ldr",
        "stur" => "str",
        "ldurb" => "ldrb",
        "sturb" => "strb",
        "ldurh" => "ldrh",
        "sturh" => "strh",
        "ldursb" => "ldrsb",
        "ldursh" => "ldrsh",
        "ldursw" => "ldrsw",
        _ => op,
    };
    let bytes = match plain {
        "ldrb" | "strb" | "ldrh" | "strh" => {
            if t.class != Class::Gpr || t.bits != 32 {
                return Err("byte/halfword transfers require a W register".into());
            }
            if plain.ends_with('b') { 1 } else { 2 }
        }
        "ldrsb" | "ldrsh" => {
            if t.class != Class::Gpr {
                return Err("signed loads require a W or X register".into());
            }
            if plain.ends_with('b') { 1 } else { 2 }
        }
        "ldrsw" => {
            if t.class != Class::Gpr || t.bits != 64 {
                return Err("ldrsw requires an X register".into());
            }
            4
        }
        "ldr" | "str" => t.bits / 8,
        _ => return Err(format!("unsupported memory instruction `{op}`")),
    };
    writeback_overlap(&t, &address)?;
    let mut fields = [
        (FieldId::Rt, t.number),
        (FieldId::Rn, address.base.number),
        (FieldId::Rd, 0),
        (FieldId::Rd, 0),
        (FieldId::Rd, 0),
    ];
    let mut field_count = 3;
    let (encoding_op, tag, form) = match address.offset {
        Offset::Immediate(offset) => {
            if unscaled && address.mode != Mode::Offset {
                return Err("unscaled instructions do not support writeback".into());
            }
            let (encoding_op, form) = match address.mode {
                Mode::Pre => (op, "immpre"),
                Mode::Post => (op, "immpost"),
                Mode::Offset
                    if !unscaled
                        && offset >= 0
                        && offset % i64::from(bytes) == 0
                        && offset / i64::from(bytes) <= 4095 =>
                {
                    fields[2] = (FieldId::imm12, (offset / i64::from(bytes)) as u32);
                    (op, "pos")
                }
                Mode::Offset => (
                    match plain {
                        "ldr" => "ldur",
                        "str" => "stur",
                        "ldrb" => "ldurb",
                        "strb" => "sturb",
                        "ldrh" => "ldurh",
                        "strh" => "sturh",
                        "ldrsb" => "ldursb",
                        "ldrsh" => "ldursh",
                        "ldrsw" => "ldursw",
                        _ => unreachable!(),
                    },
                    "unscaled",
                ),
            };
            if form != "pos" {
                fields[2] = (FieldId::imm9, signed_scaled(offset, 1, 9)?);
            }
            (encoding_op, t.tag, form)
        }
        Offset::Register {
            index,
            option,
            shift,
        } => {
            if unscaled {
                return Err("unscaled instructions require an immediate offset".into());
            }
            let scale = bytes.trailing_zeros();
            let amount = shift.unwrap_or(0);
            if amount != 0 && amount != scale {
                return Err(format!("memory index shift must be 0 or {scale}"));
            }
            // For byte transfers, explicit #0 sets S=1; omitted shift sets S=0.
            let scaled = u32::from(shift.is_some() && amount == scale);
            fields[2] = (FieldId::Rm, index.number);
            fields[3] = (FieldId::S, scaled);
            field_count = 4;
            let tag = if bytes == 1 {
                if option == 3 {
                    if t.class == Class::Fp {
                        "BL"
                    } else if t.bits == 64 {
                        "64BL"
                    } else {
                        "32BL"
                    }
                } else {
                    fields[4] = (FieldId::option, option);
                    field_count = 5;
                    if t.class == Class::Fp {
                        "B"
                    } else if t.bits == 64 {
                        "64B"
                    } else {
                        "32B"
                    }
                }
            } else {
                fields[4] = (FieldId::option, option);
                field_count = 5;
                t.tag
            };
            (op, tag, "regoff")
        }
    };
    encode_named(
        format_args!("{}_{}_ldst_{form}", upper(encoding_op), tag),
        &fields[..field_count],
    )
}

fn pair(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 3, 4)?;
    let a = transfer(args[0])?;
    let b = transfer(args[1])?;
    if a.bits != b.bits || a.class != b.class || a.bits < 32 {
        return Err("register pairs require matching W, X, S, D, or Q registers".into());
    }
    if op == "ldp" && a.number == b.number {
        return Err("load-pair destination registers must differ".into());
    }
    let address = address(&args[2..])?;
    writeback_overlap(&a, &address)?;
    writeback_overlap(&b, &address)?;
    let Offset::Immediate(offset) = address.offset else {
        return Err("register pairs require an immediate offset".into());
    };
    let imm7 = signed_scaled(offset, i64::from(a.bits / 8), 7)?;
    let form = match address.mode {
        Mode::Offset => "off",
        Mode::Pre => "pre",
        Mode::Post => "post",
    };
    encode_named(
        format_args!("{}_{}_ldstpair_{form}", upper(op), a.tag),
        &[
            (FieldId::Rt, a.number),
            (FieldId::Rt2, b.number),
            (FieldId::Rn, address.base.number),
            (FieldId::imm7, imm7),
        ],
    )
}

fn ordered(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 2, 2)?;
    let t = transfer(args[0])?;
    if t.class != Class::Gpr || (op.ends_with(['b', 'h']) && t.bits != 32) {
        return Err("ordered loads/stores require W/X registers (W for byte/halfword)".into());
    }
    let address = address(&args[1..])?;
    if address.mode != Mode::Offset || !matches!(address.offset, Offset::Immediate(0)) {
        return Err(
            "ordered loads/stores require [base] with no nonzero offset or writeback".into(),
        );
    }
    let order = if op.starts_with("ld") { "LR" } else { "SL" };
    encode_named(
        format_args!("{}_{order}{}_ldstord", upper(op), t.bits),
        &[(FieldId::Rt, t.number), (FieldId::Rn, address.base.number)],
    )
}

/// LSE atomics use only a base address, optionally spelled with #0.
pub(super) fn atomic_base(text: &str) -> Result<Reg, String> {
    let a = address(&[text])?;
    if a.mode != Mode::Offset || !matches!(a.offset, Offset::Immediate(0)) {
        return Err("atomic addresses require [base] or [base, #0], without writeback".into());
    }
    Ok(a.base)
}

/// Literal loads share signed imm19 PC-relative fixups with conditional branches.
/// Mach-O has no relocation for these forms, so symbols must stay in one atom.
pub(super) fn literal<'a>(
    op: &str,
    args: &[&'a str],
    arena: &'a bumpalo::Bump,
) -> Result<Option<super::control::Branch<'a>>, String> {
    if !matches!(op, "ldr" | "ldrsw") || !args.get(1).is_some_and(|s| !s.starts_with('[')) {
        return Ok(None);
    }
    count(args, 2, 2)?;
    let t = transfer(args[0])?;
    if t.bits < 32 || (op == "ldrsw" && (t.class != Class::Gpr || t.bits != 64)) {
        return Err("literal loads require W/X/S/D/Q (X for ldrsw)".into());
    }
    Ok(Some(super::control::Branch {
        encoding: match (op, t.tag) {
            ("ldrsw", _) => "LDRSW_64_loadlit",
            (_, "32") => "LDR_32_loadlit",
            (_, "64") => "LDR_64_loadlit",
            (_, "S") => "LDR_S_loadlit",
            (_, "D") => "LDR_D_loadlit",
            (_, "Q") => "LDR_Q_loadlit",
            _ => return Err("unsupported literal load register".into()),
        },
        field: FieldId::imm19,
        width: 19,
        fields: bumpalo::vec![in arena; (FieldId::Rt, t.number)],
        target: args[1],
    }))
}
