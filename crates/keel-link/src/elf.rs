//! Host-independent ELF64 executable linker for x86-64 and AArch64 Linux.
//! See docs/linking/elf.md for the supported input and execution contract.
pub(crate) mod aarch64;
mod bounds;
mod dynamic;
mod gc;
mod groups;
mod input;
mod lifecycle;
mod notes;
pub mod relocatable;
mod script;
mod selected;
mod sframe;
mod unwind;
use crate::{Input, Result};
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use object::read::elf::{ElfFile64, FileHeader, SectionHeader};
use object::{
    Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget, SectionFlags,
    SymbolFlags, SymbolSection,
};

const BASE: u64 = 0x400000;
const PAGE: u64 = 4096;
const LIMIT: u64 = 512 * 1024 * 1024;

fn linker_owned(name: &str) -> bool {
    matches!(name, "_GLOBAL_OFFSET_TABLE_" | "_DYNAMIC")
}

/// ELF machine selected independently of the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Machine {
    #[default]
    X86_64,
    Aarch64,
}

pub struct Options {
    /// Collect unreachable ELF input sections. Disabled by default.
    pub gc_sections: bool,
    /// Owned GNU linker script text for bounded static x86-64 final linking.
    pub script: Option<String>,
    /// ELF LOAD segment alignment override; defaults to the target page size.
    pub max_page_size: Option<u64>,
    pub machine: Machine,
    pub entry: String,
    /// Explicit entry override, taking precedence over a script ENTRY command.
    pub entry_override: Option<String>,
    pub interpreter: String,
    pub runpath: Vec<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            gc_sections: false,
            script: None,
            max_page_size: None,
            machine: Machine::X86_64,
            entry: "_start".into(),
            entry_override: None,
            interpreter: "/lib64/ld-linux-x86-64.so.2".into(),
            runpath: Vec::new(),
        }
    }
}
impl Options {
    /// Return the entry used for archive selection and executable layout.
    ///
    /// # Errors
    /// Returns an error for an invalid script or an empty/reserved entry name.
    pub fn effective_entry(&self) -> Result<String> {
        let script = self
            .script
            .as_deref()
            .map(script::Script::parse)
            .transpose()?;
        let entry = self
            .entry_override
            .as_ref()
            .or_else(|| script.as_ref().and_then(|s| s.entry.as_ref()))
            .unwrap_or(&self.entry)
            .clone();
        validate_entry(&entry)?;
        Ok(entry)
    }
}
fn validate_entry(entry: &str) -> Result<()> {
    if entry.is_empty() || entry.contains('\0') {
        return Err("invalid entry symbol".into());
    }
    if linker_owned(entry) {
        return Err("a linker-owned symbol cannot be the entry point".into());
    }
    Ok(())
}
#[derive(Debug)]
pub struct Executable {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}
struct Section {
    name: String,
    flags: u64,
    kind: u32,
    align: u64,
    size: u64,
    bss: bool,
    data: Vec<u8>,
    offset: u64,
}
impl Section {
    fn permissions(&self) -> u32 {
        4 | if self.flags & 1 != 0 {
            2
        } else if self.flags & 4 != 0 {
            1
        } else {
            0
        }
    }
}
#[derive(Clone)]
struct Symbol {
    name: String,
    section: SymbolSection,
    value: u64,
    size: u64,
    info: u8,
    other: u8,
}
impl Symbol {
    fn global(&self) -> bool {
        self.info >> 4 != 0
    }
    fn weak(&self) -> bool {
        self.info >> 4 == 2
    }
    fn rank(&self) -> u8 {
        if self.section == SymbolSection::Undefined {
            0
        } else if self.weak() {
            1
        } else if self.section == SymbolSection::Common {
            2
        } else {
            3
        }
    }
}
struct Reloc {
    section: usize,
    offset: u64,
    symbol: usize,
    kind: u32,
    addend: i64,
}
struct Unit {
    name: String,
    sections: BTreeMap<usize, usize>,
    symbols: BTreeMap<usize, Symbol>,
    relocs: Vec<Reloc>,
    consumed_properties: BTreeSet<usize>,
    groups: BTreeMap<usize, usize>,
}
struct Segment {
    flags: u32,
    offset: u64,
    file_end: u64,
    memory_end: u64,
}
fn align(n: u64, a: u64) -> Result<u64> {
    if !a.is_power_of_two() || a > LIMIT {
        return Err(format!("unsupported alignment {a}"));
    }
    n.checked_add(a - 1)
        .map(|v| v & !(a - 1))
        .filter(|v| *v <= LIMIT)
        .ok_or("ELF image exceeds 512 MiB limit".into())
}
fn end(n: u64, size: u64) -> Result<u64> {
    n.checked_add(size)
        .filter(|v| *v <= LIMIT)
        .ok_or("ELF image exceeds 512 MiB limit".into())
}

