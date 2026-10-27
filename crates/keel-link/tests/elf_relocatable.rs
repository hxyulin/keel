use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};

fn assemble(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn partial(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::relocatable::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "partial.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        elf::Machine::X86_64,
    )
}
fn final_link(objects: &[Vec<u8>]) -> Vec<u8> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "input.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
    .unwrap()
    .bytes
}
fn value(bytes: &[u8], name: &str) -> u64 {
    let f = object::File::parse(bytes).unwrap();
    f.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
fn payload(bytes: &[u8], name: &str, size: usize) -> Vec<u8> {
    let f = object::File::parse(bytes).unwrap();
    let s = f.symbols().find(|s| s.name().unwrap() == name).unwrap();
    let section = f.section_by_index(s.section_index().unwrap()).unwrap();
    let offset = (s.address() - section.address()) as usize;
    section.data().unwrap()[offset..offset + size].to_vec()
}
#[test]
fn partial_keeps_undefined_relocations_and_final_result() {
    let caller = assemble(
        ".text\n.globl _start\n_start: call answer\nret\n.data\n.globl pointer\npointer: .quad answer+3\n",
    );
    let callee = assemble(".text\n.globl answer\nanswer: mov eax,42\nret\n");
    let intermediate = partial(std::slice::from_ref(&caller)).unwrap();
    let file = object::File::parse(intermediate.bytes.as_slice()).unwrap();
    assert_eq!(file.kind(), object::ObjectKind::Relocatable);
    assert!(
        file.symbols()
            .find(|s| s.name().unwrap() == "answer")
            .unwrap()
            .is_undefined()
    );
    assert_eq!(
        file.sections()
            .map(|s| s.relocations().count())
            .sum::<usize>(),
        2
    );
    let direct = final_link(&[caller, callee.clone()]);
    let indirect = final_link(&[intermediate.bytes, callee]);
    for name in ["_start", "answer", "pointer"] {
        assert_eq!(value(&direct, name), value(&indirect, name));
    }
    assert_eq!(
        payload(&direct, "pointer", 8),
        payload(&indirect, "pointer", 8)
    );
    assert_eq!(
        payload(&direct, "_start", 6),
        payload(&indirect, "_start", 6)
    );
}
#[test]
fn common_weak_locals_and_nested_partial_links() {
    let a = assemble(
        ".comm common,16,16\n.text\n.globl _start\n_start: ret\n.weak choice\nchoice: ret\n.data\nlocal: .quad 11\n.globl a\na: .quad local,common,choice\n",
    );
    let b = assemble(
        ".comm common,64,32\n.text\n.globl choice\nchoice: mov eax,42\nret\n.data\nlocal: .quad 22\n.globl b\nb: .quad local,common,choice\n",
    );
    let combined = partial(&[a.clone(), b.clone()]).unwrap().bytes;
    assert_eq!(combined, partial(&[a.clone(), b.clone()]).unwrap().bytes);
    let f = object::File::parse(combined.as_slice()).unwrap();
    let common = f.symbols().find(|s| s.name().unwrap() == "common").unwrap();
    assert!(common.is_common());
    assert_eq!(common.address(), 32);
    assert_eq!(common.size(), 64);
    assert_eq!(
        f.symbols().filter(|s| s.name().unwrap() == "local").count(),
        2
    );
    let nested = partial(&[combined]).unwrap().bytes;
    let direct = final_link(&[a, b]);
    let indirect = final_link(&[nested]);
    for name in ["a", "b"] {
        assert_eq!(payload(&direct, name, 24), payload(&indirect, name, 24));
    }
}
#[test]
fn comdat_survives_partial_linking_and_losing_references_disappear() {
    let source = |n, extra: &str| {
        assemble(&format!(
            ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice: ret\n.section .data.choice,\"awG\",@progbits,key,comdat\n.globl payload\npayload: .quad {n}\n{extra}"
        ))
    };
    let first = source(11, "");
    let second = source(22, ".quad absent\n");
    let combined = partial(&[first, second]).unwrap().bytes;
    let f = object::File::parse(combined.as_slice()).unwrap();
    assert_eq!(f.comdats().count(), 1);
    assert!(!f.symbols().any(|s| s.name().unwrap() == "absent"));
    let exe = final_link(&[assemble(".text\n.globl _start\n_start: ret\n"), combined]);
    assert_eq!(payload(&exe, "payload", 8), 11u64.to_le_bytes());
}
#[test]
fn duplicate_strong_and_non_object_inputs_reject() {
    let object = assemble(".text\n.globl duplicate\nduplicate: ret\n");
    assert!(
        partial(&[object.clone(), object])
            .unwrap_err()
            .contains("duplicate strong symbol")
    );
    assert!(partial(&[b"bad input".to_vec()]).is_err());
}

#[test]
fn comdat_unwind_offsets_follow_filtered_records() {
    let group = |name: &str| {
        assemble(&format!(
            ".section .text.{name},\"axG\",@progbits,{name},comdat\n.weak {name}\n.type {name},@function\n{name}:\n.cfi_startproc\nret\n.cfi_endproc\n"
        ))
    };
    let first = group("choice");
    let second = assemble(
        ".section .text.choice,\"axG\",@progbits,choice,comdat\n.weak choice\n.type choice,@function\nchoice:\n.cfi_startproc\nret\n.cfi_endproc\n.text\n.globl other\n.type other,@function\nother:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let combined = partial(&[first, second]).unwrap().bytes;
    let exe = final_link(&[assemble(".text\n.globl _start\n_start: ret\n"), combined]);
    let file = object::File::parse(exe.as_slice()).unwrap();
    let hdr = file
        .section_by_name(".eh_frame_hdr")
        .unwrap()
        .data()
        .unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("keel-partial-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove partial-link test artifacts");
    }
}

#[test]
fn gnu_final_link_matches_with_and_without_keel_partial_link() {
    let Some(ld) = std::env::var_os("KEEL_GNU_LD") else {
        return;
    };
    let scratch = Scratch::new();
    let objects = [
        assemble(
            ".text\n.globl _start\n_start: call answer\nret\n.data\nlocal: .quad 11\n.globl p\np: .quad local,answer+3,common\n.comm common,16,16\n",
        ),
        assemble(".text\n.globl answer\nanswer: mov eax,42\nret\n.comm common,64,32\n"),
    ];
    let combined = partial(&objects).unwrap().bytes;
    std::fs::write(scratch.0.join("partial.o"), combined).unwrap();
    for (i, bytes) in objects.iter().enumerate() {
        std::fs::write(scratch.0.join(format!("{i}.o")), bytes).unwrap();
    }
    for (output, inputs) in [
        ("direct", vec!["0.o", "1.o"]),
        ("indirect", vec!["partial.o"]),
    ] {
        let result = std::process::Command::new(&ld)
            .current_dir(&scratch.0)
            .args(["-m", "elf_x86_64", "-o", output])
            .args(inputs)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let direct = std::fs::read(scratch.0.join("direct")).unwrap();
    let indirect = std::fs::read(scratch.0.join("indirect")).unwrap();
    for name in ["_start", "answer", "common", "p"] {
        assert_eq!(value(&direct, name), value(&indirect, name));
    }
    assert_eq!(payload(&direct, "p", 24), payload(&indirect, "p", 24));
    assert_eq!(
        payload(&direct, "_start", 6),
        payload(&indirect, "_start", 6)
    );
}

#[test]
#[ignore = "requires KEEL_KERNEL_AGGREGATE, KEEL_KERNEL_SCRIPT and KEEL_GNU_LD"]
fn pinned_kernel_partial_link_preserves_gnu_final_layout() {
    let ld = std::env::var_os("KEEL_GNU_LD").expect("GNU ld path");
    let aggregate = std::env::var_os("KEEL_KERNEL_AGGREGATE").expect("pinned aggregate path");
    let script = std::env::var_os("KEEL_KERNEL_SCRIPT").expect("pinned expanded script path");
    let bytes = std::fs::read(&aggregate).unwrap();
    let combined = partial(&[bytes]).unwrap().bytes;
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("partial.o"), &combined).unwrap();
    for (output, input) in [
        ("direct", std::path::PathBuf::from(aggregate)),
        ("indirect", scratch.0.join("partial.o")),
    ] {
        let result = std::process::Command::new(&ld)
            .current_dir(&scratch.0)
            .args([
                "-m",
                "elf_x86_64",
                "-z",
                "noexecstack",
                "--no-warn-rwx-segments",
                "-T",
            ])
            .arg(&script)
            .args(["-o", output])
            .arg(input)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let inspect = |bytes: &[u8]| {
        let f = object::File::parse(bytes).unwrap();
        let sections = f.sections().filter(|s| matches!(s.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 2 != 0)).map(|s| (s.name().unwrap().to_owned(), (s.address(), s.size(), s.data().unwrap().to_vec()))).collect::<std::collections::BTreeMap<_, _>>();
        let symbols = f
            .symbols()
            .filter(|s| s.is_global() || s.is_weak())
            .map(|s| (s.name().unwrap().to_owned(), (s.address(), s.size())))
            .collect::<std::collections::BTreeMap<_, _>>();
        (f.entry(), sections, symbols)
    };
    let direct = std::fs::read(scratch.0.join("direct")).unwrap();
    let indirect = std::fs::read(scratch.0.join("indirect")).unwrap();
    let (entry_a, sections_a, symbols_a) = inspect(&direct);
    let (entry_b, sections_b, symbols_b) = inspect(&indirect);
    assert_eq!(entry_a, entry_b);
    assert_eq!(symbols_a, symbols_b);
    assert_eq!(
        sections_a.keys().collect::<Vec<_>>(),
        sections_b.keys().collect::<Vec<_>>()
    );
    for (name, contents) in sections_a {
        assert_eq!(contents, sections_b[&name], "section {name}");
    }
}
