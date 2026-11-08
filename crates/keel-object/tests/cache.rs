use keel_object::{Atom, Cache, Definition, Fixup, FixupKind, Object, Section, Symbol, Target};

fn fixture() -> Object {
    Object {
        minimum: 0x000b_0201,
        subsections: true,
        sections: vec![
            Section {
                segment: "__TEXT".into(),
                name: "__text".into(),
                size: 4,
                alignment: 2,
                flags: 0x80000400,
                data: vec![0xc0, 0x03, 0x5f, 0xd6],
                fixups: vec![Fixup {
                    offset: 0,
                    target: Target::Symbol(0),
                    addend: 0,
                    kind: FixupKind::Branch26,
                }],
            },
            Section {
                segment: "__DATA".into(),
                name: "__bss".into(),
                size: 1 << 20,
                alignment: 4,
                flags: 1,
                data: Vec::new(),
                fixups: Vec::new(),
            },
        ],
        unwind: Vec::new(),
        atoms: vec![Atom {
            section: 0,
            offset: 0,
            boundary: true,
            no_dead_strip: false,
        }],
        symbols: vec![Symbol {
            name: "_main".into(),
            definition: Definition::Atom(0),
            private: false,
            weak_definition: false,
            weak_reference: false,
            weak_can_be_hidden: false,
            no_dead_strip: false,
        }],
    }
}

#[test]
fn flat_cache_borrows_unaligned_bytes_and_preserves_zero_fill() {
    let object = fixture();
    let bytes = object.encode().unwrap();
    assert!(
        bytes.len() < 1024,
        "zero fill must not occupy payload bytes"
    );
    assert_eq!(&bytes[..16], b"KEELOBJ\0\x03\0\0\0\x01\0\0\0");
    let mut unaligned = vec![0xff];
    unaligned.extend_from_slice(&bytes);
    let input = &unaligned[1..];
    let cache = Cache::parse(input).unwrap();
    assert_eq!(cache.minimum(), 0x000b_0201);
    assert!(cache.subsections());
    let section = cache.sections().next().unwrap();
    let start = section.data.as_ptr() as usize - input.as_ptr() as usize;
    assert!(start + section.data.len() <= input.len());
    assert_eq!(section.data, object.sections[0].data);
    let name = cache.symbols().next().unwrap().name;
    let start = name.as_ptr() as usize - input.as_ptr() as usize;
    assert!(start + name.len() <= input.len());
    assert_eq!(cache.to_object(), object);
    assert_eq!(Object::decode(&bytes).unwrap().encode().unwrap(), bytes);
}

#[test]
fn malformed_ranges_versions_tags_and_names_are_rejected() {
    let bytes = fixture().encode().unwrap();
    for end in 0..bytes.len() {
        assert!(Cache::parse(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Cache::parse(&trailing).is_err());
    // Version, target, region offset, record count and reserved header flags.
    for offset in [8, 12, 16, 24, 132] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].fill(0xff);
        assert!(Cache::parse(&bad).is_err(), "header offset {offset}");
    }
    let cache = Cache::parse(&bytes).unwrap();
    let name = cache.symbols().next().unwrap().name;
    let offset = name.as_ptr() as usize - bytes.as_ptr() as usize;
    let mut bad = bytes.clone();
    bad[offset] = 0xff;
    assert!(Cache::parse(&bad).is_err());
    let mut bad = fixture();
    bad.sections[0].fixups[0].target = Target::Symbol(50);
    assert!(Cache::parse(&bad.encode().unwrap()).is_err());
    let mut bad = fixture();
    bad.sections[0].fixups[0].offset = u32::MAX;
    assert!(Cache::parse(&bad.encode().unwrap()).is_err());
    let mut bad = fixture();
    bad.sections[0].size = 3;
    assert!(Cache::parse(&bad.encode().unwrap()).is_err());
    let mut bad = fixture();
    bad.atoms[0].section = u32::MAX;
    assert!(Cache::parse(&bad.encode().unwrap()).is_err());
}

#[test]
fn semantic_fixups_require_canonical_nonoverlapping_sites() {
    let mut object = fixture();
    let duplicate = object.sections[0].fixups[0];
    object.sections[0].fixups.push(duplicate);
    assert!(Cache::parse(&object.encode().unwrap()).is_err());
    let mut object = fixture();
    object.sections[0].fixups[0].target = Target::Atom(u32::MAX);
    assert!(object.encode().is_err());
}
