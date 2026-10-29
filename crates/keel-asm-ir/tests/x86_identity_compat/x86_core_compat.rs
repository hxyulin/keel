//! Frozen public parser contract from 3109c651, independent of generated data.
use keel_asm_ir::x86::{CoreOpcode, Opcode, isa};

#[test]
fn original_core_spellings_and_wrapper_precedence() {
    for line in include_str!("../fixtures/x86-core-identities.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split('|').collect();
        let name = parts[0];
        assert_eq!(format!("{:?}", CoreOpcode::parse(name)), parts[1], "{name}");
        assert_eq!(format!("{:?}", Opcode::parse(name)), parts[2], "{name}");
        assert_eq!(CoreOpcode::parse(&name.to_uppercase()), None);
        assert_eq!(CoreOpcode::parse(&format!("{name} ")), None);
    }
}

#[test]
fn condition_names_keep_public_and_source_dialects_separate() {
    let public = [
        "o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g",
    ];
    let source = [
        "o", "no", "b", "nb", "z", "nz", "be", "nbe", "s", "ns", "p", "np", "l", "nl", "le", "nle",
    ];
    for code in 0..=u8::MAX {
        assert_eq!(
            isa::condition_code_name(code),
            public.get(usize::from(code)).copied()
        );
        assert_eq!(
            isa::isa_condition_code_name(code),
            source.get(usize::from(code)).copied()
        );
    }
    for alias in ["", "bad", "E", "je", "nz "] {
        assert_eq!(isa::parse_condition_code(alias), None);
    }
}
