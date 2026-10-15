//! Deterministic regular static archives. Object payloads are copied verbatim.
use crate::{Input, Result};
use object::{Object, ObjectSection, ObjectSymbol};
use std::path::Path;

/// The target selects both object validation and the archive/index dialect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    X86_64LinuxGnu,
    Aarch64AppleDarwin,
}

/// Package relocatable objects in input order, retaining duplicate basenames.
/// Names are UTF-8 paths; only their final component is stored in the archive.
/// Metadata is always deterministic (zero timestamp/uid/gid, mode 0644).
/// Index offsets are 32-bit; archives exceeding that representation are rejected.
pub fn build(inputs: &[Input<'_>], target: Target) -> Result<Vec<u8>> {
    let darwin = target == Target::Aarch64AppleDarwin;
    let mut names = Vec::new();
    let mut symbols = Vec::new();
    for (member, input) in inputs.iter().enumerate() {
        let name = Path::new(input.name)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{}: invalid archive member name", input.name))?;
        if name.is_empty()
            || name.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r')
            || name.starts_with("__.SYMDEF")
            || matches!(name, "/" | "//")
        {
            return Err(format!(
                "{}: reserved or invalid archive member name",
                input.name
            ));
        }
        names.push(name);
        let mut validate = || -> Result<()> {
            let file = object::File::parse(input.bytes).map_err(|e| e.to_string())?;
            let (format, architecture) = if darwin {
                (object::BinaryFormat::MachO, object::Architecture::Aarch64)
            } else {
                (object::BinaryFormat::Elf, object::Architecture::X86_64)
            };
            if file.format() != format
                || file.architecture() != architecture
                || file.kind() != object::ObjectKind::Relocatable
                || !file.is_64()
                || !file.is_little_endian()
            {
                return Err(format!(
                    "expected {target:?} little-endian 64-bit relocatable object"
                ));
            }
            validate_raw(&file, input.bytes)?;
            // Parsing is lazy: force section payload and symbol/name bounds checks.
            for section in file.sections() {
                section.name_bytes().map_err(|e| e.to_string())?;
                section.data().map_err(|e| e.to_string())?;
            }
            for symbol in file.symbols() {
                if let object::SymbolSection::Section(index) = symbol.section() {
                    file.section_by_index(index).map_err(|e| e.to_string())?;
                }
                let name = symbol.name_bytes().map_err(|e| e.to_string())?;
                if symbol.is_global()
                    && (!symbol.is_undefined()
                        || symbol.is_common()
                        // object 0.36 reports Mach-O N_UNDF commons as undefined
                        // and is_common=false; n_value carries their size.
                        || (darwin && symbol.is_undefined() && symbol.address() != 0))
                    && !name.is_empty()
                    && !matches!(
                        symbol.kind(),
                        object::SymbolKind::File | object::SymbolKind::Section
                    )
                {
                    if name.contains(&0) {
                        return Err("NUL in symbol name".into());
                    }
                    symbols.push((member, name.to_vec()));
                }
            }
            Ok(())
        };
        validate().map_err(|e| format!("{}: {e}", input.name))?;
    }
    let mut long_names = Vec::new();
    let mut headers = Vec::new();
    for name in &names {
        if darwin {
            headers.push(String::new());
        } else if name.len() <= 15
            && name.is_ascii()
            && *name != "#1"
            && !name.bytes().any(|b| b == b'/' || b.is_ascii_whitespace())
        {
            headers.push(format!("{name}/"));
        } else {
            headers.push(format!("/{}", long_names.len()));
            long_names.extend_from_slice(name.as_bytes());
            long_names.extend_from_slice(b"/\n");
        }
    }
    let string_size = symbols.iter().try_fold(0usize, |n, (_, s)| {
        n.checked_add(s.len() + 1).ok_or("archive index overflow")
    })?;
    let index_size = if darwin {
        symbols
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(8 + string_size))
    } else {
        symbols
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(4 + string_size))
    }
    .ok_or("archive index overflow")?;
    let mut out = b"!<arch>\n".to_vec();
    let index_data = append(
        &mut out,
        if darwin { "__.SYMDEF" } else { "/" },
        &vec![0; index_size],
        darwin,
    )?;
    if !long_names.is_empty() {
        append(&mut out, "//", &long_names, false)?;
    }
    let mut offsets = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        offsets.push(u32::try_from(out.len()).map_err(|_| "archive exceeds 32-bit index offsets")?);
        append(
            &mut out,
            if darwin { names[i] } else { &headers[i] },
            input.bytes,
            darwin,
        )?;
    }
    u32::try_from(out.len()).map_err(|_| "archive exceeds 32-bit index offsets")?;
    let mut index = Vec::with_capacity(index_size);
    if darwin {
        index.extend_from_slice(
            &u32::try_from(symbols.len() * 8)
                .map_err(|_| "index overflow")?
                .to_le_bytes(),
        );
        let mut strx = 0u32;
        for (member, name) in &symbols {
            index.extend_from_slice(&strx.to_le_bytes());
            index.extend_from_slice(&offsets[*member].to_le_bytes());
            strx = strx
                .checked_add(u32::try_from(name.len() + 1).map_err(|_| "index overflow")?)
                .ok_or("index overflow")?;
        }
        index.extend_from_slice(
            &u32::try_from(string_size)
                .map_err(|_| "index overflow")?
                .to_le_bytes(),
        );
    } else {
        index.extend_from_slice(
            &u32::try_from(symbols.len())
                .map_err(|_| "index overflow")?
                .to_be_bytes(),
        );
        for (member, _) in &symbols {
            index.extend_from_slice(&offsets[*member].to_be_bytes());
        }
    }
    for (_, name) in symbols {
        index.extend_from_slice(&name);
        index.push(0);
    }
    out[index_data..index_data + index_size].copy_from_slice(&index);
    Ok(out)
}

