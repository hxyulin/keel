//! Byte-built metadata fixtures, independent of the linker's SFrame parser.
use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

fn object(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "sframe.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn locations(b: &[u8]) -> (usize, usize, usize, usize) {
    let f = object::File::parse(b).unwrap();
    let s = f.section_by_name(".sframe").unwrap();
    let r = f.section_by_name(".rela.sframe").unwrap();
    let shoff = u64_at(b, 40) as usize;
    (
        shoff + s.index().0 * 64,
        s.file_range().unwrap().0 as usize,
        shoff + r.index().0 * 64,
        r.file_range().unwrap().0 as usize,
    )
}
fn fixture(targets: [&str; 3]) -> Vec<u8> {
    // Actual CRT geometry: 28-byte header, three 16-byte indices, 26 FRE
    // bytes including attributes; the PC4 row has N=0 (RA undefined).
    let mut b = object(&format!(
        ".text\n.globl _start\n_start:\nret\n.zero 79\n.data\n.globl noncode\nnoncode:\n.zero 80\n.section .sframe,\"a\",@progbits\n.p2align 3\n\
         .byte 226,222,3,4,3,0,248,0\n.long 3,4,26,0,48\n\
         .quad {}\n.long 38,0\n.quad {}\n.long 5,10\n.quad {}\n.long 5,18\n\
         .byte 2,0,0,0,0,0,3,8,4,0\n.byte 1,0,0,0,0,0,3,8\n.byte 1,0,0,0,0,0,3,8\n",
        targets[0], targets[1], targets[2]
    ));
    let (sh, _, _, rela) = locations(&b);
    put32(&mut b, sh + 4, 0x6fff_fff4);
    for i in 0..3 {
        put32(&mut b, rela + 24 * i + 8, 24);
    }
    b
}
fn valid() -> Vec<u8> {
    fixture(["_start", "_start+48", "_start+64"])
}
fn error(b: Vec<u8>, expected: &str) {
    let e = link(&[b]).unwrap_err();
    assert!(e.contains(expected), "expected {expected:?}, got {e:?}");
}

#[test]
fn packed_crt_geometry_rows_and_loaded_program_header_are_preserved() {
    let b = valid();
    let (_, input, _, _) = locations(&b);
    let exe = link(std::slice::from_ref(&b)).unwrap();
    assert_eq!(exe.bytes, link(std::slice::from_ref(&b)).unwrap().bytes);
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let s = f.section_by_name(".sframe").unwrap();
    let data = s.data().unwrap();
    assert_eq!(s.kind(), object::SectionKind::Elf(0x6fff_fff4));
    assert_eq!(data.len(), 102);
    assert_eq!(data[3], 5);
    assert_eq!(&data[76..], &b[input + 76..input + 102]);
    let start = f
        .symbols()
        .find(|s| s.name().unwrap() == "_start")
        .unwrap()
        .address();
    let mut raw = Vec::new();
    for (i, addend) in [0, 48, 64].iter().enumerate() {
        let field = 28 + i * 16; // deliberately unaligned i64 fields
        let d = u64_at(data, field) as i64;
        assert_eq!(
            i128::from(s.address() + field as u64) + i128::from(d),
            i128::from(start + addend)
        );
        raw.push(d);
    }
    assert_eq!(raw[1], raw[2]); // sorted PCs need not have strictly sorted deltas
    let phoff = u64_at(&exe.bytes, 32) as usize;
    let phnum = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
    assert!(phoff + phnum * 56 <= 4096);
    let ph = (0..phnum)
        .map(|i| phoff + i * 56)
        .find(|&p| u32_at(&exe.bytes, p) == 0x6474_e554)
        .unwrap();
    assert_eq!(u32_at(&exe.bytes, ph + 4), 4);
    assert_eq!(u64_at(&exe.bytes, ph + 8), s.file_range().unwrap().0);
    assert_eq!(u64_at(&exe.bytes, ph + 16), s.address());
    assert_eq!(u64_at(&exe.bytes, ph + 24), s.address());
    assert_eq!(u64_at(&exe.bytes, ph + 32), 102);
    assert_eq!(u64_at(&exe.bytes, ph + 40), 102);
    assert_eq!(u64_at(&exe.bytes, ph + 48), 8);
    assert!(
        (0..phnum)
            .map(|i| phoff + i * 56)
            .any(|p| u32_at(&exe.bytes, p) == 1
                && u32_at(&exe.bytes, p + 4) == 4
                && u64_at(&exe.bytes, p + 16) <= s.address()
                && u64_at(&exe.bytes, p + 16) + u64_at(&exe.bytes, p + 32)
                    >= s.address() + s.size())
    );
}

