//! Input ordering and demand-driven archive extraction. No filesystem access.
use super::dynamic::Library;
use super::groups::Plan;
use crate::{
    Input, Result,
    elf_selection::{Request, Selection},
};
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
    let requests = inputs
        .iter()
        .map(|input| Request::File {
            name: input.name,
            bytes: input.bytes,
        })
        .collect::<Vec<_>>();
    let (selected, libraries, _) = prepare_requests(&requests, entry)?;
    Ok((selected, libraries))
}

impl<'a> Selection<'a> {
    /// Select objects using lazy archives, whole-archive controls, and nested groups.
    ///
    /// Each archive occurrence extracts a member at most once. Group rescans
    /// continue only when a pass adds an object or shared library, bounding the
    /// fixed point by the input and archive-member counts. COMDAT signatures
    /// belong only to selected objects. Undefined weak references do not cause
    /// extraction. Every group must have matching start and end controls.
    ///
    /// # Errors
    /// Returns an error for unbalanced groups, malformed inputs, thin archives,
    /// or conflicting shared-library interfaces. This does not perform ET_REL
    /// output or resolve final link errors such as duplicate strong definitions.
    pub fn prepare(requests: &[Request<'a>], entry: &str) -> Result<Self> {
        let (_, _, inputs) = prepare_requests(requests, entry)?;
        Ok(Self { inputs })
    }
}

type Prepared<'a> = (Vec<Named<'a>>, Vec<Library<'a>>, Vec<(String, &'a [u8])>);

fn prepare_requests<'a>(requests: &[Request<'a>], entry: &str) -> Result<Prepared<'a>> {
    let mut depth = 0usize;
    let mut whole = false;
    let mut modes = Vec::with_capacity(requests.len());
    for request in requests {
        match request {
            Request::StartGroup => depth += 1,
            Request::EndGroup => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("--end-group without --start-group")?;
            }
            Request::WholeArchive => whole = true,
            Request::NoWholeArchive => whole = false,
            Request::File { .. } => {}
        }
        modes.push(whole);
    }
    if depth != 0 {
        return Err("--start-group without --end-group".into());
    }
    let mut concrete = Vec::new();
    let mut visited = vec![false; requests.len()];
    let mut remaining = (0..requests.len()).map(|_| None).collect::<Vec<_>>();
    let mut groups = Vec::new();
    let mut position = 0;
    let mut selected = Vec::new();
    let mut signatures = BTreeSet::new();
    let mut libraries = Vec::new();
    let mut defined = BTreeMap::<&str, u8>::new();
    let mut required = BTreeSet::from([entry]);
    while position < requests.len() {
        let index = position;
        position += 1;
        let (name, bytes) = match requests[index] {
            Request::StartGroup => {
                groups.push((index, concrete.len()));
                continue;
            }
            Request::EndGroup => {
                let (start, before) = groups.pop().expect("group balance was validated");
                if concrete.len() != before {
                    position = start;
                }
                continue;
            }
            Request::WholeArchive | Request::NoWholeArchive => continue,
            Request::File { name, bytes } => (name, bytes),
        };
        let input = Input { name, bytes };
        let whole_archive = modes[index];
        let result = (|| -> Result<()> {
            if input.bytes.starts_with(b"!<arch>\n") || input.bytes.starts_with(b"!<thin>\n") {
                if remaining[index].is_none() {
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
                    remaining[index] = Some(members);
                }
                let members = remaining[index].as_mut().expect("archive was parsed");
                loop {
                    // Recompute against selected signatures: an unextracted
                    // member must never reserve a COMDAT signature.
                    let mut candidate = None;
                    for (index, (name, bytes)) in members.iter().enumerate() {
                        let (summary, groups) =
                            symbols(bytes, &signatures).map_err(|e| format!("{name}: {e}"))?;
                        if whole_archive
                            || summary.defined.iter().any(|(n, rank)| {
                                (required.contains(n) && !defined.contains_key(n))
                                    || (defined.get(n) == Some(&2) && *rank == 3)
                            })
                        {
                            candidate = Some((index, summary, groups));
                            break;
                        }
                    }
                    let Some((index, summary, groups)) = candidate else {
                        break;
                    };
                    let (name, bytes) = members.remove(index);
                    concrete.push((name.clone(), bytes));
                    signatures.extend(groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    selected.push(Named {
                        name,
                        bytes,
                        groups,
                    });
                }
            } else {
                if visited[index] {
                    return Ok(());
                }
                visited[index] = true;
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
                    concrete.push((input.name.into(), input.bytes));
                } else {
                    let (summary, groups) = symbols(input.bytes, &signatures)?;
                    signatures.extend(groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    concrete.push((input.name.into(), input.bytes));
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
    Ok((selected, libraries, concrete))
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
