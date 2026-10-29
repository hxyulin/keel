use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

#[test]
fn local_symbols_keep_identity_bounds_and_precede_globals() {
    let objects: Vec<_> = ["_start", "other"]
        .map(|entry| keel_asm::x86::assemble_object(&format!(
            ".text\n.globl {entry}\n{entry}: ret\n.type helper,@function\nhelper: nop\nret\n.size helper,.-helper\n.data\nlocal_data: .quad helper\n.section .debug_info,\"\",@progbits\nomitted: .byte 0\n"
        )).unwrap()).into();
    let exe = elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "locals.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
    .unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let helpers: Vec<_> = file
        .symbols()
        .filter(|s| s.name().unwrap() == "helper")
        .collect();
    assert_eq!(helpers.len(), 2);
    assert_ne!(helpers[0].address(), helpers[1].address());
    for helper in helpers {
        assert!(helper.is_local());
        assert_eq!(helper.kind(), object::SymbolKind::Text);
        assert_eq!(helper.size(), 2);
        let section = file
            .section_by_index(helper.section_index().unwrap())
            .unwrap();
        assert_eq!(helper.address(), section.address() + 1);
    }
    assert!(!file.symbols().any(|s| s.name().unwrap() == "omitted"));
    let symtab = file.section_by_name(".symtab").unwrap();
    let shoff = u64::from_le_bytes(exe.bytes[40..48].try_into().unwrap()) as usize;
    let at = shoff + symtab.index().0 * 64 + 44;
    let first_global = u32::from_le_bytes(exe.bytes[at..at + 4].try_into().unwrap()) as usize;
    let records = symtab.data().unwrap();
    assert!(first_global > 1);
    for (i, record) in records.as_chunks::<24>().0.iter().enumerate() {
        assert_eq!(record[4] >> 4 == 0, i < first_global);
    }
}
