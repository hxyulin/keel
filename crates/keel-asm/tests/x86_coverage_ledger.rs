//! Cross-check generated structural claims against the public Rust tables/APIs.
//! This does not claim per-form assembly, decoding or execution qualification.
use keel_asm_ir::x86::{Opcode, isa};
use keel_ir::Intrinsic;
use serde_json::Value;
use std::collections::HashSet;

fn ledger() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/targets/x86_64/coverage/coverage.json"
    ))
    .unwrap()
}

#[test]
fn ledger_locators_and_decode_registration_match_the_actual_catalogue() {
    let ledger = ledger();
    let source: Value =
        serde_json::from_str(include_str!("../../../tools/x86-isa/zydis.json")).unwrap();
    let local: Value =
        serde_json::from_str(include_str!("../../../tools/x86-isa/keel-extensions.json")).unwrap();
    let mut visited = HashSet::new();
    for row in ledger["forms"].as_array().unwrap() {
        let Some(index) = row["table_index"].as_u64() else {
            assert_eq!(row["decoder"], "excluded");
            continue;
        };
        let index = index as usize;
        assert!(visited.insert(index));
        let form = &isa::FORMS[index];
        let source = if row["source"] == "zydis" {
            &source
        } else {
            &local
        };
        let raw = &source[row["source_index"].as_u64().unwrap() as usize];
        assert_eq!(row["mnemonic"], form.mnemonic);
        assert_eq!(raw["mnemonic"], form.mnemonic);
        assert_eq!(row["isa_set"], form.isa);
        assert_eq!(form.vex, row["encoding"] == "vex");
        assert_eq!(
            form.byte,
            u8::from_str_radix(raw["opcode"].as_str().unwrap(), 16).unwrap()
        );
        assert_eq!(
            form.map,
            match raw["opcode_map"].as_str() {
                None => 0,
                Some("0f") => 1,
                Some("0f38") => 2,
                Some("0f3a") => 3,
                _ => panic!("unexpected opcode map"),
            }
        );
        assert_eq!(row["cpl"].as_u64().map(|n| n as u8), form.cpl);
        assert_eq!(row["decoder"] == "registered", form.decode);
        assert!(isa::decode_forms(form.vex, form.map, form.byte).any(|f| std::ptr::eq(f, form)));
        let visible: Vec<_> = raw["operands"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["visible"].as_bool().unwrap_or(true))
            .collect();
        assert_eq!(visible.len(), form.operands.len());
        for (raw, operand) in visible.iter().zip(form.operands) {
            assert_eq!(raw["operand_type"], operand.kind);
            assert_eq!(raw["encoding"].as_str().unwrap_or(""), operand.encoding);
            assert_eq!(raw["register"].as_str().unwrap_or(""), operand.register);
            for (index, size) in [16, 32, 64].into_iter().enumerate() {
                assert_eq!(
                    raw[format!("width{size}")].as_u64().unwrap_or(0),
                    u64::from(operand.widths[index])
                );
            }
        }
    }
    assert_eq!(visited.len(), isa::FORMS.len());
    assert_eq!(
        ledger["summary"]["catalogue"].as_u64().unwrap() as usize,
        isa::FORMS.len()
    );
}

#[test]
fn recorded_static_spellings_and_tracked_compiler_names_exist() {
    let ledger = ledger();
    for binding in ledger["assembler_bindings"].as_array().unwrap() {
        for spelling in binding["spellings"].as_array().unwrap() {
            let name = spelling.as_str().unwrap();
            assert!(Opcode::parse(name).is_some(), "{}: {name}", binding["id"]);
        }
    }
    for binding in ledger["compiler_bindings"].as_array().unwrap() {
        let name = binding["name"].as_str().unwrap();
        let intrinsic = Intrinsic::from_name(name).unwrap();
        assert_eq!(intrinsic as u64, binding["id"].as_u64().unwrap());
        assert!(intrinsic.is_x86_target());
        assert!(!intrinsic.info().pure);
        assert_eq!(binding["validation"], "untracked");
    }
}

#[test]
fn avx512_table_and_public_bindings_match_the_separate_ledger() {
    use keel_asm_ir::x86::EvexOpcode;
    let ledger = ledger();
    let rows = ledger["avx512_forms"].as_array().unwrap();
    let source: Value =
        serde_json::from_str(include_str!("../../../tools/x86-isa/evex.json")).unwrap();
    assert_eq!(rows.len(), isa::EVEX_FORMS.len());
    for row in rows {
        let f = &isa::EVEX_FORMS[row["table_index"].as_u64().unwrap() as usize];
        let raw = &source[row["source_index"].as_u64().unwrap() as usize];
        assert_eq!(raw["mnemonic"], f.form.mnemonic);
        assert_eq!(row["mnemonic"], f.form.mnemonic);
        assert_eq!(row["isa_set"], f.form.isa);
        assert_eq!(f.form.vex, row["encoding"] == "vex");
        let required: Vec<_> = f.requirements().names().collect();
        let expected: Vec<_> = row["requires"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert_eq!(required, expected);
        let op = EvexOpcode::parse(f.form.mnemonic).unwrap();
        assert_eq!(row["binding"], format!("EvexMnemonic::{:?}", op.kind));
        if !f.form.vex {
            assert!(
                isa::evex_decode_forms(f.form.map, f.form.byte)
                    .any(|candidate| std::ptr::eq(candidate, f))
            );
        }
        assert_eq!(f.tuple, raw["evex"]["tuple_type"].as_str().unwrap_or(""));
        assert_eq!(row["validation"], "untracked");
    }
}
