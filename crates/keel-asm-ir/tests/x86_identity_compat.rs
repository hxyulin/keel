//! Public dialect contract pinned from db6129d3, independent of generation.
use keel_asm_ir::x86::{FixedOpcode, Instruction, Opcode, V2Opcode, V3Opcode, X87Opcode};
use std::fmt::Write;

#[test]
fn original_static_spellings_identities_and_formatting() {
    for line in include_str!("fixtures/x86-static-identities.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('|').collect();
        let op = Opcode::parse(fields[0]).unwrap();
        assert_eq!(format!("{op:?}"), fields[1], "{}", fields[0]);
        assert_eq!(op.to_string(), fields[2], "{}", fields[0]);
        assert_eq!(Opcode::parse(&fields[0].to_ascii_uppercase()), None);
    }
}

#[test]
fn wrapper_precedence_and_x87_all_order() {
    // These names overlap the native ISA catalogue and must keep their existing
    // public identity rather than being reassigned to another wrapper.
    for (name, op) in [
        ("movsd", Opcode::Movsd),
        ("pextrw", Opcode::Pextrw),
        ("fadd", Opcode::Fadd),
        ("fst", Opcode::X87(X87Opcode::Fst)),
        ("wait", Opcode::Fixed(FixedOpcode::Fwait)),
        ("pclmulqdq", Opcode::Pclmulqdq),
        ("fisttp", Opcode::V2(V2Opcode::Fisttp)),
        ("vaddps", Opcode::Vaddps),
    ] {
        assert_eq!(Opcode::parse(name), Some(op), "{name}");
    }
    for op in V2Opcode::ALL {
        let name = op.mnemonic();
        if name != "pextrw" {
            assert_eq!(Opcode::parse(name), Some(Opcode::V2(*op)), "{name}");
        }
    }
    for op in V3Opcode::ALL {
        assert_eq!(Opcode::parse(op.mnemonic()), Some(Opcode::V3(*op)));
    }
    let expected: Vec<_> = include_str!("fixtures/x86-static-identities.txt")
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('|').collect();
            (fields.len() == 3 && fields[1].starts_with("X87(")).then(|| fields[0])
        })
        .collect();
    assert_eq!(
        X87Opcode::ALL
            .iter()
            .map(|op| op.mnemonic())
            .collect::<Vec<_>>(),
        expected
    );
    for op in X87Opcode::ALL {
        assert_eq!(X87Opcode::parse(op.mnemonic()), Some(*op));
        assert_eq!(X87Opcode::parse(&op.mnemonic().to_uppercase()), None);
    }
}

#[test]
fn condition_aliases_and_invalid_names_keep_their_contract() {
    let aliases = [
        "o", "no", "b c nae", "ae nb nc", "e z", "ne nz", "be na", "a nbe", "s", "ns", "p pe",
        "np po", "l nge", "ge nl", "le ng", "g nle",
    ];
    for (cc, names) in aliases.iter().enumerate() {
        let canonical = names.split_whitespace().next().unwrap();
        for alias in names.split_whitespace() {
            for (prefix, op) in [
                ("j", Opcode::Jcc(cc as u8)),
                ("set", Opcode::Setcc(cc as u8)),
                ("cmov", Opcode::Cmovcc(cc as u8)),
            ] {
                assert_eq!(Opcode::parse(&format!("{prefix}{alias}")), Some(op));
                assert_eq!(op.to_string(), format!("{prefix}{canonical}"));
            }
        }
    }
    for name in [
        "", "MOV", " mov", "mov ", "j", "set", "cmov", "jbad", "funknown", "movq.6E",
    ] {
        assert_eq!(Opcode::parse(name), None, "{name}");
    }
    for op in [Opcode::Jcc(16), Opcode::Setcc(255), Opcode::Cmovcc(16)] {
        assert!(write!(String::new(), "{op}").is_err());
    }
    assert_eq!(
        Instruction::parse_intel("jbad somewhere").unwrap_err(),
        "unsupported x86 instruction `jbad`"
    );
}
