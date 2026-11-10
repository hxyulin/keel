//! Host-independent ELF64 executable linker for x86-64 and AArch64 Linux.
//! See docs/linking/elf.md for the supported input and execution contract.
pub(crate) mod aarch64;
mod bounds;
mod dynamic;
mod gc;
mod groups;
mod input;
mod lifecycle;
mod merge;
mod notes;
mod relax;
mod reloc;
pub mod relocatable;
mod script;
mod selected;
mod sframe;
mod slots;
mod tls;
mod unwind;
use crate::{Input, Result, elf_selection::Selection};
use object::read::elf::{ElfFile64, FileHeader, SectionHeader};
use object::{
    Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget, SectionFlags,
    SymbolFlags, SymbolSection,
};
use slots::Slots;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

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
impl Machine {
    /// The ELF machine for `target`, or `None` for a target that is not ELF.
    pub fn for_target(target: keel_asm_ir::Target) -> Option<Self> {
        match target {
            keel_asm_ir::Target::X86_64LinuxGnu => Some(Self::X86_64),
            keel_asm_ir::Target::Aarch64LinuxGnu => Some(Self::Aarch64),
            keel_asm_ir::Target::Aarch64AppleDarwin | keel_asm_ir::Target::Wasm32UnknownUnknown => {
                None
            }
        }
    }

    /// The Linux target this machine links for.
    pub fn target(self) -> keel_asm_ir::Target {
        match self {
            Self::X86_64 => keel_asm_ir::Target::X86_64LinuxGnu,
            Self::Aarch64 => keel_asm_ir::Target::Aarch64LinuxGnu,
        }
    }
}

#[derive(Debug)]
pub struct Options {
    /// Relax eligible x86-64 GOTPCRELX address loads. Disabled by default.
    pub relax_got: bool,
    /// Deduplicate eligible ELF merge records and exact strings. Disabled by default.
    pub merge_constants: bool,
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
            relax_got: false,
            merge_constants: false,
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
/// An output section. Input sections borrow their bytes until a phase rewrites them.
struct Section<'a> {
    merge: Option<(u64, bool)>,
    name: String,
    flags: u64,
    kind: u32,
    align: u64,
    size: u64,
    bss: bool,
    data: Cow<'a, [u8]>,
    offset: u64,
}
impl Section<'_> {
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
struct Symbol<'a> {
    name: &'a str,
    section: SymbolSection,
    value: u64,
    size: u64,
    info: u8,
    other: u8,
}
impl Symbol<'_> {
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
struct Unit<'a> {
    name: String,
    sections: Slots<usize>,
    symbols: Slots<Symbol<'a>>,
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
        return Err(format!("unsupported alignment {a}").into());
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

/// Selection metadata retained independently of the caller-owned object bytes.
pub(crate) struct Prepared<'a> {
    inputs: Vec<input::Named<'a>>,
    libraries: Vec<dynamic::Library<'a>>,
}

struct ResolvedOptions {
    options: Options,
    script: Option<script::Script>,
    page: u64,
}

fn resolve_options(mut options: Options) -> Result<ResolvedOptions> {
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
    if arm && options.interpreter == "/lib64/ld-linux-x86-64.so.2" {
        options.interpreter = "/lib/ld-linux-aarch64.so.1".into();
    }
    if options.relax_got && (arm || script.is_some()) {
        return Err("GOT relaxation requires default-layout x86-64 ELF output".into());
    }
    if options.merge_constants && (arm || script.is_some()) {
        return Err("constant merging requires default-layout x86-64 ELF output".into());
    }
    let page = options
        .max_page_size
        .unwrap_or(if arm { 65536 } else { PAGE });
    if !page.is_power_of_two() || !(PAGE..=LIMIT).contains(&page) {
        return Err(format!(
            "invalid ELF maximum page size {page}: expected a power of two between 4096 and {LIMIT}"
        )
        .into());
    }
    if script.is_none() && !BASE.is_multiple_of(page) {
        return Err("maximum page size is incompatible with the default ELF base address".into());
    }
    Ok(ResolvedOptions {
        options,
        script,
        page,
    })
}

pub fn link(inputs: &[Input<'_>], options: Options) -> Result<Executable> {
    if inputs.is_empty() {
        return Err("no input objects".into());
    }
    let resolved = resolve_options(options)?;
    let prepared = input::prepare(inputs, &resolved.options.entry)?;
    link_prepared(&prepared, resolved)
}

/// Link an ordered selection without repeating archive and interface preparation.
/// The same immutable selection may be linked repeatedly with layout options.
///
/// # Errors
/// Returns the ordinary link errors, or an error when the effective entry differs
/// from the entry used to prepare the selection. Script and explicit entry
/// overrides are resolved before checking that contract.
pub fn link_selected(selection: &Selection<'_>, options: Options) -> Result<Executable> {
    if selection.inputs.is_empty() {
        return Err("no input objects".into());
    }
    let resolved = resolve_options(options)?;
    if selection.entry != resolved.options.entry {
        return Err("prepared ELF selection entry does not match effective link entry".into());
    }
    link_prepared(&selection.prepared, resolved)
}

/// Selected definition of each global name, as `(unit, symbol)`.
type Globals<'a> = crate::fx::Map<&'a str, (usize, usize)>;
/// The selected globals in name order, for output that lists them.
type OrderedGlobals<'a> = Vec<(&'a str, (usize, usize))>;

fn ordered<'a>(globals: &Globals<'a>) -> OrderedGlobals<'a> {
    let mut ordered: OrderedGlobals<'a> = globals.iter().map(|(&name, &key)| (name, key)).collect();
    ordered.sort_unstable_by(|a, b| a.0.cmp(b.0));
    ordered
}
/// Size and alignment of each common symbol.
type Commons<'a> = BTreeMap<&'a str, (u64, u64)>;
/// Output `(section, offset)` of each placed common symbol.
type CommonLocations<'a> = BTreeMap<&'a str, (usize, u64)>;

/// Allocated input sections and the metadata parsed with them. Section
/// indices in `sections`, `unwind_sections`, `sframe_section`, `note_plan` and
/// each `Unit::sections` agree until a phase renumbers them.
struct Parsed<'a> {
    sections: Vec<Section<'a>>,
    units: Vec<Unit<'a>>,
    unwind_sections: BTreeMap<usize, unwind::Frames>,
    sframe_section: Option<(usize, sframe::Table)>,
    warnings: Vec<String>,
    note_plan: notes::Plan,
    object_properties: Vec<notes::Properties>,
}

/// `(p_type, p_flags, p_offset, p_filesz, p_memsz, p_align)`.
type ProgramHeader = (u32, u32, u64, u64, u64, u64);

/// The `.got` output section and the slot offset of each selected
/// `(unit, symbol)` that is not an import.
struct Got {
    tls: tls::Plan,
    section: usize,
    slots: BTreeMap<(usize, usize), u64>,
}

