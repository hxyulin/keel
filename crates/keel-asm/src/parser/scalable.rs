//! Explicitly opted-in SVE/SME forms. Assembly validates feature availability;
//! execution mode, OS support, and ZA lifetime remain caller responsibilities.
mod matrix;
mod streaming;

use super::{count, integer, reg, shift, unsigned};
use crate::{TargetFeatures, encode_with_features};

fn number(text: &str, prefix: &str, max: u32) -> Result<u32, String> {
    let number = text
        .strip_prefix(prefix)
        .ok_or("invalid scalable register class")?
        .parse::<u32>()
        .map_err(|_| "invalid scalable register number")?;
    if number > max {
        return Err("scalable register number out of range".into());
    }
    Ok(number)
}
fn lane(text: &str, prefix: &str, max: u32) -> Result<(u32, u32), String> {
    let (name, suffix) = text
        .split_once('.')
        .ok_or("scalable register requires an element size")?;
    let size = match suffix {
        "b" => 0,
        "h" => 1,
        "s" => 2,
        "d" => 3,
        _ => return Err("invalid scalable element size".into()),
    };
    Ok((number(name, prefix, max)?, size))
}
fn predicate(text: &str) -> Result<u32, String> {
    number(
        text.strip_suffix("/m")
            .ok_or("outer product requires P0..P7/m")?,
        "p",
        7,
    )
}
fn pattern(text: &str) -> Result<u32, String> {
    Ok(match text {
        "pow2" => 0,
        "vl1" => 1,
        "vl2" => 2,
        "vl3" => 3,
        "vl4" => 4,
        "vl5" => 5,
        "vl6" => 6,
        "vl7" => 7,
        "vl8" => 8,
        "vl16" => 9,
        "vl32" => 10,
        "vl64" => 11,
        "vl128" => 12,
        "vl256" => 13,
        "mul4" => 29,
        "mul3" => 30,
        "all" => 31,
        _ if text.starts_with('#') => unsigned(text, 31)?,
        _ => return Err("invalid scalable predicate pattern".into()),
    })
}

