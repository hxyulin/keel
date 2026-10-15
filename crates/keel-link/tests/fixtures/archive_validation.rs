// Shared raw fixtures keep library and CLI rejection coverage identical.
fn u32_at(b: &[u8], p: usize) -> usize {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize
}
fn u64_at(b: &[u8], p: usize) -> usize {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap()) as usize
}
fn put32(b: &mut [u8], p: usize, n: usize) {
    b[p..p + 4].copy_from_slice(&(n as u32).to_le_bytes());
}
fn put64(b: &mut [u8], p: usize, n: usize) {
    b[p..p + 8].copy_from_slice(&(n as u64).to_le_bytes());
}
fn elf_tables(b: &[u8]) -> (usize, usize, usize, usize) {
    let sh = u64_at(b, 40);
    let count = u16::from_le_bytes(b[60..62].try_into().unwrap()) as usize;
    let sym = (1..count)
        .map(|i| sh + i * 64)
        .find(|&h| u32_at(b, h + 4) == 2)
        .unwrap();
    let rel = (1..count)
        .map(|i| sh + i * 64)
        .find(|&h| u32_at(b, h + 4) == 4)
        .unwrap();
    let off = u64_at(b, sym + 24);
    let definition = (off..off + u64_at(b, sym + 32))
        .step_by(24)
        .find(|&s| b[s + 4] >> 4 == 1 && b[s + 6] != 0)
        .unwrap();
    (sh, sym, rel, definition)
}
fn macho_tables(b: &[u8]) -> (usize, usize, usize) {
    let mut cmd = 32;
    let mut section = 0;
    let mut symbol = 0;
    let mut reloc = 0;
    for _ in 0..u32_at(b, 16) {
        match u32_at(b, cmd) {
            0x19 => {
                section = cmd + 72;
                reloc = u32_at(b, section + 56);
            }
            2 => {
                let off = u32_at(b, cmd + 8);
                symbol = (off..off + u32_at(b, cmd + 12) * 16)
                    .step_by(16)
                    .find(|&s| b[s + 4] & 0x0e == 0x0e)
                    .unwrap();
            }
            _ => {}
        }
        cmd += u32_at(b, cmd + 4);
    }
    (section, symbol, reloc)
}
fn extended_elf(b: &[u8]) -> (Vec<u8>, usize, usize) {
    let (sh, sym, _, definition) = elf_tables(b);
    let count = u16::from_le_bytes(b[60..62].try_into().unwrap()) as usize;
    let nsyms = u64_at(b, sym + 32) / 24;
    let ordinal = (definition - u64_at(b, sym + 24)) / 24;
    let old_section =
        u16::from_le_bytes(b[definition + 6..definition + 8].try_into().unwrap()) as usize;
    let mut out = b.to_vec();
    let indices = out.len().next_multiple_of(8);
    out.resize(indices + nsyms * 4, 0);
    put32(&mut out, indices + ordinal * 4, 0xff00);
    let headers = out.len().next_multiple_of(8);
    out.resize(headers + 0xff02 * 64, 0);
    out[headers..headers + count * 64].copy_from_slice(&b[sh..sh + count * 64]);
    out[headers + 0xff00 * 64..headers + 0xff01 * 64]
        .copy_from_slice(&b[sh + old_section * 64..sh + (old_section + 1) * 64]);
    let ext = headers + 0xff01 * 64;
    put32(&mut out, ext + 4, 18); // SHT_SYMTAB_SHNDX
    put64(&mut out, ext + 24, indices);
    put64(&mut out, ext + 32, nsyms * 4);
    put32(&mut out, ext + 40, (sym - sh) / 64);
    put64(&mut out, ext + 48, 4);
    put64(&mut out, ext + 56, 4);
    put64(&mut out, headers + 32, 0xff02); // extended e_shnum
    put64(&mut out, 40, headers);
    out[60..62].copy_from_slice(&0u16.to_le_bytes());
    out[definition + 6..definition + 8].copy_from_slice(&0xffffu16.to_le_bytes());
    (out, ext, indices + ordinal * 4)
}
fn malformed_cases(elf: &[u8], macho: &[u8]) -> Vec<(&'static str, bool, Vec<u8>)> {
    let (_, sym, rel, definition) = elf_tables(elf);
    let (section, symbol, relocation) = macho_tables(macho);
    let mut cases = Vec::new();
    for (name, value) in [
        ("missing-xindex", 0xffffu16),
        ("reserved-index", 0xff01),
        ("section-out-of-range", 0xfeff),
    ] {
        let mut b = elf.to_vec();
        b[definition + 6..definition + 8].copy_from_slice(&value.to_le_bytes());
        cases.push((name, false, b));
    }
    for (name, at, value, wide) in [
        (
            "relocation-symbol",
            u64_at(elf, rel + 24) + 12,
            0xffffff,
            false,
        ),
        ("relocation-link", rel + 40, 1, false),
        ("relocation-target", rel + 44, 0xffffff, false),
        ("relocation-entry-size", rel + 56, 16, true),
        ("relocation-table-size", rel + 32, 23, true),
        ("symbol-entry-size", sym + 56, 16, true),
        ("symbol-string-link", sym + 40, 1, false),
    ] {
        let mut b = elf.to_vec();
        if wide {
            put64(&mut b, at, value);
        } else {
            put32(&mut b, at, value);
        }
        cases.push((name, false, b));
    }
    let (ext, header, index) = extended_elf(elf);
    for (name, at, value, wide) in [
        ("extended-zero", index, 0, false),
        ("extended-unnecessary", index, 1, false),
        ("extended-out-of-range", index, 0xffffff, false),
        ("extended-size", header + 32, 4, true),
        ("extended-link", header + 40, 1, false),
        ("extended-entry-size", header + 56, 8, true),
    ] {
        let mut b = ext.clone();
        if wide {
            put64(&mut b, at, value);
        } else {
            put32(&mut b, at, value);
        }
        cases.push((name, false, b));
    }
    for (name, n) in [("N_SECT-zero", 0), ("N_SECT-out-of-range", 255)] {
        let mut b = macho.to_vec();
        b[symbol + 5] = n;
        cases.push((name, true, b));
    }
    for (name, value) in [
        ("external-relocation-symbol", 0x2dffffff),
        ("local-relocation-section", 0x250000ff),
    ] {
        let mut b = macho.to_vec();
        put32(&mut b, relocation + 4, value);
        cases.push((name, true, b));
    }
    let mut b = macho.to_vec();
    put32(&mut b, section + 60, 0xffffff);
    cases.push(("relocation-table-bounds", true, b));
    cases
}
