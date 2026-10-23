use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
fn object(s: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(s).unwrap()
}
fn link(objects: &[Vec<u8>]) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .map(|bytes| Input {
                name: "comdat.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        Default::default(),
    )
}
fn value(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
fn word(exe: &elf::Executable, name: &str) -> u64 {
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let at = (value(&f, name) - 0x400000) as usize;
    u64::from_le_bytes(exe.bytes[at..at + 8].try_into().unwrap())
}
const START: &str = ".text\n.globl _start\n_start: ret\n";
fn group(n: u64, tail: &str) -> Vec<u8> {
    object(&format!(
        ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\n.type choice,@function\nchoice:\n.cfi_startproc\nret\n.cfi_endproc\n.section .data.choice,\"awG\",@progbits,key,comdat\n.globl payload\npayload: .quad {n}\nlocal: .quad {n}\n.globl local_ptr\nlocal_ptr: .quad local\n{tail}"
    ))
}
#[test]
fn atomic_first_signature_selection_and_local_targets() {
    for (a, b) in [(11, 22), (22, 11)] {
        let exe = link(&[object(START), group(a, ""), group(b, "")]).unwrap();
        assert_eq!(word(&exe, "payload"), a);
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(word(&exe, "local_ptr"), value(&f, "payload") + 8);
        let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
        assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 1);
        assert_eq!(
            f.sections()
                .filter(|s| s.name().unwrap() == ".data.choice")
                .count(),
            1
        );
        assert!(f.sections().all(|s| !matches!(s.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 0x200 != 0)));
    }
}
#[test]
fn losing_references_do_not_become_imports_or_undefined_errors() {
    let exe = link(&[object(START), group(1, ""), group(2, ".quad absent\n")]).unwrap();
    assert_eq!(word(&exe, "payload"), 1);
}

#[test]
fn tls_contract_is_rejected_even_in_discarded_comdat_payloads() {
    for kind in ["progbits", "nobits"] {
        for retain in ["", "R"] {
            for symbol in [
                "",
                ".globl tls_value\n.type tls_value,@tls_object\ntls_value:\n",
            ] {
                let tls = object(&format!(
                    ".section .tls.choice,\"awTG{retain}\",@{kind},key,comdat\n{symbol}.zero 4\n"
                ));
                // TLS is unsupported for a selected input as a whole. Losing
                // a group must not make support depend on whether the producer
                // emitted a TLS symbol, or on initialized/zero-fill storage.
                for rest in [[group(1, ""), tls.clone()], [tls, group(1, "")]] {
                    let err = link(&[object(START), rest[0].clone(), rest[1].clone()]).unwrap_err();
                    let subject = if symbol.is_empty() {
                        "TLS section .tls.choice"
                    } else {
                        "TLS symbol tls_value"
                    };
                    assert!(
                        err.contains(subject)
                            && err.contains(
                                "native PT_TLS layout and TLS relocations are unsupported"
                            ),
                        "{err}"
                    );
                }
            }
        }
    }
    let imports = object(
        ".type tls_import,@tls_object\n.section .data.choice,\"awG\",@progbits,key,comdat\n.quad tls_import\n",
    );
    let err = link(&[object(START), group(1, ""), imports]).unwrap_err();
    assert!(
        err.contains("TLS symbol tls_import") && err.contains("external ELF linking"),
        "{err}"
    );
}

