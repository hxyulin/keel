//! Mach-O visibility must survive both textual and structured assembly.
#![allow(dead_code)]
mod common;
#[path = "private_extern/retention.rs"]
mod retention;
#[path = "private_extern/semantic_parity.rs"]
mod semantic_parity;
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
    let cases = [
        ".private_extern _hidden\n_hidden: ret\n",
        ".private_extern _hidden\n.globl _hidden\n_hidden: ret\n",
        ".globl _hidden\n_hidden: ret\n.private_extern _hidden\n",
        ".private_extern _hidden\n.extern _hidden\n_hidden: ret\n",
        ".private_extern _missing\n",
        ".private_extern _common\n.comm _common,8,3\n",
        ".comm _common,8,3\n.private_extern _common\n",
    ];
    let subsections = ".private_extern _a, _b\n_a: bl _b\n_b: ret\n.subsections_via_symbols\n";
    for source in cases.iter().chain([&subsections]) {
        for symbol in keel_asm::assemble(source).unwrap().symbols {
            assert!(symbol.private_extern && symbol.global, "{source}");
        }
    }
    // One source holds every case with its own symbol names, and the
    // comparison is per symbol. Subsections change the whole file, so that
    // case runs alone.
    let renamed = cases
        .iter()
        .enumerate()
        .map(|(n, case)| {
            ["_hidden", "_missing", "_common"]
                .iter()
                .fold(case.to_string(), |s, name| {
                    s.replace(name, &format!("{name}_{n}"))
                })
        })
        .collect::<Vec<_>>();
    for (source, cases) in [
        (renamed.concat(), renamed),
        (subsections.into(), vec![subsections.into()]),
    ] {
        let temp = Temp::new();
        let input = temp.0.join("input.s");
        let output = temp.0.join("clang.o");
        std::fs::write(&input, &source).unwrap();
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&output));
        let ours = symbols(&keel_asm::assemble_object(&source).unwrap());
        let reference = symbols(&std::fs::read(output).unwrap());
        for name in ours.keys().chain(reference.keys()) {
            let case = cases.iter().find(|c| c.contains(name.as_str())).unwrap();
            assert_eq!(ours.get(name), reference.get(name), "{name} in:\n{case}");
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
