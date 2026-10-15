use super::{Shape, count, finish};
use crate::FieldId;

pub(super) fn instruction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    if !a
        .first()
        .is_some_and(|s| *s != "sp" && s.starts_with(['v', 's', 'd']))
    {
        return Ok(None);
    }
    let high = op.ends_with('2');
    let root = if high { &op[..op.len() - 1] } else { op };
    if matches!(root, "fcvtn" | "fcvtl" | "fcvtxn") {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        if d.scalar || n.scalar {
            if root != "fcvtxn" || high || !d.scalar || !n.scalar || d.size != 2 || n.size != 3 {
                return Err("invalid scalar precision conversion".into());
            }
        } else if root == "fcvtl" {
            if d.q != 1
                || n.q != u32::from(high)
                || d.size != n.size + 1
                || !matches!(n.size, 1 | 2)
            {
                return Err("invalid widening FP arrangements".into());
            }
        } else if n.q != 1
            || d.q != u32::from(high)
            || n.size != d.size + 1
            || !matches!(d.size, 1 | 2)
            || root == "fcvtxn" && d.size != 2
        {
            return Err("invalid narrowing FP arrangements".into());
        }
        let class = if d.scalar {
            "asisdmisc_N"
        } else if root == "fcvtl" {
            "asimdmisc_L"
        } else {
            "asimdmisc_N"
        };
        return finish(
            &format!("{}_{class}", root.to_ascii_uppercase()),
            &[
                (FieldId::Rd, d.number),
                (FieldId::Rn, n.number),
                (FieldId::Q, u32::from(high)),
                (
                    FieldId::sz,
                    if root == "fcvtl" {
                        n.size - 1
                    } else {
                        d.size - 1
                    },
                ),
            ],
        )
        .map(Some);
    }
    if high {
        return Ok(None);
    }
    if op == "fmov" && a[0].starts_with('v') {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        d.valid_vector()?;
        if d.size < 2 {
            return Err("vector FP immediate requires S/D".into());
        }
        let imm = super::super::float::float_immediate(super::super::float::immediate(a[1])?)?;
        let mut fields = super::immediate_fields(imm);
        fields.extend_from_slice(&[(FieldId::Rd, d.number), (FieldId::Q, d.q)]);
        return finish(
            if d.size == 2 {
                "FMOV_asimdimm_S_s"
            } else {
                "FMOV_asimdimm_D2_d"
            },
            &fields,
        )
        .map(Some);
    }
    let unary = matches!(
        op,
        "frintn"
            | "frintm"
            | "frintp"
            | "frintz"
            | "frinta"
            | "frintx"
            | "frinti"
            | "fcvtns"
            | "fcvtms"
            | "fcvtas"
            | "fcvtps"
            | "fcvtzs"
            | "fcvtnu"
            | "fcvtmu"
            | "fcvtau"
            | "fcvtpu"
            | "fcvtzu"
            | "scvtf"
            | "ucvtf"
            | "frecpe"
            | "frsqrte"
            | "frecpx"
            | "urecpe"
            | "ursqrte"
            | "fabs"
            | "fneg"
            | "fsqrt"
    );
    let ternary = matches!(
        op,
        "fadd"
            | "fsub"
            | "fmul"
            | "fdiv"
            | "fmla"
            | "fmls"
            | "fmin"
            | "fmax"
            | "fminnm"
            | "fmaxnm"
            | "fmulx"
            | "fcmeq"
            | "fcmgt"
            | "fcmge"
            | "fcmlt"
            | "fcmle"
            | "facgt"
            | "facge"
            | "frecps"
            | "frsqrts"
            | "fabd"
            | "faddp"
            | "fmaxp"
            | "fminp"
            | "fmaxnmp"
            | "fminnmp"
    );
    if !unary && !ternary || a.get(2).is_some_and(|s| s.contains('[')) {
        return Ok(None);
    }
    let d = Shape::parse(a[0])?;
    if d.scalar {
        if matches!(
            op,
            "fabs"
                | "fneg"
                | "fsqrt"
                | "frintn"
                | "frintm"
                | "frintp"
                | "frintz"
                | "frinta"
                | "frintx"
                | "frinti"
                | "fadd"
                | "fsub"
                | "fmul"
                | "fdiv"
                | "fmin"
                | "fmax"
                | "fminnm"
                | "fmaxnm"
                | "fmla"
                | "fmls"
                | "faddp"
                | "fmaxp"
                | "fminp"
                | "fmaxnmp"
                | "fminnmp"
        ) {
            return Ok(None);
        }
        if matches!(op, "scvtf" | "ucvtf") && a.get(1).is_some_and(|s| s.starts_with(['w', 'x'])) {
            return Ok(None);
        }
        if matches!(op, "urecpe" | "ursqrte") {
            return Err("integer reciprocal estimate requires vectors".into());
        }
    } else {
        d.valid_vector()?;
    }
    if d.size < 2
        || matches!(op, "urecpe" | "ursqrte") && d.size != 2
        || op == "frecpx" && !d.scalar
    {
        return Err("invalid FP element size or register class".into());
    }
    count(a, if unary { 2 } else { 3 }, if unary { 2 } else { 3 })?;
    let mut n = Shape::parse(a[1])?;
    d.same(n)?;
    let zero = !unary && a[2].starts_with('#');
    if zero
        && (!matches!(op, "fcmeq" | "fcmgt" | "fcmge" | "fcmlt" | "fcmle")
            || super::super::float::immediate(a[2])?.to_bits() != 0)
    {
        return Err("FP SIMD comparison immediate must be #0.0".into());
    }
    let mut m = if !unary && !zero {
        let m = Shape::parse(a[2])?;
        d.same(m)?;
        Some(m)
    } else {
        None
    };
    let actual = if !zero && matches!(op, "fcmlt" | "fcmle") {
        std::mem::swap(&mut n, m.as_mut().unwrap());
        if op == "fcmlt" { "fcmgt" } else { "fcmge" }
    } else {
        op
    };
    let class = match (d.scalar, unary || zero) {
        (true, true) => "asisdmisc",
        (true, false) => "asisdsame",
        (false, true) => "asimdmisc",
        (false, false) => "asimdsame",
    };
    let tag = if zero {
        "FZ"
    } else if unary {
        "R"
    } else {
        "only"
    };
    let mut fields = vec![
        (FieldId::Rd, d.number),
        (FieldId::Rn, n.number),
        (FieldId::Q, d.q),
        (FieldId::sz, d.size - 2),
    ];
    if let Some(m) = m {
        fields.push((FieldId::Rm, m.number));
    }
    finish(
        &format!("{}_{class}_{tag}", actual.to_ascii_uppercase()),
        &fields,
    )
    .map(Some)
}

