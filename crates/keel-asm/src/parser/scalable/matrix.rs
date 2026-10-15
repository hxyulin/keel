//! SME tile operands. Opcode bits and feature predicates come only from the
//! pinned generated Arm records; these helpers validate assembly operand roles.
use super::{lane, number, predicate};
use crate::parser::{count, integer, reg, unsigned};
use crate::{TargetFeatures, encode_with_features};

type Fields = Vec<(&'static str, u32)>;

fn element(s: &str) -> Result<u32, String> {
    match s {
        "b" => Ok(0),
        "h" => Ok(1),
        "s" => Ok(2),
        "d" => Ok(3),
        "q" => Ok(4),
        _ => Err("invalid matrix element size".into()),
    }
}
fn tile(s: &str) -> Result<(u32, u32), String> {
    let (r, e) = s.split_once('.').ok_or("tile requires an element size")?;
    let size = element(e)?;
    Ok((number(r, "za", (1 << size) - 1)?, size))
}
struct Slice {
    tile: u32,
    size: u32,
    vertical: u32,
    selector: u32,
    offset: u32,
}
fn slice(s: &str) -> Result<Slice, String> {
    let (head, indices) = s
        .split_once('[')
        .ok_or("tile slice requires [W12..W15, offset]")?;
    let indices = indices.strip_suffix(']').ok_or("unclosed tile slice")?;
    let (r, e) = head
        .split_once('.')
        .ok_or("tile slice requires element size")?;
    let size = element(e)?;
    let (r, vertical) = if let Some(r) = r.strip_suffix('h') {
        (r, 0)
    } else if let Some(r) = r.strip_suffix('v') {
        (r, 1)
    } else {
        return Err("tile slice requires h or v orientation".into());
    };
    let tile = number(r, "za", (1 << size) - 1)?;
    let a: Vec<_> = indices.split(',').map(str::trim).collect();
    count(&a, 2, 2)?;
    let selector = number(a[0], "w", 15)?;
    if selector < 12 {
        return Err("tile slice selector must be W12..W15".into());
    }
    let offset = unsigned(a[1], (16 >> size) - 1)?;
    Ok(Slice {
        tile,
        size,
        vertical,
        selector: selector - 12,
        offset,
    })
}
fn slice_fields(s: &Slice, role: &'static str) -> Fields {
    let mut f = vec![("V", s.vertical), ("Rs", s.selector)];
    if s.size != 0 {
        f.push((role, s.tile));
    }
    if s.size != 4 {
        f.push((["off4", "off3", "off2", "o1"][s.size as usize], s.offset));
    }
    f
}
fn address(s: &str) -> Result<Vec<&str>, String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("expected bracketed address without writeback")?;
    Ok(inner.split(',').map(str::trim).collect())
}
fn base(s: &str) -> Result<u32, String> {
    let r = reg(s, true, false)?;
    if r.bits != 64 {
        return Err("matrix memory base must be X or SP".into());
    }
    Ok(r.number)
}
fn signed6(s: &str) -> Result<u32, String> {
    let n = integer(s)?;
    if !(-32..=31).contains(&n) {
        return Err("streaming length multiplier must be -32..31".into());
    }
    Ok(n as u32 & 63)
}

