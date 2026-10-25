//! Registration-table boundaries without merging input section identities.
use super::{Result, Section, Unit};
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::SymbolSection;

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(super) fn ordinary(s: &Section) -> bool {
    identifier(&s.name) && matches!(s.kind, 1 | 8)
}

/// Use the first occurrence's position, preserving input order within a name.
/// All indices, symbols and relocations still refer to their original section.
pub(super) fn order(sections: &[Section], indices: Vec<usize>) -> Vec<usize> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &i in &indices {
        if ordinary(&sections[i]) {
            groups.entry(&sections[i].name).or_default().push(i);
        }
    }
    let mut result = Vec::with_capacity(indices.len());
    for i in indices {
        if ordinary(&sections[i]) {
            if let Some(members) = groups.remove(sections[i].name.as_str()) {
                result.extend(members);
            }
        } else {
            result.push(i);
        }
    }
    result
}

/// An output group's start observes the strongest contribution alignment.
/// Do not round its end, or alter individual input section headers.
pub(super) fn alignments(sections: &[Section]) -> BTreeMap<usize, u64> {
    let mut groups: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for (i, s) in sections.iter().enumerate().filter(|(_, s)| ordinary(s)) {
        let entry = groups.entry(&s.name).or_insert((i, 1));
        entry.1 = entry.1.max(s.align);
    }
    groups.into_values().collect()
}

pub(super) fn symbols(
    sections: &[Section],
    units: &[Unit],
    globals: &BTreeMap<String, (usize, usize)>,
) -> Result<BTreeMap<String, (usize, bool)>> {
    let mut groups: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (i, s) in sections.iter().enumerate().filter(|(_, s)| ordinary(s)) {
        let span = groups.entry(&s.name).or_insert((i, i));
        let first = &sections[span.0];
        if first.flags != s.flags || first.kind != s.kind {
            return Err(format!(
                "incompatible registration section contributions for {}",
                s.name
            ));
        }
        span.1 = i;
    }
    let undefined: BTreeSet<_> = units
        .iter()
        .flat_map(|u| u.symbols.values())
        .filter(|s| {
            s.global() && s.section == SymbolSection::Undefined && !globals.contains_key(&s.name)
        })
        .map(|s| s.name.as_str())
        .collect();
    let mut result = BTreeMap::new();
    for (name, (first, last)) in groups {
        for (prefix, index, stop) in [("__start_", first, false), ("__stop_", last, true)] {
            let symbol = format!("{prefix}{name}");
            if undefined.contains(symbol.as_str()) {
                result.insert(symbol, (index, stop));
            }
        }
    }
    Ok(result)
}