pub(super) fn reduction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    let full = matches!(
        op,
        "addv"
            | "saddlv"
            | "uaddlv"
            | "smaxv"
            | "sminv"
            | "umaxv"
            | "uminv"
            | "fmaxv"
            | "fminv"
            | "fmaxnmv"
            | "fminnmv"
    );
    let pair = matches!(
        op,
        "addp" | "faddp" | "fmaxp" | "fminp" | "fmaxnmp" | "fminnmp"
    ) && a
        .first()
        .is_some_and(|s| s.starts_with(['b', 'h', 's', 'd']));
    if !full && !pair {
        return Ok(None);
    }
    count(a, 2, 2)?;
    let d = Shape::parse(a[0])?;
    let n = Shape::parse(a[1])?;
    if !d.scalar || n.scalar {
        return Err("SIMD reduction requires scalar destination and vector source".into());
    }
    let fp = op.starts_with('f');
    let long = matches!(op, "saddlv" | "uaddlv");
    if fp {
        if pair {
            if !matches!(n.size, 2 | 3) || n.lanes() != 2 || d.size != n.size {
                return Err("FP pair reduction requires two S/D lanes".into());
            }
        } else if n.size != 2 || n.q != 1 || d.size != 2 {
            return Err("FP reduction requires S destination and 4S source".into());
        }
    } else if pair {
        if d.size != 3 || n.size != 3 || n.q != 1 {
            return Err("scalar addp requires D and 2D".into());
        }
    } else if n.size > 2 || n.size == 2 && n.q == 0 || d.size != n.size + u32::from(long) {
        return Err("invalid integer reduction arrangements".into());
    }
    let class = if pair {
        if fp {
            "asisdpair_only_SD"
        } else {
            "asisdpair_only"
        }
    } else if fp {
        "asimdall_only_SD"
    } else {
        "asimdall_only"
    };
    finish(
        &format!("{}_{class}", op.to_ascii_uppercase()),
        &[
            (FieldId::Rd, d.number),
            (FieldId::Rn, n.number),
            (FieldId::Q, n.q),
            (FieldId::size, n.size),
            (FieldId::sz, n.size.saturating_sub(2)),
        ],
    )
    .map(Some)
}