pub(super) fn instruction(
    op: &str,
    args: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    let mentions_za = args.iter().any(|s| {
        s.trim_start_matches(['{', ' ']).starts_with(['z', 'Z'])
            && s.to_ascii_lowercase().contains("za")
    });
    if !matches!(
        op,
        "addsvl"
            | "rdsvl"
            | "fmopa"
            | "fmops"
            | "bfmopa"
            | "bfmops"
            | "smopa"
            | "smops"
            | "sumopa"
            | "sumops"
            | "usmopa"
            | "usmops"
            | "umopa"
            | "umops"
            | "addha"
            | "addva"
    ) && !(mentions_za
        && matches!(
            op,
            "mov"
                | "mova"
                | "ldr"
                | "str"
                | "ld1b"
                | "ld1h"
                | "ld1w"
                | "ld1d"
                | "ld1q"
                | "st1b"
                | "st1h"
                | "st1w"
                | "st1d"
                | "st1q"
                | "zero"
        ))
    {
        return Ok(None);
    }
    let lower: Vec<_> = args.iter().map(|s| s.to_ascii_lowercase()).collect();
    let a: Vec<_> = lower.iter().map(String::as_str).collect();
    let (name, fields) = match op {
        "rdsvl" | "addsvl" => {
            count(
                &a,
                if op == "rdsvl" { 2 } else { 3 },
                if op == "rdsvl" { 2 } else { 3 },
            )?;
            let rd = reg(a[0], op == "addsvl", op == "rdsvl")?;
            if rd.bits != 64 {
                return Err("streaming length requires X registers".into());
            }
            let mut f = vec![("Rd", rd.number), ("imm6", signed6(a[a.len() - 1])?)];
            if op == "addsvl" {
                f.push(("Rn", base(a[1])?));
            }
            (
                if op == "rdsvl" {
                    "rdsvl_r_i_"
                } else {
                    "addsvl_r_ri_"
                }
                .into(),
                f,
            )
        }
        "zero" => {
            count(&a, 1, 1)?;
            let inner = a[0]
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .ok_or("zero requires a tile list")?
                .trim();
            let mut mask = 0;
            if inner == "za" {
                mask = 255;
            } else {
                for s in inner.split(',').map(str::trim) {
                    let (n, size) = tile(s)?;
                    if size > 3 {
                        return Err("zero tile list supports B/H/S/D".into());
                    }
                    for bit in (n..8).step_by(1 << size) {
                        mask |= 1 << bit;
                    }
                }
            }
            ("zero_za_i_".into(), vec![("imm8", mask)])
        }
        "mov" | "mova" => {
            count(&a, 3, 3)?;
            let insert = a[0].starts_with("za");
            let (sl, z) = if insert { (a[0], a[2]) } else { (a[2], a[0]) };
            let s = slice(sl)?;
            let (zn, size) = z.split_once('.').ok_or("vector requires element size")?;
            let z = number(zn, "z", 31)?;
            if element(size)? != s.size {
                return Err("tile and vector element sizes must match".into());
            }
            let mut f = slice_fields(&s, if insert { "ZAd" } else { "ZAn" });
            f.extend([
                ("Pg", predicate(a[1])?),
                (if insert { "Zn" } else { "Zd" }, z),
            ]);
            (
                format!(
                    "mova_{}_{}_{}",
                    if insert { "za_p" } else { "z_p" },
                    if insert { "rz" } else { "rza" },
                    ["b", "h", "w", "d", "q"][s.size as usize]
                ),
                f,
            )
        }
        "ldr" | "str" => {
            count(&a, 2, 2)?;
            let indices = a[0]
                .strip_prefix("za[")
                .and_then(|s| s.strip_suffix(']'))
                .ok_or("expected ZA[W12..W15, offset]")?;
            let indices: Vec<_> = indices.split(',').map(str::trim).collect();
            count(&indices, 2, 2)?;
            let rv = number(indices[0], "w", 15)?;
            if rv < 12 {
                return Err("ZA vector selector must be W12..W15".into());
            }
            let off = unsigned(indices[1], 15)?;
            let addr = address(a[1])?;
            count(&addr, 1, 3)?;
            if (addr.len() == 1 && off != 0)
                || (addr.len() != 1
                    && (addr.len() != 3 || unsigned(addr[1], 15)? != off || addr[2] != "mul vl"))
            {
                return Err("ZA and address offsets must match, with mul vl".into());
            }
            (
                format!("{op}_za_ri_"),
                vec![("Rv", rv - 12), ("Rn", base(addr[0])?), ("off4", off)],
            )
        }
        "ld1b" | "ld1h" | "ld1w" | "ld1d" | "ld1q" | "st1b" | "st1h" | "st1w" | "st1d" | "st1q" => {
            count(&a, 3, 3)?;
            let sl = a[0]
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or(a[0])
                .trim();
            let s = slice(sl)?;
            let size = ["b", "h", "w", "d", "q"][s.size as usize];
            if !op.ends_with(size) {
                return Err("matrix load/store width must match tile element".into());
            }
            let pg = if op.starts_with("ld") {
                number(
                    a[1].strip_suffix("/z")
                        .ok_or("matrix load requires P0..P7/z")?,
                    "p",
                    7,
                )?
            } else {
                number(a[1], "p", 7)?
            };
            let addr = address(a[2])?;
            count(&addr, 1, 3)?;
            let rm = if addr.len() > 1 {
                let r = reg(addr[1], false, true)?;
                if r.bits != 64 {
                    return Err("matrix memory index must be X".into());
                }
                if (s.size == 0 && addr.len() == 3 && addr[2] != "lsl #0")
                    || (s.size != 0 && (addr.len() != 3 || addr[2] != format!("lsl #{}", s.size)))
                {
                    return Err("matrix memory index shift must match element size".into());
                }
                r.number
            } else {
                31
            };
            let mut f = slice_fields(&s, "ZAt");
            f.extend([("Pg", pg), ("Rn", base(addr[0])?), ("Rm", rm)]);
            (format!("{op}_za_p_rrr_"), f)
        }
        _ => {
            let add = matches!(op, "addha" | "addva");
            count(&a, if add { 4 } else { 5 }, if add { 4 } else { 5 })?;
            let (za, size) = tile(a[0])?;
            let (zn, sn) = lane(a[3], "z", 31)?;
            let (zm, sm) = if add { (0, sn) } else { lane(a[4], "z", 31)? };
            if sn != sm {
                return Err("outer-product source sizes must match".into());
            }
            let name = if add {
                if size < 2 || sn != size {
                    return Err("addha/addva requires matching S or D elements".into());
                }
                format!("{op}_za_pp_z_{}", 8 << size)
            } else if op.starts_with('f') {
                match (size, sn) {
                    (2, 2) => format!("{op}_za_pp_zz_32"),
                    (2, 1) => format!("{op}_za32_pp_zz_16"),
                    (1, 1) => format!("{op}_za_pp_zz_16"),
                    (3, 3) => format!("{op}_za_pp_zz_64"),
                    _ => return Err("invalid floating-point outer-product widths".into()),
                }
            } else if op.starts_with("bf") {
                if sn != 1 || !matches!(size, 1 | 2) {
                    return Err("BF16 outer product requires H sources and H/S tile".into());
                }
                if size == 2 {
                    format!("{op}_za32_pp_zz_")
                } else {
                    format!("{op}_za_pp_zz_16")
                }
            } else {
                match (size, sn) {
                    (2, 0) => format!("{op}_za_pp_zz_32"),
                    (3, 1) => format!("{op}_za_pp_zz_64"),
                    (2, 1) if matches!(op, "smopa" | "smops" | "umopa" | "umops") => {
                        format!("{op}_za32_pp_zz_16")
                    }
                    _ => return Err("invalid integer outer-product widths".into()),
                }
            };
            let mut f = vec![
                ("ZAda", za),
                ("Pn", predicate(a[1])?),
                ("Pm", predicate(a[2])?),
                ("Zn", zn),
            ];
            if !add {
                f.push(("Zm", zm));
            }
            (name, f)
        }
    };
    encode_with_features(&name, &fields, features).map(Some)
}
