use keel_asm_ir::x86::{FixedOpcode, Instruction, Opcode};
use object::{Object, ObjectSection};
use std::process::Command;

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
fn baseline_fixed_forms_match_clang_gnu_and_typed_transport() {
    let mut source = String::from(".intel_syntax noprefix\n.text\n");
    let mut program = keel_asm_ir::Program::new();
    let mut chunks = keel_asm::x86::ProgramAssembler::new(Default::default());
    let mut expected = Vec::new();
    for &op in FixedOpcode::ALL {
        // Optional MONITOR forms have their own gated byte/decoder tests.
        if matches!(op, FixedOpcode::Monitor | FixedOpcode::Mwait) {
            continue;
        }
        source.push_str(op.mnemonic());
        source.push('\n');
        expected.extend_from_slice(op.bytes());
        let parsed = Instruction::parse_intel(op.mnemonic()).unwrap();
        assert_eq!(parsed.opcode, Opcode::Fixed(op));
        assert_eq!(parsed.to_string(), op.mnemonic());
        let att = Instruction::parse_att(op.mnemonic()).unwrap();
        assert_eq!(parsed, att);
        program.x86_instruction(parsed.clone());
        let mut chunk = keel_asm_ir::Program::new();
        chunk.x86_instruction(parsed);
        chunks.push(chunk);
        for bad in [
            format!("{} rax", op.mnemonic()),
            format!("lock {}", op.mnemonic()),
        ] {
            assert!(keel_asm::x86::assemble_object(&bad).is_err(), "{bad}");
        }
    }
    assert_eq!(
        Instruction::parse_intel("wait").unwrap().opcode,
        Opcode::Fixed(FixedOpcode::Fwait)
    );
    let own = keel_asm::x86::assemble_object(&source).unwrap();
    let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(text(&own), expected);
    assert_eq!(own, typed);
    assert_eq!(own, chunks.finish().unwrap());
    let dir = std::env::temp_dir().join(format!("keel-fixed-baseline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("forms.s");
    // LLVM Intel lacks these historical mnemonic spellings. Width prefixes
    // select the same architectural operations; UDB uses pinned XED's D6 byte
    // because neither assembler provides its mnemonic.
    std::fs::write(
        &input,
        source
            .replace("\nleavew\n", "\ndata16 leave\n")
            .replace("\niretw\n", "\ndata16 iretd\n")
            .replace("\nudb\n", "\n.byte 0xd6\n"),
    )
    .unwrap();
    let clang = dir.join("clang.o");
    let p = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "-x",
            "assembler",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&clang)
        .output()
        .unwrap();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert_eq!(text(&std::fs::read(clang).unwrap()), expected);
    let assembler = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "as".into()
        } else {
            "x86_64-elf-as".into()
        }
    });
    // LLVM Intel syntax uses bare SYSRET for the 32-bit form. GNU Intel syntax
    // requires SYSRETD; its bare spelling is ambiguous. Both select 0F 07.
    let gnu_input = dir.join("gnu.s");
    std::fs::write(
        &gnu_input,
        std::fs::read_to_string(&input)
            .unwrap()
            .replace("\nsysret\n", "\nsysretd\n")
            .replace("\nsysexit\n", "\nsysexitd\n"),
    )
    .unwrap();
    let gnu = dir.join("gnu.o");
    let p = Command::new(assembler)
        .arg("--64")
        .arg(&gnu_input)
        .arg("-o")
        .arg(&gnu)
        .output()
        .expect("GNU assembler is required");
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert_eq!(text(&std::fs::read(gnu).unwrap()), expected);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn system_exit_widths_aliases_and_att_oracles() {
    for (spellings, expected) in [
        (&["sysexit", "sysexitd", "sysexitl"][..], &[0x0f, 0x35][..]),
        (&["sysexitq"][..], &[0x48, 0x0f, 0x35][..]),
        (&["rsm"][..], &[0x0f, 0xaa][..]),
    ] {
        for name in spellings {
            for source in [name.to_string(), format!(".att_syntax prefix\n{name}")] {
                assert_eq!(
                    text(&keel_asm::x86::assemble_object(&source).unwrap()),
                    expected
                );
            }
            for bad in [
                format!("{name} rax"),
                format!("{name} [rax]"),
                format!("lock {name}"),
            ] {
                assert!(keel_asm::x86::assemble_object(&bad).is_err(), "{bad}");
            }
        }
    }
    // Explicit AT&T sizes work in both external assemblers. Intel GNU's
    // 32-bit spelling differs from LLVM's, as covered by the full matrix.
    let dir = std::env::temp_dir().join(format!("keel-system-exits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("forms.s");
    std::fs::write(
        &input,
        ".att_syntax prefix\n.text\nsysexitl\nsysexitq\nrsm\n",
    )
    .unwrap();
    let expected = [0x0f, 0x35, 0x48, 0x0f, 0x35, 0x0f, 0xaa];
    let assembler = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "as".into()
        } else {
            "x86_64-elf-as".into()
        }
    });
    for (name, mut command) in [
        ("clang", {
            let mut c = Command::new("clang");
            c.args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-c",
                "-x",
                "assembler",
            ]);
            c
        }),
        ("gnu", {
            let mut c = Command::new(assembler);
            c.arg("--64");
            c
        }),
    ] {
        let output = dir.join(format!("{name}.o"));
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(text(&std::fs::read(output).unwrap()), expected, "{name}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
