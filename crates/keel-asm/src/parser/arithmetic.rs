use super::control::condition;
use super::storage::{Operands, encode_named, lower, upper};
use super::text::Trim;
use super::{
    Kind, Reg, count, instruction as base_instruction, is_integer, reg, same_width, shift, unsigned,
};
use crate::FieldId;
use crate::encode;

pub(super) fn bits_immediate(text: &str, bits: u32) -> Result<u64, String> {
    let text = text
        .trim_fast()
        .strip_prefix('#')
        .unwrap_or(text.trim_fast())
        .trim_fast();
    let (negative, digits) = if let Some(s) = text.strip_prefix('-') {
        (true, s)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    if digits.starts_with(['-', '+']) {
        return Err(format!("invalid bit-pattern immediate `{text}`"));
    }
    let value = if let Some(s) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i128::from_str_radix(s, 16)
    } else {
        digits.parse::<i128>()
    }
    .map_err(|_| format!("invalid bit-pattern immediate `{text}`"))?;
    let value = if negative { -value } else { value };
    if value < -(1i128 << (bits - 1)) || value >= 1i128 << bits {
        return Err(format!("immediate does not fit {bits} bits"));
    }
    Ok((value as u64) & (u64::MAX >> (64 - bits)))
}

/// Invert A64's repeated, rotated run-of-ones bitmask representation. Choose
/// the smallest repeating element and canonical rotation, matching clang.
fn bitmask(value: u64, bits: u32) -> Result<(u32, u32, u32), String> {
    for size in [2, 4, 8, 16, 32, 64] {
        if size > bits {
            break;
        }
        let mask = u64::MAX >> (64 - size);
        let element = value & mask;
        let ones = element.count_ones();
        if ones == 0 || ones == size {
            continue;
        }
        let mut repeated = 0;
        for offset in (0..bits).step_by(size as usize) {
            repeated |= element << offset;
        }
        if repeated != value {
            continue;
        }
        let run = (1u64 << ones) - 1;
        for rotation in 0..size {
            let rotated = if rotation == 0 {
                run
            } else {
                ((run >> rotation) | (run << (size - rotation))) & mask
            };
            if rotated == element {
                return Ok((
                    u32::from(size == 64),
                    rotation,
                    ((-(2 * size as i32)) as u32 | (ones - 1)) & 63,
                ));
            }
        }
    }
    Err("immediate is not an A64 logical bitmask (zero/all-ones are not encodable)".into())
}

pub(super) fn move_immediate(args: &[&str]) -> Result<u32, String> {
    let d = reg(args[0], true, true)?;
    let value = bits_immediate(args[1], d.bits)?;
    let mask = u64::MAX >> (64 - d.bits);
    if d.kind != Kind::Sp {
        for (op, pattern) in [("MOVZ", value), ("MOVN", !value & mask)] {
            for lane in (0..d.bits).step_by(16) {
                if pattern & !(0xffffu64 << lane) == 0 {
                    return encode_named(
                        format_args!("{op}_{}_movewide", d.bits),
                        &[
                            (FieldId::Rd, d.number),
                            (FieldId::hw, lane / 16),
                            (FieldId::imm16, (pattern >> lane) as u32),
                        ],
                    );
                }
            }
        }
    }
    let zero = if d.bits == 64 { "xzr" } else { "wzr" };
    instruction("orr", &[args[0], zero, args[1]])
}

pub(super) fn extended(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 3, 4)?;
    let flags = op.ends_with('s');
    let d = reg(args[0], !flags, flags)?;
    let n = reg(args[1], true, false)?;
    same_width(d, n)?;
    let m = reg(args[2], false, true)?;
    let words = Operands::collect(args.get(3).copied().unwrap_or("").split_whitespace());
    let (option, amount) = if words.is_empty() {
        (if m.bits == 64 { 3 } else { 2 }, 0)
    } else {
        count(&words, 1, 2)?;
        let kind = lower(words[0]);
        let option = match kind.as_ref() {
            "uxtb" => 0,
            "uxth" => 1,
            "uxtw" => 2,
            "uxtx" => 3,
            "sxtb" => 4,
            "sxth" => 5,
            "sxtw" => 6,
            "sxtx" => 7,
            "lsl" => {
                if m.bits == 64 {
                    3
                } else {
                    2
                }
            }
            _ => return Err("invalid add/sub register extension".into()),
        };
        let amount = words
            .get(1)
            .map(|s| unsigned(s, 4))
            .transpose()?
            .unwrap_or(0);
        if kind == "lsl" && words.len() != 2 {
            return Err("lsl requires a shift amount".into());
        }
        (option, amount)
    };
    let expected = if d.bits == 64 && [3, 7].contains(&option) {
        64
    } else {
        32
    };
    if m.bits != expected {
        return Err("source width does not match the register extension".into());
    }
    encode_named(
        format_args!(
            "{}_{}{}_addsub_ext",
            upper(op),
            d.bits,
            if flags { "S" } else { "" }
        ),
        &[
            (FieldId::Rd, d.number),
            (FieldId::Rn, n.number),
            (FieldId::Rm, m.number),
            (FieldId::option, option),
            (FieldId::imm3, amount),
        ],
    )
}

