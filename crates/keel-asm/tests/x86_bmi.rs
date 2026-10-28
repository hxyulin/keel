//! Intel SDM Vol. 2 instruction tables are the encoding authority (docs/targets/x86_64/bmi.md).
use keel_asm::x86::{
    assemble_object, assemble_object_with_features, assemble_program_object_with_features,
};
use keel_asm_ir::{
    Program,
    x86::{Features, Instruction},
};
use object::{Object, ObjectSection};
use std::process::Command;

const BMI: Features = Features {
    avx: false,
    bmi1: true,
    bmi2: true,
    ..Features {
        popcnt: true,
        sse41: true,
        ..Features::NONE
    }
};
fn text(object: &[u8]) -> Vec<u8> {
    object::File::parse(object)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn gpr(n: u8, bits: u16) -> String {
    keel_asm_ir::x86::Register {
        class: keel_asm_ir::x86::RegisterClass::Gpr,
        number: n,
        width: bits,
        high_byte: false,
    }
    .to_string()
}
fn matrix() -> Vec<String> {
    let mut cases = Vec::new();
    for w in [32, 64] {
        let size = if w == 32 { "dword" } else { "qword" };
        let sources = [
            gpr(2, w),
            gpr(11, w),
            gpr(15, w),
            format!("{size} ptr [r12+8*r13-128]"),
            format!("{size} ptr [r13]"),
            format!("{size} ptr [rsp+128]"),
            format!("{size} ptr [8*r14+4096]"),
            format!("{size} ptr fs:[rax+16]"),
            format!("{size} ptr [rip+32]"),
            "[r15+127]".into(),
        ];
        for d in [0, 9, 15] {
            let dst = gpr(d, w);
            for src in &sources {
                for op in ["blsi", "blsmsk", "blsr", "tzcnt"] {
                    cases.push(format!("{op} {dst},{src}"));
                }
                for count in [0, 1, 31, 32, 63, 64, 255] {
                    cases.push(format!("rorx {dst},{src},{count}"));
                }
                for v in [0, 10, 15] {
                    let v = gpr(v, w);
                    for op in ["andn", "pext", "pdep", "mulx"] {
                        cases.push(format!("{op} {dst},{v},{src}"));
                    }
                    for op in ["bextr", "bzhi", "shlx", "shrx", "sarx"] {
                        cases.push(format!("{op} {dst},{src},{v}"));
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn bmi_clang_gnu_and_typed_differential_matrix() {
    let cases = matrix();
    let mut program = Program::new();
    for case in &cases {
        let i = Instruction::parse_intel(case).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{case}"
        );
        program.x86_instruction(i);
    }
    assert_eq!(program.stats().text_bytes, 0);
    let source = format!(".intel_syntax noprefix\n.text\n{}\n", cases.join("\n"));
    let ours = assemble_object_with_features(&source, BMI).unwrap();
    assert_eq!(
        ours,
        assemble_program_object_with_features(&program, BMI).unwrap()
    );
    let temp = std::env::temp_dir().join(format!("keel-bmi-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    std::fs::write(&input, source).unwrap();
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "as"
        } else {
            "x86_64-elf-as"
        }
        .into()
    });
    for (tool, args) in [
        ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
        (gnu.as_str(), vec!["--64"]),
    ] {
        let output = temp.join("reference.o");
        let result = Command::new(tool).args(args).arg(&input).arg("-o").arg(&output).output()
            .unwrap_or_else(|e| panic!("{tool} required for BMI differential matrix (set KEEL_GNU_AS for GNU cross assembler): {e}"));
        assert!(
            result.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            text(&ours),
            text(&std::fs::read(output).unwrap()),
            "{tool}: {} cases",
            cases.len()
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn bmi_gating_and_invalid_operands() {
    for source in matrix() {
        let op = Instruction::parse_intel(&source).unwrap().opcode;
        let required = op.required_features();
        let mut typed = Program::new();
        typed.x86_instruction(Instruction::parse_intel(&source).unwrap());
        assert!(
            keel_asm::x86::assemble_program_object(&typed).is_err(),
            "{source}"
        );
        assert!(
            assemble_program_object_with_features(&typed, required).is_ok(),
            "{source}"
        );
        assert!(!required.avx);
        assert_ne!(required.bmi1, required.bmi2);
        let error = assemble_object(&source).unwrap_err().to_string();
        assert!(
            error.contains(if required.bmi1 { "bmi1" } else { "bmi2" }),
            "{error}"
        );
        assert!(
            assemble_object_with_features(&source, required).is_ok(),
            "{source}"
        );
        let opposite = Features {
            bmi1: !required.bmi1,
            bmi2: !required.bmi2,
            avx: true,
            xsave: true,
            osxsave: true,
            ..Features {
                popcnt: true,
                sse41: true,
                ..Features::NONE
            }
        };
        assert!(
            assemble_object_with_features(&source, opposite).is_err(),
            "{source}"
        );
    }
    for source in [
        "andn ax,bx,cx",
        "blsi al,bl",
        "shlx rax,ebx,rcx",
        "shrx eax,ebx,rcx",
        "sarx eax,word ptr [rax],ecx",
        "bextr eax,ebx,4",
        "andn eax,[rax],ebx",
        "pext eax,ebx,rcx",
        "pdep eax,xmm0,ecx",
        "mulx rax,ecx,rdx",
        "blsr [rax],rbx",
        "blsmsk eax,xmm0",
        "bzhi eax,ebx,cl",
        "rorx rax,rbx,256",
        "rorx rax,rbx,-1",
        "rorx rax,rbx,label",
        "rorx rax,rbx,cl",
        "lock andn eax,ebx,ecx",
        "shlx eax,ebx",
        "blsi eax,ebx,ecx",
        "tzcnt eax,byte ptr [rax]",
    ] {
        assert!(
            assemble_object_with_features(source, BMI).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn bmi_reserved_fields_and_known_bytes() {
    for (source, expected) in [
        ("andn r9,r10,r11", vec![0xc4, 0x42, 0xa8, 0xf2, 0xcb]),
        ("blsi r15,r11", vec![0xc4, 0xc2, 0x80, 0xf3, 0xdb]),
        ("rorx r9,r11,64", vec![0xc4, 0x43, 0xfb, 0xf0, 0xcb, 0x40]),
        ("tzcnt r9,r11", vec![0xf3, 0x4d, 0x0f, 0xbc, 0xcb]),
    ] {
        assert_eq!(
            text(&assemble_object_with_features(source, BMI).unwrap()),
            expected,
            "{source}"
        );
    }
    for source in matrix().into_iter().filter(|s| !s.starts_with("tzcnt")) {
        let bytes = text(&assemble_object_with_features(&source, BMI).unwrap());
        let vex = usize::from(bytes[0] == 0x64);
        assert_eq!(bytes[vex], 0xc4, "{source}");
        assert_eq!(bytes[vex + 2] & 4, 0, "L must be zero: {source}");
        if source.starts_with("rorx") {
            assert_eq!(bytes[vex + 2] & 0x78, 0x78, "reserved vvvv: {source}");
        }
        if source.starts_with("bls") {
            assert_eq!(bytes[vex + 1] & 0x80, 0x80, "reserved R: {source}");
        }
    }
}

#[test]
fn bmi_rip_relocations_include_trailing_immediate() {
    let source = ".intel_syntax noprefix\n.text\nandn r9,r10,[rip+external+7]\nbextr r9,[rip+external+7],r10\nblsi r9,[rip+external+7]\ntzcnt r9,[rip+external+7]\nrorx r9,[rip+external+7],255\nshlx r9,[rip+external+7],r10\n";
    let ours = assemble_object_with_features(source, BMI).unwrap();
    let temp = std::env::temp_dir().join(format!("keel-bmi-relocations-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    std::fs::write(&input, source).unwrap();
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "linux") {
            "as".into()
        } else {
            "x86_64-elf-as".into()
        }
    });
    let section = |bytes: &[u8]| {
        let object = object::File::parse(bytes).unwrap();
        let text = object.section_by_name(".text").unwrap();
        (
            text.data().unwrap().to_vec(),
            text.relocations()
                .map(|(o, r)| (o, r.addend(), r.flags()))
                .collect::<Vec<_>>(),
        )
    };
    for (tool, args) in [
        ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
        (gnu.as_str(), vec!["--64"]),
    ] {
        let output = temp.join("reference.o");
        let result = Command::new(tool)
            .args(args)
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            section(&ours),
            section(&std::fs::read(output).unwrap()),
            "{tool}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
