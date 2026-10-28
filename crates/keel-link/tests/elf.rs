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
    for rest in [[a.clone(), b.clone()], [b, a]] {
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
fn selected_symbol_emission_agrees_with_repeated_relocations_after_gc_and_layout() {
    let a = object(
        ".text\n.globl _start\n_start: ret\n\
         .comm shared,16,16\n.comm overridden,16,16\n\
         .section .data.a,\"awR\"\n.weak choice\nchoice: .quad 11\n\
         local: .quad 17\n.globl pointers_a\npointers_a:\n\
         .quad choice,choice+1,shared,shared+8,overridden,overridden+1,local,local+1\n",
    );
    let b = object(
        ".comm shared,64,32\n\
         .section .data.b,\"awR\"\n.globl choice\nchoice: .quad 29\n\
         .globl overridden\noverridden: .quad 31\n\
         local: .quad 37\n.globl pointers_b\npointers_b:\n\
         .quad choice,choice+1,shared,shared+8,overridden,overridden+1,local,local+1\n",
    );
    for objects in [[a.clone(), b.clone()], [b, a]] {
        for gc_sections in [false, true] {
            let inputs: Vec<_> = objects
                .iter()
                .map(|bytes| Input {
                    name: "selected.o",
                    bytes,
                })
                .collect();
            let options = || elf::Options {
                gc_sections,
                ..Default::default()
            };
            let exe = elf::link(&inputs, options()).unwrap();
            assert_eq!(exe.bytes, elf::link(&inputs, options()).unwrap().bytes);
            let file = object::File::parse(exe.bytes.as_slice()).unwrap();
            assert_eq!(word(&exe.bytes, value(&file, "choice")), 29);
            assert_eq!(word(&exe.bytes, value(&file, "overridden")), 31);
            assert_eq!(value(&file, "shared") % 32, 0);
            let locals: Vec<_> = file
                .symbols()
                .filter(|s| s.name().unwrap() == "local")
                .map(|s| s.address())
                .collect();
            assert_eq!(locals.len(), 2);
            assert_ne!(locals[0], locals[1]);
            for (pointer, payload) in [("pointers_a", 17), ("pointers_b", 37)] {
                let base = value(&file, pointer);
                for (i, (name, addend)) in [
                    ("choice", 0),
                    ("choice", 1),
                    ("shared", 0),
                    ("shared", 8),
                    ("overridden", 0),
                    ("overridden", 1),
                ]
                .into_iter()
                .enumerate()
                {
                    assert_eq!(
                        word(&exe.bytes, base + i as u64 * 8),
                        value(&file, name) + addend
                    );
                }
                let local = word(&exe.bytes, base + 48);
                assert!(locals.contains(&local));
                assert_eq!(word(&exe.bytes, local), payload);
                assert_eq!(word(&exe.bytes, base + 56), local + 1);
            }
        }
    }
}

#[test]
fn repeated_absolute_address_keeps_addends_and_later_overflow_diagnostics() {
    let absolute = object(".globl absolute\n.set absolute,0xffffffffffffffff\n");
    let refs = object(&format!(
        "{START}.data\n.globl pointers\npointers: .quad absolute,absolute-1,absolute+1\n"
    ));
    let exe = link(&[refs, absolute.clone()]).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(value(&file, "absolute"), u64::MAX);
    let base = value(&file, "pointers");
    assert_eq!(word(&exe.bytes, base), u64::MAX);
    assert_eq!(word(&exe.bytes, base + 8), u64::MAX - 1);
    assert_eq!(word(&exe.bytes, base + 16), 0);
    let refs = object(&format!(
        "{START}.data\n.quad absolute,absolute\n.long absolute\n"
    ));
    assert_eq!(
        link(&[refs, absolute]).unwrap_err(),
        "fixture.o: relocation 10 overflow for `absolute`"
    );
}

#[test]
fn relocation_encoding_preserves_signed_wraps_boundaries_and_adjacent_bytes() {
    // Patch a real RELA record so every field kind exercises the linker,
    // independently of which relocation the assembler chooses for an opcode.
    let template = object(&format!(
        "{START}.data\n.byte 90\n.globl field\nfield: .quad external\n.byte 165\n"
    ));
    let file = object::File::parse(template.as_slice()).unwrap();
    let rela = file
        .section_by_name(".rela.data")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let data = file
        .section_by_name(".data")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    let provider = |v: u64| object(&format!(".globl external\n.set external,{v:#x}\n"));
    let probe = link(&[template.clone(), provider(0)]).unwrap();
    let file = object::File::parse(probe.bytes.as_slice()).unwrap();
    let p = value(&file, "field");
    let mut cases = vec![
        (1u32, u64::MAX, 1, Some(0u64.to_le_bytes().to_vec())),
        (1, 0, -1, Some(u64::MAX.to_le_bytes().to_vec())),
        (
            10,
            u32::MAX as u64,
            0,
            Some(u32::MAX.to_le_bytes().to_vec()),
        ),
        (10, u32::MAX as u64, 1, None),
        (10, 0, -1, None),
        (11, u64::MAX, 1, Some(0i32.to_le_bytes().to_vec())),
        (11, 0, -1, Some((-1i32).to_le_bytes().to_vec())),
        (
            11,
            i32::MIN as u64,
            0,
            Some(i32::MIN.to_le_bytes().to_vec()),
        ),
        (
            11,
            i32::MAX as u64,
            0,
            Some(i32::MAX.to_le_bytes().to_vec()),
        ),
        (11, i32::MIN as u64, -1, None),
        (11, i32::MAX as u64, 1, None),
    ];
    for kind in [2, 4] {
        for delta in [i32::MIN as i64, -1, i32::MAX as i64] {
            cases.push((kind, p, delta, Some((delta as i32).to_le_bytes().to_vec())));
        }
        cases.push((kind, p, i32::MIN as i64 - 1, None));
        cases.push((kind, p, i32::MAX as i64 + 1, None));
    }
    for delta in [-128i64, -1, 127] {
        cases.push((15, p, delta, Some(vec![delta as u8])));
    }
    cases.push((15, p, -129, None));
    cases.push((15, p, 128, None));
    for delta in [i64::MIN, -1, i64::MAX] {
        cases.push((24, p, delta, Some(delta.to_le_bytes().to_vec())));
    }
    cases.push((24, p + 1, i64::MAX, None));
    cases.push((24, p - 1, i64::MIN, None));
    for (kind, target, addend, expected) in cases {
        let mut input = template.clone();
        input[rela + 8..rela + 12].copy_from_slice(&kind.to_le_bytes());
        input[rela + 16..rela + 24].copy_from_slice(&addend.to_le_bytes());
        input[data + 1..data + 9].fill(0xa5);
        let result = link(&[input, provider(target)]);
        if let Some(expected) = expected {
            let exe = result.unwrap();
            let file = object::File::parse(exe.bytes.as_slice()).unwrap();
            assert_eq!(value(&file, "field"), p);
            let mut bytes = vec![0x5a];
            bytes.extend_from_slice(&expected);
            bytes.resize(10, 0xa5);
            assert_eq!(
                file.section_by_name(".data").unwrap().data().unwrap(),
                bytes,
                "kind={kind}, target={target:#x}, addend={addend}"
            );
        } else {
            assert_eq!(
                result.unwrap_err(),
                format!("fixture.o: relocation {kind} overflow for `external`")
            );
        }
    }
}
#[test]
fn diagnostics_for_undefined_duplicate_missing_entry_and_overflow() {
    let start = object(START);
    assert!(
        link(&[start.clone(), start])
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
            ..elf::Options::default()
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
    assert!(link(&[shared]).unwrap_err().contains("dynamic table"));
    assert!(elf::link(&[], elf::Options::default()).is_err());
}
#[test]
fn unsupported_tls_and_relocations_are_diagnostics() {
    let tls =
        object(".text\n.globl _start\n_start:\nret\n.section .tdata,\"awT\",@progbits\n.quad 1\n");
    let error = link(&[tls]).unwrap_err();
    assert!(
        error.contains("external ELF linking") && error.contains("PT_TLS"),
        "{error}"
    );
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

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = b"!<arch>\n".to_vec();
    for (name, data) in members {
        bytes.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                "100644",
                data.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(data);
        if !bytes.len().is_multiple_of(2) {
            bytes.push(b'\n');
        }
    }
    bytes
}
#[test]
fn archives_extract_transitively_skip_unused_duplicates_and_respect_order() {
    let main = object(".text\n.globl _start\n_start:\ncall answer\n");
    let a = object(".text\n.globl answer\nanswer:\ncall helper\nret\n");
    let b = object(".text\n.globl helper\nhelper:\nret\n");
    let ar = archive(&[("b.o", b), ("unused.o", object(START)), ("a.o", a)]);
    assert!(link(&[main.clone(), ar]).is_ok());
    let order_archive = archive(&[("answer.o", object(".text\n.globl answer\nanswer:\nret\n"))]);
    assert!(
        link(&[order_archive, main])
            .unwrap_err()
            .contains("undefined symbol `answer`")
    );
    assert!(link(&[archive(&[("entry.o", object(START))])]).is_ok());
}
#[test]
fn weak_undefined_does_not_pull_archive_member() {
    let main =
        object(".text\n.globl _start\n_start:\nret\n.data\n.quad optional\n.weak optional\n");
    let ar = archive(&[(
        "optional.o",
        object(".text\n.globl optional\noptional:\ncall missing\n"),
    )]);
    assert!(link(&[main, ar]).is_ok());
    assert!(
        link(&[b"!<thin>\n".to_vec()])
            .unwrap_err()
            .contains("thin archive")
    );
}
// A portable shared-interface fixture: promote an existing image's symbol
// table to DYNSYM and its explicit dynamic table to SHT_DYNAMIC. Runtime
// validation separately uses shared libraries produced by the Linux toolchain.
fn shared_interface() -> Vec<u8> {
    let obj = object(
        ".text\n.globl _start\n_start:\nret\n.globl answer\n.type answer,@function\nanswer:\nret\n.data\n.globl value\n.type value,@object\nvalue:\n.quad 42\n.section .dynamic,\"aw\",@progbits\n.quad 0\n.quad 0\n",
    );
    let mut image = link(&[obj]).unwrap().bytes;
    let file = object::File::parse(image.as_slice()).unwrap();
    let dynamic = file.section_by_name(".dynamic").unwrap().index().0;
    let symbols = file.section_by_name(".symtab").unwrap().index().0;
    let strings = file.section_by_name(".strtab").unwrap().index().0;
    let shoff = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + symbols * 64 + 4..shoff + symbols * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image
}
#[test]
fn dynamic_metadata_imports_exports_and_writable_pointer_relocations() {
    let main = object(
        ".text\n.globl _start\n_start:\ncall answer\nmov rax,qword ptr [rip+value@GOTPCREL]\nret\n.data\n.globl pointer\npointer:\n.quad answer+1\n",
    );
    let shared = shared_interface();
    let exe = link(&[main, shared]).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(
        file.dynamic_symbols().filter(|s| s.is_undefined()).count(),
        2
    );
    assert!(
        file.dynamic_symbols()
            .any(|s| s.name().unwrap() == "pointer" && !s.is_undefined())
    );
    assert_eq!(file.section_by_name(".rela.dyn").unwrap().size(), 72);
    assert_eq!(
        file.section_by_name(".interp").unwrap().data().unwrap(),
        b"/lib64/ld-linux-x86-64.so.2\0"
    );
    assert_eq!(file.section_by_name(".plt").unwrap().size(), 8);
}
#[test]
fn imported_data_needs_pic_and_readonly_pointers_are_rejected() {
    let shared = shared_interface();
    let direct = object(".text\n.globl _start\n_start:\nmov eax,dword ptr [rip+value]\nret\n");
    assert!(
        link(&[direct, shared.clone()])
            .unwrap_err()
            .contains("copy/text relocations")
    );
    let pointer = object(".text\n.globl _start\n_start:\nret\n.section .rodata\n.quad answer\n");
    assert!(
        link(&[pointer, shared])
            .unwrap_err()
            .contains("copy/text relocations")
    );
}
#[test]
fn strong_archive_definition_overrides_tentative_common() {
    let common = object(".text\n.globl _start\n_start:\nret\n.comm shared,8,8\n");
    let strong = object(".data\n.p2align 3\n.globl shared\nshared:\n.quad 42\n");
    let exe = link(&[common, archive(&[("strong.o", strong)])]).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(word(&exe.bytes, value(&file, "shared")), 42);
}

#[test]
fn omitted_symbol_addresses_remain_lazy_and_errors_follow_relocation_order() {
    let source = ".text\n.globl _start\n_start: ret\n\
        .section .debug_info,\"\",@progbits\n\
        .globl omitted\nomitted: .byte 0\n\
        .globl another\nanother: .byte 0\n";
    assert!(link(&[object(source)]).is_ok());
    for (first, second) in [("omitted", "another"), ("another", "omitted")] {
        assert_eq!(
            link(&[object(&format!(
                "{source}.data\n.quad _start,_start,{first},{second}\n"
            ))])
            .unwrap_err(),
            format!("fixture.o: fixture.o: reference to omitted section for `{first}`")
        );
    }
}
