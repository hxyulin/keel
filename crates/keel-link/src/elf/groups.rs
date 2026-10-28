//! ELF group selection, shared by archive demand and the image reader.
//! Signatures name groups independently of symbol binding or resolution.
use crate::Result;
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::read::elf::{ElfFile64, SectionHeader};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SymbolSection};

type File<'a> = ElfFile64<'a, object::Endianness>;
#[derive(Default)]
pub(super) struct Plan {
    pub membership: BTreeMap<usize, usize>,
    pub discarded: BTreeSet<usize>,
    pub signatures: BTreeSet<String>,
    signature_symbols: BTreeSet<usize>,
    /// GAS and Clang may define a signature in its SHT_GROUP section. These
    /// sections are linker metadata, never providers of program symbols.
    metadata: BTreeSet<usize>,
    pub frames: BTreeMap<usize, super::unwind::Filtered>,
    /// Symbols referenced by retained allocated sections, and by any allocated section.
    pub live: BTreeSet<usize>,
    pub referenced: BTreeSet<usize>,
}
/// Immutable input metadata. Selection never reserves signatures here.
pub(super) struct Metadata<'a> {
    file: File<'a>,
    membership: BTreeMap<usize, usize>,
    groups: Vec<Group<'a>>,
    signature_symbols: BTreeSet<usize>,
    metadata: BTreeSet<usize>,
    references: Vec<Reference>,
    section_names: Vec<Option<Result<&'a str>>>,
    pub symbols: Vec<Symbol<'a>>,
}
struct Group<'a> {
    name: &'a str,
    members: Vec<usize>,
}
struct Reference {
    section: usize,
    offset: u64,
    symbol: usize,
    target_section: SymbolSection,
}
pub(super) struct Symbol<'a> {
    pub index: usize,
    pub section: SymbolSection,
    pub name: Result<Option<&'a str>>,
    pub global: bool,
    pub weak: bool,
    pub common: bool,
}
impl<'a> Metadata<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let file = File::parse(bytes).map_err(|e| e.to_string())?;
        let mut signature_symbols = BTreeSet::new();
        let mut metadata = BTreeSet::new();
        let mut groups = Vec::new();
        let mut membership = BTreeMap::new();
        for section in file.sections() {
            let raw = section.elf_section_header();
            if raw.sh_type(file.endian()) != object::elf::SHT_GROUP {
                continue;
            }
            metadata.insert(section.index().0);
            let malformed = || format!("malformed ELF group section {}", section.index().0);
            let data = section.data().map_err(|e| e.to_string())?;
            if raw.sh_flags(file.endian()) != 0
                || raw.sh_entsize(file.endian()) != 4
                || section.align() != 4
                || data.len() < 8
                || data.len() % 4 != 0
                || raw.sh_link(file.endian()) as usize != file.elf_symbol_table().section().0
            {
                return Err(malformed());
            }
            let flags = u32::from_le_bytes(data[..4].try_into().unwrap());
            if flags != object::elf::GRP_COMDAT {
                return Err(format!(
                    "unsupported ELF group flags {flags:#x}: expected GRP_COMDAT"
                ));
            }
            let index = raw.sh_info(file.endian()) as usize;
            if index == 0 {
                return Err(malformed());
            }
            signature_symbols.insert(index);
            let signature = file
                .symbol_by_index(object::SymbolIndex(index))
                .map_err(|_| malformed())?;
            let mut name = signature.name().map_err(|e| e.to_string())?;
            // GNU as also uses unnamed STT_SECTION signatures: their section
            // names are the signature (GNU ld and LLD convention).
            if name.is_empty() && signature.kind() == object::SymbolKind::Section {
                let SymbolSection::Section(i) = signature.section() else {
                    return Err(malformed());
                };
                name = file
                    .section_by_index(i)
                    .map_err(|_| malformed())?
                    .name()
                    .map_err(|e| e.to_string())?;
            }
            if name.is_empty() {
                return Err("empty ELF COMDAT signature is unsupported".into());
            }
            let mut members = Vec::new();
            for word in data[4..].as_chunks::<4>().0 {
                let member = u32::from_le_bytes(*word) as usize;
                let member_section = file
                    .section_by_index(object::SectionIndex(member))
                    .map_err(|_| malformed())?;
                // Even discarded payloads must have valid file bounds.
                let member_data = member_section.data().map_err(|e| e.to_string())?;
                let header = member_section.elf_section_header();
                if member <= section.index().0
                    || header.sh_flags(file.endian()) & u64::from(object::elf::SHF_GROUP) == 0
                    || membership.insert(member, section.index().0).is_some()
                {
                    return Err(malformed());
                }
                match header.sh_type(file.endian()) {
                    object::elf::SHT_PROGBITS
                    | object::elf::SHT_NOBITS
                    | object::elf::SHT_INIT_ARRAY
                    | object::elf::SHT_FINI_ARRAY
                    | object::elf::SHT_PREINIT_ARRAY
                    | object::elf::SHT_X86_64_UNWIND => {}
                    object::elf::SHT_RELA => {
                        if header.sh_entsize(file.endian()) != 24
                            || !member_data.len().is_multiple_of(24)
                        {
                            return Err("malformed ELF group relocation table".into());
                        }
                    }
                    kind => return Err(format!("unsupported ELF group member type {kind:#x}")),
                }
                // Special metadata has additional whole-object contracts. Do
                // not let selection silently weaken any of those contracts.
                let name = member_section.name().map_err(|e| e.to_string())?;
                if name == ".sframe" || name.starts_with(".note") {
                    return Err(format!("ELF group member {name} is unsupported"));
                }
                members.push(member);
            }
            groups.push(Group { name, members });
        }
        for section in file.sections() {
            let raw = section.elf_section_header();
            let group = membership.get(&section.index().0);
            if raw.sh_flags(file.endian()) & u64::from(object::elf::SHF_GROUP) != 0
                && group.is_none()
            {
                return Err(format!("orphan SHF_GROUP section {}", section.index().0));
            }
            if matches!(
                raw.sh_type(file.endian()),
                object::elf::SHT_REL | object::elf::SHT_RELA
            ) {
                let target = raw.sh_info(file.endian()) as usize;
                // RELA tables must accompany their target atomically, including
                // tables that have incorrectly omitted SHF_GROUP.
                if group != membership.get(&target) {
                    return Err(
                        "ELF group relocation table and target must belong to the same group"
                            .into(),
                    );
                }
            }
        }
        let mut references = Vec::new();
        let mut section_names = Vec::new();
        for section in file.sections() {
            let name = section.name().map_err(|e| e.to_string());
            section_names.resize_with(section.index().0 + 1, || None);
            section_names[section.index().0] = Some(name.clone());
            if section.elf_section_header().sh_flags(file.endian())
                & u64::from(object::elf::SHF_ALLOC)
                == 0
            {
                continue;
            }
            for (offset, relocation) in section.relocations() {
                if matches!(
                    relocation.flags(),
                    object::RelocationFlags::Elf { r_type: 0 }
                ) {
                    continue;
                }
                let RelocationTarget::Symbol(symbol) = relocation.target() else {
                    continue;
                };
                references.push(Reference {
                    section: section.index().0,
                    offset,
                    symbol: symbol.0,
                    target_section: if name.as_ref().is_ok_and(|name| *name == ".sframe") {
                        file.symbol_by_index(symbol)
                            .map_err(|e| e.to_string())?
                            .section()
                    } else {
                        SymbolSection::None
                    },
                });
            }
        }
        let mut symbols = Vec::new();
        for symbol in file.symbols() {
            let global = symbol.is_global();
            let weak = symbol.is_weak();
            symbols.push(Symbol {
                index: symbol.index().0,
                section: symbol.section(),
                // Local symbol names do not participate in archive demand.
                name: if global || weak {
                    symbol.name().map(Some).map_err(|e| e.to_string())
                } else {
                    Ok(None)
                },
                global,
                weak,
                common: symbol.is_common(),
            });
        }
        Ok(Self {
            file,
            membership,
            groups,
            signature_symbols,
            metadata,
            references,
            section_names,
            symbols,
        })
    }

    pub fn conflicts(&self, selected: &BTreeSet<String>) -> Vec<bool> {
        self.groups
            .iter()
            .map(|group| selected.contains(group.name))
            .collect()
    }

    pub fn selection_matches(&self, conflicts: &[bool], selected: &BTreeSet<String>) -> bool {
        self.groups
            .iter()
            .zip(conflicts)
            .all(|(group, &discard)| selected.contains(group.name) == discard)
    }

    pub fn select(&self, conflicts: &[bool]) -> Result<Plan> {
        let mut plan = Plan {
            membership: self.membership.clone(),
            signature_symbols: self.signature_symbols.clone(),
            metadata: self.metadata.clone(),
            ..Plan::default()
        };
        for (group, &conflict) in self.groups.iter().zip(conflicts) {
            let keep = !conflict && plan.signatures.insert(group.name.to_owned());
            if !keep {
                plan.discarded.extend(group.members.iter().copied());
            }
        }
        if !plan.discarded.is_empty() {
            for (index, name) in self.section_names.iter().enumerate() {
                let Some(name) = name else {
                    continue;
                };
                if *name.as_ref().map_err(Clone::clone)? != ".eh_frame"
                    || plan.discarded.contains(&index)
                {
                    continue;
                }
                let section = self
                    .file
                    .section_by_index(object::SectionIndex(index))
                    .map_err(|e| e.to_string())?;
                if let Some(filtered) =
                    super::unwind::filter(&section, &self.file, &plan.discarded)?
                {
                    plan.frames.insert(index, filtered);
                }
            }
        }
        for reference in &self.references {
            if !plan.discarded.contains(&reference.section)
                && *self.section_names[reference.section]
                    .as_ref()
                    .expect("section name was decoded")
                    .as_ref()
                    .map_err(Clone::clone)?
                    == ".sframe"
                && matches!(reference.target_section, SymbolSection::Section(i) if plan.discarded.contains(&i.0))
            {
                return Err(
                    ".sframe: references discarded COMDAT code; COMDAT filtering is unsupported"
                        .into(),
                );
            }
            plan.referenced.insert(reference.symbol);
            if !plan.discarded.contains(&reference.section)
                && plan.offset(reference.section, reference.offset).is_some()
            {
                plan.live.insert(reference.symbol);
            }
        }
        // Filtered unwind offsets must not silently retarget references.
        for symbol in &self.symbols {
            if matches!(symbol.section, SymbolSection::Section(i) if plan.frames.contains_key(&i.0))
            {
                if symbol.global || symbol.weak {
                    return Err("global symbol in COMDAT-filtered .eh_frame is unsupported".into());
                }
                if plan.live.contains(&symbol.index) {
                    return Err("reference into COMDAT-filtered .eh_frame is unsupported".into());
                }
            }
        }
        Ok(plan)
    }
}
impl Plan {
    pub fn offset(&self, section: usize, offset: u64) -> Option<u64> {
        self.frames
            .get(&section)
            .map_or(Some(offset), |f| f.offset(offset))
    }
    pub fn dead_symbol(&self, index: usize, section: SymbolSection) -> bool {
        match section {
            SymbolSection::Undefined => {
                (self.signature_symbols.contains(&index) || self.referenced.contains(&index))
                    && !self.live.contains(&index)
            }
            SymbolSection::Section(i) => {
                self.discarded.contains(&i.0) || self.metadata.contains(&i.0)
            }
            _ => false,
        }
    }
}
