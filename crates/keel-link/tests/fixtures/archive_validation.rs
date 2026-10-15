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
    cases.extend(round2_malformed_cases(elf, macho));
    cases
}

fn macho_strings(b: &[u8]) -> (usize, usize) {
    let mut cmd = 32;
    for _ in 0..u32_at(b, 16) {
        if u32_at(b, cmd) == 2 {
            return (u32_at(b, cmd + 16), u32_at(b, cmd + 20));
        }
        cmd += u32_at(b, cmd + 4);
    }
    panic!("missing symtab")
}
fn alias_object(macho: &[u8], offset: usize) -> Vec<u8> {
    let (_, symbol, _) = macho_tables(macho);
    let mut b = macho.to_vec();
    b[symbol + 4] = 0x0b;
    b[symbol + 5] = 0;
    put64(&mut b, symbol + 8, offset);
    b
}
fn addend_object(macho: &[u8], kind: usize) -> Vec<u8> {
    let (section, _, rel) = macho_tables(macho);
    let mut b = macho.to_vec();
    let at = b.len().next_multiple_of(4);
    b.resize(at + 16, 0);
    b[at..at + 8].copy_from_slice(&macho[rel..rel + 8]);
    put32(&mut b, at + 4, 0xa4ffffff); // signed -1 payload, not an ordinal
    b[at + 8..at + 16].copy_from_slice(&macho[rel..rel + 8]);
    let flags = if kind == 4 { 0x0c000000 } else { 0x0d000000 };
    put32(
        &mut b,
        at + 12,
        (kind << 28) | flags | (u32_at(macho, rel + 4) & 0xffffff),
    );
    put32(&mut b, section + 56, at);
    put32(&mut b, section + 60, 2);
    b
}
fn elf_site_object(elf: &[u8], kind: usize, offset: usize, size: usize) -> Vec<u8> {
    let (sh, _, rel, _) = elf_tables(elf);
    let target = sh + u32_at(elf, rel + 44) * 64;
    let mut b = elf.to_vec();
    let data = b.len();
    b.resize(data + size, 0);
    put64(&mut b, target + 24, data);
    put64(&mut b, target + 32, size);
    put64(&mut b, u64_at(elf, rel + 24), offset);
    put32(&mut b, u64_at(elf, rel + 24) + 8, kind);
    b
}
fn round2_malformed_cases(elf: &[u8], macho: &[u8]) -> Vec<(&'static str, bool, Vec<u8>)> {
    let mut cases = Vec::new();
    let (_, strsize) = macho_strings(macho);
    for (name, offset) in [("alias-strsize", strsize), ("alias-u64-max", usize::MAX)] {
        cases.push((name, true, alias_object(macho, offset)));
    }
    let (strings, _) = macho_strings(macho);
    let mut b = alias_object(macho, strsize - 1);
    b[strings + strsize - 1] = b'x'; // no NUL before raw table end
    cases.push(("alias-unterminated", true, b));
    let (section, _, rel) = macho_tables(macho);
    for (name, info) in [
        ("addend-orphan", 0xa4ffffff),
        ("addend-extern-orphan", 0xacffffff),
    ] {
        let mut b = macho.to_vec();
        put32(&mut b, rel + 4, info);
        cases.push((name, true, b));
    }
    let valid = addend_object(macho, 3);
    let at = u32_at(&valid, section + 56);
    for (name, field, value) in [
        ("addend-extern", 4, 0xacffffff),
        ("addend-pcrel", 4, 0xa5ffffff),
        ("addend-length", 4, 0xa6ffffff),
        ("addend-address", 0, 4),
        ("addend-wrong-type", 12, 0x0c000000),
        ("addend-double", 12, 0xa4ffffff),
        ("addend-pair-local", 12, 0x35000001),
        ("addend-pair-length", 12, 0x3f000001),
        ("addend-pair-pcrel", 12, 0x3c000001),
        ("addend-pageoff-pcrel", 12, 0x4d000001),
        ("addend-pair-symbol", 12, 0x3dffffff),
    ] {
        let mut b = valid.clone();
        put32(&mut b, at + field, value);
        cases.push((name, true, b));
    }
    let (_, sym, _, _) = elf_tables(elf);
    for (name, boundary) in [
        ("symbol-info-oob", 0xffffff),
        ("symbol-info-zero", 0),
        (
            "symbol-info-global-before-boundary",
            u64_at(elf, sym + 32) / 24,
        ),
    ] {
        let mut b = elf.to_vec();
        put32(&mut b, sym + 44, boundary);
        cases.push((name, false, b));
    }
    let (_, _, _, definition) = elf_tables(elf);
    let mut b = elf.to_vec();
    b[definition + 4] &= 0x0f;
    cases.push(("symbol-local-after-boundary", false, b));
    for (name, kind, offset, size) in [
        ("elf-site-u64-max", 4, usize::MAX, 6),
        ("elf-site-end", 4, 6, 6),
        ("elf-site-cross-end", 4, 3, 6),
        ("elf-site-8-cross-end", 1, 9, 16),
        ("elf-site-16-cross-end", 36, 1, 16),
        ("elf-site-2-cross-end", 12, 15, 16),
        ("elf-site-1-end", 14, 16, 16),
        ("elf-site-unknown-end", 0x1234, 16, 16),
        ("elf-site-empty", 0x1234, 0, 0),
    ] {
        cases.push((name, false, elf_site_object(elf, kind, offset, size)));
    }
    let mut bad_pair_site = valid.clone();
    put32(&mut bad_pair_site, at, 0xffffffff);
    put32(&mut bad_pair_site, at + 8, 0xffffffff);
    cases.push(("addend-pair-site-negative", true, bad_pair_site));
    let mut rel_site = elf_site_object(elf, 4, usize::MAX, 16);
    make_elf_rel(&mut rel_site);
    cases.push(("elf-REL-site-overflow", false, rel_site));
    for (name, offset) in [
        ("macho-site-negative", 0xffffffff),
        ("macho-site-end", 8),
        ("macho-site-cross-end", 6),
    ] {
        let mut b = macho.to_vec();
        put32(&mut b, rel, offset);
        cases.push((name, true, b));
    }
    cases
}
fn make_elf_rel(b: &mut [u8]) {
    let (_, _, rel, _) = elf_tables(b);
    put32(b, rel + 4, 9); // SHT_REL; same leading offset/info fields
    put64(b, rel + 32, 16);
    put64(b, rel + 56, 16);
}
fn round2_valid_cases(elf: &[u8], macho: &[u8]) -> Vec<(&'static str, bool, Vec<u8>)> {
    let (_, strsize) = macho_strings(macho);
    let mut cases = vec![
        ("alias-empty-target", true, alias_object(macho, 0)),
        ("alias-last-NUL", true, alias_object(macho, strsize - 1)),
        ("addend-branch", true, addend_object(macho, 2)),
        ("addend-page21", true, addend_object(macho, 3)),
        ("addend-pageoff12", true, addend_object(macho, 4)),
    ];
    for (name, payload) in [
        ("addend-zero", 0),
        ("addend-max", 0x7fffff),
        ("addend-min", 0x800000),
    ] {
        let mut b = addend_object(macho, 3);
        let (section, _, _) = macho_tables(macho);
        let at = u32_at(&b, section + 56);
        put32(&mut b, at + 4, 0xa4000000 | payload);
        cases.push((name, true, b));
    }
    let mut rel_site = elf_site_object(elf, 4, 12, 16);
    make_elf_rel(&mut rel_site);
    cases.push(("site-REL-boundary", false, rel_site));
    // Target the name of an actual undefined symbol, retaining the alias provider.
    let (_, symbol, _) = macho_tables(macho);
    let mut cmd = 32;
    while u32_at(macho, cmd) != 2 {
        cmd += u32_at(macho, cmd + 4);
    }
    let off = u32_at(macho, cmd + 8);
    let undefined = (off..off + u32_at(macho, cmd + 12) * 16)
        .step_by(16)
        .find(|&s| macho[s + 4] & 0x0e == 0 && macho[s + 4] & 1 != 0)
        .unwrap();
    assert_ne!(symbol, undefined);
    cases.push((
        "alias-undefined",
        true,
        alias_object(macho, u32_at(macho, undefined)),
    ));
    for (name, kind, offset) in [
        ("site-width1", 14, 15),
        ("site-width2", 12, 14),
        ("site-width4", 4, 12),
        ("site-width8", 1, 8),
        ("site-width16", 36, 0),
        ("site-TLSGD", 19, 12),
        ("site-unknown", 0x1234, 15),
        ("site-marker", 35, 15),
        ("site-NONE-ignored", 0, usize::MAX),
    ] {
        cases.push((name, false, elf_site_object(elf, kind, offset, 16)));
    }
    cases
}
