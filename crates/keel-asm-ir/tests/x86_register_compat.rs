//! Register contract captured from 5b04a1aa independently of the new schema.
use keel_asm_ir::x86::{Register, RegisterClass, isa};
use std::{collections::BTreeSet, fmt::Write};

const CLASSES: [(RegisterClass, u8, u8); 11] = [
    (RegisterClass::Gpr, 0, 0),
    (RegisterClass::Xmm, 1, 1),
    (RegisterClass::Mmx, 2, 4),
    (RegisterClass::Ymm, 3, 5),
    (RegisterClass::X87, 4, 2),
    (RegisterClass::Segment, 5, 6),
    (RegisterClass::Rip, 6, 3),
    (RegisterClass::Control, 7, 7),
    (RegisterClass::Debug, 8, 8),
    (RegisterClass::Zmm, 9, 9),
    (RegisterClass::Mask, 10, 10),
];
fn canonical() -> Vec<(Register, &'static str)> {
    include_str!("fixtures/x86-registers.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split('|').collect();
            let class = CLASSES
                .iter()
                .find(|(c, _, _)| format!("{c:?}") == fields[0])
                .unwrap()
                .0;
            (
                Register {
                    class,
                    number: fields[1].parse().unwrap(),
                    width: fields[2].parse().unwrap(),
                    high_byte: fields[3].parse().unwrap(),
                },
                fields[4],
            )
        })
        .collect()
}

#[test]
fn public_ids_tags_names_and_parse_roundtrips_are_stable() {
    for (class, id, tag) in CLASSES {
        assert_eq!(class as u8, id, "{class:?}");
        assert_eq!(class.symbolic_tag(), tag, "{class:?}");
        assert_eq!(RegisterClass::from_symbolic_tag(tag), Some(class));
    }
    for (register, name) in canonical() {
        assert_eq!(register.to_string(), name);
        assert_eq!(Register::parse(name), Some(register));
        assert_eq!(Register::parse(&name.to_uppercase()), Some(register));
        assert!(register.is_valid(), "{register:?}");
    }
}

#[test]
fn structural_validation_matches_the_original_canonical_domain() {
    let valid: BTreeSet<_> = canonical()
        .into_iter()
        .map(|(r, _)| (r.class as u8, r.number, r.width, r.high_byte))
        .collect();
    assert_eq!(valid.len(), 137);
    for (class, _, _) in CLASSES {
        for number in 0..=255 {
            for width in (0..=512).chain([65535]) {
                for high_byte in [false, true] {
                    let r = Register {
                        class,
                        number,
                        width,
                        high_byte,
                    };
                    assert_eq!(
                        r.is_valid(),
                        valid.contains(&(class as u8, number, width, high_byte))
                            || (16..32).contains(&number)
                                && !high_byte
                                && match class {
                                    RegisterClass::Xmm => width == 128,
                                    RegisterClass::Ymm => width == 256,
                                    _ => false,
                                }
                            || !high_byte
                                && match class {
                                    RegisterClass::Zmm => number < 32 && width == 512,
                                    RegisterClass::Mask => number < 8 && width == 64,
                                    _ => false,
                                },
                        "{r:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn numeric_spelling_and_formatter_compatibility_remain_separate_from_validity() {
    for (source, canonical) in [
        ("xmm+01", "xmm1"),
        ("xmm0015", "xmm15"),
        ("ymm+000", "ymm0"),
        ("r+008b", "r8b"),
        ("r00015d", "r15d"),
        ("ST", "st(0)"),
        ("CR8", "cr8"),
        ("DR4", "dr4"),
    ] {
        assert_eq!(
            Register::parse(source),
            Register::parse(canonical),
            "{source}"
        );
        assert!(Register::parse(source).is_some());
    }
    for source in [
        "cr00", "cr+0", "CR08", "dr04", "dr+4", "mm00", "st(00)", "st(+0)", "r7", "r16", "xmm32",
        "ymm32", "xmm-0", " rax", "rax ", "cr1", "dr8",
    ] {
        assert_eq!(Register::parse(source), None, "{source}");
    }
    for (class, number, width, high_byte, expected) in [
        (RegisterClass::Gpr, 4, 64, true, Some("ah")),
        (RegisterClass::Segment, 3, 64, true, Some("ds")),
        (RegisterClass::Rip, 255, 0, true, Some("rip")),
        (RegisterClass::Xmm, 255, 0, true, Some("xmm255")),
        (RegisterClass::Ymm, 2, 8, true, Some("ymm2")),
        (RegisterClass::Mmx, 8, 64, false, None),
        (RegisterClass::X87, 2, 16, false, None),
        (RegisterClass::Control, 1, 64, false, None),
        (RegisterClass::Debug, 1, 32, false, None),
        (RegisterClass::Gpr, 0, 64, true, None),
        (RegisterClass::Segment, 6, 16, false, None),
    ] {
        let r = Register {
            class,
            number,
            width,
            high_byte,
        };
        assert!(!r.is_valid());
        let mut text = String::new();
        let result = write!(text, "{r}");
        assert_eq!(result.is_ok(), expected.is_some(), "{r:?}");
        assert_eq!(text, expected.unwrap_or(""), "{r:?}");
    }
}

#[test]
fn decoded_register_fields_keep_their_existing_recognition_limits() {
    assert_eq!(
        RegisterClass::from_isa_kind("gpr32_32_64"),
        Some(RegisterClass::Gpr)
    );
    assert_eq!(
        RegisterClass::from_isa_kind("fpr"),
        Some(RegisterClass::X87)
    );
    assert_eq!(
        RegisterClass::from_isa_kind("cr"),
        Some(RegisterClass::Control)
    );
    assert_eq!(
        RegisterClass::from_isa_kind("mask"),
        Some(RegisterClass::Mask)
    );
    // Decoded fields can describe reserved encodings that the typed assembler rejects.
    assert_eq!(isa::register_name("cr", 1).as_deref(), Some("cr1"));
    assert_eq!(Register::parse("cr1"), None);
    assert_eq!(isa::register_name("dr", 8).as_deref(), Some("dr8"));
    assert_eq!(isa::register_name("sreg", 6), None);
    assert_eq!(isa::register_name("mmx", 8), None);
    assert_eq!(isa::register_name("fpr", 8), None);
    assert_eq!(isa::register_name("mask", 7).as_deref(), Some("k7"));
    assert_eq!(isa::register_name("unknown", 0), None);
}
