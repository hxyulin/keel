use super::{Lane, Shape, count, finish};

pub(super) fn instruction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    if !a
        .get(2)
        .is_some_and(|s| s.starts_with('v') && s.contains('['))
    {
        return Ok(None);
    }
    let high = op.ends_with('2');
    let root = if high { &op[..op.len() - 1] } else { op };
    let fp = matches!(root, "fmla" | "fmls" | "fmul" | "fmulx");
    let wide = matches!(
        root,
        "smlal"
            | "umlal"
            | "smlsl"
            | "umlsl"
            | "smull"
            | "umull"
            | "sqdmlal"
            | "sqdmlsl"
            | "sqdmull"
    );
    let regular = matches!(root, "mul" | "mla" | "mls" | "sqdmulh" | "sqrdmulh");
    if !fp && !wide && !regular {
        return Ok(None);
    }
    count(a, 3, 3)?;
    let d = Shape::parse(a[0])?;
    let n = Shape::parse(a[1])?;
    let m = Lane::parse(a[2])?;
    if d.scalar != n.scalar || n.size != m.size {
        return Err("indexed operand element sizes must match".into());
    }
    if fp {
        if high || d.size < 2 {
            return Err("indexed FP requires S/D without high-half suffix".into());
        }
        d.same(n)?;
        if !d.scalar {
            d.valid_vector()?;
        }
    } else {
        if !matches!(n.size, 1 | 2)
            || d.scalar
                && !matches!(
                    root,
                    "sqdmlal" | "sqdmlsl" | "sqdmull" | "sqdmulh" | "sqrdmulh"
                )
        {
            return Err(
                "indexed integer arithmetic requires H/S and a supported scalar form".into(),
            );
        }
        if wide {
            if d.size != n.size + 1
                || d.scalar && high
                || !d.scalar && (d.q != 1 || n.q != u32::from(high))
            {
                return Err("invalid indexed widening arrangements".into());
            }
        } else {
            if high {
                return Err("unexpected high-half suffix".into());
            }
            d.same(n)?;
            if !d.scalar {
                d.valid_vector()?;
            }
        }
    }
    if m.size == 1 && m.number >= 16 {
        return Err("H-element multiplier requires V0..V15".into());
    }
    let (l, mbit, h) = match m.size {
        1 => ((m.index >> 1) & 1, m.index & 1, m.index >> 2),
        2 => (m.index & 1, m.number >> 4, m.index >> 1),
        3 => (0, m.number >> 4, m.index),
        _ => return Err("unsupported indexed multiplier size".into()),
    };
    let class = if d.scalar { "asisdelem" } else { "asimdelem" };
    let tag = if fp {
        "R_SD"
    } else if wide {
        "L"
    } else {
        "R"
    };
    finish(
        &format!("{}_{class}_{tag}", root.to_ascii_uppercase()),
        &[
            ("Rd", d.number),
            ("Rn", n.number),
            ("Rm", m.number & 15),
            ("L", l),
            ("M", mbit),
            ("H", h),
            ("Q", if wide { u32::from(high) } else { d.q }),
            ("size", n.size),
            ("sz", n.size.saturating_sub(2)),
        ],
    )
    .map(Some)
}
