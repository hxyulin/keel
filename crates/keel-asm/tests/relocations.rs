#[path = "relocations/subsections.rs"]
mod subsections;
use keel_asm::{RelocationKind, assemble, assemble_object};
use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};
use std::process::Command;
mod common;
use common::{Temp, compare_clang, run};

#[derive(Debug, PartialEq, Eq)]
struct Record {
    offset: u64,
    symbol: String,
    addend: i64,
    kind: u8,
    pcrel: bool,
    length: u8,
}
fn records(bytes: &[u8]) -> Vec<Record> {
    let file = object::File::parse(bytes).unwrap();
    let section = file.section_by_name("__text").unwrap();
    section
        .relocations()
        .map(|(offset, r)| {
            let RelocationTarget::Symbol(index) = r.target() else {
                panic!("expected symbol relocation")
            };
            let RelocationFlags::MachO {
                r_type,
                r_pcrel,
                r_length,
            } = r.flags()
            else {
                panic!("expected Mach-O relocation")
            };
            Record {
                offset,
                symbol: file.symbol_by_index(index).unwrap().name().unwrap().into(),
                addend: r.addend(),
                kind: r_type,
                pcrel: r_pcrel,
                length: r_length,
            }
        })
        .collect()
}
fn compare_relocations(source: &str) {
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&output));
    let reference = std::fs::read(output).unwrap();
    let ours = assemble_object(source).unwrap();
    let ours_file = object::File::parse(ours.as_slice()).unwrap();
    let clang_file = object::File::parse(reference.as_slice()).unwrap();
    assert_eq!(
        ours_file.section_by_name("__text").unwrap().data().unwrap(),
        clang_file
            .section_by_name("__text")
            .unwrap()
            .data()
            .unwrap()
    );
    assert_eq!(records(&ours), records(&reference));
    let mut undefined = ours_file
        .symbols()
        .filter(|s| s.is_undefined())
        .map(|s| s.name().unwrap())
        .collect::<Vec<_>>();
    let mut clang_undefined = clang_file
        .symbols()
        .filter(|s| s.is_undefined())
        .map(|s| s.name().unwrap())
        .collect::<Vec<_>>();
    undefined.sort();
    clang_undefined.sort();
    assert_eq!(undefined, clang_undefined);
}

#[test]
fn all_instruction_relocation_kinds_match_clang() {
    compare_relocations(
        ".text\n.globl _entry\n_entry:\nbl _callee\nb _tail\nadrp x0, _data@PAGE\nadd x0, x0, _data@PAGEOFF\nldr x1, [x0, _data@PAGEOFF]\nstr w1, [x0, _data@PAGEOFF]\nadrp x2, _external@GOTPAGE\nldr x2, [x2, _external@GOTPAGEOFF]\nadrp x3, _tls@TLVPPAGE\nldr x3, [x3, _tls@TLVPPAGEOFF]\nret\n",
    );
}

#[test]
fn addend_pairs_and_access_sizes_match_clang() {
    let mut source = String::from(".text\n.globl _entry\n_entry:\n");
    for addend in [4, 16, 8388604] {
        source.push_str(&format!("bl _callee{addend:+}\nadrp x0, _data@PAGE{addend:+}\nadd x0, x0, _data@PAGEOFF{addend:+}\n"));
    }
    for (op, r) in [
        ("ldr", "x1"),
        ("str", "w1"),
        ("ldr", "b31"),
        ("str", "h31"),
        ("ldr", "s31"),
        ("str", "d31"),
        ("ldr", "q31"),
        ("ldrb", "w1"),
        ("strb", "w1"),
        ("ldrh", "w1"),
        ("strh", "w1"),
        ("ldrsb", "x1"),
        ("ldrsh", "w1"),
        ("ldrsw", "x1"),
    ] {
        source.push_str(&format!("{op} {r}, [sp, _data@PAGEOFF+16]\n"));
    }
    compare_relocations(&source);
    assert_eq!(
        assemble("add x0, x0, _data+8@PAGEOFF").unwrap().relocations,
        assemble("add x0, x0, _data@PAGEOFF + 8")
            .unwrap()
            .relocations
    );
}

