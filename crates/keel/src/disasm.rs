//! Object-file presentation; the byte decoder has no object or compiler dependency.
use object::{Object, ObjectSection, ObjectSymbol};
use std::fmt::Write;

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let opts = super::parse_options(args, &["-o"])?;
    let input = opts
        .input
        .as_ref()
        .ok_or("usage: keel disasm <input.o> [-o output.txt]")?;
    let source = super::input::Source::open(input).map_err(|e| format!("{input}: {e}"))?;
    let file = object::File::parse(source.bytes()).map_err(|e| format!("{input}: {e}"))?;
    if !matches!(
        file.format(),
        object::BinaryFormat::Elf | object::BinaryFormat::MachO
    ) || !matches!(
        file.architecture(),
        object::Architecture::Aarch64 | object::Architecture::X86_64
    ) || !file.is_little_endian()
    {
        return Err(
            "disasm supports little-endian AArch64 and x86-64 Mach-O and ELF objects".into(),
        );
    }
    let mut output = String::new();
    for section in file
        .sections()
        .filter(|s| s.kind() == object::SectionKind::Text)
    {
        let bytes = section.data().map_err(|e| format!("{input}: {e}"))?;
        if bytes.is_empty() {
            continue;
        }
        let name = section.name().map_err(|e| e.to_string())?;
        writeln!(output, "\n{name}:").unwrap();
        let mut symbols: Vec<_> = file
            .symbols()
            .filter(|s| {
                s.section_index() == Some(section.index())
                    && !s.is_undefined()
                    && matches!(
                        s.kind(),
                        object::SymbolKind::Text | object::SymbolKind::Unknown
                    )
            })
            .filter_map(|s| Some((s.address(), s.name().ok()?.to_owned())))
            .filter(|(_, name)| {
                !name.is_empty() && !name.starts_with("$x") && !name.starts_with("$d")
            })
            .collect();
        symbols.sort();
        symbols.dedup();
        let arm = file.architecture() == object::Architecture::Aarch64;
        let mut offset = 0;
        let mut code = true;
        while offset < bytes.len() {
            let address = section.address().wrapping_add(offset as u64);
            let (length, assembly) = if arm {
                let length = (bytes.len() - offset).min(4);
                let assembly = <[u8; 4]>::try_from(&bytes[offset..offset + length])
                    .ok()
                    .map(|word| keel_disasm::decode_word(word, address).assembly());
                (length, assembly)
            } else if code {
                if let Some(instruction) = keel_disasm::x86::decode(&bytes[offset..], address) {
                    (instruction.bytes.len(), Some(instruction.assembly()))
                } else {
                    // An unknown x86 instruction has no trustworthy length.
                    // Keep the rest as data rather than invent boundaries.
                    code = false;
                    ((bytes.len() - offset).min(15), None)
                }
            } else {
                ((bytes.len() - offset).min(15), None)
            };
            let chunk = &bytes[offset..offset + length];
            for (_, name) in symbols.iter().filter(|(value, _)| *value == address) {
                writeln!(output, "{address:016x} <{name}>:").unwrap();
            }
            write!(output, "  {address:016x}: ").unwrap();
            for index in 0..if arm { 4 } else { 15 } {
                if let Some(byte) = chunk.get(index) {
                    write!(output, "{byte:02x} ").unwrap();
                } else {
                    output.push_str("   ");
                }
            }
            if let Some(assembly) = assembly {
                writeln!(output, " {assembly}").unwrap();
            } else {
                let values: Vec<_> = chunk.iter().map(|b| format!("0x{b:02x}")).collect();
                writeln!(output, " .byte {}", values.join(", ")).unwrap();
            }
            offset += length;
        }
    }
    // Release mapped input before writing, including when input == output.
    drop(file);
    drop(source);
    super::write_output(&opts, &output)
}