/// Link phases, in order: `parse_units`, `resolve_globals`, `collect` (GC),
/// `merge_metadata` (unwind and SFrame), `allocate_commons`, `plan_imports`,
/// `plan_got`, dynamic and note preparation, `layout`, then the `Placed`
/// phases that write bytes and `program_headers`.
fn link_prepared(prepared: &Prepared<'_>, resolved: ResolvedOptions) -> Result<Executable> {
    let ResolvedOptions {
        options,
        script,
        page,
    } = resolved;
    let arm = options.machine == Machine::Aarch64;
    let inputs = &prepared.inputs;
    let libraries = &prepared.libraries;
    for library in libraries {
        if library.arm != arm {
            return Err("shared library architecture does not match output".into());
        }
    }
    if script.is_some() && (arm || !libraries.is_empty() || options.gc_sections) {
        return Err("scripts currently require static x86-64 output without --gc-sections".into());
    }
    let mut parsed = parse_units(inputs, arm)?;
    let (mut globals, mut commons) = resolve_globals(&parsed.units)?;
    if let Some(script) = script {
        return link_script(script, parsed, globals, &options);
    }
    let mut sframe_targets = collect(
        &mut parsed,
        &mut globals,
        &mut commons,
        &options,
        !libraries.is_empty(),
    )?;
    merge_metadata(&mut parsed, &mut sframe_targets, options.gc_sections)?;
    let ordered = ordered(&globals);
    let Parsed {
        mut sections,
        mut units,
        unwind_sections,
        sframe_section,
        warnings,
        mut note_plan,
        object_properties,
    } = parsed;
    if options.merge_constants {
        merge::constants(&mut sections, &mut units, &globals);
    }
    let (common_locations, common_sizes) =
        allocate_commons(&mut sections, &units, &globals, commons)?;
    let bounds = bounds::symbols(&sections, &units, &globals)?;
    if !arm {
        relax::tls_general(&mut sections, &mut units, &globals)?;
    }
    let imports = plan_imports(&units, &globals, &bounds, libraries, arm)?;
    if options.relax_got {
        relax::got_loads(&mut sections, &mut units, &globals, &imports)?;
    }
    // GOT keys retain selected raw identities, including independent locals.
    // Build after the final symbol/section rewrites; address validation stays lazy.
    let selected = selected::Index::new(&units, &globals, &imports);
    check_property_references(&units, &globals, &selected, libraries)?;
    let got = plan_got(&mut sections, &units, &selected, arm)?;
    let mut dynamic = if libraries.is_empty() {
        None
    } else {
        let definitions = dynamic_definitions(&units, &ordered, &common_sizes);
        let absolute = absolute_imports(arm, &units, &sections, &selected)?;
        let lifecycle = lifecycle::Plan::new(&sections, &units, &globals)?;
        Some(dynamic::Dynamic::prepare(
            &mut sections,
            imports.clone(),
            definitions,
            libraries,
            &options,
            absolute + got.tls.relocation_count(&selected)?,
            lifecycle,
        )?)
    };
    let unwind_header = unwind::prepare(&mut sections, &unwind_sections)?;
    note_plan.prepare(
        &mut sections,
        &object_properties,
        dynamic.as_ref().is_some_and(|d| !d.stubs.is_empty()),
    )?;
    let segments = layout(&mut sections, page)?;
    let mut linker_symbols = bounds;
    linker_symbols.insert("_GLOBAL_OFFSET_TABLE_".into(), (got.section, false));
    if let Some(d) = &dynamic {
        linker_symbols.insert("_DYNAMIC".into(), (d.table, false));
    }
    let placed = Placed {
        sections: &sections,
        units: &units,
        globals: &globals,
        ordered: &ordered,
        selected: &selected,
        linker_symbols: &linker_symbols,
        common_locations: &common_locations,
        common_sizes: &common_sizes,
    };
    let entry = placed.entry(&options.entry)?;
    let file_end = segments.iter().map(|s| s.file_end).max().unwrap_or(page);
    let mut bytes = placed.blank_image(file_end, arm);
    let written = placed.write_sections(&mut bytes, &got, dynamic.as_ref(), arm);
    // Report failures in the order of the serial phases: GOT slots, TLS
    // slots, then relocations. Import relocations follow the TLS ones.
    placed.got_slots(&mut bytes, &got)?;
    got.tls.emit(&placed, &mut bytes, &mut dynamic)?;
    if let Some((_, error)) = written.error {
        return Err(error);
    }
    if let Some(d) = dynamic.as_mut() {
        d.relocations.extend(
            written
                .imports
                .into_iter()
                .map(|(_, relocation)| relocation),
        );
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
        d.emit(&mut bytes, &sections, &placed.dynamic_exports()?)?;
    }
    if let Some(header) = unwind_header {
        unwind::emit(&mut bytes, &sections, &unwind_sections, header)?;
    }
    let (shoff, section_headers) = placed.tables(&mut bytes, dynamic.as_ref())?;
    let phdrs = program_headers(
        &sections,
        &segments,
        page,
        dynamic.as_ref(),
        unwind_header,
        sframe_section.as_ref().map(|(section, _)| *section),
        &note_plan,
    )?;
    write_headers(
        &mut bytes,
        &sections,
        arm,
        entry,
        shoff,
        section_headers,
        &phdrs,
    );
    Ok(Executable { bytes, warnings })
}

