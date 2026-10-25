//! Privileged encodings only. Never execute these instructions on the host.
use keel_asm_ir::x86::{Features, Instruction, Syntax};
use object::{Object, ObjectSection};
use std::{fs, process::Command};

fn text(object: &[u8]) -> Vec<u8> {
    object::File::parse(object)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn long_mode_table_loads_have_exact_memory_encodings() {
    for (intel, att, expected) in [
        (
            "lgdt tbyte ptr [rax]",
            "lgdt (%rax)",
            vec![0x0f, 0x01, 0x10],
        ),
        (
            "lidt tbyte ptr [rax]",
            "lidt (%rax)",
            vec![0x0f, 0x01, 0x18],
        ),
        (
            "lgdt [r13+8]",
            "lgdt 8(%r13)",
            vec![0x41, 0x0f, 0x01, 0x55, 8],
        ),
        (
            "lidt [r12+r9*4+16]",
            "lidt 16(%r12,%r9,4)",
            vec![0x43, 0x0f, 0x01, 0x5c, 0x8c, 16],
        ),
        ("lgdt [eax]", "lgdt (%eax)", vec![0x67, 0x0f, 0x01, 0x10]),
    ] {
        let parsed = Instruction::parse_intel(intel).unwrap();
        assert_eq!(
            parsed,
            Instruction::parse_intel(&parsed.to_string()).unwrap()
        );
        for (syntax, source) in [(Syntax::Intel, intel), (Syntax::Att, att)] {
            let object = keel_asm::x86::assemble_object_with_syntax_and_features(
                source,
                syntax,
                Features::NONE,
            )
            .unwrap();
            assert_eq!(text(&object), expected, "{source}");
        }
    }
    for op in ["lgdt", "lidt"] {
        for operand in [
            "rax",
            "ax",
            "0",
            "word ptr [rax]",
            "dword ptr [rax]",
            "qword ptr [rax]",
            "xmmword ptr [rax]",
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
}

#[test]
fn table_loads_match_external_assemblers() {
    let source = ".intel_syntax noprefix\n.text\nlgdt [rax]\nlidt [r13+8]\nlgdt [r12+r9*4+16]\nlidt [eax]\nlgdt [rip+table]\nlidt [rip+table]\ntable: .zero 10\n";
    let own = keel_asm::x86::assemble_object_with_features(source, Features::NONE).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-descriptor-loads-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let mut cmd = if gnu {
            let mut cmd =
                Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            cmd.arg("--64");
            cmd
        } else {
            let mut cmd = Command::new("clang");
            cmd.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            cmd
        };
        let output = dir.join("test.o");
        let result = cmd.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(text(&own), text(&fs::read(output).unwrap()));
    }
    fs::remove_dir_all(dir).unwrap();
}
