//! Accepted hand-written input must mean what Clang's integrated assembler
//! makes of it: same bytes, section attributes and symbol bindings.
use crate::common::Temp;
use object::{Object, ObjectSymbol};
use std::process::Command;

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
    // One source holds every case with its own symbol names; the bindings
    // are compared per symbol.
    let cases = [
        ".extern _foo\n_foo: ret",
        ".extern _foo\n.text\nret",
        ".extern _foo\nbl _foo",
        ".globl _foo\n.extern _foo\n_foo: ret",
        ".extern _foo\n.globl _foo\n_foo: ret",
        ".extern _a, _b\n_a: ret\nbl _b",
        ".extern _a\n.globl _b\n_a: ret\n_b: bl _a",
    ]
    .iter()
    .enumerate()
    .map(|(n, case)| {
        ["_foo", "_a", "_b"]
            .iter()
            .fold(case.to_string(), |s, name| {
                s.replace(name, &format!("{name}_{n}"))
            })
    })
    .collect::<Vec<_>>();
    let source = cases.join("\n") + "\n";
    let ours = keel_asm::assemble(&source)
        .and_then(|a| a.to_macho())
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    let ours = a64_bindings(&ours);
    let reference = a64_bindings(&clang(A64, &source).unwrap());
    let names = ours.iter().chain(&reference).map(|(name, ..)| name);
    for name in names {
        let case = cases.iter().find(|c| c.contains(name.as_str())).unwrap();
        let binding =
            |bindings: &[(String, bool, bool)]| bindings.iter().find(|(n, ..)| n == name).cloned();
        assert_eq!(binding(&ours), binding(&reference), "{name} in:\n{case}");
    }
    // Keel only accepts the meaningful spelling; Clang ignores the statement.
    assert!(keel_asm::assemble(".extern").is_err());
    assert!(keel_asm::assemble(".extern Lfoo\nbl Lfoo").is_err());
}
