use super::super::{integer, unsigned};
use super::{Shape, count, finish};

pub(super) fn instruction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    if !a
        .first()
        .is_some_and(|s| s.starts_with(['v', 'b', 'h', 's', 'd']))
    {
        return Ok(None);
    }
    if a.get(2).is_some_and(|s| s.contains('[')) {
        return Ok(None);
    }
    if op == "addp" && !a[0].starts_with('v') {
        return Ok(None);
    }
    let high = op != "rev32" && op.ends_with('2');
    let root = if high { &op[..op.len() - 1] } else { op };
    // Different-width operations use the narrow element size in the encoding.
    let wide = matches!(
        root,
        "saddl"
            | "uaddl"
            | "ssubl"
            | "usubl"
            | "sabal"
            | "uabal"
            | "sabdl"
            | "uabdl"
            | "smlal"
            | "umlal"
            | "smlsl"
            | "umlsl"
            | "smull"
            | "umull"
            | "sqdmlal"
            | "sqdmlsl"
            | "sqdmull"
            | "pmull"
    );
    let addwide = matches!(root, "saddw" | "uaddw" | "ssubw" | "usubw");
    let narrow = matches!(
        root,
        "addhn" | "raddhn" | "subhn" | "rsubhn" | "xtn" | "sqxtn" | "uqxtn" | "sqxtun"
    );
    let pairwide = matches!(root, "saddlp" | "uaddlp" | "sadalp" | "uadalp");
    if wide || addwide || narrow || pairwide {
        let unary = pairwide || matches!(root, "xtn" | "sqxtn" | "uqxtn" | "sqxtun");
        count(a, if unary { 2 } else { 3 }, if unary { 2 } else { 3 })?;
        let d = Shape::parse(a[0])?;
        let n = Shape::parse(a[1])?;
        if d.scalar != n.scalar {
            return Err("SIMD operand classes must match".into());
        }
        if d.scalar
            && (high
                || !matches!(
                    root,
                    "sqxtn" | "uqxtn" | "sqxtun" | "sqdmlal" | "sqdmlsl" | "sqdmull"
                ))
        {
            return Err("no scalar form for this operation".into());
        }
        let size = if narrow {
            d.size
        } else if addwide {
            n.size.checked_sub(1).ok_or("wide source is too narrow")?
        } else {
            n.size
        };
        if size > 2
            || (d.scalar && wide && size == 0)
            || (root.starts_with("sqd") && size == 0)
            || (root == "pmull" && size != 0)
        {
            return Err("invalid SIMD element size".into());
        }
        if pairwide {
            if high || d.scalar || d.size != n.size + 1 || d.q != n.q {
                return Err("pairwise widening requires matching vector widths and double-width destination elements".into());
            }
        } else if narrow {
            if n.size != d.size + 1 || (!d.scalar && (n.q != 1 || d.q != u32::from(high))) {
                return Err("invalid narrowing arrangements".into());
            }
        } else if d.size != size + 1
            || (!d.scalar && (d.q != 1 || n.q != if addwide { 1 } else { u32::from(high) }))
        {
            return Err("invalid widening arrangements".into());
        }
        let mut fields = vec![
            ("Rd", d.number),
            ("Rn", n.number),
            ("size", size),
            ("Q", if pairwide { d.q } else { u32::from(high) }),
        ];
        if !unary {
            let m = Shape::parse(a[2])?;
            if m.scalar != d.scalar
                || m.size != if narrow { size + 1 } else { size }
                || (!d.scalar && m.q != if narrow { 1 } else { u32::from(high) })
            {
                return Err("invalid third operand arrangement".into());
            }
            fields.push(("Rm", m.number));
        }
        let class = if pairwide {
            "asimdmisc_P"
        } else if unary {
            if d.scalar {
                "asisdmisc_N"
            } else {
                "asimdmisc_N"
            }
        } else if d.scalar {
            "asisddiff_only"
        } else if narrow {
            "asimddiff_N"
        } else if addwide {
            "asimddiff_W"
        } else {
            "asimddiff_L"
        };
        return finish(&format!("{}_{class}", root.to_ascii_uppercase()), &fields).map(Some);
    }
    if high {
        return Ok(None);
    }
    let unary = matches!(
        root,
        "abs"
            | "neg"
            | "sqabs"
            | "sqneg"
            | "suqadd"
            | "usqadd"
            | "cls"
            | "clz"
            | "rev16"
            | "rev32"
            | "rev64"
            | "rbit"
            | "cnt"
    );
    let ternary = matches!(
        root,
        "add"
            | "sub"
            | "cmeq"
            | "cmgt"
            | "cmge"
            | "cmhi"
            | "cmhs"
            | "cmtst"
            | "cmlt"
            | "cmle"
            | "shadd"
            | "uhadd"
            | "srhadd"
            | "urhadd"
            | "shsub"
            | "uhsub"
            | "sqadd"
            | "uqadd"
            | "sqsub"
            | "uqsub"
            | "sshl"
            | "ushl"
            | "sqshl"
            | "uqshl"
            | "srshl"
            | "urshl"
            | "sqrshl"
            | "uqrshl"
            | "smax"
            | "smin"
            | "umax"
            | "umin"
            | "sabd"
            | "uabd"
            | "saba"
            | "uaba"
            | "mul"
            | "mla"
            | "mls"
            | "pmul"
            | "smaxp"
            | "sminp"
            | "umaxp"
            | "uminp"
            | "addp"
            | "sqdmulh"
            | "sqrdmulh"
    );
    if !unary && !ternary {
        return Ok(None);
    }
    // Shift immediates and indexed arithmetic have distinct operand rules.
    if a.get(2).is_some_and(|s| {
        s.contains('[')
            || (s.starts_with('#') && !matches!(root, "cmeq" | "cmgt" | "cmge" | "cmlt" | "cmle"))
    }) {
        return Ok(None);
    }
    count(a, if unary { 2 } else { 3 }, if unary { 2 } else { 3 })?;
    let d = Shape::parse(a[0])?;
    let mut n = Shape::parse(a[1])?;
    d.same(n)?;
    let zero = !unary && a[2].starts_with('#');
    if zero && integer(a[2])? != 0 {
        return Err("SIMD immediate comparison requires zero".into());
    }
    let sat = matches!(
        root,
        "sqadd"
            | "uqadd"
            | "sqsub"
            | "uqsub"
            | "sqshl"
            | "uqshl"
            | "sqrshl"
            | "uqrshl"
            | "sqabs"
            | "sqneg"
            | "suqadd"
            | "usqadd"
    );
    let doubling = matches!(root, "sqdmulh" | "sqrdmulh");
    let full = sat
        || matches!(
            root,
            "add"
                | "sub"
                | "sshl"
                | "ushl"
                | "srshl"
                | "urshl"
                | "cmeq"
                | "cmgt"
                | "cmge"
                | "cmhi"
                | "cmhs"
                | "cmtst"
                | "cmlt"
                | "cmle"
                | "abs"
                | "neg"
                | "addp"
        );
    if d.scalar {
        if !(sat || doubling || (full && d.size == 3 && root != "addp")) {
            return Ok(None);
        }
    } else {
        d.valid_vector()?;
        if d.size == 3 && !full {
            return Err("operation does not support D lanes".into());
        }
    }
    if doubling && !matches!(d.size, 1 | 2) {
        return Err("doubling multiply requires H or S".into());
    }
    match root {
        "cnt" | "rbit" | "rev16" | "pmul" if d.size != 0 => {
            return Err("operation requires byte lanes".into());
        }
        "rev32" if d.size > 1 => return Err("rev32 requires B/H".into()),
        "rev64" if d.size > 2 => return Err("rev64 requires B/H/S".into()),
        _ => {}
    }
    let mut actual = root;
    let mut m = if !unary && !zero {
        let m = Shape::parse(a[2])?;
        d.same(m)?;
        Some(m)
    } else {
        None
    };
    if !zero && matches!(root, "cmlt" | "cmle") {
        actual = if root == "cmlt" { "cmgt" } else { "cmge" };
        std::mem::swap(&mut n, m.as_mut().unwrap());
    }
    let mut fields = vec![
        ("Rd", d.number),
        ("Rn", n.number),
        ("size", d.size),
        ("Q", d.q),
    ];
    if let Some(m) = m {
        fields.push(("Rm", m.number));
    }
    let class = match (d.scalar, unary || zero) {
        (true, true) => "asisdmisc",
        (true, false) => "asisdsame",
        (false, true) => "asimdmisc",
        (false, false) => "asimdsame",
    };
    let tag = if zero {
        "Z"
    } else if unary {
        "R"
    } else {
        "only"
    };
    finish(
        &format!("{}_{class}_{tag}", actual.to_ascii_uppercase()),
        &fields,
    )
    .map(Some)
}

