//! Mach-O visibility must survive both textual and structured assembly.
#![allow(dead_code)]
mod common;
use common::{Temp, run};
use object::{Object, ObjectSymbol};
use std::{collections::BTreeMap, process::Command};

fn symbols(bytes: &[u8]) -> BTreeMap<String, (object::SymbolScope, bool, u64)> {
    object::File::parse(bytes)
        .unwrap()
        .symbols()
        .filter(|s| !s.name().unwrap().starts_with("ltmp"))
        .map(|s| {
            (
                s.name().unwrap().to_owned(),
                (s.scope(), s.is_undefined(), s.address()),
            )
        })
        .collect()
}

#[test]
fn private_extern_matches_clang_for_definitions_references_and_commons() {
    for source in [
        ".private_extern _hidden\n_hidden: ret\n",
        ".private_extern _hidden\n.globl _hidden\n_hidden: ret\n",
        ".globl _hidden\n_hidden: ret\n.private_extern _hidden\n",
        ".private_extern _hidden\n.extern _hidden\n_hidden: ret\n",
        ".private_extern _missing\n",
        ".private_extern _common\n.comm _common,8,3\n",
        ".comm _common,8,3\n.private_extern _common\n",
        ".private_extern _a, _b\n_a: bl _b\n_b: ret\n.subsections_via_symbols\n",
    ] {
        let temp = Temp::new();
        let input = temp.0.join("input.s");
        let output = temp.0.join("clang.o");
        std::fs::write(&input, source).unwrap();
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&output));
        let ours = keel_asm::assemble_object(source).unwrap();
        let reference = std::fs::read(output).unwrap();
        assert_eq!(symbols(&ours), symbols(&reference), "{source}");
        for symbol in keel_asm::assemble(source).unwrap().symbols {
            assert!(symbol.private_extern && symbol.global);
        }
    }
    for source in [
        ".private_extern",
        ".private_extern Llocal\nLlocal: ret",
        ".private_extern 123",
    ] {
        assert!(keel_asm::assemble(source).is_err(), "{source}");
    }
}

#[test]
fn structured_private_extern_matches_text_and_rejects_elf() {
    use keel_asm_ir::{Directive, Item, Program};
    let mut p = Program::default();
    p.items
        .push(Item::Directive(Directive::PrivateExtern("_hidden".into())));
    p.items.push(Item::Text("_hidden: ret".into()));
    let object =
        keel_asm::assemble_program_object(&p, keel_asm::TargetFeatures::default()).unwrap();
    assert_eq!(
        object,
        keel_asm::assemble_object(".private_extern _hidden\n_hidden: ret").unwrap()
    );
    assert!(keel_asm::x86::assemble_program_object(&p).is_err());
    assert!(
        Directive::PrivateExtern("Llocal".into())
            .validate()
            .is_err()
    );
}
