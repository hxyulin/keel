//! System syntax and pointer authentication. Encoding permission is not an
//! assertion that a register or cache operation is executable at EL0.
use super::storage::{encode_named_features, lower, upper};
use super::{count, reg, unsigned};
use crate::{FieldId, TargetFeatures, encode_with_features};

pub(super) fn instruction(
    op: &str,
    a: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    let hint = match op {
        "autia1716" => Some("AUTIA1716_HI_hints"),
        "autiasp" => Some("AUTIASP_HI_hints"),
        "autiaz" => Some("AUTIAZ_HI_hints"),
        "autib1716" => Some("AUTIB1716_HI_hints"),
        "autibsp" => Some("AUTIBSP_HI_hints"),
        "autibz" => Some("AUTIBZ_HI_hints"),
        "pacia1716" => Some("PACIA1716_HI_hints"),
        "paciasp" => Some("PACIASP_HI_hints"),
        "paciaz" => Some("PACIAZ_HI_hints"),
        "pacib1716" => Some("PACIB1716_HI_hints"),
        "pacibsp" => Some("PACIBSP_HI_hints"),
        "pacibz" => Some("PACIBZ_HI_hints"),
        "xpaclri" => Some("XPACLRI_HI_hints"),
        _ => None,
    };
    if let Some(name) = hint {
        count(a, 0, 0)?;
        return encode_with_features(name, &[], features).map(Some);
    }
    match op {
        "bti" => {
            count(a, 0, 1)?;
            let option = lower(a.first().copied().unwrap_or(""));
            let value = match option.as_ref() {
                "" => 0,
                "c" => 2,
                "j" => 4,
                "jc" => 6,
                _ => return Err("invalid BTI target".into()),
            };
            return encode_with_features("BTI_HB_hints", &[(FieldId::op2, value)], features)
                .map(Some);
        }
        "retaa" | "retab" => {
            count(a, 0, 0)?;
            return encode_named_features(
                format_args!("{}_64E_branch_reg", upper(op)),
                &[],
                features,
            )
            .map(Some);
        }
        "braa" | "brab" | "blraa" | "blrab" => {
            count(a, 2, 2)?;
            let n = xreg(a[0], false)?;
            let m = xreg(a[1], true)?;
            return encode_named_features(
                format_args!("{}_64P_branch_reg", upper(op)),
                &[(FieldId::Rn, n), (FieldId::Rm, m)],
                features,
            )
            .map(Some);
        }
        "braaz" | "brabz" | "blraaz" | "blrabz" => {
            count(a, 1, 1)?;
            return encode_named_features(
                format_args!("{}_64_branch_reg", upper(op)),
                &[(FieldId::Rn, xreg(a[0], false)?)],
                features,
            )
            .map(Some);
        }
        "pacia" | "pacib" | "pacda" | "pacdb" | "autia" | "autib" | "autda" | "autdb" => {
            count(a, 2, 2)?;
            return encode_named_features(
                format_args!("{}_64P_dp_1src", upper(op)),
                &[
                    (FieldId::Rd, xreg(a[0], false)?),
                    (FieldId::Rn, xreg(a[1], true)?),
                ],
                features,
            )
            .map(Some);
        }
        "paciza" | "pacizb" | "pacdza" | "pacdzb" | "autiza" | "autizb" | "autdza" | "autdzb"
        | "xpaci" | "xpacd" => {
            count(a, 1, 1)?;
            return encode_named_features(
                format_args!("{}_64Z_dp_1src", upper(op)),
                &[(FieldId::Rd, xreg(a[0], false)?)],
                features,
            )
            .map(Some);
        }
        "pacga" => {
            count(a, 3, 3)?;
            return encode_with_features(
                "PACGA_64P_dp_2src",
                &[
                    (FieldId::Rd, xreg(a[0], false)?),
                    (FieldId::Rn, xreg(a[1], false)?),
                    (FieldId::Rm, xreg(a[2], true)?),
                ],
                features,
            )
            .map(Some);
        }
        "mrs" | "msr" | "dc" | "ic" => {
            count(a, if op == "ic" { 1 } else { 2 }, 2)?;
            let (name, rt) = if op == "mrs" {
                (a[1], xreg(a[0], false)?)
            } else {
                (a[0], if a.len() == 2 { xreg(a[1], false)? } else { 31 })
            };
            let name = lower(name);
            let system = crate::generated::system_operand(op, &name)
                .ok_or("unsupported system operand or access direction")?;
            if op == "ic" && ((name == "ivau") != (a.len() == 2)) {
                return Err("invalid IC operand count".into());
            }
            let mut fields = [(FieldId::Rt, rt); 6];
            fields[1..1 + system.len()].copy_from_slice(system);
            let encoding = match op {
                "mrs" => "MRS_RS_systemmove",
                "msr" => "MSR_SR_systemmove",
                _ => "SYS_CR_systeminstrs",
            };
            return encode_with_features(encoding, &fields[..1 + system.len()], features).map(Some);
        }
        "sys" => {
            count(a, 4, 5)?;
            let c = |s: &str| {
                unsigned(
                    s.strip_prefix(['c', 'C'])
                        .ok_or("expected C system field")?,
                    15,
                )
            };
            let fields = [
                (FieldId::op1, unsigned(a[0], 7)?),
                (FieldId::CRn, c(a[1])?),
                (FieldId::CRm, c(a[2])?),
                (FieldId::op2, unsigned(a[3], 7)?),
                (FieldId::Rt, a.get(4).map_or(Ok(31), |s| xreg(s, false))?),
            ];
            return encode_with_features("SYS_CR_systeminstrs", &fields, features).map(Some);
        }
        "ldapr" | "ldaprb" | "ldaprh" => {
            count(a, 2, 2)?;
            let t = reg(a[0], false, true)?;
            if op != "ldapr" && t.bits != 32 {
                return Err("byte/halfword LDAPR requires W registers".into());
            }
            let n = super::memory::atomic_base(a[1])?;
            return encode_named_features(
                format_args!("{}_{}L_memop", upper(op), t.bits),
                &[(FieldId::Rt, t.number), (FieldId::Rn, n.number)],
                features,
            )
            .map(Some);
        }
        "prfm" | "prfum" => return super::memory::prefetch(op, a).map(Some),
        _ => return crypto(op, a, features),
    }
}
fn xreg(s: &str, sp: bool) -> Result<u32, String> {
    let r = reg(s, sp, !sp)?;
    if r.bits != 64 {
        return Err("expected X register".into());
    }
    Ok(r.number)
}

