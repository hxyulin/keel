//! GNU prefix statements retain their own byte positions and symbol boundaries.
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::x86::{SourceParser, Syntax};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn text(object: &[u8]) -> Vec<u8> {
    object::File::parse(object)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

fn gnu(source: &str) -> Vec<u8> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "keel-gnu-prefixes-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    let output = dir.join("output.o");
    std::fs::write(&input, source).unwrap();
    let tool = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    let run = Command::new(tool)
        .arg("--64")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let bytes = std::fs::read(output).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    bytes
}

#[test]
fn standalone_prefix_bytes_match_gnu_in_both_dialects() {
    let prefixes = [
        ("rep", 0xf3),
        ("repe", 0xf3),
        ("repz", 0xf3),
        ("repne", 0xf2),
        ("repnz", 0xf2),
        ("lock", 0xf0),
        ("data16", 0x66),
        ("addr32", 0x67),
        ("rex", 0x40),
        ("rex64", 0x48),
        ("cs", 0x2e),
        ("ds", 0x3e),
        ("fs", 0x64),
        ("gs", 0x65),
        ("notrack", 0x3e),
        ("xacquire", 0xf2),
        ("xrelease", 0xf3),
    ];
    for syntax in [".att_syntax prefix", ".intel_syntax noprefix"] {
        let mut source = format!(".text\n{syntax}\n");
        let mut expected = Vec::new();
        for (prefix, byte) in prefixes {
            source.push_str(&format!("{prefix}; nop\n"));
            expected.extend([byte, 0x90]);
        }
        source.push_str("ReP\n");
        expected.push(0xf3);
        let ours = assemble_object(&source).unwrap();
        assert_eq!(text(&ours), expected, "{source}");
        assert_eq!(text(&ours), text(&gnu(&source)), "{source}");
    }
}

#[test]
fn prefixes_remain_separate_from_labels_sections_and_alignment() {
    let source = r#"
.att_syntax prefix
.text
.globl before, middle, after, aligned
before: rep; middle: bsfq %rdi, %rax
after: rep
.p2align 2, 0xcc
aligned: nop
rep
.pushsection .data,"aw",@progbits
.byte 0x55
.popsection
nop
.long middle-before, after-middle, aligned-after
.long external
rep
call external
"#;
    let ours = assemble_object(source).unwrap();
    let reference = gnu(source);
    assert_eq!(text(&ours), text(&reference));
    let ours_file = object::File::parse(&*ours).unwrap();
    let reference_file = object::File::parse(&*reference).unwrap();
    for name in ["before", "middle", "after", "aligned"] {
        let find = |file: &object::File<'_>| {
            file.symbols()
                .find(|s| s.name() == Ok(name))
                .unwrap()
                .address()
        };
        assert_eq!(find(&ours_file), find(&reference_file), "{name}");
    }
    let relocations = |file: &object::File<'_>| {
        file.section_by_name(".text")
            .unwrap()
            .relocations()
            .map(|(offset, relocation)| {
                let RelocationTarget::Symbol(index) = relocation.target() else {
                    panic!("expected a named relocation target");
                };
                (
                    offset,
                    relocation.flags(),
                    relocation.addend(),
                    file.symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(relocations(&ours_file), relocations(&reference_file));
    assert_eq!(&text(&ours)[..5], &[0xf3, 0x48, 0x0f, 0xbc, 0xc7]);
    let program = SourceParser::new(Syntax::Att).parse(source).unwrap();
    assert_eq!(ours, assemble_program_object(&program).unwrap());
}

#[test]
fn whitespace_symbol_types_match_gnu_and_keep_existing_spellings() {
    let source = r#"
.text
.globl entry, object, tls, plain
.type entry STT_FUNC
entry: ret
.size entry, .-entry
.data
.type object, STT_OBJECT
object: .long 7
.size object, .-object
.section .tdata,"awT",@progbits
.type tls STT_TLS
tls: .long 9
.size tls, .-tls
.data
.type plain STT_NOTYPE
plain: .byte 1
.type "quoted symbol" STT_OBJECT
"quoted symbol": .byte 2
.type legacy, @function
legacy: .byte 0xc3
"#;
    let ours = assemble_object(source).unwrap();
    let reference = gnu(source);
    let ours = object::File::parse(&*ours).unwrap();
    let reference = object::File::parse(&*reference).unwrap();
    for name in ["entry", "object", "tls", "plain", "quoted symbol", "legacy"] {
        let find = |file: &object::File<'_>| {
            let s = file.symbols().find(|s| s.name() == Ok(name)).unwrap();
            (s.kind(), s.address(), s.size())
        };
        assert_eq!(find(&ours), find(&reference), "{name}");
    }
    for source in [
        ".type entry",
        ".type entry STT_FUNC extra",
        ".type entry,STT_FUNC,extra",
        ".type entry STT_GNU_IFUNC",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}
