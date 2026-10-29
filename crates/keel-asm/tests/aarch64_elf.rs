use object::{Object, ObjectSection, ObjectSymbol};

#[test]
fn preserves_elf_names_visibility_common_sizes_and_data() {
    let bytes = keel_asm::assemble_elf_object(
        r#"
.text
.global LinuxName
.hidden LinuxName
.type LinuxName, %function
LinuxName: mov x0, #42; ret // same-line statements
.size LinuxName, .-LinuxName
.weak optional
.data
.asciz "hello, /* not a comment */; // world"
.byte -128, 255
.hword 65535
.xword 0xffffffffffffffff
.comm common, 32, 16
"#,
    )
    .unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.architecture(), object::Architecture::Aarch64);
    let symbol = file.symbol_by_name("LinuxName").unwrap();
    assert_eq!(symbol.size(), 8);
    assert!(symbol.is_global());
    assert_eq!(symbol.kind(), object::SymbolKind::Text);
    assert_eq!(
        symbol.flags(),
        object::SymbolFlags::Elf {
            st_info: 18,
            st_other: 2
        }
    );
    let common = file.symbol_by_name("common").unwrap();
    assert!(common.is_common());
    assert_eq!(common.size(), 32);
    assert_eq!(common.address(), 16);
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert!(data.starts_with(b"hello, /* not a comment */; // world\0\x80\xff\xff\xff"));
    assert_eq!(&data[data.len() - 8..], &[255; 8]);
}
#[test]
fn rejects_truncation_duplicate_definitions_and_invalid_sections() {
    for source in [
        ".byte 256",
        ".byte -129",
        ".word 0x100000000",
        ".xword 0x10000000000000000",
        "a: ret\na: ret",
        ".bss\n.word 1",
        ".data\nret",
        ".section .x,\"awx\",%progbits\n.balign 3",
        ".ascii \"unterminated",
        "/* unterminated",
    ] {
        assert!(keel_asm::assemble_elf_object(source).is_err(), "{source}");
    }
}
#[test]
fn instruction_bytes_match_clang_aarch64_elf() {
    let source = ".text\nmov x0, #42\nadd x1, x0, #17\nsub x2, x1, x0\nstp x29, x30, [sp, #-16]!\nmov x29, sp\nldr x3, [x1, #8]\nstr w2, [sp, #8]\nfadd d0, d1, d2\nadd v0.4s, v1.4s, v2.4s\nldp x29, x30, [sp], #16\nret\n";
    let dir = std::env::temp_dir().join(format!("keel-arm-elf-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("input.s"), source).unwrap();
    let output = std::process::Command::new("clang")
        .args(["--target=aarch64-unknown-linux-gnu", "-c"])
        .arg(dir.join("input.s"))
        .arg("-o")
        .arg(dir.join("reference.o"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference = std::fs::read(dir.join("reference.o")).unwrap();
    let actual = keel_asm::assemble_elf_object(source).unwrap();
    let text = |bytes: &[u8]| {
        object::File::parse(bytes)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(text(&actual), text(&reference));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn symbolic_leb_fields_never_shrink() {
    // LSDA-style fields whose growth a later `.p2align` absorbs. Clang keeps
    // the wider encoding and pads it.
    let bytes = keel_asm::assemble_elf_object(
        ".section .rodata,\"a\"\n.uleb128 .Lb-.La\n.La:\n.p2align 2\n.space 125\n.Lb:\n\
         .sleb128 .Ld-.Lc\n.Lc:\n.p2align 2\n.space 62\n.Ld:\n.p2align 2\n\
         .sleb128 .Le-.Lf\n.Le:\n.p2align 2\n.space 62\n.Lf:\n",
    )
    .unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let data = file.section_by_name(".rodata").unwrap().data().unwrap();
    assert_eq!(&data[..2], &[0xff, 0]);
    assert_eq!(&data[129..131], &[0xbf, 0]);
    assert_eq!(&data[196..198], &[0xc0, 0x7f]);
}