#[test]
fn rejects_old_future_truncated_headers_flags_and_header_contracts() {
    let good = valid();
    let (sh, data, _, _) = locations(&good);
    for n in 0..102 {
        // no truncated prefix of this nonempty table is valid
        let mut b = good.clone();
        put64(&mut b, sh + 32, n);
        error(b, ".sframe");
    }
    for (off, value, expected) in [
        (0, 0, "magic"),
        (2, 1, "version 3"),
        (2, 2, "version 3"),
        (2, 4, "version 3"),
        (3, 0, "flags"),
        (3, 6, "flags"),
        (3, 0x84, "flags"),
        (4, 2, "AMD64"),
        (5, 8, "FP=0"),
        (6, 0, "RA=-8"),
        (7, 4, "auxiliary"),
    ] {
        let mut b = good.clone();
        b[data + off] = value;
        error(b, expected);
    }
    for (off, value) in [
        (8, u32::MAX),
        (12, u32::MAX),
        (16, u32::MAX),
        (20, 4),
        (24, 52),
        (12, 3),
        (12, 5),
    ] {
        let mut b = good.clone();
        put32(&mut b, data + off, value);
        error(b, ".sframe");
    }
    let mut b = good;
    put64(&mut b, sh + 32, 103);
    error(b, "exact section size");
}

#[test]
fn rejects_incorrect_section_contract_even_when_nonallocated() {
    let good = valid();
    let (sh, _, _, _) = locations(&good);
    for (off, val, width) in [
        (4, 1, 4),
        (8, 0, 8),
        (8, 3, 8),
        (8, 6, 8),
        (8, 0x802, 8),
        (48, 4, 8),
        (48, 16 * 1024 * 1024, 8),
        (56, 16, 8),
        (40, 1, 4),
        (44, 1, 4),
    ] {
        let mut b = good.clone();
        if width == 4 {
            put32(&mut b, sh + off, val as u32);
        } else {
            put64(&mut b, sh + off, val);
        }
        error(b, ".sframe requires");
    }
    let mut b = good.clone();
    // Type identifies SFrame even if its section name no longer does.
    put32(&mut b, sh, 0);
    error(b, ".sframe requires");
}

#[test]
fn rejects_bad_attributes_rows_blocks_and_counts() {
    let good = valid();
    let (_, d, _, _) = locations(&good);
    for info in [3, 0x10, 0x20, 0x40, 0x80] {
        let mut b = good.clone();
        b[d + 78] = info;
        error(b, "unsupported attributes");
    }
    for (off, val) in [(79, 1), (79, 32), (80, 1)] {
        let mut b = good.clone();
        b[d + off] = val;
        error(b, "unsupported attributes");
    }
    for val in [0x61, 7, 0x83] {
        let mut b = good.clone();
        b[d + 82] = val;
        error(b, "row data count/width");
    }
    for (off, val) in [(76, 0), (76, 3), (81, 1), (84, 0), (84, 38)] {
        let mut b = good.clone();
        b[d + off] = val;
        error(b, ".sframe");
    }
    for (off, val) in [
        (36, 0),
        (40, 1),
        (56, 0),
        (56, 9),
        (56, 11),
        (72, 25),
        (72, 26),
    ] {
        let mut b = good.clone();
        put32(&mut b, d + off, val);
        error(b, ".sframe");
    }
    // A row which would consume bytes in the next function must fail locally.
    let mut b = good.clone();
    b[d + 85] = 3;
    error(b, "within their block");
    // Missing declared rows cannot be hidden in another block.
    let mut b = good;
    b[d + 76] = 1;
    error(b, "FRE block");
}

