use keel_asm::{assemble, assemble_object};
use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};
use std::process::Command;
mod common;
use common::{Temp, run};

#[derive(Debug, PartialEq)]
struct Record {
    section: String,
    offset: u64,
    target: String,
    kind: u8,
    length: u8,
    pcrel: bool,
}
fn records(bytes: &[u8]) -> Vec<Record> {
    let object = object::File::parse(bytes).unwrap();
    let mut records = Vec::new();
    for section in object.sections() {
        for (offset, relocation) in section.relocations() {
            let target = match relocation.target() {
                RelocationTarget::Symbol(index) => object
                    .symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_string(),
                RelocationTarget::Section(index) => format!(
                    "section:{}",
                    object.section_by_index(index).unwrap().name().unwrap()
                ),
                _ => panic!("unexpected relocation target"),
            };
            let RelocationFlags::MachO {
                r_type,
                r_pcrel,
                r_length,
            } = relocation.flags()
            else {
                panic!("not Mach-O")
            };
            records.push(Record {
                section: section.name().unwrap().into(),
                offset,
                target,
                kind: r_type,
                length: r_length,
                pcrel: r_pcrel,
            });
        }
    }
    records.sort_by(|a, b| (&a.section, a.offset, a.kind).cmp(&(&b.section, b.offset, b.kind)));
    records
}
fn compare(source: &str) {
    let temp = Temp::new();
    let input = temp.0.join("data.s");
    let reference = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    let reference = std::fs::read(reference).unwrap();
    let ours = assemble_object(source).unwrap();
    let wanted = object::File::parse(reference.as_slice()).unwrap();
    let got = object::File::parse(ours.as_slice()).unwrap();
    for section in wanted.sections() {
        let actual = got
            .sections()
            .find(|s| {
                s.name().unwrap() == section.name().unwrap()
                    && s.segment_name().unwrap() == section.segment_name().unwrap()
            })
            .unwrap();
        assert_eq!(
            actual.size(),
            section.size(),
            "{} size",
            section.name().unwrap()
        );
        assert_eq!(
            actual.data().unwrap(),
            section.data().unwrap(),
            "{} data",
            section.name().unwrap()
        );
        assert!(actual.align() >= section.align());
    }
    assert_eq!(records(&ours), records(&reference));
    for expected in wanted.symbols().filter(|s| !s.is_undefined()) {
        let Some(symbol) = got
            .symbols()
            .find(|s| s.name().unwrap() == expected.name().unwrap())
        else {
            assert!(
                expected.name().unwrap().starts_with("ltmp"),
                "missing symbol {}",
                expected.name().unwrap()
            );
            continue; // LLVM synthesizes section-start temporary symbols.
        };
        let expected_section = wanted
            .section_by_index(expected.section_index().unwrap())
            .unwrap();
        let actual_section = got
            .section_by_index(symbol.section_index().unwrap())
            .unwrap();
        assert_eq!(
            actual_section.name().unwrap(),
            expected_section.name().unwrap()
        );
        assert_eq!(
            symbol.address() - actual_section.address(),
            expected.address() - expected_section.address()
        );
        assert_eq!(symbol.is_global(), expected.is_global());
    }
}

#[test]
fn initialized_sections_and_pointers_match_clang() {
    compare(
        ".text\n.globl _entry, _bytes, _integers, _table, _mutable, _other\n_entry: nop\nret\n.section __TEXT,__const\n_bytes: .byte 0, 127, 255, -128\n.p2align 3\n_integers: .short 0x1234, -1\n.long 0xffffffff, -2147483648\n.quad 0xffffffffffffffff, -9223372036854775808\n.space 3, 7\n.section __DATA,__const\n.p2align 3\n_table: .quad _entry, _bytes+2, _external-8\n.section __DATA,__data\n_mutable: .long 42\n.zero 4\n.p2align 4\n_other: .quad 123\n.text\nret\n",
    );
}

#[test]
fn differences_and_section_switches_match_clang() {
    compare(
        ".text\n.globl _start, _end, _data, _differences\n_start: nop\nLbegin: nop\nLend: ret\n_end: ret\n.long Lend-Lbegin, Lbegin-Lend, _end-_start+4\n.data\n_data: .long 42\n.p2align 3\n_differences: .quad _data-_start+8\n.long _start-_data-4\n.text\n.quad 3\nret\n",
    );
}

#[test]
fn zero_fill_uses_virtual_space_and_correct_symbol_sections() {
    let source = ".globl _small, _large, _value\n.zerofill __DATA,__bss,_small,3,0\n.zerofill __DATA,__bss,_large,1048576,4\n.data\n_value: .quad 42\n";
    compare(source);
    let bytes = assemble_object(source).unwrap();
    assert!(bytes.len() < 1024);
    let object = object::File::parse(bytes.as_slice()).unwrap();
    let section = object.section_by_name("__bss").unwrap();
    assert_eq!(section.size(), 1048592);
    assert!(section.data().unwrap().is_empty());
    let large = object
        .symbols()
        .find(|s| s.name().unwrap() == "_large")
        .unwrap();
    assert_eq!(large.address() - section.address(), 16);
    let value = object
        .symbols()
        .find(|s| s.name().unwrap() == "_value")
        .unwrap();
    assert_ne!(large.section_index(), value.section_index());
}

