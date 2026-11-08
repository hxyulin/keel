use super::{link_with, object as assemble, value};
use keel_link::elf;
use object::{Object, ObjectSection};

fn got_object(source: &str, kind: u32) -> Vec<u8> {
    let mut bytes = assemble(source);
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let rela = file.section_by_name(".rela.text").unwrap();
    let start = rela.file_range().unwrap().0 as usize;
    let indices: Vec<_> = rela
        .data()
        .unwrap()
        .as_chunks::<24>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, r)| u32::from_le_bytes(r[8..12].try_into().unwrap()) == 9)
        .map(|(i, _)| start + i * 24 + 8)
        .collect();
    assert!(!indices.is_empty(), "fixture must have GOT relocations");
    for index in indices {
        bytes[index..index + 4].copy_from_slice(&kind.to_le_bytes());
    }
    bytes
}

fn link(objects: &[Vec<u8>], enabled: bool) -> elf::Executable {
    link_with(
        "relax.o",
        objects,
        elf::Options {
            relax_got: enabled,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn got_loads_of_selected_definitions_become_direct_addresses_without_slots() {
    for reg in ["rax", "r11"] {
        let source = got_object(
            &format!(
                ".text\n.globl _start\n_start: mov {reg},qword ptr [rip+answer@GOTPCREL]\nret\n"
            ),
            object::elf::R_X86_64_REX_GOTPCRELX,
        );
        for definition in [
            ".data\n.globl answer\nanswer: .quad 42\n",
            ".comm answer,8,8\n",
        ] {
            let objects = [source.clone(), assemble(definition)];
            let baseline = link(&objects, false);
            let candidate = link(&objects, true);
            let old = object::File::parse(baseline.bytes.as_slice()).unwrap();
            let new = object::File::parse(candidate.bytes.as_slice()).unwrap();
            let old_code = old.section_by_name(".text").unwrap().data().unwrap();
            let code = new.section_by_name(".text").unwrap();
            let data = code.data().unwrap();
            assert_eq!(old_code[1], 0x8b);
            assert_eq!(data[1], 0x8d);
            assert_eq!(old_code.len(), data.len());
            let displacement = i32::from_le_bytes(data[3..7].try_into().unwrap());
            assert_eq!(
                (code.address() + 7).wrapping_add_signed(displacement.into()),
                value(&new, "answer"),
                "{reg}, {definition}"
            );
            assert_eq!(old.section_by_name(".got").unwrap().size(), 8);
            assert_eq!(new.section_by_name(".got").unwrap().size(), 0);
            assert_eq!(candidate.bytes, link(&objects, true).bytes);
        }
    }
}

#[test]
fn ordinary_got_relocations_and_noncanonical_forms_keep_the_got() {
    for (instruction, kind) in [
        ("mov rax,qword ptr [rip+answer@GOTPCREL]", 9),
        ("mov rax,qword ptr [rip+answer@GOTPCREL]", 41),
        ("mov eax,dword ptr [rip+answer@GOTPCREL]", 42),
        (".byte 0x67\nmov rax,qword ptr [rip+answer@GOTPCREL]", 42),
        ("add rax,qword ptr [rip+answer@GOTPCREL]", 42),
        ("mov rax,qword ptr [rip+answer@GOTPCREL+1]", 42),
    ] {
        let source = got_object(
            &format!(".text\n.globl _start\n_start: {instruction}\nret\n"),
            kind,
        );
        let objects = [source, assemble(".data\n.globl answer\nanswer: .quad 42\n")];
        assert_eq!(
            link(&objects, false).bytes,
            link(&objects, true).bytes,
            "{instruction}, kind {kind}"
        );
    }
}

#[test]
fn overlapping_relocation_fields_keep_the_original_instructions() {
    let mut source = got_object(
        ".text\n.globl _start\n_start: mov rax,qword ptr [rip+answer@GOTPCREL]\nret\n.long answer\n",
        object::elf::R_X86_64_REX_GOTPCRELX,
    );
    let file = object::File::parse(source.as_slice()).unwrap();
    let at = file
        .section_by_name(".rela.text")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    source[at + 24..at + 32].copy_from_slice(&1u64.to_le_bytes());
    let objects = [source, assemble(".data\n.globl answer\nanswer: .quad 42\n")];
    assert_eq!(link(&objects, false).bytes, link(&objects, true).bytes);
}

#[test]
fn weak_undefined_absolute_and_imported_symbols_keep_the_got() {
    let source = ".text\n.globl _start\n_start: mov rax,qword ptr [rip+answer@GOTPCREL]\nret\n";
    let kind = object::elf::R_X86_64_REX_GOTPCRELX;
    for objects in [
        vec![got_object(&format!("{source}.weak answer\n"), kind)],
        vec![
            got_object(source, kind),
            assemble(".globl answer\n.set answer,0xffff800000000000\n"),
        ],
        vec![got_object(source, kind), super::shared_interface()],
    ] {
        assert_eq!(link(&objects, false).bytes, link(&objects, true).bytes);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn relaxed_image_executes_with_the_address_of_the_selected_definition() {
    use std::os::unix::fs::PermissionsExt;
    let source = got_object(
        ".text\n.globl _start\n_start: mov rax,qword ptr [rip+answer@GOTPCREL]\nmov edi,dword ptr [rax]\nmov eax,60\nsyscall\n",
        object::elf::R_X86_64_REX_GOTPCRELX,
    );
    let objects = [source, assemble(".data\n.globl answer\nanswer: .long 42\n")];
    let dir = keel_test_support::TempDir::new("elf-relax");
    for enabled in [false, true] {
        let path = dir.join(format!("relax-{enabled}"));
        std::fs::write(&path, link(&objects, enabled).bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            std::process::Command::new(path).status().unwrap().code(),
            Some(42)
        );
    }
}
