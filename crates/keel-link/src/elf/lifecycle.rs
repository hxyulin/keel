//! Executable initialization/finalization contributions. Layout keeps each
//! input's symbol/relocation identity while placing runtime spans together.
use super::{BASE, Result, Section, Unit};
use bbtree::unsync::BBTreeMap as BTreeMap;
use object::SymbolSection;

const ARRAYS: [(&str, u32, u64, u64); 3] = [
    (".preinit_array", 16, 32, 33),
    (".init_array", 14, 25, 27),
    (".fini_array", 15, 26, 28),
];

fn group(name: &str) -> Option<usize> {
    if name == ".tm_clone_table" {
        return Some(5);
    }
    if name == ".init" {
        return Some(0);
    }
    if name == ".fini" {
        return Some(1);
    }
    ARRAYS
        .iter()
        .position(|(base, ..)| {
            name == *base || name.strip_prefix(base).is_some_and(|s| s.starts_with('.'))
        })
        .map(|i| i + 2)
}

fn priority(name: &str, g: usize) -> Result<(u16, bool)> {
    let base = ARRAYS[g - 2].0;
    if name == base {
        return Ok((u16::MAX, true));
    }
    if g == 2 {
        return Err("prioritized preinitialization arrays are unsupported".into());
    }
    let suffix = &name[base.len() + 1..];
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid initialization priority on {name}"));
    }
    Ok((
        suffix
            .parse()
            .map_err(|_| format!("initialization priority exceeds 65535 on {name}"))?,
        false,
    ))
}

pub(super) fn validate(
    name: &str,
    kind: u32,
    flags: u64,
    size: u64,
    alignment: u64,
    entsize: u64,
) -> Result<bool> {
    let g = group(name);
    let array_kind = ARRAYS.iter().any(|(_, k, ..)| *k == kind);
    let Some(g) = g else {
        if array_kind {
            return Err(format!("unsupported initialization array name {name}"));
        }
        return Ok(false);
    };
    if g == 5 {
        if kind != 1 || flags != 3 || size != 0 || alignment != 8 || entsize != 0 {
            return Err(
                ".tm_clone_table currently requires empty writable PROGBITS with alignment 8"
                    .into(),
            );
        }
        return Ok(true);
    }
    if g < 2 {
        if kind != 1 || flags != 6 || entsize != 0 {
            return Err(format!("{name} requires executable, read-only PROGBITS"));
        }
    } else {
        priority(name, g)?;
        if kind != ARRAYS[g - 2].1
            || flags != 3
            || !matches!(entsize, 0 | 8)
            || (alignment != 8 && (size != 0 || alignment > 8))
            || !size.is_multiple_of(8)
        {
            return Err(format!(
                "{name} requires its matching array type, WRITE/ALLOC, 8-byte entries and alignment"
            ));
        }
    }
    Ok(true)
}

pub(super) fn order(sections: &[Section], permissions: u32, bss: bool) -> Vec<usize> {
    let indices: Vec<_> = sections
        .iter()
        .enumerate()
        .filter(|(_, s)| s.permissions() == permissions && s.bss == bss)
        .map(|(i, _)| i)
        .collect();
    let mut seen = [false; 6];
    let mut result = Vec::with_capacity(indices.len());
    for &i in &indices {
        if let Some(g) = group(&sections[i].name) {
            if seen[g] {
                continue;
            }
            seen[g] = true;
            let mut members: Vec<_> = indices
                .iter()
                .copied()
                .filter(|&j| group(&sections[j].name) == Some(g))
                .collect();
            if (2..5).contains(&g) {
                // Validated before layout; stable ties preserve input order.
                members.sort_by_key(|&j| {
                    priority(&sections[j].name, g).expect("validated lifecycle priority")
                });
            }
            result.extend(members);
        } else {
            result.push(i);
        }
    }
    result
}

pub(super) struct Plan {
    functions: Vec<(u64, usize, u64)>,
}
impl Plan {
    pub(super) fn new(
        sections: &[Section],
        units: &[Unit],
        globals: &BTreeMap<String, (usize, usize)>,
    ) -> Result<Self> {
        let mut functions = Vec::new();
        for (name, tag) in [("_init", 12), ("_fini", 13)] {
            let Some(&(ui, si)) = globals.get(name) else {
                continue;
            };
            let symbol = &units[ui].symbols[&si];
            let section = match symbol.section {
                SymbolSection::Section(i) => units[ui].sections.get(&i.0),
                _ => None,
            }
            .ok_or_else(|| format!("{name} must be defined in executable code"))?;
            if sections[*section].permissions() != 5 || symbol.value >= sections[*section].size {
                return Err(format!("{name} must be inside executable code"));
            }
            functions.push((tag, *section, symbol.value));
        }
        Ok(Self { functions })
    }
    pub(super) fn tag_count(&self, sections: &[Section]) -> usize {
        self.functions.len()
            + (2..5)
                .filter(|&g| {
                    sections
                        .iter()
                        .any(|s| s.size != 0 && group(&s.name) == Some(g))
                })
                .count()
                * 2
    }
    pub(super) fn tags(&self, sections: &[Section]) -> Result<Vec<(u64, u64)>> {
        let mut tags: Vec<_> = self
            .functions
            .iter()
            .map(|&(tag, index, value)| (tag, BASE + sections[index].offset + value))
            .collect();
        for g in 2..5 {
            let mut contributions: Vec<_> = sections
                .iter()
                .filter(|s| s.size != 0 && group(&s.name) == Some(g))
                .collect();
            contributions.sort_by_key(|s| s.offset);
            let Some(first) = contributions.first() else {
                continue;
            };
            let mut end = first.offset;
            for s in &contributions {
                if s.offset != end {
                    return Err("initialization array span is not contiguous".into());
                }
                end = s
                    .offset
                    .checked_add(s.size)
                    .ok_or("initialization span overflow")?;
            }
            let (_, _, address_tag, size_tag) = ARRAYS[g - 2];
            tags.extend([
                (address_tag, BASE + first.offset),
                (size_tag, end - first.offset),
            ]);
        }
        Ok(tags)
    }
}

pub(super) fn code_padding(bytes: &mut [u8], sections: &[Section]) {
    for g in 0..2 {
        let mut contributions: Vec<_> = sections
            .iter()
            .filter(|s| group(&s.name) == Some(g))
            .collect();
        contributions.sort_by_key(|s| s.offset);
        for pair in contributions.windows(2) {
            let end = (pair[0].offset + pair[0].size) as usize;
            let start = pair[1].offset as usize;
            bytes[end..start].fill(0x90);
        }
    }
}
