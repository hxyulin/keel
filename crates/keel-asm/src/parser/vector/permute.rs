use super::super::{reg, unsigned};
use super::{Lane, Shape, count, encode, register_list};

pub(super) fn instruction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    let is_lane = a.iter().any(|s| s.starts_with('v') && s.contains('['));
    if op == "mov" && !is_lane && a.first().is_some_and(|s| s.starts_with('v')) {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        d.same(n)?;
        return encode(
            "ORR_asimdsame_only",
            &[
                ("Rd", d.number),
                ("Rn", n.number),
                ("Rm", n.number),
                ("Q", d.q),
            ],
        )
        .map(Some);
    }
    match op {
        "zip1" | "zip2" | "uzp1" | "uzp2" | "trn1" | "trn2" | "ext" => {
            let arity = if op == "ext" { 4 } else { 3 };
            count(a, arity, arity)?;
            let d = Shape::parse(a[0])?;
            d.valid_vector()?;
            let n = Shape::parse(a[1])?;
            let m = Shape::parse(a[2])?;
            d.same(n)?;
            d.same(m)?;
            let mut fields = vec![
                ("Rd", d.number),
                ("Rn", n.number),
                ("Rm", m.number),
                ("Q", d.q),
            ];
            let class = if op == "ext" {
                if d.size != 0 {
                    return Err("ext requires byte arrangements".into());
                }
                fields.push(("imm4", unsigned(a[3], i64::from(d.lanes() - 1))?));
                "asimdext"
            } else {
                fields.push(("size", d.size));
                "asimdperm"
            };
            encode(
                &format!("{}_{class}_only", op.to_ascii_uppercase()),
                &fields,
            )
            .map(Some)
        }
        "tbl" | "tbx" => {
            count(a, 3, 3)?;
            let d = Shape::parse(a[0])?;
            let m = Shape::parse(a[2])?;
            d.valid_vector()?;
            d.same(m)?;
            if d.size != 0 {
                return Err("table lookup requires byte arrangements".into());
            }
            let table = register_list(a[1])?;
            if table[0].q != 1 || table[0].size != 0 {
                return Err("table registers require 16B".into());
            }
            let len = table.len();
            encode(
                &format!("{}_asimdtbl_L{len}_{len}", op.to_ascii_uppercase()),
                &[
                    ("Rd", d.number),
                    ("Rn", table[0].number),
                    ("Rm", m.number),
                    ("Q", d.q),
                ],
            )
            .map(Some)
        }
        "dup" | "mov"
            if (op == "dup" && !a.first().is_some_and(|s| s.starts_with('z')))
                || (op == "mov"
                    && is_lane
                    && a.first()
                        .is_some_and(|s| s.starts_with(['b', 'h', 's', 'd']))) =>
        {
            count(a, 2, 2)?;
            let d = Shape::parse(a[0])?;
            let (name, rn, imm5) = if a[1].contains('[') {
                let n = Lane::parse(a[1])?;
                if d.size != n.size {
                    return Err("dup element sizes must match".into());
                }
                if !d.scalar {
                    d.valid_vector()?;
                }
                (
                    if d.scalar {
                        "DUP_asisdone_only"
                    } else {
                        "DUP_asimdins_DV_v"
                    },
                    n.number,
                    n.imm5(),
                )
            } else {
                d.valid_vector()?;
                let n = reg(a[1], false, true)?;
                if n.bits != if d.size == 3 { 64 } else { 32 } {
                    return Err("dup requires W for B/H/S or X for D".into());
                }
                ("DUP_asimdins_DR_r", n.number, 1 << d.size)
            };
            let mut fields = vec![("Rd", d.number), ("Rn", rn), ("imm5", imm5)];
            if !d.scalar {
                fields.push(("Q", d.q));
            }
            encode(name, &fields).map(Some)
        }
        "ins" | "mov" if is_lane && a.first().is_some_and(|s| s.starts_with('v')) => {
            count(a, 2, 2)?;
            let d = Lane::parse(a[0])?;
            let mut fields = vec![("Rd", d.number), ("imm5", d.imm5())];
            let name = if a[1].contains('[') {
                let n = Lane::parse(a[1])?;
                if d.size != n.size {
                    return Err("inserted lane sizes must match".into());
                }
                fields.extend_from_slice(&[("Rn", n.number), ("imm4", n.index << d.size)]);
                "INS_asimdins_IV_v"
            } else {
                let n = reg(a[1], false, true)?;
                if n.bits != if d.size == 3 { 64 } else { 32 } {
                    return Err("ins requires W for B/H/S or X for D".into());
                }
                fields.push(("Rn", n.number));
                "INS_asimdins_IR_r"
            };
            encode(name, &fields).map(Some)
        }
        "smov" | "umov" | "mov" if is_lane => {
            count(a, 2, 2)?;
            let d = reg(a[0], false, true)?;
            let n = Lane::parse(a[1])?;
            if op == "mov" && n.size < 2 {
                return Err("mov extraction requires S or D; use umov for B/H".into());
            }
            let signed = op == "smov";
            let valid = if signed {
                n.size < if d.bits == 64 { 3 } else { 2 }
            } else {
                if d.bits == 64 {
                    n.size == 3
                } else {
                    n.size <= 2
                }
            };
            if !valid {
                return Err("invalid lane extraction width".into());
            }
            let root = if signed { "SMOV" } else { "UMOV" };
            let tag = if d.bits == 64 { "X_x" } else { "W_w" };
            encode(
                &format!("{root}_asimdins_{tag}"),
                &[("Rd", d.number), ("Rn", n.number), ("imm5", n.imm5())],
            )
            .map(Some)
        }
        _ => Ok(None),
    }
}
