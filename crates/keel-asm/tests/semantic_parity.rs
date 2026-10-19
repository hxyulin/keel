//! Accepted hand-written input must mean what Clang's integrated assembler
//! makes of it: same bytes, section attributes and symbol bindings.
#![allow(dead_code)]
mod common;

use common::Temp;
use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

const X86: &str = "x86_64-unknown-linux-gnu";
const A64: &str = "arm64-apple-macos11";

/// Clang's object for `source`, or its diagnostics when it rejects it.
fn clang(target: &str, source: &str) -> Result<Vec<u8>, String> {
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    let result = Command::new("clang")
        .args(["-target", target, "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("clang is required for assembler differential tests");
    if result.status.success() {
        Ok(std::fs::read(output).unwrap())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}

fn section_data(bytes: &[u8], name: &str) -> Vec<u8> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(name)
        .map_or_else(Vec::new, |s| s.data().unwrap().to_vec())
}

/// Accept/reject parity, and identical bytes for accepted sources.
fn compare_x86(source: &str, sections: &[&str]) {
    let ours = assemble_object(source);
    match (ours, clang(X86, source)) {
        (Ok(ours), Ok(reference)) => {
            for name in sections {
                assert_eq!(
                    section_data(&ours, name),
                    section_data(&reference, name),
                    "{name} differs from clang for:\n{source}"
                );
            }
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => panic!("clang rejects but keel accepts:\n{source}\n{e}"),
        (Err(e), Ok(_)) => panic!("keel rejects but clang accepts:\n{source}\n{e}"),
    }
}

#[test]
fn x86_data_literal_ranges_match_clang() {
    for (directive, bits) in [(".byte", 8), (".short", 16), (".long", 32), (".quad", 64)] {
        let edges: [i128; 6] = [
            -(1 << (bits - 1)),
            -(1 << (bits - 1)) - 1,
            -1,
            (1 << bits) - 1,
            1 << bits,
            1 << (bits - 1),
        ];
        for value in edges {
            // Clang rejects literals above u64::MAX outright, and wraps
            // values below -2^63 (see the test below).
            if value > i128::from(u64::MAX) || value < i128::from(i64::MIN) {
                continue;
            }
            compare_x86(&format!(".data\n{directive} {value}"), &[".data"]);
        }
    }
    for source in [
        ".data\n.byte 300",
        ".data\n.byte 0x100",
        ".data\n.byte 255+1",
        ".data\n.byte 1, 256",
        ".data\n.byte 0x8000000000000000-0x7fffffffffffff01",
        ".data\n.word 65535",
        ".data\n.int -1",
        ".data\n.quad 0xffffffffffffffff",
        ".data\n.quad 18446744073709551616",
        ".data\n.quad 0x10000000000000000",
        ".data\n.quad 0x10000000000000000-1",
        // A resolved absolute symbol shares the same field range check.
        ".set big, 300\n.data\n.byte big",
        ".set ok, 255\n.data\n.byte ok",
    ] {
        compare_x86(source, &[".data"]);
    }
}

/// LLVM evaluates in wrapping 64-bit arithmetic before its range check, so it
/// accepts these as a different number than written. Keel rejects them instead.
#[test]
fn x86_data_literals_that_llvm_wraps_are_rejected() {
    for source in [
        ".data\n.byte 0xffffffffffffffff",
        ".data\n.byte -0xffffffffffffffff",
        ".data\n.quad -0x8000000000000001",
        ".data\n.quad 0x7fffffffffffffff+0x8000000000000001",
    ] {
        assert!(clang(X86, source).is_ok(), "clang now rejects:\n{source}");
        let error = assemble_object(source).unwrap_err().to_string();
        assert!(error.contains("out of range literal"), "{source}: {error}");
    }
}

#[test]
fn x86_leading_zero_literals_are_octal_like_clang() {
    compare_x86(
        r#".intel_syntax noprefix
.text
mov eax, 010
mov eax, [rax + 010]
add eax, 0x10
mov eax, 0
mov eax, 00
mov rax, -017
.byte 010, 0, 07
.long 010-1
.zero 010
.uleb128 010
.sleb128 -010
.set x, 010
.long x
.data
.byte 1
.p2align 03
.byte 2
.balign 010
.quad 0777777777777777777777, 01777777777777777777777, -010
"#,
        &[".text", ".data"],
    );
    for source in [
        ".data\n.byte 08",
        ".data\n.byte 09",
        ".data\n.quad 02000000000000000000000",
        ".intel_syntax noprefix\nmov eax, 08",
        ".data\n.byte 0x+5",
    ] {
        compare_x86(source, &[".data", ".text"]);
    }
}

/// `(sh_type, sh_flags)` by name, from the raw ELF64 section headers.
fn section_headers(bytes: &[u8]) -> Vec<(String, u32, u64)> {
    let file = object::File::parse(bytes).unwrap();
    let shoff = u64::from_le_bytes(bytes[0x28..0x30].try_into().unwrap()) as usize;
    file.sections()
        .map(|s| {
            let at = shoff + s.index().0 * 64;
            (
                s.name().unwrap().to_owned(),
                u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()),
                u64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}

#[test]
fn x86_inferred_section_attributes_match_clang() {
    let source = r#"
.section .text.foo
.section .text.hot.x
.section .text.unlikely.
.section .textfoo
.section .init
.section .fini
.section .rodata.x
.section .rodata1
.section .rodatax
.section .data.rel.ro.x
.section .data.x
.section .data1
.section .bss.x
.section .bss.rel.ro
.section .bssx
.section .tdata.x
.section .tbss.x
.section .tbssx
.section .comment
.section .foo
.section .debug_foo
.section .bss.a,"aw"
.section .bss.b,"aw",@progbits
.section .text.c,""
.section .text.d,"a"
.section .rodata.e,"aw"
.section .tbss.f,"awT"
.section .data.g,"",@nobits
.section .bss.h,"",@progbits
.section .foo2,"aw",@nobits
.section .text.x
.section .text.x,"ax",@progbits
.section .data.y,"aw"
.section .data.y
.section .bss.z
.section .bss.z,"aw",@nobits
.section .text.w,"aw"
.section .note.GNU-stack,"",@progbits
"#;
    let names: Vec<_> = source
        .lines()
        .filter_map(|l| l.strip_prefix(".section "))
        .map(|l| l.split(',').next().unwrap())
        .collect();
    let pick = |bytes: &[u8]| {
        let mut headers: Vec<_> = section_headers(bytes)
            .into_iter()
            .filter(|(name, ..)| names.contains(&name.as_str()))
            .collect();
        headers.sort();
        headers
    };
    let ours = pick(&assemble_object(source).unwrap());
    let reference = pick(&clang(X86, source).unwrap());
    assert_eq!(ours, reference);
    assert!(ours.contains(&(".text.foo".into(), 1, 6)));
    assert!(ours.contains(&(".bss.x".into(), 8, 3)));
}

/// Name, external and undefined bits, ignoring Clang's `ltmp` section symbols.
fn a64_bindings(bytes: &[u8]) -> Vec<(String, bool, bool)> {
    let file = object::File::parse(bytes).unwrap();
    let mut symbols: Vec<_> = file
        .symbols()
        .filter(|s| !s.name().unwrap().starts_with("ltmp"))
        .map(|s| {
            (
                s.name().unwrap().to_owned(),
                s.is_global(),
                s.is_undefined(),
            )
        })
        .collect();
    symbols.sort();
    symbols
}

#[test]
fn a64_extern_keeps_declared_binding_like_clang() {
    for source in [
        ".extern _foo\n_foo: ret",
        ".extern _foo\n.text\nret",
        ".extern _foo\nbl _foo",
        ".globl _foo\n.extern _foo\n_foo: ret",
        ".extern _foo\n.globl _foo\n_foo: ret",
        ".extern _a, _b\n_a: ret\nbl _b",
        ".extern _a\n.globl _b\n_a: ret\n_b: bl _a",
    ] {
        let ours = keel_asm::assemble(source)
            .and_then(|a| a.to_macho())
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        let reference = clang(A64, source).unwrap();
        assert_eq!(a64_bindings(&ours), a64_bindings(&reference), "{source}");
    }
    // Keel only accepts the meaningful spelling; Clang ignores the statement.
    assert!(keel_asm::assemble(".extern").is_err());
    assert!(keel_asm::assemble(".extern Lfoo\nbl Lfoo").is_err());
}