#[test]
fn local_adr_and_numeric_address_displacements_match_clang() {
    compare_clang(
        "Lback: nop\nadr x0, Lforward\nadr xzr, Lback\nadr x0, Lforward+4\nadr x0, #-1048576\nadr x0, #1048575\nadrp x0, #-4294967296\nadrp xzr, #4294963200\nLforward: ret\n",
    );
    let a = assemble("adr x0, Lend\nnop\nLend: ret").unwrap();
    assert!(a.relocations.is_empty());
}

#[test]
fn defined_and_undefined_symbols_are_retained() {
    let a = assemble(
        ".globl _entry, _declared\n.extern _external\n_entry: bl _external\nb Lend\nLend: ret",
    )
    .unwrap();
    assert_eq!(
        a.symbols
            .iter()
            .find(|s| s.name == "_entry")
            .unwrap()
            .offset,
        Some(0)
    );
    for name in ["_declared", "_external"] {
        let symbol = a.symbols.iter().find(|s| s.name == name).unwrap();
        assert!(symbol.global);
        assert_eq!(symbol.offset, None);
    }
    assert_eq!(a.relocations.len(), 1);
    assert_eq!(a.relocations[0].kind, RelocationKind::Branch26);
    let bytes = a.to_macho().unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(
        file.symbols()
            .find(|s| s.name().unwrap() == "_external")
            .unwrap()
            .is_undefined()
    );
    assert_eq!(
        bytes,
        assemble_object(
            ".globl _entry, _declared\n.extern _external\n_entry: bl _external\nb Lend\nLend: ret"
        )
        .unwrap()
    );
}

