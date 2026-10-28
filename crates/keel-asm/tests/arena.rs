//! Ownership and isolation regressions for phase-scoped assembler scratch.
use keel_asm::{assemble, assemble_object};

fn source() -> String {
    let mut source = String::from(
        ".subsections_via_symbols\n.file 1 \"arena λ.c\"\n.text\n.globl _entry\n_entry:\n.cfi_startproc\n.cfi_def_cfa sp, 16\n.loc 1 1 0\nb Ldone\nLdone: ret\n.cfi_endproc\n.data\nLstart:\n.uleb128 Lend-Lstart\n.zero 127\nLend:\n.sleb128 Lstart-Lend\nLptr: .long _type@GOT-Lptr\n.quad _entry\n.asciz \"λ中; // payload\"\n.byte ",
    );
    // Exercise operand overflow together with arena-backed fixups and debug.
    source.push_str(&["7"; 100].join(","));
    source.push('\n');
    source
}

#[test]
fn returned_assembly_object_and_errors_outlive_source_and_scratch() {
    let assembly = assemble(&source()).unwrap();
    let retained = assemble_object(&source()).unwrap();
    let error = assemble(&String::from(
        ".text\n.cfi_startproc\n.cfi_def_cfa sp, 16\nb Lmissing\n",
    ))
    .unwrap_err();
    // Repeated arena destruction must not invalidate any retained result.
    for _ in 0..20 {
        assert_eq!(assemble_object(&source()).unwrap(), retained);
    }
    assert_eq!(assembly.to_macho().unwrap(), retained);
    assert_eq!(error.line, 4);
    assert_eq!(error.message, "undefined local branch target `Lmissing`");
    let file = object::File::parse(retained.as_slice()).unwrap();
    use object::{Object, ObjectSection};
    assert!(file.section_by_name("__debug_line").is_some());
    assert!(file.section_by_name("__eh_frame").is_some());
    assert!(
        file.section_by_name("__data")
            .unwrap()
            .data()
            .unwrap()
            .windows("λ中; // payload\0".len())
            .any(|w| w == "λ中; // payload\0".as_bytes())
    );
}

#[test]
fn independent_calls_and_errors_are_safe_on_multiple_threads() {
    let expected = assemble_object(&source()).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let expected = &expected;
            scope.spawn(move || {
                for _ in 0..12 {
                    assert_eq!(assemble_object(&source()).unwrap(), *expected);
                    for (text, line) in [
                        (".text\nLx: ret\nLx: ret", 3),
                        (".text\n.cfi_startproc\n.cfi_def_cfa sp, 16", 2),
                        (".data\n.uleb128 Missing-Lmissing", 2),
                        (".data\nLx: .quad _foo\n.byte 1,2,3,4,5,6,7,8,", 3),
                    ] {
                        assert_eq!(assemble(text).unwrap_err().line, line);
                    }
                }
            });
        }
    });
}

#[test]
fn leb_passes_rebuild_scratch_and_preserve_convergence_diagnostic() {
    let source = ".data\nLstart:\n.uleb128 Lend-Lstart\n.zero 127\nLend:\n.sleb128 Lstart-Lend\n";
    let assembly = assemble(source).unwrap();
    let data = assembly.sections[0].data();
    assert_eq!(&data[..2], &[0x81, 1]);
    assert_eq!(&data[129..], &[0xff, 0x7e]);
    // Width oscillates between 1 and 2 bytes (128 versus 127).
    let error = assemble(".data\nLbegin:\n.uleb128 129-Lend+Lbegin\nLend:").unwrap_err();
    assert_eq!(error.line, 0);
    assert_eq!(
        error.message,
        "symbolic LEB128 layout did not converge after 32 passes"
    );
    assert_eq!(assemble(source).unwrap(), assembly);
}
