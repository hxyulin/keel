//! Input ordering and demand-driven archive extraction. No filesystem access.
use super::dynamic::Library;
use crate::{Input, Result};
use object::{Object, ObjectSymbol};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Named<'a> {
    pub name: String,
    pub bytes: &'a [u8],
}
#[derive(Default)]
// Archive members and selected inputs retain their original input storage.
// Ordered maps still compare name contents, including equal names in different
// members/DSOs, without allocating another copy for each demand/provider set.
struct Symbols<'data> {
    defined: BTreeMap<&'data str, u8>,
    required: BTreeSet<&'data str>,
}
fn symbols(bytes: &[u8]) -> Result<Symbols<'_>> {
    let file = object::File::parse(bytes).map_err(|e| e.to_string())?;
    let mut out = Symbols::default();
    for s in file.symbols() {
        if !s.is_global() && !s.is_weak() {
            continue;
        }
        let name = s.name().map_err(|e| e.to_string())?;
        if s.is_undefined() {
            if !s.is_weak() && !super::linker_owned(name) {
                out.required.insert(name);
            }
        } else {
            out.defined.insert(
                name,
                if s.is_weak() {
                    1
                } else if s.is_common() {
                    2
                } else {
                    3
                },
            );
        }
    }
    Ok(out)
}
/// Normal left-to-right archives, rescanned to a fixed point within each
/// archive. Undefined weak references do not cause extraction. Thin archives
/// would require filesystem access and are deliberately rejected.
pub(super) fn prepare<'a>(
    inputs: &[Input<'a>],
    entry: &str,
) -> Result<(Vec<Named<'a>>, Vec<Library<'a>>)> {
    let mut selected = Vec::new();
    let mut libraries = Vec::new();
    let mut defined = BTreeMap::<&str, u8>::new();
    let mut required = BTreeSet::from([entry]);
    for input in inputs {
        let result = (|| -> Result<()> {
            if input.bytes.starts_with(b"!<arch>\n") || input.bytes.starts_with(b"!<thin>\n") {
                let archive = object::read::archive::ArchiveFile::parse(input.bytes)
                    .map_err(|e| e.to_string())?;
                if archive.is_thin() {
                    return Err("thin archives are unsupported; use a regular archive".into());
                }
                let mut members = Vec::new();
                for member in archive.members() {
                    let member = member.map_err(|e| e.to_string())?;
                    let bytes = member.data(input.bytes).map_err(|e| e.to_string())?;
                    let name =
                        format!("{}({})", input.name, String::from_utf8_lossy(member.name()));
                    let summary = symbols(bytes).map_err(|e| format!("{name}: {e}"))?;
                    members.push((Named { name, bytes }, summary));
                }
                while let Some(index) = members.iter().position(|(_, s)| {
                    s.defined.iter().any(|(n, rank)| {
                        (required.contains(n) && !defined.contains_key(n))
                            || (defined.get(n) == Some(&2) && *rank == 3)
                    })
                }) {
                    let (member, summary) = members.remove(index);
                    absorb(summary, &mut defined, &mut required);
                    selected.push(member);
                }
            } else {
                let file = object::File::parse(input.bytes).map_err(|e| {
                    if input.bytes.windows(5).any(|w|w==b"GROUP") || input.bytes.windows(5).any(|w|w==b"INPUT") {
                        "GNU linker scripts are unsupported; pass concrete ELF libraries (e.g. -l:libc.so.6)".into()
                    } else { e.to_string() }
                })?;
                if file.kind() == object::ObjectKind::Dynamic {
                    let library = Library::parse(input.name, input.bytes)?;
                    if let Some(existing) = libraries
                        .iter()
                        .find(|l: &&Library| l.needed == library.needed)
                    {
                        if existing.exports != library.exports {
                            return Err(format!(
                                "conflicting interfaces for shared-library SONAME `{}`",
                                library.needed
                            ));
                        }
                        return Ok(());
                    }
                    if let Some(name) = library.required.iter().find(|n| super::linker_owned(n)) {
                        return Err(format!(
                            "shared library requires reserved linker-owned symbol `{name}`"
                        ));
                    }
                    required.extend(library.required.iter().copied());
                    for name in library.exports.keys() {
                        defined.entry(*name).or_insert(3);
                    }
                    libraries.push(library);
                } else {
                    let summary = symbols(input.bytes)?;
                    absorb(summary, &mut defined, &mut required);
                    selected.push(Named {
                        name: input.name.into(),
                        bytes: input.bytes,
                    });
                }
            }
            Ok(())
        })();
        result.map_err(|e| format!("{}: {e}", input.name))?;
    }
    Ok((selected, libraries))
}

fn absorb<'a>(
    summary: Symbols<'a>,
    defined: &mut BTreeMap<&'a str, u8>,
    required: &mut BTreeSet<&'a str>,
) {
    for (name, rank) in summary.defined {
        defined
            .entry(name)
            .and_modify(|r| *r = (*r).max(rank))
            .or_insert(rank);
    }
    required.extend(summary.required);
}
