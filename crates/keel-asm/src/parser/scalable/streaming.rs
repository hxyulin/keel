//! Streaming-compatible Z memory and predicates. Gather/scatter and fault-first
//! forms are deliberately outside this contiguous-memory increment.
use super::{lane, number};
use crate::FieldId;
use crate::parser::{count, integer, reg, shift};
use crate::{TargetFeatures, encode_with_features};

pub(super) fn instruction(
    op: &str,
    args: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    let predicate = matches!(op, "whilelt" | "whilelo" | "whilele" | "whilels");
    let memory = matches!(
        op,
        "ld1b"
            | "ld1h"
            | "ld1w"
            | "ld1d"
            | "ld1sb"
            | "ld1sh"
            | "ld1sw"
            | "st1b"
            | "st1h"
            | "st1w"
            | "st1d"
    );
    if !predicate
        && op != "addvl"
        && !(memory
            && args.first().is_some_and(|s| {
                let s = s.trim_start_matches(['{', ' ']);
                s.starts_with(['z', 'Z'])
                    && !s.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("za"))
            }))
    {
        return Ok(None);
    }
    let lower: Vec<_> = args.iter().map(|s| s.to_ascii_lowercase()).collect();
    let a: Vec<_> = lower.iter().map(String::as_str).collect();
    let (name, fields) = if predicate {
        count(&a, 3, 3)?;
        let (pd, size) = lane(a[0], "p", 15)?;
        let rn = reg(a[1], false, true)?;
        let rm = reg(a[2], false, true)?;
        if rn.bits != rm.bits {
            return Err("while predicate operands require matching W/X widths".into());
        }
        (
            format!("{op}_p_p_rr_"),
            vec![
                (FieldId::Pd, pd),
                (FieldId::size, size),
                (FieldId::Rn, rn.number),
                (FieldId::Rm, rm.number),
                (FieldId::sf, u32::from(rn.bits == 64)),
            ],
        )
    } else if op == "addvl" {
        count(&a, 3, 3)?;
        let rd = reg(a[0], true, false)?;
        let rn = reg(a[1], true, false)?;
        let imm = integer(a[2])?;
        if rd.bits != 64 || rn.bits != 64 || !(-32..=31).contains(&imm) {
            return Err("addvl requires X/SP and an immediate in -32..31".into());
        }
        (
            "addvl_r_ri_".into(),
            vec![
                (FieldId::Rd, rd.number),
                (FieldId::Rn, rn.number),
                (FieldId::imm6, imm as u32 & 63),
            ],
        )
    } else {
        count(&a, 3, 3)?;
        let z = if a[0].starts_with('{') {
            a[0].strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .ok_or("expected single braced Z register")?
                .trim()
        } else {
            a[0]
        };
        let (zt, size) = lane(z, "z", 31)?;
        let load = op.starts_with("ld");
        let signed = matches!(op, "ld1sb" | "ld1sh" | "ld1sw");
        let width = match op.as_bytes()[op.len() - 1] {
            b'b' => 0,
            b'h' => 1,
            b'w' => 2,
            _ => 3,
        };
        if size < width || (signed && size == width) {
            return Err("Z memory element width is too small for the transfer".into());
        }
        let pg = if load {
            a[1].strip_suffix("/z").ok_or("Z load requires P0..P7/z")?
        } else {
            a[1]
        };
        let pg = number(pg, "p", 7)?;
        let addr = a[2]
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or("expected address without writeback")?;
        let addr: Vec<_> = addr.split(',').map(str::trim).collect();
        count(&addr, 1, 3)?;
        let rn = reg(addr[0], true, false)?;
        if rn.bits != 64 {
            return Err("Z memory base must be X or SP".into());
        }
        let mut fields = vec![
            (FieldId::Zt, zt),
            (FieldId::Pg, pg),
            (FieldId::Rn, rn.number),
        ];
        let mode = if addr.len() == 1 || addr[1].starts_with('#') {
            let imm = if addr.len() == 1 {
                0
            } else {
                if addr.len() != 3 || addr[2] != "mul vl" {
                    return Err("Z memory immediate requires mul vl".into());
                }
                integer(addr[1])?
            };
            if !(-8..=7).contains(&imm) {
                return Err("Z memory vector-length offset must be -8..7".into());
            }
            fields.push((FieldId::imm4, imm as u32 & 15));
            "bi"
        } else {
            let rm = reg(addr[1], false, false)?;
            if rm.bits != 64 {
                return Err("Z memory index must be X0..X30".into());
            }
            let amount = if addr.len() == 3 {
                match shift(addr[2])? {
                    ("lsl", n) => n,
                    _ => return Err("Z memory index requires lsl".into()),
                }
            } else {
                0
            };
            if amount != width {
                return Err("Z memory index shift must match transfer width".into());
            }
            fields.push((FieldId::Rm, rm.number));
            "br"
        };
        let name = if load {
            format!(
                "{op}_z_p_{mode}_{}{}",
                if signed { "s" } else { "u" },
                8 << size
            )
        } else {
            if width < 2 {
                fields.push((FieldId::size, size));
            } else if width == 2 {
                fields.push((FieldId::sz, size - 2));
            }
            format!("{op}_z_p_{mode}_")
        };
        (name, fields)
    };
    encode_with_features(&name, &fields, features).map(Some)
}
