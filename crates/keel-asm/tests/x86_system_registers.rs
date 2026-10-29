//! Encoding only. Control/debug instructions never execute on the host.
#[path = "x86_system_registers/x86_descriptor_loads.rs"]
mod x86_descriptor_loads;
#[path = "x86_system_registers/x86_selector_queries.rs"]
mod x86_selector_queries;
#[path = "x86_system_registers/x86_system_write.rs"]
mod x86_system_write;
use keel_asm_ir::x86::{Features, Instruction, Operand, Register, RegisterClass, Syntax};
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
fn rows() -> Vec<(String, String, Vec<u8>)> {
    let mut rows = Vec::new();
    for (prefix, numbers, read) in [
        ("cr", &[0, 2, 3, 4, 8][..], 0x20),
        ("dr", &[0, 1, 2, 3, 4, 5, 6, 7][..], 0x21),
    ] {
        for &number in numbers {
            for gpr in 0..16 {
                let name = Register {
                    class: RegisterClass::Gpr,
                    number: gpr,
                    width: 64,
                    high_byte: false,
                }
                .to_string();
                for write in [false, true] {
                    let mut bytes = Vec::new();
                    let rex = 0x40 | ((number >> 3) << 2) | (gpr >> 3);
                    if rex != 0x40 {
                        bytes.push(rex);
                    }
                    bytes.extend([
                        0x0f,
                        read + if write { 2 } else { 0 },
                        0xc0 | ((number & 7) << 3) | (gpr & 7),
                    ]);
                    let system = format!("{prefix}{number}");
                    let (dst, src) = if write {
                        (&system, &name)
                    } else {
                        (&name, &system)
                    };
                    rows.push((
                        format!("mov {dst},{src}"),
                        format!("mov %{src},%{dst}"),
                        bytes,
                    ));
                }
            }
        }
    }
    rows
}
#[test]
fn system_register_literals_roundtrip_and_reject_invalid_operands() {
    assert_eq!(
        RegisterClass::Rip as u8,
        6,
        "preserve existing public register class identities"
    );
    for (intel, att, expected) in rows() {
        let parsed = Instruction::parse_intel(&intel).unwrap();
        assert_eq!(
            parsed,
            Instruction::parse_intel(&parsed.to_string()).unwrap()
        );
        assert_eq!(parsed, Instruction::parse_att(&att).unwrap(), "{att}");
        for (source, syntax) in [(&intel, Syntax::Intel), (&att, Syntax::Att)] {
            let object = keel_asm::x86::assemble_object_with_syntax_and_features(
                source,
                syntax,
                Features::NONE,
            )
            .unwrap();
            assert_eq!(text(&object), expected, "{source}");
        }
    }
    for (att, intel) in [
        ("movq %cr8,%r15", "mov r15,cr8"),
        ("movq %r8,%dr7", "mov dr7,r8"),
    ] {
        assert_eq!(
            Instruction::parse_att(att).unwrap(),
            Instruction::parse_intel(intel).unwrap()
        );
    }
    for att in ["movl %cr0,%eax", "movw %ax,%dr0", "movb %dr0,%al"] {
        assert!(Instruction::parse_att(att).is_err(), "{att}");
    }
    for source in [
        "mov eax,cr0",
        "mov cr0,eax",
        "mov ax,dr0",
        "mov dr0,al",
        "mov [rax],cr0",
        "mov cr0,[rax]",
        "mov xmm0,dr0",
        "mov dr0,cr0",
        "mov cr1,rax",
        "mov cr9,rax",
        "mov dr8,rax",
        "lock mov cr0,rax",
        "mov rax,[cr0]",
        "mov rax,[rax+dr0*2]",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, Features::NONE).is_err(),
            "{source}"
        );
    }
    for (class, number, width, high_byte) in [
        (RegisterClass::Control, 1, 64, false),
        (RegisterClass::Control, 8, 32, false),
        (RegisterClass::Debug, 8, 64, false),
        (RegisterClass::Debug, 0, 64, true),
    ] {
        let mut i = Instruction::parse_intel("mov rax,cr0").unwrap();
        i.operands[1] = Operand::Register(Register {
            class,
            number,
            width,
            high_byte,
        });
        let mut program = keel_asm_ir::Program::new();
        program.x86_instruction(i);
        assert!(keel_asm::x86::assemble_program_object(&program).is_err());
    }
}
#[test]
fn system_register_matrix_matches_clang_and_gnu() {
    let rows = rows();
    let expected = rows.iter().flat_map(|r| r.2.clone()).collect::<Vec<_>>();
    let dir = std::env::temp_dir().join(format!("keel-system-registers-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for syntax in [Syntax::Intel, Syntax::Att] {
        let source = rows
            .iter()
            .map(|r| {
                if syntax == Syntax::Intel {
                    r.0.as_str()
                } else {
                    r.1.as_str()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let source = if syntax == Syntax::Att {
            source.replace("mov ", "movq ")
        } else {
            source
        };
        let own = keel_asm::x86::assemble_object_with_syntax_and_features(
            &source,
            syntax,
            Features::NONE,
        )
        .unwrap();
        assert_eq!(text(&own), expected);
        let asm = dir.join("matrix.s");
        let obj = dir.join("matrix.o");
        fs::write(
            &asm,
            format!(
                "{}\n.text\n{source}\n",
                if syntax == Syntax::Intel {
                    ".intel_syntax noprefix"
                } else {
                    ".att_syntax prefix"
                }
            ),
        )
        .unwrap();
        let output = Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(&asm)
            .arg("-o")
            .arg(&obj)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(text(&fs::read(&obj).unwrap()), expected);
        if let Some(assembler) = std::env::var_os("KEEL_GNU_AS") {
            let output = Command::new(assembler)
                .args(["--64", "-o"])
                .arg(&obj)
                .arg(&asm)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(text(&fs::read(&obj).unwrap()), expected);
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
