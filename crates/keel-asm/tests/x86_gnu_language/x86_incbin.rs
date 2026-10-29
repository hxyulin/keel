use crate::common;
use keel_asm::x86::{self, Features, ProgramAssembler, Syntax};
use keel_asm_ir::x86::SourceParser;
use object::{Object, ObjectSection, ObjectSymbol};
use std::{collections::BTreeMap, fs, process::Command};

fn section(bytes: &[u8], name: &str) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(name)
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn binary_inputs_match_gnu_and_are_snapshotted_across_layout_and_chunks() {
    let temp = common::Temp::new();
    let payload: Vec<u8> = (0..=255).collect();
    fs::write(temp.0.join("payload.bin"), &payload).unwrap();
    let chunks = [
        ".text\nstart: jmp end\n.incbin \"payload.bin\"\n",
        "end: ret\n.section .rodata\n.set offset,1\n.rept 2\n.incbin \"payload.bin\",offset,2\n.set offset,offset+1\n.endr\n",
        ".incbin \"payload.bin\",254,0\n.incbin \"payload.bin\",,1\nafter:\n",
    ];
    let source = chunks.concat();
    let mut calls = 0;
    let mut resolver = |name: &str| {
        calls += 1;
        assert_eq!(name, "payload.bin");
        Ok(payload.clone())
    };
    let ours = x86::assemble_object_with_binary_inputs(
        &source,
        Syntax::Att,
        Features::NONE,
        &mut resolver,
    )
    .unwrap();
    assert_eq!(
        calls, 1,
        "repeated inputs and layout passes reuse the snapshot"
    );
    let program = SourceParser::new(Syntax::Att).parse(&source).unwrap();
    let mut resolver = |_: &str| Ok(payload.clone());
    assert_eq!(
        ours,
        x86::assemble_program_object_with_binary_inputs(&program, Features::NONE, &mut resolver)
            .unwrap()
    );
    let mut assembler = ProgramAssembler::new(Features::NONE);
    let mut calls = 0;
    let mut resolver = |_: &str| {
        calls += 1;
        Ok(payload.clone())
    };
    for chunk in chunks {
        let program = SourceParser::new(Syntax::Att).parse(chunk).unwrap();
        assembler.push_with_binary_inputs(program, &mut resolver);
    }
    assert_eq!(ours, assembler.finish().unwrap());
    assert_eq!(calls, 1, "chunks share one snapshot per name");
    assert_eq!(section(&ours, ".rodata"), [1, 2, 2, 3, 254, 255, 0]);
    fs::write(temp.0.join("input.s"), source).unwrap();
    common::run(
        Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()))
            .current_dir(&temp.0)
            .args(["--64", "input.s", "-o", "gnu.o"]),
    );
    let reference = fs::read(temp.0.join("gnu.o")).unwrap();
    for name in [".text", ".rodata"] {
        assert_eq!(section(&ours, name), section(&reference, name), "{name}");
    }
    let symbols = |bytes: &[u8]| {
        object::File::parse(bytes)
            .unwrap()
            .symbols()
            .filter_map(|s| {
                let name = s.name().ok()?;
                ["start", "end", "after"]
                    .contains(&name)
                    .then(|| (name.to_owned(), s.address()))
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(symbols(&ours), symbols(&reference));
}

#[test]
fn input_access_and_ranges_are_explicit() {
    let object = x86::assemble_object(".if 0\n.incbin \"unavailable\"\n.endif\n.byte 7").unwrap();
    assert_eq!(section(&object, ".text"), [7]);
    let error = x86::assemble_object(".text\n.incbin \"payload.bin\"")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("line 2") && error.contains("explicit binary-input resolver"),
        "{error}"
    );
    for directive in [
        ".incbin unquoted",
        ".incbin \"x\",-1",
        ".incbin \"x\",5",
        ".incbin \"x\",1,4",
        ".incbin \"x\",0,-1",
        ".incbin \"x\",0,1,2",
        ".incbin \"x\",unknown",
        ".incbin \"x\\000y\"",
    ] {
        let mut resolver = |_: &str| Ok(vec![1, 2, 3, 4]);
        assert!(
            x86::assemble_object_with_binary_inputs(
                directive,
                Syntax::Att,
                Features::NONE,
                &mut resolver
            )
            .is_err(),
            "{directive}"
        );
    }
    let mut resolver = |_: &str| Err("missing input".into());
    let error = x86::assemble_object_with_binary_inputs(
        ".incbin \"missing\",0,0",
        Syntax::Att,
        Features::NONE,
        &mut resolver,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("missing input"), "{error}");
}