pub(super) fn instruction(
    op: &str,
    args: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    if let Some(word) = matrix::instruction(op, args, features)? {
        return Ok(Some(word));
    }
    if let Some(word) = streaming::instruction(op, args, features)? {
        return Ok(Some(word));
    }
    // Lowercase only operands handled here; ordinary instructions keep their
    // existing syntax and feature validation paths.
    let scalable_dest = args.first().is_some_and(|s| s.starts_with(['z', 'Z']));
    if !matches!(
        op,
        "smstart"
            | "smstop"
            | "ptrue"
            | "pfalse"
            | "rdvl"
            | "cntb"
            | "cnth"
            | "cntw"
            | "cntd"
            | "lastb"
    ) && !(scalable_dest
        && matches!(
            op,
            "add" | "sub" | "and" | "orr" | "eor" | "fadd" | "fsub" | "fmul" | "dup"
        ))
    {
        return Ok(None);
    }
    let args: Vec<_> = args.iter().map(|s| s.to_ascii_lowercase()).collect();
    let a: Vec<_> = args.iter().map(String::as_str).collect();
    let (name, fields) = match op {
        "smstart" | "smstop" => {
            count(&a, 0, 1)?;
            let target = match a.first().copied() {
                None => 6,
                Some("sm") => 2,
                Some("za") => 4,
                _ => return Err("smstart/smstop accepts only sm or za".into()),
            };
            (
                op.to_ascii_uppercase(),
                vec![
                    ("op1", 3),
                    ("op2", 3),
                    ("CRm", target | u32::from(op == "smstart")),
                ],
            )
        }
        "ptrue" => {
            count(&a, 1, 2)?;
            let (pd, size) = lane(a[0], "p", 15)?;
            (
                "ptrue_p_s_".into(),
                vec![
                    ("Pd", pd),
                    ("size", size),
                    ("pattern", pattern(a.get(1).copied().unwrap_or("all"))?),
                ],
            )
        }
        "pfalse" => {
            count(&a, 1, 1)?;
            let (pd, size) = lane(a[0], "p", 15)?;
            if size != 0 {
                return Err("pfalse requires .b".into());
            }
            ("pfalse_p_".into(), vec![("Pd", pd)])
        }
        "rdvl" => {
            count(&a, 2, 2)?;
            let rd = reg(a[0], false, true)?;
            let imm = integer(a[1])?;
            if rd.bits != 64 || !(-32..=31).contains(&imm) {
                return Err("rdvl requires X and an immediate in -32..31".into());
            }
            (
                "rdvl_r_i_".into(),
                vec![("Rd", rd.number), ("imm6", (imm as u32) & 63)],
            )
        }
        "cntb" | "cnth" | "cntw" | "cntd" => {
            count(&a, 1, 3)?;
            let rd = reg(a[0], false, true)?;
            if rd.bits != 64 {
                return Err("scalable element count requires X".into());
            }
            let multiplier = if a.len() == 3 {
                let imm = a[2]
                    .strip_prefix("mul ")
                    .ok_or("count multiplier requires mul #1..16")?;
                let n = unsigned(imm.trim(), 16)?;
                if n == 0 {
                    return Err("count multiplier must be 1..16".into());
                }
                n
            } else {
                1
            };
            (
                format!("{op}_r_s_"),
                vec![
                    ("Rd", rd.number),
                    ("pattern", pattern(a.get(1).copied().unwrap_or("all"))?),
                    ("imm4", multiplier - 1),
                ],
            )
        }
        "dup" => {
            count(&a, 2, 3)?;
            let (zd, size) = lane(a[0], "z", 31)?;
            if a.len() == 2 && !a[1].starts_with('#') && !super::is_integer(a[1]) {
                let rn = reg(a[1], true, false)?;
                if rn.bits != if size == 3 { 64 } else { 32 } {
                    return Err("dup from GPR requires W for B/H/S or X for D".into());
                }
                return encode_with_features(
                    "dup_z_r_",
                    &[("Zd", zd), ("size", size), ("Rn", rn.number)],
                    features,
                )
                .map(Some);
            }
            let mut imm = integer(a[1])?;
            let sh = if a.len() == 3 {
                match shift(a[2])? {
                    ("lsl", 0) => 0,
                    ("lsl", 8) => 1,
                    _ => return Err("dup shift must be lsl #0 or #8".into()),
                }
            } else if !(-128..=127).contains(&imm) && imm % 256 == 0 {
                imm /= 256;
                1
            } else {
                0
            };
            if !(-128..=127).contains(&imm) || (size == 0 && sh != 0) {
                return Err("dup immediate requires signed 8 bits and no shift for B lanes".into());
            }
            (
                "dup_z_i_".into(),
                vec![
                    ("Zd", zd),
                    ("size", size),
                    ("sh", sh),
                    ("imm8", (imm as u32) & 255),
                ],
            )
        }
        "lastb" => {
            count(&a, 3, 3)?;
            let rd = reg(a[0], false, true)?;
            let pg = number(a[1], "p", 7)?;
            let (zn, size) = lane(a[2], "z", 31)?;
            if rd.bits != if size == 3 { 64 } else { 32 } {
                return Err("lastb requires W for B/H/S or X for D".into());
            }
            (
                "lastb_r_p_z_".into(),
                vec![("Rd", rd.number), ("Pg", pg), ("Zn", zn), ("size", size)],
            )
        }
        _ => {
            count(&a, 3, 3)?;
            let (zd, size) = lane(a[0], "z", 31)?;
            let (zn, sn) = lane(a[1], "z", 31)?;
            let (zm, sm) = lane(a[2], "z", 31)?;
            if size != sn || size != sm {
                return Err("scalable operands require matching element sizes".into());
            }
            let logical = matches!(op, "and" | "orr" | "eor");
            if logical && size != 3 {
                return Err("unpredicated scalable logical forms require .d".into());
            }
            let mut fields = vec![("Zd", zd), ("Zn", zn), ("Zm", zm)];
            if !logical {
                fields.push(("size", size));
            }
            (format!("{op}_z_zz_"), fields)
        }
    };
    encode_with_features(&name, &fields, features).map(Some)
}
