//! Scalar integer / FP / SIMD memory operands. Immediate offsets are byte
//! offsets in source; scaling and signed field packing happen only at encoding.
use super::{Kind, Reg, count, integer, is_integer, reg, unsigned};
use crate::encode;

/// Keep bracketed addresses together when splitting instruction operands.
/// This also catches malformed addresses before they reach an encoder.
pub(super) fn operands(text: &str) -> Result<Vec<&str>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut bracket = false;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        match c {
            '[' if bracket => return Err("nested memory brackets are not supported".into()),
            '[' => bracket = true,
            ']' if !bracket => return Err("unexpected closing memory bracket".into()),
            ']' => bracket = false,
            ',' if !bracket => {
                result.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if bracket {
        return Err("unclosed memory bracket".into());
    }
    result.push(text[start..].trim());
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
    let text = args[0].trim();
    let (text, pre) = if let Some(text) = text.strip_suffix('!') {
        (text.trim_end(), true)
    } else {
        (text, false)
    };
    let inner = text
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("expected bracketed memory address; literal loads are not supported yet")?;
    let args_inner: Vec<_> = inner.split(',').map(str::trim).collect();
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
        let words: Vec<_> = args_inner[2].split_whitespace().collect();
        count(&words, 1, 2)?;
        let kind = words[0].to_ascii_lowercase();
        let option =
            match (kind.as_str(), index.bits) {
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
    tag: String,
}
fn transfer(s: &str) -> Result<Transfer, String> {
    let s = s.to_ascii_lowercase();
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
                tag: r.bits.to_string(),
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
        tag: s[..1].to_ascii_uppercase(),
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
    let plain = op.replace("ldur", "ldr").replace("stur", "str");
    let bytes = match plain.as_str() {
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
    let base_fields = [("Rt", t.number), ("Rn", address.base.number)];
    let mut fields = base_fields.to_vec();
    let (encoding_op, tag, form) = match address.offset {
        Offset::Immediate(offset) => {
            if unscaled && address.mode != Mode::Offset {
                return Err("unscaled instructions do not support writeback".into());
            }
            let (encoding_op, form) = match address.mode {
                Mode::Pre => (op.to_string(), "immpre"),
                Mode::Post => (op.to_string(), "immpost"),
                Mode::Offset
                    if !unscaled
                        && offset >= 0
                        && offset % i64::from(bytes) == 0
                        && offset / i64::from(bytes) <= 4095 =>
                {
                    fields.push(("imm12", (offset / i64::from(bytes)) as u32));
                    (op.to_string(), "pos")
                }
                Mode::Offset => (
                    plain.replace("ldr", "ldur").replace("str", "stur"),
                    "unscaled",
                ),
            };
            if form != "pos" {
                fields.push(("imm9", signed_scaled(offset, 1, 9)?));
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
            fields.extend_from_slice(&[("Rm", index.number), ("S", scaled)]);
            let tag = if bytes == 1 {
                let tag = if t.class == Class::Fp {
                    t.tag
                } else {
                    format!("{}B", t.tag)
                };
                if option == 3 {
                    format!("{tag}L")
                } else {
                    fields.push(("option", option));
                    tag
                }
            } else {
                fields.push(("option", option));
                t.tag
            };
            (op.to_string(), tag, "regoff")
        }
    };
    encode(
        &format!("{}_{}_ldst_{form}", encoding_op.to_ascii_uppercase(), tag),
        &fields,
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
    encode(
        &format!("{}_{}_ldstpair_{form}", op.to_ascii_uppercase(), a.tag),
        &[
            ("Rt", a.number),
            ("Rt2", b.number),
            ("Rn", address.base.number),
            ("imm7", imm7),
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
    encode(
        &format!("{}_{order}{}_ldstord", op.to_ascii_uppercase(), t.bits),
        &[("Rt", t.number), ("Rn", address.base.number)],
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
