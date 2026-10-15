use super::storage::{encode_named, lower, upper};
use super::{count, reg, unsigned};
use crate::FieldId;
use crate::encode;
use bumpalo::{Bump, collections::Vec as BumpVec};

pub(super) fn condition(text: &str) -> Result<u32, String> {
    match lower(text).as_ref() {
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

pub(super) struct Branch<'a> {
    pub encoding: &'static str,
    pub field: FieldId,
    pub width: u32,
    pub fields: BumpVec<'a, (FieldId, u32)>,
    pub target: &'a str,
}

pub(super) fn branch<'a>(
    op: &str,
    args: &[&'a str],
    arena: &'a Bump,
) -> Result<Option<Branch<'a>>, String> {
    let (encoding, field, width, fields, target) = if op == "b" || op == "bl" {
        count(args, 1, 1)?;
        (
            if op == "bl" {
                "BL_only_branch_imm"
            } else {
                "B_only_branch_imm"
            },
            FieldId::imm26,
            26,
            bumpalo::vec![in arena;],
            args[0],
        )
    } else if let Some(cc) = op.strip_prefix("b.") {
        count(args, 1, 1)?;
        (
            "B_only_condbranch",
            FieldId::imm19,
            19,
            bumpalo::vec![in arena; (FieldId::cond, condition(cc)?)],
            args[0],
        )
    } else if op == "cbz" || op == "cbnz" {
        count(args, 2, 2)?;
        let r = reg(args[0], false, true)?;
        (
            match (op, r.bits) {
                ("cbz", 64) => "CBZ_64_compbranch",
                ("cbz", _) => "CBZ_32_compbranch",
                (_, 64) => "CBNZ_64_compbranch",
                _ => "CBNZ_32_compbranch",
            },
            FieldId::imm19,
            19,
            bumpalo::vec![in arena; (FieldId::Rt, r.number)],
            args[1],
        )
    } else if op == "tbz" || op == "tbnz" {
        count(args, 3, 3)?;
        let r = reg(args[0], false, true)?;
        let bit = unsigned(args[1], i64::from(r.bits - 1))?;
        (
            if op == "tbz" {
                "TBZ_only_testbranch"
            } else {
                "TBNZ_only_testbranch"
            },
            FieldId::imm14,
            14,
            bumpalo::vec![in arena; (FieldId::Rt, r.number), (FieldId::b5, bit >> 5), (FieldId::b40, bit & 31)],
            args[2],
        )
    } else {
        return super::memory::literal(op, args, arena);
    };
    Ok(Some(Branch {
        encoding,
        field,
        width,
        fields,
        target,
    }))
}

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 1, 1)?;
    if op == "brk" || op == "svc" {
        return encode(
            if op == "brk" {
                "BRK_EX_exception"
            } else {
                "SVC_EX_exception"
            },
            &[(FieldId::imm16, unsigned(args[0], 65535)?)],
        );
    }
    let r = reg(args[0], false, true)?;
    if r.bits != 64 {
        return Err("indirect branches require an X register".into());
    }
    encode_named(
        format_args!("{}_64_branch_reg", upper(op)),
        &[(FieldId::Rn, r.number)],
    )
}
