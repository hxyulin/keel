//! Optional non-scalable numeric extensions. All encodings retain Arm feature
//! predicates; conditional H-element forms also check FP16 explicitly.
mod half;

use super::super::{reg, unsigned};
use super::{Lane, Shape, count, finish_with_features, number};
use crate::FieldId;
use crate::TargetFeatures;

fn require(features: TargetFeatures, feature: &str) -> Result<(), String> {
    if !features.supports(feature) {
        return Err(format!("instruction requires {feature}"));
    }
    Ok(())
}
fn grouped_lane(text: &str, element: &str) -> Result<Lane, String> {
    let (name, index) = text
        .split_once(&format!(".{element}["))
        .ok_or("invalid grouped lane arrangement")?;
    let index = index
        .strip_suffix(']')
        .ok_or("unclosed grouped lane")?
        .parse::<u32>()
        .map_err(|_| "invalid grouped lane index")?;
    if index > 3 {
        return Err("grouped lane index out of range".into());
    }
    Ok(Lane {
        number: number(name, 'v')?,
        size: 2,
        index,
    })
}
fn lane_fields(m: Lane) -> Result<Vec<(FieldId, u32)>, String> {
    if m.size == 1 && m.number >= 16 {
        return Err("H multiplier requires V0..V15".into());
    }
    let (l, mb, h) = if m.size == 1 {
        ((m.index >> 1) & 1, m.index & 1, m.index >> 2)
    } else {
        (m.index & 1, m.number >> 4, m.index >> 1)
    };
    Ok(vec![
        (FieldId::Rm, m.number & 15),
        (FieldId::L, l),
        (FieldId::M, mb),
        (FieldId::H, h),
    ])
}

fn mentions_half(args: &[&str]) -> bool {
    args.iter().any(|s| {
        (s.starts_with(['h', 'H'])
            && s.len() > 1
            && s.as_bytes()[1..].iter().all(u8::is_ascii_digit))
            || (s.starts_with(['v', 'V'])
                && (s.ends_with(['h', 'H']) || s.contains(".h[") || s.contains(".H[")))
    })
}

pub(super) fn instruction(
    op: &str,
    args: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    if !mentions_half(args)
        && !matches!(
            op,
            "sdot"
                | "udot"
                | "usdot"
                | "sudot"
                | "bfdot"
                | "smmla"
                | "ummla"
                | "usmmla"
                | "bfmmla"
                | "sqrdmlah"
                | "sqrdmlsh"
                | "fmlal"
                | "fmlsl"
                | "fmlal2"
                | "fmlsl2"
                | "bfmlalb"
                | "bfmlalt"
                | "fcmla"
                | "fcadd"
                | "bfcvt"
                | "bfcvtn"
                | "bfcvtn2"
                | "fjcvtzs"
                | "frint32z"
                | "frint32x"
                | "frint64z"
                | "frint64x"
        )
    {
        return Ok(None);
    }
    let owned: Vec<_> = args.iter().map(|s| s.to_ascii_lowercase()).collect();
    let a: Vec<_> = owned.iter().map(String::as_str).collect();
    if let Some(word) = advanced(op, &a, features)? {
        return Ok(Some(word));
    }
    half::instruction(op, &a, features)
}