fn crypto(op: &str, a: &[&str], features: TargetFeatures) -> Result<Option<u32>, String> {
    if matches!(
        op,
        "smax" | "smin" | "umax" | "umin" | "cnt" | "ctz" | "abs"
    ) && a
        .first()
        .is_some_and(|s| s.starts_with(['w', 'x', 'W', 'X']))
    {
        let unary = matches!(op, "cnt" | "ctz" | "abs");
        count(a, if unary { 2 } else { 3 }, if unary { 2 } else { 3 })?;
        let d = reg(a[0], false, true)?;
        let n = reg(a[1], false, true)?;
        super::same_width(d, n)?;
        let fields = [(FieldId::Rd, d.number), (FieldId::Rn, n.number)];
        if unary {
            return encode_named_features(
                format_args!("{}_{}_dp_1src", upper(op), d.bits),
                &fields,
                features,
            )
            .map(Some);
        }
        if super::is_integer(a[2]) {
            let imm = super::integer(a[2])?;
            if !(if op.starts_with('s') {
                -128..=127
            } else {
                0..=255
            })
            .contains(&imm)
            {
                return Err("CSSC immediate out of range".into());
            }
            return encode_named_features(
                format_args!(
                    "{}_{}{}_minmax_imm",
                    upper(op),
                    d.bits,
                    if op.starts_with('u') { "U" } else { "" }
                ),
                &[
                    (FieldId::Rd, d.number),
                    (FieldId::Rn, n.number),
                    (FieldId::imm8, (imm & 255) as u32),
                ],
                features,
            )
            .map(Some);
        }
        let m = reg(a[2], false, true)?;
        super::same_width(d, m)?;
        return encode_named_features(
            format_args!("{}_{}_dp_2src", upper(op), d.bits),
            &[
                (FieldId::Rd, d.number),
                (FieldId::Rn, n.number),
                (FieldId::Rm, m.number),
            ],
            features,
        )
        .map(Some);
    }
    if matches!(op, "pmull" | "pmull2") && a.first().is_some_and(|s| lower(s).ends_with(".1q")) {
        count(a, 3, 3)?;
        if !features.supports("FEAT_PMULL") {
            return Err("64-bit PMULL requires FEAT_PMULL".into());
        }
        let mut fields = [
            (FieldId::Rd, 0),
            (FieldId::Rn, 0),
            (FieldId::Rm, 0),
            (FieldId::Q, u32::from(op == "pmull2")),
            (FieldId::size, 3),
        ];
        for (index, text) in a.iter().enumerate() {
            let text = lower(text);
            let (number, shape) = text
                .strip_prefix('v')
                .and_then(|s| s.split_once('.'))
                .ok_or("invalid PMULL vector")?;
            if shape
                != if index == 0 {
                    "1q"
                } else if op == "pmull2" {
                    "2d"
                } else {
                    "1d"
                }
            {
                return Err("invalid PMULL arrangement".into());
            }
            fields[index].1 = number
                .parse::<u32>()
                .ok()
                .filter(|n| *n < 32)
                .ok_or("invalid PMULL register")?;
        }
        return encode_with_features("PMULL_asimddiff_L", &fields, features).map(Some);
    }
    let (form, shapes) = match op {
        "aese" | "aesd" | "aesmc" | "aesimc" => ("B_cryptoaes", &["16b", "16b"][..]),
        "sha1c" | "sha1p" | "sha1m" => ("QSV_cryptosha3", &["q", "s", "4s"][..]),
        "sha1su0" => ("VVV_cryptosha3", &["4s", "4s", "4s"][..]),
        "sha256h" | "sha256h2" => ("QQV_cryptosha3", &["q", "q", "4s"][..]),
        "sha256su1" => ("VVV_cryptosha3", &["4s", "4s", "4s"][..]),
        "sha1h" => ("SS_cryptosha2", &["s", "s"][..]),
        "sha1su1" | "sha256su0" => ("VV_cryptosha2", &["4s", "4s"][..]),
        "crc32b" | "crc32h" | "crc32w" | "crc32cb" | "crc32ch" | "crc32cw" | "crc32x"
        | "crc32cx" => {
            count(a, 3, 3)?;
            let d = reg(a[0], false, true)?;
            let n = reg(a[1], false, true)?;
            let m = reg(a[2], false, true)?;
            let wide = op.ends_with('x');
            if d.bits != 32 || n.bits != 32 || m.bits != if wide { 64 } else { 32 } {
                return Err("invalid CRC register width".into());
            }
            return encode_named_features(
                format_args!("{}_{}C_dp_2src", upper(op), if wide { 64 } else { 32 }),
                &[
                    (FieldId::Rd, d.number),
                    (FieldId::Rn, n.number),
                    (FieldId::Rm, m.number),
                ],
                features,
            )
            .map(Some);
        }
        _ => return Ok(None),
    };
    count(a, shapes.len(), shapes.len())?;
    let mut fields = [(FieldId::Rd, 0), (FieldId::Rn, 0), (FieldId::Rm, 0)];
    for ((field, text), shape) in fields.iter_mut().zip(a).zip(shapes) {
        let text = lower(text);
        let digits = if matches!(*shape, "q" | "s") {
            text.strip_prefix(shape)
        } else {
            text.strip_prefix('v')
                .and_then(|s| s.split_once('.'))
                .filter(|(_, suffix)| suffix == shape)
                .map(|(n, _)| n)
        }
        .ok_or("invalid crypto register arrangement")?;
        field.1 = digits
            .parse::<u32>()
            .ok()
            .filter(|n| *n < 32)
            .ok_or("invalid crypto register")?;
    }
    encode_named_features(
        format_args!("{}_{form}", upper(op)),
        &fields[..a.len()],
        features,
    )
    .map(Some)
}
