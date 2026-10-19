//! Fixed-point Mach-O archive extraction over eagerly validated members.
use crate::read::{Object, Symbol};
use std::collections::{BTreeMap, BTreeSet};

fn definition(s: &Symbol) -> bool {
    s.kind & 1 != 0 && s.kind & 0xe != 0
}

#[derive(Default)]
struct Resolution<'a> {
    defined: BTreeSet<&'a str>,
    needed: BTreeSet<&'a str>,
    providers: BTreeMap<&'a str, Vec<usize>>,
    ready: BTreeSet<usize>,
}

impl<'a> Resolution<'a> {
    fn need(&mut self, name: &'a str) {
        if !self.defined.contains(name)
            && self.needed.insert(name)
            && let Some(members) = self.providers.get(name)
        {
            self.ready.extend(members);
        }
    }

    fn include(&mut self, object: &'a Object) {
        // Definitions only accumulate, so previously satisfied references never
        // become unresolved again. Commons and weak definitions satisfy names
        // here; their precedence and duplicate diagnostics belong to prepare.
        for s in &object.symbols {
            if s.kind & 1 != 0 && (s.kind & 0xe != 0 || s.value != 0) {
                self.defined.insert(&s.name);
                self.needed.remove(s.name.as_str());
            } else if s.kind & 0xe == 0 && s.value == 0 {
                self.need(&s.name);
            }
        }
    }
}

fn selection(objects: &[Object], members: &[Object]) -> Vec<usize> {
    let mut resolution = Resolution::default();
    // Build our own index from parsed symbols, never from the archive's index.
    // Commons cannot pull a member, even though included commons satisfy names.
    for (index, member) in members.iter().enumerate() {
        for s in member.symbols.iter().filter(|s| definition(s)) {
            resolution.providers.entry(&s.name).or_default().push(index);
        }
    }
    resolution.need("_main");
    for object in objects {
        resolution.include(object);
    }
    let mut selected = Vec::new();
    let mut included = vec![false; members.len()];
    while let Some(index) = resolution.ready.pop_first() {
        let member = &members[index];
        // A newly included object can satisfy other pending candidates. Recheck
        // them and always choose the earliest still-needed unselected member,
        // including when a later member introduces a dependency on an earlier
        // archive. This is the original ordered fixed point, not a FIFO queue.
        if included[index]
            || !member
                .symbols
                .iter()
                .any(|s| definition(s) && resolution.needed.contains(s.name.as_str()))
        {
            continue;
        }
        included[index] = true;
        selected.push(index);
        resolution.include(member);
    }
    selected
}

pub(crate) fn archives(objects: &mut Vec<Object>, members: Vec<Object>) {
    if members.is_empty() {
        return;
    }
    let selected = selection(objects, &members);
    // Stable slots let us move each member once without shifting the remaining
    // archive objects after every extraction. Selection borrows only names.
    let mut members: Vec<_> = members.into_iter().map(Some).collect();
    objects.extend(
        selected
            .into_iter()
            .map(|index| members[index].take().unwrap()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keep the old full-set algorithm as an independent ordering oracle. The
    // generated graphs exercise interactions beyond the named image fixtures.
    fn original(objects: &[Object], members: &[Object]) -> Vec<usize> {
        let mut objects: Vec<_> = objects.iter().collect();
        let mut remaining: Vec<_> = members.iter().enumerate().collect();
        let mut selected = Vec::new();
        loop {
            let defined: BTreeSet<_> = objects
                .iter()
                .flat_map(|o| &o.symbols)
                .filter(|s| s.kind & 1 != 0 && (s.kind & 0xe != 0 || s.value != 0))
                .map(|s| s.name.as_str())
                .collect();
            let needed: BTreeSet<_> = objects
                .iter()
                .flat_map(|o| &o.symbols)
                .filter(|s| s.kind & 0xe == 0 && s.value == 0 && !defined.contains(s.name.as_str()))
                .map(|s| s.name.as_str())
                .chain((!defined.contains("_main")).then_some("_main"))
                .collect();
            let Some(index) = remaining.iter().position(|(_, o)| {
                o.symbols.iter().any(|s| {
                    s.kind & 1 != 0 && s.kind & 0xe != 0 && needed.contains(s.name.as_str())
                })
            }) else {
                return selected;
            };
            let (index, member) = remaining.remove(index);
            selected.push(index);
            objects.push(member);
        }
    }

    #[test]
    fn generated_symbol_graphs_preserve_original_extraction_sequence() {
        let mut seed = 0x6b65656cu64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 32) as usize
        };
        for case in 0..10_000 {
            let direct = next() % 4;
            let count = direct + next() % 12;
            let objects: Vec<_> = (0..count)
                .map(|index| Object {
                    name: index.to_string(),
                    sections: Vec::new(),
                    symbols: (0..next() % 10)
                        .map(|_| {
                            let name = match next() % 9 {
                                0 => "_main".into(),
                                n => format!("_s{n}"),
                            };
                            // Undefined, weak undefined, common, private common,
                            // strong/weak/private section, absolute, local.
                            let (kind, desc, value) = match next() % 9 {
                                0 => (1, 0, 0),
                                1 => (1, 0x40, 0),
                                2 => (1, 0, 8),
                                3 => (0x11, 0, 16),
                                4 => (0xf, 0, 0),
                                5 => (0xf, 0x80, 0),
                                6 => (0x1f, 0, 0),
                                7 => (3, 0, 7),
                                _ => (0xe, 0, 0),
                            };
                            Symbol {
                                name,
                                kind,
                                section: 0,
                                desc,
                                value,
                            }
                        })
                        .collect(),
                    minimum: 11 << 16,
                    debug_bytes: None,
                })
                .collect();
            let (objects, members) = objects.split_at(direct);
            assert_eq!(
                selection(objects, members),
                original(objects, members),
                "case {case}"
            );
        }
    }
}
