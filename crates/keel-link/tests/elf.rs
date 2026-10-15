use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
fn object(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "fixture.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        elf::Options::default(),
    )
}
fn value(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
fn word(bytes: &[u8], address: u64) -> u64 {
    let at = (address - 0x400000) as usize;
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
const START: &str = ".text\n.globl _start\n_start:\nmov eax,60\nmov edi,42\nsyscall\n";
#[test]
fn layout_relocations_got_bss_and_determinism() {
    let start = object(
        ".text\n.globl _start\n_start:\ncall answer\nmov edi,eax\nmov eax,60\nsyscall\n.data\n.globl ptr\nptr:\n.quad answer+3\n.quad missing\n.weak missing\n.bss\n.p2align 5\n.globl zero\nzero:\n.zero 96\n",
    );
    let answer = object(
        ".text\n.globl answer\nanswer:\nmov eax,42\nret\n.globl got_load\ngot_load:\nmov rax, qword ptr [rip+answer@GOTPCREL]\nret\n",
    );
    let inputs = [start, answer];
    let exe = link(&inputs).unwrap();
    assert_eq!(exe.bytes, link(&inputs).unwrap().bytes);
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(file.kind(), object::ObjectKind::Executable);
    assert_eq!(file.entry(), value(&file, "_start"));
    assert_eq!(
        word(&exe.bytes, value(&file, "ptr")),
        value(&file, "answer") + 3
    );
    assert_eq!(word(&exe.bytes, value(&file, "ptr") + 8), 0);
    assert_eq!(value(&file, "zero") % 32, 0);
    let got = file.section_by_name(".got").unwrap();
    assert_eq!(word(&exe.bytes, got.address()), value(&file, "answer"));
    assert_eq!(file.section_by_name(".bss").unwrap().size(), 96);
}
#[test]
fn common_merges_and_strong_overrides_weak_in_both_orders() {
    let a = object(".comm shared,16,16\n.text\n.weak answer\nanswer:\nret\n");
    let b = object(".comm shared,64,32\n.text\n.globl answer\nanswer:\nmov eax,42\nret\n");
    for rest in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
        let exe = link(&[object(START), rest[0].clone(), rest[1].clone()]).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(value(&file, "shared") % 32, 0);
        assert_eq!(
            file.symbols()
                .find(|s| s.name().unwrap() == "shared")
                .unwrap()
                .size(),
            64
        );
        let at = (value(&file, "answer") - 0x400000) as usize;
        assert_eq!(&exe.bytes[at..at + 5], &[0xb8, 42, 0, 0, 0]);
        assert_eq!(file.section_by_name(".bss.common").unwrap().size(), 64);
    }
}
#[test]
fn diagnostics_for_undefined_duplicate_missing_entry_and_overflow() {
    let start = object(START);
    assert!(
        link(&[start.clone(), start.clone()])
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(
        link(&[object(".text\n.globl _start\n_start:\ncall missing\n")])
            .unwrap_err()
            .contains("undefined symbol `missing`")
    );
    assert!(
        link(&[object(".text\n.globl answer\nanswer:\nret\n")])
            .unwrap_err()
            .contains("missing entry")
    );
    let huge = object(".text\n.globl _start\n_start:\nmov eax, far\n");
    assert!(
        link(&[huge, object(".globl far\n.set far,0x100000000\n")])
            .unwrap_err()
            .contains("overflow")
    );
}
#[test]
fn local_names_do_not_collide_and_custom_entry_is_supported() {
    let a = object(
        ".text\n.globl begin\nbegin:\nret\n.data\nlocal:\n.quad 17\n.globl pointer_a\npointer_a:\n.quad local\n",
    );
    let b = object(".data\nlocal:\n.quad 29\n.globl pointer_b\npointer_b:\n.quad local\n");
    let exe = elf::link(
        &[
            Input {
                name: "a",
                bytes: &a,
            },
            Input {
                name: "b",
                bytes: &b,
            },
        ],
        elf::Options {
            entry: "begin".into(),
        },
    )
    .unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        word(&exe.bytes, word(&exe.bytes, value(&file, "pointer_a"))),
        17
    );
    assert_eq!(
        word(&exe.bytes, word(&exe.bytes, value(&file, "pointer_b"))),
        29
    );
}
#[test]
fn malformed_truncated_and_foreign_inputs_are_errors() {
    for n in 0..128 {
        assert!(link(&[vec![0; n]]).is_err());
    }
    let valid = object(START);
    for n in 0..valid.len() {
        assert!(link(&[valid[..n].to_vec()]).is_err(), "truncation {n}");
    }
    let mut wrong = valid.clone();
    wrong[18] = 183;
    assert!(link(&[wrong]).unwrap_err().contains("x86-64"));
    let mut shared = valid;
    shared[16] = 3;
    assert!(link(&[shared]).unwrap_err().contains("relocatable"));
    assert!(elf::link(&[], elf::Options::default()).is_err());
}
#[test]
fn unsupported_tls_and_relocations_are_diagnostics() {
    let tls =
        object(".text\n.globl _start\n_start:\nret\n.section .tdata,\"awT\",@progbits\n.quad 1\n");
    assert!(link(&[tls]).unwrap_err().contains("unsupported flags"));
    let relocation = object(".text\n.globl _start\n_start:\nret\n.data\n.short _start\n");
    assert!(
        link(&[relocation])
            .unwrap_err()
            .contains("unsupported x86-64 relocation")
    );
}

#[test]
fn malformed_relocation_tables_and_fields_are_rejected() {
    let valid = object(".text\n.globl _start\n_start:\ncall target\n");
    let target = object(".text\n.globl target\ntarget:\nret\n");
    let file = object::File::parse(valid.as_slice()).unwrap();
    let rela = file.section_by_name(".rela.text").unwrap();
    let (offset, _) = rela.file_range().unwrap();
    let shoff = u64::from_le_bytes(valid[40..48].try_into().unwrap()) as usize;
    let header = shoff + rela.index().0 * 64;
    for (at, data) in [
        (header + 56, 23u64.to_le_bytes().to_vec()),
        (header + 32, 23u64.to_le_bytes().to_vec()),
        (offset as usize, u64::MAX.to_le_bytes().to_vec()),
        (offset as usize + 12, 999999u32.to_le_bytes().to_vec()),
    ] {
        let mut bad = valid.clone();
        bad[at..at + data.len()].copy_from_slice(&data);
        assert!(link(&[bad, target.clone()]).is_err());
    }
}
