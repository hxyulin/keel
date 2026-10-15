use keel_asm::{assemble, assemble_with_metrics};

#[test]
fn instruction_locations_exclude_text_padding_data_and_other_sections() {
    let source =
        ".text\nstart: mov x0, #7\n.p2align 4\n.long 0\nadd x0, x0, #1\nb start\n.data\n.long 42\n";
    let (assembly, instructions) = assemble_with_metrics(source).unwrap();
    assert_eq!(assembly, assemble(source).unwrap());
    assert_eq!(
        instructions
            .iter()
            .map(|i| (i.line, i.offset))
            .collect::<Vec<_>>(),
        [(2, 0), (5, 20), (6, 24)]
    );
    assert_eq!(assembly.text.len(), 28);
    // Final branch fixup, not the placeholder captured during parsing.
    assert_eq!(
        u32::from_le_bytes(assembly.text[24..28].try_into().unwrap()),
        0x17fffffa
    );
}

#[test]
fn collecting_metrics_preserves_all_example_objects() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "s") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let (assembly, instructions) = assemble_with_metrics(&source).unwrap();
        assert_eq!(
            assembly.to_macho().unwrap(),
            assemble(&source).unwrap().to_macho().unwrap(),
            "{}",
            path.display()
        );
        assert!(!instructions.is_empty());
        for location in instructions {
            assert!(location.offset.is_multiple_of(4));
            assert!(location.offset + 4 <= assembly.text.len());
        }
    }
}
