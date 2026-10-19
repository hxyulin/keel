use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::BTreeMap;

fn obj(source: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(source).unwrap()
}
fn link(inputs: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &inputs
            .iter()
            .map(|bytes| Input {
                name: "lifecycle.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn u64at(b: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(b[n..n + 8].try_into().unwrap())
}
fn symbol(file: &object::File<'_>, n: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == n)
        .unwrap()
        .address()
}
fn interface() -> Vec<u8> {
    let mut image = link(&[obj(
        ".text\n.globl _start\n_start: ret\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n",
    )])
    .unwrap()
    .bytes;
    let file = object::File::parse(image.as_slice()).unwrap();
    let dynamic = file.section_by_name(".dynamic").unwrap().index().0;
    let sym = file.section_by_name(".symtab").unwrap().index().0;
    let strings = file.section_by_name(".strtab").unwrap().index().0;
    let shoff = u64at(&image, 40) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + sym * 64 + 4..shoff + sym * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image
}
fn tags(file: &object::File<'_>) -> BTreeMap<u64, u64> {
    file.section_by_name(".dynamic")
        .unwrap()
        .data()
        .unwrap()
        .chunks_exact(16)
        .map(|c| (u64at(c, 0), u64at(c, 8)))
        .collect()
}
fn words(exe: &[u8], start: u64, size: u64) -> Vec<u64> {
    exe[(start - 0x400000) as usize..(start - 0x400000 + size) as usize]
        .chunks_exact(8)
        .map(|c| u64at(c, 0))
        .collect()
}
#[test]
fn arrays_sort_numeric_priorities_and_keep_input_ties_with_contiguous_tags() {
    let a = obj(
        ".text\n.globl _start,early,late,first,second,pre,finish\n_start: ret\nearly: ret\nlate: ret\nfirst: ret\nsecond: ret\npre: ret\nfinish: ret\n.section .init_array,\"aw\",@init_array\n.p2align 3\n.quad late\n.section .init_array.200,\"aw\",@init_array\n.p2align 3\n.quad first\n.data\n.quad 987\n.section .init_array.101,\"aw\",@init_array\n.p2align 3\n.quad early\n.section .preinit_array,\"aw\",@preinit_array\n.p2align 3\n.quad pre\n.section .fini_array,\"aw\",@fini_array\n.p2align 3\n.quad finish\n",
    );
    let b = obj(".section .init_array.200,\"aw\",@init_array\n.p2align 3\n.quad second\n");
    let exe = link(&[a.clone(), b.clone(), interface()]).unwrap();
    assert_eq!(exe.bytes, link(&[a, b, interface()]).unwrap().bytes);
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let t = tags(&f);
    assert_eq!(
        words(&exe.bytes, t[&25], t[&27]),
        ["early", "first", "second", "late"].map(|n| symbol(&f, n))
    );
    assert_eq!(words(&exe.bytes, t[&32], t[&33]), [symbol(&f, "pre")]);
    assert_eq!(words(&exe.bytes, t[&26], t[&28]), [symbol(&f, "finish")]);
    for s in f.sections().filter(|s| {
        matches!(
            s.name().unwrap(),
            ".init_array"
                | ".init_array.101"
                | ".init_array.200"
                | ".fini_array"
                | ".preinit_array"
        )
    }) {
        let raw = u64at(&exe.bytes, 40) as usize + s.index().0 * 64;
        assert_eq!(u64at(&exe.bytes, raw + 56), 8);
    }
}
#[test]
fn legacy_code_fragments_are_joined_with_nop_alignment_and_dynamic_tags() {
    let a = obj(
        ".text\n.globl _start\n_start: ret\n.section .init,\"ax\",@progbits\n.globl _init\n.hidden _init\n_init:\n.byte 0x55\n.section .fini,\"ax\",@progbits\n.globl _fini\n.hidden _fini\n_fini:\n.byte 0x55\n",
    );
    let b = obj(
        ".text\n.byte 0xcc,0xcc,0xcc\n.section .init,\"ax\",@progbits\n.p2align 4\n.byte 0x5d,0xc3\n.section .fini,\"ax\",@progbits\n.p2align 3\n.byte 0x5d,0xc3\n",
    );
    let exe = link(&[a, b, interface()]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let t = tags(&f);
    for (name, tag) in [(".init", 12), (".fini", 13)] {
        let mut spans: Vec<_> = f.sections().filter(|s| s.name().unwrap() == name).collect();
        spans.sort_by_key(|s| s.address());
        assert_eq!(t[&tag], spans[0].address());
        let start = (spans[0].address() - 0x400000) as usize;
        let tail = (spans[1].address() - 0x400000) as usize;
        assert_eq!(exe.bytes[start], 0x55);
        assert!(exe.bytes[start + 1..tail].iter().all(|b| *b == 0x90));
        assert_eq!(&exe.bytes[tail..tail + 2], &[0x5d, 0xc3]);
    }
}
#[test]
fn malformed_arrays_fail_instead_of_silently_skipping_initializers() {
    for source in [
        ".section .preinit_array.101,\"aw\",@preinit_array\n.p2align 3\n.quad 0\n",
        ".section .wrong,\"aw\",@init_array\n.p2align 3\n.quad 0\n",
        ".section .init_array.bad,\"aw\",@init_array\n.p2align 3\n.quad 0\n",
        ".section .init_array.65536,\"aw\",@init_array\n.p2align 3\n.quad 0\n",
        ".section .init_array,\"aw\",@init_array\n.p2align 3\n.byte 1\n",
    ] {
        let input = obj(source);
        let err = link(&[obj(".text\n.globl _start\n_start: ret\n"), input])
            .err()
            .expect(source);
        assert!(err.contains("array") || err.contains("priority"), "{err}");
    }
}

#[test]
fn explicit_wrong_array_type_entry_size_and_alignment_are_rejected() {
    let valid = obj(".section .init_array,\"aw\",@init_array\n.p2align 3\n.quad 0\n");
    let f = object::File::parse(valid.as_slice()).unwrap();
    let at = u64at(&valid, 40) as usize + f.section_by_name(".init_array").unwrap().index().0 * 64;
    for (offset, bytes) in [
        (4, 1u32.to_le_bytes().to_vec()),
        (56, 4u64.to_le_bytes().to_vec()),
        (48, 16u64.to_le_bytes().to_vec()),
        (8, 2u64.to_le_bytes().to_vec()),
    ] {
        let mut malformed = valid.clone();
        malformed[at + offset..at + offset + bytes.len()].copy_from_slice(&bytes);
        let err = link(&[obj(".text\n.globl _start\n_start: ret\n"), malformed])
            .err()
            .expect("invalid array accepted");
        assert!(err.contains("array"), "{err}");
    }
}

#[test]
fn empty_arrays_and_unmarked_fragments_do_not_create_dispatch_tags() {
    let exe=link(&[obj(".text\n.globl _start\n_start: ret\n.section .init,\"ax\",@progbits\nret\n.section .init_array,\"aw\",@init_array\n"),interface()]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let t = tags(&f);
    for tag in [12, 13, 25, 26, 27, 28, 32, 33] {
        assert!(!t.contains_key(&tag));
    }
}

#[test]
fn dispatch_uses_the_symbol_offset_instead_of_the_section_start() {
    let exe=link(&[obj(".text\n.globl _start\n_start: ret\n.section .init,\"ax\",@progbits\n.byte 0xcc\n.globl _init\n.hidden _init\n_init: ret\n"),interface()]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let t = tags(&f);
    assert_eq!(t[&12], symbol(&f, "_init"));
    assert_ne!(t[&12], f.section_by_name(".init").unwrap().address());
}