#[test]
fn invalid_symbol_addressing_is_rejected() {
    for source in [
        "adr w0, #0",
        "adr sp, #0",
        "adr x0, _external",
        "adr x0, Lnone",
        "adr x0, #1048576",
        "adr x0, #-1048577",
        "adrp x0, #1",
        "adrp x0, #4294967296",
        "adrp x0, _data",
        "adrp x0, _data@PAGEOFF",
        "adrp x0, _data@GOTPAGE+8",
        "adrp x0, _data@TLVPPAGE-8",
        "add x0, x0, _data@GOTPAGEOFF",
        "add x0, x0, _data@PAGEOFF, lsl #12",
        "adds x0, x0, _data@PAGEOFF",
        "add x0, x0, _data@PAGE",
        "add x0, x0, _data@PAGEOFF+8388608",
        "add x0, x0, _data@PAGEOFF-8388609",
        "add x0, x0, _data@PAGEOFF+_other",
        "add x0, x0, _data@PAGEOFF@PAGE",
        "add x0, x0, _data@NOPE",
        "add x0, x0, _data _other@PAGEOFF",
        "ldr x0, [x1, _data@PAGE]",
        "ldr x0, [x1, _data@PAGEOFF]!",
        "ldr x0, [x1, _data@PAGEOFF], #8",
        "ldur x0, [x1, _data@PAGEOFF]",
        "str x0, [x1, _data@GOTPAGEOFF]",
        "ldr w0, [x1, _data@GOTPAGEOFF]",
        "ldr d0, [x1, _data@GOTPAGEOFF]",
        "ldr x0, [x1, _data@GOTPAGEOFF+8]",
        "ldr x0, [xzr, _data@PAGEOFF]",
        "b _callee+2",
        "bl _callee+8388608",
        "b _callee@PAGE",
        "b Lundefined",
        "b.eq _external",
        "cbz x0, _external",
        "adrp x0, Lundefined@PAGE",
    ] {
        assert!(assemble(source).is_err(), "{source}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn external_calls_page_addresses_and_got_link_and_execute() {
    use common::execute;
    let source = "
.text
.globl _direct, _get_data, _get_got, _tail, _with_addend, _local_address, _before
.globl _load_symbol, _store_symbol, _copy_simd, _get_abs, _libcall
_direct: stp x29, x30, [sp, #-16]!
    bl _answer
    ldp x29, x30, [sp], #16
    ret
_tail: b _answer
_get_data: adrp x0, _data@PAGE
    add x0, x0, _data@PAGEOFF
    ret
_get_got: adrp x0, _data@GOTPAGE
    ldr x0, [x0, _data@GOTPAGEOFF]
    ret
_with_addend: adrp x0, _data@PAGE+8
    add x0, x0, _data@PAGEOFF+8
    ldr x0, [x0]
    ret
_load_symbol: adrp x0, _data@PAGE
    ldr x0, [x0, _data@PAGEOFF+8]
    ret
_store_symbol: adrp x1, _data@PAGE
    str x0, [x1, _data@PAGEOFF+8]
    ret
_copy_simd: adrp x0, _data@PAGE
    ldr q0, [x0, _data@PAGEOFF]
    adrp x0, _copied@PAGE
    str q0, [x0, _copied@PAGEOFF]
    ret
_get_abs: adrp x0, _abs@GOTPAGE
    ldr x0, [x0, _abs@GOTPAGEOFF]
    ret
_libcall: b _abs
_before: adrp x0, _data@PAGE-8
    add x0, x0, _data@PAGEOFF-8
    ret
_local_address: adrp x0, Ltarget@PAGE
    add x0, x0, Ltarget@PAGEOFF
    ret
Ltarget: mov w0, #42
    ret
";
    let driver = "
#include <stdint.h>
#include <stdlib.h>
long copied[2] __attribute__((aligned(16)));
long data[2] __attribute__((aligned(4096))) = {41, 42};
long answer(void) { return 42; }
extern long direct(void), tail(void), with_addend(void), load_symbol(void);
extern void store_symbol(long), copy_simd(void);
extern int (*get_abs(void))(int);
extern int libcall(int);
extern long *get_data(void), *get_got(void), *before(void);
extern long (*local_address(void))(void);
int main(void) {
    if (direct() != 42 || tail() != 42) return 1;
    if (get_data() != data || get_got() != data || with_addend() != 42) return 2;
    if (local_address()() != 42) return 3;
    if ((uintptr_t)before() != (uintptr_t)data - 8) return 4;
    if (load_symbol() != 42) return 5;
    store_symbol(43);
    if (data[1] != 43 || with_addend() != 43) return 6;
    copy_simd();
    if (copied[0] != data[0] || copied[1] != data[1]) return 7;
    if (get_abs() != abs || get_abs()(-42) != 42 || libcall(-42) != 42) return 8;
    return 0;
}
";
    let temp = Temp::new();
    let obj = temp.0.join("addresses.o");
    let c = temp.0.join("driver.c");
    let exe = temp.0.join("addresses");
    std::fs::write(&obj, assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang")
        .args(["-std=c11", "-O2"])
        .arg(&c)
        .arg(&obj)
        .arg("-o")
        .arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn tlvp_addresses_link_and_execute() {
    use common::execute;
    let source = ".text\n.globl _get_tls\n_get_tls:\nstp x29, x30, [sp, #-16]!\nadrp x0, _value@TLVPPAGE\nldr x0, [x0, _value@TLVPPAGEOFF]\nldr x16, [x0]\nblr x16\nldp x29, x30, [sp], #16\nret\n";
    compare_relocations(source);
    let driver = "__thread int value = 42;\nextern int *get_tls(void);\nint main(void) { int *p = get_tls(); if (p != &value || *p != 42) return 1; *p = 43; return value != 43; }\n";
    let temp = Temp::new();
    let obj = temp.0.join("tls.o");
    let c = temp.0.join("driver.c");
    let exe = temp.0.join("tls");
    std::fs::write(&obj, assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang").arg(&c).arg(&obj).arg("-o").arg(&exe));
    assert_eq!(execute(&exe).code(), Some(0));
}

#[test]
fn negative_addends_have_correct_signed_values_and_flags() {
    // Some Apple clang releases OR a sign-extended addend into the complete
    // record word. Validate the specified 24-bit payload independently instead.
    for value in [-8388608, -16, -4] {
        let source = format!(
            "bl _callee{value:+}\nadrp x0, _data@PAGE{value:+}\nadd x0, x0, _data@PAGEOFF{value:+}\n"
        );
        let bytes = assemble_object(&source).unwrap();
        let expected = [
            Record {
                offset: 8,
                symbol: "_data".into(),
                addend: value,
                kind: 4,
                pcrel: false,
                length: 2,
            },
            Record {
                offset: 4,
                symbol: "_data".into(),
                addend: value,
                kind: 3,
                pcrel: true,
                length: 2,
            },
            Record {
                offset: 0,
                symbol: "_callee".into(),
                addend: value,
                kind: 2,
                pcrel: true,
                length: 2,
            },
        ];
        assert_eq!(records(&bytes), expected);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn two_keel_objects_link_with_negative_branch_addend() {
    use common::execute;
    let caller = ".text\n.globl _main\n_main:\nstp x29, x30, [sp, #-16]!\nbl _answer_end-4\nldp x29, x30, [sp], #16\nret\n";
    let callee = ".text\n.globl _answer_end\nnop\n_answer_end: mov w0, #42\nret\n";
    let temp = Temp::new();
    let main = temp.0.join("main.o");
    let answer = temp.0.join("answer.o");
    let exe = temp.0.join("answer");
    std::fs::write(&main, assemble_object(caller).unwrap()).unwrap();
    std::fs::write(&answer, assemble_object(callee).unwrap()).unwrap();
    run(Command::new("clang")
        .arg(&main)
        .arg(&answer)
        .arg("-o")
        .arg(&exe));
    assert_eq!(execute(&exe).code(), Some(42));
}

#[test]
fn malformed_public_relocation_data_is_rejected() {
    let baseline = assemble("bl _answer").unwrap();
    let mut a = baseline.clone();
    a.relocations[0].offset = 4;
    assert!(a.to_macho().is_err());
    a = baseline.clone();
    a.relocations[0].symbol = "_missing".into();
    assert!(a.to_macho().is_err());
    a = baseline.clone();
    a.relocations[0].addend = 1;
    assert!(a.to_macho().is_err());
    a = baseline.clone();
    a.relocations.push(a.relocations[0].clone());
    assert!(a.to_macho().is_err());
    a = baseline;
    a.symbols[0].global = false;
    assert!(a.to_macho().is_err());
    let mut a = assemble("adrp x0, _data@GOTPAGE").unwrap();
    a.relocations[0].addend = 8;
    assert!(a.to_macho().is_err());
    for source in [
        "adrp x0, Lundefined@PAGE",
        "ldr x0, [x1, Lundefined@PAGEOFF]",
    ] {
        assert_eq!(assemble(&format!("nop\n{source}")).unwrap_err().line, 2);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn keel_generated_calls_and_symbol_addresses_link_and_execute() {
    use common::execute;
    let source = "decl @answer() -> i32\ndecl data @data\nfunc @main() -> i32 {\nblock0:\n v0 = call @answer()\n v1 = symbol_addr @data\n v2 = load.i32 v1\n v3 = iadd v0, v2\n return v3\n}\n";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let source = keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    // Isolate instruction relocations; unwind records have separate native tests.
    let source: String = source
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with(".cfi_")
        })
        .map(|line| format!("{line}\n"))
        .collect();
    compare_relocations(&source);
    let temp = Temp::new();
    let obj = temp.0.join("keel.o");
    let c = temp.0.join("driver.c");
    let exe = temp.0.join("keel");
    std::fs::write(&obj, assemble_object(&source).unwrap()).unwrap();
    std::fs::write(&c, "int data = 2; int answer(void) {return 40;}\n").unwrap();
    run(Command::new("clang").arg(&c).arg(&obj).arg("-o").arg(&exe));
    assert_eq!(execute(&exe).code(), Some(42));
}
