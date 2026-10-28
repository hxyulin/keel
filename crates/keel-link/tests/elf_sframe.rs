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
    fixture_extra(targets, "")
}
fn fixture_extra(targets: [&str; 3], extra: &str) -> Vec<u8> {
    // Actual CRT geometry: 28-byte header, three 16-byte indices, 26 FRE
    // bytes including attributes; the PC4 row has N=0 (RA undefined).
    let mut b = object(
        &(format!(
            ".text\n.globl _start\n_start:\nret\n.zero 79\n.data\n.globl noncode\nnoncode:\n.zero 80\n.section .sframe,\"a\",@progbits\n.p2align 3\n\
         .byte 226,222,3,4,3,0,248,0\n.long 3,4,26,0,48\n\
         .quad {}\n.long 38,0\n.quad {}\n.long 5,10\n.quad {}\n.long 5,18\n\
         .byte 2,0,0,0,0,0,3,8,4,0\n.byte 1,0,0,0,0,0,3,8\n.byte 1,0,0,0,0,0,3,8\n",
            targets[0], targets[1], targets[2]
        ) + extra),
    );
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

#[test]
fn notes_abi_sframe_unwind_and_dynamic_share_final_headers_and_addresses() {
    let abi = include_bytes!("fixtures/gnu-notes/crt1.o.note.ABI-tag.bin");
    let metadata = object(&format!(
        ".section .note.gnu.property,\"a\",@note\n.p2align 3\n.long 4,16,5,0x00554e47,0xc0008002,4,8,0\n\
         .section .note.ABI-tag,\"a\",@note\n.p2align 2\nabi: .byte {}\n\
         .section .data.keep,\"awR\"\n.globl abi_pointer\nabi_pointer: .quad abi+7\n",
        abi.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
    ));
    let unwind = object(
        ".section .text,\"axR\",@progbits\n.globl framed\nframed:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    for (dynamic, gc) in [(false, false), (true, false), (false, true), (true, true)] {
        for reverse in [false, true] {
            let mut inputs = vec![metadata.clone(), valid(), unwind.clone()];
            if reverse {
                inputs.reverse();
            }
            if dynamic {
                inputs.push(include_bytes!("fixtures/gnu-notes/libprotected-defined.so").to_vec());
            }
            let exe = if gc { gc_link(&inputs) } else { link(&inputs) }.unwrap();
            let f = object::File::parse(exe.bytes.as_slice()).unwrap();
            let phnum = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
            let headers = exe.bytes[64..64 + 56 * phnum].as_chunks::<56>().0;
            assert!(64 + 56 * phnum <= 4096);
            assert_eq!(phnum, 3 + 1 + 1 + 1 + 3 + if dynamic { 3 } else { 0 });
            for (kind, name, alignment) in [
                (object::elf::PT_GNU_PROPERTY, ".note.gnu.property", 8),
                (object::elf::PT_NOTE, ".note.gnu.property", 8),
                (object::elf::PT_NOTE, ".note.ABI-tag", 4),
                (0x6474_e554, ".sframe", 8),
                (object::elf::PT_GNU_EH_FRAME, ".eh_frame_hdr", 4),
            ] {
                let s = f.section_by_name(name).unwrap();
                let matches = headers
                    .iter()
                    .filter(|p| u32_at(*p, 0) == kind && u64_at(*p, 16) == s.address())
                    .collect::<Vec<_>>();
                assert_eq!(matches.len(), 1);
                let p = matches[0];
                assert_eq!(u32_at(p, 4), 4);
                assert_eq!(u64_at(p, 8), s.file_range().unwrap().0);
                assert_eq!(u64_at(p, 32), s.size());
                assert_eq!(u64_at(p, 40), s.size());
                assert_eq!(u64_at(p, 48), alignment);
                assert!(headers.iter().any(|load| u32_at(load, 0) == 1
                    && u32_at(load, 4) == 4
                    && u64_at(load, 16) <= s.address()
                    && s.address() + s.size() <= u64_at(load, 16) + u64_at(load, 32)));
            }
            if dynamic {
                let p = headers
                    .iter()
                    .find(|p| u32_at(*p, 0) == object::elf::PT_PHDR)
                    .unwrap();
                assert_eq!(u64_at(p, 32), (phnum * 56) as u64);
                assert_eq!(u64_at(p, 40), (phnum * 56) as u64);
            }
            assert_eq!(
                f.section_by_name(".note.ABI-tag").unwrap().data().unwrap(),
                abi
            );
            let symbol = |name| {
                f.symbols()
                    .find(|s| s.name().unwrap() == name)
                    .unwrap()
                    .address()
            };
            assert_eq!(
                u64_at(&exe.bytes, (symbol("abi_pointer") - 0x400000) as usize),
                f.section_by_name(".note.ABI-tag").unwrap().address() + 7
            );
            let s = f.section_by_name(".sframe").unwrap();
            let data = s.data().unwrap();
            assert_eq!(data[3], 5);
            for (i, addend) in [0, 48, 64].into_iter().enumerate() {
                let at = 28 + 16 * i;
                assert_eq!(
                    (s.address() + at as u64) as i128 + i128::from(u64_at(data, at) as i64),
                    i128::from(symbol("_start") + addend)
                );
            }
            let eh = f.section_by_name(".eh_frame_hdr").unwrap();
            let data = eh.data().unwrap();
            assert_eq!(u32_at(data, 8), 1);
            assert_eq!(
                (eh.address() as i64 + i64::from(u32_at(data, 12) as i32)) as u64,
                symbol("framed")
            );
        }
    }
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
    let mut b = good;
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

#[test]
fn registration_code_grouping_still_checks_final_sframe_order() {
    let producers = [
        object(".section registry,\"ax\",@progbits\n.globl first\nfirst: .zero 40\n"),
        object(".section other,\"ax\",@progbits\n.globl middle\nmiddle: .zero 8\n"),
        object(".section registry,\"ax\",@progbits\n.globl last\nlast: .zero 8\n"),
    ];
    // Grouping moves last before middle. Never certify the old index order as
    // SORTED: general SFrame reordering remains deliberately unsupported.
    let mut objects = vec![fixture(["first", "middle", "last"])];
    objects.extend(producers.iter().cloned());
    assert!(
        link(&objects)
            .unwrap_err()
            .contains("unordered or overlapping")
    );
    objects[0] = fixture(["first", "last", "middle"]);
    assert!(link(&objects).is_ok());
}

fn gc_link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "gc-sframe.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        elf::Options {
            gc_sections: true,
            ..Default::default()
        },
    )
}