// Return the payload start (after an optional BSD extended name). Darwin object
// payloads are aligned to eight bytes by NUL-padding the extended name.
fn append(out: &mut Vec<u8>, name: &str, bytes: &[u8], darwin: bool) -> Result<usize> {
    let name_size = if darwin {
        name.len() + (8 - (out.len() + 60 + name.len()) % 8) % 8
    } else {
        0
    };
    let label = if darwin {
        format!("#1/{name_size}")
    } else {
        name.to_owned()
    };
    let size = bytes
        .len()
        .checked_add(name_size)
        .ok_or("archive member size overflow")?;
    if label.len() > 16 || size > 9_999_999_999 {
        return Err("archive member header overflow".into());
    }
    out.extend_from_slice(
        format!(
            "{label:<16}{:<12}{:<6}{:<6}{:<8}{size:<10}`\n",
            0, 0, 0, "644"
        )
        .as_bytes(),
    );
    if darwin {
        out.extend_from_slice(name.as_bytes());
        out.resize(out.len() + name_size - name.len(), 0);
    }
    let data = out.len();
    out.extend_from_slice(bytes);
    if !out.len().is_multiple_of(2) {
        out.push(b'\n');
    }
    Ok(data)
}

/// Validate/build in memory, then atomically replace a destination using a
/// create-new temporary file in the same directory. Failures preserve output.
pub fn write(inputs: &[Input<'_>], target: Target, output: &Path) -> Result<()> {
    use std::io::Write;
    let bytes = build(inputs, target)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = output.with_file_name(format!(".keel-archive-{}-{stamp}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("{}: {e}", temporary.display()))?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        drop(file);
        std::fs::rename(&temporary, output)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("{}: {e}", output.display()))
}

// Generic ObjectSymbol::section() deliberately maps malformed references and
// valid format-specific special symbols to Unknown. Validate the raw values,
// rather than rejecting Unknown (which would reject e.g. Mach-O N_INDR).
fn validate_raw(file: &object::File<'_>, bytes: &[u8]) -> Result<()> {
    match file {
        object::File::Elf64(elf) => {
            use object::elf::*;
            use object::read::elf::{SectionHeader, Sym};
            let endian = elf.endian();
            let sections = elf.elf_section_table();
            for (index, section) in sections.enumerate() {
                let kind = section.sh_type(endian);
                let entry_size = match kind {
                    SHT_SYMTAB | SHT_DYNSYM | SHT_RELA => 24,
                    SHT_REL => 16,
                    SHT_SYMTAB_SHNDX => 4,
                    _ => continue,
                };
                if section.sh_entsize(endian) != entry_size
                    || section.sh_size(endian) % entry_size != 0
                {
                    return Err("invalid ELF table entry size".into());
                }
                if kind == SHT_SYMTAB || kind == SHT_DYNSYM {
                    let symbols = sections
                        .symbol_table_by_index(endian, bytes, index)
                        .map_err(|e| e.to_string())?;
                    for (i, symbol) in symbols.enumerate() {
                        symbols
                            .symbol_name(endian, symbol)
                            .map_err(|e| e.to_string())?;
                        match symbol.st_shndx(endian) {
                            SHN_UNDEF | SHN_ABS | SHN_COMMON => {}
                            // AMD64 ABI large common, not an ordinary section.
                            0xff02 => {}
                            SHN_XINDEX => {
                                let actual = symbols
                                    .shndx(endian, i)
                                    .ok_or("missing ELF extended symbol section index")?;
                                // Extended indices are actual section ordinals,
                                // never SHN_UNDEF/ABS/COMMON/reserved encodings.
                                if actual < u32::from(SHN_LORESERVE)
                                    || actual as usize >= sections.len()
                                {
                                    return Err("invalid ELF extended symbol section index".into());
                                }
                            }
                            n if n < SHN_LORESERVE && (n as usize) < sections.len() => {}
                            _ => {
                                return Err(
                                    "invalid or unsupported ELF symbol section index".into()
                                );
                            }
                        }
                    }
                } else if kind == SHT_SYMTAB_SHNDX {
                    let symbols = sections
                        .symbol_table_by_index(endian, bytes, section.link(endian))
                        .map_err(|e| e.to_string())?;
                    if section.sh_size(endian) / 4 != symbols.len() as u64
                        || symbols.shndx_section() != index
                    {
                        return Err("invalid or duplicate ELF extended index table".into());
                    }
                    for (i, symbol) in symbols.enumerate() {
                        if symbol.st_shndx(endian) != SHN_XINDEX
                            && symbols.shndx(endian, i) != Some(0)
                        {
                            return Err("nonzero unused ELF extended symbol index".into());
                        }
                    }
                } else {
                    let symbols = sections
                        .symbol_table_by_index(endian, bytes, section.link(endian))
                        .map_err(|e| e.to_string())?;
                    // ET_REL sh_info identifies the section being relocated.
                    sections
                        .section(object::SectionIndex(section.sh_info(endian) as usize))
                        .map_err(|e| e.to_string())?;
                    let data = section.data(endian, bytes).map_err(|e| e.to_string())?;
                    for relocation in data.chunks_exact(entry_size as usize) {
                        // Both Elf64_Rel and Elf64_Rela have r_info at byte 8.
                        let info = u64::from_le_bytes(relocation[8..16].try_into().unwrap());
                        if (info >> 32) as usize >= symbols.len() {
                            return Err("invalid ELF relocation symbol index".into());
                        }
                    }
                }
            }
        }
        object::File::MachO64(macho) => {
            use object::macho::*;
            use object::read::macho::Nlist;
            let mut commands = macho.macho_load_commands().map_err(|e| e.to_string())?;
            while commands.next().map_err(|e| e.to_string())?.is_some() {}
            let count = macho.sections().count();
            let symbols = macho.macho_symbol_table();
            for symbol in symbols.iter() {
                // STABS have their own n_type/section semantics.
                if !symbol.is_stab()
                    && symbol.n_type() & N_TYPE == N_SECT
                    && (symbol.n_sect() == 0 || symbol.n_sect() as usize > count)
                {
                    return Err("invalid Mach-O symbol section index".into());
                }
            }
            for section in macho.sections() {
                for relocation in section.macho_relocations().map_err(|e| e.to_string())? {
                    let info = relocation.info(macho.endian());
                    // ARM64_RELOC_ADDEND stores a signed 24-bit addend in
                    // r_symbolnum, not a symbol/section reference. Local zero
                    // is R_ABS. Other relocation types need no linker support.
                    if info.r_type == ARM64_RELOC_ADDEND {
                        continue;
                    }
                    if (info.r_extern && info.r_symbolnum as usize >= symbols.len())
                        || (!info.r_extern && info.r_symbolnum as usize > count)
                    {
                        return Err("invalid Mach-O relocation symbol or section index".into());
                    }
                }
            }
        }
        _ => unreachable!("format and width checked above"),
    }
    Ok(())
}
