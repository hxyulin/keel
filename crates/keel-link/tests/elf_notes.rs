use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::BTreeMap;

const AND: u32 = 0xc0000002;
const NEEDED: u32 = 0xc0008002;
const FEATURE_USED: u32 = 0xc0010001;
const ISA_USED: u32 = 0xc0010002;
const START: &str = ".text\n.globl _start\n_start: ret\n";
const ABI: &[u8] = include_bytes!("fixtures/gnu-notes/crt1.o.note.ABI-tag.bin");
const SPLIT: &[u8] = include_bytes!("fixtures/gnu-notes/split.o.note.gnu.property.bin");

fn obj(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(inputs: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &inputs
            .iter()
            .map(|bytes| Input {
                name: "notes.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn u32at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}
fn u64at(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap())
}
fn put32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(data: &mut [u8], at: usize, value: u64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn note(records: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for v in [4u32, records.len() as u32 * 16, 5, 0x00554e47] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for &(id, value) in records {
        for v in [id, 4, value, 0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes
}
fn section(name: &str, data: &[u8]) -> String {
    let power = if name == ".note.ABI-tag" { 2 } else { 3 };
    let bytes = data.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    format!(
        ".section {name},\"a\",@note\n.p2align {power}\n{}",
        if data.is_empty() {
            String::new()
        } else {
            format!(".byte {bytes}\n")
        }
    )
}
fn marked(source: &str, records: &[(u32, u32)]) -> Vec<u8> {
    obj(&format!(
        "{source}{}",
        section(".note.gnu.property", &note(records))
    ))
}
// Independently decode canonical output, checking uniqueness/order and padding.
fn decode(data: &[u8]) -> BTreeMap<u32, u32> {
    assert_eq!(u32at(data, 0), 4);
    assert_eq!(u32at(data, 4) as usize, data.len() - 16);
    assert_eq!(u32at(data, 8), 5);
    assert_eq!(&data[12..16], b"GNU\0");
    let mut result = BTreeMap::new();
    let mut previous = 0;
    for row in data[16..].as_chunks::<16>().0 {
        let id = u32at(row, 0);
        assert!(id > previous);
        assert_eq!(u32at(row, 4), 4);
        assert_eq!(&row[12..], &[0; 4]);
        assert!(result.insert(id, u32at(row, 8)).is_none());
        previous = id;
    }
    assert_eq!(data.len(), 16 + result.len() * 16);
    result
}
fn output(exe: &elf::Executable) -> BTreeMap<u32, u32> {
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let notes = f
        .sections()
        .filter(|s| s.name().unwrap() == ".note.gnu.property")
        .collect::<Vec<_>>();
    assert!(notes.len() <= 1);
    notes
        .first()
        .map(|s| decode(s.data().unwrap()))
        .unwrap_or_default()
}
fn locations(bytes: &[u8], name: &str) -> (usize, usize) {
    let f = object::File::parse(bytes).unwrap();
    let s = f.section_by_name(name).unwrap();
    (
        u64at(bytes, 40) as usize + s.index().0 * 64,
        s.file_range().unwrap().0 as usize,
    )
}
fn archive(members: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"!<arch>\n".to_vec();
    for (i, data) in members.iter().enumerate() {
        bytes.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("m{i}.o/"),
                0,
                0,
                0,
                "100644",
                data.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(data);
        if !bytes.len().is_multiple_of(2) {
            bytes.push(b'\n');
        }
    }
    bytes
}

#[test]
fn duplicate_types_within_and_across_unsorted_notes_are_ored_per_object() {
    let mut split = note(&[(AND, 1), (AND, 2), (NEEDED, 8), (ISA_USED, 2)]);
    split.extend(note(&[(ISA_USED, 4), (FEATURE_USED, 8), (NEEDED, 1)]));
    let a = obj(&format!("{START}{}", section(".note.gnu.property", &split)));
    let b = marked(
        "",
        &[(AND, 2), (NEEDED, 2), (ISA_USED, 0), (FEATURE_USED, 1)],
    );
    let expected = BTreeMap::from([(AND, 2), (NEEDED, 11), (FEATURE_USED, 9), (ISA_USED, 6)]);
    assert_eq!(output(&link(&[a.clone(), b.clone()]).unwrap()), expected);
    assert_eq!(output(&link(&[b, a]).unwrap()), expected);
}

#[test]
fn missing_and_present_zero_have_distinct_or_and_semantics() {
    let a = marked(
        START,
        &[(AND, 2), (NEEDED, 15), (ISA_USED, 4), (FEATURE_USED, 8)],
    );
    let present_zero = marked("", &[(ISA_USED, 0), (FEATURE_USED, 0)]);
    assert_eq!(
        output(&link(&[a.clone(), present_zero]).unwrap()),
        BTreeMap::from([(NEEDED, 15), (ISA_USED, 4), (FEATURE_USED, 9)])
    );
    for propertyless in [obj(""), obj(".text\nhelper: ret\n")] {
        assert_eq!(
            output(&link(&[a.clone(), propertyless]).unwrap()),
            BTreeMap::from([(NEEDED, 15)])
        );
    }
    assert_eq!(
        output(&link(&[marked(START, &[(ISA_USED, 0)])]).unwrap()),
        BTreeMap::from([(ISA_USED, 0)])
    );
    assert_eq!(
        output(&link(&[marked(START, &[(FEATURE_USED, 0)])]).unwrap()),
        BTreeMap::from([(FEATURE_USED, 1)])
    );
}

#[test]
fn zero_and_and_needed_properties_produce_no_note_or_segment() {
    let exe = link(&[marked(START, &[(AND, 0), (NEEDED, 0)])]).unwrap();
    assert!(output(&exe).is_empty());
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(f.section_by_name(".note.gnu.property").is_none());
    for p in exe.bytes
        [64..64 + u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize * 56]
        .as_chunks::<56>()
        .0
    {
        assert!(![object::elf::PT_NOTE, object::elf::PT_GNU_PROPERTY].contains(&u32at(p, 0)));
    }
}

#[test]
fn selected_empty_archive_members_count_but_unselected_metadata_does_not() {
    let start = marked(START, &[(AND, 2), (NEEDED, 8), (ISA_USED, 1)]);
    let unused = marked(".globl unused\n.set unused,7\n", &[(0xdeadbeef, 1)]);
    let empty = obj(".globl demanded\n.set demanded,7\n");
    let ar = archive(&[unused, empty]);
    assert_eq!(
        output(&link(&[start.clone(), ar.clone()]).unwrap()),
        BTreeMap::from([(AND, 2), (NEEDED, 8), (ISA_USED, 1)])
    );
    let demand = marked(
        ".text\n.globl _start\n_start: ret\n.data\n.quad demanded\n",
        &[(AND, 2), (NEEDED, 8), (ISA_USED, 1)],
    );
    assert_eq!(
        output(&link(&[demand, ar]).unwrap()),
        BTreeMap::from([(NEEDED, 8)])
    );
    let selected = marked(
        ".text\n.globl demanded\ndemanded: ret\n",
        &[(AND, 2), (NEEDED, 1), (ISA_USED, 2)],
    );
    let demand = marked(
        ".text\n.globl _start\n_start: call demanded\n",
        &[(AND, 2), (NEEDED, 8), (ISA_USED, 1)],
    );
    assert_eq!(
        output(&link(&[demand, archive(&[selected])]).unwrap()),
        BTreeMap::from([(AND, 2), (NEEDED, 9), (ISA_USED, 3)])
    );
}

// A portable DSO interface; no execution or claim about a real CRT follows.
fn dso(records: &[(u32, u32)]) -> Vec<u8> {
    let source = ".text\n.globl _start,answer\n.type answer,@function\n_start: ret\nanswer: ret\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n";
    let input = if records.is_empty() {
        obj(source)
    } else {
        marked(source, records)
    };
    let mut bytes = link(&[input]).unwrap().bytes;
    let (dynamic, _) = locations(&bytes, ".dynamic");
    let (sym, _) = locations(&bytes, ".symtab");
    let f = object::File::parse(bytes.as_slice()).unwrap();
    let strings = f.section_by_name(".strtab").unwrap().index().0;
    bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
    put32(&mut bytes, sym + 4, 11);
    put32(&mut bytes, dynamic + 4, 6);
    put32(&mut bytes, dynamic + 40, strings as u32);
    put64(&mut bytes, dynamic + 56, 16);
    bytes
}

#[test]
fn dso_properties_and_missing_dso_properties_do_not_join_relocatable_merge() {
    let main = marked(START, &[(AND, 2), (NEEDED, 8), (ISA_USED, 0)]);
    for shared in [dso(&[]), dso(&[(NEEDED, 1)])] {
        assert_eq!(
            output(&link(&[main.clone(), shared]).unwrap()),
            BTreeMap::from([(AND, 2), (NEEDED, 8), (ISA_USED, 0)])
        );
    }
    let mut shared = dso(&[(NEEDED, 1)]);
    let (_, payload) = locations(&shared, ".note.gnu.property");
    put32(&mut shared, payload + 16, 0xdeadbeef);
    assert_eq!(output(&link(&[main, shared]).unwrap())[&NEEDED], 8);
}

#[test]
fn generated_import_code_updates_only_complete_used_metadata() {
    let main = marked(
        ".text\n.globl _start\n_start: call answer\n",
        &[(ISA_USED, 0), (FEATURE_USED, 0)],
    );
    let exe = link(&[main, dso(&[])]).unwrap();
    assert_eq!(
        output(&exe),
        BTreeMap::from([(ISA_USED, 1), (FEATURE_USED, 1)])
    );
    let main = obj(".text\n.globl _start\n_start: call answer\n");
    assert!(output(&link(&[main, dso(&[(ISA_USED, 1)])]).unwrap()).is_empty());
}

#[test]
fn retained_crt_note_bytes_preserve_v4_with_unmarked_user_object() {
    let notes: &[&[u8]] = &[
        include_bytes!("fixtures/gnu-notes/crt1.o.note.gnu.property.bin"),
        include_bytes!("fixtures/gnu-notes/crti.o.note.gnu.property.bin"),
        include_bytes!("fixtures/gnu-notes/crtbegin.o.note.gnu.property.bin"),
        include_bytes!("fixtures/gnu-notes/crtend.o.note.gnu.property.bin"),
        include_bytes!("fixtures/gnu-notes/crtn.o.note.gnu.property.bin"),
    ];
    let mut objects = vec![obj(START)];
    objects.extend(
        notes
            .iter()
            .map(|data| obj(&section(".note.gnu.property", data))),
    );
    assert_eq!(
        output(&link(&objects).unwrap()),
        BTreeMap::from([(NEEDED, 15)])
    );
    // Without an unmarked object these actual bytes advertise IBT, which this
    // stage deliberately rejects even for otherwise static output.
    objects[0] = marked(START, &[(AND, 3)]);
    assert!(link(&objects).unwrap_err().contains("merged GNU IBT"));
}

#[test]
fn retained_gnu_oracle_matches_split_missing_usedzero_and_isazero() {
    let split = obj(&format!("{START}{}", section(".note.gnu.property", SPLIT)));
    let usedzero = obj(&section(
        ".note.gnu.property",
        include_bytes!("fixtures/gnu-notes/usedzero.o.note.gnu.property.bin"),
    ));
    for (extra, expected) in [
        (
            obj(""),
            include_bytes!("fixtures/gnu-notes/split-missing.note.gnu.property.bin").as_slice(),
        ),
        (
            usedzero,
            include_bytes!("fixtures/gnu-notes/split-usedzero.note.gnu.property.bin").as_slice(),
        ),
    ] {
        let exe = link(&[split.clone(), extra]).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(
            f.section_by_name(".note.gnu.property")
                .unwrap()
                .data()
                .unwrap(),
            expected
        );
    }
    let zero = obj(&format!(
        "{START}{}",
        section(
            ".note.gnu.property",
            include_bytes!("fixtures/gnu-notes/isazero.o.note.gnu.property.bin")
        )
    ));
    let exe = link(&[zero]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        f.section_by_name(".note.gnu.property")
            .unwrap()
            .data()
            .unwrap(),
        include_bytes!("fixtures/gnu-notes/isazero-gnu.note.gnu.property.bin")
    );
    for bytes in [
        SPLIT,
        include_bytes!("fixtures/gnu-notes/allprops.o.note.gnu.property.bin").as_slice(),
    ] {
        assert!(
            link(&[obj(&format!(
                "{START}{}",
                section(".note.gnu.property", bytes)
            ))])
            .unwrap_err()
            .contains("merged GNU IBT")
        );
    }
}

#[test]
fn canonical_note_segments_are_readonly_load_backed_and_header_counts_agree() {
    let mut props = note(&[(ISA_USED, 4), (NEEDED, 15), (AND, 2), (FEATURE_USED, 8)]);
    for i in 0..4 {
        props[28 + 16 * i..32 + 16 * i].fill(0xa5);
    }
    let main = obj(&format!(
        "{START}{}{}",
        section(".note.gnu.property", &props),
        section(".note.ABI-tag", ABI)
    ));
    let exe = link(&[main.clone(), dso(&[])]).unwrap();
    assert_eq!(exe.bytes, link(&[main, dso(&[])]).unwrap().bytes);
    assert_eq!(
        output(&exe),
        BTreeMap::from([(AND, 2), (NEEDED, 15), (FEATURE_USED, 9), (ISA_USED, 4)])
    );
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let property = f.section_by_name(".note.gnu.property").unwrap();
    let abi = f.section_by_name(".note.ABI-tag").unwrap();
    let phnum = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
    assert!(64 + 56 * phnum <= 4096);
    let headers = exe.bytes[64..64 + 56 * phnum]
        .as_chunks::<56>()
        .0
        .iter()
        .map(|p| p.as_slice())
        .collect::<Vec<_>>();
    let phdr = headers
        .iter()
        .find(|p| u32at(p, 0) == object::elf::PT_PHDR)
        .unwrap();
    assert_eq!(u64at(phdr, 32), (56 * phnum) as u64);
    assert_eq!(u64at(phdr, 40), (56 * phnum) as u64);
    for (kind, section, alignment) in [
        (object::elf::PT_GNU_PROPERTY, &property, 8),
        (object::elf::PT_NOTE, &property, 8),
        (object::elf::PT_NOTE, &abi, 4),
    ] {
        let matches = headers
            .iter()
            .filter(|p| u32at(p, 0) == kind && u64at(p, 16) == section.address())
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        let p = matches[0];
        assert_eq!(u32at(p, 4), 4);
        assert_eq!(u64at(p, 8), section.file_range().unwrap().0);
        assert_eq!(u64at(p, 32), section.size());
        assert_eq!(u64at(p, 40), section.size());
        assert_eq!(u64at(p, 48), alignment);
        assert!(headers.iter().any(|load| u32at(load, 0) == 1
            && u32at(load, 4) == 4
            && u64at(load, 8) <= u64at(p, 8)
            && u64at(p, 8) + section.size() <= u64at(load, 8) + u64at(load, 32)));
    }
}

#[test]
fn abi_bytes_and_local_symbol_addresses_retain_original_unit_identity() {
    let bytes = ABI.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    let first = obj(&format!(
        "{START}.data\n.globl abi_pointer\nabi_pointer: .quad __abi_tag+7\n.section .note.ABI-tag,\"a\",@note\n.p2align 2\n.type __abi_tag,@object\n__abi_tag: .byte {bytes}\n.size __abi_tag,32\n"
    ));
    let second = obj(
        ".data\n.globl other_pointer\nother_pointer: .quad __abi_tag\n.section .rodata,\"a\",@progbits\n__abi_tag: .quad 123\n",
    );
    let exe = link(&[first, second]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let abi = f.section_by_name(".note.ABI-tag").unwrap();
    assert_eq!(abi.data().unwrap(), ABI);
    let symbol = |name| {
        f.symbols()
            .find(|s| s.name().unwrap() == name)
            .unwrap()
            .address()
    };
    assert_eq!(
        u64at(&exe.bytes, (symbol("abi_pointer") - 0x400000) as usize),
        abi.address() + 7
    );
    assert_eq!(
        u64at(&exe.bytes, (symbol("other_pointer") - 0x400000) as usize),
        f.section_by_name(".rodata").unwrap().address()
    );
    assert!(!f.symbols().any(|s| s.name().unwrap() == "__abi_tag"));
}

#[test]
fn malformed_note_descriptors_and_unknown_contracts_fail_closed() {
    let valid = note(&[(NEEDED, 8)]);
    let mut cases = (0..valid.len())
        .map(|len| valid[..len].to_vec())
        .collect::<Vec<_>>();
    for (at, v) in [
        (0, 3),
        (4, 0),
        (4, 8),
        (4, 15),
        (4, u32::MAX),
        (8, 1),
        (12, 0),
        (16, 0xdeadbeef),
        (16, 0xc0008001),
        (20, 3),
        (20, 8),
        (24, 16),
    ] {
        let mut bad = valid.clone();
        put32(&mut bad, at, v);
        cases.push(bad);
    }
    let mut tail = valid.clone();
    tail.extend([0; 8]);
    cases.push(tail);
    for bad in cases {
        assert!(
            link(&[obj(&format!(
                "{START}{}",
                section(".note.gnu.property", &bad)
            ))])
            .is_err(),
            "accepted malformed bytes {bad:?}"
        );
    }
    // Reject unknown bits before a missing property can erase the input.
    for (id, value) in [(AND, 4), (ISA_USED, 16), (FEATURE_USED, 4096), (NEEDED, 16)] {
        let bad = marked("", &[(id, value)]);
        assert!(
            link(&[obj(START), bad])
                .unwrap_err()
                .contains("unsupported bits")
        );
    }
    assert!(
        link(&[marked(START, &[(AND, 1)])])
            .unwrap_err()
            .contains("merged GNU IBT")
    );
    assert!(link(&[marked(START, &[(AND, 1)]), obj("")]).is_ok());
}

#[test]
fn section_metadata_abi_bounds_and_additional_abi_contracts_are_checked() {
    for name in [".note.gnu.property", ".note.ABI-tag"] {
        let data = if name == ".note.ABI-tag" {
            ABI.to_vec()
        } else {
            note(&[(NEEDED, 8)])
        };
        let valid = obj(&format!("{START}{}", section(name, &data)));
        let (header, _) = locations(&valid, name);
        for (at, value, wide) in [
            (4, 1, false),
            (8, 0, true),
            (8, 3, true),
            (8, 6, true),
            (8, 0x802, true),
            (40, 1, false),
            (44, 1, false),
            (48, 1, true),
            (48, 16, true),
            (56, 4, true),
        ] {
            let mut bad = valid.clone();
            if wide {
                put64(&mut bad, header + at, value);
            } else {
                put32(&mut bad, header + at, value as u32);
            }
            assert!(link(&[bad]).unwrap_err().contains("requires SHT_NOTE"));
        }
    }
    for bad in [vec![], ABI[..31].to_vec(), [ABI, ABI].concat()] {
        assert!(link(&[obj(&format!("{START}{}", section(".note.ABI-tag", &bad)))]).is_err());
    }
    for (at, value) in [(0, 3), (4, 15), (8, 2), (12, 0), (16, 1)] {
        let mut bad = ABI.to_vec();
        put32(&mut bad, at, value);
        assert!(link(&[obj(&format!("{START}{}", section(".note.ABI-tag", &bad)))]).is_err());
    }
    let abi = obj(&section(".note.ABI-tag", ABI));
    assert!(
        link(&[obj(START), abi.clone(), abi])
            .unwrap_err()
            .contains("additional .note.ABI-tag")
    );
    for source in [
        ".section .note.unknown,\"a\",@note\n.byte 0\n",
        ".section .unknown,\"a\",@note\n.byte 0\n",
        ".section .note.unknown,\"a\",@progbits\n.byte 0\n",
    ] {
        assert!(
            link(&[obj(&format!("{START}{source}"))])
                .unwrap_err()
                .contains("unsupported allocated note")
        );
    }
}

#[test]
fn note_relocations_and_references_into_consumed_properties_are_rejected() {
    for name in [".note.gnu.property", ".note.ABI-tag"] {
        let source = format!(
            "{START}.section {name},\"a\",@note\n.p2align {}\n.quad _start\n",
            if name == ".note.ABI-tag" { 2 } else { 3 }
        );
        assert!(
            link(&[obj(&source)])
                .unwrap_err()
                .contains("relocations into")
        );
    }
    let payload = note(&[(NEEDED, 8)])
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    for reference in [
        ".data\n.quad prop+16\n",
        ".text\nmov rax,qword ptr [rip+prop@GOTPCREL]\n",
    ] {
        let source = format!(
            "{START}{reference}.section .note.gnu.property,\"a\",@note\n.p2align 3\nprop: .byte {payload}\n"
        );
        assert!(
            link(&[obj(&source)])
                .unwrap_err()
                .contains("reference into consumed GNU property")
        );
    }
    let provider = obj(&format!(
        ".section .note.gnu.property,\"a\",@note\n.p2align 3\n.globl prop\nprop: .byte {payload}\n"
    ));
    let user = obj(&format!("{START}.data\n.quad prop\n"));
    assert!(
        link(&[user, provider])
            .unwrap_err()
            .contains("reference into consumed GNU property")
    );
}

// Retained independent-review DSO: real LOAD/DYNAMIC/hash and data relocation.
const REVIEW_DSO: &[u8] = include_bytes!("fixtures/gnu-notes/libreview.so");
const DEFINED_DSO: &[u8] = include_bytes!("fixtures/gnu-notes/libdefined.so");
const WEAK_DEFINED_DSO: &[u8] = include_bytes!("fixtures/gnu-notes/libweak-defined.so");
const PROTECTED_DSO: &[u8] = include_bytes!("fixtures/gnu-notes/libprotected-defined.so");

fn symbol_location(bytes: &[u8], table: &str, name: &str) -> usize {
    let file = object::File::parse(bytes).unwrap();
    let index = if table == ".dynsym" {
        file.dynamic_symbols()
            .find(|s| s.name().unwrap() == name)
            .unwrap()
            .index()
            .0
    } else {
        file.symbols()
            .find(|s| s.name().unwrap() == name)
            .unwrap()
            .index()
            .0
    };
    locations(bytes, table).1 + 24 * index
}
fn property_provider(binding: &str) -> Vec<u8> {
    obj(
        &section(".note.gnu.property", &note(&[(NEEDED, 8)])).replace(
            ".byte",
            &format!("{binding}\n.type prop,@object\nprop: .byte"),
        ),
    )
}
fn exports_prop(exe: &elf::Executable, weak: bool) {
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let prop = file
        .dynamic_symbols()
        .find(|s| s.name().unwrap() == "prop")
        .unwrap();
    assert!(!prop.is_undefined());
    assert_eq!(prop.is_weak(), weak);
    let section = file
        .section_by_index(prop.section_index().unwrap())
        .unwrap();
    assert_eq!(section.name().unwrap(), ".data");
    assert_eq!(prop.address(), section.address());
}

#[test]
fn relocation_associations_are_checked_before_note_consumption() {
    let ordinary = obj(&format!("{START}.data\n.quad _start\n"));
    assert!(link(std::slice::from_ref(&ordinary)).is_ok());
    for (name, payload) in [
        (".note.gnu.property", note(&[(NEEDED, 8)])),
        (".note.ABI-tag", ABI.to_vec()),
    ] {
        let valid = obj(&format!(
            "{START}.data\n.quad _start\n{}",
            section(name, &payload)
        ));
        let (rh, rp) = locations(&valid, ".rela.data");
        let f = object::File::parse(valid.as_slice()).unwrap();
        let note_index = f.section_by_name(name).unwrap().index().0 as u32;
        let wrong_table = f.section_by_name(".strtab").unwrap().index().0 as u32;
        let mut incoming = valid.clone();
        put32(&mut incoming, rh + 44, note_index);
        put64(&mut incoming, rp, 24);
        assert!(
            link(&[incoming.clone()])
                .unwrap_err()
                .contains("relocations into")
        );
        for (template, expected) in [
            (incoming, "relocations into"),
            (
                obj(&format!(
                    "{START}.data\n.quad prop\n{}",
                    section(name, &payload).replace(".byte", "prop: .byte")
                )),
                if name == ".note.ABI-tag" {
                    ""
                } else {
                    "reference into consumed"
                },
            ),
        ] {
            let (rh, _) = locations(&template, ".rela.data");
            if expected.is_empty() {
                assert!(link(std::slice::from_ref(&template)).is_ok()); // retained ABI address identity
            } else {
                assert!(
                    link(std::slice::from_ref(&template))
                        .unwrap_err()
                        .contains(expected)
                );
            }
            for bad_link in [0, u32::MAX, wrong_table, note_index] {
                let mut bad = template.clone();
                put32(&mut bad, rh + 40, bad_link);
                let err = link(&[bad]).unwrap_err();
                assert!(
                    err.contains("relocation table") && err.contains("sh_link"),
                    "{err}"
                );
            }
        }
        // A valid association must also name a target: object skips sh_info=0.
        let mut bad = valid;
        put32(&mut bad, rh + 44, 0);
        assert!(link(&[bad]).unwrap_err().contains("sh_info"));
    }
    let mut other_table = ordinary.clone();
    let (symtab, _) = locations(&other_table, ".symtab");
    let count = u16::from_le_bytes(other_table[60..62].try_into().unwrap());
    let next = u64at(&other_table, 40) as usize + usize::from(count) * 64;
    let header = other_table[symtab..symtab + 64].to_vec();
    other_table.resize(next, 0);
    other_table.extend_from_slice(&header);
    other_table[60..62].copy_from_slice(&(count + 1).to_le_bytes());
    let (rh, _) = locations(&other_table, ".rela.data");
    put32(&mut other_table, rh + 40, u32::from(count));
    assert!(link(&[other_table]).unwrap_err().contains("sh_link"));
    // Even ordinary-data relocation associations may not silently disappear.
    let mut bad = ordinary;
    let (rh, _) = locations(&bad, ".rela.data");
    put32(&mut bad, rh + 40, 0);
    assert!(link(&[bad]).unwrap_err().contains("sh_link"));
}

#[test]
fn selected_dso_lookups_cannot_lose_consumed_executable_providers() {
    let file = object::File::parse(REVIEW_DSO).unwrap();
    let prop = file
        .dynamic_symbols()
        .find(|s| s.name().unwrap() == "prop")
        .unwrap();
    assert!(prop.is_undefined() && prop.is_global() && !prop.is_weak());
    let (offset, r) = file.dynamic_relocations().unwrap().next().unwrap();
    assert_eq!(offset, 0x1000);
    assert_eq!(r.target(), object::RelocationTarget::Symbol(prop.index()));
    assert_eq!(r.flags(), object::RelocationFlags::Elf { r_type: 1 });
    let mut weak_dso = REVIEW_DSO.to_vec();
    let record = symbol_location(&weak_dso, ".dynsym", "prop");
    weak_dso[record + 4] = 0x21;
    let mut alternative = REVIEW_DSO.to_vec();
    let record = symbol_location(&alternative, ".dynsym", "prop");
    let file = object::File::parse(alternative.as_slice()).unwrap();
    let data_index = file.section_by_name(".data").unwrap().index().0 as u16;
    alternative[record + 6..record + 8].copy_from_slice(&data_index.to_le_bytes());
    put64(&mut alternative, record + 8, 0x1000);
    let (_, strings) = locations(&alternative, ".dynstr");
    alternative[strings + 6] = b'x'; // separate SONAME, equivalent external provider
    let start = obj(START);
    for shared in [REVIEW_DSO.to_vec(), weak_dso.clone()] {
        for binding in [".globl prop", ".weak prop"] {
            let provider = property_provider(binding);
            for pair in [
                vec![provider.clone(), shared.clone()],
                vec![shared.clone(), provider.clone()],
            ] {
                let mut inputs = vec![start.clone()];
                inputs.extend(pair);
                let err = link(&inputs).unwrap_err();
                assert!(
                    err.contains("libreview.so")
                        && err.contains("reference into consumed")
                        && err.contains("prop"),
                    "{err}"
                );
            }
            let err =
                link(&[start.clone(), provider, shared.clone(), alternative.clone()]).unwrap_err();
            assert!(err.contains("reference into consumed")); // executable precedence
        }
        let data = obj(".data\n.globl prop\n.type prop,@object\nprop: .quad 0\n");
        exports_prop(
            &link(&[start.clone(), data.clone(), shared.clone()]).unwrap(),
            false,
        );
        // Strong ordinary data overrides weak consumed definitions in either order.
        let weak_note = property_provider(".weak prop");
        for pair in [
            vec![weak_note.clone(), data.clone()],
            vec![data.clone(), weak_note],
        ] {
            let mut inputs = vec![start.clone(), shared.clone()];
            inputs.extend(pair);
            exports_prop(&link(&inputs).unwrap(), false);
        }
        let weak_data = obj(".data\n.weak prop\n.type prop,@object\nprop: .quad 0\n");
        exports_prop(
            &link(&[
                start.clone(),
                shared.clone(),
                weak_data.clone(),
                property_provider(".weak prop"),
            ])
            .unwrap(),
            true,
        );
        assert!(
            link(&[
                start.clone(),
                shared.clone(),
                property_provider(".weak prop"),
                weak_data
            ])
            .unwrap_err()
            .contains("reference into consumed")
        );
        // A same-named local is independent; a hidden global cannot supply DSO lookups.
        exports_prop(
            &link(&[
                start.clone(),
                shared.clone(),
                property_provider(""),
                data.clone(),
            ])
            .unwrap(),
            false,
        );
        let mut protected = property_provider(".globl prop");
        let record = symbol_location(&protected, ".symtab", "prop");
        protected[record + 5] = 3;
        assert!(
            link(&[start.clone(), shared.clone(), protected])
                .unwrap_err()
                .contains("reference into consumed")
        );
    }
    // Hidden/internal definitions are absent from dynsym by existing policy.
    for visibility in [1, 2] {
        let mut hidden = property_provider(".globl prop");
        let record = symbol_location(&hidden, ".symtab", "prop");
        hidden[record + 5] = visibility;
        assert!(
            link(&[
                start.clone(),
                REVIEW_DSO.to_vec(),
                alternative.clone(),
                hidden
            ])
            .is_ok()
        );
    }
    // Strong DSO requirements pull an archive member. Weak lookups never do.
    let ar = archive(&[property_provider(".globl prop")]);
    assert!(
        link(&[start.clone(), REVIEW_DSO.to_vec(), ar.clone()])
            .unwrap_err()
            .contains("reference into consumed")
    );
    assert!(link(&[start.clone(), weak_dso, ar]).is_ok());
    // Same-SONAME policy retains the first equivalent export interface only.
    let mut no_lookup = REVIEW_DSO.to_vec();
    let record = symbol_location(&no_lookup, ".dynsym", "prop");
    // Make a valid local definition and a symbol-free RELATIVE relocation,
    // so the first interface has the same exports but no dynamic lookup.
    no_lookup[record + 4] = 1;
    no_lookup[record + 6..record + 8].copy_from_slice(&data_index.to_le_bytes());
    put64(&mut no_lookup, record + 8, 0x1000);
    let (dynsym, _) = locations(&no_lookup, ".dynsym");
    put32(&mut no_lookup, dynsym + 44, 2); // both symbols precede the global boundary
    let (_, rela) = locations(&no_lookup, ".rela.dyn");
    put64(&mut no_lookup, rela + 8, 8); // R_X86_64_RELATIVE, symbol zero
    put64(&mut no_lookup, rela + 16, 0x1000);
    let provider = property_provider(".globl prop");
    assert!(
        link(&[
            start.clone(),
            provider.clone(),
            no_lookup.clone(),
            REVIEW_DSO.to_vec()
        ])
        .is_ok()
    );
    assert!(
        link(&[start, provider, REVIEW_DSO.to_vec(), no_lookup])
            .unwrap_err()
            .contains("reference into consumed")
    );
}

#[test]
fn compiler_defined_lookups_follow_executable_selection_without_archive_demand() {
    let start = obj(START);
    let strong = obj(".data\n.globl prop\n.type prop,@object\nprop: .quad 7\n");
    let weak = obj(".data\n.weak prop\n.type prop,@object\nprop: .quad 7\n");
    for shared in [DEFINED_DSO, WEAK_DEFINED_DSO] {
        let f = object::File::parse(shared).unwrap();
        let prop = f
            .dynamic_symbols()
            .find(|s| s.name().unwrap() == "prop")
            .unwrap();
        assert!(!prop.is_undefined());
        assert!(f.dynamic_relocations().unwrap().any(|(_, r)| {
            r.target() == object::RelocationTarget::Symbol(prop.index())
                && r.flags() == object::RelocationFlags::Elf { r_type: 1 }
        }));
        for binding in [".globl prop", ".weak prop"] {
            let note = property_provider(binding);
            for inputs in [
                vec![start.clone(), note.clone(), shared.to_vec()],
                vec![start.clone(), shared.to_vec(), note.clone()],
            ] {
                assert!(
                    link(&inputs)
                        .unwrap_err()
                        .contains("reference into consumed")
                );
            }
            // Merely having a defined lookup never extracts even a strong
            // property provider. Independent native demand still selects it.
            let ar = archive(std::slice::from_ref(&note));
            assert!(link(&[start.clone(), shared.to_vec(), ar.clone()]).is_ok());
            let demand = obj(".data\n.quad prop\n");
            assert!(
                link(&[start.clone(), demand, ar, shared.to_vec()])
                    .unwrap_err()
                    .contains("reference into consumed")
            );
            // Select the note member through a different symbol so only the
            // DSO relocation, not a native prop relocation, exercises the guard.
            let member = obj(&format!(
                ".text\n.globl pull\npull: ret\n{}",
                section(".note.gnu.property", &crate::note(&[(NEEDED, 8)])).replace(
                    ".byte",
                    &format!("{binding}\n.type prop,@object\nprop: .byte")
                )
            ));
            let demand = obj(".data\n.quad pull\n");
            let err =
                link(&[start.clone(), demand, archive(&[member]), shared.to_vec()]).unwrap_err();
            assert!(
                err.contains("reference into consumed") && err.contains("lib"),
                "{err}"
            );
        }
        for inputs in [
            vec![
                start.clone(),
                shared.to_vec(),
                strong.clone(),
                property_provider(".weak prop"),
            ],
            vec![
                start.clone(),
                property_provider(".weak prop"),
                shared.to_vec(),
                strong.clone(),
            ],
        ] {
            exports_prop(&link(&inputs).unwrap(), false);
        }
        exports_prop(
            &link(&[
                start.clone(),
                shared.to_vec(),
                weak.clone(),
                property_provider(".weak prop"),
            ])
            .unwrap(),
            true,
        );
        assert!(
            link(&[
                start.clone(),
                shared.to_vec(),
                property_provider(".weak prop"),
                weak.clone()
            ])
            .unwrap_err()
            .contains("reference into consumed")
        );
        // Only selected visible executable globals can preempt a DSO.
        assert!(link(&[start.clone(), shared.to_vec(), property_provider("")]).is_ok());
        for visibility in [1, 2, 3] {
            let mut provider = property_provider(".globl prop");
            let record = symbol_location(&provider, ".symtab", "prop");
            provider[record + 5] = visibility;
            let result = link(&[start.clone(), shared.to_vec(), provider]);
            if visibility == 3 {
                assert!(result.unwrap_err().contains("reference into consumed"));
            } else {
                assert!(result.is_ok());
            }
        }
        exports_prop(
            &link(&[start.clone(), shared.to_vec(), strong.clone()]).unwrap(),
            false,
        );
    }
    // Compiler emits RELATIVE for protected data: the DSO's slot binds locally.
    assert!(
        link(&[
            start,
            property_provider(".globl prop"),
            PROTECTED_DSO.to_vec()
        ])
        .is_ok()
    );
}

fn dynamic_entry(bytes: &[u8], tag: u32) -> usize {
    let f = object::File::parse(bytes).unwrap();
    let s = f.section_by_name(".dynamic").unwrap();
    let offset = s.file_range().unwrap().0 as usize;
    (0..s.size() as usize)
        .step_by(16)
        .map(|i| offset + i)
        .find(|&i| u64at(bytes, i) == u64::from(tag))
        .unwrap()
}

#[test]
fn compiler_got_and_plt_lookups_include_both_runtime_tables() {
    let start = obj(START);
    let data = obj(".data\n.globl prop\n.type prop,@object\nprop: .quad 7\n");
    for (shared, kind) in [
        (
            include_bytes!("fixtures/gnu-notes/libgot-defined.so").as_slice(),
            6,
        ),
        (
            include_bytes!("fixtures/gnu-notes/libplt-defined.so").as_slice(),
            7,
        ),
    ] {
        let f = object::File::parse(shared).unwrap();
        let prop = f
            .dynamic_symbols()
            .find(|s| s.name().unwrap() == "prop")
            .unwrap();
        assert!(f.dynamic_relocations().unwrap().any(|(_, r)| {
            r.target() == object::RelocationTarget::Symbol(prop.index())
                && r.flags() == object::RelocationFlags::Elf { r_type: kind }
        }));
        if kind == 7 {
            assert!(f.section_by_name(".rela.dyn").unwrap().size() > 0);
            assert!(f.section_by_name(".rela.plt").unwrap().size() > 0);
        }
        assert!(
            link(&[
                start.clone(),
                property_provider(".globl prop"),
                shared.to_vec()
            ])
            .unwrap_err()
            .contains("reference into consumed")
        );
        exports_prop(
            &link(&[start.clone(), data.clone(), shared.to_vec()]).unwrap(),
            false,
        );
    }
}

#[test]
fn runtime_tables_and_binding_decide_defined_lookups() {
    let start = obj(START);
    let note = property_provider(".globl prop");
    let rejects = |shared: Vec<u8>| {
        let e = link(&[start.clone(), note.clone(), shared]).unwrap_err();
        assert!(e.contains("reference into consumed"), "{e}");
    };
    let accepts = |shared: Vec<u8>| {
        link(&[start.clone(), note.clone(), shared]).unwrap();
    };
    // The small complete review DSO has spare nonessential HASH/SONAME tags,
    // and fixed file==virtual addresses, making table mutations independent
    // of the compiler's section names and placement.
    let mut defined = REVIEW_DSO.to_vec();
    let record = symbol_location(&defined, ".dynsym", "prop");
    let data_index = object::File::parse(defined.as_slice())
        .unwrap()
        .section_by_name(".data")
        .unwrap()
        .index()
        .0 as u16;
    defined[record + 6..record + 8].copy_from_slice(&data_index.to_le_bytes());
    put64(&mut defined, record + 8, 0x1000);
    rejects(defined.clone());
    let (rh, rp) = locations(&defined, ".rela.dyn");
    for kind in [1, 2, 6, 7, 16, 17, 18, 36] {
        let mut b = defined.clone();
        put32(&mut b, rp + 8, kind);
        rejects(b);
    }
    for kind in [0, 5, 8, 38] {
        // no named lookup, even with a nonzero r_sym
        let mut b = defined.clone();
        put32(&mut b, rp + 8, kind);
        accepts(b);
    }
    let mut irelative = defined.clone();
    put64(&mut irelative, rp + 8, 37);
    accepts(irelative);
    for visibility in [1, 2, 3] {
        let mut b = defined.clone();
        b[record + 5] = visibility;
        accepts(b);
    }
    let mut local = defined.clone();
    local[record + 4] = 1;
    accepts(local);
    for (tag, value) in [(object::elf::DT_SYMBOLIC, 0), (object::elf::DT_FLAGS, 2)] {
        let mut b = defined.clone();
        let at = dynamic_entry(&b, object::elf::DT_HASH);
        put64(&mut b, at, u64::from(tag));
        put64(&mut b, at + 8, value);
        accepts(b.clone());
        // Symbolic binding only affects definitions, not undefined demand.
        b[record + 6..record + 8].fill(0);
        put64(&mut b, record + 8, 0);
        rejects(b);
    }
    let mut rel = defined.clone();
    put32(&mut rel, rh + 4, object::elf::SHT_REL);
    put64(&mut rel, rh + 32, 16);
    put64(&mut rel, rh + 56, 16);
    for (old, new, value) in [(7, 17, rp as u64), (8, 18, 16), (9, 19, 16)] {
        let at = dynamic_entry(&rel, old);
        put64(&mut rel, at, new);
        put64(&mut rel, at + 8, value);
    }
    rejects(rel.clone());
    for (mut b, format) in [(defined.clone(), 7), (rel, 17)] {
        // Convert each runtime table to a PLT table, including REL-form PLT.
        let at = dynamic_entry(&b, format);
        put64(&mut b, at, 23); // DT_JMPREL
        let at = dynamic_entry(&b, if format == 7 { 8 } else { 18 });
        put64(&mut b, at, 2); // DT_PLTRELSZ
        let at = dynamic_entry(&b, object::elf::DT_HASH);
        put64(&mut b, at, 20); // DT_PLTREL
        put64(&mut b, at + 8, u64::from(format));
        put32(&mut b, rp + 8, 7); // JUMP_SLOT
        rejects(b);
    }
    // A default-visible export with no runtime lookup must not reject.
    let mut unused = defined.clone();
    let at = dynamic_entry(&unused, 8);
    put64(&mut unused, at + 8, 0);
    accepts(unused);
    for (at, value, message) in [
        (rh + 40, 0, "association"),
        (rh + 40, u32::MAX as u64, "association"),
        (rh + 56, 16, "association"),
        (rp + 12, u32::MAX as u64, "symbol index"),
        (rp + 8, (1u64 << 32) | 37, "symbol-free"),
        (dynamic_entry(&defined, 6) + 8, 0, "DYNSYM"),
        (dynamic_entry(&defined, 8) + 8, 23, "bounds"),
        (dynamic_entry(&defined, 7) + 8, u64::MAX, "bounds"),
    ] {
        let mut bad = defined.clone();
        if at == rh + 40 || at == rp + 12 {
            put32(&mut bad, at, value as u32);
        } else {
            put64(&mut bad, at, value);
        }
        let e = link(&[start.clone(), note.clone(), bad]).unwrap_err();
        assert!(e.contains(message), "{e}");
    }
    // Conflicting duplicate binding tags cannot turn a preemptible lookup
    // into a purported local binding merely because one copy says SYMBOLIC.
    let mut duplicate = defined;
    for (old, value) in [(object::elf::DT_HASH, 2), (object::elf::DT_SONAME, 0)] {
        let at = dynamic_entry(&duplicate, old);
        put64(&mut duplicate, at, u64::from(object::elf::DT_FLAGS));
        put64(&mut duplicate, at + 8, value);
    }
    assert!(
        link(&[start, note, duplicate])
            .unwrap_err()
            .contains("duplicate dynamic")
    );
}
