use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::{Directive, Program};
use object::{Object, ObjectComdat, ObjectSection, ObjectSymbol};
use std::process::Command;

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn entry_size(bytes: &[u8], index: usize) -> u64 {
    u64_at(bytes, u64_at(bytes, 40) as usize + index * 64 + 56)
}
fn group_metadata(bytes: &[u8]) -> Vec<(String, Vec<String>)> {
    let file = object::File::parse(bytes).unwrap();
    let mut groups: Vec<_> = file
        .comdats()
        .map(|group| {
            let mut members: Vec<_> = group
                .sections()
                .map(|i| {
                    file.section_by_index(i)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_string()
                })
                .collect();
            members.sort();
            (group.name().unwrap().to_string(), members)
        })
        .collect();
    groups.sort();
    groups
}

const SOURCE: &str = r#"
.intel_syntax noprefix
.section .text.pick,"axG",@progbits,pick,comdat
.weak pick
.hidden pick
.type pick,@function
pick: mov eax, 42
ret
.size pick,.-pick
.section .rodata.pick,"aMG",@progbits,8,pick,comdat
.quad external
.section .text.other,"axG",%progbits,.Lsignature,comdat
.Lsignature: ret
.section .rodata.str,"aMS",@progbits,1
.asciz "hello"
.section .rodata.wide,"aMS",@progbits,2
.short 65,0
.section .rodata.constants,"aM",@progbits,16
.quad 1,2
.section .data.shared,"awG",@progbits,pick,comdat
.long 7
.section .bss.pick,"awG",@nobits,pick,comdat
.zero 16
.section .note.GNU-stack,"",@progbits
"#;

#[test]
fn merge_entries_groups_and_symbol_metadata() {
    let bytes = assemble_object(SOURCE).unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    for (name, flags, size) in [
        (".rodata.str", 0x32, 1),
        (".rodata.wide", 0x32, 2),
        (".rodata.constants", 0x12, 16),
        (".rodata.pick", 0x212, 8),
    ] {
        let section = file.section_by_name(name).unwrap();
        assert_eq!(
            section.flags(),
            object::SectionFlags::Elf { sh_flags: flags }
        );
        assert_eq!(entry_size(&bytes, section.index().0), size);
    }
    assert_eq!(
        group_metadata(&bytes),
        vec![
            (".Lsignature".into(), vec![".text.other".into()]),
            (
                "pick".into(),
                vec![
                    ".bss.pick".into(),
                    ".data.shared".into(),
                    ".rela.rodata.pick".into(),
                    ".rodata.pick".into(),
                    ".text.pick".into()
                ]
            ),
        ]
    );
    let relocation = file.section_by_name(".rela.rodata.pick").unwrap();
    assert!(
        matches!(relocation.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 0x200 != 0)
    );
    let pick = file.symbols().find(|s| s.name() == Ok("pick")).unwrap();
    assert!(pick.is_weak());
    assert_eq!(pick.kind(), object::SymbolKind::Text);
    assert_eq!(pick.size(), 6);
    assert!(matches!(
        pick.flags(),
        object::SymbolFlags::Elf { st_other: 2, .. }
    ));
    for _ in 0..8 {
        assert_eq!(bytes, assemble_object(SOURCE).unwrap());
    }
}

#[test]
fn sections_with_the_same_name_belong_to_distinct_groups() {
    let bytes = assemble_object(
        r#"
.section .payload,"aG",@progbits,first,comdat
.byte 1
.section .payload,"aG",@progbits,second,comdat
.byte 2
.section .payload,"aG",@progbits,first,comdat
.byte 3
.section .payload,"a"
.byte 4
"#,
    )
    .unwrap();
    let file = object::File::parse(&*bytes).unwrap();
    let data: Vec<_> = file
        .sections()
        .filter(|s| s.name() == Ok(".payload"))
        .map(|s| s.data().unwrap().to_vec())
        .collect();
    assert_eq!(data, vec![vec![1, 3], vec![2], vec![4]]);
    assert_eq!(group_metadata(&bytes).len(), 2);
    let first = file.symbols().find(|s| s.name() == Ok("first")).unwrap();
    let second = file.symbols().find(|s| s.name() == Ok("second")).unwrap();
    assert_ne!(first.section_index(), second.section_index());
    assert_eq!(
        file.section_by_index(first.section_index().unwrap())
            .unwrap()
            .name(),
        Ok(".group")
    );
    assert!(!first.is_global());
}