#[test]
fn gc_all_live_retains_exact_crt_rows() {
    let bytes = valid();
    let exe = gc_link(std::slice::from_ref(&bytes)).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let data = f.section_by_name(".sframe").unwrap().data().unwrap();
    let (_, input, _, _) = locations(&bytes);
    assert_eq!(&data[76..], &bytes[input + 76..input + 102]);
    assert_eq!(u32_at(data, 8), 3);
    assert_eq!(data[3], 5);
}

#[test]
fn retained_real_crt_section_symbol_relocations_match_final_image() {
    let crt = include_bytes!("fixtures/sframe/crt1.o").to_vec();
    let input = object::File::parse(crt.as_slice()).unwrap();
    let metadata = input.section_by_name(".sframe").unwrap();
    let text = input.section_by_name(".text").unwrap();
    let relocs: Vec<_> = metadata.relocations().collect();
    assert_eq!(relocs.len(), 3);
    for (i, (offset, reloc)) in relocs.iter().enumerate() {
        assert_eq!(*offset, 28 + i as u64 * 16);
        assert_eq!(reloc.flags(), object::RelocationFlags::Elf { r_type: 24 });
        let object::RelocationTarget::Symbol(index) = reloc.target() else {
            panic!("symbol relocation required")
        };
        let symbol = input.symbol_by_index(index).unwrap();
        assert_eq!(symbol.kind(), object::SymbolKind::Section);
        assert_eq!(symbol.section_index(), Some(text.index()));
        assert_eq!(reloc.addend(), [0, 48, 64][i]);
    }
    // Structural cross-host linking only: these stubs do not implement libc startup.
    let stubs =
        object(".text\n.globl main\nmain: ret\n.globl __libc_start_main\n__libc_start_main: ret\n");
    let exe = link(&[crt.clone(), stubs.clone()]).unwrap();
    let gc = gc_link(&[crt.clone(), stubs]).unwrap();
    assert_eq!(output_records(&gc), output_records(&exe));
    let output = object::File::parse(exe.bytes.as_slice()).unwrap();
    let sframe = output.section_by_name(".sframe").unwrap();
    let data = sframe.data().unwrap();
    assert_eq!(&data[76..], &metadata.data().unwrap()[76..]);
    assert_eq!(data[3], 5);
    let start = output
        .symbols()
        .find(|s| s.name().unwrap() == "_start")
        .unwrap()
        .address();
    let mut expected = metadata.data().unwrap().to_vec();
    expected[3] |= 1;
    for (i, relative) in [0, 48, 64].into_iter().enumerate() {
        let field = 28 + 16 * i;
        assert_eq!(
            i128::from(sframe.address() + field as u64) + i128::from(u64_at(data, field) as i64),
            i128::from(start + relative)
        );
        let delta = i64::try_from(
            i128::from(start + relative) - i128::from(sframe.address() + field as u64),
        )
        .unwrap();
        expected[field..field + 8].copy_from_slice(&delta.to_le_bytes());
    }
    // The only permitted byte changes are the three relocated fields and SORTED.
    assert_eq!(data, expected);
    // The real section symbol is local and need not survive in the global-only
    // output symbol table for its original identity to bind correctly.
    assert!(
        !output
            .symbols()
            .any(|s| s.kind() == object::SymbolKind::Section)
    );
}

