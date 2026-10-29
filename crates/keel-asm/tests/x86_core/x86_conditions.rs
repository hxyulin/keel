//! Literal opcode families check all aliases through the generated-name bridge.
use keel_asm_ir::x86::Features;
use object::{Object, ObjectSection};

#[test]
fn all_condition_aliases_keep_their_numeric_encoding() {
    let aliases = [
        "o", "no", "b c nae", "ae nb nc", "e z", "ne nz", "be na", "a nbe", "s", "ns", "p pe",
        "np po", "l nge", "ge nl", "le ng", "g nle",
    ];
    for (cc, names) in aliases.iter().enumerate() {
        for name in names.split_whitespace() {
            for (source, expected) in [
                (
                    format!("j{name} target\nnop\ntarget: ret"),
                    vec![0x70 + cc as u8, 1, 0x90, 0xc3],
                ),
                (
                    format!("cmov{name} r8, r9"),
                    vec![0x4d, 0x0f, 0x40 + cc as u8, 0xc1],
                ),
                (
                    format!("set{name} r9b"),
                    vec![0x41, 0x0f, 0x90 + cc as u8, 0xc1],
                ),
            ] {
                let bytes =
                    keel_asm::x86::assemble_object_with_features(&source, Features::NONE).unwrap();
                let object = object::File::parse(bytes.as_slice()).unwrap();
                let text = object.section_by_name(".text").unwrap();
                assert_eq!(text.data().unwrap(), expected, "{source}");
            }
        }
    }
}
