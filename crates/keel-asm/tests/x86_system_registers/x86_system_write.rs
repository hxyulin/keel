//! Independent byte and external-assembler witnesses. Never execute on the host.
use keel_asm_ir::x86::{Features, Instruction, Syntax};
use object::{Object, ObjectSection};
use std::{fs, process::Command};
fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn canonical_system_write_encodings_and_operand_domains() {
    for (op, map, group) in [
        ("lldt", 0, 2),
        ("ltr", 0, 3),
        ("lmsw", 1, 6),
        ("invlpg", 1, 7),
    ] {
        let mut rows = vec![
            (
                format!("{op} [rax]"),
                format!("{op} (%rax)"),
                vec![0x0f, map, group << 3],
            ),
            (
                format!("{op} [r13+8]"),
                format!("{op} 8(%r13)"),
                vec![0x41, 0x0f, map, 0x45 | (group << 3), 8],
            ),
            (
                format!("{op} [eax]"),
                format!("{op} (%eax)"),
                vec![0x67, 0x0f, map, group << 3],
            ),
        ];
        if op != "invlpg" {
            rows.push((
                format!("{op} ax"),
                format!("{op} %ax"),
                vec![0x0f, map, 0xc0 | (group << 3)],
            ));
            rows.push((
                format!("{op} r9w"),
                format!("{op} %r9w"),
                vec![0x41, 0x0f, map, 0xc1 | (group << 3)],
            ));
        }
        for (intel, att, bytes) in rows {
            let parsed = Instruction::parse_intel(&intel).unwrap();
            assert_eq!(
                parsed,
                Instruction::parse_intel(&parsed.to_string()).unwrap()
            );
            for (source, syntax) in [(intel, Syntax::Intel), (att, Syntax::Att)] {
                assert_eq!(
                    text(
                        &keel_asm::x86::assemble_object_with_syntax_and_features(
                            &source,
                            syntax,
                            Features::NONE
                        )
                        .unwrap()
                    ),
                    bytes,
                    "{source}"
                );
            }
        }
        for operand in [
            "eax",
            "rax",
            "xmm0",
            "1",
            "dword ptr [rax]",
            "qword ptr [rax]",
        ] {
            assert!(
                keel_asm::x86::assemble_object_with_features(
                    &format!("{op} {operand}"),
                    Features::NONE
                )
                .is_err(),
                "{op} {operand}"
            );
        }
    }
    assert!(keel_asm::x86::assemble_object_with_features("invlpg ax", Features::NONE).is_err());
}
#[test]
fn generated_system_forms_match_clang_and_gnu() {
    let dir = std::env::temp_dir().join(format!("keel-system-write-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (syntax, header, rows) in [
        (
            Syntax::Intel,
            ".intel_syntax noprefix\n",
            "lldt ax\nltr r9w\nlmsw word ptr [r13+8]\ninvlpg [r12+r9*4+16]\nlldt [eax]\nltr [rip+value]\nlmsw cx\ninvlpg [rip+value]\n",
        ),
        (
            Syntax::Att,
            "",
            "lldt %ax\nltr %r9w\nlmsw 8(%r13)\ninvlpg 16(%r12,%r9,4)\nlldt (%eax)\nltr value(%rip)\nlmsw %cx\ninvlpg value(%rip)\n",
        ),
    ] {
        let source = format!("{header}.text\n{rows}value: .word 0\n");
        let own = keel_asm::x86::assemble_object_with_syntax_and_features(
            &source,
            syntax,
            Features::NONE,
        )
        .unwrap();
        let input = dir.join("test.s");
        fs::write(&input, source).unwrap();
        for gnu in [false, true] {
            let mut c = if gnu {
                let mut c =
                    Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
                c.arg("--64");
                c
            } else {
                let mut c = Command::new("clang");
                c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
                c
            };
            let output = dir.join("test.o");
            let r = c.arg(&input).arg("-o").arg(&output).output().unwrap();
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
            assert_eq!(text(&own), text(&fs::read(output).unwrap()));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
