use object::{Object, ObjectSection};

#[test]
fn debug_leb_differences_can_span_atoms_but_runtime_data_cannot() {
    let prefix =
        ".subsections_via_symbols\n.globl _first, _second\n_first: ret\n_second:\nLsecond: ret\n";
    let source = format!(
        "{prefix}.section __DWARF,__debug_rnglists,regular,debug\n.uleb128 Lsecond-_first\n"
    );
    let bytes = keel_asm::assemble_object(&source).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(
        file.section_by_name("__debug_rnglists")
            .unwrap()
            .data()
            .unwrap(),
        &[4]
    );
    let runtime = format!("{prefix}.data\n.uleb128 Lsecond-_first\n");
    assert!(
        keel_asm::assemble_object(&runtime)
            .unwrap_err()
            .message
            .contains("one linker atom")
    );
}

#[test]
fn segment_virtual_size_covers_file_padding_with_mixed_alignment() {
    fn u64le(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }
    // Different file/VM starting offsets cause different padding at the next
    // aligned section. Exercise both sides of each alignment boundary.
    for align in 0..=4 {
        for instructions in 1..=4 {
            for strings in 1..=17 {
                let source = format!(
                    ".text\n{}.section __TEXT,__literal16,16byte_literals\n.p2align {align}\n.space 16\n.section __DWARF,__debug_str,regular,debug\n.space {strings}\n",
                    "ret\n".repeat(instructions)
                );
                let bytes = keel_asm::assemble_object(&source).unwrap();
                // The first command is LC_SEGMENT_64, starting at byte 32.
                assert!(u64le(&bytes, 64) >= u64le(&bytes, 80), "{source}");
                object::File::parse(bytes.as_slice()).unwrap();
            }
        }
    }
}
