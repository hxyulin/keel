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
fn leb_passes_rebuild_scratch_and_never_shrink_fields() {
    let source = ".data\nLstart:\n.uleb128 Lend-Lstart\n.zero 127\nLend:\n.sleb128 Lstart-Lend\n";
    let assembly = assemble(source).unwrap();
    let data = assembly.sections[0].data();
    assert_eq!(&data[..2], &[0x81, 1]);
    assert_eq!(&data[129..], &[0xff, 0x7e]);
    assert_eq!(assemble(source).unwrap(), assembly);
    // A field that would shrink keeps its width and pads, as clang does:
    // 128 at one byte needs two, and 127 at two bytes is `ff 00`.
    let data = |source: &str| assemble(source).unwrap().sections[0].data().to_vec();
    assert_eq!(
        data(".data\nLbegin:\n.uleb128 129-Lend+Lbegin\nLend:"),
        [0xff, 0]
    );
    // The LSDA shape: alignment after the field absorbs its growth.
    let lsda = data(
        ".section __TEXT,__const\n.uleb128 Lb-La\nLa:\n.p2align 2\n.space 125\nLb:\n\
         .sleb128 Ld-Lc\nLc:\n.p2align 2\n.space 62\nLd:\n.p2align 2\n.sleb128 Le-Lf\n\
         Le:\n.p2align 2\n.space 62\nLf:\n",
    );
    assert_eq!(&lsda[..2], &[0xff, 0]);
    assert_eq!(&lsda[129..131], &[0xbf, 0]);
    assert_eq!(&lsda[196..198], &[0xc0, 0x7f]);
    // Each field grows only after the next one does: 40 layout passes.
    let mut chain = String::from(".data\n");
    for i in 0..40 {
        chain.push_str(&format!(".uleb128 Le{i}-Ls{i}\n"));
        if i > 0 {
            chain.push_str(&format!("Le{}:\n", i - 1));
        }
        chain.push_str(&format!("Ls{i}:\n.space 126\n"));
    }
    chain.push_str(".space 2\nLe39:\n");
    let chain = data(&chain);
    assert_eq!(chain.len(), 40 * 128 + 2);
    assert!(chain.chunks(128).take(40).all(|c| c[..2] == [0x80, 1]));
}