pub fn link(inputs: &[Input<'_>], mut options: Options) -> Result<Executable> {
    if inputs.is_empty() {
        return Err("no input objects".into());
    }
    let script = options
        .script
        .as_deref()
        .map(script::Script::parse)
        .transpose()?;
    if let Some(entry) = options
        .entry_override
        .as_ref()
        .or_else(|| script.as_ref().and_then(|s| s.entry.as_ref()))
    {
        options.entry.clone_from(entry);
    }
    validate_entry(&options.entry)?;
    let arm = options.machine == Machine::Aarch64;
    let page = options
        .max_page_size
        .unwrap_or(if arm { 65536 } else { PAGE });
    if !page.is_power_of_two() || !(PAGE..=LIMIT).contains(&page) {
        return Err(format!(
            "invalid ELF maximum page size {page}: expected a power of two between 4096 and {LIMIT}"
        ));
    }
    if script.is_none() && !BASE.is_multiple_of(page) {
        return Err("maximum page size is incompatible with the default ELF base address".into());
    }
    let (inputs, libraries) = input::prepare(inputs, &options.entry)?;
    if arm && (!libraries.is_empty() || !options.runpath.is_empty()) {
        return Err("native AArch64 ELF linking currently supports freestanding static executables; use `keel link --system` for shared libraries/libc".into());
    }
    if script.is_some() && (arm || !libraries.is_empty() || options.gc_sections) {
        return Err("scripts currently require static x86-64 output without --gc-sections".into());
    }
    let mut sections = Vec::new();
    let mut units = Vec::new();
    let mut unwind_sections = BTreeMap::new();
    let mut sframe_section = None;
    let mut warnings = Vec::new();
    let mut note_plan = notes::Plan::default();
    let mut object_properties = Vec::new();
    for input in &inputs {
        let mut properties = notes::Properties::new();
        let mut parse = || -> Result<Unit> {
            let file =
                ElfFile64::<object::Endianness>::parse(input.bytes).map_err(|e| e.to_string())?;
            if file.architecture()
                != if arm {
                    object::Architecture::Aarch64
                } else {
                    object::Architecture::X86_64
                }
                || !file.is_64()
                || !file.is_little_endian()
                || file.kind() != object::ObjectKind::Relocatable
            {
                return Err(format!(
                    "expected a little-endian {} ELF64 relocatable object",
                    if arm { "AArch64" } else { "x86-64" }
                ));
            }
            let header = file.elf_header();
            if !matches!(header.e_ident().os_abi, 0 | 3) || header.e_flags(file.endian()) != 0 {
                return Err("unsupported ELF OS ABI or machine flags".into());
            }
            let group_plan = &input.groups;
            if let Some(symbol) = file.symbols().find(|s| {
                matches!(s.flags(),
                SymbolFlags::Elf { st_info, .. } if st_info & 15 == object::elf::STT_TLS)
            }) {
                return Err(format!(
                    "TLS symbol {} requires external ELF linking: native PT_TLS layout and TLS relocations are unsupported",
                    symbol.name().unwrap_or("?")
                ));
            }
            // object ignores REL/RELA tables not linked to its chosen symbol
            // table (and sh_info=0). Validate every association before consuming
            // notes or trusting its per-section relocation iterators.
            for section in file.sections() {
                let raw = section.elf_section_header();
                if !matches!(
                    raw.sh_type(file.endian()),
                    object::elf::SHT_REL | object::elf::SHT_RELA
                ) {
                    continue;
                }
                let name = section.name().map_err(|e| e.to_string())?;
                let link = raw.sh_link(file.endian());
                if link == 0 || link as usize != file.elf_symbol_table().section().0 {
                    return Err(format!(
                        "unsupported relocation table {name} sh_link {link}: expected the selected SHT_SYMTAB"
                    ));
                }
                let info = raw.sh_info(file.endian());
                let target = file
                    .section_by_index(object::SectionIndex(info as usize))
                    .map_err(|_| format!("invalid relocation table {name} sh_info {info}"))?;
                if info == 0
                    || matches!(
                        target.elf_section_header().sh_type(file.endian()),
                        object::elf::SHT_REL | object::elf::SHT_RELA
                    )
                {
                    return Err(format!(
                        "unsupported relocation table {name} sh_info {info}"
                    ));
                }
            }
            let mut unit = Unit {
                name: input.name.clone(),
                sections: BTreeMap::new(),
                symbols: BTreeMap::new(),
                relocs: Vec::new(),
                consumed_properties: BTreeSet::new(),
                groups: group_plan.membership.clone(),
            };
            for section in file.sections() {
                if group_plan.discarded.contains(&section.index().0) {
                    // TLS is a whole-input unsupported contract, including
                    // losing groups without STT_TLS symbols. Keep ordinary
                    // live-section validation (notably unwind) in its original
                    // order, but do not let selection hide this boundary.
                    let flags = section.elf_section_header().sh_flags(file.endian());
                    if flags & u64::from(object::elf::SHF_ALLOC | object::elf::SHF_TLS)
                        == u64::from(object::elf::SHF_ALLOC | object::elf::SHF_TLS)
                    {
                        let name = section.name().map_err(|e| e.to_string())?;
                        return Err(format!(
                            "TLS section {name} requires external ELF linking: native PT_TLS layout and TLS relocations are unsupported"
                        ));
                    }
                    continue;
                }
                let name = section.name().map_err(|e| e.to_string())?.to_owned();
                let original_data = section.data().map_err(|e| e.to_string())?;
                let data = group_plan
                    .frames
                    .get(&section.index().0)
                    .map_or(original_data, |f| f.data.as_slice());
                let input_size = if group_plan.frames.contains_key(&section.index().0) {
                    data.len() as u64
                } else {
                    section.size()
                };
                let raw = section.elf_section_header();
                match raw.sh_type(file.endian()) {
                    object::elf::SHT_REL => {
                        return Err("REL relocations are unsupported; use RELA".into());
                    }
                    object::elf::SHT_RELA | object::elf::SHT_SYMTAB
                        if raw.sh_entsize(file.endian()) != 24
                            || !data.len().is_multiple_of(24) =>
                    {
                        return Err(format!("malformed ELF record table {name}"));
                    }
                    _ => {}
                }
                let SectionFlags::Elf { sh_flags } = section.flags() else {
                    unreachable!()
                };
                if name == ".note.GNU-stack" && sh_flags & 4 != 0 {
                    return Err("executable stack requests are unsupported".into());
                }
                let kind = raw.sh_type(file.endian());
                if arm
                    && (name == ".eh_frame_hdr"
                        || name == ".sframe"
                        || name.starts_with(".note.gnu.property")
                        || name == ".note.ABI-tag"
                        || matches!(kind, 14..=16 | 0x70000001))
                {
                    return Err(format!(
                        "AArch64 ELF input section {name} is not yet supported"
                    ));
                }
                let sframe = sframe::validate_section(
                    &name,
                    kind,
                    sh_flags,
                    section.align(),
                    raw.sh_entsize(file.endian()),
                    raw.sh_link(file.endian()),
                    raw.sh_info(file.endian()),
                )?;
                let note = notes::validate(
                    &name,
                    kind,
                    sh_flags,
                    section.align(),
                    raw.sh_entsize(file.endian()),
                    raw.sh_link(file.endian()),
                    raw.sh_info(file.endian()),
                )?;
                let linux_note = name == ".note.Linux";
                if linux_note {
                    notes::linux(data)?;
                    section.elf_linked_rela().map_err(|e| e.to_string())?;
                    if section.relocations().next().is_some() {
                        return Err("relocations into .note.Linux are unsupported".into());
                    }
                }
                if let Some(note) = note {
                    end(0, section.size())?;
                    if data.len() as u64 != section.size() {
                        return Err(format!("invalid section size for {name}"));
                    }
                    section.elf_linked_rela().map_err(|e| e.to_string())?;
                    if section.relocations().next().is_some() {
                        return Err(format!("relocations into {name} are unsupported"));
                    }
                    match note {
                        notes::Kind::Property => {
                            notes::properties(data, &mut properties)?;
                            unit.consumed_properties.insert(section.index().0);
                            continue;
                        }
                        notes::Kind::Abi => {
                            notes::abi(data)?;
                            if note_plan.abi.replace(sections.len()).is_some() {
                                return Err(
                                    "additional .note.ABI-tag contracts are unsupported".into()
                                );
                            }
                        }
                    }
                }
                let lifecycle = lifecycle::validate(
                    &name,
                    kind,
                    sh_flags & !u64::from(object::elf::SHF_GROUP),
                    section.size(),
                    section.align(),
                    raw.sh_entsize(file.endian()),
                )?;
                let frame = name == ".eh_frame" || kind == object::elf::SHT_X86_64_UNWIND;
                if name == ".eh_frame_hdr" {
                    return Err(
                        "input .eh_frame_hdr is unsupported; the linker generates it".into(),
                    );
                }
                if frame
                    && (name != ".eh_frame"
                        || !matches!(
                            kind,
                            object::elf::SHT_PROGBITS | object::elf::SHT_X86_64_UNWIND
                        )
                        || sh_flags & !u64::from(object::elf::SHF_GROUP)
                            != u64::from(object::elf::SHF_ALLOC)
                        || raw.sh_entsize(file.endian()) != 0)
                {
                    return Err(".eh_frame requires PROGBITS or SHT_X86_64_UNWIND, SHF_ALLOC only, and no entry size".into());
                }
                if sh_flags & 2 == 0 {
                    if name.starts_with(".debug") || name.starts_with(".zdebug") {
                        warnings.push(format!("{}: omitted debug section {name}", input.name));
                    }
                    continue;
                }
                if sh_flags & u64::from(object::elf::SHF_TLS) != 0 {
                    return Err(format!(
                        "TLS section {name} requires external ELF linking: native PT_TLS layout and TLS relocations are unsupported"
                    ));
                }
                // SHF_WRITE/ALLOC/EXECINSTR/MERGE/STRINGS/GNU_RETAIN. In particular,
                // TLS, compression and link-order semantics are rejected.
                // Group membership was validated and selected before parsing.
                if sh_flags & !0x200237 != 0 || sh_flags & 5 == 5 {
                    return Err(format!("unsupported flags {sh_flags:#x} on {name}"));
                }
                let bss = section.kind() == object::SectionKind::UninitializedData;
                if !lifecycle
                    && !sframe
                    && note.is_none()
                    && !linux_note
                    && !matches!(
                        section.kind(),
                        object::SectionKind::Text
                            | object::SectionKind::Data
                            | object::SectionKind::ReadOnlyData
                            | object::SectionKind::ReadOnlyString
                            | object::SectionKind::UninitializedData
                            | object::SectionKind::Elf(object::elf::SHT_X86_64_UNWIND)
                    )
                {
                    return Err(format!(
                        "unsupported allocated section {name} ({:?})",
                        section.kind()
                    ));
                }
                let size = end(0, input_size)?;
                let alignment = section.align().max(1);
                align(0, alignment)?;
                if !bss && data.len() as u64 != size {
                    return Err(format!("invalid section size for {name}"));
                }
                if frame {
                    if size != 0 && alignment < 4 {
                        return Err(".eh_frame alignment must be at least 4".into());
                    }
                    let frames =
                        unwind::Frames::parse(data).map_err(|e| format!(".eh_frame: {e}"))?;
                    unwind_sections.insert(sections.len(), frames);
                }
                if sframe {
                    if sframe_section.is_some() {
                        return Err(".sframe: second selected SFrame producer is unsupported; merging is not implemented (empty headers also count)".into());
                    }
                    sframe_section = Some((sections.len(), sframe::Table::parse(data)?));
                }
                unit.sections.insert(section.index().0, sections.len());
                sections.push(Section {
                    name,
                    flags: sh_flags & 0x200007,
                    kind,
                    align: alignment,
                    size,
                    bss,
                    data: if sframe {
                        sframe_section.as_ref().unwrap().1.rebuild()?
                    } else {
                        data.to_vec()
                    },
                    offset: 0,
                });
                // The generic relocation iterator skips malformed tables. Validate
                // explicitly before consuming it so bad inputs cannot mislink.
                section.elf_linked_rela().map_err(|e| e.to_string())?;
                let mut unwind_relocations = BTreeSet::new();
                let mut sframe_relocations = BTreeSet::new();
                for (offset, r) in section.relocations() {
                    let Some(offset) = group_plan.offset(section.index().0, offset) else {
                        continue;
                    };
                    let RelocationFlags::Elf { r_type: kind } = r.flags() else {
                        return Err("non-ELF relocation".into());
                    };
                    if kind == 0 {
                        continue;
                    }
                    if r.has_implicit_addend() {
                        return Err("REL relocations are unsupported; use RELA".into());
                    }
                    if sframe {
                        sframe_section.as_ref().unwrap().1.relocation(
                            offset,
                            kind,
                            &mut sframe_relocations,
                        )?;
                    }
                    let width = if arm {
                        aarch64::width(kind)?
                    } else {
                        match kind {
                            15 => 1, // R_X86_64_PC8, used by LOOP/J*CXZ.
                            1 | 24 => 8,
                            2 | 4 | 9 | 10 | 11 | 41 | 42 => 4,
                            _ => return Err(format!("unsupported x86-64 relocation {kind}")),
                        }
                    };
                    if bss || offset.checked_add(width).is_none_or(|e| e > size) {
                        return Err("relocation field is outside file-backed section data".into());
                    }
                    if let Some(frames) = unwind_sections.get(&(sections.len() - 1)) {
                        if !unwind_relocations.insert(offset) {
                            return Err("duplicate .eh_frame pointer relocation".into());
                        }
                        frames
                            .relocation(
                                offset,
                                if arm {
                                    match kind {
                                        257 => 1,
                                        258 => 10,
                                        260 => 24,
                                        261 => 2,
                                        _ => kind,
                                    }
                                } else {
                                    kind
                                },
                            )
                            .map_err(|e| format!(".eh_frame: {e}"))?;
                    }
                    let RelocationTarget::Symbol(symbol) = r.target() else {
                        return Err("unsupported relocation target".into());
                    };
                    unit.relocs.push(Reloc {
                        section: sections.len() - 1,
                        offset,
                        symbol: symbol.0,
                        kind,
                        addend: r.addend(),
                    });
                }
                if sframe {
                    sframe_section
                        .as_ref()
                        .unwrap()
                        .1
                        .require_relocations(&sframe_relocations)?;
                }
            }
            for s in file.symbols() {
                if group_plan.dead_symbol(s.index().0, s.section()) {
                    // Surviving named global references rebind through the
                    // ordinary global resolver. Locals cannot be substituted.
                    if !group_plan.live.contains(&s.index().0) {
                        continue;
                    }
                    if !s.is_global() && !s.is_weak() {
                        return Err(format!(
                            "reference to discarded COMDAT local `{}` is unsupported",
                            s.name().unwrap_or("?")
                        ));
                    }
                }
                let SymbolFlags::Elf {
                    st_info: info,
                    st_other: other,
                } = s.flags()
                else {
                    unreachable!()
                };
                if info >> 4 > 2 || !matches!(info & 15, 0..=4) {
                    return Err(format!(
                        "unsupported symbol binding/type for {}",
                        s.name().unwrap_or("?")
                    ));
                }
                let mut symbol = Symbol {
                    name: s.name().map_err(|e| e.to_string())?.into(),
                    section: if group_plan.dead_symbol(s.index().0, s.section()) {
                        SymbolSection::Undefined
                    } else {
                        s.section()
                    },
                    value: s.address(),
                    size: s.size(),
                    info,
                    other,
                };
                if let SymbolSection::Section(index) = symbol.section {
                    let section = file.section_by_index(index).map_err(|e| e.to_string())?;
                    if symbol
                        .value
                        .checked_add(symbol.size)
                        .is_none_or(|e| e > section.size())
                    {
                        return Err(format!(
                            "symbol {} extends outside its section",
                            symbol.name
                        ));
                    }
                }
                if let SymbolSection::Section(index) = symbol.section
                    && let Some(filtered) = group_plan.frames.get(&index.0)
                {
                    if symbol.info & 15 == 3 {
                        symbol.value = 0;
                        symbol.size = 0;
                    } else if let Some(value) = filtered.offset(symbol.value) {
                        // An addressable symbol must fit within one retained record.
                        if symbol.size != 0
                            && filtered.offset(symbol.value + symbol.size - 1)
                                != Some(value + symbol.size - 1)
                        {
                            return Err("symbol spans discarded .eh_frame records".into());
                        }
                        symbol.value = value;
                    } else {
                        continue;
                    }
                }
                unit.symbols.insert(s.index().0, symbol);
            }
            Ok(unit)
        };
        units.push(parse().map_err(|e| format!("{}: {e}", input.name))?);
        object_properties.push(properties);
    }
    let mut globals: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut commons: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (ui, u) in units.iter().enumerate() {
        for (&si, s) in &u.symbols {
            if !s.global() || s.rank() == 0 {
                continue;
            }
            if linker_owned(&s.name) {
                return Err(format!(
                    "{}: input defines reserved linker-owned symbol `{}`",
                    u.name, s.name
                ));
            }
            if s.section == SymbolSection::Common {
                align(0, s.value.max(1))?;
                let c = commons.entry(s.name.clone()).or_insert((0, 1));
                c.0 = c.0.max(s.size);
                c.1 = c.1.max(s.value.max(1));
            }
            if let Some(&(prev_u, prev_s)) = globals.get(&s.name) {
                let previous = &units[prev_u].symbols[&prev_s];
                if previous.rank() == 3 && s.rank() == 3 {
                    return Err(format!(
                        "{}: duplicate definition `{}` (also in {})",
                        u.name, s.name, units[prev_u].name
                    ));
                }
                if previous.rank() >= s.rank() {
                    continue;
                }
            }
            globals.insert(s.name.clone(), (ui, si));
        }
    }
    if let Some(script) = script {
        if sframe_section.is_some() {
            return Err("SFrame reconstruction with linker scripts is unsupported".into());
        }
        unwind::merge(&mut sections, &mut units, &mut unwind_sections)?;
        note_plan.prepare(&mut sections, &object_properties, false)?;
        return script::link(script, sections, units, globals, &options, warnings);
    }
    // Bind original identities and validate even dead ranges before GC or merge.
    let mut sframe_targets = sframe_section
        .as_ref()
        .map(|(section, table)| {
            table.bind(*section, &units, &sections, &globals, options.gc_sections)
        })
        .transpose()?;
    if options.gc_sections {
        let remap = gc::collect(
            &mut sections,
            &mut units,
            &mut unwind_sections,
            &mut globals,
            &options.entry,
            !libraries.is_empty(),
            &mut sframe_section,
            &mut sframe_targets,
        )?;
        note_plan.abi = note_plan.abi.and_then(|i| remap[i]);
        commons.retain(|name, _| globals.contains_key(name));
    }
    let section_remap = unwind::merge(&mut sections, &mut units, &mut unwind_sections)?;
    note_plan.abi = note_plan.abi.map(|i| section_remap[i]);
    if let Some((section, table)) = &mut sframe_section {
        *section = section_remap[*section];
        sframe_targets
            .as_mut()
            .unwrap()
            .remap(&section_remap.iter().copied().map(Some).collect::<Vec<_>>())?;
        if options.gc_sections {
            let targets = sframe_targets.as_mut().unwrap();
            let indices = table.order(targets, &sections);
            table.reconstruct(targets, indices, *section, &mut sections, &mut units)?;
        }
    }
    let mut common_locations = BTreeMap::new();
    let mut common_sizes = BTreeMap::new();
    let mut common = Section {
        name: ".bss.common".into(),
        flags: 3,
        kind: 8,
        align: 1,
        size: 0,
        bss: true,
        data: Vec::new(),
        offset: 0,
    };
    for (name, (size, alignment)) in commons {
        let (u, s) = globals[&name];
        if units[u].symbols[&s].section != SymbolSection::Common {
            continue;
        }
        let offset = align(common.size, alignment)?;
        common_sizes.insert(name.clone(), size);
        common_locations.insert(name, (sections.len(), offset));
        common.size = end(offset, size)?;
        common.align = common.align.max(alignment);
    }
    sections.push(common);
    let bounds = bounds::symbols(&sections, &units, &globals)?;
    let mut imports: BTreeMap<String, dynamic::Import> = BTreeMap::new();
    for u in &units {
        for s in u.symbols.values() {
            if !s.global() || s.section != SymbolSection::Undefined || globals.contains_key(&s.name)
            {
                continue;
            }
            if bounds.contains_key(s.name.as_str()) {
                continue;
            }
            if linker_owned(&s.name) {
                if s.name == "_DYNAMIC" && libraries.is_empty() && !s.weak() {
                    return Err(format!(
                        "{}: `_DYNAMIC` requires dynamically linked output",
                        u.name
                    ));
                }
                continue;
            }
            let export = libraries
                .iter()
                .enumerate()
                .find_map(|(i, l)| l.exports.get(s.name.as_str()).map(|e| (i, e)));
            if export.is_none() && !s.weak() {
                return Err(format!("{}: undefined symbol `{}`", u.name, s.name));
            }
            if libraries.is_empty() {
                continue;
            }
            if s.other & 3 != 0 {
                return Err(format!(
                    "{}: non-default visibility undefined symbol `{}` cannot be imported",
                    u.name, s.name
                ));
            }
            let (library, kind, version) = match export {
                Some((i, e)) => (Some(i), e.kind, e.version.map(str::to_owned)),
                None => (None, s.info & 15, None),
            };
            if !matches!(kind, 0 | 1 | 2 | 10) {
                return Err(format!(
                    "unsupported shared-library symbol type {kind} for `{}`",
                    s.name
                ));
            }
            imports
                .entry(s.name.clone())
                .and_modify(|i| i.weak &= s.weak())
                .or_insert(dynamic::Import {
                    index: 0,
                    kind,
                    weak: s.weak(),
                    library,
                    version,
                });
        }
    }
    for (i, import) in imports.values_mut().enumerate() {
        import.index = (i + 1) as u32;
    }
    // GOT keys retain selected raw identities, including independent locals.
    // Build after the final symbol/section rewrites; address validation stays lazy.
    let selected = selected::Index::new(&units, &globals, &imports);
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            let selection = selected.get(ui, r.symbol)?;
            let target_unit = selection.key.0;
            let target = selection.definition;
            if matches!(target.section, SymbolSection::Section(i)
                if units[target_unit].consumed_properties.contains(&i.0))
            {
                return Err(format!(
                    "{}: reference into consumed GNU property section for `{}` is unsupported",
                    u.name, target.name
                ));
            }
        }
    }
    // DSO consumers have no Unit.relocs. Check their dynamic lookups,
    // including preemptible defined symbols, against the selected executable
    // definitions used by native references and dynamic exports. A discarded
    // weak property definition or a same-named local is not the provider.
    for library in &libraries {
        for name in &library.references {
            if let Some(&(ui, si)) = globals.get(*name) {
                let target = &units[ui].symbols[&si];
                // Hidden/internal definitions are not executable exports and
                // cannot satisfy a DSO lookup, even with the same name.
                if matches!(target.other & 3, 0 | 3)
                    && matches!(target.section, SymbolSection::Section(i)
                    if units[ui].consumed_properties.contains(&i.0))
                {
                    return Err(format!(
                        "{}: reference into consumed GNU property section for `{name}` is unsupported (provider {})",
                        library.needed, units[ui].name
                    ));
                }
            }
        }
    }
    let mut got = BTreeMap::new();
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            if if arm {
                aarch64::got(r.kind)
            } else {
                matches!(r.kind, 9 | 41 | 42)
            } {
                let selection = selected.get(ui, r.symbol)?;
                if selection.import.is_none() {
                    let next = got.len() as u64 * 8;
                    got.entry(selection.key).or_insert(next);
                }
            }
        }
    }
    let got_section = sections.len();
    let got_size = end(0, got.len() as u64 * 8)?;
    sections.push(Section {
        name: ".got".into(),
        flags: 2,
        kind: 1,
        align: 8,
        size: got_size,
        bss: false,
        data: vec![0; got_size as usize],
        offset: 0,
    });
    let mut dynamic = if libraries.is_empty() {
        None
    } else {
        let mut definitions = Vec::new();
        for (name, &(ui, si)) in &globals {
            let s = &units[ui].symbols[&si];
            if s.other & 3 == 2 || s.other & 3 == 1 {
                continue;
            }
            if matches!(s.section,SymbolSection::Section(i) if !units[ui].sections.contains_key(&i.0))
            {
                continue;
            }
            definitions.push(dynamic::Definition {
                name: name.clone(),
                info: s.info,
                other: s.other,
                size: common_sizes.get(name).copied().unwrap_or(s.size),
            });
        }
        let mut absolute = 0;
        for (ui, u) in units.iter().enumerate() {
            for r in &u.relocs {
                if let Some(name) = selected.get(ui, r.symbol)?.import {
                    match r.kind {
                        1 if sections[r.section].flags & 1 != 0 => absolute += 1,
                        4 => {}
                        9 | 41 | 42 => {}
                        _ => {
                            return Err(format!(
                                "{}: relocation {} against import `{name}` requires writable 64-bit data, a PLT32 call, or GOT addressing; use PIC (copy/text relocations are unsupported)",
                                u.name, r.kind
                            ));
                        }
                    }
                }
            }
        }
        let lifecycle = lifecycle::Plan::new(&sections, &units, &globals)?;
        Some(dynamic::Dynamic::prepare(
            &mut sections,
            imports.clone(),
            definitions,
            &libraries,
            &options,
            absolute,
            lifecycle,
        )?)
    };
    let unwind_header = unwind::prepare(&mut sections, &unwind_sections)?;
    note_plan.prepare(
        &mut sections,
        &object_properties,
        dynamic.as_ref().is_some_and(|d| !d.stubs.is_empty()),
    )?;
    let registration_alignments = bounds::alignments(&sections);
    let mut segments = Vec::new();
    // Header space holds three LOADs and a non-executable GNU_STACK header.
    let mut cursor = page;
    for permissions in [4, 5, 6] {
        let start = if permissions == 4 {
            0
        } else {
            align(cursor, page)?
        };
        cursor = if permissions == 4 { page } else { start };
        let mut file_end = cursor;
        for bss in [false, true] {
            for index in bounds::order(&sections, lifecycle::order(&sections, permissions, bss)) {
                let section = &mut sections[index];
                cursor = align(
                    cursor,
                    registration_alignments
                        .get(&index)
                        .copied()
                        .unwrap_or(section.align),
                )?;
                section.offset = cursor;
                cursor = end(cursor, section.size)?;
                if !bss {
                    file_end = cursor;
                }
            }
        }
        if cursor > start {
            segments.push(Segment {
                flags: permissions,
                offset: start,
                file_end,
                memory_end: cursor,
            });
        }
    }
    let mut linker_symbols = bounds;
    linker_symbols.insert("_GLOBAL_OFFSET_TABLE_".into(), (got_section, false));
    if let Some(d) = &dynamic {
        linker_symbols.insert("_DYNAMIC".into(), (d.table, false));
    }
    // Output symbol tables already traverse selected definitions or locals.
    // Use those symbols directly instead of looking up and selecting them again.
    let definition_address = |ui: usize, s: &Symbol| -> Result<u64> {
        match s.section {
            SymbolSection::Absolute => Ok(s.value),
            SymbolSection::Undefined
                if s.global() && linker_symbols.contains_key(s.name.as_str()) =>
            {
                let (index, stop) = linker_symbols[s.name.as_str()];
                Ok(BASE + sections[index].offset + if stop { sections[index].size } else { 0 })
            }
            SymbolSection::Undefined if s.weak() => Ok(0),
            SymbolSection::Common => {
                let (section, offset) = common_locations
                    .get(&s.name)
                    .ok_or("unallocated common symbol")?;
                Ok(BASE + sections[*section].offset + offset)
            }
            SymbolSection::Section(index) => {
                let out = units[ui].sections.get(&index.0).ok_or_else(|| {
                    format!(
                        "{}: reference to omitted section for `{}`",
                        units[ui].name, s.name
                    )
                })?;
                Ok(BASE + sections[*out].offset + s.value)
            }
            _ => Err(format!(
                "{}: unresolved symbol `{}`",
                units[ui].name, s.name
            )),
        }
    };
    let address = |ui: usize, si: usize| -> Result<u64> {
        let selection = selected.get(ui, si)?;
        definition_address(selection.key.0, selection.definition)
    };
    let &(entry_u, entry_s) = globals
        .get(&options.entry)
        .ok_or_else(|| format!("missing entry symbol `{}`", options.entry))?;
    let entry_symbol = &units[entry_u].symbols[&entry_s];
    let entry_section = entry_symbol
        .section
        .index()
        .and_then(|i| units[entry_u].sections.get(&i.0))
        .ok_or("entry must be defined in an executable section")?;
    if sections[*entry_section].flags & 4 == 0
        || entry_symbol.value >= sections[*entry_section].size
    {
        return Err("entry must be inside an executable section".into());
    }
    let entry = address(entry_u, entry_s)?;
    let file_end = segments.iter().map(|s| s.file_end).max().unwrap_or(page);
    let mut bytes = vec![0; file_end as usize];
    if !arm {
        lifecycle::code_padding(&mut bytes, &sections);
    }
    for s in &sections {
        if !s.bss && !s.data.is_empty() {
            bytes[s.offset as usize..s.offset as usize + s.data.len()].copy_from_slice(&s.data);
        }
    }
    for (&(ui, si), &offset) in &got {
        put64(
            &mut bytes,
            (sections[got_section].offset + offset) as usize,
            address(ui, si)?,
        );
    }
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            let selection = selected.get(ui, r.symbol)?;
            let import = selection.import;
            if let (Some(name), Some(d)) = (import, dynamic.as_mut())
                && r.kind == 1
            {
                let offset = sections[r.section].offset + r.offset;
                d.relocations
                    .push((BASE + offset, d.imports[name].index, 1, r.addend));
                bytes[offset as usize..offset as usize + 8].fill(0);
                continue;
            }
            let result = || -> Result<(usize, [u8; 8], usize)> {
                let offset = sections[r.section].offset + r.offset;
                let p = i128::from(BASE + offset);
                let value = if let (Some(name), Some(d)) = (import, dynamic.as_ref()) {
                    if matches!(r.kind, 9 | 41 | 42) {
                        i128::from(d.slot(name, &sections))
                    } else {
                        i128::from(d.stub(name, &sections)?)
                    }
                } else if if arm {
                    aarch64::got(r.kind)
                } else {
                    matches!(r.kind, 9 | 41 | 42)
                } {
                    i128::from(BASE + sections[got_section].offset + got[&selection.key])
                } else {
                    i128::from(definition_address(selection.key.0, selection.definition)?)
                } + i128::from(r.addend);
                if arm {
                    let width = aarch64::width(r.kind)? as usize;
                    let encoded = aarch64::apply(
                        r.kind,
                        value,
                        p,
                        &bytes[offset as usize..offset as usize + width],
                    )?;
                    let mut data = [0u8; 8];
                    data[..width].copy_from_slice(&encoded);
                    return Ok((offset as usize, data, width));
                }
                // Preserve the allocation-free x86 relocation path.

                let data = match r.kind {
                    15 => i8::try_from(value - p).map(|v| (v as u64, 1)).ok(),
                    1 => Some((value as u64, 8)),
                    10 => u32::try_from(value).map(|v| (u64::from(v), 4)).ok(),
                    11 => i32::try_from(value as u64 as i64)
                        .map(|v| (v as u64, 4))
                        .ok(),
                    24 => i64::try_from(value - p).map(|v| (v as u64, 8)).ok(),
                    _ => i32::try_from(value - p).map(|v| (v as u64, 4)).ok(),
                }
                .ok_or_else(|| {
                    format!(
                        "relocation {} overflow for `{}`",
                        r.kind, u.symbols[&r.symbol].name
                    )
                })?;
                Ok((offset as usize, data.0.to_le_bytes(), data.1))
            };
            let (offset, data, width) = result().map_err(|e| format!("{}: {e}", u.name))?;
            bytes[offset..offset + width].copy_from_slice(&data[..width]);
        }
    }
    if let Some((section, table)) = &sframe_section {
        table.finish(
            &mut bytes,
            *section,
            &sections,
            sframe_targets.as_ref().unwrap(),
        )?;
    }
    if let Some(d) = dynamic.as_mut() {
        let mut definitions = BTreeMap::new();
        for (name, &(ui, si)) in &globals {
            let s = &units[ui].symbols[&si];
            let index = match s.section {
                SymbolSection::Absolute => 0xfff1,
                SymbolSection::Common => {
                    common_locations.get(name).map(|(i, _)| i + 1).unwrap_or(0)
                }
                SymbolSection::Section(i) => {
                    units[ui].sections.get(&i.0).map(|i| i + 1).unwrap_or(0)
                }
                _ => 0,
            };
            if index == 0 {
                continue;
            }
            if index >= 0xff00 && index != 0xfff1 {
                return Err("too many output sections".into());
            }
            definitions.insert(name.clone(), (definition_address(ui, s)?, index as u16));
        }
        d.emit(&mut bytes, &sections, &definitions)?;
    }
    if let Some(header) = unwind_header {
        unwind::emit(&mut bytes, &sections, &unwind_sections, header)?;
    }
    // Section headers and a global symbol table make readelf/nm useful without
    // promising source debugging or retaining input debug sections.
    let mut names = vec![0];
    let mut headers = vec![[0u8; 64]];
    for (index, s) in sections.iter().enumerate() {
        let name = string(&mut names, &s.name);
        let (kind, link, info, entsize) = dynamic
            .as_ref()
            .and_then(|d| d.section_header(index))
            .unwrap_or((s.kind, 0, 0, if matches!(s.kind, 14..=16) { 8 } else { 0 }));
        headers.push(shdr(
            name,
            kind,
            s.flags,
            BASE + s.offset,
            s.offset,
            s.size,
            link,
            info,
            s.align,
            entsize,
        ));
    }
    let mut strings = vec![0];
    let mut symbols = vec![0; 24];
    // Keep local names for debugger function bounds and prologue unwinding.
    // ELF requires all local records to precede the first nonlocal record.
    for (ui, unit) in units.iter().enumerate() {
        for s in unit.symbols.values() {
            if s.global() || s.name.is_empty() {
                continue;
            }
            let index = match s.section {
                SymbolSection::Absolute => 0xfff1,
                SymbolSection::Section(i) => unit.sections.get(&i.0).map(|i| i + 1).unwrap_or(0),
                _ => 0,
            };
            if index == 0 {
                continue;
            }
            if index >= 0xff00 && index != 0xfff1 {
                return Err("too many output sections".into());
            }
            let mut record = [0u8; 24];
            put32(&mut record, 0, string(&mut strings, &s.name));
            record[4] = s.info;
            record[5] = s.other;
            put16(&mut record, 6, index as u16);
            put64(&mut record, 8, definition_address(ui, s)?);
            put64(&mut record, 16, s.size);
            symbols.extend_from_slice(&record);
        }
    }
    let first_global = u32::try_from(symbols.len() / 24).map_err(|_| "too many local symbols")?;
    for (name, &(ui, si)) in &globals {
        let s = &units[ui].symbols[&si];
        // Nonallocated metadata symbols are not part of the executable image.
        let index = match s.section {
            SymbolSection::Absolute => 0xfff1,
            SymbolSection::Common => common_locations.get(name).map(|(i, _)| i + 1).unwrap_or(0),
            SymbolSection::Section(i) => units[ui].sections.get(&i.0).map(|i| i + 1).unwrap_or(0),
            _ => 0,
        };
        if index == 0 {
            continue;
        }
        if index >= 0xff00 && index != 0xfff1 {
            return Err("too many output sections".into());
        }
        let mut record = [0u8; 24];
        put32(&mut record, 0, string(&mut strings, name));
        record[4] = s.info;
        record[5] = s.other;
        put16(&mut record, 6, index as u16);
        put64(&mut record, 8, definition_address(ui, s)?);
        put64(
            &mut record,
            16,
            common_sizes.get(name).copied().unwrap_or(s.size),
        );
        symbols.extend_from_slice(&record);
    }
    for (name, &(index, stop)) in &linker_symbols {
        if !units.iter().any(|u| {
            u.symbols
                .values()
                .any(|s| s.global() && s.section == SymbolSection::Undefined && s.name == *name)
        }) {
            continue;
        }
        if index + 1 >= 0xff00 {
            return Err("too many output sections".into());
        }
        let mut record = [0u8; 24];
        put32(&mut record, 0, string(&mut strings, name));
        record[4] = if linker_owned(name) { 0x11 } else { 0x10 }; // GLOBAL, hidden and absent from .dynsym.
        record[5] = 2;
        put16(&mut record, 6, (index + 1) as u16);
        put64(
            &mut record,
            8,
            BASE + sections[index].offset + if stop { sections[index].size } else { 0 },
        );
        symbols.extend_from_slice(&record);
    }
    let str_index = headers.len() + 1;
    let sym_offset = align(bytes.len() as u64, 8)?;
    bytes.resize(sym_offset as usize, 0);
    bytes.extend_from_slice(&symbols);
    headers.push(shdr(
        string(&mut names, ".symtab"),
        2,
        0,
        0,
        sym_offset,
        symbols.len() as u64,
        str_index as u32,
        first_global,
        8,
        24,
    ));
    let str_offset = bytes.len() as u64;
    bytes.extend_from_slice(&strings);
    headers.push(shdr(
        string(&mut names, ".strtab"),
        3,
        0,
        0,
        str_offset,
        strings.len() as u64,
        0,
        0,
        1,
        0,
    ));
    let names_name = string(&mut names, ".shstrtab");
    let names_offset = bytes.len() as u64;
    bytes.extend_from_slice(&names);
    headers.push(shdr(
        names_name,
        3,
        0,
        0,
        names_offset,
        names.len() as u64,
        0,
        0,
        1,
        0,
    ));
    if headers.len() >= 0xff00 {
        return Err("too many output sections".into());
    }
    let shoff = align(bytes.len() as u64, 8)?;
    bytes.resize(shoff as usize, 0);
    for h in &headers {
        bytes.extend_from_slice(h);
    }
    bytes[..16].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    if sections.iter().any(|s| s.flags & 0x200000 != 0) {
        bytes[7] = 3;
    }
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, if arm { 183 } else { 62 });
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, entry);
    put64(&mut bytes, 32, 64);
    put64(&mut bytes, 40, shoff);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    let phnum = segments.len()
        + 1
        + usize::from(unwind_header.is_some())
        + usize::from(sframe_section.is_some())
        + note_plan.header_count()
        + if dynamic.is_some() { 3 } else { 0 };
    if 64 + 56 * phnum > page as usize {
        return Err("ELF program headers exceed the reserved first page".into());
    }
    put16(&mut bytes, 56, phnum as u16);
    put16(&mut bytes, 58, 64);
    put16(&mut bytes, 60, headers.len() as u16);
    put16(&mut bytes, 62, (headers.len() - 1) as u16);
    let mut phdrs = Vec::new();
    if let Some(d) = &dynamic {
        phdrs.push((6, 4, 64, (phnum * 56) as u64, (phnum * 56) as u64, 8));
        let s = &sections[d.interp];
        phdrs.push((3, 4, s.offset, s.size, s.size, 1));
    }
    for s in &segments {
        phdrs.push((
            1,
            s.flags,
            s.offset,
            s.file_end - s.offset,
            s.memory_end - s.offset,
            page,
        ));
    }
    if let Some(d) = &dynamic {
        let s = &sections[d.table];
        phdrs.push((2, 6, s.offset, s.size, s.size, 8));
    }
    if let Some(header) = unwind_header {
        let s = &sections[header];
        phdrs.push((object::elf::PT_GNU_EH_FRAME, 4, s.offset, s.size, s.size, 4));
    }
    if let Some((section, _)) = &sframe_section {
        let s = &sections[*section];
        phdrs.push((sframe::PT_GNU_SFRAME, 4, s.offset, s.size, s.size, 8));
    }
    phdrs.extend(note_plan.headers(&sections));
    phdrs.push((0x6474e551, 6, 0, 0, 0, 16));
    if phdrs.len() != phnum {
        return Err("inconsistent ELF program header count".into());
    }
    for (i, (kind, flags, offset, filesz, memsz, alignment)) in phdrs.iter().enumerate() {
        let at = 64 + i * 56;
        put32(&mut bytes, at, *kind);
        put32(&mut bytes, at + 4, *flags);
        put64(&mut bytes, at + 8, *offset);
        put64(
            &mut bytes,
            at + 16,
            if *kind == 0x6474e551 {
                0
            } else {
                BASE + offset
            },
        );
        put64(
            &mut bytes,
            at + 24,
            if *kind == 0x6474e551 {
                0
            } else {
                BASE + offset
            },
        );
        put64(&mut bytes, at + 32, *filesz);
        put64(&mut bytes, at + 40, *memsz);
        put64(&mut bytes, at + 48, *alignment);
    }
    Ok(Executable { bytes, warnings })
}
fn string(table: &mut Vec<u8>, name: &str) -> u32 {
    let at = table.len() as u32;
    table.extend_from_slice(name.as_bytes());
    table.push(0);
    at
}
#[allow(clippy::too_many_arguments)]
fn shdr(
    name: u32,
    kind: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    alignment: u64,
    entry_size: u64,
) -> [u8; 64] {
    let mut h = [0; 64];
    put32(&mut h, 0, name);
    put32(&mut h, 4, kind);
    put64(&mut h, 8, flags);
    put64(&mut h, 16, address);
    put64(&mut h, 24, offset);
    put64(&mut h, 32, size);
    put32(&mut h, 40, link);
    put32(&mut h, 44, info);
    put64(&mut h, 48, alignment);
    put64(&mut h, 56, entry_size);
    h
}
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