#[test]
fn empty_header_still_counts_as_a_second_selected_producer() {
    let mut empty = object(
        ".section .sframe,\"a\",@progbits\n.p2align 3\n.byte 226,222,3,4,3,0,248,0\n.long 0,0,0,0,0\n",
    );
    let f = object::File::parse(empty.as_slice()).unwrap();
    let index = f.section_by_name(".sframe").unwrap().index().0;
    let sh = u64_at(&empty, 40) as usize + 64 * index;
    put32(&mut empty, sh + 4, 0x6fff_fff4);
    assert!(
        link(&[valid(), empty])
            .unwrap_err()
            .contains("second selected SFrame producer")
    );
}

fn output_records(exe: &elf::Executable) -> Vec<(u64, u32, Vec<u8>)> {
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let s = f.section_by_name(".sframe").unwrap();
    let b = s.data().unwrap();
    assert_eq!(b[3], 5);
    let count = u32_at(b, 8) as usize;
    let fre = 28 + 16 * count;
    assert_eq!(u32_at(b, 24) as usize, 16 * count);
    assert_eq!(fre + u32_at(b, 16) as usize, b.len());
    let mut rows = 0;
    let result = (0..count)
        .map(|i| {
            let field = 28 + 16 * i;
            let start = fre + u32_at(b, field + 12) as usize;
            let end = (0..count)
                .map(|j| fre + u32_at(b, 40 + 16 * j) as usize)
                .filter(|&n| n > start)
                .min()
                .unwrap_or(b.len());
            rows += u16::from_le_bytes(b[start..start + 2].try_into().unwrap()) as u32;
            (
                ((s.address() + field as u64) as i128 + i128::from(u64_at(b, field) as i64)) as u64,
                u32_at(b, field + 8),
                b[start..end].to_vec(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(rows, u32_at(b, 12));
    assert!(
        result
            .windows(2)
            .all(|r| r[0].0 + u64::from(r[0].1) <= r[1].0)
    );
    let phnum = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
    let ph = exe.bytes[64..64 + 56 * phnum]
        .as_chunks::<56>()
        .0
        .iter()
        .find(|p| u32_at(*p, 0) == 0x6474_e554)
        .unwrap();
    assert_eq!(u64_at(ph, 16), s.address());
    assert_eq!(u64_at(ph, 32), b.len() as u64);
    assert_eq!(u64_at(ph, 40), b.len() as u64);
    result
}
fn symbol_address(exe: &elf::Executable, name: &str) -> Option<u64> {
    object::File::parse(exe.bytes.as_slice())
        .unwrap()
        .symbols()
        .find(|s| s.name().unwrap() == name)
        .map(|s| s.address())
}

#[test]
fn gc_liveness_index_and_fre_permutations_preserve_exact_owned_blocks() {
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let names = ["first", "middle", "last"];
    for mask in 0u32..8 {
        let code = object(&names.iter().enumerate().map(|(i, name)| format!(
            ".section .text.{name},\"ax{}\",@progbits\n.globl {name}\n{name}: .zero 40\n",
            if mask & (1 << i) != 0 { "R" } else { "" }
        )).collect::<String>());
        for indices in permutations {
            for blocks in permutations {
                let mut metadata = fixture(["first", "middle", "last"]);
                let (_, d, _, r) = locations(&metadata);
                // Reorder indices and relocations as distinct paired identities.
                let original = metadata.clone();
                let mut offset = 0;
                for block in blocks {
                    let size = if block == 0 { 10 } else { 8 };
                    let old = [0, 10, 18][block];
                    metadata[d + 76 + offset..d + 76 + offset + size]
                        .copy_from_slice(&original[d + 76 + old..d + 76 + old + size]);
                    // Distinct exact signed CFA words expose accidental block swaps.
                    metadata[d + 76 + offset + 7] = 8 + block as u8;
                    let output = indices.iter().position(|&i| i == block).unwrap();
                    metadata[d + 28 + 16 * output..d + 44 + 16 * output]
                        .copy_from_slice(&original[d + 28 + 16 * block..d + 44 + 16 * block]);
                    put32(&mut metadata, d + 40 + 16 * output, offset as u32);
                    metadata[r + 24 * output..r + 24 * output + 24]
                        .copy_from_slice(&original[r + 24 * block..r + 24 * block + 24]);
                    put64(&mut metadata, r + 24 * output, (28 + 16 * output) as u64);
                    offset += size;
                }
                let exe = gc_link(&[metadata, code.clone()]).unwrap();
                let records = output_records(&exe);
                assert_eq!(records.len(), mask.count_ones() as usize);
                for (i, name) in names.iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        let record = records
                            .iter()
                            .find(|r| Some(r.0) == symbol_address(&exe, name))
                            .unwrap();
                        assert_eq!(record.1, if i == 0 { 38 } else { 5 });
                        assert_eq!(record.2[7], 8 + i as u8);
                    } else {
                        assert_eq!(symbol_address(&exe, name), None);
                    }
                }
            }
        }
    }
}

#[test]
fn gc_registration_boundaries_and_pointer_relocations_drive_final_code_order() {
    let code = [
        object(".section registry,\"ax\",@progbits\n.globl first\nfirst: .zero 40\n"),
        object(".section other,\"axR\",@progbits\n.globl middle\nmiddle: .zero 8\n"),
        object(".section registry,\"ax\",@progbits\n.globl last\nlast: .zero 8\n"),
        object(
            ".section .data.keep,\"awR\"\n.globl pointers\npointers: .quad __start_registry, __stop_registry, middle, last\n",
        ),
    ];
    let mut objects = vec![fixture(["middle", "first", "last"])];
    // The large first index must fit middle before it can be sorted.
    let (_, d, _, _) = locations(&objects[0]);
    put32(&mut objects[0], d + 36, 5);
    put32(&mut objects[0], d + 52, 38);
    // Its PC4 row still fits a five-byte function.
    objects.extend(code);
    let exe = gc_link(&objects).unwrap();
    let records = output_records(&exe);
    assert_eq!(
        records.iter().map(|r| r.0).collect::<Vec<_>>(),
        ["first", "last", "middle"].map(|n| symbol_address(&exe, n).unwrap())
    );
    let p = (symbol_address(&exe, "pointers").unwrap() - 0x400000) as usize;
    assert_eq!(u64_at(&exe.bytes, p), records[0].0);
    assert_eq!(u64_at(&exe.bytes, p + 8), records[1].0 + 8);
    assert_eq!(u64_at(&exe.bytes, p + 16), records[2].0);
    assert_eq!(u64_at(&exe.bytes, p + 24), records[1].0);
}

#[test]
fn gc_weak_global_winner_and_local_addends_bind_before_pruning() {
    let metadata = fixture(["choice", "local_code+48", "local_code+64"]);
    let loser = object(".section .text.loser,\"ax\"\n.weak choice\nchoice: .zero 40\n");
    let winner = object(
        ".section .text.winner,\"axR\"\n.globl choice\nchoice: .zero 40\n.section .text.local,\"axR\"\n.globl local_code\nlocal_code: .zero 80\n",
    );
    let exe = gc_link(&[metadata, loser, winner.clone()]).unwrap();
    let records = output_records(&exe);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].0, symbol_address(&exe, "choice").unwrap());
    assert_eq!(
        records[1].0,
        symbol_address(&exe, "local_code").unwrap() + 48
    );
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(f.section_by_name(".text.loser").is_none());
    // A malformed range in the losing definition must still fail before GC.
    let small_loser = fixture_extra(
        ["choice", "local_code+48", "local_code+64"],
        ".section .text.loser,\"ax\"\n.weak choice\nchoice: ret\n",
    );
    assert!(link(&[small_loser.clone(), winner.clone()]).is_ok());
    assert!(
        gc_link(&[small_loser, winner])
            .unwrap_err()
            .contains("original target code range")
    );
}