#[test]
fn typed_sections_match_text_and_reject_invalid_attributes() {
    let mut program = Program::new();
    program.directive(Directive::ElfSectionMetadata {
        name: ".rodata.group".into(),
        flags: 0x232,
        nobits: false,
        entry_size: 2,
        group: Some(".Lsignature".into()),
    });
    program.directive(Directive::Bytes(vec![65, 0, 0, 0]));
    let source = program
        .items
        .iter()
        .map(|item| match item {
            keel_asm_ir::Item::Directive(d) => d.to_string(),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        assemble_program_object(&program).unwrap(),
        assemble_object(&source).unwrap()
    );
    assert!(keel_asm::assemble_program(&program, Default::default()).is_err());
    for (flags, nobits, entry_size, group) in [
        (0x32, false, 0, None),
        (0x22, false, 1, None),
        (2, false, 8, None),
        (0x12, true, 8, None),
        (0x202, false, 0, None),
        (2, false, 0, Some("signature")),
        (0x800, false, 0, None),
    ] {
        let mut program = Program::new();
        program.directive(Directive::ElfSectionMetadata {
            name: ".invalid".into(),
            flags,
            nobits,
            entry_size,
            group: group.map(str::to_string),
        });
        assert!(assemble_program_object(&program).is_err());
    }
}

#[test]
fn malformed_section_diagnostics() {
    for (declaration, diagnostic) in [
        (".section .bad,\"aM\",@progbits", "positive entry size"),
        (".section .bad,\"aM\",@progbits,0", "positive u64"),
        (".section .bad,\"aM\",@progbits,-1", "positive u64"),
        (
            ".section .bad,\"aM\",@progbits,18446744073709551616",
            "positive u64",
        ),
        (
            ".section .bad,\"aM\",@progbits,symbol",
            "invalid merge entry size",
        ),
        (".section .bad,\"aS\",@progbits", "SHF_STRINGS requires"),
        (".section .bad,\"aMS\",@nobits,1", "NOBITS"),
        (".section .bad,\"aG\",@progbits", "missing COMDAT"),
        (".section .bad,\"aG\",@progbits,,comdat", "missing COMDAT"),
        (".section .bad,\"aG\",@progbits,sig", "explicit comdat"),
        (
            ".section .bad,\"aG\",@progbits,sig,discard",
            "unsupported section group selection",
        ),
        (
            ".section .bad,\"a\",@progbits,sig,comdat",
            "unexpected section arguments",
        ),
        (
            ".section .bad,\"aMG\",@progbits,8,sig,comdat,extra",
            "unexpected section arguments",
        ),
        (
            ".section .bad,\"az\",@progbits",
            "unsupported ELF section flag `z`",
        ),
        (".section .bad,\"a\",@group", "unsupported section type"),
        (
            ".section .bad,\"aM\",@progbits,8\n.section .bad,\"aM\",@progbits,4",
            "conflicting section attributes",
        ),
    ] {
        let error = assemble_object(declaration).unwrap_err().to_string();
        assert!(error.contains(diagnostic), "{declaration}: {error}");
    }
}

// Normalize away section/symbol numbering and the reference tools' empty
// default sections; compare actual ELF entry sizes, flags, data and groups.
type SectionMetadata = (String, object::SectionFlags, u64, u64, Vec<u8>);
type SymbolMetadata = (String, u8, u8, Option<String>, u64);
fn normalized(
    bytes: &[u8],
) -> (
    Vec<SectionMetadata>,
    Vec<(String, Vec<String>)>,
    Vec<SymbolMetadata>,
) {
    let file = object::File::parse(bytes).unwrap();
    let sections = file
        .sections()
        .filter(|s| {
            (s.name().unwrap().starts_with(".rodata")
                || s.name().unwrap().starts_with(".rela.rodata"))
                || s.name().unwrap().starts_with(".text.")
                || s.name() == Ok(".data.shared")
                || s.name() == Ok(".bss.pick")
        })
        .map(|s| {
            (
                s.name().unwrap().to_string(),
                match s.flags() {
                    // SHF_INFO_LINK is optional on RELA: GNU/object set it,
                    // while some Clang versions rely on the section type.
                    object::SectionFlags::Elf { sh_flags }
                        if s.name().unwrap().starts_with(".rela") =>
                    {
                        object::SectionFlags::Elf {
                            sh_flags: sh_flags & !0x40,
                        }
                    }
                    flags => flags,
                },
                entry_size(bytes, s.index().0),
                s.size(),
                if s.name().unwrap().starts_with(".rela") {
                    Vec::new()
                } else {
                    s.data().unwrap().to_vec()
                },
            )
        })
        .collect();
    let mut symbols: Vec<_> = file
        .symbols()
        .filter(|s| matches!(s.name(), Ok("pick" | ".Lsignature" | "first" | "second")))
        .map(|s| {
            let object::SymbolFlags::Elf { st_info, st_other } = s.flags() else {
                panic!("not ELF")
            };
            (
                s.name().unwrap().to_string(),
                st_info,
                st_other,
                s.section_index().map(|i| {
                    let owner = file.section_by_index(i).unwrap().name().unwrap();
                    if s.scope() == object::SymbolScope::Compilation {
                        if let Some(group) = file.comdats().find(|group| group.name() == s.name()) {
                            // GNU as anchors local signatures in .group; Clang
                            // may retain a defined label in the member section.
                            // Both identify the same group by the same name.
                            assert!(
                                owner == ".group" || group.sections().any(|member| member == i)
                            );
                            return "<local-group-signature>".into();
                        }
                    }
                    owner.to_owned()
                }),
                s.size(),
            )
        })
        .collect();
    symbols.sort();
    (sections, group_metadata(bytes), symbols)
}

#[test]
fn section_metadata_matches_clang_and_gnu() {
    let temp = std::env::temp_dir().join(format!("keel-x86-sections-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("input.s");
    for source in [
        SOURCE,
        r#"
.section .rodata.payload,"aG",@progbits,first,comdat
.byte 1
.section .rodata.payload,"aG",@progbits,second,comdat
.byte 2
.section .rodata.payload,"aG",@progbits,first,comdat
.byte 3
"#,
        ".hidden first\n.section .rodata.one,\"aG\",@progbits,first,comdat\n.byte 1",
        ".globl first\n.section .rodata.one,\"aG\",@progbits,first,comdat\n.byte 1",
        ".weak first\n.section .rodata.one,\"aG\",@progbits,first,comdat\n.byte 1",
        ".type first,@object\n.section .rodata.one,\"aG\",@progbits,first,comdat\n.byte 1",
    ] {
        std::fs::write(&input, source).unwrap();
        let ours = assemble_object(source).unwrap();
        let mut tools = vec![("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"])];
        if cfg!(target_os = "linux") {
            tools.push(("as", vec!["--64"]));
        }
        for (tool, args) in tools {
            let output = temp.join(format!("{tool}.o"));
            let result = Command::new(tool)
                .args(args)
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{tool}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let reference = std::fs::read(output).unwrap();
            assert_eq!(normalized(&ours), normalized(&reference), "{tool}");
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn same_signature_comdat_coalesces_and_executes() {
    let temp = std::env::temp_dir().join(format!("keel-x86-comdat-link-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    // Strong duplicate definitions must coalesce as a group. Both the function
    // and its companion object (with a relocation) must come from the winner.
    let source = |value| {
        format!(
            r#"
.intel_syntax noprefix
.section .text.pick,"axG",@progbits,pick,comdat
.globl pick
.hidden pick
.type pick,@function
pick: mov eax,[rip+payload]
ret
.size pick,.-pick
.section .data.payload,"awG",@progbits,pick,comdat
.globl payload
.type payload,@object
payload: .long {value}
.globl pointer
pointer: .quad payload
.section .note.GNU-stack,"",@progbits
"#
        )
    };
    let main = temp.join("main.c");
    std::fs::write(&main, "extern int pick(void), payload; extern int *pointer; int main(void) { return pick() != 42 || payload != 42 || pointer != &payload; }").unwrap();
    for (tool, args) in [
        ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
        ("as", vec!["--64"]),
    ] {
        let mut ours = Vec::new();
        let mut refs = Vec::new();
        for (i, value) in [42, 99].into_iter().enumerate() {
            let text = source(value);
            let native = temp.join(format!("ours-{i}.o"));
            std::fs::write(&native, assemble_object(&text).unwrap()).unwrap();
            ours.push(native);
            let input = temp.join(format!("{i}.s"));
            let reference = temp.join(format!("{tool}-{i}.o"));
            std::fs::write(&input, text).unwrap();
            let result = Command::new(tool)
                .args(&args)
                .arg(input)
                .arg("-o")
                .arg(&reference)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            refs.push(reference);
        }
        // Also mix native/reference inputs, exercising both linker directions.
        for (i, objects) in [
            vec![&ours[0], &ours[1]],
            vec![&refs[0], &refs[1]],
            vec![&ours[0], &refs[1]],
            vec![&refs[0], &ours[1]],
        ]
        .into_iter()
        .enumerate()
        {
            let executable = temp.join(format!("{tool}-{i}"));
            let result = Command::new("clang")
                .arg(&main)
                .args(objects)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn mergeable_constants_and_strings_share_storage_after_linking() {
    let temp = std::env::temp_dir().join(format!("keel-x86-merge-link-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let source = |index| {
        format!(
            r#"
.intel_syntax noprefix
.section .rodata.str1.1,"aMS",@progbits,1
.Lstring: .asciz "identical string"
.section .rodata.cst8,"aM",@progbits,8
.p2align 3
.Lconstant: .quad 0x123456789abcdef0
.text
.globl get{index}
.type get{index},@function
get{index}: lea rax,[rip+.Lstring]
ret
.data
.globl pointer{index}
pointer{index}: .quad .Lconstant
.section .note.GNU-stack,"",@progbits
"#
        )
    };
    let driver = temp.join("main.c");
    std::fs::write(&driver, "#include <string.h>\nextern const char *get0(void), *get1(void); extern const unsigned long long *pointer0,*pointer1; int main(void) { return get0()!=get1() || strcmp(get0(),\"identical string\") || pointer0!=pointer1 || *pointer0!=0x123456789abcdef0ULL; }\n").unwrap();
    let mut native = Vec::new();
    let mut reference = Vec::new();
    for index in 0..2 {
        let asm = source(index);
        let obj = temp.join(format!("native-{index}.o"));
        std::fs::write(&obj, assemble_object(&asm).unwrap()).unwrap();
        native.push(obj);
        let input = temp.join(format!("input-{index}.s"));
        let obj = temp.join(format!("reference-{index}.o"));
        std::fs::write(&input, asm).unwrap();
        let output = Command::new("clang")
            .args(["-c"])
            .arg(input)
            .arg("-o")
            .arg(&obj)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        reference.push(obj);
    }
    for (index, objects) in [
        vec![&native[0], &native[1]],
        vec![&reference[0], &reference[1]],
        vec![&native[0], &reference[1]],
        vec![&reference[0], &native[1]],
    ]
    .into_iter()
    .enumerate()
    {
        let exe = temp.join(format!("check-{index}"));
        let output = Command::new("clang")
            .arg(&driver)
            .args(objects)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            Command::new(exe).status().unwrap().success(),
            "link combination {index}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