#[test]
fn initialized_and_zero_tls_sections_match_clang() {
    compare(
        ".section __DATA,__thread_data,thread_local_regular\n.p2align 3\n_init: .quad 42\n.tbss _zero,4,2\n.section __DATA,__thread_vars,thread_local_variables\n.globl _counter\n.p2align 3\n_counter: .quad __tlv_bootstrap, 0, _init\n",
    );
}

#[test]
fn data_and_zero_fill_directives_reject_invalid_operands() {
    for source in [
        ".byte 256",
        ".short -32769",
        ".long 0x100000000",
        ".quad 0x10000000000000000",
        ".byte",
        ".byte 1,",
        ".space -1",
        ".space 1,256",
        ".zero 1,2,3",
        ".quad -_symbol",
        ".quad _a+_b",
        ".quad _a-_b-_c",
        ".long _pointer",
        ".short _a-_b",
        ".quad _a@PAGE",
        ".data\nnop",
        ".byte 1\nnop",
        ".bss\n.byte 1",
        ".bss\n.quad _symbol",
        ".bss\n.space 1,2",
        ".zerofill __DATA,__bss,_x,-1,0",
        ".zerofill __DATA,__bss,_x,1,17",
        ".zerofill __TEXT,__const,_x,1,0",
        ".tbss _x,1,17",
        ".data\n_x: .quad 1\n.text\n_x: ret",
        "adr x0, _data\n.data\n_data: .quad 0",
        "b.eq _data\n.data\n_data: .quad 0",
        ".section __DATA,__data,zerofill",
    ] {
        assert!(assemble(source).is_err(), "{source}");
    }
}

#[test]
fn same_section_differences_and_text_padding_are_resolved() {
    let a =
        assemble(".byte 1\n.p2align 3\nLbegin: nop\nLend: ret\n.long Lend-Lbegin, Lbegin-Lend\n")
            .unwrap();
    assert_eq!(&a.text[16..], &[4, 0, 0, 0, 252, 255, 255, 255]);
    assert!(a.relocations.is_empty());
    let folded = assemble(".quad _external-_external+3").unwrap();
    assert_eq!(folded.text, 3u64.to_le_bytes());
    assert!(folded.symbols.is_empty());
    common::compare_clang(".byte 1\n.p2align 3\nnop\n");
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn keel_generated_globals_and_tls_execute() {
    use common::execute;
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    for program in ["globals", "tls", "switch", "linkage"] {
        let source = std::fs::read_to_string(repo.join(format!("{program}.keel"))).unwrap();
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let source =
            keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
        let source: String = source
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                line != ".subsections_via_symbols"
            })
            .map(|line| format!("{line}\n"))
            .collect();
        let temp = Temp::new();
        let obj = temp.0.join("keel.o");
        let exe = temp.0.join("program");
        std::fs::write(&obj, assemble_object(&source).unwrap()).unwrap();
        run(Command::new("clang")
            .arg(repo.join(format!("{program}.c")))
            .arg(&obj)
            .arg("-o")
            .arg(&exe));
        assert_eq!(execute(&exe).code(), Some(0), "{program}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn pointer_tables_and_cross_section_differences_execute() {
    use common::execute;
    let source = ".text\n.globl _answer\n_answer: mov w0, #42\nret\n.section __TEXT,__const\n.globl _numbers\n.p2align 3\n_numbers: .quad 41,42\n.section __DATA,__const\n.globl _pointers, _delta, _delta32\n.p2align 3\n_pointers: .quad _numbers+8, _answer\n_delta: .quad _numbers-_answer+8\n_delta32: .long _answer-_numbers-4\n";
    let driver = "
#include <stdint.h>
extern long numbers[2];
extern uintptr_t pointers[2], delta;
extern int32_t delta32;
extern long answer(void);
int main(void) {
    if (*(long *)pointers[0] != 42 || ((long (*)(void))pointers[1])() != 42) return 1;
    if (delta != (uintptr_t)numbers - (uintptr_t)answer + 8) return 2;
    if ((uint32_t)delta32 != (uint32_t)((uintptr_t)answer - (uintptr_t)numbers - 4)) return 3;
    return 0;
}
";
    let temp = Temp::new();
    let obj = temp.0.join("data.o");
    let c = temp.0.join("driver.c");
    let exe = temp.0.join("data");
    std::fs::write(&obj, assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang").arg(&c).arg(&obj).arg("-o").arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}

#[test]
fn malformed_section_and_data_relocation_models_are_rejected() {
    let base = assemble(".data\n_ptr: .quad _external\n").unwrap();
    let mut a = base.clone();
    a.relocations[0].section = 42;
    assert!(a.to_macho().is_err());
    a = base.clone();
    a.relocations[0].offset = 1;
    assert!(a.to_macho().is_err());
    a = base.clone();
    a.sections[0].alignment = 17;
    assert!(a.to_macho().is_err());
    a = base.clone();
    a.sections[0].name = "name_exceeds_16_bytes".into();
    assert!(a.to_macho().is_err());
    a = base.clone();
    a.sections[0].kind = keel_asm::SectionKind::ZeroFill;
    assert!(a.to_macho().is_err());
    a = base.clone();
    a.symbols
        .iter_mut()
        .find(|s| s.name == "_ptr")
        .unwrap()
        .section = Some(42);
    assert!(a.to_macho().is_err());
    a = base;
    a.relocations[0].size = 4;
    assert!(a.to_macho().is_err());
    let error = assemble(
        "Lstart: nop\nLend: ret\n.quad Lend-Lstart+170141183460469231731687303715884105727",
    )
    .unwrap_err();
    assert!(error.message.contains("overflow"));
}