#[test]
fn gc_validates_malformed_dead_ranges_and_targets_before_prune() {
    let dead = object(".section .text.dead,\"ax\"\n.globl dead\ndead: .zero 80\n");
    for (targets, diagnostic) in [
        (["dead", "dead+48", "dead+79"], "outside its executable"),
        (["dead", "dead+48", "dead-1"], "outside its executable"),
        (["dead", "dead+48", "absent"], "undefined or imported"),
        (["dead", "dead+48", "noncode"], "non-code"),
    ] {
        assert!(
            gc_link(&[fixture(targets), dead.clone()])
                .unwrap_err()
                .contains(diagnostic)
        );
    }
    let common = object(".comm common_code,80,8\n");
    assert!(
        gc_link(&[fixture(["common_code"; 3]), common])
            .unwrap_err()
            .contains("native executable")
    );
    let mut bad = fixture(["dead", "dead+48", "dead+64"]);
    let (_, d, _, _) = locations(&bad);
    bad[d + 84] = 0;
    assert!(gc_link(&[bad, dead]).unwrap_err().contains("row PCs"));
}

#[test]
fn gc_rewritten_section_references_reject_before_symbols_are_pruned() {
    let mut metadata = valid();
    // Define a symbol at the table start without changing the input's geometry.
    let f = object::File::parse(metadata.as_slice()).unwrap();
    let index = f.section_by_name(".sframe").unwrap().index().0;
    let symbol = f
        .symbols()
        .find(|s| s.name().unwrap() == "noncode")
        .unwrap()
        .index()
        .0;
    let symtab = f
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    metadata[symtab + symbol * 24 + 6..symtab + symbol * 24 + 8]
        .copy_from_slice(&(index as u16).to_le_bytes());
    let reference = object(".section .data.keep,\"awR\"\n.quad noncode+7\n");
    assert!(
        gc_link(&[metadata, reference])
            .unwrap_err()
            .contains("references into rewritten .sframe")
    );
}