#[test]
fn pc_and_dataword_widths_and_ra_undefined_are_supported() {
    for pc_type in 0..=2 {
        for width in 0..=2 {
            let mut b = valid();
            let (sh, d, rh, _) = locations(&b);
            // Reuse space from indices 2/3 for a single larger row block.
            put32(&mut b, d + 8, 1);
            put32(&mut b, d + 12, 2);
            let p = 1usize << pc_type;
            let w = 1usize << width;
            let len = 5 + 2 * (p + 1) + 2 * w;
            put32(&mut b, d + 16, len as u32);
            put32(&mut b, d + 24, 16);
            put64(&mut b, sh + 32, (44 + len) as u64);
            put64(&mut b, rh + 32, 24);
            b[d + 44..d + 44 + len].fill(0);
            b[d + 44] = 2;
            b[d + 46] = pc_type;
            let a = d + 49;
            b[a + p] = 5 | (width << 5); // SP, N=2
            b[a + p + 1..a + p + 1 + 2 * w].fill(0xff); // signed -1 offsets
            let a = a + p + 1 + 2 * w;
            b[a] = 4; // second row PC4, N=0
            assert!(link(&[b]).is_ok(), "PC type {pc_type}, data width {width}");
        }
    }
}

#[test]
fn only_exact_required_pc64_fields_allow_real_relocations() {
    let good = valid();
    let (_, _, rh, rela) = locations(&good);
    for kind in [1, 2, 10, 11, 4, 9, 42, 99] {
        let mut b = good.clone();
        put32(&mut b, rela + 8, kind);
        error(b, "require PC64");
    }
    for offset in [0, 3, 20, 27, 29, 32, 36, 40, 76, 81] {
        let mut b = good.clone();
        put64(&mut b, rela, offset);
        error(b, "index start-address");
    }
    let mut b = good.clone();
    put64(&mut b, rela + 24, 28);
    error(b, "duplicate");
    let mut b = good.clone();
    put64(&mut b, rh + 32, 48);
    error(b, "missing required");
    let mut b = good.clone();
    put32(&mut b, rela + 8, 0);
    error(b, "missing required");
    // NONE can coexist as an explicit no-op without counting as a real field.
    let mut b = good;
    let (sh, d, _, _) = locations(&b);
    b[d + 8..d + 28].fill(0);
    put64(&mut b, sh + 32, 28);
    put64(&mut b, rh + 32, 24);
    b[rela..rela + 24].fill(0);
    assert!(link(&[b]).is_ok());
}

#[test]
fn shared_library_code_is_not_a_native_sframe_target_and_phdr_span_is_exact() {
    let obj = object(
        ".text\n.globl _start\n_start:\nret\n.globl imported\n.type imported,@function\nimported:\nret\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n",
    );
    let mut dso = link(&[obj]).unwrap().bytes;
    let f = object::File::parse(dso.as_slice()).unwrap();
    let dh = f.section_by_name(".dynamic").unwrap().index().0;
    let sym = f.section_by_name(".symtab").unwrap().index().0;
    let strings = f.section_by_name(".strtab").unwrap().index().0;
    let sh = u64_at(&dso, 40) as usize;
    dso[16..18].copy_from_slice(&3u16.to_le_bytes());
    put32(&mut dso, sh + sym * 64 + 4, 11);
    put32(&mut dso, sh + dh * 64 + 4, 6);
    put32(&mut dso, sh + dh * 64 + 40, strings as u32);
    put64(&mut dso, sh + dh * 64 + 56, 16);
    let bad = fixture(["imported", "_start+48", "_start+64"]);
    assert!(
        link(&[bad, dso.clone()])
            .unwrap_err()
            .contains("undefined or imported")
    );
    let exe = link(&[valid(), dso]).unwrap();
    let n = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as u64;
    assert_eq!(u32_at(&exe.bytes, 64), 6); // PT_PHDR
    assert_eq!(u64_at(&exe.bytes, 64 + 32), n * 56);
    assert_eq!(u64_at(&exe.bytes, 64 + 40), n * 56);
}

