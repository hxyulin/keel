//! SME2 register groups and multi-vector ZA arrays.
use super::*;
use crate::FieldId;

struct Group {
    first: u32,
    size: u32,
    len: u32,
}
fn group(s: &str) -> Result<Group, String> {
    let inner = s
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected braced Z register group")?
        .trim();
    let regs = if let Some((first, last)) = inner.split_once('-') {
        let (first, size) = lane(first.trim(), "z", 31)?;
        let (last, end_size) = lane(last.trim(), "z", 31)?;
        let len = (last + 32 - first) % 32 + 1;
        if size != end_size || !matches!(len, 2 | 4) {
            return Err("Z group requires two or four consecutive matching registers".into());
        }
        (0..len)
            .map(|i| ((first + i) % 32, size))
            .collect::<Vec<_>>()
    } else {
        inner
            .split(',')
            .map(|s| lane(s.trim(), "z", 31))
            .collect::<Result<Vec<_>, _>>()?
    };
    let (first, size) = regs[0];
    if !matches!(regs.len(), 2 | 4)
        || regs
            .iter()
            .enumerate()
            .any(|(i, (n, s))| *s != size || *n != (first + i as u32) % 32)
    {
        return Err("Z group requires two or four consecutive matching registers".into());
    }
    Ok(Group {
        first,
        size,
        len: regs.len() as u32,
    })
}
fn aligned(g: &Group) -> Result<(), String> {
    if !g.first.is_multiple_of(g.len) {
        return Err("Z group must start at a multiple of its length".into());
    }
    Ok(())
}
struct Array {
    size: u32,
    selector: u32,
    offset: u32,
}
fn array(s: &str, len: u32) -> Result<Array, String> {
    let (head, index) = s
        .split_once('[')
        .ok_or("ZA array requires [W8..W11, offset, vgx2/vgx4]")?;
    let size = element(
        head.strip_prefix("za.")
            .ok_or("expected ZA element array")?,
    )?;
    let index = index.strip_suffix(']').ok_or("unclosed ZA array")?;
    let a: Vec<_> = index.split(',').map(str::trim).collect();
    count(&a, 2, 3)?;
    let selector = number(a[0], "w", 11)?;
    if selector < 8 {
        return Err("ZA array selector must be W8..W11".into());
    }
    let offset = unsigned(a[1], 7)?;
    if a.len() == 3 && a[2] != format!("vgx{len}") {
        return Err("ZA vgx must match Z group length".into());
    }
    Ok(Array {
        size,
        selector: selector - 8,
        offset,
    })
}
fn array_fields(a: &Array) -> Fields {
    vec![(FieldId::Rv, a.selector), (FieldId::off3, a.offset)]
}
fn require_f64(features: TargetFeatures) -> Result<(), String> {
    if !features.supports("FEAT_SME_F64F64") {
        return Err("instruction requires FEAT_SME_F64F64".into());
    }
    Ok(())
}
fn grouped_move(a: &[&str], features: TargetFeatures) -> Result<u32, String> {
    count(a, 2, 2)?;
    let insert = a[0].starts_with("za");
    let (sl, z) = if insert { (a[0], a[1]) } else { (a[1], a[0]) };
    let g = group(z)?;
    aligned(&g)?;
    let (name, mut fields) = if sl.starts_with("za.") {
        let ar = array(sl, g.len)?;
        if ar.size != 3 || g.size != 3 {
            return Err("ZA array move requires .d elements".into());
        }
        (
            format!(
                "mova_{}_{}_1",
                if insert { "za" } else { "mz" },
                if insert {
                    format!("mz{}", g.len)
                } else {
                    format!("za{}", g.len)
                }
            ),
            array_fields(&ar),
        )
    } else {
        let (head, index) = sl.split_once('[').ok_or("expected ZA tile slice range")?;
        let index = index
            .strip_suffix(']')
            .ok_or("unclosed ZA tile slice range")?;
        let (selector, range) = index
            .split_once(',')
            .ok_or("tile slice range requires W12..W15 and offset range")?;
        let (start, end) = range
            .trim()
            .split_once(':')
            .ok_or("tile slice group requires a range")?;
        let start = unsigned(start.trim(), 15)?;
        let end = unsigned(end.trim(), 15)?;
        let s = slice(&format!("{head}[{}, {start}]", selector.trim()))?;
        if s.size == 4
            || g.size != s.size
            || !start.is_multiple_of(g.len)
            || end != start + g.len - 1
            || end >= (16 >> s.size).max(g.len)
        {
            return Err("tile slice range must align with group length and element size".into());
        }
        let mut fields = vec![(FieldId::V, s.vertical), (FieldId::Rs, s.selector)];
        if s.size != 0 {
            fields.push((if insert { FieldId::ZAd } else { FieldId::ZAn }, s.tile));
        }
        let bits = (4 - s.size).saturating_sub(g.len.ilog2());
        if bits != 0 {
            fields.push((
                [FieldId::o1, FieldId::off2, FieldId::off3][bits as usize - 1],
                start / g.len,
            ));
        }
        (
            format!(
                "mova_{}_{}_{}1",
                if insert {
                    format!("za{}", g.len)
                } else {
                    format!("mz{}", g.len)
                },
                if insert { "z" } else { "za" },
                ["b", "h", "w", "d"][s.size as usize]
            ),
            fields,
        )
    };
    fields.push((
        if insert { FieldId::Zn } else { FieldId::Zd },
        g.first / g.len,
    ));
    encode_with_features(&name, &fields, features)
}