#[test]
fn gc_retained_dwarf_references_into_sframe_reject_before_rewriting() {
    // A section symbol survives SFrame rewriting, so silently retaining this
    // pointer would let its addend address a different record (or past EOF).
    for directive in [
        ".cfi_personality 155,sframe_anchor",
        ".cfi_lsda 27,sframe_anchor",
    ] {
        for symbol_type in [0x10, 0x13] {
            for mask in 0..4 {
                let metadata = fixture_extra(
                    ["dead", "dead+48", "dead+64"],
                    ".section .sframe\n.globl sframe_anchor\n.set sframe_anchor,0\n.section .text.dead,\"ax\"\ndead: .zero 80\n",
                );
                // Turn the alias into a global anchor or section symbol.
                let mut metadata = metadata;
                let f = object::File::parse(metadata.as_slice()).unwrap();
                let index = f.section_by_name(".sframe").unwrap().index().0;
                let symbol = f
                    .symbols()
                    .find(|s| s.name().unwrap() == "sframe_anchor")
                    .unwrap()
                    .index()
                    .0;
                let symtab = f
                    .section_by_name(".symtab")
                    .unwrap()
                    .file_range()
                    .unwrap()
                    .0 as usize;
                metadata[symtab + symbol * 24 + 4] = symbol_type;
                metadata[symtab + symbol * 24 + 6..symtab + symbol * 24 + 8]
                    .copy_from_slice(&(index as u16).to_le_bytes());
                // Identical CFI shares a CIE; retain either/both/neither owner.
                let source: String = (0..2)
                    .map(|i| {
                        format!(
                            ".section .text.framed{i},\"ax{}\"\nframed{i}:\n.cfi_startproc\n{directive}\nret\n.cfi_endproc\n",
                            if mask & (1 << i) != 0 { "R" } else { "" },
                        )
                    })
                    .collect();
                let mut code = object(&source);
                let f = object::File::parse(code.as_slice()).unwrap();
                let symbol = f
                    .symbols()
                    .find(|s| s.name().unwrap() == "sframe_anchor")
                    .unwrap()
                    .index()
                    .0;
                let (start, size) = f
                    .section_by_name(".rela.eh_frame")
                    .unwrap()
                    .file_range()
                    .unwrap();
                for at in (start as usize..(start + size) as usize).step_by(24) {
                    if u64_at(&code, at + 8) >> 32 == symbol as u64 {
                        put64(&mut code, at + 16, 60);
                    }
                }
                assert!(link(&[metadata.clone(), code.clone()]).is_ok());
                let result = gc_link(&[metadata, code]);
                if mask != 0 {
                    let error = result.expect_err("retained DWARF reference was silently accepted");
                    assert!(
                        error.contains("references into rewritten .sframe"),
                        "{directive}: {error}"
                    );
                } else {
                    assert!(output_records(&result.unwrap()).is_empty()); // Discarded pointers are harmless.
                }
            }
        }
    }
}

