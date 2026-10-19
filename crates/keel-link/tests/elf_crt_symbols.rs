use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

fn obj(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "crt.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn word(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn value(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
fn pointer(exe: &[u8], file: &object::File<'_>, name: &str) -> u64 {
    word(exe, (value(file, name) - 0x400000) as usize)
}
fn interface() -> Vec<u8> {
    let mut image=link(&[obj(".text\n.globl _start\n_start: ret\n.data\n.globl _DYNAMIX\n_DYNAMIX: .quad 0\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n")]).unwrap().bytes;
    let f = object::File::parse(image.as_slice()).unwrap();
    let dynamic = f.section_by_name(".dynamic").unwrap().index().0;
    let sym = f.section_by_name(".symtab").unwrap().index().0;
    let strings = f.section_by_name(".strtab").unwrap().index().0;
    let shoff = word(&image, 40) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + sym * 64 + 4..shoff + sym * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image
}
fn archive(member: Vec<u8>) -> Vec<u8> {
    let mut bytes = b"!<arch>\n".to_vec();
    bytes.extend_from_slice(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "fake.o/",
            0,
            0,
            0,
            "100644",
            member.len()
        )
        .as_bytes(),
    );
    bytes.extend_from_slice(&member);
    if !bytes.len().is_multiple_of(2) {
        bytes.push(b'\n');
    }
    bytes
}
#[test]
fn got_declarations_and_supported_references_use_a_hidden_internal_anchor() {
    let source = obj(
        ".text\n.globl _start\n_start:\nmov rax,qword ptr [rip+_GLOBAL_OFFSET_TABLE_@GOTPCREL]\nret\n.data\n.globl got_pointer\ngot_pointer: .quad _GLOBAL_OFFSET_TABLE_\n",
    );
    for objects in [vec![source.clone()], vec![source, interface()]] {
        let exe = link(&objects).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        let got = f.section_by_name(".got").unwrap();
        assert_eq!(pointer(&exe.bytes, &f, "got_pointer"), got.address());
        assert_eq!(word(got.data().unwrap(), 0), got.address());
        assert_eq!(value(&f, "_GLOBAL_OFFSET_TABLE_"), got.address());
        let s = f
            .symbols()
            .find(|s| s.name().unwrap() == "_GLOBAL_OFFSET_TABLE_")
            .unwrap();
        assert!(matches!(
            s.flags(),
            object::SymbolFlags::Elf { st_other: 2, .. }
        ));
        assert!(
            !f.dynamic_symbols()
                .any(|s| s.name().unwrap() == "_GLOBAL_OFFSET_TABLE_")
        );
    }
    let decl = obj(".text\n.globl _start\n_start: ret\n.globl _GLOBAL_OFFSET_TABLE_\n");
    assert!(link(&[decl]).is_ok());
}
#[test]
fn dynamic_anchor_is_nonpreemptible_and_static_weak_reference_stays_zero() {
    let source = obj(
        ".text\n.globl _start\n_start:\nmov rax,qword ptr [rip+_DYNAMIC@GOTPCREL]\nret\n.hidden _DYNAMIC\n.data\n.globl dynamic_pointer\ndynamic_pointer: .quad _DYNAMIC\n",
    );
    let err = link(&[source.clone()]).unwrap_err();
    assert!(err.contains("requires dynamically linked output"), "{err}");
    let exe = link(&[source, interface()]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let dynamic = f.section_by_name(".dynamic").unwrap();
    assert_eq!(
        pointer(&exe.bytes, &f, "dynamic_pointer"),
        dynamic.address()
    );
    assert_eq!(
        word(f.section_by_name(".got").unwrap().data().unwrap(), 0),
        dynamic.address()
    );
    assert_eq!(value(&f, "_DYNAMIC"), dynamic.address());
    assert!(!f.dynamic_symbols().any(|s| s.name().unwrap() == "_DYNAMIC"));
    let weak = obj(
        ".text\n.globl _start\n_start: ret\n.weak _DYNAMIC\n.data\n.globl weak_pointer\nweak_pointer: .quad _DYNAMIC\n",
    );
    let exe = link(&[weak]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(pointer(&exe.bytes, &f, "weak_pointer"), 0);
}
#[test]
fn linker_owned_names_neither_pull_archive_members_nor_accept_input_definitions() {
    let main = obj(".text\n.globl _start\n_start: ret\n.globl _GLOBAL_OFFSET_TABLE_\n");
    let fake = archive(obj(
        ".text\n.globl _start,_GLOBAL_OFFSET_TABLE_\n_start: ret\n_GLOBAL_OFFSET_TABLE_: ret\n",
    ));
    assert!(link(&[main.clone(), fake]).is_ok());
    for name in ["_GLOBAL_OFFSET_TABLE_", "_DYNAMIC"] {
        for declaration in [
            format!(".globl {name}\n.set {name},42\n"),
            format!(".comm {name},8,8\n"),
            format!(".text\n.weak {name}\n{name}: ret\n"),
        ] {
            let err = link(&[main.clone(), obj(&declaration)]).unwrap_err();
            assert!(err.contains("reserved linker-owned symbol"), "{err}");
        }
    }
}
#[test]
fn unsupported_got_base_relocations_are_not_enabled_by_the_new_anchor() {
    let mut input = obj(
        ".text\n.globl _start\n_start: mov rax,qword ptr [rip+_GLOBAL_OFFSET_TABLE_@GOTPCREL]\nret\n",
    );
    let f = object::File::parse(input.as_slice()).unwrap();
    let at = f
        .section_by_name(".rela.text")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    input[at + 8..at + 12].copy_from_slice(&26u32.to_le_bytes());
    let err = link(&[input]).unwrap_err();
    assert!(err.contains("unsupported x86-64 relocation 26"), "{err}");
}
#[test]
fn empty_clone_tables_share_one_address_despite_intervening_arrays_and_data() {
    let a = obj(
        ".text\n.globl _start\n_start: ret\n.section .tm_clone_table,\"aw\",@progbits\n.p2align 3\nlist:\n.data\n.globl list_pointer\nlist_pointer: .quad list\n.section .init_array,\"aw\",@init_array\n.p2align 3\n.quad _start\n",
    );
    let b = obj(
        ".data\n.zero 120\n.section .tm_clone_table,\"aw\",@progbits\n.p2align 3\n.globl __TMC_END__\n.hidden __TMC_END__\n__TMC_END__:\n",
    );
    let exe = link(&[a, b, interface()]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        pointer(&exe.bytes, &f, "list_pointer"),
        value(&f, "__TMC_END__")
    );
    let tables: Vec<_> = f
        .sections()
        .filter(|s| s.name().unwrap() == ".tm_clone_table")
        .collect();
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].address(), tables[1].address());
    let bad = obj(
        ".text\n.globl _start\n_start: ret\n.section .tm_clone_table,\"aw\",@progbits\n.p2align 3\n.quad 0,0\n",
    );
    assert!(
        link(&[bad])
            .unwrap_err()
            .contains("currently requires empty")
    );
}

#[test]
fn shared_library_imports_cannot_use_hidden_executable_anchors() {
    let mut library = interface();
    let f = object::File::parse(library.as_slice()).unwrap();
    let sym = f
        .dynamic_symbols()
        .find(|s| s.name().unwrap() == "_DYNAMIX")
        .unwrap();
    let index = sym.index().0;
    let at = f
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize
        + index * 24;
    library[at + 6..at + 8].copy_from_slice(&0u16.to_le_bytes());
    let name = library.windows(9).position(|w| w == b"_DYNAMIX\0").unwrap();
    library[name..name + 9].copy_from_slice(b"_DYNAMIC\0");
    let err = link(&[obj(".text\n.globl _start\n_start: ret\n"), library]).unwrap_err();
    assert!(
        err.contains("shared library requires reserved linker-owned symbol `_DYNAMIC`"),
        "{err}"
    );
}
