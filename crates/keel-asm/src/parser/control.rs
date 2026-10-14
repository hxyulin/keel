use super::{count, reg, unsigned};
use crate::encode;

pub(super) fn condition(text: &str) -> Result<u32, String> {
    match text.to_ascii_lowercase().as_str() {
        "eq" => Ok(0),
        "ne" => Ok(1),
        "cs" | "hs" => Ok(2),
        "cc" | "lo" => Ok(3),
        "mi" => Ok(4),
        "pl" => Ok(5),
        "vs" => Ok(6),
        "vc" => Ok(7),
        "hi" => Ok(8),
        "ls" => Ok(9),
        "ge" => Ok(10),
        "lt" => Ok(11),
        "gt" => Ok(12),
        "le" => Ok(13),
        "al" => Ok(14),
        "nv" => Ok(15),
        _ => Err(format!("invalid condition `{text}`")),
    }
}

pub(super) struct Branch {
    pub encoding: String,
    pub field: &'static str,
    pub width: u32,
    pub fields: Vec<(&'static str, u32)>,
    pub target: String,
}

pub(super) fn branch(op: &str, args: &[&str]) -> Result<Option<Branch>, String> {
    let (encoding, field, width, fields, target) = if op == "b" || op == "bl" {
        count(args, 1, 1)?;
        (
            format!("{}_only_branch_imm", op.to_ascii_uppercase()),
            "imm26",
            26,
            vec![],
            args[0],
        )
    } else if let Some(cc) = op.strip_prefix("b.") {
        count(args, 1, 1)?;
        (
            "B_only_condbranch".into(),
            "imm19",
            19,
            vec![("cond", condition(cc)?)],
            args[0],
        )
    } else if op == "cbz" || op == "cbnz" {
        count(args, 2, 2)?;
        let r = reg(args[0], false, true)?;
        (
            format!("{}_{}_compbranch", op.to_ascii_uppercase(), r.bits),
            "imm19",
            19,
            vec![("Rt", r.number)],
            args[1],
        )
    } else if op == "tbz" || op == "tbnz" {
        count(args, 3, 3)?;
        let r = reg(args[0], false, true)?;
        let bit = unsigned(args[1], i64::from(r.bits - 1))?;
        (
            format!("{}_only_testbranch", op.to_ascii_uppercase()),
            "imm14",
            14,
            vec![("Rt", r.number), ("b5", bit >> 5), ("b40", bit & 31)],
            args[2],
        )
    } else {
        return Ok(None);
    };
    Ok(Some(Branch {
        encoding,
        field,
        width,
        fields,
        target: target.into(),
    }))
}

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 1, 1)?;
    if op == "brk" {
        return encode("BRK_EX_exception", &[("imm16", unsigned(args[0], 65535)?)]);
    }
    let r = reg(args[0], false, true)?;
    if r.bits != 64 {
        return Err("indirect branches require an X register".into());
    }
    encode(
        &format!("{}_64_branch_reg", op.to_ascii_uppercase()),
        &[("Rn", r.number)],
    )
}
