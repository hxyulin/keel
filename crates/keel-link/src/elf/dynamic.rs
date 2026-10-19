//! Shared-library interfaces and eagerly bound dynamic executable metadata.
use super::{BASE, Section, lifecycle, put16, put32, put64, string};
use crate::Result;
use object::read::elf::{Dyn, ElfFile64, FileHeader};
use object::{Object, ObjectSymbol, SymbolFlags};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Export {
    pub kind: u8,
    pub version: Option<String>,
}
pub(super) struct Library {
    pub needed: String,
    pub exports: BTreeMap<String, Export>,
    pub required: BTreeSet<String>,
}
impl Library {
    pub fn parse(name: &str, bytes: &[u8]) -> Result<Self> {
        let file = ElfFile64::<object::Endianness>::parse(bytes).map_err(|e| e.to_string())?;
        if file.architecture() != object::Architecture::X86_64
            || !file.is_little_endian()
            || file.kind() != object::ObjectKind::Dynamic
        {
            return Err("expected an x86-64 little-endian ELF shared library".into());
        }
        if !matches!(file.elf_header().e_ident().os_abi, 0 | 3)
            || file.elf_header().e_flags(file.endian()) != 0
        {
            return Err("unsupported shared-library OS ABI or machine flags".into());
        }
        let mut needed = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("shared library needs a UTF-8 basename")?
            .to_owned();
        let (entries, _) = file
            .elf_section_table()
            .dynamic(file.endian(), bytes)
            .map_err(|e| e.to_string())?
            .ok_or("shared library is missing its dynamic table")?;
        if entries.iter().any(|e| {
            e.tag32(file.endian()) == Some(object::elf::DT_FLAGS_1)
                && e.d_val(file.endian()) & u64::from(object::elf::DF_1_PIE) != 0
        }) {
            return Err("PIE executables cannot be used as shared libraries".into());
        }
        let strings = file.elf_dynamic_symbol_table().strings();
        if !entries
            .iter()
            .any(|e| e.tag32(file.endian()) == Some(object::elf::DT_NULL))
        {
            return Err("unterminated shared-library dynamic table".into());
        }
        for entry in entries
            .iter()
            .take_while(|e| e.tag32(file.endian()) != Some(object::elf::DT_NULL))
        {
            if entry.tag32(file.endian()) == Some(object::elf::DT_SONAME) {
                needed = std::str::from_utf8(
                    entry
                        .string(file.endian(), strings)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?
                .into();
            }
        }
        if needed.is_empty() || needed.contains('\0') {
            return Err("invalid shared library SONAME".into());
        }
        let versions = file
            .elf_section_table()
            .versions(file.endian(), bytes)
            .map_err(|e| e.to_string())?;
        let mut exports = BTreeMap::new();
        let mut required = BTreeSet::new();
        for s in file.dynamic_symbols() {
            if s.is_undefined() && s.is_global() && !s.is_weak() {
                required.insert(s.name().map_err(|e| e.to_string())?.to_owned());
            }
            if s.is_undefined() || (!s.is_global() && !s.is_weak()) {
                continue;
            }
            let SymbolFlags::Elf { st_info, st_other } = s.flags() else {
                unreachable!()
            };
            if !matches!(st_other & 3, 0 | 3) {
                continue;
            }
            let version = if let Some(table) = &versions {
                let index = table.version_index(file.endian(), s.index());
                if index.is_hidden() || index.is_local() {
                    continue;
                }
                table
                    .version(index)
                    .map_err(|e| e.to_string())?
                    .map(|v| {
                        std::str::from_utf8(v.name())
                            .map(str::to_owned)
                            .map_err(|e| e.to_string())
                    })
                    .transpose()?
            } else {
                None
            };
            exports.insert(
                s.name().map_err(|e| e.to_string())?.into(),
                Export {
                    kind: st_info & 15,
                    version,
                },
            );
        }
        Ok(Self {
            needed,
            exports,
            required,
        })
    }
}
#[derive(Clone)]
pub(super) struct Import {
    pub index: u32,
    pub kind: u8,
    pub weak: bool,
    pub library: Option<usize>,
    pub version: Option<String>,
}
pub(super) struct Definition {
    pub name: String,
    pub info: u8,
    pub other: u8,
    pub size: u64,
}
/// Each synthesized section carries its ELF header metadata separately from
/// normal PROGBITS/NOBITS contributions.
struct Header {
    index: usize,
    kind: u32,
    link: u32,
    info: u32,
    entsize: u64,
}
pub(super) struct Dynamic {
    pub imports: BTreeMap<String, Import>,
    definitions: Vec<Definition>,
    pub stubs: BTreeMap<String, u64>,
    pub slots: BTreeMap<String, u64>,
    pub interp: usize,
    pub table: usize,
    pub got: usize,
    plt: usize,
    dynstr: usize,
    dynsym: usize,
    hash: usize,
    rela: usize,
    versym: usize,
    verneed: usize,
    headers: Vec<Header>,
    strings: Vec<u8>,
    symbol_names: BTreeMap<String, u32>,
    needed: Vec<u32>,
    runpath: Option<u32>,
    version_indices: BTreeMap<(usize, String), u16>,
    version_names: BTreeMap<(usize, String), u32>,
    pub relocations: Vec<(u64, u32, u32, i64)>,
    expected_relocations: usize,
    lifecycle: lifecycle::Plan,
}
impl Dynamic {
    pub fn prepare(
        sections: &mut Vec<Section>,
        imports: BTreeMap<String, Import>,
        definitions: Vec<Definition>,
        libraries: &[Library],
        options: &super::Options,
        absolute_relocations: usize,
        lifecycle: lifecycle::Plan,
    ) -> Result<Self> {
        let interpreter = &options.interpreter;
        let runpath = &options.runpath;
        let lifecycle_tags = lifecycle.tag_count(sections);
        if !interpreter.starts_with('/') || interpreter.contains('\0') {
            return Err("ELF interpreter must be an absolute path without NUL bytes".into());
        }
        if runpath.iter().any(|s| s.contains('\0')) {
            return Err("invalid runtime library path".into());
        }
        let mut strings = vec![0];
        let needed = libraries
            .iter()
            .map(|l| string(&mut strings, &l.needed))
            .collect::<Vec<_>>();
        let runpath = (!runpath.is_empty()).then(|| string(&mut strings, &runpath.join(":")));
        let symbol_names = imports
            .keys()
            .chain(definitions.iter().map(|d| &d.name))
            .map(|n| (n.clone(), string(&mut strings, n)))
            .collect();
        let symbol_count = imports.len() + definitions.len() + 1;
        let mut version_indices = BTreeMap::new();
        let mut version_names = BTreeMap::new();
        for import in imports.values() {
            if let (Some(lib), Some(version)) = (import.library, &import.version) {
                let key = (lib, version.clone());
                if !version_indices.contains_key(&key) {
                    let index = version_indices.len() + 2;
                    if index >= 0x8000 {
                        return Err("too many symbol versions".into());
                    }
                    version_indices.insert(key.clone(), index as u16);
                    version_names.insert(key, string(&mut strings, version));
                }
            }
        }
        let mut headers = Vec::new();
        let mut add = |name: &str,
                       kind: u32,
                       flags: u64,
                       alignment: u64,
                       data: Vec<u8>,
                       link: u32,
                       info: u32,
                       entsize: u64| {
            let index = sections.len();
            sections.push(Section {
                name: name.into(),
                flags,
                kind,
                align: alignment,
                size: data.len() as u64,
                bss: false,
                data,
                offset: 0,
            });
            headers.push(Header {
                index,
                kind,
                link,
                info,
                entsize,
            });
            index
        };
        let mut interp_data = interpreter.as_bytes().to_vec();
        interp_data.push(0);
        let interp = add(".interp", 1, 2, 1, interp_data, 0, 0, 0);
        let dynstr = add(".dynstr", 3, 2, 1, strings.clone(), 0, 0, 0);
        let dynsym = add(
            ".dynsym",
            11,
            2,
            8,
            vec![0; symbol_count * 24],
            dynstr as u32 + 1,
            1,
            24,
        );
        let hash = add(
            ".hash",
            5,
            2,
            4,
            vec![0; (3 + symbol_count) * 4],
            dynsym as u32 + 1,
            0,
            4,
        );
        let versym = add(
            ".gnu.version",
            0x6fffffff,
            2,
            2,
            vec![0; symbol_count * 2],
            dynsym as u32 + 1,
            0,
            2,
        );
        let version_libraries = version_indices
            .keys()
            .map(|(l, _)| *l)
            .collect::<std::collections::BTreeSet<_>>();
        let verneed_size = version_libraries.len() * 16 + version_indices.len() * 16;
        let verneed = add(
            ".gnu.version_r",
            0x6ffffffe,
            2,
            4,
            vec![0; verneed_size],
            dynstr as u32 + 1,
            version_libraries.len() as u32,
            0,
        );
        // One GOT slot per import. Function stubs jump through the same slot as
        // address-taking code, preserving function pointer identity.
        let slots = imports
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u64 * 8))
            .collect();
        let stubs = imports
            .iter()
            .filter(|(_, i)| matches!(i.kind, 0 | 2 | 10))
            .enumerate()
            .map(|(i, (n, _))| (n.clone(), i as u64 * 8))
            .collect::<BTreeMap<_, _>>();
        let got = add(".got.imports", 1, 3, 8, vec![0; imports.len() * 8], 0, 0, 0);
        let plt = add(".plt", 1, 6, 8, vec![0; stubs.len() * 8], 0, 0, 8);
        let expected_relocations = imports.len() + absolute_relocations;
        let rela = add(
            ".rela.dyn",
            4,
            2,
            8,
            vec![0; expected_relocations * 24],
            dynsym as u32 + 1,
            0,
            24,
        );
        // DT_NEEDED + eight basic tags + version tags + RUNPATH + NULL.
        let count = libraries.len()
            + 9
            + lifecycle_tags
            + if version_indices.is_empty() { 0 } else { 3 }
            + usize::from(runpath.is_some());
        let table = add(
            ".dynamic",
            6,
            3,
            8,
            vec![0; count * 16],
            dynstr as u32 + 1,
            0,
            16,
        );
        Ok(Self {
            imports,
            definitions,
            stubs,
            slots,
            interp,
            table,
            got,
            plt,
            dynstr,
            dynsym,
            hash,
            rela,
            versym,
            verneed,
            headers,
            strings,
            symbol_names,
            needed,
            runpath,
            version_indices,
            version_names,
            relocations: Vec::new(),
            expected_relocations,
            lifecycle,
        })
    }
    pub fn stub(&self, name: &str, sections: &[Section]) -> Result<u64> {
        self.stubs.get(name).map(|o|BASE+sections[self.plt].offset+o).ok_or_else(||format!("imported data `{name}` needs a GOT relocation; copy relocations are unsupported"))
    }
    pub fn slot(&self, name: &str, sections: &[Section]) -> u64 {
        BASE + sections[self.got].offset + self.slots[name]
    }
    pub fn section_header(&self, index: usize) -> Option<(u32, u32, u32, u64)> {
        self.headers
            .iter()
            .find(|h| h.index == index)
            .map(|h| (h.kind, h.link, h.info, h.entsize))
    }
    pub fn emit(
        &mut self,
        bytes: &mut [u8],
        sections: &[Section],
        definitions: &BTreeMap<String, (u64, u16)>,
    ) -> Result<()> {
        let at = |i: usize| sections[i].offset as usize;
        let symbol_count = self.imports.len() + self.definitions.len() + 1;
        let mut dynsym = vec![0; symbol_count * 24];
        let mut versym = vec![0; symbol_count * 2];
        for (name, import) in &self.imports {
            let i = import.index as usize;
            put32(&mut dynsym, i * 24, self.symbol_names[name]);
            dynsym[i * 24 + 4] = (if import.weak { 2 } else { 1 }) << 4
                | if import.kind == 10 { 2 } else { import.kind };
            let version = match (import.library, &import.version) {
                (Some(l), Some(v)) => self.version_indices[&(l, v.clone())],
                _ => 1,
            };
            put16(&mut versym, i * 2, version);
            self.relocations
                .push((self.slot(name, sections), import.index, 6, 0));
        }
        for (i, d) in self.definitions.iter().enumerate() {
            let i = i + self.imports.len() + 1;
            let (value, section) = definitions
                .get(&d.name)
                .ok_or("missing exported definition")?;
            put32(&mut dynsym, i * 24, self.symbol_names[&d.name]);
            dynsym[i * 24 + 4] = d.info;
            dynsym[i * 24 + 5] = d.other;
            put16(&mut dynsym, i * 24 + 6, *section);
            put64(&mut dynsym, i * 24 + 8, *value);
            put64(&mut dynsym, i * 24 + 16, d.size);
            put16(&mut versym, i * 2, 1);
        }
        bytes[at(self.dynsym)..at(self.dynsym) + dynsym.len()].copy_from_slice(&dynsym);
        bytes[at(self.versym)..at(self.versym) + versym.len()].copy_from_slice(&versym);
        // A single SysV hash bucket is sufficient for correctness. This bounded
        // initial writer prioritizes valid lookup over hash-table performance.
        let hash = at(self.hash);
        put32(bytes, hash, 1);
        put32(bytes, hash + 4, symbol_count as u32);
        if symbol_count > 1 {
            put32(bytes, hash + 8, 1);
        }
        for i in 1..symbol_count - 1 {
            put32(bytes, hash + 12 + i * 4, (i + 1) as u32);
        }
        for (name, offset) in &self.stubs {
            let start = at(self.plt) + *offset as usize;
            let displacement = i32::try_from(
                i128::from(self.slot(name, sections)) - i128::from(BASE + start as u64 + 6),
            )
            .map_err(|_| "import stub displacement overflow")?;
            bytes[start..start + 2].copy_from_slice(&[0xff, 0x25]);
            put32(bytes, start + 2, displacement as u32);
            bytes[start + 6..start + 8].copy_from_slice(&[0x66, 0x90]);
        }
        let groups = self
            .version_indices
            .keys()
            .map(|(l, _)| *l)
            .collect::<std::collections::BTreeSet<_>>();
        let mut cursor = at(self.verneed);
        for (gi, lib) in groups.iter().enumerate() {
            let entries = self
                .version_indices
                .iter()
                .filter(|((l, _), _)| l == lib)
                .collect::<Vec<_>>();
            let length = 16 + entries.len() * 16;
            put16(bytes, cursor, 1);
            put16(bytes, cursor + 2, entries.len() as u16);
            put32(bytes, cursor + 4, self.needed[*lib]);
            put32(bytes, cursor + 8, 16);
            put32(
                bytes,
                cursor + 12,
                if gi + 1 == groups.len() {
                    0
                } else {
                    length as u32
                },
            );
            for (i, ((lib, version), index)) in entries.iter().enumerate() {
                let aux = cursor + 16 + i * 16;
                put32(bytes, aux, elf_hash(version.as_bytes()));
                put16(bytes, aux + 6, **index);
                put32(bytes, aux + 8, self.version_names[&(*lib, version.clone())]);
                put32(bytes, aux + 12, if i + 1 == entries.len() { 0 } else { 16 });
            }
            cursor += length;
        }
        if self.relocations.len() != self.expected_relocations {
            return Err("internal dynamic relocation count mismatch".into());
        }
        for (i, (offset, symbol, kind, addend)) in self.relocations.iter().enumerate() {
            let r = at(self.rela) + i * 24;
            put64(bytes, r, *offset);
            put64(bytes, r + 8, u64::from(*symbol) << 32 | u64::from(*kind));
            put64(bytes, r + 16, *addend as u64);
        }
        let mut tags = self
            .needed
            .iter()
            .map(|n| (1, u64::from(*n)))
            .collect::<Vec<_>>();
        tags.extend([
            (4, BASE + sections[self.hash].offset),
            (5, BASE + sections[self.dynstr].offset),
            (6, BASE + sections[self.dynsym].offset),
            (7, BASE + sections[self.rela].offset),
            (8, sections[self.rela].size),
            (9, 24),
            (10, self.strings.len() as u64),
            (11, 24),
        ]);
        if !self.version_indices.is_empty() {
            tags.extend([
                (0x6ffffff0, BASE + sections[self.versym].offset),
                (0x6ffffffe, BASE + sections[self.verneed].offset),
                (0x6fffffff, groups.len() as u64),
            ]);
        }
        if let Some(runpath) = self.runpath {
            tags.push((29, u64::from(runpath)));
        }
        tags.extend(self.lifecycle.tags(sections)?);
        tags.push((0, 0));
        if tags.len() * 16 != sections[self.table].size as usize {
            return Err("internal dynamic table size mismatch".into());
        }
        for (i, (tag, value)) in tags.iter().enumerate() {
            put64(bytes, at(self.table) + i * 16, *tag);
            put64(bytes, at(self.table) + i * 16 + 8, *value);
        }
        Ok(())
    }
}
fn elf_hash(name: &[u8]) -> u32 {
    let mut hash = 0u32;
    for &b in name {
        hash = (hash << 4).wrapping_add(u32::from(b));
        let high = hash & 0xf0000000;
        if high != 0 {
            hash ^= high >> 24;
        }
        hash &= !high;
    }
    hash
}