/// Parse and validate every input object. Output section indices are assigned
/// in input order.
fn parse_units<'a>(inputs: &'a [input::Named<'_>], arm: bool) -> Result<Parsed<'a>> {
    // An object's parse is independent of the others except for its output
    // section numbers and two whole-link checks: one SFrame producer and one
    // ABI note. Parse each object alone, in parallel for large links, then
    // renumber and check while absorbing them in input order.
    const BYTES_PER_WORKER: usize = 1 << 20;
    let alone = crate::parallel::map(
        inputs.iter().collect(),
        |input| input.bytes.len(),
        BYTES_PER_WORKER,
        |input| {
            let mut parsed = Parsed::empty();
            let mut properties = notes::Properties::new();
            let unit = parsed.parse_unit(input, arm, &mut properties).ok()?;
            Some((parsed, unit, properties))
        },
    );
    let mut parsed = Parsed::empty();
    for object in alone {
        let Some((alone, unit, properties)) = object else {
            return parse_units_serially(inputs, arm);
        };
        if !parsed.absorb(alone, unit, properties) {
            return parse_units_serially(inputs, arm);
        }
    }
    Ok(parsed)
}

/// Parse in input order, failing at the first error. Every failed link takes
/// this path, so it reports the same error that a serial parse always did.
fn parse_units_serially<'a>(inputs: &'a [input::Named<'_>], arm: bool) -> Result<Parsed<'a>> {
    let mut parsed = Parsed::empty();
    for input in inputs {
        let mut properties = notes::Properties::new();
        let unit = parsed
            .parse_unit(input, arm, &mut properties)
            .map_err(|e| format!("{}: {e}", input.name))?;
        parsed.units.push(unit);
        parsed.object_properties.push(properties);
    }
    Ok(parsed)
}

impl<'a> Parsed<'a> {
    fn empty() -> Self {
        Self {
            sections: Vec::new(),
            units: Vec::new(),
            unwind_sections: BTreeMap::new(),
            sframe_section: None,
            warnings: Vec::new(),
            note_plan: notes::Plan::default(),
            object_properties: Vec::new(),
        }
    }

    /// Append one object parsed alone, renumbering its output sections after
    /// the ones already present. Returns false when the object adds a second
    /// SFrame producer or ABI note, which the serial parse diagnoses.
    fn absorb(
        &mut self,
        mut alone: Parsed<'a>,
        mut unit: Unit<'a>,
        properties: notes::Properties,
    ) -> bool {
        let base = self.sections.len();
        if alone.sframe_section.is_some() && self.sframe_section.is_some()
            || alone.note_plan.abi.is_some() && self.note_plan.abi.is_some()
        {
            return false;
        }
        if let Some((section, table)) = alone.sframe_section {
            self.sframe_section = Some((section + base, table));
        }
        if let Some(section) = alone.note_plan.abi {
            self.note_plan.abi = Some(section + base);
        }
        for (section, frames) in alone.unwind_sections {
            self.unwind_sections.insert(section + base, frames);
        }
        for section in unit.sections.values_mut() {
            *section += base;
        }
        for reloc in &mut unit.relocs {
            reloc.section += base;
        }
        self.sections.append(&mut alone.sections);
        self.warnings.append(&mut alone.warnings);
        self.units.push(unit);
        self.object_properties.push(properties);
        true
    }

    /// Validate one relocatable object, append its allocated sections, and
    /// return its symbols and relocations. GNU property notes go to `properties`.
    fn parse_unit(
        &mut self,
        input: &'a input::Named<'_>,
        arm: bool,
        properties: &mut notes::Properties,
    ) -> Result<Unit<'a>> {
        let Self {
            sections,
            unwind_sections,
            sframe_section,
            warnings,
            note_plan,
            ..
        } = self;
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
            )
            .into());
        }
        let header = file.elf_header();
        if !matches!(header.e_ident().os_abi, 0 | 3) || header.e_flags(file.endian()) != 0 {
            return Err("unsupported ELF OS ABI or machine flags".into());
        }
        let group_plan = &input.groups;
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
                ).into());
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
                return Err(format!("unsupported relocation table {name} sh_info {info}").into());
            }
        }
        let mut unit = Unit {
            name: input.name.clone(),
            sections: Slots::default(),
            symbols: Slots::default(),
            relocs: Vec::new(),
            consumed_properties: BTreeSet::new(),
            groups: group_plan.membership.clone(),
        };
        for section in file.sections() {
            if group_plan.discarded.contains(&section.index().0) {
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
                    if raw.sh_entsize(file.endian()) != 24 || !data.len().is_multiple_of(24) =>
                {
                    return Err(format!("malformed ELF record table {name}").into());
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
                return Err(
                    format!("AArch64 ELF input section {name} is not yet supported").into(),
                );
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
                    return Err(format!("invalid section size for {name}").into());
                }
                section.elf_linked_rela().map_err(|e| e.to_string())?;
                if section.relocations().next().is_some() {
                    return Err(format!("relocations into {name} are unsupported").into());
                }
                match note {
                    notes::Kind::Property => {
                        notes::properties(data, properties)?;
                        unit.consumed_properties.insert(section.index().0);
                        continue;
                    }
                    notes::Kind::Abi => {
                        notes::abi(data)?;
                        if note_plan.abi.replace(sections.len()).is_some() {
                            return Err("additional .note.ABI-tag contracts are unsupported".into());
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
                return Err("input .eh_frame_hdr is unsupported; the linker generates it".into());
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
            if sh_flags & 0x400 != 0
                && (sh_flags & 5 != 1
                    || !matches!(section.elf_section_header().sh_type(file.endian()), 1 | 8))
            {
                return Err(format!("invalid TLS section {name}").into());
            }
            // SHF_WRITE/ALLOC/EXECINSTR/MERGE/STRINGS/GNU_RETAIN. In particular,
            // Compression and link-order semantics are rejected.
            // Group membership was validated and selected before parsing.
            if sh_flags & !0x200637 != 0 || sh_flags & 5 == 5 {
                return Err(format!("unsupported flags {sh_flags:#x} on {name}").into());
            }
            let bss = matches!(
                section.kind(),
                object::SectionKind::UninitializedData | object::SectionKind::UninitializedTls
            );
            if !lifecycle
                && !sframe
                && note.is_none()
                && !linux_note
                && !matches!(
                    section.kind(),
                    object::SectionKind::Text
                        | object::SectionKind::Data
                        | object::SectionKind::Tls
                        | object::SectionKind::UninitializedTls
                        | object::SectionKind::ReadOnlyData
                        | object::SectionKind::ReadOnlyString
                        | object::SectionKind::UninitializedData
                        | object::SectionKind::Elf(object::elf::SHT_X86_64_UNWIND)
                )
            {
                return Err(format!(
                    "unsupported allocated section {name} ({:?})",
                    section.kind()
                )
                .into());
            }
            let size = end(0, input_size)?;
            let alignment = section.align().max(1);
            align(0, alignment)?;
            if !bss && data.len() as u64 != size {
                return Err(format!("invalid section size for {name}").into());
            }
            if frame {
                if size != 0 && alignment < 4 {
                    return Err(".eh_frame alignment must be at least 4".into());
                }
                let frames = unwind::Frames::parse(data).map_err(|e| format!(".eh_frame: {e}"))?;
                unwind_sections.insert(sections.len(), frames);
            }
            if sframe {
                if sframe_section.is_some() {
                    return Err(".sframe: second selected SFrame producer is unsupported; merging is not implemented (empty headers also count)".into());
                }
                *sframe_section = Some((sections.len(), sframe::Table::parse(data)?));
            }
            unit.sections.insert(section.index().0, sections.len());
            sections.push(Section {
                merge: (sh_flags & 0x10 != 0)
                    .then_some((raw.sh_entsize(file.endian()), sh_flags & 0x20 != 0)),
                name,
                flags: sh_flags & 0x200407,
                kind,
                align: alignment,
                size,
                bss,
                data: if sframe {
                    Cow::Owned(sframe_section.as_ref().unwrap().1.rebuild()?)
                } else {
                    Cow::Borrowed(data)
                },
                offset: 0,
            });
            // The generic relocation iterator skips malformed tables. Validate
            // explicitly before consuming it so bad inputs cannot mislink.
            section.elf_linked_rela().map_err(|e| e.to_string())?;
            // One flag per encoded pointer field: a relocation must address a
            // field, so this detects duplicates without an ordered set.
            let frames = unwind_sections.get(&(sections.len() - 1));
            let mut unwind_relocations = vec![false; frames.map_or(0, |f| f.field_count())];
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
                    reloc::x86_64::width(kind)?
                };
                let width = if !arm && kind == object::elf::R_X86_64_TLSDESC_CALL {
                    2
                } else {
                    width
                };
                if bss || offset.checked_add(width).is_none_or(|e| e > size) {
                    return Err("relocation field is outside file-backed section data".into());
                }
                if let Some(frames) = frames {
                    // A first relocation at a non-field offset fails here, so a
                    // duplicate always addresses a field, as an offset set would find.
                    let field = frames
                        .field_at(offset)
                        .map_err(|e| format!(".eh_frame: {e}"))?;
                    if std::mem::replace(&mut unwind_relocations[field], true) {
                        return Err("duplicate .eh_frame pointer relocation".into());
                    }
                    frames
                        .check_field(
                            field,
                            if arm {
                                use object::elf::*;
                                match kind {
                                    R_AARCH64_ABS64 => R_X86_64_64,
                                    R_AARCH64_ABS32 => R_X86_64_32,
                                    R_AARCH64_PREL64 => R_X86_64_PC64,
                                    R_AARCH64_PREL32 => R_X86_64_PC32,
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
        unit.symbols.insert(
            0,
            Symbol {
                name: "",
                section: SymbolSection::Absolute,
                value: 0,
                size: 0,
                info: 0,
                other: 0,
            },
        );
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
                    )
                    .into());
                }
            }
            let SymbolFlags::Elf {
                st_info: info,
                st_other: other,
            } = s.flags()
            else {
                unreachable!()
            };
            if info & 15 == object::elf::STT_TLS
                && s.is_definition()
                && !group_plan.dead_symbol(s.index().0, s.section())
            {
                let valid = s
                    .section()
                    .index()
                    .and_then(|i| unit.sections.get(&i.0))
                    .is_some_and(|&i| is_tls(&sections[i]));
                if !valid {
                    return Err(format!(
                        "TLS symbol {} requires a TLS section",
                        s.name().unwrap_or("?")
                    )
                    .into());
                }
            }
            if info >> 4 > 2 || !matches!(info & 15, 0..=4 | 6) {
                return Err(format!(
                    "unsupported symbol binding/type for {}",
                    s.name().unwrap_or("?")
                )
                .into());
            }
            let mut symbol = Symbol {
                name: s.name().map_err(|e| e.to_string())?,
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
                    return Err(
                        format!("symbol {} extends outside its section", symbol.name).into(),
                    );
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
    }
}

/// Choose the definition each global name binds to, as `(unit, symbol)`, and
/// the largest size and alignment requested for each common symbol.
fn resolve_globals<'a>(units: &[Unit<'a>]) -> Result<(Globals<'a>, Commons<'a>)> {
    let mut globals = Globals::default();
    let mut commons: Commons = BTreeMap::new();
    for (ui, u) in units.iter().enumerate() {
        for (si, s) in u.symbols.iter() {
            if !s.global() || s.rank() == 0 {
                continue;
            }
            if linker_owned(s.name) {
                return Err(format!(
                    "{}: input defines reserved linker-owned symbol `{}`",
                    u.name, s.name
                )
                .into());
            }
            if s.section == SymbolSection::Common {
                align(0, s.value.max(1))?;
                let c = commons.entry(s.name).or_insert((0, 1));
                c.0 = c.0.max(s.size);
                c.1 = c.1.max(s.value.max(1));
            }
            if let Some(&(prev_u, prev_s)) = globals.get(&s.name) {
                let previous = &units[prev_u].symbols[&prev_s];
                if previous.rank() == 3 && s.rank() == 3 {
                    return Err(format!(
                        "{}: duplicate definition `{}` (also in {})",
                        u.name, s.name, units[prev_u].name
                    )
                    .into());
                }
                if previous.rank() >= s.rank() {
                    continue;
                }
            }
            globals.insert(s.name, (ui, si));
        }
    }
    Ok((globals, commons))
}

/// Linker scripts lay out sections themselves and do not support GC, so only
/// the unwind merge and note preparation run before handing over.
fn link_script(
    script: script::Script,
    parsed: Parsed,
    globals: Globals,
    options: &Options,
) -> Result<Executable> {
    let Parsed {
        mut sections,
        mut units,
        mut unwind_sections,
        sframe_section,
        warnings,
        mut note_plan,
        object_properties,
    } = parsed;
    if sframe_section.is_some() {
        return Err("SFrame reconstruction with linker scripts is unsupported".into());
    }
    unwind::merge(&mut sections, &mut units, &mut unwind_sections)?;
    note_plan.prepare(&mut sections, &object_properties, false)?;
    script::link(script, sections, units, globals, options, warnings)
}

/// Bind SFrame functions to their original symbols, then run section GC when
/// requested. GC renumbers and drops output sections and remaps `parsed`,
/// `globals` and the returned bindings itself; this phase remaps
/// `note_plan.abi` and drops commons whose global was collected.
fn collect(
    parsed: &mut Parsed,
    globals: &mut Globals,
    commons: &mut Commons,
    options: &Options,
    dynamic: bool,
) -> Result<Option<sframe::Bindings>> {
    let Parsed {
        sections,
        units,
        unwind_sections,
        sframe_section,
        note_plan,
        ..
    } = parsed;
    // Bind original identities and validate even dead ranges before GC or merge.
    let mut sframe_targets = sframe_section
        .as_ref()
        .map(|(section, table)| table.bind(*section, units, sections, globals, options.gc_sections))
        .transpose()?;
    if options.gc_sections {
        let remap = gc::collect(
            sections,
            units,
            unwind_sections,
            globals,
            gc::Roots {
                entry: &options.entry,
                dynamic,
            },
            sframe_section,
            &mut sframe_targets,
        )?;
        note_plan.abi = note_plan.abi.and_then(|i| remap[i]);
        commons.retain(|name, _| globals.contains_key(name));
    }
    Ok(sframe_targets)
}

/// Merge the input `.eh_frame` sections into one output section and, after GC,
/// rebuild the SFrame table over the live functions. The merge renumbers output
/// sections and remaps `parsed` itself; this phase remaps `note_plan.abi`, the
/// SFrame section and the SFrame bindings.
fn merge_metadata(
    parsed: &mut Parsed,
    sframe_targets: &mut Option<sframe::Bindings>,
    gc_sections: bool,
) -> Result<()> {
    let Parsed {
        sections,
        units,
        unwind_sections,
        sframe_section,
        note_plan,
        ..
    } = parsed;
    let section_remap = unwind::merge(sections, units, unwind_sections)?;
    note_plan.abi = note_plan.abi.map(|i| section_remap[i]);
    if let Some((section, table)) = sframe_section {
        *section = section_remap[*section];
        sframe_targets
            .as_mut()
            .unwrap()
            .remap(&section_remap.iter().copied().map(Some).collect::<Vec<_>>())?;
        if gc_sections {
            let targets = sframe_targets.as_mut().unwrap();
            let indices = table.order(targets, sections);
            table.reconstruct(targets, indices, *section, sections, units)?;
        }
    }
    Ok(())
}

/// Append `.bss.common` and place each common symbol that is still the selected
/// definition. Returns each placed symbol's `(section, offset)` and its size.
fn allocate_commons<'a>(
    sections: &mut Vec<Section>,
    units: &[Unit<'a>],
    globals: &Globals<'a>,
    commons: Commons<'a>,
) -> Result<(CommonLocations<'a>, BTreeMap<&'a str, u64>)> {
    let mut common_locations = BTreeMap::new();
    let mut common_sizes = BTreeMap::new();
    let mut common = Section {
        merge: None,
        name: ".bss.common".into(),
        flags: 3,
        kind: 8,
        align: 1,
        size: 0,
        bss: true,
        data: Vec::new().into(),
        offset: 0,
    };
    for (name, (size, alignment)) in commons {
        let (u, s) = globals[&name];
        if units[u].symbols[&s].section != SymbolSection::Common {
            continue;
        }
        let offset = align(common.size, alignment)?;
        common_sizes.insert(name, size);
        common_locations.insert(name, (sections.len(), offset));
        common.size = end(offset, size)?;
        common.align = common.align.max(alignment);
    }
    sections.push(common);
    Ok((common_locations, common_sizes))
}

/// Check undefined globals against the linker-defined bounds and the shared
/// libraries, and number the imports in name order starting at 1.
fn plan_imports(
    units: &[Unit],
    globals: &Globals,
    bounds: &BTreeMap<String, (usize, bool)>,
    libraries: &[dynamic::Library<'_>],
    arm: bool,
) -> Result<BTreeMap<String, dynamic::Import>> {
    // Non-PIC executable startup objects take direct addresses of optional
    // functions. With no DSO provider, these references bind to zero, as in GNU
    // ld. Bind the whole symbol consistently, including any GOT references in
    // other units; PIC-only weak references keep their runtime lookup.
    let mut direct_weak = BTreeSet::new();
    if !arm {
        for unit in units {
            for r in &unit.relocs {
                if matches!(
                    r.kind,
                    object::elf::R_X86_64_32
                        | object::elf::R_X86_64_32S
                        | object::elf::R_X86_64_PC32
                ) && let Some(s) = unit.symbols.get(&r.symbol)
                    && s.global()
                    && s.weak()
                    && s.section == SymbolSection::Undefined
                    && s.info & 15 != object::elf::STT_TLS
                {
                    direct_weak.insert(s.name);
                }
            }
        }
    }
    let mut imports: BTreeMap<String, dynamic::Import> = BTreeMap::new();
    for u in units {
        for s in u.symbols.values() {
            if !s.global() || s.section != SymbolSection::Undefined || globals.contains_key(&s.name)
            {
                continue;
            }
            if bounds.contains_key(s.name) {
                continue;
            }
            if linker_owned(s.name) {
                if s.name == "_DYNAMIC" && libraries.is_empty() && !s.weak() {
                    return Err(format!(
                        "{}: `_DYNAMIC` requires dynamically linked output",
                        u.name
                    )
                    .into());
                }
                continue;
            }
            let export = libraries
                .iter()
                .enumerate()
                .find_map(|(i, l)| l.exports.get(s.name).map(|e| (i, e)));
            if export.is_none() && !s.weak() {
                return Err(format!("{}: undefined symbol `{}`", u.name, s.name).into());
            }
            if libraries.is_empty()
                || (export.is_none() && s.weak() && direct_weak.contains(s.name))
            {
                continue;
            }
            if s.other & 3 != 0 {
                return Err(format!(
                    "{}: non-default visibility undefined symbol `{}` cannot be imported",
                    u.name, s.name
                )
                .into());
            }
            let (library, kind, version) = match export {
                Some((i, e)) => (Some(i), e.kind, e.version.map(str::to_owned)),
                None => (None, s.info & 15, None),
            };
            if (kind == object::elf::STT_TLS) != (s.info & 15 == object::elf::STT_TLS) {
                return Err(format!("TLS type mismatch for import `{}`", s.name).into());
            }
            if !matches!(kind, 0 | 1 | 2 | 6 | 10) {
                return Err(format!(
                    "unsupported shared-library symbol type {kind} for `{}`",
                    s.name
                )
                .into());
            }
            imports
                .entry(s.name.to_owned())
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
    Ok(imports)
}

/// Reject references, from inputs or shared libraries, into GNU property
/// sections that note parsing consumed.
fn check_property_references(
    units: &[Unit],
    globals: &Globals,
    selected: &selected::Index<'_>,
    libraries: &[dynamic::Library<'_>],
) -> Result<()> {
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
                )
                .into());
            }
        }
    }
    // DSO consumers have no Unit.relocs. Check their dynamic lookups,
    // including preemptible defined symbols, against the selected executable
    // definitions used by native references and dynamic exports. A discarded
    // weak property definition or a same-named local is not the provider.
    for library in libraries {
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
                    ).into());
                }
            }
        }
    }
    Ok(())
}

