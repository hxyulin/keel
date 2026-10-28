//! Input ordering and demand-driven archive extraction. No filesystem access.
use super::dynamic::Library;
use super::groups::{Metadata, Plan};
use crate::{
    Input, Result,
    elf_selection::{Request, Selection},
};
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::Object;

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
fn symbols<'a>(metadata: &Metadata<'a>, plan: &Plan) -> Result<Symbols<'a>> {
    let mut out = Symbols::default();
    for symbol in &metadata.symbols {
        if plan.dead_symbol(symbol.index, symbol.section) {
            if plan.live.contains(&symbol.index) && symbol.global && !symbol.weak {
                out.required.insert(
                    symbol
                        .name
                        .as_ref()
                        .map_err(Clone::clone)?
                        .expect("global symbol name was decoded"),
                );
            }
            continue;
        }
        let Some(name) = *symbol.name.as_ref().map_err(Clone::clone)? else {
            continue;
        };
        if symbol.section == object::SymbolSection::Undefined {
            if !symbol.weak && !super::linker_owned(name) {
                out.required.insert(name);
            }
        } else {
            out.defined.insert(
                name,
                if symbol.weak {
                    1
                } else if symbol.common {
                    2
                } else {
                    3
                },
            );
        }
    }
    Ok(out)
}

struct Member<'a> {
    name: String,
    bytes: &'a [u8],
    metadata: Option<Metadata<'a>>,
    selection: Option<MemberSelection<'a>>,
    #[cfg(test)]
    preparations: usize,
}
struct MemberSelection<'a> {
    conflicts: Vec<bool>,
    symbols: Symbols<'a>,
    groups: Plan,
}
impl<'a> Member<'a> {
    fn new(name: String, bytes: &'a [u8]) -> Self {
        Self {
            name,
            bytes,
            metadata: None,
            selection: None,
            #[cfg(test)]
            preparations: 0,
        }
    }
    fn prepare(&mut self, signatures: &BTreeSet<String>) -> Result<&MemberSelection<'a>> {
        if self.metadata.is_none() {
            self.metadata = Some(Metadata::new(self.bytes)?);
        }
        let metadata = self.metadata.as_ref().expect("member metadata was decoded");
        if self
            .selection
            .as_ref()
            .is_none_or(|selection| !metadata.selection_matches(&selection.conflicts, signatures))
        {
            #[cfg(test)]
            {
                self.preparations += 1;
            }
            let conflicts = metadata.conflicts(signatures);
            let groups = metadata.select(&conflicts)?;
            let symbols = symbols(metadata, &groups)?;
            self.selection = Some(MemberSelection {
                conflicts,
                symbols,
                groups,
            });
        }
        Ok(self
            .selection
            .as_ref()
            .expect("member selection was prepared"))
    }
    fn select(self) -> (Named<'a>, Symbols<'a>) {
        let selection = self.selection.expect("selected member was prepared");
        (
            Named {
                name: self.name,
                bytes: self.bytes,
                groups: selection.groups,
            },
            selection.symbols,
        )
    }
}
/// Normal left-to-right archives, rescanned to a fixed point within each
/// archive. Undefined weak references do not cause extraction. Thin archives
/// would require filesystem access and are deliberately rejected.
pub(super) fn prepare<'a>(inputs: &[Input<'a>], entry: &str) -> Result<super::Prepared<'a>> {
    let requests = inputs
        .iter()
        .map(|input| Request::File {
            name: input.name,
            bytes: input.bytes,
        })
        .collect::<Vec<_>>();
    let (selected, libraries, _) = prepare_requests(&requests, entry)?;
    Ok(super::Prepared {
        inputs: selected,
        libraries,
    })
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
        let (objects, libraries, inputs) = prepare_requests(requests, entry)?;
        Ok(Self {
            inputs,
            prepared: super::Prepared {
                inputs: objects,
                libraries,
            },
            entry: entry.into(),
        })
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
                        members.push(Some(Member::new(name, bytes)));
                    }
                    remaining[index] = Some(members);
                }
                let members = remaining[index].as_mut().expect("archive was parsed");
                loop {
                    // Stable slots preserve earliest-eligible archive order. Only
                    // selection-dependent plans change when a relevant signature wins.
                    let mut candidate = None;
                    for (index, member) in members.iter_mut().enumerate() {
                        let Some(member) = member else {
                            continue;
                        };
                        if let Err(error) = member.prepare(&signatures) {
                            return Err(format!("{}: {error}", member.name));
                        }
                        let summary = member
                            .selection
                            .as_ref()
                            .expect("member selection was prepared");
                        if whole_archive
                            || summary.symbols.defined.iter().any(|(n, rank)| {
                                (required.contains(n) && !defined.contains_key(n))
                                    || (defined.get(n) == Some(&2) && *rank == 3)
                            })
                        {
                            candidate = Some(index);
                            break;
                        }
                    }
                    let Some(index) = candidate else {
                        break;
                    };
                    let member = members[index]
                        .take()
                        .expect("candidate occupies its member slot");
                    let (member, summary) = member.select();
                    concrete.push((member.name.clone(), member.bytes));
                    signatures.extend(member.groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    selected.push(member);
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
                    let mut member = Member::new(input.name.into(), input.bytes);
                    member.prepare(&signatures)?;
                    let (member, summary) = member.select();
                    signatures.extend(member.groups.signatures.iter().cloned());
                    absorb(summary, &mut defined, &mut required);
                    concrete.push((input.name.into(), input.bytes));
                    selected.push(member);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_member_reuses_metadata_until_its_comdat_selection_changes() {
        let bytes = keel_asm::x86::assemble_object(
            ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice: ret\n",
        )
        .unwrap();
        let mut member = Member::new("choice.o".into(), &bytes);
        let mut selected = BTreeSet::new();
        member.prepare(&selected).unwrap();
        assert_eq!(member.preparations, 1);
        assert!(
            member
                .selection
                .as_ref()
                .unwrap()
                .symbols
                .defined
                .contains_key("choice")
        );
        assert!(
            selected.is_empty(),
            "unselected members do not reserve signatures"
        );
        selected.insert("unrelated".into());
        member.prepare(&selected).unwrap();
        member.prepare(&selected).unwrap();
        assert_eq!(
            member.preparations, 1,
            "unrelated selections retain the cached plan"
        );
        selected.insert("key".into());
        member.prepare(&selected).unwrap();
        assert_eq!(member.preparations, 2);
        assert!(
            !member
                .selection
                .as_ref()
                .unwrap()
                .symbols
                .defined
                .contains_key("choice")
        );
        member.prepare(&selected).unwrap();
        assert_eq!(member.preparations, 2);
    }
}
