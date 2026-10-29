//! Accepted hand-written input must mean what Clang's integrated assembler
//! makes of it: same bytes, section attributes and symbol bindings.
use crate::common::Temp;
use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection};
use std::process::Command;

const X86: &str = "x86_64-unknown-linux-gnu";

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
