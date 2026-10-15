use super::control::condition;
use super::storage::{encode_named, lower, upper};
use super::text::Trim;
use super::{count, reg};
use crate::FieldId;

fn fp(text: &str) -> Result<(u32, &'static str), String> {
    let text = lower(text);
    let tag = match text.as_bytes().first() {
        Some(b's') => "S",
        Some(b'd') => "D",
        _ => {
            return Err(format!(
                "expected S/D floating-point register, got `{text}`"
            ));
        }
    };
    let number = text[1..]
        .parse::<u32>()
        .map_err(|_| format!("invalid FP register `{text}`"))?;
    if number > 31 {
        return Err(format!("invalid FP register `{text}`"));
    }
    Ok((number, tag))
}
fn registers(args: &[&str]) -> Result<(Vec<u32>, &'static str), String> {
    let pairs = args.iter().map(|s| fp(s)).collect::<Result<Vec<_>, _>>()?;
    let tag = pairs[0].1;
    if pairs.iter().any(|r| r.1 != tag) {
        return Err("floating-point register widths must match".into());
    }
    Ok((pairs.iter().map(|r| r.0).collect(), tag))
}
pub(super) fn immediate(text: &str) -> Result<f64, String> {
    text.trim_fast()

        .strip_prefix('#')
        .unwrap_or(text.trim_fast())
        .trim_fast()
        .parse::<f64>()
        .map_err(|_| format!("invalid floating-point immediate `{text}`"))
}

/// VFPExpandImm: sign, an expanded exponent, and four mantissa bits. All
/// representable S/D constants are exact in f64; reject approximation.
pub(super) fn float_immediate(value: f64) -> Result<u32, String> {
    for imm in 0u32..256 {
        let b = (imm >> 6) & 1;
        let exponent = ((b ^ 1) << 10) | (if b == 1 { 0xff << 2 } else { 0 }) | ((imm >> 4) & 3);
        let bits =
            (u64::from(imm >> 7) << 63) | (u64::from(exponent) << 52) | (u64::from(imm & 15) << 48);
        if f64::from_bits(bits).to_bits() == value.to_bits() {
            return Ok(imm);
        }
    }
    Err("immediate is not an encodable scalar floating-point constant".into())
}

pub(super) fn instruction(op: &str, args: &[&str]) -> Result<u32, String> {
    match op {
        "fmov" => {
            count(args, 2, 2)?;
            if args[1].starts_with('#') {
                let (d, tag) = fp(args[0])?;
                return encode_named(
                    format_args!("FMOV_{tag}_floatimm"),
                    &[
                        (FieldId::Rd, d),
                        (FieldId::imm8, float_immediate(immediate(args[1])?)?),
                    ],
                );
            }
            match (fp(args[0]), fp(args[1])) {
                (Ok(d), Ok(n)) => {
                    if d.1 != n.1 {
                        return Err("fmov FP register widths must match".into());
                    }
                    encode_named(
                        format_args!("FMOV_{}_floatdp1", d.1),
                        &[(FieldId::Rd, d.0), (FieldId::Rn, n.0)],
                    )
                }
                (Ok(d), Err(_)) => {
                    let n = reg(args[1], false, true)?;
                    if n.bits != if d.1 == "S" { 32 } else { 64 } {
                        return Err("fmov requires S/W or D/X register pairs".into());
                    }
                    encode_named(
                        format_args!("FMOV_{}{}_float2int", d.1, n.bits),
                        &[(FieldId::Rd, d.0), (FieldId::Rn, n.number)],
                    )
                }
                (Err(_), Ok(n)) => {
                    let d = reg(args[0], false, true)?;
                    if d.bits != if n.1 == "S" { 32 } else { 64 } {
                        return Err("fmov requires S/W or D/X register pairs".into());
                    }
                    encode_named(
                        format_args!("FMOV_{}{}_float2int", d.bits, n.1),
                        &[(FieldId::Rd, d.number), (FieldId::Rn, n.0)],
                    )
                }
                _ => Err("fmov requires a floating-point register".into()),
            }
        }
        "fabs" | "fneg" | "fsqrt" | "frintn" | "frintp" | "frintm" | "frintz" => {
            count(args, 2, 2)?;
            let (r, tag) = registers(args)?;
            encode_named(
                format_args!("{}_{tag}_floatdp1", upper(op)),
                &[(FieldId::Rd, r[0]), (FieldId::Rn, r[1])],
            )
        }
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmin" | "fmax" | "fcsel" => {
            count(
                args,
                if op == "fcsel" { 4 } else { 3 },
                if op == "fcsel" { 4 } else { 3 },
            )?;
            let (r, tag) = registers(&args[..3])?;
            let mut fields = vec![
                (FieldId::Rd, r[0]),
                (FieldId::Rn, r[1]),
                (FieldId::Rm, r[2]),
            ];
            let form = if op == "fcsel" {
                fields.push((FieldId::cond, condition(args[3])?));
                "floatsel"
            } else {
                "floatdp2"
            };
            encode_named(format_args!("{}_{tag}_{form}", upper(op)), &fields)
        }
        "fmadd" | "fmsub" | "fnmadd" | "fnmsub" => {
            count(args, 4, 4)?;
            let (r, tag) = registers(args)?;
            encode_named(
                format_args!("{}_{tag}_floatdp3", upper(op)),
                &[
                    (FieldId::Rd, r[0]),
                    (FieldId::Rn, r[1]),
                    (FieldId::Rm, r[2]),
                    (FieldId::Ra, r[3]),
                ],
            )
        }
        "fcmp" | "fcmpe" => {
            count(args, 2, 2)?;
            let (n, tag) = fp(args[0])?;
            let mut fields = vec![(FieldId::Rn, n)];
            let zero = if args[1].starts_with('#') {
                if immediate(args[1])?.to_bits() != 0 {
                    return Err("floating-point comparisons only accept immediate #0.0".into());
                }
                "Z"
            } else {
                let m = fp(args[1])?;
                if m.1 != tag {
                    return Err("FP comparison register widths must match".into());
                }
                fields.push((FieldId::Rm, m.0));
                ""
            };
            encode_named(format_args!("{}_{tag}{zero}_floatcmp", upper(op)), &fields)
        }
        "fcvt" => {
            count(args, 2, 2)?;
            let d = fp(args[0])?;
            let n = fp(args[1])?;
            if d.1 == n.1 {
                return Err("fcvt requires different FP widths".into());
            }
            encode_named(
                format_args!("FCVT_{}{}_floatdp1", d.1, n.1),
                &[(FieldId::Rd, d.0), (FieldId::Rn, n.0)],
            )
        }
        "scvtf" | "ucvtf" => {
            count(args, 2, 2)?;
            let d = fp(args[0])?;
            let n = reg(args[1], false, true)?;
            encode_named(
                format_args!("{}_{}{}_float2int", upper(op), d.1, n.bits),
                &[(FieldId::Rd, d.0), (FieldId::Rn, n.number)],
            )
        }
        "fcvtzs" | "fcvtzu" => {
            count(args, 2, 2)?;
            let d = reg(args[0], false, true)?;
            let n = fp(args[1])?;
            encode_named(
                format_args!("{}_{}{}_float2int", upper(op), d.bits, n.1),
                &[(FieldId::Rd, d.number), (FieldId::Rn, n.0)],
            )
        }
        _ => Err(format!("unsupported instruction `{op}`")),
    }
}
