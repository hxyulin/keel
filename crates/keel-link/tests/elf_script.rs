use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

fn link(source: &str, script: &str) -> Result<elf::Executable, String> {
    let bytes = keel_asm::x86::assemble_object(source).unwrap();
    link_bytes(&bytes, script)
}
fn link_bytes(bytes: &[u8], script: &str) -> Result<elf::Executable, String> {
    elf::link(
        &[Input {
            name: "script-fixture.o",
            bytes,
        }],
        elf::Options {
            script: Some(script.into()),
            ..Default::default()
        },
    )
}
fn value(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
const SOURCE: &str = ".section .text.z,\"ax\"\n.globl startup\nstartup: ret\n\
.section .text.a,\"ax\"\n.byte 0x90\n\
.section .data,\"aw\"\n.globl pointer\npointer: .quad startup\n.long startup - .\n\
.section .discard.me,\"a\"\n.quad missing_discarded\n\
.bss\n.globl zero\nzero: .zero 32\n";
const SCRIPT: &str = r#"
OUTPUT_FORMAT("elf64-x86-64")
OUTPUT_ARCH(i386:x86-64)
ENTRY(physical_entry)
alias = startup;
PHDRS { text PT_LOAD FLAGS(5); data PT_LOAD FLAGS(6); }
SECTIONS {
 . = 0xffffffff81000000;
 physical_entry = ABSOLUTE(startup - 0xffffffff80000000);
 .text : AT(ADDR(.text) - 0xffffffff80000000) {
   text_begin = .; KEEP(*(SORT(.text.*))) . = ALIGN(16); text_end = .;
 } :text = 0xcccccccc
 . = ALIGN(4096);
 .data : AT(ADDR(.data) - 0xffffffff80000000) { *(.data) } :data
 .bss : AT(ADDR(.bss) - 0xffffffff80000000) { *(.bss) *(COMMON) . += 64; }
 /DISCARD/ : { *(.discard.*) }
 ASSERT(SIZEOF(.text) == 16, "text size")
 ASSERT(LOADADDR(.data) == ADDR(.data) - 0xffffffff80000000, "load address")
}
PROVIDE(unused = nonexistent);
"#;
#[test]
fn high_addresses_aliases_aggregation_and_physical_entry() {
    let exe = link(SOURCE, SCRIPT).unwrap();
    assert!(exe.bytes.len() < 16384);
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(value(&file, "startup"), 0xffffffff81000001);
    assert_eq!(value(&file, "alias"), value(&file, "startup"));
    assert_eq!(file.entry(), 0x1000001);
    assert_eq!(value(&file, "text_end") - value(&file, "text_begin"), 16);
    let text = file.section_by_name(".text").unwrap();
    assert_eq!(
        text.data().unwrap(),
        &[
            0x90, 0xc3, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc,
            0xcc, 0xcc
        ]
    );
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert_eq!(
        u64::from_le_bytes(data[..8].try_into().unwrap()),
        value(&file, "startup")
    );
    assert_eq!(i32::from_le_bytes(data[8..12].try_into().unwrap()), -4103);
    assert_eq!(file.section_by_name(".bss").unwrap().size(), 96);
    assert!(file.section_by_name(".text.a").is_none());
    assert!(file.section_by_name(".discard.me").is_none());
    let paddr = u64::from_le_bytes(exe.bytes[88..96].try_into().unwrap());
    assert_eq!(paddr, 0x1000000);
}
#[test]
fn rejects_orphans_cycles_assertions_and_unsupported_commands() {
    let source = ".text\n.globl _start\n_start: ret\n";
    for (script, expected) in [
        ("SECTIONS { .other : { *(.other) } }", "orphan"),
        (
            "a = b; b = a; SECTIONS { .text 4096 : { *(.text) } }",
            "cyclic",
        ),
        (
            "SECTIONS { .text 4096 : { *(.text) } ASSERT(0, \"broken\") }",
            "broken",
        ),
        (
            "SECTIONS { .text 4096 : { BYTE(1) *(.text) } }",
            "unsupported",
        ),
        ("SECTIONS { .text 4096 : { *(.text) . = 0; } }", "backwards"),
    ] {
        let error = link(source, script).unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}
#[test]
fn common_and_section_relative_constants() {
    let exe = link(".text\n.globl _start\n_start: ret\n.comm shared,16,16\n", "SECTIONS { .text 4096 : { *(.text) } .bss : { *(COMMON) boundary = 16; absolute = ABSOLUTE(16); } }").unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(value(&file, "shared"), 4112);
    assert_eq!(value(&file, "boundary"), 4128);
    assert_eq!(value(&file, "absolute"), 16);
}
#[test]
fn gnu_differential() {
    let Ok(ld) = std::env::var("KEEL_GNU_LD") else {
        eprintln!("GNU differential skipped: set KEEL_GNU_LD");
        return;
    };
    let dir = std::env::temp_dir().join(format!("keel-script-differential-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        (
            SOURCE,
            SCRIPT,
            &[
                "startup",
                "alias",
                "physical_entry",
                "text_begin",
                "text_end",
                "pointer",
                "zero",
            ][..],
            &[".text", ".data", ".bss"][..],
        ),
        (
            ".section .text.b,\"ax\"\n.globl _start\n_start: ret\n.section .text.a,\"ax\"\n.byte 0x90\n",
            "SECTIONS { .text 4096 : { *(.text.a .text.b) . += 3; } = 0x11223344 }",
            &["_start"][..],
            &[".text"][..],
        ),
        (
            include_str!("fixtures/linux-notes/aggregate.s"),
            "PHDRS { text PT_LOAD FLAGS(5); note PT_NOTE FLAGS(0); } SECTIONS { .text 4096 : { *(.text) } :text .notes : { KEEP(*(.note.*)) } :text :note }",
            &["_start"][..],
            &[".text", ".notes"][..],
        ),
        (
            ".text\n.globl _start\n_start: .long target - .\nret\n",
            "target = 0xffffffff81000000; SECTIONS { .text 0x100000 : { *(.text) } }",
            &["_start", "target"][..],
            &[".text"][..],
        ),
    ];
    for (source, script, symbol_names, section_names) in cases {
        let input = keel_asm::x86::assemble_object(source).unwrap();
        std::fs::write(dir.join("input.o"), &input).unwrap();
        std::fs::write(dir.join("layout.lds"), script).unwrap();
        let status = std::process::Command::new(&ld)
            .args([
                "-z",
                "max-page-size=4096",
                "-T",
                "layout.lds",
                "input.o",
                "-o",
                "gnu",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let reference = std::fs::read(dir.join("gnu")).unwrap();
        let ours = link_bytes(&input, script).unwrap();
        let a = object::File::parse(ours.bytes.as_slice()).unwrap();
        let b = object::File::parse(reference.as_slice()).unwrap();
        assert_eq!(a.entry(), b.entry());
        for name in symbol_names {
            assert_eq!(value(&a, name), value(&b, name), "{name}");
        }
        for name in section_names {
            let sa = a.section_by_name(name).unwrap();
            let sb = b.section_by_name(name).unwrap();
            assert_eq!(sa.address(), sb.address(), "{name} address");
            assert_eq!(sa.size(), sb.size(), "{name} size");
            assert_eq!(sa.data().unwrap(), sb.data().unwrap(), "{name} data");
            assert_eq!(sa.file_range(), sb.file_range(), "{name} file range");
        }
        let phnum = u16::from_le_bytes(ours.bytes[56..58].try_into().unwrap()) as usize;
        assert_eq!(
            &ours.bytes[64..64 + phnum * 56],
            &reference[64..64 + phnum * 56],
            "program headers"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
#[test]
#[ignore = "requires the pinned external kernel aggregate and script"]
fn pinned_kernel_aggregate() {
    let object = std::fs::read(
        std::env::var("KEEL_KERNEL_AGGREGATE")
            .unwrap_or_else(|_| "/tmp/keel-linux64-keel-aggregate.o".into()),
    )
    .unwrap();
    let script = std::fs::read_to_string("/tmp/keel-linux64-vmlinux.lds").unwrap();
    let exe = link_bytes(&object, &script).unwrap();
    eprintln!("native final link: {} bytes", exe.bytes.len());
    if let Ok(path) = std::env::var("KEEL_KERNEL_OUTPUT") {
        std::fs::write(path, exe.bytes).unwrap();
    }
}

#[test]
fn input_lists_preserve_input_order_and_fill_restarts_at_each_hole() {
    let source = ".section .text.b,\"ax\"\n.globl _start\n_start: .byte 0xc3\n.section .text.a,\"ax\"\n.byte 0x90\n";
    let script = "SECTIONS { .text 4096 : { *(.text.a .text.b) . += 3; } = 0x11223344 }";
    let exe = link(source, script).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[0xc3, 0x90, 0x11, 0x22, 0x33]
    );
}

#[test]
fn linux_notes_survive_default_and_script_layout_and_reject_writes() {
    let source = include_str!("fixtures/linux-notes/aggregate.s");
    let input = keel_asm::x86::assemble_object(source).unwrap();
    let expected = include_bytes!("fixtures/linux-notes/aggregate.note.Linux.bin");
    let ordinary = elf::link(
        &[Input {
            name: "linux-note.o",
            bytes: &input,
        }],
        Default::default(),
    )
    .unwrap();
    let script = "PHDRS { text PT_LOAD FLAGS(5); note PT_NOTE FLAGS(0); } SECTIONS { .text 4096 : { *(.text) } :text .notes : { KEEP(*(.note.*)) } :text :note }";
    let scripted = link_bytes(&input, script).unwrap();
    for (exe, name) in [(&ordinary, ".note.Linux"), (&scripted, ".notes")] {
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(
            file.section_by_name(name).unwrap().data().unwrap(),
            expected
        );
        let count = u16::from_le_bytes(exe.bytes[56..58].try_into().unwrap()) as usize;
        assert!((0..count).any(|i| {
            let at = 64 + i * 56;
            u32::from_le_bytes(exe.bytes[at..at + 4].try_into().unwrap()) == 4
                && u64::from_le_bytes(exe.bytes[at + 32..at + 40].try_into().unwrap()) == 48
        }));
    }
    let malicious = source.replace(".long 0\n", ".long _start\n");
    let error = link(&malicious, script).unwrap_err();
    assert!(error.contains("relocations into .note.Linux"), "{error}");
}

#[test]
fn relocation_overflow_is_checked_at_kernel_addresses() {
    let script = "SECTIONS { .text 0xffffffff81000000 : { *(.text) } .data : { *(.data) } }";
    let error = link(
        ".text\n.globl _start\n_start: ret\n.data\n.long _start\n",
        script,
    )
    .unwrap_err();
    assert!(error.contains("relocation 10 overflow"), "{error}");
}