/// Assign a GOT slot to each selected non-import target of a GOT relocation and
/// append the `.got` section.
fn plan_got(
    sections: &mut Vec<Section>,
    units: &[Unit],
    selected: &selected::Index<'_>,
    arm: bool,
) -> Result<Got> {
    let tls = tls::Plan::prepare(sections, units, selected, arm)?;
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
        merge: None,
        name: ".got".into(),
        flags: 2,
        kind: 1,
        align: 8,
        size: got_size,
        bss: false,
        data: vec![0; got_size as usize].into(),
        offset: 0,
    });
    Ok(Got {
        tls,
        section: got_section,
        slots: got,
    })
}

/// The executable's dynamic symbol definitions: every selected global with
/// default or protected visibility in a retained section.
fn dynamic_definitions(
    units: &[Unit],
    ordered: &OrderedGlobals,
    common_sizes: &BTreeMap<&str, u64>,
) -> Vec<dynamic::Definition> {
    let mut definitions = Vec::new();
    for &(name, (ui, si)) in ordered {
        let s = &units[ui].symbols[&si];
        if s.other & 3 == 2 || s.other & 3 == 1 {
            continue;
        }
        if matches!(s.section,SymbolSection::Section(i) if !units[ui].sections.contains_key(&i.0)) {
            continue;
        }
        definitions.push(dynamic::Definition {
            name: name.to_owned(),
            info: s.info,
            other: s.other,
            size: common_sizes.get(name).copied().unwrap_or(s.size),
        });
    }
    definitions
}