fn regs(args: &[&str]) -> Result<Vec<Reg>, String> {
    let registers: Vec<_> = args
        .iter()
        .map(|s| reg(s, false, true))
        .collect::<Result<_, _>>()?;
    for r in &registers[1..] {
        same_width(registers[0], *r)?;
    }
    Ok(registers)
}

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    match op {
        "and" | "ands" | "orr" | "eor" | "bic" | "bics" | "orn" | "eon" => {
            count(args, 3, 4)?;
            let immediate = is_integer(args[2]);
            let d = reg(
                args[0],
                immediate && op != "ands",
                !immediate || op == "ands",
            )?;
            let n = reg(args[1], false, true)?;
            same_width(d, n)?;
            if immediate {
                count(args, 3, 3)?;
                if !["and", "ands", "orr", "eor"].contains(&op) {
                    return Err("this logical instruction requires a register operand".into());
                }
                let (bit, immr, imms) = bitmask(bits_immediate(args[2], d.bits)?, d.bits)?;
                let mut fields = vec![
                    (FieldId::Rd, d.number),
                    (FieldId::Rn, n.number),
                    (FieldId::immr, immr),
                    (FieldId::imms, imms),
                ];
                if d.bits == 64 {
                    fields.push((FieldId::N, bit));
                }
                encode_named(
                    format_args!(
                        "{}_{}{}_log_imm",
                        upper(op),
                        d.bits,
                        if op == "ands" { "S" } else { "" }
                    ),
                    &fields,
                )
            } else {
                let m = reg(args[2], false, true)?;
                same_width(d, m)?;
                let (kind, amount) = if args.len() == 4 {
                    shift(args[3])?
                } else {
                    ("lsl", 0)
                };
                let shift = match kind {
                    "lsl" => 0,
                    "lsr" => 1,
                    "asr" => 2,
                    "ror" => 3,
                    _ => return Err("invalid logical register shift".into()),
                };
                if amount >= d.bits {
                    return Err("shift exceeds register width".into());
                }
                encode_named(
                    format_args!("{}_{}_log_shift", upper(op), d.bits),
                    &[
                        (FieldId::Rd, d.number),
                        (FieldId::Rn, n.number),
                        (FieldId::Rm, m.number),
                        (FieldId::shift, shift),
                        (FieldId::imm6, amount),
                    ],
                )
            }
        }
        "neg" | "negs" | "mvn" | "tst" => {
            count(args, 2, 3)?;
            let d = reg(args[0], false, true)?;
            let zero = if d.bits == 64 { "xzr" } else { "wzr" };
            let (opcode, mut expanded) = match op {
                "tst" => ("ands", vec![zero, args[0], args[1]]),
                "mvn" => ("orn", vec![args[0], zero, args[1]]),
                _ => (
                    if op == "neg" { "sub" } else { "subs" },
                    vec![args[0], zero, args[1]],
                ),
            };
            if let Some(shift) = args.get(2) {
                expanded.push(shift);
            }
            base_instruction(opcode, &expanded)
        }
        "ngc" | "ngcs" => {
            count(args, 2, 2)?;
            let d = reg(args[0], false, true)?;
            base_instruction(
                if op == "ngc" { "sbc" } else { "sbcs" },
                &[args[0], if d.bits == 64 { "xzr" } else { "wzr" }, args[1]],
            )
        }
        "adc" | "adcs" | "sbc" | "sbcs" | "sdiv" | "udiv" | "lslv" | "lsrv" | "asrv" | "rorv" => {
            count(args, 3, 3)?;
            let r = regs(args)?;
            let form = if op.contains("div") || op.ends_with('v') {
                "dp_2src"
            } else {
                "addsub_carry"
            };
            encode_named(
                format_args!("{}_{}_{form}", upper(op), r[0].bits),
                &[
                    (FieldId::Rd, r[0].number),
                    (FieldId::Rn, r[1].number),
                    (FieldId::Rm, r[2].number),
                ],
            )
        }
        "clz" | "cls" | "rbit" | "rev" | "rev16" | "rev32" => {
            count(args, 2, 2)?;
            let r = regs(args)?;
            if op == "rev32" && r[0].bits != 64 {
                return Err("rev32 requires X registers".into());
            }
            encode_named(
                format_args!("{}_{}_dp_1src", upper(op), r[0].bits),
                &[(FieldId::Rd, r[0].number), (FieldId::Rn, r[1].number)],
            )
        }
        "mul" | "mneg" | "madd" | "msub" => {
            let alias = op == "mul" || op == "mneg";
            count(args, if alias { 3 } else { 4 }, if alias { 3 } else { 4 })?;
            let r = regs(args)?;
            encode_named(
                format_args!(
                    "{}_{}A_dp_3src",
                    if op == "mneg" || op == "msub" {
                        "MSUB"
                    } else {
                        "MADD"
                    },
                    r[0].bits
                ),
                &[
                    (FieldId::Rd, r[0].number),
                    (FieldId::Rn, r[1].number),
                    (FieldId::Rm, r[2].number),
                    (FieldId::Ra, if alias { 31 } else { r[3].number }),
                ],
            )
        }
        "smulh" | "umulh" | "smull" | "umull" | "smaddl" | "umaddl" => {
            let long = op.ends_with('l');
            let accumulate = op.contains("add");
            count(
                args,
                if accumulate { 4 } else { 3 },
                if accumulate { 4 } else { 3 },
            )?;
            let r: Vec<_> = args
                .iter()
                .map(|s| reg(s, false, true))
                .collect::<Result<_, _>>()?;
            if r[0].bits != 64
                || r[1].bits != if long { 32 } else { 64 }
                || r[2].bits != r[1].bits
                || (accumulate && r[3].bits != 64)
            {
                return Err("invalid widening/high multiply register widths".into());
            }
            let mut fields = vec![
                (FieldId::Rd, r[0].number),
                (FieldId::Rn, r[1].number),
                (FieldId::Rm, r[2].number),
            ];
            let name = if long {
                fields.push((FieldId::Ra, if accumulate { r[3].number } else { 31 }));
                format!(
                    "{}MADDL_64WA_dp_3src",
                    if op.starts_with('s') { "S" } else { "U" }
                )
            } else {
                format!("{}_64_dp_3src", upper(op))
            };
            encode(&name, &fields)
        }
        "csel" | "csinc" | "csinv" | "csneg" | "cset" | "csetm" | "cinc" | "cinv" | "cneg" => {
            let set = op == "cset" || op == "csetm";
            let alias = set || ["cinc", "cinv", "cneg"].contains(&op);
            count(
                args,
                if set {
                    2
                } else if alias {
                    3
                } else {
                    4
                },
                if set {
                    2
                } else if alias {
                    3
                } else {
                    4
                },
            )?;
            let d = reg(args[0], false, true)?;
            let (n, m) = if set {
                (31, 31)
            } else {
                let n = reg(args[1], false, true)?;
                let m = reg(if alias { args[1] } else { args[2] }, false, true)?;
                same_width(d, n)?;
                same_width(d, m)?;
                (n.number, m.number)
            };
            let mut cc = condition(args[args.len() - 1])?;
            if alias {
                if cc >= 14 {
                    return Err("conditional aliases cannot use al/nv".into());
                }
                cc ^= 1;
            }
            let canonical = match op {
                "cset" | "cinc" => "csinc",
                "csetm" | "cinv" => "csinv",
                "cneg" => "csneg",
                _ => op,
            };
            encode_named(
                format_args!("{}_{}_condsel", upper(canonical), d.bits),
                &[
                    (FieldId::Rd, d.number),
                    (FieldId::Rn, n),
                    (FieldId::Rm, m),
                    (FieldId::cond, cc),
                ],
            )
        }
        "ccmp" | "ccmn" => {
            count(args, 4, 4)?;
            let n = reg(args[0], false, true)?;
            let mut fields = vec![
                (FieldId::Rn, n.number),
                (FieldId::nzcv, unsigned(args[2], 15)?),
                (FieldId::cond, condition(args[3])?),
            ];
            let form = if is_integer(args[1]) {
                fields.push((FieldId::imm5, unsigned(args[1], 31)?));
                "imm"
            } else {
                let m = reg(args[1], false, true)?;
                same_width(n, m)?;
                fields.push((FieldId::Rm, m.number));
                "reg"
            };
            encode_named(
                format_args!("{}_{}_condcmp_{form}", upper(op), n.bits),
                &fields,
            )
        }
        "lsl" | "lsr" | "asr" | "ror" | "sbfm" | "ubfm" | "bfm" | "bfi" | "bfxil" | "ubfx"
        | "sbfx" | "extr" | "uxtb" | "uxth" | "sxtb" | "sxth" | "sxtw" => bitfield(op, args),
        _ => Err(format!("unsupported instruction `{op}`")),
    }
}