#[test]
fn gc_comdat_code_with_ordinary_sframe_has_normal_ownership() {
    let code = object(
        ".section .text.first,\"axG\",@progbits,key,comdat\n.globl first\nfirst: .zero 40\n.section .text.peer,\"axG\",@progbits,key,comdat\npeer: ret\n.section .text.middle,\"ax\"\n.globl middle\nmiddle: .zero 8\n.section .text.last,\"ax\"\n.globl last\nlast: .zero 8\n",
    );
    let roots = object(".section .data.keep,\"awR\"\n.quad first\n");
    let exe = gc_link(&[fixture(["first", "middle", "last"]), code.clone(), roots]).unwrap();
    assert_eq!(output_records(&exe).len(), 1);
    assert!(
        object::File::parse(exe.bytes.as_slice())
            .unwrap()
            .section_by_name(".text.peer")
            .is_some()
    );
    let exe = gc_link(&[fixture(["first", "middle", "last"]), code]).unwrap();
    assert!(output_records(&exe).is_empty());
}

#[test]
fn gc_sframe_and_dwarf_compact_together_before_synthetic_layout() {
    let code = object(
        ".section .text.first,\"axR\"\n.globl first\nfirst:\n.cfi_startproc\n.zero 40\n.cfi_endproc\n.section .text.middle,\"ax\"\n.globl middle\nmiddle:\n.cfi_startproc\n.zero 8\n.cfi_endproc\n.section .text.last,\"axR\"\n.globl last\nlast:\n.cfi_startproc\n.zero 8\n.cfi_endproc\n",
    );
    let extra = object(".section .text.extra,\"axR\"\nextra:\n.cfi_startproc\nret\n.cfi_endproc\n");
    for objects in [
        vec![
            fixture(["first", "middle", "last"]),
            extra.clone(),
            code.clone(),
        ],
        vec![extra, code, fixture(["first", "middle", "last"])],
    ] {
        let exe = gc_link(&objects).unwrap();
        assert_eq!(output_records(&exe).len(), 2);
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(
            u32_at(
                f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap(),
                8
            ),
            3
        );
    }
}

#[test]
fn gc_none_relocation_ignores_offset_symbol_and_addend() {
    let mut bytes = fixture_extra(["_start", "_start+48", "_start+64"], ".quad absent\n");
    let (sh, _, _, r) = locations(&bytes);
    put64(&mut bytes, sh + 32, 102);
    put64(&mut bytes, r + 72, u64::MAX);
    put64(&mut bytes, r + 80, (u32::MAX as u64) << 32);
    put64(&mut bytes, r + 88, i64::MAX as u64);
    assert_eq!(
        gc_link(&[bytes]).unwrap().bytes,
        gc_link(&[valid()]).unwrap().bytes
    );
}

#[test]
fn gc_local_code_identity_survives_same_named_global_and_compaction() {
    let metadata = fixture_extra(
        ["local_code", "local_code+48", "local_code+64"],
        ".section .text.local,\"axR\"\nlocal_code: .zero 80\n",
    );
    let other = object(".section .text.other,\"ax\"\n.globl local_code\nlocal_code: .zero 80\n");
    let exe = gc_link(&[metadata, other]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let address = f.section_by_name(".text.local").unwrap().address();
    assert_eq!(
        output_records(&exe).iter().map(|r| r.0).collect::<Vec<_>>(),
        [address, address + 48, address + 64]
    );
    assert!(f.section_by_name(".text.other").is_none());
}

#[test]
fn gc_sorts_records_for_lifecycle_code_contribution_groups() {
    let code = [
        object(".section .init,\"ax\",@progbits\n.globl first\nfirst: .zero 40\n"),
        object(".section .text.other,\"axR\",@progbits\n.globl middle\nmiddle: .zero 8\n"),
        object(".section .init,\"ax\",@progbits\n.globl last\nlast: .zero 8\n"),
    ];
    let mut objects = vec![fixture(["first", "middle", "last"])];
    objects.extend(code);
    let exe = gc_link(&objects).unwrap();
    assert_eq!(
        output_records(&exe).iter().map(|r| r.0).collect::<Vec<_>>(),
        ["first", "last", "middle"].map(|n| symbol_address(&exe, n).unwrap())
    );
}
