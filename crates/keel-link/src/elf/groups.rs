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
impl Plan {
    pub fn new(file: &File<'_>, selected: &BTreeSet<String>) -> Result<Self> {
        let mut plan = Self::default();
        let mut membership = BTreeMap::new();
        for section in file.sections() {
            let raw = section.elf_section_header();
            if raw.sh_type(file.endian()) != object::elf::SHT_GROUP {
                continue;
            }
            plan.metadata.insert(section.index().0);
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
            plan.signature_symbols.insert(index);
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
            let keep = !selected.contains(name) && plan.signatures.insert(name.to_owned());
            for word in data[4..].chunks_exact(4) {
                let member = u32::from_le_bytes(word.try_into().unwrap()) as usize;
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
                if !keep {
                    plan.discarded.insert(member);
                }
            }
        }
        plan.membership = membership;
        for section in file.sections() {
            let raw = section.elf_section_header();
            let group = plan.membership.get(&section.index().0);
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
                if group != plan.membership.get(&target) {
                    return Err(
                        "ELF group relocation table and target must belong to the same group"
                            .into(),
                    );
                }
            }
        }
        if !plan.discarded.is_empty() {
            for section in file.sections() {
                if section.name().map_err(|e| e.to_string())? != ".eh_frame"
                    || plan.discarded.contains(&section.index().0)
                {
                    continue;
                }
                let filtered = super::unwind::filter(&section, file, &plan.discarded)?;
                if let Some(filtered) = filtered {
                    plan.frames.insert(section.index().0, filtered);
                }
            }
        }
        for section in file.sections() {
            let allocated = section.elf_section_header().sh_flags(file.endian())
                & u64::from(object::elf::SHF_ALLOC)
                != 0;
            for (offset, r) in section.relocations() {
                if matches!(r.flags(), object::RelocationFlags::Elf { r_type: 0 }) {
                    continue;
                }
                let RelocationTarget::Symbol(i) = r.target() else {
                    continue;
                };
                if allocated
                    && !plan.discarded.contains(&section.index().0)
                    && section.name().map_err(|e| e.to_string())? == ".sframe"
                    && matches!(file.symbol_by_index(i).map_err(|e| e.to_string())?.section(), SymbolSection::Section(i) if plan.discarded.contains(&i.0))
                {
                    return Err(".sframe: references discarded COMDAT code; COMDAT filtering is unsupported".into());
                }
                if allocated {
                    plan.referenced.insert(i.0);
                }
                if allocated
                    && !plan.discarded.contains(&section.index().0)
                    && plan.offset(section.index().0, offset).is_some()
                {
                    plan.live.insert(i.0);
                }
            }
        }
        // Rewriting unwind contributions changes record offsets. References
        // into those contributions require a richer mapping contract; diagnose
        // rather than silently retargeting an address/addend.
        for s in file.symbols() {
            if matches!(s.section(), SymbolSection::Section(i) if plan.frames.contains_key(&i.0))
                && (s.is_global() || s.is_weak())
            {
                return Err("global symbol in COMDAT-filtered .eh_frame is unsupported".into());
            }
            if matches!(s.section(), SymbolSection::Section(i) if plan.frames.contains_key(&i.0))
                && plan.live.contains(&s.index().0)
            {
                return Err("reference into COMDAT-filtered .eh_frame is unsupported".into());
            }
        }
        Ok(plan)
    }
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
