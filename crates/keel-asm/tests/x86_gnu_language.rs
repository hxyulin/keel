use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::Program;
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::process::Command;

const SOURCE: &str = r#"
.att_syntax prefix
.text
.code64
.globl entry
entry:
1: nop
.pushsection __ex_table,"a",@progbits
.long 1b - .
.pushsection .data,"aw",@progbits
.quad entry, external+7
.popsection
.long 2f - .
.popsection
jmp 2f
.macro move arg:req
movq \arg, %rax
.Lunique_\@: nop
jmp .Lunique_\@
.endm
move %rbx
move %rcx
.macro inner
.byte \@
.endm
.macro outer
.byte \@
inner
.byte \@
.endm
outer
outer
2: ret
jmp 3f
.org 256, 0x90
3: jmp 1b
.org 272, 0x90
.data
base: .byte 5
.org base+8, 0x42
.previous
.ident "test compiler"
.byte 0xcc
.previous
.byte 6
.bss
.org 17
"#;

// Normalize section-symbol relocations and named local relocations to the
// same section + addend. Keep ELF relocation types and offsets independent.
fn snapshot(
    bytes: &[u8],
) -> Vec<(
    String,
    u64,
    object::SectionFlags,
    Vec<u8>,
    Vec<(u64, u32, String, i64)>,
)> {
    let file = object::File::parse(bytes).unwrap();
    let mut out = Vec::new();
    for name in [".text", ".data", ".bss", "__ex_table"] {
        let s = file.section_by_name(name).unwrap();
        let relocs = s
            .relocations()
            .map(|(offset, r)| {
                let object::RelocationFlags::Elf { r_type } = r.flags() else {
                    panic!()
                };
                let RelocationTarget::Symbol(index) = r.target() else {
                    panic!()
                };
                let sym = file.symbol_by_index(index).unwrap();
                let (target, addend) = if let Some(section) = sym.section_index() {
                    (
                        file.section_by_index(section)
                            .unwrap()
                            .name()
                            .unwrap()
                            .to_owned(),
                        r.addend() + sym.address() as i64,
                    )
                } else {
                    (sym.name().unwrap().to_owned(), r.addend())
                };
                (offset, r_type, target, addend)
            })
            .collect();
        out.push((
            name.into(),
            s.size(),
            s.flags(),
            s.data().unwrap().to_vec(),
            relocs,
        ));
    }
    out
}

