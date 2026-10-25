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
    ) || file.architecture() != object::Architecture::Aarch64
        || !file.is_little_endian()
    {
        return Err("disasm supports little-endian AArch64 Mach-O and ELF objects".into());
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
        for (offset, chunk) in bytes.chunks(4).enumerate() {
            let address = section.address().wrapping_add((offset as u64) * 4);
            for (_, name) in symbols.iter().filter(|(value, _)| *value == address) {
                writeln!(output, "{address:016x} <{name}>:").unwrap();
            }
            write!(output, "  {address:016x}: ").unwrap();
            for index in 0..4 {
                if let Some(byte) = chunk.get(index) {
                    write!(output, "{byte:02x} ").unwrap();
                } else {
                    output.push_str("   ");
                }
            }
            if let Ok(bytes) = <[u8; 4]>::try_from(chunk) {
                let inst = keel_disasm::decode_word(bytes, address);
                writeln!(output, " {}", inst.assembly()).unwrap();
            } else {
                let values: Vec<_> = chunk.iter().map(|b| format!("0x{b:02x}")).collect();
                writeln!(output, " .byte {}", values.join(", ")).unwrap();
            }
        }
    }
    // Release mapped input before writing, including when input == output.
    drop(file);
    drop(source);
    super::write_output(&opts, &output)
}