fn advanced(op: &str, a: &[&str], features: TargetFeatures) -> Result<Option<u32>, String> {
    let dot = matches!(op, "sdot" | "udot" | "usdot" | "sudot" | "bfdot");
    let matrix = matches!(op, "smmla" | "ummla" | "usmmla" | "bfmmla");
    let rdm = matches!(op, "sqrdmlah" | "sqrdmlsh");
    let fhm = matches!(op, "fmlal" | "fmlsl" | "fmlal2" | "fmlsl2");
    let bfml = matches!(op, "bfmlalb" | "bfmlalt");
    let complex = matches!(op, "fcmla" | "fcadd");
    if dot || matrix || rdm || fhm || bfml || complex {
        count(a, if complex { 4 } else { 3 }, if complex { 4 } else { 3 })?;
        let d = Shape::parse(a[0])?;
        let indexed = a[2].contains('[');
        let mut fields = vec![
            (FieldId::Rd, d.number),
            (FieldId::Q, d.q),
            (FieldId::size, d.size),
        ];
        let name = if fhm {
            d.valid_vector()?;
            if d.size != 2 {
                return Err("widening FP16 requires 2S/4S destination".into());
            }
            let srcarr = if d.q == 0 { "2h" } else { "4h" };
            let parse_source = |text: &str| -> Result<u32, String> {
                let (name, arr) = text.split_once('.').ok_or("expected H vector source")?;
                if arr != srcarr {
                    return Err("widening FP16 source must be 2H for 2S or 4H for 4S".into());
                }
                number(name, 'v')
            };
            fields.push((FieldId::Rn, parse_source(a[1])?));
            if indexed {
                let m = Lane::parse(a[2])?;
                if m.size != 1 {
                    return Err("widening FP16 requires H multiplier".into());
                }
                fields.extend(lane_fields(m)?);
            } else {
                fields.push((FieldId::Rm, parse_source(a[2])?));
            }
            format!(
                "{}_{}",
                op.to_ascii_uppercase(),
                if indexed {
                    "asimdelem_LH"
                } else {
                    "asimdsame_F"
                }
            )
        } else {
            let n = Shape::parse(a[1])?;
            fields.push((FieldId::Rn, n.number));
            if rdm {
                d.same(n)?;
                if !matches!(d.size, 1 | 2) {
                    return Err("RDM requires H/S".into());
                }
                if !d.scalar {
                    d.valid_vector()?;
                }
                if indexed {
                    let m = Lane::parse(a[2])?;
                    if m.size != d.size {
                        return Err("RDM multiplier size mismatch".into());
                    }
                    fields.extend(lane_fields(m)?);
                } else {
                    let m = Shape::parse(a[2])?;
                    d.same(m)?;
                    fields.push((FieldId::Rm, m.number));
                }
                format!(
                    "{}_{}_{}",
                    op.to_ascii_uppercase(),
                    if indexed {
                        if d.scalar { "asisdelem" } else { "asimdelem" }
                    } else if d.scalar {
                        "asisdsame2"
                    } else {
                        "asimdsame2"
                    },
                    if indexed { "R" } else { "only" }
                )
            } else if complex {
                d.valid_vector()?;
                d.same(n)?;
                if d.size < 1 {
                    return Err("complex arithmetic requires H/S/D".into());
                }
                if d.size == 1 {
                    require(features, "FEAT_FP16")?;
                }
                let rot = unsigned(a[3], 270)?;
                if rot % 90 != 0 || op == "fcadd" && !matches!(rot, 90 | 270) {
                    return Err("invalid complex rotation".into());
                }
                fields.push((
                    FieldId::rot,
                    if op == "fcadd" {
                        (rot - 90) / 180
                    } else {
                        rot / 90
                    },
                ));
                if indexed {
                    if op != "fcmla" || d.size == 3 || d.size == 2 && d.q == 0 {
                        return Err("indexed FCMLA requires 4H/8H/4S".into());
                    }
                    let m = Lane::parse(a[2])?;
                    if m.size != d.size || m.index >= d.lanes() / 2 {
                        return Err("invalid complex multiplier lane".into());
                    }
                    fields.extend_from_slice(&[
                        (FieldId::Rm, m.number & 15),
                        (FieldId::M, m.number >> 4),
                        (FieldId::L, if d.size == 1 { m.index & 1 } else { 0 }),
                        (FieldId::H, if d.size == 1 { m.index >> 1 } else { m.index }),
                    ]);
                    "FCMLA_advsimd_elt".into()
                } else {
                    let m = Shape::parse(a[2])?;
                    d.same(m)?;
                    fields.push((FieldId::Rm, m.number));
                    format!("{}_asimdsame2_C", op.to_ascii_uppercase())
                }
            } else {
                d.valid_vector()?;
                if n.scalar || d.size != 2 {
                    return Err("dot/matrix products require S vector accumulators".into());
                }
                if bfml {
                    if d.q != 1 || n.size != 1 || n.q != 1 {
                        return Err("BFMLAL requires 4S destination and 8H source".into());
                    }
                    fields.iter_mut().find(|(n, _)| *n == FieldId::Q).unwrap().1 =
                        u32::from(op == "bfmlalt");
                    if indexed {
                        let m = Lane::parse(a[2])?;
                        if m.size != 1 {
                            return Err("BFMLAL multiplier requires H".into());
                        }
                        fields.extend(lane_fields(m)?);
                    } else {
                        let m = Shape::parse(a[2])?;
                        n.same(m)?;
                        fields.push((FieldId::Rm, m.number));
                    }
                    format!(
                        "BFMLAL_{}",
                        if indexed {
                            "asimdelem_F"
                        } else {
                            "asimdsame2_F_"
                        }
                    )
                } else {
                    let bfloat = op.starts_with("bf");
                    let ns = if bfloat { 1 } else { 0 };
                    if n.size != ns || n.q != d.q || matrix && d.q != 1 {
                        return Err("invalid dot/matrix product arrangements".into());
                    }
                    if indexed {
                        if matrix {
                            return Err("matrix product has no indexed form".into());
                        }
                        fields.extend(lane_fields(grouped_lane(
                            a[2],
                            if bfloat { "2h" } else { "4b" },
                        )?)?);
                    } else {
                        if op == "sudot" {
                            return Err("sudot requires an indexed multiplier".into());
                        }
                        let m = Shape::parse(a[2])?;
                        n.same(m)?;
                        fields.push((FieldId::Rm, m.number));
                    }
                    let tag = if bfloat {
                        if indexed || matrix { "E" } else { "D" }
                    } else if matrix {
                        "G"
                    } else {
                        "D"
                    };
                    format!(
                        "{}_{}_{tag}",
                        op.to_ascii_uppercase(),
                        if indexed { "asimdelem" } else { "asimdsame2" }
                    )
                }
            }
        };
        return finish_with_features(&name, &fields, features).map(Some);
    }
    if matches!(op, "bfcvt" | "bfcvtn" | "bfcvtn2") {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        let name = if op == "bfcvt" {
            if !d.scalar || !n.scalar || d.size != 1 || n.size != 2 {
                return Err("bfcvt requires H,S".into());
            }
            "BFCVT_BS_floatdp1"
        } else {
            if d.scalar
                || n.scalar
                || d.size != 1
                || n.size != 2
                || n.q != 1
                || d.q != u32::from(op.ends_with('2'))
            {
                return Err("bfcvtn requires 4H/8H and 4S".into());
            }
            "BFCVTN_asimdmisc_4S"
        };
        return finish_with_features(
            name,
            &[
                (FieldId::Rd, d.number),
                (FieldId::Rn, n.number),
                (FieldId::Q, d.q),
            ],
            features,
        )
        .map(Some);
    }
    if op == "fjcvtzs" {
        count(a, 2, 2)?;
        let d = reg(a[0], false, true)?;
        let n = Shape::parse(a[1])?;
        if d.bits != 32 || !n.scalar || n.size != 3 {
            return Err("fjcvtzs requires W,D".into());
        }
        return finish_with_features(
            "FJCVTZS_32D_float2int",
            &[(FieldId::Rd, d.number), (FieldId::Rn, n.number)],
            features,
        )
        .map(Some);
    }
    if matches!(op, "frint32z" | "frint32x" | "frint64z" | "frint64x") {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        d.same(n)?;
        if d.size < 2 {
            return Err("bounded rounding requires S/D".into());
        }
        if !d.scalar {
            d.valid_vector()?;
        }
        let tag = if d.scalar {
            if d.size == 2 {
                "S_floatdp1"
            } else {
                "D_floatdp1"
            }
        } else {
            "asimdmisc_R"
        };
        return finish_with_features(
            &format!("{}_{tag}", op.to_ascii_uppercase()),
            &[
                (FieldId::Rd, d.number),
                (FieldId::Rn, n.number),
                (FieldId::sz, d.size - 2),
                (FieldId::Q, d.q),
            ],
            features,
        )
        .map(Some);
    }
    Ok(None)
}
