//! Scalar LSE atomics and memory barriers. Ordering and transfer width select
//! distinct Arm encoding records; no ordering bits are inferred or discarded.
use super::{count, memory, reg, same_width, unsigned};
use crate::encode;

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    if matches!(op, "dmb" | "dsb" | "isb") {
        count(args, usize::from(op != "isb"), 1)?;
        let option = args.first().copied().unwrap_or("sy").to_ascii_lowercase();
        let crm = match option.as_str() {
            "sy" => 15,
            _ if option.starts_with('#') => unsigned(&option, 15)?,
            _ if op == "isb" => return Err("isb requires sy or a 4-bit immediate".into()),
            "oshld" => 1,
            "oshst" => 2,
            "osh" => 3,
            "nshld" => 5,
            "nshst" => 6,
            "nsh" => 7,
            "ishld" => 9,
            "ishst" => 10,
            "ish" => 11,
            "ld" => 13,
            "st" => 14,
            _ => return Err("invalid barrier option".into()),
        };
        return encode(
            &format!(
                "{}_{}_barriers",
                op.to_ascii_uppercase(),
                if op == "isb" { "BI" } else { "BO" }
            ),
            &[("CRm", crm)],
        )
        .map(Some);
    }
    let (root, suffix, store) = if let Some((root, suffix)) = [
        "ldadd", "ldclr", "ldset", "ldeor", "ldsmin", "ldsmax", "ldumin", "ldumax", "swp", "cas",
    ]
    .iter()
    .find_map(|root| op.strip_prefix(root).map(|s| (*root, s)))
    {
        (root, suffix, false)
    } else if let Some((root, suffix)) = [
        "stadd", "stclr", "stset", "steor", "stsmin", "stsmax", "stumin", "stumax",
    ]
    .iter()
    .find_map(|root| op.strip_prefix(root).map(|s| (*root, s)))
    {
        (root, suffix, true)
    } else {
        return Ok(None);
    };
    let (order, width) = match suffix {
        "" | "a" | "l" | "al" => (suffix, ""),
        "b" | "ab" | "lb" | "alb" => (&suffix[..suffix.len() - 1], "B"),
        "h" | "ah" | "lh" | "alh" => (&suffix[..suffix.len() - 1], "H"),
        _ => return Err(format!("invalid atomic ordering/width suffix in `{op}`")),
    };
    if store && !matches!(order, "" | "l") {
        return Err("store-only atomic aliases support relaxed or release ordering".into());
    }
    count(args, if store { 2 } else { 3 }, if store { 2 } else { 3 })?;
    let rs = reg(args[0], false, true)?;
    let rt = if store {
        reg(if rs.bits == 64 { "xzr" } else { "wzr" }, false, true)?
    } else {
        reg(args[1], false, true)?
    };
    same_width(rs, rt)?;
    if !width.is_empty() && rs.bits != 32 {
        return Err("byte/halfword atomics require W registers".into());
    }
    let rn = memory::atomic_base(args[if store { 1 } else { 2 }])?;
    let name = if store {
        format!("ld{}", &root[2..])
    } else {
        root.to_string()
    };
    let form = if root == "cas" { "comswap" } else { "memop" };
    let tag = format!("{}{}", if root == "cas" { "C" } else { "" }, rs.bits);
    encode(
        &format!(
            "{}{}{width}_{tag}_{form}",
            name.to_ascii_uppercase(),
            order.to_ascii_uppercase()
        ),
        &[("Rs", rs.number), ("Rt", rt.number), ("Rn", rn.number)],
    )
    .map(Some)
}
