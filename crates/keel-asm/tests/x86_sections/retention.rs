use crate::common::run;
use keel_asm_ir::{Directive as D, Program};
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};
#[test]
fn elf_retain_section_flag_and_osabi_match_gnu_as() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("asm-retain-elf");
    fs::create_dir_all(&dir).unwrap();
    let source = ".section .text,\"axR\",@progbits\n.byte 0xc3\n.section .text,\"ax\",@progbits\n.byte 0x90\n.section .custom,\"awR\",@progbits\n.p2align 5\n.globl x\nx: .long 7\n.section .custom_bss,\"awR\",@nobits\n.p2align 6\ny: .zero 16\n";
    let input = dir.join("oracle.s");
    let output = dir.join("oracle.o");
    fs::write(&input, source).unwrap();
    let assembler = std::env::var("KEEL_TEST_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-as".into()
        } else {
            "as".into()
        }
    });
    run(Command::new(assembler)
        .arg("--64")
        .arg(&input)
        .arg("-o")
        .arg(&output));
    let oracle = fs::read(&output).unwrap();
    let actual = keel_asm::x86::assemble_object(source).unwrap();
    assert_eq!(actual[7], oracle[7]);
    assert_eq!(actual[7], 3);
    let a = object::File::parse(actual.as_slice()).unwrap();
    let b = object::File::parse(oracle.as_slice()).unwrap();
    for name in [".text", ".custom", ".custom_bss"] {
        let a = a.section_by_name(name).unwrap();
        let b = b.section_by_name(name).unwrap();
        assert_eq!(
            (a.flags(), a.kind(), a.align(), a.size(), a.data().unwrap()),
            (b.flags(), b.kind(), b.align(), b.size(), b.data().unwrap())
        );
    }
    let retained = |bytes: &[u8]| {
        let file = object::File::parse(bytes).unwrap();
        file.sections()
            .filter(|s| s.name().unwrap() == ".text")
            .map(|s| (s.flags(), s.data().unwrap().to_vec()))
            .collect::<Vec<_>>()
    };
    assert_eq!(retained(&actual), retained(&oracle));
    let mut p = Program::new();
    p.directive(D::ElfSection {
        name: ".custom".into(),
        flags: 0x200003,
        nobits: false,
    });
    p.directive(D::Align(5));
    p.directive(D::Global("x".into()));
    p.label("x");
    p.directive(D::Bytes(vec![7, 0, 0, 0]));
    let bytes = keel_asm::x86::assemble_program_object(&p).unwrap();
    let o = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(
        o.section_by_name(".custom").unwrap().flags(),
        a.section_by_name(".custom").unwrap().flags()
    );
    assert!(keel_asm::x86::assemble_object(".section .custom,\"aQ\",@progbits\n").is_err());
}