#[test]
fn tls_archive_member_is_checked_only_after_extraction() {
    let tls = object(
        ".section .tdata,\"awT\",@progbits\n.globl dormant_tls\n.type dormant_tls,@tls_object\ndormant_tls: .long 1\n",
    );
    let archive = archive(&[("tls.o", &tls)]);
    link(&[object(START), group(1, ""), group(2, ""), archive]).unwrap();

    let tls = object(
        ".text\n.globl pull_tls_member\npull_tls_member: ret\n.section .tls.choice,\"awTG\",@progbits,key,comdat\n.zero 4\n",
    );
    let archive = self::archive(&[("tls.o", &tls)]);
    let entry = object(&format!("{START}.data\n.quad pull_tls_member\n"));
    let err = link(&[entry, group(1, ""), archive]).unwrap_err();
    assert!(
        err.contains("comdat.o(tls.o): TLS section .tls.choice")
            && err.contains("external ELF linking"),
        "{err}"
    );
}
#[test]
fn ordinary_and_cross_group_references_rebind_globals() {
    let refs = object(
        ".data\n.globl from_ordinary\nfrom_ordinary: .quad payload\n.section .data.other,\"awG\",@progbits,other,comdat\n.globl from_group\nfrom_group: .quad payload\n",
    );
    let exe = link(&[object(START), group(7,""), group(8,".text\n.globl in_loser_object\nin_loser_object: ret\n.data\n.globl from_loser\nfrom_loser: .quad payload\n"), refs]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    for n in ["from_ordinary", "from_group", "from_loser"] {
        assert_eq!(word(&exe, n), value(&f, "payload"));
    }
    assert!(value(&f, "in_loser_object") > 0);
}
#[test]
fn discarded_local_reference_is_precisely_diagnosed() {
    let error = link(&[
        object(START),
        group(1, ""),
        group(2, ".data\n.quad local\n"),
    ])
    .unwrap_err();
    assert!(error.contains("discarded COMDAT local"), "{error}");
}
#[test]
fn strong_override_is_independent_of_group_signature() {
    let weak = object(
        ".section .data.weak,\"awG\",@progbits,weak,comdat\n.weak weak\nweak: .quad 1\n.globl companion\ncompanion: .quad 77\n.data\n.globl ref\nref: .quad weak\n",
    );
    let strong = object(".data\n.globl weak\nweak: .quad 2\n");
    for rest in [[weak.clone(), strong.clone()], [strong, weak]] {
        let exe = link(&[object(START), rest[0].clone(), rest[1].clone()]).unwrap();
        assert_eq!(word(&exe, "weak"), 2);
        assert_eq!(word(&exe, "companion"), 77);
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(word(&exe, "ref"), value(&f, "weak"));
    }
}
#[test]
fn clang_cpp_template_inline_data_and_mixed_unwind() {
    let a = include_bytes!("fixtures/comdat/clang-a.o").to_vec();
    let b = include_bytes!("fixtures/comdat/clang-b.o").to_vec();
    for rest in [[a.clone(), b.clone()], [b, a]] {
        let exe = link(&[object(START), rest[0].clone(), rest[1].clone()]).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert!(value(&f, "invoke_a") != value(&f, "invoke_b"));
        assert_eq!(
            f.symbols()
                .filter(|s| s.name().unwrap() == "_Z6answerILi23EEiv")
                .count(),
            1
        );
        assert_eq!(
            f.symbols()
                .filter(|s| s.name().unwrap() == "shared")
                .count(),
            1
        );
        let at = (value(&f, "shared") - 0x400000) as usize;
        assert_eq!(
            u32::from_le_bytes(exe.bytes[at..at + 4].try_into().unwrap()),
            19
        );
        let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
        assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 4);
    }
}
fn archive(objects: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, bytes) in objects {
        out.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                100644,
                bytes.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(bytes);
        if bytes.len() % 2 != 0 {
            out.push(b'\n');
        }
    }
    out
}
#[test]
fn archive_extraction_selects_groups_only_on_extraction_and_ignores_losing_demands() {
    let first = group(3, "");
    let second = group(4, ".quad absent\n.text\n.globl needed\nneeded: ret\n");
    let never = object(
        ".data\n.globl absent\nabsent: .quad duplicate\n.globl payload\npayload: .quad 99\n",
    );
    let demand = object(".text\n.globl _start\n_start: call needed\nret\n");
    let ar = archive(&[("second.o", &second), ("never.o", &never)]);
    let exe = link(&[first, demand, ar]).unwrap();
    assert_eq!(word(&exe, "payload"), 3);
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(f.symbols().all(|s| s.name().unwrap() != "absent"));
    // An unused earlier archive does not reserve signatures.
    let unused = archive(&[("unused.o", &group(9, ""))]);
    assert_eq!(
        word(
            &link(&[unused, object(START), group(7, "")]).unwrap(),
            "payload"
        ),
        7
    );
}
fn u64_at(b: &[u8], p: usize) -> usize {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap()) as usize
}
fn put32(b: &mut [u8], p: usize, v: u32) {
    b[p..p + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], p: usize, v: u64) {
    b[p..p + 8].copy_from_slice(&v.to_le_bytes());
}
#[test]
fn malformed_groups_and_relocation_attachments_are_rejected_even_for_losers() {
    let original = group(1, "");
    let f = object::File::parse(original.as_slice()).unwrap();
    let g = f
        .sections()
        .find(|s| s.name().unwrap() == ".group")
        .unwrap();
    let data = g.file_range().unwrap().0 as usize;
    let header = u64_at(&original, 40) + g.index().0 * 64;
    let member = u32::from_le_bytes(original[data + 4..data + 8].try_into().unwrap()) as usize;
    let member_header = u64_at(&original, 40) + member * 64;
    let rela = f
        .sections()
        .find(|s| s.name().unwrap() == ".rela.data.choice")
        .unwrap();
    let rela_header = u64_at(&original, 40) + rela.index().0 * 64;
    let mut cases = Vec::new();
    for (at, v) in [
        (data, 0),
        (data, 3),
        (data + 4, 0),
        (data + 4, 9999),
        (header + 40, 0),
        (header + 44, 0),
        (header + 44, 9999),
    ] {
        let mut b = original.clone();
        put32(&mut b, at, v);
        cases.push(b);
    }
    for (at, v) in [
        (header + 56, 0),
        (header + 48, 8),
        (header + 32, 5),
        (header + 8, 2),
        (member_header + 8, 6),
        (rela_header + 8, 0),
        (rela_header + 56, 0),
        (rela_header + 32, 23),
    ] {
        let mut b = original.clone();
        put64(&mut b, at, v);
        cases.push(b);
    }
    let mut duplicate_member = original.clone();
    let member_word = original[data + 4..data + 8].to_vec();
    duplicate_member[data + 8..data + 12].copy_from_slice(&member_word);
    cases.push(duplicate_member);
    for b in cases {
        assert!(link(&[object(START), original.clone(), b]).is_err());
    }
}
#[test]
fn local_and_global_signature_bindings_share_identity() {
    let fixture = |global: bool, n| {
        object(&format!(
            ".section .data.sig,\"awG\",@progbits,sig,comdat\n{}sig:\n.globl payload\npayload: .quad {n}\n",
            if global { ".globl sig\n" } else { "" }
        ))
    };
    for global in [false, true] {
        let exe = link(&[object(START), fixture(global, 3), fixture(!global, 4)]).unwrap();
        assert_eq!(word(&exe, "payload"), 3);
    }
}
#[test]
fn grouped_retain_and_custom_name_preserve_abi_flags_and_bounds() {
    let fixture = |n| {
        object(&format!(
            ".section registry,\"awGR\",@progbits,key,comdat\n.globl payload\npayload: .quad {n}\n"
        ))
    };
    let refs = object(
        ".data\n.globl first\nfirst: .quad __start_registry\n.globl last\nlast: .quad __stop_registry\n",
    );
    let exe = link(&[object(START), fixture(1), fixture(2), refs]).unwrap();
    assert_eq!(word(&exe, "last") - word(&exe, "first"), 8);
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(
        matches!(f.section_by_name("registry").unwrap().flags(),object::SectionFlags::Elf { sh_flags } if sh_flags == 0x200003)
    );
}
#[test]
fn undefined_global_signature_is_metadata_not_archive_or_import_demand() {
    let a = object(
        ".globl key\n.section .data.a,\"awG\",@progbits,key,comdat\n.globl payload\npayload: .quad 5\n",
    );
    // The object writer creates an absolute signature anchor; model the valid
    // undefined-signature form explicitly in the raw symbol table.
    let mut a = a;
    let f = object::File::parse(a.as_slice()).unwrap();
    let key = f
        .symbols()
        .find(|s| s.name().unwrap() == "key")
        .unwrap()
        .index()
        .0;
    let symtab = f
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    a[symtab + key * 24 + 6..symtab + key * 24 + 8].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(
        word(&link(&[object(START), a.clone(), a]).unwrap(), "payload"),
        5
    );
}
#[test]
fn losing_lifecycle_and_orphan_cie_personality_do_not_survive() {
    let selected = object(
        ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let losing = object(
        ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice:\n.cfi_startproc\n.cfi_personality 0x9b,absent_personality\n.cfi_lsda 0x1b,absent_lsda\nret\n.cfi_endproc\n.section .init_array.100,\"awG\",@init_array,key,comdat\n.p2align 3\n.quad absent_initializer\n.text\n.globl ordinary\nordinary:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let exe = link(&[object(START), selected, losing]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(f.sections().all(|s| s.name().unwrap() != ".init_array.100"));
    assert!(
        f.symbols()
            .all(|s| !s.name().unwrap().starts_with("absent"))
    );
    let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
}
#[test]
fn selected_grouped_lifecycle_is_retained_once() {
    let fixture = object(
        ".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice: ret\n.section .init_array.100,\"awG\",@init_array,key,comdat\n.p2align 3\n.quad choice\n",
    );
    let exe = link(&[object(START), fixture.clone(), fixture]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let array = f.section_by_name(".init_array.100").unwrap();
    assert_eq!(array.size(), 8);
    assert_eq!(
        u64::from_le_bytes(array.data().unwrap().try_into().unwrap()),
        value(&f, "choice")
    );
}

#[test]
fn gcc_cpp_template_inline_data_and_mixed_unwind() {
    let a = include_bytes!("fixtures/comdat/gcc-a.o").to_vec();
    let b = include_bytes!("fixtures/comdat/gcc-b.o").to_vec();
    for rest in [[a.clone(), b.clone()], [b, a]] {
        let exe = link(&[object(START), rest[0].clone(), rest[1].clone()]).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_ne!(value(&f, "invoke_a"), value(&f, "invoke_b"));
        for name in ["shared", "_Z6answerILi23EEiv", "_Z11read_sharedv"] {
            assert_eq!(f.symbols().filter(|s| s.name().unwrap() == name).count(), 1);
        }
        let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
        assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 4);
    }
}

#[test]
fn different_signatures_do_not_suppress_strong_duplicates() {
    let fixture = |key| {
        object(&format!(
            ".section .data.value,\"awG\",@progbits,{key},comdat\n.globl collision\ncollision: .quad 1\n"
        ))
    };
    assert!(
        link(&[object(START), fixture("one"), fixture("two")])
            .unwrap_err()
            .contains("duplicate definition")
    );
}
#[test]
fn unnamed_section_signature_uses_section_name() {
    let original = group(1, "");
    let mut input = original.clone();
    let f = object::File::parse(input.as_slice()).unwrap();
    let group = f
        .sections()
        .find(|s| s.name().unwrap() == ".group")
        .unwrap();
    let group_header = u64_at(&input, 40) + group.index().0 * 64;
    let section = f.section_by_name(".text.choice").unwrap().index().0;
    let symbol = f
        .symbols()
        .find(|s| {
            s.kind() == object::SymbolKind::Section
                && s.section_index().is_some_and(|i| i.0 == section)
        })
        .unwrap()
        .index()
        .0;
    let symtab = f
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    put32(&mut input, group_header + 44, symbol as u32);
    put32(&mut input, symtab + symbol * 24, 0);
    let exe = link(&[object(START), input.clone(), input]).unwrap();
    assert_eq!(word(&exe, "payload"), 1);
}
#[test]
fn invalid_losing_payload_bounds_are_rejected() {
    let original = group(1, "");
    let mut bad = original.clone();
    let f = object::File::parse(bad.as_slice()).unwrap();
    let member = f.section_by_name(".data.choice").unwrap().index().0;
    let header = u64_at(&bad, 40) + member * 64;
    put64(&mut bad, header + 24, u64::MAX);
    assert!(link(&[object(START), original, bad]).is_err());
}
#[test]
fn exported_anchors_in_rewritten_frames_are_diagnosed_for_external_references() {
    let first = group(1, "");
    let mut second = group(2, "");
    let f = object::File::parse(second.as_slice()).unwrap();
    let frame = f.section_by_name(".eh_frame").unwrap().index().0;
    let payload = f
        .symbols()
        .find(|s| s.name().unwrap() == "payload")
        .unwrap()
        .index()
        .0;
    let symtab = f
        .section_by_name(".symtab")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    second[symtab + payload * 24 + 6..symtab + payload * 24 + 8]
        .copy_from_slice(&(frame as u16).to_le_bytes());
    put64(&mut second, symtab + payload * 24 + 16, 0);
    let error = link(&[object(START), first, second]).unwrap_err();
    assert!(
        error.contains("global symbol in COMDAT-filtered .eh_frame"),
        "{error}"
    );
}

#[test]
fn nobits_members_are_selected_with_initialized_companions() {
    let fixture = |n| {
        group(
            n,
            ".section .bss.choice,\"awG\",@nobits,key,comdat\n.p2align 5\n.globl zero\nzero: .zero 64\n",
        )
    };
    let exe = link(&[object(START), fixture(1), fixture(2)]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let bss: Vec<_> = f
        .sections()
        .filter(|s| s.name().unwrap() == ".bss.choice")
        .collect();
    assert_eq!(bss.len(), 1);
    assert_eq!(bss[0].size(), 64);
    assert_eq!(bss[0].kind(), object::SectionKind::UninitializedData);
    assert_eq!(value(&f, "zero") % 32, 0);
}

#[test]
fn group_section_signature_is_metadata_even_with_global_binding() {
    let input = object(
        ".globl key\n.section .data.sig,\"awG\",@progbits,key,comdat\n.globl payload\npayload: .quad 3\n",
    );
    let exe = link(&[object(START), input.clone(), input]).unwrap();
    assert_eq!(word(&exe, "payload"), 3);
}

#[test]
fn no_op_unwind_relocation_does_not_select_an_fde_for_discard() {
    let first = group(1, "");
    let mut second = group(2, "");
    let f = object::File::parse(second.as_slice()).unwrap();
    let rela = f.section_by_name(".rela.eh_frame").unwrap();
    let at = rela.file_range().unwrap().0 as usize;
    // The sole relocation is the FDE initial location. R_NONE's symbol index
    // has no meaning and must not be used to decide whether this FDE is dead.
    assert_eq!(rela.size(), 24);
    put32(&mut second, at + 8, 0);
    let error = link(&[object(START), first, second]).unwrap_err();
    assert!(error.contains("unwind FDE range"), "{error}");
}

#[test]
fn group_metadata_does_not_satisfy_archive_demand_or_override_real_definitions() {
    let metadata = object(
        ".globl key\n.section .data.sig,\"awG\",@progbits,key,comdat\n.globl payload\npayload: .quad 3\n",
    );
    let demand = object(
        ".text\n.globl _start\n_start: call key\nret\n.data\n.globl lookup\nlookup: .quad key\n",
    );
    let provider = object(".text\n.globl key\nkey: ret\n");
    for tail in [provider.clone(), archive(&[("key.o", &provider)])] {
        let exe = link(&[demand.clone(), metadata.clone(), tail]).unwrap();
        let f = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(word(&exe, "lookup"), value(&f, "key"));
    }
}

#[test]
fn no_op_unwind_relocation_can_share_a_real_pointer_offset() {
    let first = group(1, "");
    let mut second = group(2, "");
    let f = object::File::parse(second.as_slice()).unwrap();
    let rela = f.section_by_name(".rela.eh_frame").unwrap();
    let header = u64_at(&second, 40) + rela.index().0 * 64;
    let mut entries = rela.data().unwrap().to_vec();
    assert_eq!(entries.len(), 24);
    let mut noop = entries.clone();
    // An ignored symbol index, sharing the real initial-location field.
    put64(&mut noop, 8, u64::from(u32::MAX) << 32);
    entries.extend(noop);
    let at = second.len() as u64;
    second.extend(entries);
    put64(&mut second, header + 24, at);
    put64(&mut second, header + 32, 48);
    link(&[object(START), first, second]).unwrap();
}

#[test]
fn invalid_initial_location_encoding_is_not_hidden_by_fde_selection() {
    let first = group(1, "");
    let mut second = group(2, "");
    let f = object::File::parse(second.as_slice()).unwrap();
    let at = f
        .section_by_name(".rela.eh_frame")
        .unwrap()
        .file_range()
        .unwrap()
        .0 as usize;
    // An absolute 64-bit relocation cannot identify this pcrel sdata4 field.
    put32(&mut second, at + 8, 1);
    let error = link(&[object(START), first, second])
        .err()
        .expect("invalid FDE must diagnose");
    assert!(error.contains("pointer encoding"), "{error}");
}

#[test]
fn grouped_abi_notes_and_sframe_diagnose_even_in_losing_groups() {
    for (name, kind) in [
        (".note.gnu.property", "note"),
        (".note.ABI-tag", "note"),
        (".sframe", "progbits"),
    ] {
        let first = group(1, "");
        let second = group(
            2,
            &format!(".section {name},\"aG\",@{kind},key,comdat\n.zero 32\n"),
        );
        assert!(link(&[object(START), first, second]).is_err(), "{name}");
    }
    // An ordinary object property remains a contract even if all its code lost.
    let second = group(
        2,
        ".section .note.gnu.property,\"a\",@note\n.p2align 3\n.long 4,16,5,0x00554e47,0xc0008001,4,1,0\n",
    );
    let error = link(&[object(START), group(1, ""), second]).err().unwrap();
    assert!(error.contains("FEATURE_2_NEEDED"), "{error}");
}

#[test]
fn retained_section_symbol_addends_preserve_offsets_and_losing_ones_diagnose() {
    let tail = ".Lanchor: .quad 9\n.globl adjusted\nadjusted: .quad .Lanchor+8\n";
    let first = group(1, tail);
    let f = object::File::parse(first.as_slice()).unwrap();
    let data = f.section_by_name(".data.choice").unwrap();
    let (offset, reloc) = data.relocations().last().unwrap();
    assert_eq!(offset, 32);
    let object::RelocationTarget::Symbol(i) = reloc.target() else {
        panic!()
    };
    assert_eq!(
        f.symbol_by_index(i).unwrap().kind(),
        object::SymbolKind::Section
    );
    assert_eq!(reloc.addend(), 32);
    let exe = link(&[object(START), first.clone(), group(2, tail)]).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(word(&exe, "adjusted"), value(&f, "payload") + 32);
    let losing = group(2, &format!("{tail}.data\n.quad .Lanchor+8\n"));
    let error = link(&[object(START), first, losing]).err().unwrap();
    assert!(error.contains("discarded COMDAT local"), "{error}");
}