fn bitfield(op: &str, args: &[&str]) -> Result<u32, String> {
    let extension = op.starts_with("sxt") || op.starts_with("uxt");
    let shift_op = ["lsl", "lsr", "asr", "ror"].contains(&op);
    count(
        args,
        if extension {
            2
        } else if shift_op {
            3
        } else {
            4
        },
        if extension {
            2
        } else if shift_op {
            3
        } else {
            4
        },
    )?;
    if shift_op && !is_integer(args[2]) {
        return instruction(&format!("{op}v"), args);
    }
    let d = reg(args[0], false, true)?;
    let n = reg(args[1], false, true)?;
    if extension {
        if n.bits != 32 || (op == "sxtw" && d.bits != 64) || (op.starts_with('u') && d.bits != 32) {
            return Err("invalid extension register widths".into());
        }
    } else {
        same_width(d, n)?;
    }
    let mut fields = vec![(FieldId::Rd, d.number), (FieldId::Rn, n.number)];
    let (canonical, immr, imms) = if extension {
        (
            if op.starts_with('s') { "SBFM" } else { "UBFM" },
            0,
            match op {
                "sxtb" | "uxtb" => 7,
                "sxth" | "uxth" => 15,
                _ => 31,
            },
        )
    } else {
        let first = if op == "extr" {
            0
        } else {
            unsigned(args[2], i64::from(d.bits - 1))?
        };
        if op == "ror" || op == "extr" {
            let (m, amount) = if op == "extr" {
                let m = reg(args[2], false, true)?;
                same_width(d, m)?;
                (m.number, unsigned(args[3], i64::from(d.bits - 1))?)
            } else {
                (n.number, first)
            };
            fields.extend_from_slice(&[(FieldId::Rm, m), (FieldId::imms, amount)]);
            return encode_named(format_args!("EXTR_{}_extract", d.bits), &fields);
        }
        if shift_op {
            match op {
                "lsl" => ("UBFM", (d.bits - first) % d.bits, d.bits - 1 - first),
                "lsr" => ("UBFM", first, d.bits - 1),
                _ => ("SBFM", first, d.bits - 1),
            }
        } else if ["sbfm", "ubfm", "bfm"].contains(&op) {
            (
                if op == "sbfm" {
                    "SBFM"
                } else if op == "ubfm" {
                    "UBFM"
                } else {
                    "BFM"
                },
                first,
                unsigned(args[3], i64::from(d.bits - 1))?,
            )
        } else {
            let width = unsigned(args[3], i64::from(d.bits - first))?;
            if width == 0 {
                return Err("bitfield width must be positive".into());
            }
            match op {
                "bfi" => ("BFM", (d.bits - first) % d.bits, width - 1),
                "bfxil" => ("BFM", first, first + width - 1),
                "sbfx" => ("SBFM", first, first + width - 1),
                _ => ("UBFM", first, first + width - 1),
            }
        }
    };
    fields.extend_from_slice(&[(FieldId::immr, immr), (FieldId::imms, imms)]);
    encode_named(format_args!("{canonical}_{}M_bitfield", d.bits), &fields)
}
