//! REX2 encoding/decoding checks. No APX instruction executes on the host.
use keel_asm_ir::x86::{Features, Instruction, Syntax};
use keel_disasm::x86::decode;
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
fn assemble(source: &str, syntax: Syntax) -> Vec<u8> {
    text(
        &keel_asm::x86::assemble_object_with_syntax_and_features(
            source,
            syntax,
            Features::parse("apx_f,clflush").unwrap(),
        )
        .unwrap_or_else(|error| panic!("{source}: {error}")),
    )
}
#[test]
fn rex2_literal_fields_and_roundtrips() {
    for (source, expected) in [
        ("add r16,r17", &[0xd5, 0x58, 0x01, 0xc8][..]),
        ("push r16", &[0xd5, 0x10, 0x50]),
        ("pop r31", &[0xd5, 0x11, 0x5f]),
        ("pushp r16", &[0xd5, 0x18, 0x50]),
        ("popp r31", &[0xd5, 0x19, 0x5f]),
        (
            "mov qword ptr [r20+r28*4+8],r31",
            &[0xd5, 0x7e, 0x89, 0x7c, 0xa4, 0x08],
        ),
        ("mov r16,qword ptr [r21]", &[0xd5, 0x58, 0x8b, 0x45, 0x00]),
        (
            "jmpabs 0x123456789abcdef0",
            &[
                0xd5, 0x00, 0xa1, 0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12,
            ],
        ),
    ] {
        let bytes = assemble(source, Syntax::Intel);
        assert_eq!(bytes, expected, "{source}");
        let instruction =
            decode(&bytes, 0).unwrap_or_else(|| panic!("decode {source}: {bytes:02x?}"));
        assert_eq!(instruction.bytes, bytes);
        assert_eq!(
            assemble(&instruction.assembly(), Syntax::Intel),
            bytes,
            "{}",
            instruction.assembly()
        );
    }
}
#[test]
fn rex2_independent_oracles_and_addressing() {
    let source = ".intel_syntax noprefix\n.text\nadd r16,r17\nadd r31d,dword ptr [r20+r28*4+8]\nmov r16b,r31b\nmov r16w,word ptr [r21]\nmov qword ptr [r20+r28*4+8],r31\nlea r29,[r20+r28*8+128]\nmov eax,dword ptr [r20d+r28d*2+16]\nmov eax,dword ptr [rax+r20*8]\nmovdqa xmm9,xmmword ptr [r18]\nclflush [r31]\npush r16\npop r31\npushp r17\npopp r30\njmpabs 0x123456789abcdef0\n";
    let bytes = assemble(source, Syntax::Intel);
    let directory = std::env::temp_dir().join(format!("keel-apx-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("oracle.s");
    fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let mut command = if gnu {
            let mut command =
                Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            command.arg("--64");
            command
        } else {
            let mut command = Command::new("clang");
            command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            command
        };
        let output = directory.join("oracle.o");
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "GNU={gnu}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(bytes, text(&fs::read(output).unwrap()), "GNU={gnu}");
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let instruction = decode(&bytes[offset..], offset as u64)
            .unwrap_or_else(|| panic!("decode offset {offset}: {:02x?}", &bytes[offset..]));
        assert_eq!(
            assemble(&instruction.assembly(), Syntax::Intel),
            instruction.bytes,
            "{}",
            instruction.assembly()
        );
        offset += instruction.bytes.len();
    }
    assert_eq!(
        assemble("addq %r17,%r16", Syntax::Att),
        assemble("add r16,r17", Syntax::Intel)
    );
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn rex2_requires_feature_and_rejects_unimplemented_encodings() {
    for source in ["add r16,r17", "mov rax,[r16]", "pushp rax", "jmpabs 42"] {
        let error =
            keel_asm::x86::assemble_object_with_features(source, Features::default()).unwrap_err();
        assert!(error.to_string().contains("apx_f"), "{source}: {error}");
        assert!(
            Instruction::parse_intel(source)
                .unwrap()
                .required_features()
                .apx_f
        );
    }
    for source in [
        "mov ah,r16b",
        "mov ah,byte ptr [r16]",
        "crc32 r16,rax",
        "vaddps xmm0,xmm1,[r16]",
        "add r16,r17,r18",
        "mov r32,rax",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, Features::all_known()).is_err(),
            "{source}"
        );
    }
    for bytes in [
        &[0xd5][..],
        &[0xd5, 0],
        &[0x48, 0xd5, 0, 0x90],
        &[0xd5, 0, 0x0f, 0x90],
        &[0xd5, 0, 0xc4, 0xe1, 0x78, 0x58, 0xc0],
    ] {
        assert!(decode(bytes, 0).is_none(), "{bytes:02x?}");
    }
}

#[test]
fn absolute_jump_keeps_a_full_width_symbol_relocation() {
    let bytes = keel_asm::x86::assemble_object_with_features(
        "jmpabs destination+8",
        Features::parse("apx_f").unwrap(),
    )
    .unwrap();
    let object = object::File::parse(&*bytes).unwrap();
    let section = object.section_by_name(".text").unwrap();
    let relocations: Vec<_> = section.relocations().collect();
    assert_eq!(relocations.len(), 1);
    let (offset, relocation) = &relocations[0];
    assert_eq!(*offset, 3);
    assert_eq!(relocation.kind(), object::RelocationKind::Absolute);
    assert_eq!(relocation.size(), 64);
    assert_eq!(relocation.addend(), 8);
}