/// Count absolute 64-bit relocations against imports in writable data, and
/// reject the import relocations that would need copy or text relocations.
fn absolute_imports(
    arm: bool,
    units: &[Unit],
    sections: &[Section],
    selected: &selected::Index<'_>,
) -> Result<usize> {
    let mut absolute = 0;
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            if let Some(name) = selected.get(ui, r.symbol)?.import {
                if selected.get(ui, r.symbol)?.definition.info & 15 == object::elf::STT_TLS {
                    if tls::model(r.kind, arm).is_some() {
                        continue;
                    }
                    return Err(format!(
                        "unsupported relocation {} against imported TLS `{name}`",
                        r.kind
                    )
                    .into());
                }
                match r.kind {
                    kind if kind == if arm { 257 } else { 1 }
                        && sections[r.section].flags & 1 != 0 =>
                    {
                        absolute += 1
                    }
                    kind if if arm {
                        matches!(kind, 282 | 283)
                    } else {
                        kind == 4
                    } => {}
                    kind if if arm {
                        aarch64::got(kind)
                    } else {
                        matches!(kind, 9 | 41 | 42)
                    } => {}
                    _ => {
                        return Err(format!(
                            "{}: relocation {} against import `{name}` requires writable 64-bit data, a PLT32 call, or GOT addressing; use PIC (copy/text relocations are unsupported)",
                            u.name, r.kind
                        ).into());
                    }
                }
            }
        }
    }
    Ok(absolute)
}

fn is_tls(section: &Section) -> bool {
    section.flags & u64::from(object::elf::SHF_TLS) != 0
}

fn tls_header(sections: &[Section]) -> Option<ProgramHeader> {
    let tls: Vec<_> = sections.iter().filter(|s| is_tls(s)).collect();
    let start = tls.iter().map(|s| s.offset).min()?;
    let memory_end = tls.iter().map(|s| s.offset + s.size).max()?;
    let file_end = tls
        .iter()
        .filter(|s| !s.bss)
        .map(|s| s.offset + s.size)
        .max()
        .unwrap_or(start);
    let alignment = tls.iter().map(|s| s.align).max()?;
    Some((
        object::elf::PT_TLS,
        4,
        start,
        file_end - start,
        memory_end - start,
        alignment,
    ))
}

