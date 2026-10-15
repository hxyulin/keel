use super::super::{integer, reg};
use super::{Lane, count, finish, register_list};

pub(super) fn instruction(op: &str, a: &[&str]) -> Result<Option<u32>, String> {
    if !matches!(
        op,
        "ld1"
            | "ld2"
            | "ld3"
            | "ld4"
            | "st1"
            | "st2"
            | "st3"
            | "st4"
            | "ld1r"
            | "ld2r"
            | "ld3r"
            | "ld4r"
    ) {
        return Ok(None);
    }
    count(a, 2, 3)?;
    let replicate = op.ends_with('r');
    let load = op.starts_with("ld");
    let structures = u32::from(op.as_bytes()[2] - b'0');
    let (list, lane) = if let Some((list, index)) = a[0].split_once("}[") {
        let index = index
            .strip_suffix(']')
            .ok_or("unclosed structure lane index")?;
        (format!("{list}}}"), Some(index))
    } else {
        (a[0].to_string(), None)
    };
    let (rt, size, q, regs) = if let Some(index) = lane {
        if replicate {
            return Err("replicate loads do not accept lane indices".into());
        }
        let inner = list
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or("expected braced structure lane list")?;
        let items = if let Some((start, end)) = inner.split_once('-') {
            let start = Lane::parse(&format!("{}[{index}]", start.trim()))?;
            let end = Lane::parse(&format!("{}[{index}]", end.trim()))?;
            let count = (end.number + 32 - start.number) % 32 + 1;
            if count > 4 || end.size != start.size {
                return Err("invalid structure lane range".into());
            }
            (0..count)
                .map(|i| Lane {
                    number: (start.number + i) % 32,
                    ..start
                })
                .collect::<Vec<_>>()
        } else {
            inner
                .split(',')
                .map(|r| Lane::parse(&format!("{}[{index}]", r.trim())))
                .collect::<Result<Vec<_>, _>>()?
        };
        if items.len() != structures as usize {
            return Err("structure lane list length must match mnemonic".into());
        }
        for (i, r) in items.iter().enumerate() {
            if r.size != items[0].size || r.number != (items[0].number + i as u32) % 32 {
                return Err(
                    "structure lane list must be consecutive with matching element sizes".into(),
                );
            }
        }
        let first = items[0];
        (
            first.number,
            first.size,
            (first.index << first.size) >> 3,
            items.len(),
        )
    } else {
        let regs = register_list(&list)?;
        let first = regs[0];
        if (structures != 1 || replicate) && regs.len() != structures as usize {
            return Err("structure register list length must match mnemonic".into());
        }
        if !replicate && structures != 1 && first.size == 3 && first.q == 0 {
            return Err("interleaved structures do not support 1D".into());
        }
        (first.number, first.size, first.q, regs.len())
    };
    // Unlike scalar memory operands, structures have no offset addressing.
    let base = super::super::memory::atomic_base(a[1])?;
    // Require the canonical [base] spelling, not an explicit offset.
    if a[1].contains(',') {
        return Err("structure accesses require [base] without an offset".into());
    }
    let bytes = if lane.is_some() || replicate {
        (8u32 << size) / 8 * structures
    } else {
        (8u32 << q) * regs as u32
    };
    let post = if a.len() == 3 {
        if a[2].starts_with('#') {
            if integer(a[2])? != i64::from(bytes) {
                return Err(format!("structure post-index immediate must be #{bytes}"));
            }
            Some(None)
        } else {
            let m = reg(a[2], false, false)?;
            if m.bits != 64 {
                return Err("structure post-index requires X0..X30".into());
            }
            Some(Some(m.number))
        }
    } else {
        None
    };
    let (class, tag) = if let Some(index) = lane {
        let letter = ["B", "H", "S", "D"][size as usize];
        let low = letter.to_ascii_lowercase();
        let tag = match post {
            None => format!("{letter}{structures}_{structures}{low}"),
            Some(None) => format!("{letter}{structures}_i{structures}{low}"),
            Some(Some(_)) => format!("{letter}X{structures}_r{structures}{low}"),
        };
        let _ = index;
        (
            if post.is_some() {
                "asisdlsop"
            } else {
                "asisdlso"
            },
            tag,
        )
    } else if replicate {
        let tag = match post {
            None => format!("R{structures}"),
            Some(None) => format!("R{structures}_i"),
            Some(Some(_)) => format!("RX{structures}_r"),
        };
        (
            if post.is_some() {
                "asisdlsop"
            } else {
                "asisdlso"
            },
            tag,
        )
    } else {
        let n = regs;
        let tag = match (structures, post) {
            (1, None) => format!("R{n}_{n}v"),
            (_, None) => format!("R{n}"),
            (1, Some(None)) => format!("I{n}_i{n}"),
            (_, Some(None)) => format!("I{n}_i"),
            (1, Some(Some(_))) => format!("R{n}_r{n}"),
            (_, Some(Some(_))) => format!("R{n}_r"),
        };
        (
            if post.is_some() {
                "asisdlsep"
            } else {
                "asisdlse"
            },
            tag,
        )
    };
    let mut fields = vec![("Rt", rt), ("Rn", base.number), ("Q", q), ("size", size)];
    if let Some(index) = lane {
        let i = index
            .parse::<u32>()
            .map_err(|_| "invalid structure lane index")?;
        let packed = i << size;
        fields.push(("S", (packed >> 2) & 1));
        fields.iter_mut().find(|(n, _)| *n == "size").unwrap().1 = match size {
            0 => packed & 3,
            1 => packed & 2,
            2 | 3 => size - 2,
            _ => unreachable!(),
        };
    }
    if let Some(Some(m)) = post {
        fields.push(("Rm", m));
    }
    let root = if replicate {
        format!("LD{structures}R")
    } else {
        format!("{}{structures}", if load { "LD" } else { "ST" })
    };
    finish(&format!("{root}_{class}_{tag}"), &fields).map(Some)
}
