use keel_link::{Dylib, Input, Options};
use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::BTreeSet;

fn link(source: &str) -> keel_link::Result<keel_link::Executable> {
    let bytes = keel_asm::assemble_object(source).unwrap();
    keel_link::link(
        &[Input {
            name: "unwind.o",
            bytes: &bytes,
        }],
        Options {
            dylibs: vec![Dylib {
                install_name: "/usr/lib/libSystem.B.dylib".into(),
                exports: BTreeSet::from(["_p0".into(), "_p1".into(), "_p2".into()]),
                weak_exports: BTreeSet::new(),
            }],
            ..Options::default()
        },
    )
}
fn word(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}
fn pointer(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
// Read every referenced format extent, including sentinel/LSDA indexes and the
// final regular page. Checking the last nonzero byte would miss unused entries.
type ParsedTable = (Vec<(usize, usize)>, usize, Vec<(usize, usize)>, usize);
fn table(b: &[u8]) -> ParsedTable {
    assert_eq!(word(b, 0), 1);
    assert_eq!(word(b, 8), 0);
    let index = word(b, 20);
    let count = word(b, 24);
    let mut entries = Vec::new();
    let mut lsdas = Vec::new();
    let mut used = index + count * 12;
    for i in 0..count - 1 {
        let at = index + i * 12;
        let page = word(b, at + 4);
        assert_eq!(word(b, page), 2);
        let offset = u16::from_le_bytes(b[page + 4..page + 6].try_into().unwrap()) as usize;
        let n = u16::from_le_bytes(b[page + 6..page + 8].try_into().unwrap()) as usize;
        assert!(n > 0 && n <= 511);
        assert_eq!(word(b, at), word(b, page + offset));
        used = used.max(page + offset + n * 8);
        for j in 0..n {
            entries.push((
                word(b, page + offset + j * 8),
                word(b, page + offset + j * 8 + 4),
            ));
        }
        let start = word(b, at + 8);
        let end = word(b, at + 20);
        assert!(start <= end);
        for cursor in (start..end).step_by(8) {
            lsdas.push((word(b, cursor), word(b, cursor + 4)));
        }
        used = used.max(end);
    }
    assert_eq!(word(b, index + (count - 1) * 12 + 4), 0);
    (entries, word(b, index + (count - 1) * 12), lsdas, used)
}

#[test]
fn regular_pages_gaps_personalities_lsdas_and_final_relocations_are_exact() {
    for count in [0usize, 1, 510, 511, 512, 1022, 1023] {
        for gaps in [false, true] {
            let mut source = String::from(".text\n.globl _main\n_main:\n");
            for i in 0..count {
                source.push_str(&format!(".globl _f{i}\n_f{i}:\nret\n"));
                if gaps && i + 1 < count {
                    source.push_str("nop\n");
                }
            }
            if count == 0 {
                source.push_str("ret\n");
            }
            source.push_str(".section __TEXT,__gcc_except_tab\n.globl _lsda\n_lsda:\n.long 0\n.data\n.quad _main\n.quad _lsda\n.section __LD,__compact_unwind\n");
            for i in 0..count {
                let encoding = 0x02001000u32 | if i % 3 == 0 { 0x40000000 } else { 0 };
                source.push_str(&format!(
                    ".quad _f{i}\n.long 4\n.long {encoding}\n.quad _p{}\n",
                    i % 3
                ));
                source.push_str(if i % 3 == 0 {
                    ".quad _lsda\n"
                } else {
                    ".quad 0\n"
                });
            }
            let exe = link(&source).unwrap();
            let file = object::File::parse(exe.bytes.as_slice()).unwrap();
            let symbols: std::collections::BTreeMap<_, _> = file
                .symbols()
                .map(|s| (s.name().unwrap(), s.address()))
                .collect();
            let unwind = file.section_by_name("__unwind_info").unwrap();
            let b = unwind.data().unwrap();
            let (entries, sentinel, lsdas, used) = table(b);
            let ranges = count + if gaps { count.saturating_sub(1) } else { 0 };
            assert_eq!(entries.len(), ranges);
            let personalities = count.min(3);
            assert_eq!(word(b, 16), personalities);
            let lsda_count = count.div_ceil(3);
            assert_eq!(lsdas.len(), lsda_count);
            assert_eq!(
                b.len(),
                40 + personalities * 4 + ranges.div_ceil(511) * 20 + ranges * 8 + lsda_count * 8
            );
            assert_eq!(used, b.len(), "no unreferenced reservation tail");
            assert_eq!(unwind.size() as usize, b.len());
            for i in 0..count {
                let start = (symbols[format!("_f{i}").as_str()] - 0x1_0000_0000) as usize;
                let at = if gaps { i * 2 } else { i };
                assert_eq!(entries[at].0, start);
                let encoding =
                    0x02001000 | ((i % 3 + 1) << 28) | if i % 3 == 0 { 0x40000000 } else { 0 };
                assert_eq!(entries[at].1, encoding);
                if gaps && i + 1 < count {
                    assert_eq!(entries[at + 1], (start + 4, 0));
                }
                if i % 3 == 0 {
                    assert_eq!(
                        lsdas[i / 3],
                        (start, (symbols["_lsda"] - 0x1_0000_0000) as usize)
                    );
                }
            }
            let got = file.section_by_name("__got").unwrap();
            let mut personality_offsets: Vec<_> =
                (0..personalities).map(|i| word(b, 28 + i * 4)).collect();
            personality_offsets.sort_unstable();
            assert_eq!(
                personality_offsets,
                (0..personalities)
                    .map(|i| (got.address() - 0x1_0000_0000) as usize + i * 8)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                sentinel,
                if count == 0 {
                    0
                } else {
                    (symbols[format!("_f{}", count - 1).as_str()] - 0x1_0000_0000) as usize + 4
                }
            );
            let data = file.section_by_name("__data").unwrap().data().unwrap();
            assert_eq!(pointer(data, 0), symbols["_main"]);
            assert_eq!(pointer(data, 8), symbols["_lsda"]);
        }
    }
}

#[test]
fn planning_accounts_for_address_dependent_gap_changes() {
    // At the conservative size __DATA starts at 0xc000. Shrinking unwind moves
    // it to 0x8000 and removes the gap following _f2. Another plan is required.
    let source = ".text\n.globl _main\n_main:\nret\n_f1:\nret\n_f2:\nret\n.space 16268\n.data\n_f3:\n.long 0\n.quad _main\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n.long 0x02001000\n.quad 0\n.quad 0\n.quad _f1\n.long 4\n.long 0x02001000\n.quad 0\n.quad 0\n.quad _f2\n.long 16376\n.long 0x02001000\n.quad 0\n.quad 0\n.quad _f3\n.long 4\n.long 0x02001000\n.quad 0\n.quad 0\n";
    for (length, expected_size, data_address, range_count, boundary) in [
        (16376, 92, 0x8000, 4, false),
        (16380, 148, 0xc000, 5, false),
        (16376, 92, 0x8000, 4, true),
        (16380, 148, 0xc000, 5, true),
    ] {
        // A deferred boundary target must move with the section during both
        // provisional planning and final relocation, just like its definition.
        let source = if boundary {
            source.replace(".quad _f3\n", ".quad section$start$__DATA$__data\n")
        } else {
            source.to_owned()
        };
        let exe = link(&source.replace(".long 16376", &format!(".long {length}"))).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let b = file
            .section_by_name("__unwind_info")
            .unwrap()
            .data()
            .unwrap();
        let (entries, sentinel, _, used) = table(b);
        assert_eq!(entries.len(), range_count);
        assert_eq!(entries.last().unwrap().0, data_address);
        assert_eq!(sentinel, data_address + 4);
        assert_eq!(b.len(), expected_size);
        if length == 16376 {
            assert_eq!(used, b.len());
        } else {
            // The smaller provisional layout overlaps _f2 with _f3. Retain the
            // checked bound and validate/relocate against its final addresses.
            assert_eq!(used, 100);
            assert!(b[used..].iter().all(|b| *b == 0));
        }
        let data = file.section_by_name("__data").unwrap().data().unwrap();
        assert_eq!(pointer(data, 4), 0x1_0000_4000);
    }
}

#[test]
fn weak_copies_keep_their_own_dwarf_unwind_entries() {
    // An x28 CFA forces DWARF mode. The copies differ in length, so mapping
    // the kept copy to the other object's FDE gives it the wrong range.
    // Without subsections the FDE names a local label. With them, as in
    // compiled output, it names _shared, which resolves to the kept copy.
    let copy = |directives: &str, body: &str| {
        format!(
            ".globl _shared\n{directives}_shared:\n.cfi_startproc\n.cfi_def_cfa x28,16\n{body}ret\n.cfi_endproc\n"
        )
    };
    let weak = ".weak_definition _shared\n";
    let hidden = ".private_extern _shared\n.weak_definition _shared\n";
    for (subsections, first, second) in [
        ("", weak, weak),
        (".subsections_via_symbols\n", weak, weak),
        (".subsections_via_symbols\n", hidden, hidden),
        (".subsections_via_symbols\n", weak, ""),
    ] {
        let first = keel_asm::assemble_object(&format!(
            "{subsections}.globl _main\n_main:\nbl _shared\nb _other\n{}",
            copy(first, "")
        ))
        .unwrap();
        let strong = second.is_empty();
        let second = keel_asm::assemble_object(&format!(
            "{subsections}.globl _other\n_other:\nb _shared\n{}",
            copy(second, "nop\n")
        ))
        .unwrap();
        // A strong definition wins, otherwise the first weak one does. Both
        // copies share a section with live code, so dead_strip keeps both.
        for (objects, lengths) in [([&first, &second], (4, 8)), ([&second, &first], (8, 4))] {
            let (kept, other) = if strong { (8, 4) } else { lengths };
            for dead_strip in [false, true] {
                let inputs = objects.map(|bytes| Input {
                    name: "weak.o",
                    bytes,
                });
                let exe = keel_link::link(
                    &inputs,
                    Options {
                        dead_strip,
                        dylibs: vec![Dylib {
                            install_name: "/usr/lib/libSystem.B.dylib".into(),
                            exports: BTreeSet::new(),
                            weak_exports: BTreeSet::new(),
                        }],
                        ..Options::default()
                    },
                )
                .unwrap();
                let file = object::File::parse(exe.bytes.as_slice()).unwrap();
                let eh_frame = file.section_by_name("__eh_frame").unwrap();
                let fdes = eh_frame.data().unwrap();
                let unwind = file.section_by_name("__unwind_info").unwrap();
                let (entries, sentinel, _, _) = table(unwind.data().unwrap());
                let mut ranges = Vec::new();
                for (i, &(start, encoding)) in entries.iter().enumerate() {
                    if encoding & 0x0f000000 != 0x03000000 {
                        continue;
                    }
                    let end = entries.get(i + 1).map_or(sentinel, |e| e.0);
                    let fde = encoding & 0xffffff;
                    let field = eh_frame.address() + fde as u64 + 8;
                    let begin = field.wrapping_add(pointer(fdes, fde + 8));
                    assert_eq!(begin, 0x1_0000_0000 + start as u64);
                    assert_eq!(pointer(fdes, fde + 16), (end - start) as u64);
                    ranges.push((begin, end - start));
                }
                let shared = file
                    .symbols()
                    .find(|s| s.name() == Ok("_shared"))
                    .unwrap()
                    .address();
                ranges.sort_by_key(|&(begin, _)| begin != shared);
                assert_eq!(ranges.len(), 2);
                assert_eq!((ranges[0], ranges[1].1), ((shared, kept), other));
            }
        }
    }
}

#[test]
fn tlv_metadata_replans_when_an_untracked_tls_template_moves() {
    let source = ".text\n.globl _main\n_main:\nret\n.section __TEXT,__txttls\n.quad 0\n.tbss _tls,8,3\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n.long 0\n.quad 0\n.quad 0\n.quad 0x100004004\n.long 124\n.long 0\n.quad 0\n.quad 0\n.quad _tls+4294967552\n.long 0\n.long 0\n.quad 0\n.quad 0\n";
    let mut bytes = keel_asm::assemble_object(source).unwrap();
    // The assembler does not spell these unusual section types. Each padded
    // section name occurs once in this fixture's Mach-O section headers.
    for (name, flags) in [("__txttls", 0x11u32), ("__compact_unwind", 0x13u32)] {
        let mut padded = [0; 16];
        padded[..name.len()].copy_from_slice(name.as_bytes());
        let offsets: Vec<_> = bytes
            .windows(16)
            .enumerate()
            .filter_map(|(i, b)| (b == padded).then_some(i))
            .collect();
        assert_eq!(offsets.len(), 1);
        let at = offsets[0] + 64;
        bytes[at..at + 4].copy_from_slice(&flags.to_le_bytes());
    }
    let exe = keel_link::link(
        &[Input {
            name: "tlv-metadata.o",
            bytes: &bytes,
        }],
        Options {
            dylibs: vec![Dylib {
                install_name: "/usr/lib/libSystem.B.dylib".into(),
                exports: BTreeSet::new(),
                weak_exports: BTreeSet::new(),
            }],
            ..Options::default()
        },
    )
    .unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let unwind = file
        .section_by_name("__unwind_info")
        .unwrap()
        .data()
        .unwrap();
    assert_eq!(unwind.len(), 92);
    let (entries, sentinel, _, used) = table(unwind);
    assert_eq!(
        entries.iter().map(|e| e.0).collect::<Vec<_>>(),
        [0x4000, 0x4004, 0x4080, 0x40a0]
    );
    assert_eq!(sentinel, 0x40a0);
    assert_eq!(used, unwind.len());
    assert_eq!(
        file.section_by_name("__txttls").unwrap().address(),
        0x1_0000_4060
    );
}

#[test]
fn malformed_metadata_and_relocation_ranges_remain_errors() {
    let prefix =
        ".text\n.globl _main\n_main:\nret\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n";
    for (tail, diagnostic) in [
        (".long 0x03000000\n.quad 0\n.quad 0\n", "no relocated FDE"),
        (
            ".long 0x40000000\n.quad 0\n.quad 0\n",
            "LSDA flag/pointer mismatch",
        ),
        (
            ".long 0x05000000\n.quad 0\n.quad 0\n",
            "unknown arm64 compact unwind mode",
        ),
        (".long 0\n", "not a multiple of 32"),
    ] {
        assert!(
            link(&format!("{prefix}{tail}"))
                .err()
                .unwrap()
                .contains(diagnostic)
        );
    }
    let overlapping = format!(
        "{prefix}.long 0\n.quad 0\n.quad 0\n.quad _main\n.long 4\n.long 0\n.quad 0\n.quad 0\n"
    );
    assert!(
        link(&overlapping)
            .err()
            .unwrap()
            .contains("overlapping compact unwind")
    );
    let overflow = ".globl _main\n_main:\nret\n.section __LD,__compact_unwind\n.quad 0x1ffffffff\n.long 4\n.long 0\n.quad 0\n.quad 0\n";
    assert!(
        link(overflow)
            .err()
            .unwrap()
            .contains("unwind function range overflow")
    );
}
