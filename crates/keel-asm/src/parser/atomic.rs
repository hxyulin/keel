//! Scalar LSE atomics and memory barriers. Ordering and transfer width select
//! distinct Arm encoding records; no ordering bits are inferred or discarded.
use super::storage::{encode_named, lower, upper};
use super::{count, memory, reg, same_width, unsigned};
use crate::FieldId;

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    if let Some(word) = exclusive(op, args)? {
        return Ok(Some(word));
    }
    if op == "clrex" {
        count(args, 0, 1)?;
        let crm = args.first().map_or(Ok(15), |s| unsigned(s, 15))?;
        return encode("CLREX_BN_barriers", &[("CRm", crm)]).map(Some);
    }
    if matches!(op, "dmb" | "dsb" | "isb") {
        count(args, usize::from(op != "isb"), 1)?;
        let option = lower(args.first().copied().unwrap_or("sy"));
        let crm = match option.as_ref() {
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
        return encode_named(
            format_args!(
                "{}_{}_barriers",
                upper(op),
                if op == "isb" { "BI" } else { "BO" }
            ),
            &[(FieldId::CRm, crm)],
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
    encode_named(
        format_args!("{}{}{width}_{tag}_{form}", upper(name), upper(order)),
        &[
            (FieldId::Rs, rs.number),
            (FieldId::Rt, rt.number),
            (FieldId::Rn, rn.number),
        ],
    )
    .map(Some)
}

/// Exclusive accesses are baseline A64, independent of LSE.
fn exclusive(op: &str, args: &[&str]) -> Result<Option<u32>, String> {
    let (root, suffix) = match [
        "ldaxr", "ldxr", "stlxr", "stxr", "ldaxp", "ldxp", "stlxp", "stxp",
    ]
    .iter()
    .find_map(|root| op.strip_prefix(root).map(|suffix| (*root, suffix)))
    {
        Some(pair) => pair,
        None => return Ok(None),
    };
    let pair = root.ends_with('p');
    if !matches!(suffix, "" | "b" | "h") || (pair && !suffix.is_empty()) {
        return Err("invalid exclusive access width suffix".into());
    }
    let store = root.starts_with("st");
    let arity = 2 + usize::from(pair) + usize::from(store);
    count(args, arity, arity)?;
    let status = if store {
        let r = reg(args[0], false, true)?;
        if r.bits != 32 {
            return Err("exclusive store status requires a W register".into());
        }
        Some(r)
    } else {
        None
    };
    let t = reg(args[usize::from(store)], false, true)?;
    if !suffix.is_empty() && t.bits != 32 {
        return Err("byte/halfword exclusive access requires W registers".into());
    }
    let n = memory::atomic_base(args[arity - 1])?;
    let mut fields = vec![("Rt", t.number), ("Rn", n.number)];
    let t2 = if pair {
        let t2 = reg(args[usize::from(store) + 1], false, true)?;
        same_width(t, t2)?;
        if !store && t.number == t2.number && t.number != 31 {
            return Err("exclusive load-pair destinations must differ".into());
        }
        fields.push(("Rt2", t2.number));
        Some(t2)
    } else {
        None
    };
    if let Some(s) = status {
        if s.number == t.number
            || t2.is_some_and(|r| r.number == s.number)
            || (n.number != 31 && s.number == n.number)
        {
            return Err("exclusive store status overlaps a source register".into());
        }
        fields.push(("Rs", s.number));
    }
    let role = match (store, pair) {
        (false, false) => "LR",
        (true, false) => "SR",
        (false, true) => "LP",
        (true, true) => "SP",
    };
    encode(
        &format!(
            "{}{}_{role}{}_{}",
            root.to_ascii_uppercase(),
            suffix.to_ascii_uppercase(),
            t.bits,
            if pair { "ldstexclp" } else { "ldstexclr" }
        ),
        &fields,
    )
    .map(Some)
}