/// Assign file offsets, keeping initialized and zero-filled TLS adjacent.
fn layout(sections: &mut [Section], page: u64) -> Result<Vec<Segment>> {
    let has_tdata = sections.iter().any(|s| is_tls(s) && !s.bss);
    let registration_alignments = bounds::alignments(sections);
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
            let mut order = bounds::order(sections, lifecycle::order(sections, permissions, bss));
            order.sort_by_key(|&i| {
                if bss {
                    !is_tls(&sections[i])
                } else {
                    is_tls(&sections[i])
                }
            });
            let tls_align = sections
                .iter()
                .filter(|s| is_tls(s))
                .map(|s| s.align)
                .max()
                .unwrap_or(1);
            let mut first_tls = true;
            for index in order {
                let section = &mut sections[index];
                if is_tls(section) && first_tls {
                    // Align the template base to every TLS contribution.
                    if !bss || !has_tdata {
                        cursor = align(cursor, tls_align)?;
                    }
                    first_tls = false;
                }
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
    Ok(segments)
}

/// `(r_offset, symbol, r_type, r_addend)` of a dynamic relocation.
type DynamicRelocation = (u64, u32, u32, i64);

/// What writing the sections produced: dynamic relocations for imports and the
/// first failure, each keyed by `(unit, relocation)`.
#[derive(Default)]
struct Written {
    imports: Vec<((usize, usize), DynamicRelocation)>,
    error: Option<((usize, usize), crate::Error)>,
}

/// Sections written by one worker, and the image bytes from file offset `base`
/// that cover them.
struct SectionJob<'b> {
    sections: Vec<usize>,
    base: usize,
    bytes: &'b mut [u8],
}

/// Read-only view of the laid-out link, shared by the phases that write bytes.
struct Placed<'a> {
    sections: &'a [Section<'a>],
    units: &'a [Unit<'a>],
    globals: &'a Globals<'a>,
    ordered: &'a OrderedGlobals<'a>,
    selected: &'a selected::Index<'a>,
    linker_symbols: &'a BTreeMap<String, (usize, bool)>,
    common_locations: &'a CommonLocations<'a>,
    common_sizes: &'a BTreeMap<&'a str, u64>,
}

impl<'a> Placed<'a> {
    // Output symbol tables already traverse selected definitions or locals.
    // Use those symbols directly instead of looking up and selecting them again.
    fn definition_address(&self, ui: usize, s: &Symbol) -> Result<u64> {
        let Self {
            sections,
            units,
            linker_symbols,
            common_locations,
            ..
        } = self;
        match s.section {
            SymbolSection::Absolute => Ok(s.value),
            SymbolSection::Undefined if s.global() && linker_symbols.contains_key(s.name) => {
                let (index, stop) = linker_symbols[s.name];
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
                if s.info & 15 == object::elf::STT_TLS {
                    let tls = tls_header(sections).ok_or("TLS symbol without template")?;
                    Ok(sections[*out].offset - tls.2 + s.value)
                } else {
                    Ok(BASE + sections[*out].offset + s.value)
                }
            }
            _ => Err(format!("{}: unresolved symbol `{}`", units[ui].name, s.name).into()),
        }
    }

    fn address(&self, ui: usize, si: usize) -> Result<u64> {
        let selection = self.selected.get(ui, si)?;
        self.definition_address(selection.key.0, selection.definition)
    }

    /// The entry address, which must lie inside an executable section.
    fn entry(&self, name: &str) -> Result<u64> {
        let Self {
            sections,
            units,
            globals,
            ..
        } = self;
        let &(entry_u, entry_s) = globals
            .get(name)
            .ok_or_else(|| format!("missing entry symbol `{}`", name))?;
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
        self.address(entry_u, entry_s)
    }

    /// The zero-filled file image, with x86-64 code padding between the
    /// executable sections of each lifecycle group.
    fn blank_image(&self, file_end: u64, arm: bool) -> Vec<u8> {
        let mut bytes = vec![0; file_end as usize];
        if !arm {
            lifecycle::code_padding(&mut bytes, self.sections);
        }
        bytes
    }

    /// Copy each section's bytes into place and apply the input relocations
    /// that target it. Sections are independent jobs, run in parallel for
    /// large images. Within a section, relocations apply in (unit,
    /// relocation) order, as one serial pass did, because a field may read
    /// bytes an earlier relocation wrote. Absolute 64-bit references to
    /// imports become dynamic relocations, returned in that order with the
    /// first failure.
    fn write_sections(
        &self,
        bytes: &mut [u8],
        got: &Got,
        dynamic: Option<&dynamic::Dynamic>,
        arm: bool,
    ) -> Written {
        // Copies and relocation are memory-bound: more, smaller jobs add page
        // fault contention without shortening the link.
        const COST_PER_WORKER: usize = 8 << 20;
        let sections = self.sections;
        let mut runs: Vec<Vec<(usize, std::ops::Range<usize>)>> = vec![Vec::new(); sections.len()];
        for (ui, u) in self.units.iter().enumerate() {
            let mut start = 0;
            for ri in 1..=u.relocs.len() {
                if ri == u.relocs.len() || u.relocs[ri].section != u.relocs[start].section {
                    runs[u.relocs[start].section].push((ui, start..ri));
                    start = ri;
                }
            }
        }
        let mut order: Vec<usize> = (0..sections.len())
            .filter(|&i| !sections[i].bss && !sections[i].data.is_empty())
            .collect();
        order.sort_by_key(|&i| sections[i].offset);
        let disjoint = order.windows(2).all(|pair| {
            let first = &sections[pair[0]];
            first.offset + first.data.len() as u64 <= sections[pair[1]].offset
        }) && (0..sections.len())
            .all(|i| runs[i].is_empty() || (!sections[i].bss && !sections[i].data.is_empty()));
        let mut jobs = Vec::new();
        if disjoint {
            let mut rest = &mut *bytes;
            let mut consumed = 0;
            for &i in &order {
                let s = &sections[i];
                let (_, tail) = rest.split_at_mut(s.offset as usize - consumed);
                let (field, tail) = tail.split_at_mut(s.data.len());
                consumed = (s.offset + s.data.len() as u64) as usize;
                rest = tail;
                jobs.push(SectionJob {
                    sections: vec![i],
                    base: s.offset as usize,
                    bytes: field,
                });
            }
        } else {
            // Overlapping file ranges: one serial job in section index order.
            jobs.push(SectionJob {
                sections: (0..sections.len()).collect(),
                base: 0,
                bytes,
            });
        }
        let results = crate::parallel::map(
            jobs,
            |job| {
                job.sections
                    .iter()
                    .map(|&i| {
                        sections[i].data.len()
                            + 32 * runs[i].iter().map(|(_, r)| r.len()).sum::<usize>()
                    })
                    .sum()
            },
            COST_PER_WORKER,
            |job| self.write_job(job, &runs, got, dynamic, arm),
        );
        let mut written = Written::default();
        for result in results {
            written.imports.extend(result.imports);
            if let Some((key, error)) = result.error
                && written.error.as_ref().is_none_or(|(first, _)| key < *first)
            {
                written.error = Some((key, error));
            }
        }
        written.imports.sort_unstable_by_key(|&(key, _)| key);
        written
    }

