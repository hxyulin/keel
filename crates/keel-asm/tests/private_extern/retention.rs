#![cfg(target_os = "macos")]
use crate::common::run;
use keel_asm_ir::Directive as D;
use object::{Object, ObjectSymbol};
use std::{fs, path::Path, process::Command};
#[test]
fn macho_symbol_retention_matches_clang_and_preserves_weak_binding() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("asm-retain-macho");
    fs::create_dir_all(&dir).unwrap();
    let source = ".section __TEXT,__custom,regular,pure_instructions\n.globl _weak\n.weak_definition _weak\n.no_dead_strip _weak\n_weak: ret\n.no_dead_strip _local\n_local: ret\n.subsections_via_symbols\n";
    let input = dir.join("oracle.s");
    let output = dir.join("oracle.o");
    fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&output));
    let oracle = fs::read(&output).unwrap();
    let actual = keel_asm::assemble_object(source).unwrap();
    let a = object::File::parse(actual.as_slice()).unwrap();
    let b = object::File::parse(oracle.as_slice()).unwrap();
    for name in ["_weak", "_local"] {
        let a = a.symbols().find(|s| s.name().unwrap() == name).unwrap();
        let b = b.symbols().find(|s| s.name().unwrap() == name).unwrap();
        assert_eq!(a.flags(), b.flags());
        assert_eq!(a.is_global(), b.is_global());
        assert_eq!(a.is_weak(), b.is_weak());
    }
    assert!(D::NoDeadStrip("bad\nname".into()).validate().is_err());
}
