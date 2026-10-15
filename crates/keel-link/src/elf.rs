//! Host-independent, bounded ELF64 executable linker for x86-64 Linux.
//! See docs/elf-linker.md for the supported input and execution contract.
mod dynamic;
mod input;
use crate::{Input, Result};
use object::read::elf::{ElfFile64, FileHeader, SectionHeader};
use object::{
    Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget, SectionFlags,
    SymbolFlags, SymbolSection,
};
use std::collections::BTreeMap;

const BASE: u64 = 0x400000;
const PAGE: u64 = 4096;
const LIMIT: u64 = 512 * 1024 * 1024;

pub struct Options {
    pub entry: String,
    pub interpreter: String,
    pub runpath: Vec<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            entry: "_start".into(),
            interpreter: "/lib64/ld-linux-x86-64.so.2".into(),
            runpath: Vec::new(),
        }
    }
}
#[derive(Debug)]
pub struct Executable {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}
struct Section {
    name: String,
    flags: u64,
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
}
struct Segment {
    flags: u32,
    offset: u64,
    file_end: u64,
    memory_end: u64,
}
fn align(n: u64, a: u64) -> Result<u64> {
    if !a.is_power_of_two() || a > 1024 * 1024 {
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

pub fn link(inputs: &[Input<'_>], options: Options) -> Result<Executable> {
    if inputs.is_empty() {
        return Err("no input objects".into());
    }
    if options.entry.is_empty() || options.entry.contains('\0') {
        return Err("invalid entry symbol".into());
    }
    let (inputs, libraries) = input::prepare(inputs, &options.entry)?;
    let mut sections = Vec::new();
    let mut units = Vec::new();
    let mut warnings = Vec::new();
    for input in &inputs {
        let mut parse = || -> Result<Unit> {
            let file =
                ElfFile64::<object::Endianness>::parse(input.bytes).map_err(|e| e.to_string())?;
            if file.architecture() != object::Architecture::X86_64
                || !file.is_64()
                || !file.is_little_endian()
                || file.kind() != object::ObjectKind::Relocatable
            {
                return Err("expected a little-endian x86-64 ELF64 relocatable object".into());
            }
            let header = file.elf_header();
            if !matches!(header.e_ident().os_abi, 0 | 3) || header.e_flags(file.endian()) != 0 {
                return Err("unsupported ELF OS ABI or machine flags".into());
            }
            if file.comdats().next().is_some() {
                return Err("COMDAT groups are unsupported".into());
            }
            let mut unit = Unit {
                name: input.name.clone(),
                sections: BTreeMap::new(),
                symbols: BTreeMap::new(),
                relocs: Vec::new(),
            };
            for section in file.sections() {
                let name = section.name().map_err(|e| e.to_string())?.to_owned();
                let data = section.data().map_err(|e| e.to_string())?;
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
                if sh_flags & 2 == 0 {
                    if name.starts_with(".debug") || name.starts_with(".zdebug") {
                        warnings.push(format!("{}: omitted debug section {name}", input.name));
                    }
                    continue;
                }
                // SHF_WRITE/ALLOC/EXECINSTR/MERGE/STRINGS only. In particular,
                // TLS, compression, groups and link-order semantics are rejected.
                if sh_flags & !0x37 != 0 || sh_flags & 5 == 5 {
                    return Err(format!("unsupported flags {sh_flags:#x} on {name}"));
                }
                let bss = section.kind() == object::SectionKind::UninitializedData;
                if !matches!(
                    section.kind(),
                    object::SectionKind::Text
                        | object::SectionKind::Data
                        | object::SectionKind::ReadOnlyData
                        | object::SectionKind::ReadOnlyString
                        | object::SectionKind::UninitializedData
                ) {
                    return Err(format!(
                        "unsupported allocated section {name} ({:?})",
                        section.kind()
                    ));
                }
                let size = end(0, section.size())?;
                let alignment = section.align().max(1);
                align(0, alignment)?;
                if !bss && data.len() as u64 != size {
                    return Err(format!("invalid section size for {name}"));
                }
                unit.sections.insert(section.index().0, sections.len());
                sections.push(Section {
                    name,
                    flags: sh_flags & 7,
                    align: alignment,
                    size,
                    bss,
                    data: data.to_vec(),
                    offset: 0,
                });
                // The generic relocation iterator skips malformed tables. Validate
                // explicitly before consuming it so bad inputs cannot mislink.
                section.elf_linked_rela().map_err(|e| e.to_string())?;
                for (offset, r) in section.relocations() {
                    let RelocationFlags::Elf { r_type: kind } = r.flags() else {
                        return Err("non-ELF relocation".into());
                    };
                    if kind == 0 {
                        continue;
                    }
                    if r.has_implicit_addend() {
                        return Err("REL relocations are unsupported; use RELA".into());
                    }
                    let width = match kind {
                        1 | 24 => 8,
                        2 | 4 | 9 | 10 | 11 | 41 | 42 => 4,
                        _ => return Err(format!("unsupported x86-64 relocation {kind}")),
                    };
                    if bss || offset.checked_add(width).is_none_or(|e| e > size) {
                        return Err("relocation field is outside file-backed section data".into());
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
            }
            for s in file.symbols() {
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
                let symbol = Symbol {
                    name: s.name().map_err(|e| e.to_string())?.into(),
                    section: s.section(),
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
                unit.symbols.insert(s.index().0, symbol);
            }
            Ok(unit)
        };
        units.push(parse().map_err(|e| format!("{}: {e}", input.name))?);
    }
    let mut globals: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut commons: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (ui, u) in units.iter().enumerate() {
        for (&si, s) in &u.symbols {
            if !s.global() || s.rank() == 0 {
                continue;
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
    let mut common_locations = BTreeMap::new();
    let mut common_sizes = BTreeMap::new();
    let mut common = Section {
        name: ".bss.common".into(),
        flags: 3,
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
    let mut imports: BTreeMap<String, dynamic::Import> = BTreeMap::new();
    for u in &units {
        for s in u.symbols.values() {
            if !s.global() || s.section != SymbolSection::Undefined || globals.contains_key(&s.name)
            {
                continue;
            }
            let export = libraries
                .iter()
                .enumerate()
                .find_map(|(i, l)| l.exports.get(&s.name).map(|e| (i, e)));
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
                Some((i, e)) => (Some(i), e.kind, e.version.clone()),
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
    let imported = |ui: usize, si: usize| -> Result<Option<&str>> {
        let s = units[ui]
            .symbols
            .get(&si)
            .ok_or("relocation references an invalid symbol index")?;
        Ok((s.global() && imports.contains_key(&s.name)).then_some(s.name.as_str()))
    };
    // GOT relocations retain their original instruction; no relaxation needed.
    // Keys use resolved definitions, so equal local names remain independent.
    let canonical = |u: usize, s: usize| -> Result<(usize, usize)> {
        let symbol = units[u]
            .symbols
            .get(&s)
            .ok_or("relocation references an invalid symbol index")?;
        Ok(if symbol.global() {
            globals.get(&symbol.name).copied().unwrap_or((u, s))
        } else {
            (u, s)
        })
    };
    let mut got = BTreeMap::new();
    for (ui, u) in units.iter().enumerate() {
        for r in &u.relocs {
            if matches!(r.kind, 9 | 41 | 42) && imported(ui, r.symbol)?.is_none() {
                let key = canonical(ui, r.symbol)?;
                let next = got.len() as u64 * 8;
                got.entry(key).or_insert(next);
            }
        }
    }
    let got_section = sections.len();
    let got_size = end(0, got.len() as u64 * 8)?;
    sections.push(Section {
        name: ".got".into(),
        flags: 2,
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
                if let Some(name) = imported(ui, r.symbol)? {
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
        Some(dynamic::Dynamic::prepare(
            &mut sections,
            imports.clone(),
            definitions,
            &libraries,
            &options.interpreter,
            &options.runpath,
            absolute,
        )?)
    };
    let mut segments = Vec::new();
    // Header space holds three LOADs and a non-executable GNU_STACK header.
    let mut cursor = PAGE;
    for permissions in [4, 5, 6] {
        let start = if permissions == 4 {
            0
        } else {
            align(cursor, PAGE)?
        };
        cursor = if permissions == 4 { PAGE } else { start };
        let mut file_end = cursor;
        for bss in [false, true] {
            for section in sections
                .iter_mut()
                .filter(|s| s.permissions() == permissions && s.bss == bss)
            {
                cursor = align(cursor, section.align)?;
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
    let address = |ui: usize, si: usize| -> Result<u64> {
        let (ui, si) = canonical(ui, si)?;
        let s = &units[ui].symbols[&si];
        match s.section {
            SymbolSection::Absolute => Ok(s.value),
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
    let file_end = segments.iter().map(|s| s.file_end).max().unwrap_or(PAGE);
    let mut bytes = vec![0; file_end as usize];
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
            let import = imported(ui, r.symbol)?;
            if let (Some(name), Some(d)) = (import, dynamic.as_mut())
                && r.kind == 1
            {
                let offset = sections[r.section].offset + r.offset;
                d.relocations
                    .push((BASE + offset, d.imports[name].index, 1, r.addend));
                bytes[offset as usize..offset as usize + 8].fill(0);
                continue;
            }
            let result = || -> Result<(usize, Vec<u8>)> {
                let offset = sections[r.section].offset + r.offset;
                let p = i128::from(BASE + offset);
                let value = if let (Some(name), Some(d)) = (import, dynamic.as_ref()) {
                    if matches!(r.kind, 9 | 41 | 42) {
                        i128::from(d.slot(name, &sections))
                    } else {
                        i128::from(d.stub(name, &sections)?)
                    }
                } else if matches!(r.kind, 9 | 41 | 42) {
                    i128::from(BASE + sections[got_section].offset + got[&canonical(ui, r.symbol)?])
                } else {
                    i128::from(address(ui, r.symbol)?)
                } + i128::from(r.addend);
                let data = match r.kind {
                    1 => Some((value as u64).to_le_bytes().to_vec()),
                    10 => u32::try_from(value).map(|v| v.to_le_bytes().to_vec()).ok(),
                    11 => i32::try_from(value as u64 as i64)
                        .map(|v| v.to_le_bytes().to_vec())
                        .ok(),
                    24 => i64::try_from(value - p)
                        .map(|v| v.to_le_bytes().to_vec())
                        .ok(),
                    _ => i32::try_from(value - p)
                        .map(|v| v.to_le_bytes().to_vec())
                        .ok(),
                }
                .ok_or_else(|| {
                    format!(
                        "relocation {} overflow for `{}`",
                        r.kind, u.symbols[&r.symbol].name
                    )
                })?;
                Ok((offset as usize, data))
            };
            let (offset, data) = result().map_err(|e| format!("{}: {e}", u.name))?;
            bytes[offset..offset + data.len()].copy_from_slice(&data);
        }
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
            definitions.insert(name.clone(), (address(ui, si)?, index as u16));
        }
        d.emit(&mut bytes, &sections, &definitions)?;
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
            .unwrap_or((if s.bss { 8 } else { 1 }, 0, 0, 0));
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
        put64(&mut record, 8, address(ui, si)?);
        put64(
            &mut record,
            16,
            common_sizes.get(name).copied().unwrap_or(s.size),
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
        1,
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
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 62);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, entry);
    put64(&mut bytes, 32, 64);
    put64(&mut bytes, 40, shoff);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(
        &mut bytes,
        56,
        (segments.len() + 1 + if dynamic.is_some() { 3 } else { 0 }) as u16,
    );
    put16(&mut bytes, 58, 64);
    put16(&mut bytes, 60, headers.len() as u16);
    put16(&mut bytes, 62, (headers.len() - 1) as u16);
    let mut phdrs = Vec::new();
    let phnum = segments.len() + 1 + if dynamic.is_some() { 3 } else { 0 };
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
            PAGE,
        ));
    }
    if let Some(d) = &dynamic {
        let s = &sections[d.table];
        phdrs.push((2, 6, s.offset, s.size, s.size, 8));
    }
    phdrs.push((0x6474e551, 6, 0, 0, 0, 16));
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
