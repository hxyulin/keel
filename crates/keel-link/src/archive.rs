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
            // Generic readers intentionally suppress some malformed load-command
            // and relocation-table errors. Force their fallible raw accessors.
            match &file {
                object::File::MachO64(macho) => {
                    let mut commands = macho.macho_load_commands().map_err(|e| e.to_string())?;
                    while commands.next().map_err(|e| e.to_string())?.is_some() {}
                    for section in macho.sections() {
                        section.macho_relocations().map_err(|e| e.to_string())?;
                    }
                }
                object::File::Elf64(elf) => {
                    use object::read::elf::SectionHeader;
                    for section in elf.elf_section_table().iter() {
                        let entry_size = match section.sh_type(elf.endian()) {
                            object::elf::SHT_SYMTAB | object::elf::SHT_DYNSYM => 24,
                            object::elf::SHT_RELA => 24,
                            object::elf::SHT_REL => 16,
                            _ => continue,
                        };
                        if section.sh_entsize(elf.endian()) != entry_size
                            || section.sh_size(elf.endian()) % entry_size != 0
                        {
                            return Err("invalid ELF table entry size".into());
                        }
                    }
                }
                _ => unreachable!("format and width checked above"),
            }
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
