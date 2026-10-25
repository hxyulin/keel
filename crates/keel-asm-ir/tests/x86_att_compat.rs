//! Frozen AT&T behavior from 14bed00d; expected values are not generated.
use keel_asm_ir::x86::Instruction;

#[test]
fn generated_att_rules_keep_parsing_formatting_and_errors() {
    for line in include_str!("fixtures/x86-att-rules.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.splitn(3, '|').collect();
        let source = parts[0];
        match Instruction::parse_att(source) {
            Ok(i) => {
                assert_eq!(format!("{i:?}"), parts[1], "{source}");
                assert_eq!(i.to_string(), parts[2], "{source}");
            }
            Err(e) => {
                assert_eq!(parts[1], "ERR", "{source}: {e}");
                assert_eq!(e, parts[2], "{source}");
            }
        }
    }
}
