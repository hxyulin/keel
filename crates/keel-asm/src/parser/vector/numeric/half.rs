//! FP16 scalar/vector syntax, including conditional use of shared encodings.
use super::super::super::{reg, unsigned};
use super::super::{Lane, Shape, count, finish_with_features, number};
use super::{lane_fields, mentions_half, require};
use crate::FieldId;
use crate::TargetFeatures;

pub(super) fn instruction(
    op: &str,
    a: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    use super::super::super::control::condition;
    use super::super::super::float::{float_immediate, immediate};
    let fp_op = matches!(
        op,
        "fmov"
            | "fabs"
            | "fneg"
            | "fsqrt"
            | "frintn"
            | "frintp"
            | "frintm"
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
            | "fnmul"
            | "fmla"
            | "fmls"
            | "fmadd"
            | "fmsub"
            | "fnmadd"
            | "fnmsub"
            | "fcmp"
            | "fcmpe"
            | "fccmp"
            | "fccmpe"
            | "fcsel"
            | "fcvt"
            | "scvtf"
            | "ucvtf"
            | "fcvtns"
            | "fcvtnu"
            | "fcvtas"
            | "fcvtau"
            | "fcvtps"
            | "fcvtpu"
            | "fcvtms"
            | "fcvtmu"
            | "fcvtzs"
            | "fcvtzu"
            | "fmulx"
            | "fcmeq"
            | "fcmgt"
            | "fcmge"
            | "fcmlt"
            | "fcmle"
            | "facgt"
            | "facge"
            | "fabd"
            | "frecps"
            | "frsqrts"
            | "frecpe"
            | "frecpx"
            | "frsqrte"
            | "faddp"
            | "fmaxp"
            | "fminp"
            | "fmaxnmp"
            | "fminnmp"
            | "fmaxv"
            | "fminv"
            | "fmaxnmv"
            | "fminnmv"
    );
    if !fp_op || !mentions_half(a) {
        return Ok(None);
    }
    if op == "fcvt" {
        count(a, 2, 2)?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        if !d.scalar || !n.scalar || d.size == n.size || d.size < 1 || n.size < 1 {
            return Err("invalid scalar precision conversion".into());
        }
        let tag = |size| match size {
            1 => "H",
            2 => "S",
            _ => "D",
        };
        return finish_with_features(
            &format!("FCVT_{}{}_floatdp1", tag(d.size), tag(n.size)),
            &[(FieldId::Rd, d.number), (FieldId::Rn, n.number)],
            features,
        )
        .map(Some);
    }
    // FP16 register/integer transfers and conversions use the integer operand
    // width for fixed-point scaling, not the destination FP precision.
    let gp_dest = a.first().is_some_and(|s| s.starts_with(['w', 'x']));
    let gp_source = a.get(1).is_some_and(|s| s.starts_with(['w', 'x']));
    if gp_dest || gp_source {
        let signed_to_fp = matches!(op, "scvtf" | "ucvtf");
        let to_int = matches!(
            op,
            "fcvtns"
                | "fcvtnu"
                | "fcvtas"
                | "fcvtau"
                | "fcvtps"
                | "fcvtpu"
                | "fcvtms"
                | "fcvtmu"
                | "fcvtzs"
                | "fcvtzu"
        );
        if op != "fmov" && !(signed_to_fp && gp_source || to_int && gp_dest) {
            return Err("invalid FP16/GPR transfer".into());
        }
        count(
            a,
            2,
            if signed_to_fp || matches!(op, "fcvtzs" | "fcvtzu") {
                3
            } else {
                2
            },
        )?;
        let g = reg(a[usize::from(gp_source)], false, true)?;
        let h = Shape::parse(a[usize::from(gp_dest)])?;
        if !h.scalar || h.size != 1 {
            return Err("expected H register".into());
        }
        let tag = if gp_dest {
            format!("{}H", g.bits)
        } else {
            format!("H{}", g.bits)
        };
        let (rd, rn) = if gp_dest {
            (g.number, h.number)
        } else {
            (h.number, g.number)
        };
        let mut fields = vec![(FieldId::Rd, rd), (FieldId::Rn, rn)];
        let form = if a.len() == 3 {
            let fbits = unsigned(a[2], i64::from(g.bits))?;
            if fbits == 0 {
                return Err("fixed-point scale must be 1..GPR width".into());
            }
            fields.push((FieldId::scale, 64 - fbits));
            "float2fix"
        } else {
            "float2int"
        };
        return finish_with_features(
            &format!("{}_{tag}_{form}", op.to_ascii_uppercase()),
            &fields,
            features,
        )
        .map(Some);
    }
    let d = Shape::parse(a[0])?;
    if d.size != 1 {
        return Err("half arithmetic requires H elements".into());
    }
    if !d.scalar {
        d.valid_vector()?;
    }
    let compare = matches!(op, "fcmp" | "fcmpe");
    let conditional_compare = matches!(op, "fccmp" | "fccmpe");
    let full_reduce = matches!(op, "fmaxv" | "fminv" | "fmaxnmv" | "fminnmv");
    let pair = matches!(op, "faddp" | "fmaxp" | "fminp" | "fmaxnmp" | "fminnmp");
    if full_reduce || pair && d.scalar {
        count(a, 2, 2)?;
        if !d.scalar {
            return Err("reduction requires H destination".into());
        }
        let (rn, q) = if pair {
            let (name, arr) = a[1].split_once('.').ok_or("expected 2H vector")?;
            if arr != "2h" {
                return Err("scalar half pair reduction requires 2H".into());
            }
            (number(name, 'v')?, 0)
        } else {
            let n = Shape::parse(a[1])?;
            if n.scalar || n.size != 1 {
                return Err("half reduction requires 4H/8H".into());
            }
            (n.number, n.q)
        };
        return finish_with_features(
            &format!(
                "{}_{}",
                op.to_ascii_uppercase(),
                if pair {
                    "asisdpair_only_H"
                } else {
                    "asimdall_only_H"
                }
            ),
            &[(FieldId::Rd, d.number), (FieldId::Rn, rn), (FieldId::Q, q)],
            features,
        )
        .map(Some);
    }
    if op == "fmov" && a.get(1).is_some_and(|s| s.starts_with('#')) {
        count(a, 2, 2)?;
        let imm = float_immediate(immediate(a[1])?)?;
        let mut fields = if d.scalar {
            vec![(FieldId::imm8, imm)]
        } else {
            super::super::immediate_fields(imm)
        };
        fields.extend_from_slice(&[(FieldId::Rd, d.number), (FieldId::Q, d.q)]);
        return finish_with_features(
            if d.scalar {
                "FMOV_H_floatimm"
            } else {
                "FMOV_asimdimm_H_h"
            },
            &fields,
            features,
        )
        .map(Some);
    }
    let fixed = matches!(op, "scvtf" | "ucvtf" | "fcvtzs" | "fcvtzu") && a.len() == 3;
    if fixed {
        count(a, 3, 3)?;
        let n = Shape::parse(a[1])?;
        d.same(n)?;
        let fbits = unsigned(a[2], 16)?;
        if fbits == 0 {
            return Err("half fixed-point scale must be 1..16".into());
        }
        require(features, "FEAT_FP16")?;
        let imm = 32 - fbits;
        return finish_with_features(
            &format!(
                "{}_{}_C",
                op.to_ascii_uppercase(),
                if d.scalar { "asisdshf" } else { "asimdshf" }
            ),
            &[
                (FieldId::Rd, d.number),
                (FieldId::Rn, n.number),
                (FieldId::Q, d.q),
                (FieldId::immh, imm >> 3),
                (FieldId::immb, imm & 7),
            ],
            features,
        )
        .map(Some);
    }
    if a.get(2).is_some_and(|s| s.contains('[')) {
        if !matches!(op, "fmla" | "fmls" | "fmul" | "fmulx") {
            return Err("no indexed form for this half operation".into());
        }
        count(a, 3, 3)?;
        let n = Shape::parse(a[1])?;
        let m = Lane::parse(a[2])?;
        d.same(n)?;
        if m.size != 1 {
            return Err("half multiplier requires H lane".into());
        }
        let mut fields = lane_fields(m)?;
        fields.extend_from_slice(&[
            (FieldId::Rd, d.number),
            (FieldId::Rn, n.number),
            (FieldId::Q, d.q),
        ]);
        return finish_with_features(
            &format!(
                "{}_{}_RH_H",
                op.to_ascii_uppercase(),
                if d.scalar { "asisdelem" } else { "asimdelem" }
            ),
            &fields,
            features,
        )
        .map(Some);
    }
    let scalar_one = matches!(
        op,
        "fmov"
            | "fabs"
            | "fneg"
            | "fsqrt"
            | "frintn"
            | "frintp"
            | "frintm"
            | "frintz"
            | "frinta"
            | "frintx"
            | "frinti"
    );
    let misc = scalar_one
        || matches!(
            op,
            "fcvtns"
                | "fcvtnu"
                | "fcvtas"
                | "fcvtau"
                | "fcvtps"
                | "fcvtpu"
                | "fcvtms"
                | "fcvtmu"
                | "fcvtzs"
                | "fcvtzu"
                | "scvtf"
                | "ucvtf"
                | "frecpe"
                | "frecpx"
                | "frsqrte"
        );
    let scalar_two = matches!(
        op,
        "fmul" | "fdiv" | "fadd" | "fsub" | "fmax" | "fmin" | "fmaxnm" | "fminnm" | "fnmul"
    );
    let scalar_three = matches!(op, "fmadd" | "fmsub" | "fnmadd" | "fnmsub");
    let zero = a.get(2).is_some_and(|s| s.starts_with('#'));
    let zero_fp = matches!(op, "fcmeq" | "fcmgt" | "fcmge" | "fcmlt" | "fcmle") && zero;
    if d.scalar && matches!(op, "fcmlt" | "fcmle") && !zero_fp {
        return Err(
            "scalar H fcmlt/fcmle accepts only #0.0; use reversed fcmgt/fcmge for registers".into(),
        );
    }
    let arity = if conditional_compare || scalar_three || op == "fcsel" {
        4
    } else if compare || misc {
        2
    } else {
        3
    };
    count(a, arity, arity)?;
    let mut fields = vec![
        (FieldId::Rd, d.number),
        (FieldId::Rn, d.number),
        (FieldId::Q, d.q),
    ];
    let mut actual = op;
    let name = if compare {
        if !d.scalar {
            return Err("fcmp requires scalar registers".into());
        }
        let zero = a[1].starts_with('#');
        if zero {
            if immediate(a[1])?.to_bits() != 0 {
                return Err("comparison requires #0.0".into());
            }
        } else {
            let m = Shape::parse(a[1])?;
            d.same(m)?;
            fields.push((FieldId::Rm, m.number));
        }
        format!(
            "{}_H{}_floatcmp",
            op.to_ascii_uppercase(),
            if zero { "Z" } else { "" }
        )
    } else {
        let other = Shape::parse(a[1])?;
        d.same(other)?;
        let mut n = if conditional_compare { d } else { other };
        let mut m = if conditional_compare {
            Some(other)
        } else if !misc && !zero_fp {
            let m = Shape::parse(a[2])?;
            d.same(m)?;
            Some(m)
        } else {
            None
        };
        if zero_fp {
            if immediate(a[2])?.to_bits() != 0 {
                return Err("comparison requires #0.0".into());
            }
        } else if matches!(op, "fcmlt" | "fcmle") {
            std::mem::swap(&mut n, m.as_mut().unwrap());
            actual = if op == "fcmlt" { "fcmgt" } else { "fcmge" };
        }
        fields
            .iter_mut()
            .find(|(n, _)| *n == FieldId::Rn)
            .unwrap()
            .1 = n.number;
        if let Some(m) = m {
            fields.push((FieldId::Rm, m.number));
        }
        if conditional_compare {
            if !d.scalar {
                return Err("conditional comparison requires scalar H".into());
            }
            fields.extend_from_slice(&[
                (FieldId::nzcv, unsigned(a[2], 15)?),
                (FieldId::cond, condition(a[3])?),
            ]);
            format!("{}_H_floatccmp", actual.to_ascii_uppercase())
        } else if scalar_three {
            if !d.scalar {
                return Err("multiply-add requires scalar H".into());
            }
            let ra = Shape::parse(a[3])?;
            d.same(ra)?;
            fields.push((FieldId::Ra, ra.number));
            format!("{}_H_floatdp3", actual.to_ascii_uppercase())
        } else if op == "fcsel" {
            if !d.scalar {
                return Err("fcsel requires scalar H".into());
            }
            fields.push((FieldId::cond, condition(a[3])?));
            "FCSEL_H_floatsel".into()
        } else if d.scalar && (scalar_one || scalar_two) {
            format!(
                "{}_H_{}",
                actual.to_ascii_uppercase(),
                if scalar_one { "floatdp1" } else { "floatdp2" }
            )
        } else {
            if !d.scalar && matches!(op, "frecpx" | "fmov" | "fnmul") {
                return Err("no vector form for this operation".into());
            }
            let class = if misc || zero_fp {
                if d.scalar {
                    "asisdmiscfp16"
                } else {
                    "asimdmiscfp16"
                }
            } else if d.scalar {
                "asisdsamefp16"
            } else {
                "asimdsamefp16"
            };
            format!(
                "{}_{class}_{}",
                actual.to_ascii_uppercase(),
                if zero_fp {
                    "FZ"
                } else if misc {
                    "R"
                } else {
                    "only"
                }
            )
        }
    };
    finish_with_features(&name, &fields, features).map(Some)
}
