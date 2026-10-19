use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SymbolKind};
use std::{collections::BTreeMap, process::Command};

fn header_u32(bytes: &[u8], section: usize, field: usize) -> u32 {
    let shoff = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
    u32::from_le_bytes(
        bytes[shoff + 64 * section + field..shoff + 64 * section + field + 4]
            .try_into()
            .unwrap(),
    )
}
fn addrsig_names(bytes: &[u8]) -> Vec<String> {
    let file = object::File::parse(bytes).unwrap();
    let section = file.section_by_name(".llvm_addrsig").unwrap();
    let mut names = Vec::new();
    let (mut index, mut shift) = (0, 0);
    for &byte in section.data().unwrap() {
        index |= usize::from(byte & 127) << shift;
        if byte & 128 == 0 {
            names.push(
                file.symbol_by_index(object::SymbolIndex(index))
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_string(),
            );
            index = 0;
            shift = 0;
        } else {
            shift += 7;
        }
    }
    names
}

#[test]
fn source_file_ident_and_addrsig_retain_object_semantics() {
    let bytes = assemble_object(
        r#"
.file "translation.cpp"
.text
.globl f
.type f,@function
f: ret
.local object
.comm object,8,8
.ident "compiler\b\f\v\a"
.addrsig
.addrsig_sym f
.addrsig_sym object
.addrsig_sym never_used
.addrsig_sym f
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert!(
        file.symbols()
            .any(|s| s.kind() == SymbolKind::File && s.name() == Ok("translation.cpp"))
    );
    assert_eq!(
        file.section_by_name(".comment").unwrap().data().unwrap(),
        b"\0compiler\x08\x0c\x0b\x07\0"
    );
    let symbol = file.symbols().find(|s| s.name() == Ok("object")).unwrap();
    assert!(symbol.is_local());
    assert_eq!(symbol.size(), 8);
    assert_eq!(
        file.section_by_index(symbol.section_index().unwrap())
            .unwrap()
            .name()
            .unwrap(),
        ".bss"
    );
    assert_eq!(addrsig_names(&bytes), ["f", "object"]);
    let section = file.section_by_name(".llvm_addrsig").unwrap();
    assert_eq!(header_u32(&bytes, section.index().0, 4), 0x6fff4c03);
    assert!(header_u32(&bytes, section.index().0, 40) != 0);
    assert_eq!(
        section.flags(),
        object::SectionFlags::Elf {
            sh_flags: 0x8000_0000
        }
    );
}

#[test]
fn leb_differences_relax_forward_and_backward_and_capture_dot() {
    let bytes = assemble_object(
        r#"
.data
start:
.uleb128 end-start
.zero 127
end:
.sleb128 start-end
.uleb128 .-start
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert_eq!(&data[..2], &[0x81, 1]); // 129, including the two-byte LEB itself
    assert!(data[2..129].iter().all(|&b| b == 0));
    assert_eq!(&data[129..], &[0xff, 0x7e, 0x83, 1]); // -129 and 131
    assert!(assemble_object(".data\na:\n.text\nb:\n.uleb128 b-a").is_err());
    assert!(assemble_object(".uleb128 missing-undefined").is_err());
    assert!(assemble_object(".sleb128 -65\n.uleb128 -1").is_err());
}

#[test]
fn unique_sections_keep_identity_and_array_section_types() {
    let bytes = assemble_object(
        r#"
.section .text.same,"ax",@progbits,unique,1
a: ret
.section .text.same,"ax",@progbits,unique,2
b: nop
.section .text.same,"ax",@progbits,unique,1
ret
.section .init_array,"aw",@init_array
.quad a,b
.section .fini_array,"aw",@fini_array
.quad a
.section .preinit_array,"aw",@preinit_array
.quad b
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let sections: Vec<_> = file
        .sections()
        .filter(|s| s.name() == Ok(".text.same"))
        .collect();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].data().unwrap(), &[0xc3, 0xc3]);
    assert_eq!(sections[1].data().unwrap(), &[0x90]);
    for (name, typ) in [
        (".init_array", 14),
        (".fini_array", 15),
        (".preinit_array", 16),
    ] {
        assert_eq!(
            header_u32(&bytes, file.section_by_name(name).unwrap().index().0, 4),
            typ
        );
    }
    let targets: Vec<_> = file
        .section_by_name(".init_array")
        .unwrap()
        .relocations()
        .map(|(_, r)| {
            let RelocationTarget::Symbol(i) = r.target() else {
                panic!()
            };
            file.symbol_by_index(i).unwrap().section_index().unwrap()
        })
        .collect();
    assert_ne!(targets[0], targets[1]);
    assert!(assemble_object(".section .foo,\"a\",@progbits,unique").is_err());
}

#[test]
fn prefalign_tracks_body_size_and_relaxed_branches() {
    let bytes = assemble_object(
        r#"
.text
.byte 0
.prefalign 4, end, nop
start:
jmp far
.zero 200
far: ret
end:
.data
.quad start,end
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let text = file.section_by_name(".text").unwrap();
    assert_eq!(text.align(), 16);
    assert_eq!(text.data().unwrap()[16], 0xe9);
    assert_eq!(
        file.symbols()
            .find(|s| s.name() == Ok("start"))
            .unwrap()
            .address(),
        16
    );
    assert_eq!(text.size(), 222);
    assert!(assemble_object(".prefalign 4,missing,nop\nret").is_err());
    assert!(assemble_object(".prefalign 4,end,nop\n.data\nend:").is_err());
}

fn relocations(bytes: &[u8], name: &str) -> Vec<(u64, object::RelocationFlags, String, u64, i64)> {
    let file = object::File::parse(bytes).unwrap();
    let mut records: Vec<_> = file
        .section_by_name(name)
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let RelocationTarget::Symbol(i) = r.target() else {
                panic!()
            };
            let s = file.symbol_by_index(i).unwrap();
            let name = if let Some(index) = s.section_index() {
                file.section_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_string()
            } else {
                s.name().unwrap().to_string()
            };
            (offset, r.flags(), name, s.address(), r.addend())
        })
        .collect();
    records.sort_by_key(|r| r.0);
    records
}