pub(super) fn instruction(
    op: &str,
    a: &[&str],
    features: TargetFeatures,
) -> Result<Option<u32>, String> {
    if matches!(op, "mov" | "mova") && a.iter().any(|s| s.starts_with('{')) {
        return grouped_move(a, features).map(Some);
    }
    if op == "zero"
        && a.first()
            .is_some_and(|s| s.trim_matches(['{', '}', ' ']) == "zt0")
    {
        count(a, 1, 1)?;
        // Unlike whole-ZA zero, ZT0 requires its explicit braced list.
        if a[0]
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .map(str::trim)
            != Some("zt0")
        {
            return Err("zero ZT0 requires {zt0}".into());
        }
        return encode_with_features("zero_zt_i_", &[], features).map(Some);
    }
    if matches!(op, "ldr" | "str") && a.first() == Some(&"zt0") {
        count(a, 2, 2)?;
        let addr = address(a[1])?;
        count(&addr, 1, 1)?;
        return encode_with_features(
            &format!("{op}_zt_br_"),
            &[(FieldId::Rn, base(addr[0])?)],
            features,
        )
        .map(Some);
    }
    if !matches!(op, "add" | "sub" | "fmla" | "fmls")
        || !a.first().is_some_and(|s| s.starts_with("za."))
    {
        return Ok(None);
    }
    count(a, 2, 3)?;
    let fp = op.starts_with('f');
    if fp && a.len() != 3 {
        return Err("ZA multiply-accumulate requires three operands".into());
    }
    let n = group(a[1])?;
    let ar = array(a[0], n.len)?;
    if ar.size != n.size || (!fp && !matches!(ar.size, 2 | 3)) || (fp && !matches!(ar.size, 1..=3))
    {
        return Err("ZA array and vector elements must match legal arithmetic widths".into());
    }
    // The shared S/D Arm record has a single SME2 condition; the D operand
    // additionally requires the optional FP64 matrix type.
    if fp && ar.size == 3 {
        require_f64(features)?;
    }
    let half = fp && ar.size == 1;
    let mut f = array_fields(&ar);
    let name;
    if a.len() == 2 {
        aligned(&n)?;
        name = format!("{op}_za_zw_{}x{}", n.len, n.len);
        f.push((FieldId::Zm, n.first / n.len));
    } else if a[2].starts_with('{') {
        let m = group(a[2])?;
        aligned(&n)?;
        aligned(&m)?;
        if m.len != n.len || m.size != n.size {
            return Err("ZA arithmetic Z groups must match".into());
        }
        name = format!(
            "{op}_za_zzw_{}x{}{}",
            n.len,
            n.len,
            if half { "_16" } else { "" }
        );
        f.extend([
            (FieldId::Zn, n.first / n.len),
            (FieldId::Zm, m.first / m.len),
        ]);
    } else if a[2].contains('[') {
        if !fp {
            return Err("indexed ZA form requires fmla/fmls".into());
        }
        aligned(&n)?;
        let (z, index) = a[2].split_once('[').unwrap();
        let (m, size) = lane(z, "z", 15)?;
        let index = unsigned(
            index.strip_suffix(']').ok_or("unclosed indexed vector")?,
            (16 >> size) - 1,
        )?;
        if size != n.size {
            return Err("indexed source size must match ZA array".into());
        }
        name = format!(
            "{op}_za_zzi_{}{}xi",
            ["b", "h", "s", "d"][size as usize],
            n.len
        );
        f.extend([(FieldId::Zn, n.first / n.len), (FieldId::Zm, m)]);
        if size == 1 {
            f.extend([(FieldId::i3h, index >> 1), (FieldId::i3l, index & 1)]);
        } else {
            f.push((if size == 2 { FieldId::i2 } else { FieldId::i1 }, index));
        }
        return encode_with_features(&name, &f, features).map(Some);
    } else {
        let (m, size) = lane(a[2], "z", 15)?;
        if size != n.size {
            return Err("ZA arithmetic scalar vector element must match".into());
        }
        name = format!("{op}_za_zzv_{}x1{}", n.len, if half { "_16" } else { "" });
        f.extend([(FieldId::Zn, n.first), (FieldId::Zm, m)]);
    }
    if !half {
        f.push((FieldId::sz, ar.size - 2));
    }
    encode_with_features(&name, &f, features).map(Some)
}
