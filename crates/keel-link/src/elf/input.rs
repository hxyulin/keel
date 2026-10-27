//! Input ordering and demand-driven archive extraction. No filesystem access.
use super::dynamic::Library;
use super::groups::Plan;
use crate::{Input, Result};
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::{Object, ObjectSymbol};

pub(super) struct Named<'a> {
    pub name: String,
    pub bytes: &'a [u8],
    pub groups: Plan,
}
#[derive(Default)]
// Archive members and selected inputs retain their original input storage.
// Ordered maps still compare name contents, including equal names in different
// members/DSOs, without allocating another copy for each demand/provider set.
struct Symbols<'data> {
    defined: BTreeMap<&'data str, u8>,
    required: BTreeSet<&'data str>,
}
fn symbols<'a>(bytes: &'a [u8], selected: &BTreeSet<String>) -> Result<(Symbols<'a>, Plan)> {
    let file = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes)
        .map_err(|e| e.to_string())?;
    let plan = Plan::new(&file, selected)?;
    let mut out = Symbols::default();
    for s in file.symbols() {
        if plan.dead_symbol(s.index().0, s.section()) {
            if plan.live.contains(&s.index().0) && s.is_global() && !s.is_weak() {
                out.required.insert(s.name().map_err(|e| e.to_string())?);
            }
            continue;
        }
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
    Ok((out, plan))
}
/// Normal left-to-right archives, rescanned to a fixed point within each
/// archive. Undefined weak references do not cause extraction. Thin archives
/// would require filesystem access and are deliberately rejected.
pub(super) fn prepare<'a>(
    inputs: &[Input<'a>],
    entry: &str,
) -> Result<(Vec<Named<'a>>, Vec<Library<'a>>)> {
    let mut selected = Vec::new();
    let mut signatures = BTreeSet::new();
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
                    members.push((name, bytes));
                }
                loop {
                    // Recompute against selected signatures: an unextracted
                    // member must never reserve a COMDAT signature.
                    let mut candidate = None;
                    for (index, (name, bytes)) in members.iter().enumerate() {
                        let (summary, groups) =
                            symbols(bytes, &signatures).map_err(|e| format!("{name}: {e}"))?;
                        if summary.defined.iter().any(|(n, rank)| {
                            (required.contains(n) && !defined.contains_key(n))
                                || (defined.get(n) == Some(&2) && *rank == 3)
                        }) {
                            candidate = Some((index, summary, groups));
                            break;
                        }
                    }
                    let Some((index, summary, groups)) = candidate else {
                        break;
                    };
                    let (name, bytes) = members.remove(index);
                    signatures.extend(groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    selected.push(Named {
                        name,
                        bytes,
                        groups,
                    });
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
                    let (summary, groups) = symbols(input.bytes, &signatures)?;
                    signatures.extend(groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    selected.push(Named {
                        name: input.name.into(),
                        bytes: input.bytes,
                        groups,
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