pub(super) fn shift(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    let high = op != "rev32" && op.ends_with('2');
    let root = if high { &op[..op.len() - 1] } else { op };
    let narrow = matches!(
        root,
        "shrn" | "rshrn" | "sqshrn" | "sqrshrn" | "sqshrun" | "sqrshrun" | "uqshrn" | "uqrshrn"
    );
    let wide = matches!(root, "sshll" | "ushll" | "sxtl" | "uxtl" | "shll");
    let left = matches!(root, "shl" | "sli" | "sqshl" | "sqshlu" | "uqshl");
    let right = matches!(
        root,
        "sshr" | "ushr" | "ssra" | "usra" | "srshr" | "urshr" | "srsra" | "ursra" | "sri"
    );
    let convert = matches!(root, "scvtf" | "ucvtf" | "fcvtzs" | "fcvtzu");
    if !(narrow || wide || left || right || convert)
        || !a
            .first()
            .is_some_and(|s| s.starts_with(['v', 'b', 'h', 's', 'd']))
    {
        return Ok(None);
    }
    if (left || convert) && !a.get(2).is_some_and(|s| s.starts_with('#')) {
        return Ok(None);
    }
    if high && !(narrow || wide) {
        return Err("unexpected high-half suffix".into());
    }
    let alias = matches!(root, "sxtl" | "uxtl");
    count(a, if alias { 2 } else { 3 }, if alias { 2 } else { 3 })?;
    let d = Shape::parse(a[0])?;
    let n = Shape::parse(a[1])?;
    if d.scalar != n.scalar {
        return Err("SIMD register classes must match".into());
    }
    let size = if narrow { d.size } else { n.size };
    let bits = if narrow { d.bits() } else { n.bits() };
    if narrow || wide {
        if size > 2 || (d.scalar && (wide || matches!(root, "shrn" | "rshrn") || high)) {
            return Err("invalid narrowing/widening form".into());
        }
        if narrow {
            if n.size != d.size + 1 || (!d.scalar && (n.q != 1 || d.q != u32::from(high))) {
                return Err("invalid narrowing shift arrangements".into());
            }
        } else if d.size != n.size + 1 || d.q != 1 || n.q != u32::from(high) {
            return Err("invalid widening shift arrangements".into());
        }
    } else {
        d.same(n)?;
        if d.scalar {
            if !(convert && size >= 2 || matches!(root, "sqshl" | "uqshl" | "sqshlu") || size == 3)
            {
                return Err("invalid scalar shift size".into());
            }
        } else {
            d.valid_vector()?;
        }
        if convert && size < 2 {
            return Err("fixed-point FP conversions require S/D".into());
        }
    }
    let amount = if alias {
        0
    } else {
        unsigned(a[2], i64::from(bits))?
    };
    let fixedwide = root == "shll";
    if fixedwide && amount != bits
        || !fixedwide && (left || wide) && amount >= bits
        || (narrow || right || convert) && amount == 0
    {
        return Err("shift amount out of range".into());
    }
    let imm = if narrow || right || convert {
        2 * bits - amount
    } else {
        bits + amount
    };
    let actual = match root {
        "sxtl" => "sshll",
        "uxtl" => "ushll",
        _ => root,
    };
    let class = if fixedwide {
        "asimdmisc_S"
    } else {
        match (d.scalar, narrow, wide, convert) {
            (true, true, _, _) => "asisdshf_N",
            (true, _, _, true) => "asisdshf_C",
            (true, _, _, _) => "asisdshf_R",
            (false, true, _, _) => "asimdshf_N",
            (false, _, true, _) => "asimdshf_L",
            (false, _, _, true) => "asimdshf_C",
            _ => "asimdshf_R",
        }
    };
    finish(
        &format!("{}_{class}", actual.to_ascii_uppercase()),
        &[
            ("Rd", d.number),
            ("Rn", n.number),
            ("Q", if narrow || wide { u32::from(high) } else { d.q }),
            ("size", size),
            ("immh", imm >> 3),
            ("immb", imm & 7),
        ],
    )
    .map(Some)
}
