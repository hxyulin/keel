//! The two Advanced SIMD forms used by Keel's scalar popcount lowering.
use super::count;
use super::storage::{encode_named, lower};
use crate::FieldId;
use crate::encode;

fn number(text: &str, prefix: char) -> Result<u32, String> {
    let digits = text
        .strip_prefix(prefix)
        .ok_or("invalid SIMD register class")?;
    let number = digits
        .parse::<u32>()
        .map_err(|_| format!("invalid SIMD register `{text}`"))?;
    if number > 31 {
        return Err(format!("invalid SIMD register `{text}`"));
    }
    Ok(number)
}
fn vector(text: &str) -> Result<(u32, &str, u32, u32), String> {
    let (name, arrangement) = text
        .split_once('.')
        .ok_or("vector register requires an arrangement")?;
    let register = number(name, 'v')?;
    let (q, size) = match arrangement {
        "8b" => (0, 0),
        "16b" => (1, 0),
        "4h" => (0, 1),
        "8h" => (1, 1),
        "4s" => (1, 2),
        _ => return Err("unsupported SIMD reduction arrangement".into()),
    };
    Ok((register, arrangement, q, size))
}
pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    count(args, 2, 2)?;
    let d = lower(args[0]);
    let n = lower(args[1]);
    let (rn, arrangement, q, size) = vector(&n)?;
    let rd = if op == "cnt" {
        let (rd, dest_arrangement, _, _) = vector(&d)?;
        if size != 0 || arrangement != dest_arrangement {
            return Err("cnt requires matching 8B or 16B arrangements".into());
        }
        rd
    } else {
        number(
            &d,
            match size {
                0 => 'b',
                1 => 'h',
                _ => 's',
            },
        )?
    };
    encode(
        if op == "cnt" {
            "CNT_asimdmisc_R"
        } else {
            "ADDV_asimdall_only"
        },
        &[
            (FieldId::Rd, rd),
            (FieldId::Rn, rn),
            (FieldId::Q, q),
            (FieldId::size, size),
        ],
    )
}

/// Advanced SIMD modified immediates: byte replication, shifted halfwords /
/// words, low-one MSL words, and one immediate bit per 00/FF byte in 64-bit lanes.
pub(super) fn move_immediate(args: &[&str]) -> Result<u32, String> {
    count(args, 2, 3)?;
    let dest = lower(args[0]);
    let (rd, arrangement, q) = if dest.starts_with('d') {
        (number(&dest, 'd')?, "d", 0)
    } else {
        let (name, arrangement) = dest
            .split_once('.')
            .ok_or("movi requires a lane arrangement")?;
        let q = match arrangement {
            "8b" | "4h" | "2s" => 0,
            "16b" | "8h" | "4s" | "2d" => 1,
            _ => return Err("invalid movi lane arrangement".into()),
        };
        (number(name, 'v')?, arrangement, q)
    };
    let wide = matches!(arrangement, "d" | "2d");
    let imm = if wide {
        let expr = super::data::expression(args[1].strip_prefix('#').unwrap_or(args[1]))?;
        if expr.add.is_some() || !(-(1i128 << 63)..(1i128 << 64)).contains(&expr.constant) {
            return Err("invalid movi 64-bit byte mask".into());
        }
        let value = expr.constant as u64;
        let mut imm = 0;
        for i in 0..8 {
            match (value >> (8 * i)) & 255 {
                0 => {}
                255 => imm |= 1 << i,
                _ => return Err("movi D immediates require each byte to be 00 or FF".into()),
            }
        }
        imm
    } else {
        super::unsigned(args[1], 255)?
    };
    let shift = if args.len() == 3 {
        super::shift(args[2])?
    } else {
        ("lsl", 0)
    };
    let (form, cmode) = match (arrangement, shift) {
        ("8b" | "16b", ("lsl", 0)) if args.len() == 2 => ("N_b", None),
        ("4h" | "8h", ("lsl", n)) if matches!(n, 0 | 8) => ("L_hl", Some(8 + n / 4)),
        ("2s" | "4s", ("lsl", n)) if matches!(n, 0 | 8 | 16 | 24) => ("L_sl", Some(n / 4)),
        ("2s" | "4s", ("msl", n)) if matches!(n, 8 | 16) => ("M_sm", Some(11 + n / 8)),
        ("d", ("lsl", 0)) if args.len() == 2 => ("D_ds", None),
        ("2d", ("lsl", 0)) if args.len() == 2 => ("D2_d", None),
        _ => return Err("invalid movi shift for lane arrangement".into()),
    };
    let mut fields = vec![(FieldId::Rd, rd)];
    if !wide {
        fields.push((FieldId::Q, q));
    }
    if let Some(cmode) = cmode {
        fields.push((FieldId::cmode, cmode));
    }
    for (i, name) in [
        FieldId::a,
        FieldId::b,
        FieldId::c,
        FieldId::d,
        FieldId::e,
        FieldId::f,
        FieldId::g,
        FieldId::h,
    ]
    .into_iter()
    .enumerate()
    {
        fields.push((name, (imm >> (7 - i)) & 1));
    }
    encode_named(format_args!("MOVI_asimdimm_{form}"), &fields)
}