#[test]
fn gnu_language_objects_match_clang_and_gnu() {
    let ours = assemble_object(SOURCE).unwrap();
    let mut program = Program::new();
    program.text(SOURCE);
    assert_eq!(ours, assemble_program_object(&program).unwrap());
    let temp = std::env::temp_dir().join(format!("keel-gnu-language-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    std::fs::write(&input, SOURCE).unwrap();
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
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
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            snapshot(&ours),
            snapshot(&std::fs::read(&output).unwrap()),
            "{tool}"
        );
        // Both assemblers ignore explicit fill in a NOBITS origin.
        let source = ".bss\n.org 17, 0x55\n";
        std::fs::write(&input, source).unwrap();
        let mut command = Command::new(tool);
        if tool == "clang" {
            command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        } else {
            command.arg("--64");
        }
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reference = std::fs::read(&output).unwrap();
        let file = object::File::parse(&*reference).unwrap();
        let ours = assemble_object(source).unwrap();
        let ours = object::File::parse(&*ours).unwrap();
        assert_eq!(file.section_by_name(".bss").unwrap().size(), 17);
        assert_eq!(ours.section_by_name(".bss").unwrap().size(), 17);
        assert_eq!(file.section_by_name(".bss").unwrap().data().unwrap(), b"");
        // Restore the common differential source for the next assembler.
        std::fs::write(&input, SOURCE).unwrap();
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn numeric_labels_repeat_cross_sections_and_keep_lexical_boundaries() {
    let bytes = assemble_object(
        r#"
.att_syntax prefix
1 : 01: nop; jmp 1b
jmp 1f
1: nop
10: movabsq $10b, %rax
.data
.quad 1b, 10b
.ascii "1b 10f # ;"
.byte 0b101, 0xff
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        &file.section_by_name(".text").unwrap().data().unwrap()[..6],
        &[0x90, 0xeb, 0xfd, 0xeb, 0, 0x90]
    );
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert!(data.ends_with(b"1b 10f # ;\x05\xff"));
    assert!(
        !file
            .symbols()
            .any(|s| s.name().unwrap().starts_with(".Lkeel.numeric"))
    );
}

#[test]
fn org_uses_current_section_offsets_and_zero_default_fill() {
    let bytes = assemble_object(
        ".text\n.byte 1\n.org .text+2\n.org .+2\n.data\n.byte 2\n.org 4, -1\n.bss\n.org 9",
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[1, 0, 0, 0]
    );
    assert_eq!(
        file.section_by_name(".data").unwrap().data().unwrap(),
        &[2, 255, 255, 255]
    );
    assert_eq!(file.section_by_name(".bss").unwrap().size(), 9);
}

#[test]
fn unsupported_modes_and_invalid_language_fail_explicitly() {
    for (source, expected) in [
        (".code16", "only 64-bit"),
        (".code32", "only 64-bit"),
        (".code16gcc", "only 64-bit"),
        (".code64 1", "unsupported ELF directive"),
        ("jmp 1b", "undefined numeric"),
        ("jmp 1f", "undefined numeric"),
        (".quad 23f", "undefined numeric"),
        (".macro f\nf\n.endm\nf", "exceeds"),
        (".popsection", "without .pushsection"),
        (".previous", "without a previous"),
        (".macro f a:req\n.endm\nf", "required macro argument"),
        (".macro f a:req,b\n.endm\nf ,1", "required macro argument"),
        (".byte 1\n.org 0", "backwards"),
        (".org -1", "nonnegative"),
        (".org missing", "undefined symbol"),
        (".data\nother:\n.text\n.org other+16", "current section"),
        (".org missing@GOTPCREL", "modifiers"),
    ] {
        let error = assemble_object(source).unwrap_err();
        assert!(error.message.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn comments_conditionals_and_expanded_error_lines() {
    let bytes = assemble_object(
        "/* block\ncomment */\n.if 0\n.code32\n.else\n.code64; nop # comment\n.endif\n",
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[0x90]
    );
    let e = assemble_object(".macro f a:req\nmovq \\a,%rax\n.endm\nf %bad").unwrap_err();
    assert_eq!(e.line, 2);
    assert!(e.message.contains("expanded at line 4"), "{e}");
}

#[test]
fn macro_state_and_numeric_labels_survive_program_fragments() {
    let mut p = Program::new();
    p.text(
        ".macro f reg:req\n.Lsame_\\@: movq \\reg, %rax\n.endm\n.att_syntax prefix\n1: f %rbx\n",
    );
    // A typed item prevents Program from merging adjacent text fragments.
    p.label("between");
    p.text("f %rcx\njmp 1b\n");
    let bytes = assemble_program_object(&p).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[0x48, 0x89, 0xd8, 0x48, 0x89, 0xc8, 0xeb, 0xf8]
    );
}

#[test]
fn invalid_constructs_also_fail_independent_assemblers() {
    let temp =
        std::env::temp_dir().join(format!("keel-gnu-language-negative-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    for source in [
        ".byte 1\n.org 0\n",
        "jmp 1f\n",
        "jmp 1b\n",
        ".macro f a:req\n.endm\nf\n",
    ] {
        assert!(assemble_object(source).is_err());
        std::fs::write(&input, source).unwrap();
        for (tool, args) in [
            ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
            (gnu.as_str(), vec!["--64"]),
        ] {
            let result = Command::new(tool)
                .args(args)
                .arg(&input)
                .arg("-o")
                .arg(temp.join("out.o"))
                .output()
                .unwrap();
            assert!(!result.status.success(), "{tool} accepted {source:?}");
        }
    }
    // LLVM accepts this cross-section .org; GNU rejects it. Follow GNU's
    // current-section rule, rather than silently treating it as an address.
    let source = ".data\nother:\n.text\n.org other+16\n";
    std::fs::write(&input, source).unwrap();
    assert!(
        assemble_object(source)
            .unwrap_err()
            .message
            .contains("current section")
    );
    assert!(
        !Command::new(&gnu)
            .arg("--64")
            .arg(&input)
            .arg("-o")
            .arg(temp.join("out.o"))
            .output()
            .unwrap()
            .status
            .success()
    );
    // GNU preserves .previous across .lcomm. LLVM changes it to .bss,
    // so this state transition specifically uses GNU as the oracle.
    let source = ".data\n.byte 1\n.text\n.lcomm local_buffer,3\n.previous\n.byte 2\n";
    std::fs::write(&input, source).unwrap();
    let output = temp.join("common.o");
    let result = Command::new(&gnu)
        .arg("--64")
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
    let reference = std::fs::read(output).unwrap();
    let reference = object::File::parse(&*reference).unwrap();
    let ours = assemble_object(source).unwrap();
    let ours = object::File::parse(&*ours).unwrap();
    assert_eq!(
        reference.section_by_name(".data").unwrap().data().unwrap(),
        &[1, 2]
    );
    assert_eq!(
        ours.section_by_name(".data").unwrap().data().unwrap(),
        &[1, 2]
    );
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn section_stack_restores_unique_section_identity() {
    let bytes = assemble_object(
        r#"
.pushsection .same,"a",@progbits,unique,1
.byte 1
.pushsection .same,"a",@progbits,unique,2
.byte 2
.popsection
.byte 3
.popsection
.byte 0x90
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let same: Vec<_> = file
        .sections()
        .filter(|s| s.name() == Ok(".same"))
        .map(|s| s.data().unwrap().to_vec())
        .collect();
    assert_eq!(same, [vec![1, 3], vec![2]]);
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[0x90]
    );
}