#[test]
fn target_identity_ranges_and_resolved_order_are_checked() {
    error(fixture(["noncode", "_start+48", "_start+64"]), "non-code");
    error(
        fixture(["missing", "_start+48", "_start+64"]),
        "undefined or imported",
    );
    error(
        fixture(["_start-1", "_start+48", "_start+64"]),
        "outside its executable input",
    );
    error(
        fixture(["_start", "_start+48", "_start+76"]),
        "outside its executable input",
    );
    error(
        fixture(["_start+48", "_start", "_start+64"]),
        "outside its executable input",
    );
    // Valid ranges with false SORTED flag and reversed/overlapping resolved PCs.
    for targets in [
        ["_start", "_start+64", "_start+48"],
        ["_start", "_start+32", "_start+64"],
        ["_start", "_start+48", "_start+48"],
    ] {
        let mut b = fixture(targets);
        let (_, d, _, _) = locations(&b);
        b[d + 3] = 5;
        error(b, "unordered or overlapping");
    }
    // Ascending PCs with descending raw deltas are valid: size1 indices at 0,8,16.
    let mut b = fixture(["_start", "_start+8", "_start+16"]);
    let (_, d, _, _) = locations(&b);
    put32(&mut b, d + 36, 5);
    let exe = link(&[b]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let s = f.section_by_name(".sframe").unwrap().data().unwrap();
    assert!((u64_at(s, 28) as i64) > (u64_at(s, 44) as i64));
    assert!((u64_at(s, 44) as i64) > (u64_at(s, 60) as i64));
    // Huge signed addends must not wrap back into input code.
    let mut b = valid();
    let (_, _, _, r) = locations(&b);
    put64(&mut b, r + 16, i64::MAX as u64);
    error(b, "outside its executable input");
    let mut b = valid();
    let (_, _, _, r) = locations(&b);
    put64(&mut b, r + 16, i64::MIN as u64);
    error(b, "outside its executable input");
}

fn archive(member: &[u8]) -> Vec<u8> {
    let mut b = b"!<arch>\n".to_vec();
    b.extend_from_slice(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "unused.o/",
            0,
            0,
            0,
            "100644",
            member.len()
        )
        .as_bytes(),
    );
    b.extend_from_slice(member);
    if !b.len().is_multiple_of(2) {
        b.push(b'\n');
    }
    b
}

#[test]
fn second_selected_producer_fails_but_unused_archive_metadata_does_not_count() {
    let a = valid();
    let mut b = valid();
    // Rename the second object's entry to keep ordinary symbol resolution separate.
    let f = object::File::parse(b.as_slice()).unwrap();
    let strings = f.section_by_name(".strtab").unwrap();
    let (off, len) = strings.file_range().unwrap();
    let at = b[off as usize..(off + len) as usize]
        .windows(7)
        .position(|s| s == b"_start\0")
        .unwrap()
        + off as usize;
    b[at..at + 6].copy_from_slice(b"second");
    assert!(
        link(&[a.clone(), b.clone()])
            .unwrap_err()
            .contains("second selected SFrame producer")
    );
    let demand = object(".data\n.quad second\n");
    assert!(
        link(&[a.clone(), demand, archive(&b)])
            .unwrap_err()
            .contains("second selected SFrame producer")
    );
    assert!(link(&[a, archive(&b)]).is_ok());
}

#[test]
fn empty_header_is_retained_but_zero_bytes_are_invalid() {
    let mut b = valid();
    let (sh, d, rh, _) = locations(&b);
    b[d + 8..d + 28].fill(0);
    put64(&mut b, sh + 32, 28);
    put64(&mut b, rh + 32, 0);
    let exe = link(&[b]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let s = f.section_by_name(".sframe").unwrap();
    assert_eq!(s.size(), 28);
    assert_eq!(s.data().unwrap()[3], 5);
}

#[test]
fn independent_fre_block_order_and_prevalidated_sorted_input_are_preserved() {
    let mut b = valid();
    let (_, d, _, _) = locations(&b);
    b[d + 3] = 5;
    // Equal row shapes, independent block identities, descending FRE offsets.
    put32(&mut b, d + 56, 18);
    put32(&mut b, d + 72, 10);
    let exe = link(std::slice::from_ref(&b)).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let out = f.section_by_name(".sframe").unwrap().data().unwrap();
    assert_eq!(out[3], 5);
    assert_eq!(u32_at(out, 56), 18);
    assert_eq!(u32_at(out, 72), 10);
    assert_eq!(&out[76..], &b[d + 76..d + 102]);
}