    fn write_job(
        &self,
        job: SectionJob<'_>,
        runs: &[Vec<(usize, std::ops::Range<usize>)>],
        got: &Got,
        dynamic: Option<&dynamic::Dynamic>,
        arm: bool,
    ) -> Written {
        let SectionJob {
            sections: indices,
            base,
            bytes,
        } = job;
        for &i in &indices {
            let s = &self.sections[i];
            if !s.bss && !s.data.is_empty() {
                let at = s.offset as usize - base;
                bytes[at..at + s.data.len()].copy_from_slice(&s.data);
            }
        }
        let mut relocations: Vec<_> = indices
            .iter()
            .flat_map(|&i| runs[i].iter())
            .flat_map(|(ui, range)| range.clone().map(move |ri| (*ui, ri)))
            .collect();
        if indices.len() > 1 {
            relocations.sort_unstable();
        }
        let mut written = Written::default();
        for (ui, ri) in relocations {
            match self.relocate(ui, ri, bytes, base, got, dynamic, arm) {
                Ok(Some(import)) => written.imports.push(((ui, ri), import)),
                Ok(None) => {}
                Err(error) => {
                    written.error = Some(((ui, ri), error));
                    break;
                }
            }
        }
        written
    }

    /// Apply input relocation `ri` of unit `ui` to `bytes`, which hold the
    /// image from file offset `base`. Returns the dynamic relocation that
    /// replaces an absolute 64-bit reference to an import.
    #[allow(clippy::too_many_arguments)]
    fn relocate(
        &self,
        ui: usize,
        ri: usize,
        bytes: &mut [u8],
        base: usize,
        got: &Got,
        dynamic: Option<&dynamic::Dynamic>,
        arm: bool,
    ) -> Result<Option<DynamicRelocation>> {
        let Self {
            sections,
            units,
            selected,
            ..
        } = self;
        let u = &units[ui];
        let r = &u.relocs[ri];
        let selection = selected.get(ui, r.symbol)?;
        let import = selection.import;
        if let (Some(name), Some(d)) = (import, dynamic)
            && r.kind
                == if arm {
                    object::elf::R_AARCH64_ABS64
                } else {
                    object::elf::R_X86_64_64
                }
        {
            let offset = sections[r.section].offset + r.offset;
            let at = offset as usize - base;
            bytes[at..at + 8].fill(0);
            return Ok(Some((
                BASE + offset,
                d.imports[name].index,
                if arm {
                    object::elf::R_AARCH64_ABS64
                } else {
                    object::elf::R_X86_64_64
                },
                r.addend,
            )));
        }
        if !arm && r.kind == object::elf::R_X86_64_TLSDESC_CALL {
            let at = (sections[r.section].offset + r.offset) as usize - base;
            if bytes.get(at..at + 2) != Some(&[0xff, 0x10]) {
                return Err("TLSDESC_CALL requires call *(%rax)".into());
            }
        }
        let result = || -> Result<(usize, [u8; 8], usize)> {
            let offset = sections[r.section].offset + r.offset;
            let at = offset as usize - base;
            let p = i128::from(BASE + offset);
            let mut value = if let Some(model) = tls::model(r.kind, arm) {
                tls::value(self, got, r, ui, model)?
            } else if let (Some(name), Some(d)) = (import, dynamic) {
                if if arm {
                    aarch64::got(r.kind)
                } else {
                    reloc::x86_64::uses_got(r.kind)
                } {
                    i128::from(d.slot(name, sections))
                } else {
                    i128::from(d.stub(name, sections)?)
                }
            } else if if arm {
                aarch64::got(r.kind)
            } else {
                reloc::x86_64::uses_got(r.kind)
            } {
                i128::from(BASE + sections[got.section].offset + got.slots[&selection.key])
            } else {
                i128::from(self.definition_address(selection.key.0, selection.definition)?)
            } + i128::from(r.addend);
            let tls_reloc = if arm {
                matches!(r.kind, 523..=538 | 544..=559 | 570..=573)
            } else {
                matches!(r.kind, 17 | 18 | 21 | 23)
            };
            if tls::model(r.kind, arm).is_some() {
                // TLS GOT addressing was resolved separately above.
            } else if tls_reloc {
                if selection.definition.info & 15 != object::elf::STT_TLS || import.is_some() {
                    return Err("TLS relocation requires a defined TLS symbol".into());
                }
                let tls = tls_header(sections).ok_or("TLS relocation without template")?;
                if arm && matches!(r.kind, 544..=559 | 570 | 571) {
                    value += i128::from(align(16, tls.5)?);
                } else if matches!(r.kind, 18 | 23) {
                    value -= i128::from(align(tls.4, tls.5)?);
                }
            } else if selection.definition.info & 15 == object::elf::STT_TLS {
                return Err("non-TLS relocation against TLS symbol is unsupported".into());
            }
            if arm {
                let width = aarch64::width(r.kind)? as usize;
                let encoded = aarch64::apply(r.kind, value, p, &bytes[at..at + width])?;
                let mut data = [0u8; 8];
                data[..width].copy_from_slice(&encoded);
                return Ok((at, data, width));
            }
            let field =
                reloc::x86_64::encode(r.kind, value, BASE + offset).map_err(|e| match e {
                    reloc::x86_64::Error::Unsupported => {
                        format!("unsupported x86-64 relocation {}", r.kind)
                    }
                    reloc::x86_64::Error::Overflow => format!(
                        "relocation {} overflow for `{}`",
                        r.kind, u.symbols[&r.symbol].name
                    ),
                })?;
            Ok((at, field.bytes, field.width))
        };
        let (at, data, width) = result().map_err(|e| format!("{}: {e}", u.name))?;
        bytes[at..at + width].copy_from_slice(&data[..width]);
        Ok(None)
    }

    /// Fill the GOT slots of selected non-import targets.
    fn got_slots(&self, bytes: &mut [u8], got: &Got) -> Result<()> {
        let sections = self.sections;
        for (&(ui, si), &offset) in &got.slots {
            put64(bytes, (sections[got.section].offset + offset) as usize, {
                let selection = self.selected.get(ui, si)?;
                let value = self.address(ui, si)?;
                if selection.definition.info & 15 == object::elf::STT_TLS {
                    let tls = tls_header(sections).ok_or("TLS GOT without template")?;
                    value.wrapping_sub(align(tls.4, tls.5)?)
                } else {
                    value
                }
            });
        }
        Ok(())
    }