#[test]
fn compiler_cfi_and_debug_decode_like_clang() {
    if Command::new("clang").arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("keel-elf-directives-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = r#"
.intel_syntax noprefix
.file "fixture.cpp"
.file 0 "/fixture" "fixture.cpp" md5 0x1234
.file 1 "/include" "other.hpp" md5 0x5678 source "int f();"
.file 2 "<stdin>"
.text
.globl f
.type f,@function
f:
.cfi_startproc
.cfi_personality 155, DW.ref.personality
.cfi_lsda 27, exception
.loc 1 3 2 basic_block discriminator 5
push rbp
.cfi_adjust_cfa_offset 8
.cfi_offset rbp,-16
.loc 1 0 7 is_stmt 0
.p2align 4
.loc 1 4 7 is_stmt 0 prologue_end isa 2
mov rbp,rsp
.cfi_def_cfa_register rbp
.cfi_remember_state
.loc 2 8 1 epilogue_begin
pop rbp
.cfi_def_cfa rsp,8
.cfi_restore_state
.loc 1 9 0 is_stmt 1
ret
.cfi_endproc
.size f,.-f
.globl alias
.set alias,f
.section .gcc_except_table,"a",@progbits
exception: .uleb128 0
.section .data.DW.ref.personality,"awG",@progbits,DW.ref.personality,comdat
.weak DW.ref.personality
.hidden DW.ref.personality
DW.ref.personality: .quad personality
.ident "compiler"
.addrsig
.addrsig_sym f
.addrsig_sym personality
"#;
    let input = dir.join("fixture.s");
    let reference = dir.join("ref.o");
    let candidate = dir.join("keel.o");
    std::fs::write(&input, source).unwrap();
    let result = Command::new("clang")
        .args(["-c", "-target", "x86_64-unknown-linux-gnu"])
        .arg(&input)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = assemble_object(source).unwrap();
    std::fs::write(&candidate, &bytes).unwrap();
    let refbytes = std::fs::read(&reference).unwrap();
    let a = object::File::parse(&*refbytes).unwrap();
    let b = object::File::parse(&*bytes).unwrap();
    for name in [".text", ".comment", ".eh_frame", ".gcc_except_table"] {
        assert_eq!(
            a.section_by_name(name).unwrap().data().unwrap(),
            b.section_by_name(name).unwrap().data().unwrap(),
            "{name}"
        );
        assert_eq!(
            relocations(&refbytes, name),
            relocations(&bytes, name),
            "{name}"
        );
    }
    assert_eq!(addrsig_names(&refbytes), addrsig_names(&bytes));
    let symbols = |f: &object::File<'_>| {
        f.symbols()
            .filter(|s| !matches!(s.kind(), SymbolKind::Section | SymbolKind::File))
            .map(|s| {
                (
                    s.name().unwrap().to_owned(),
                    (s.flags(), s.size(), s.address()),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(symbols(&a), symbols(&b));
    if Command::new("dwarfdump").arg("--version").output().is_ok() {
        let dump = |path: &std::path::Path| {
            let output = Command::new("dwarfdump")
                .arg("--debug-line")
                .arg(path)
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .skip(1)
                .filter(|s| !s.contains("total_length:"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(dump(&reference), dump(&candidate));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn metadata_forms_that_cannot_be_preserved_fail_explicitly() {
    for source in [
        ".cfi_startproc\n.cfi_personality 0,personality\nret\n.cfi_endproc",
        ".cfi_startproc\n.cfi_lsda 16,exception\nret\n.cfi_endproc",
        ".file 0 \"\" \"file\"",
        ".file 1 \"file\"\n.loc 1 3 0 mystery",
        ".section .foo,\"a\",@unrecognized",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}

#[test]
fn weak_common_binding_transitions_preserve_elf_partition() {
    for binding in [
        ".local foo\n.weak foo",
        ".weak foo",
        ".local foo\n.globl foo",
    ] {
        let bytes = assemble_object(&format!("{binding}\n.comm foo,8,8")).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        let foo = file.symbols().find(|s| s.name() == Ok("foo")).unwrap();
        assert_eq!(foo.section(), object::SymbolSection::Common);
        assert_eq!(foo.size(), 8);
        assert_eq!(foo.address(), 8); // SHN_COMMON st_value is the alignment.
        assert!(!foo.is_local());
        assert_eq!(foo.is_weak(), binding.contains(".weak"));
        let symtab = file.section_by_name(".symtab").unwrap();
        let first_nonlocal = header_u32(&bytes, symtab.index().0, 44) as usize;
        for symbol in file.symbols() {
            assert_eq!(symbol.index().0 < first_nonlocal, symbol.is_local());
        }
    }
    for source in [
        ".weak foo\n.local foo\n.comm foo,8,8",
        ".weak foo\n.globl foo\n.comm foo,8,8",
        ".weak foo\n.lcomm foo,8,8",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}

#[test]
fn priority_arrays_infer_type_and_allocated_writable_flags() {
    for (prefix, typ) in [
        (".init_array", 14),
        (".fini_array", 15),
        (".preinit_array", 16),
    ] {
        for suffix in ["", ".00100", ".65535"] {
            for flags in ["", ",\"aw\"", ",\"\""] {
                let name = format!("{prefix}{suffix}");
                let bytes = assemble_object(&format!(".section {name}{flags}\n.quad 0")).unwrap();
                let file = object::File::parse(&*bytes).unwrap();
                let section = file.section_by_name(&name).unwrap();
                assert_eq!(
                    header_u32(&bytes, section.index().0, 4),
                    typ,
                    "{name}{flags}"
                );
                assert_eq!(section.flags(), object::SectionFlags::Elf { sh_flags: 3 });
            }
        }
        let name = format!("{prefix}_unrelated");
        let bytes = assemble_object(&format!(".section {name}\n.quad 0")).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        let section = file.section_by_name(&name).unwrap();
        assert_eq!(header_u32(&bytes, section.index().0, 4), 1);
        assert_eq!(section.flags(), object::SectionFlags::Elf { sh_flags: 0 });
        // Explicit types still override the inferred array type.
        let bytes =
            assemble_object(&format!(".section {prefix}.100,\"aw\",@progbits\n.quad 0")).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        assert_eq!(
            header_u32(
                &bytes,
                file.section_by_name(&format!("{prefix}.100"))
                    .unwrap()
                    .index()
                    .0,
                4
            ),
            1
        );
    }
}

#[test]
fn unsigned_leb_domain_is_checked_after_layout_stabilizes() {
    let bytes = assemble_object(
        ".data\nstart:\n.uleb128 end-start-130\n.uleb128 end-start\n.zero 127\nend:",
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert_eq!(data.len(), 130);
    assert_eq!(&data[..3], &[0, 0x82, 1]);
    // A different LEB widens first, but the unsigned value stays negative.
    let error = assemble_object(
        ".data\nstart:\n.uleb128 end-start-131\n.uleb128 end-start\n.zero 127\nend:",
    )
    .unwrap_err();
    assert!(error.to_string().contains("negative unsigned LEB128"));
    assert!(assemble_object(".set n,-1\n.uleb128 n").is_err());
    // Branch widening can also move a provisional unsigned value into range.
    let bytes =
        assemble_object("start:\n.uleb128 end-start-134\njmp end\n.zero 130\nend: ret").unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        &file.section_by_name(".text").unwrap().data().unwrap()[..2],
        &[2, 0xe9]
    );
}

#[test]
fn long_acyclic_leb_chains_converge_and_actual_cycles_fail() {
    let count = 300;
    let mut source = String::from(".data\n");
    for i in 0..count {
        source.push_str(&format!(
            "s{i}:\n.uleb128 e{}-s{}+126\ne{i}:\n",
            i + 1,
            i + 1
        ));
    }
    source.push_str(&format!("s{count}:\n.uleb128 128\ne{count}:\n"));
    let bytes = assemble_object(&source).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".data").unwrap().data().unwrap(),
        [0x80, 1].repeat(count + 1)
    );
    let error = assemble_object(".byte 0\n.prefalign 4,end,nop\n.p2align 4\nend:").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cyclic variable-length directive layout")
    );
    let error = assemble_object(".data\nstart:\n.uleb128 start-end+129\nend:").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cyclic variable-length directive layout")
    );
}

fn decoded_object(source: &str, option: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("keel-elf-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.o");
    std::fs::write(&path, assemble_object(source).unwrap()).unwrap();
    let output = Command::new("dwarfdump")
        .arg(option)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn emitted_code_data_commits_locations_but_alignment_does_not() {
    for (index, (emission, size)) in [
        (".byte 0x90", 1),
        (".byte end-start", 1),
        (".short 0x9090", 2),
        (".long 0x90909090", 4),
        (".quad 0", 8),
        (".ascii \"x\"", 1),
        (".zero 1", 1),
        (".uleb128 end-start", 1),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            ".file 1 \"/src/x.cpp\"\n.loc 1 1 0\nstart:\n{emission}\nend:\n.loc 1 2 0\nret"
        );
        let dump = decoded_object(&source, "--debug-line", &format!("data-loc-{index}"));
        let rows: Vec<_> = dump
            .lines()
            .filter(|s| s.starts_with("0x"))
            .map(|s| {
                let fields: Vec<_> = s.split_whitespace().collect();
                (
                    u64::from_str_radix(&fields[0][2..], 16).unwrap(),
                    fields[1].parse::<u64>().unwrap(),
                    s.contains("end_sequence"),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [(0, 1, false), (size, 2, false), (size + 1, 2, true)],
            "{source}"
        );
    }
    let dump = decoded_object(
        ".file 1 \"/src/x.cpp\"\n.loc 1 1 0\n.byte 0xc3",
        "--debug-line",
        "only-data-loc",
    );
    assert!(
        dump.lines().any(
            |s| s.starts_with("0x0000000000000000") && s.split_whitespace().nth(1) == Some("1")
        )
    );
    let dump = decoded_object(
        ".file 1 \"/src/x.cpp\"\nnop\n.loc 1 2 0\n.p2align 4\n.byte 0xc3",
        "--debug-line",
        "align-data-loc",
    );
    let row = dump.lines().find(|s| s.starts_with("0x")).unwrap();
    assert_eq!(row.split_whitespace().next(), Some("0x0000000000000010"));
    // Empty emissions do not move a pending location ahead of padding.
    let dump = decoded_object(
        ".file 1 \"/src/x.cpp\"\nnop\n.loc 1 2 0\n.zero 0\n.ascii \"\"\n.p2align 4\nret",
        "--debug-line",
        "empty-data-loc",
    );
    assert_eq!(
        dump.lines()
            .find(|s| s.starts_with("0x"))
            .unwrap()
            .split_whitespace()
            .next(),
        Some("0x0000000000000010")
    );
}

#[test]
fn preferred_nop_fill_is_explicit_in_nonexecutable_sections() {
    for section in [".data", ".section .nonalloc", ".text"] {
        let bytes = assemble_object(&format!(
            "{section}\n.byte 0\n.prefalign 4,end,nop\n.zero 7\nend:"
        ))
        .unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        let name = section.strip_prefix(".section ").unwrap_or(section);
        let data = file.section_by_name(name).unwrap().data().unwrap();
        assert_eq!(&data[..8], &[0, 0x0f, 0x1f, 0x80, 0, 0, 0, 0]);
        assert_eq!(&data[8..], &[0; 7]);
    }
    let bytes =
        assemble_object(".data\n.byte 0\n.p2align 3\n.byte 0\n.prefalign 4,end,42\n.zero 7\nend:")
            .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let data = file.section_by_name(".data").unwrap().data().unwrap();
    assert_eq!(&data[..9], &[0; 9]);
    assert_eq!(&data[9..16], &[42; 7]);
    assert!(assemble_object(".bss\n.zero 1\n.prefalign 4,end,nop\n.zero 7\nend:").is_err());
}

#[test]
fn leb_relocation_modifiers_are_rejected_even_for_absolute_aliases() {
    for directive in [".uleb128", ".sleb128"] {
        for modifier in ["GOTPCREL", "PLT", "TPOFF", "DTPOFF", "GOTTPOFF"] {
            let source = format!(".set foo,128\n{directive} foo@{modifier}");
            let error = assemble_object(&source).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("LEB128 relocation modifiers are unsupported"),
                "{source}: {error}"
            );
        }
        assert!(assemble_object(&format!("{directive} external")).is_err());
    }
    let bytes = assemble_object(".set foo,128\n.uleb128 foo\n.sleb128 foo").unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    assert_eq!(
        file.section_by_name(".text").unwrap().data().unwrap(),
        &[0x80, 1, 0x80, 1]
    );
}

#[test]
fn cfi_adjust_uses_restored_cfa_register_and_offset() {
    let dump = decoded_object(
        ".cfi_startproc\npush rbp\n.cfi_adjust_cfa_offset 8\n.cfi_remember_state\n.cfi_def_cfa rbp,32\n.cfi_remember_state\n.cfi_adjust_cfa_offset 16\nnop\n.cfi_restore_state\n.cfi_adjust_cfa_offset 8\nnop\n.cfi_restore_state\n.cfi_adjust_cfa_offset -8\nret\n.cfi_endproc",
        "--eh-frame",
        "cfi-restored-adjust",
    );
    // Decode the resulting rules: nested restore returns RBP+32 before +8;
    // the outer restore returns RSP+16 before -8. No Clang-byte oracle.
    for row in ["0x1: CFA=RBP+48", "0x2: CFA=RBP+40", "0x3: CFA=RSP+8"] {
        assert!(dump.contains(row), "missing {row}: {dump}");
    }
    assert!(assemble_object(".cfi_startproc\n.cfi_restore_state\nret\n.cfi_endproc").is_err());
}