    /// Section index and address of every exported global, for `.dynsym`.
    fn dynamic_exports(&self) -> Result<BTreeMap<&'a str, (u64, u16)>> {
        let Self {
            units,
            ordered,
            common_locations,
            ..
        } = self;
        let mut definitions = BTreeMap::new();
        for &(name, (ui, si)) in *ordered {
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
            definitions.insert(name, (self.definition_address(ui, s)?, index as u16));
        }
        Ok(definitions)
    }

    /// Append the section headers and the symbol and string tables. Returns
    /// the section header offset and count.
    fn tables(
        &self,
        bytes: &mut Vec<u8>,
        dynamic: Option<&dynamic::Dynamic>,
    ) -> Result<(u64, usize)> {
        let Self {
            sections,
            units,
            ordered,
            linker_symbols,
            common_locations,
            common_sizes,
            ..
        } = self;
        // Section headers and a global symbol table make readelf/nm useful without
        // promising source debugging or retaining input debug sections.
        let mut names = vec![0];
        let mut headers = vec![[0u8; 64]];
        for (index, s) in sections.iter().enumerate() {
            let name = string(&mut names, &s.name);
            let (kind, link, info, entsize) = dynamic
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
                    SymbolSection::Section(i) => {
                        unit.sections.get(&i.0).map(|i| i + 1).unwrap_or(0)
                    }
                    _ => 0,
                };
                if index == 0 {
                    continue;
                }
                if index >= 0xff00 && index != 0xfff1 {
                    return Err("too many output sections".into());
                }
                symbols.extend_from_slice(&symbol_record(
                    string(&mut strings, s.name),
                    s.info,
                    s.other,
                    index as u16,
                    self.definition_address(ui, s)?,
                    s.size,
                ));
            }
        }
        let first_global =
            u32::try_from(symbols.len() / 24).map_err(|_| "too many local symbols")?;
        for &(name, (ui, si)) in *ordered {
            let s = &units[ui].symbols[&si];
            // Nonallocated metadata symbols are not part of the executable image.
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
            symbols.extend_from_slice(&symbol_record(
                string(&mut strings, name),
                s.info,
                s.other,
                index as u16,
                self.definition_address(ui, s)?,
                common_sizes.get(name).copied().unwrap_or(s.size),
            ));
        }
        for (name, &(index, stop)) in *linker_symbols {
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
            // GLOBAL, hidden and absent from .dynsym.
            symbols.extend_from_slice(&symbol_record(
                string(&mut strings, name),
                if linker_owned(name) { 0x11 } else { 0x10 },
                2,
                (index + 1) as u16,
                BASE + sections[index].offset + if stop { sections[index].size } else { 0 },
                0,
            ));
        }
        let shoff = symbol_and_section_tables(
            bytes,
            &mut headers,
            names,
            &symbols,
            &strings,
            first_global,
        )?;
        Ok((shoff, headers.len()))
    }
}

/// Program headers, in file order: PHDR and INTERP for dynamic output, the LOAD
/// segments, DYNAMIC, GNU_EH_FRAME, GNU_SFRAME, notes and GNU_STACK.
fn program_headers(
    sections: &[Section],
    segments: &[Segment],
    page: u64,
    dynamic: Option<&dynamic::Dynamic>,
    unwind_header: Option<usize>,
    sframe_section: Option<usize>,
    note_plan: &notes::Plan,
) -> Result<Vec<ProgramHeader>> {
    let tls = tls_header(sections);
    let phnum = segments.len()
        + usize::from(tls.is_some())
        + 1
        + usize::from(unwind_header.is_some())
        + usize::from(sframe_section.is_some())
        + note_plan.header_count()
        + if dynamic.is_some() { 3 } else { 0 };
    if 64 + 56 * phnum > page as usize {
        return Err("ELF program headers exceed the reserved first page".into());
    }
    let mut phdrs = Vec::new();
    if let Some(d) = dynamic {
        phdrs.push((6, 4, 64, (phnum * 56) as u64, (phnum * 56) as u64, 8));
        let s = &sections[d.interp];
        phdrs.push((3, 4, s.offset, s.size, s.size, 1));
    }
    for s in segments {
        phdrs.push((
            1,
            s.flags,
            s.offset,
            s.file_end - s.offset,
            s.memory_end - s.offset,
            page,
        ));
    }
    if let Some(d) = dynamic {
        let s = &sections[d.table];
        phdrs.push((2, 6, s.offset, s.size, s.size, 8));
    }
    if let Some(header) = unwind_header {
        let s = &sections[header];
        phdrs.push((object::elf::PT_GNU_EH_FRAME, 4, s.offset, s.size, s.size, 4));
    }
    if let Some(section) = sframe_section {
        let s = &sections[section];
        phdrs.push((sframe::PT_GNU_SFRAME, 4, s.offset, s.size, s.size, 8));
    }
    if let Some(tls) = tls {
        phdrs.push(tls);
    }
    phdrs.extend(note_plan.headers(sections));
    phdrs.push((0x6474e551, 6, 0, 0, 0, 16));
    if phdrs.len() != phnum {
        return Err("inconsistent ELF program header count".into());
    }
    Ok(phdrs)
}
/// Write the ELF file header and the program header table.
fn write_headers(
    bytes: &mut [u8],
    sections: &[Section],
    arm: bool,
    entry: u64,
    shoff: u64,
    section_headers: usize,
    phdrs: &[ProgramHeader],
) {
    bytes[..16].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    if sections.iter().any(|s| s.flags & 0x200000 != 0) {
        bytes[7] = 3;
    }
    put16(bytes, 16, 2);
    put16(bytes, 18, if arm { 183 } else { 62 });
    put32(bytes, 20, 1);
    put64(bytes, 24, entry);
    put64(bytes, 32, 64);
    put64(bytes, 40, shoff);
    put16(bytes, 52, 64);
    put16(bytes, 54, 56);
    put16(bytes, 56, phdrs.len() as u16);
    put16(bytes, 58, 64);
    put16(bytes, 60, section_headers as u16);
    put16(bytes, 62, (section_headers - 1) as u16);
    for (i, (kind, flags, offset, filesz, memsz, alignment)) in phdrs.iter().enumerate() {
        let at = 64 + i * 56;
        put32(bytes, at, *kind);
        put32(bytes, at + 4, *flags);
        put64(bytes, at + 8, *offset);
        put64(
            bytes,
            at + 16,
            if *kind == 0x6474e551 {
                0
            } else {
                BASE + offset
            },
        );
        put64(
            bytes,
            at + 24,
            if *kind == 0x6474e551 {
                0
            } else {
                BASE + offset
            },
        );
        put64(bytes, at + 32, *filesz);
        put64(bytes, at + 40, *memsz);
        put64(bytes, at + 48, *alignment);
    }
}
/// Append .symtab, .strtab, .shstrtab and the section header table, and
/// return the header table offset. `symbols` starts with the null record, and
/// its first `locals` records (null included) are the local symbols.
fn symbol_and_section_tables(
    bytes: &mut Vec<u8>,
    headers: &mut Vec<[u8; 64]>,
    mut names: Vec<u8>,
    symbols: &[u8],
    strings: &[u8],
    locals: u32,
) -> Result<u64> {
    let sym_offset = align(bytes.len() as u64, 8)?;
    bytes.resize(sym_offset as usize, 0);
    bytes.extend_from_slice(symbols);
    let str_index = headers.len() + 1;
    headers.push(shdr(
        string(&mut names, ".symtab"),
        object::elf::SHT_SYMTAB,
        0,
        0,
        sym_offset,
        symbols.len() as u64,
        str_index as u32,
        locals,
        8,
        24,
    ));
    let str_offset = bytes.len() as u64;
    bytes.extend_from_slice(strings);
    headers.push(shdr(
        string(&mut names, ".strtab"),
        object::elf::SHT_STRTAB,
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
        object::elf::SHT_STRTAB,
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
    for h in headers.iter() {
        bytes.extend_from_slice(h);
    }
    Ok(shoff)
}
/// One Elf64_Sym record.
fn symbol_record(name: u32, info: u8, other: u8, section: u16, value: u64, size: u64) -> [u8; 24] {
    let mut record = [0u8; 24];
    put32(&mut record, 0, name);
    record[4] = info;
    record[5] = other;
    put16(&mut record, 6, section);
    put64(&mut record, 8, value);
    put64(&mut record, 16, size);
    record
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
