use keel_link::{
    Input,
    archive::{self, Target},
};
use object::read::archive::ArchiveFile;

fn object(target: Target, source: &str) -> Vec<u8> {
    match target {
        Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(source).unwrap(),
        Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(source).unwrap(),
        Target::Aarch64AppleDarwin => keel_asm::assemble(source).unwrap().to_macho().unwrap(),
    }
}

#[test]
fn deterministic_index_long_names_duplicates_and_payloads() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        let bytes = object(target, ".text\n.globl exported\nexported:\nret\n");
        let names = [
            "a/same.o",
            "b/same.o",
            "a very long archive member name.o",
            "odd",
            "é.o",
            "#1",
        ];
        let inputs = names
            .iter()
            .map(|name| Input {
                name,
                bytes: &bytes,
            })
            .collect::<Vec<_>>();
        let result = archive::build(&inputs, target).unwrap();
        assert_eq!(result, archive::build(&inputs, target).unwrap());
        let file = ArchiveFile::parse(result.as_slice()).unwrap();
        let members = file.members().map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(members.len(), names.len());
        for (i, member) in members.iter().enumerate() {
            assert_eq!(
                member.name(),
                names[i].rsplit('/').next().unwrap().as_bytes()
            );
            assert_eq!(member.data(result.as_slice()).unwrap(), bytes);
            if target == Target::Aarch64AppleDarwin {
                assert_eq!(member.file_range().0 % 8, 0);
            }
        }
        let symbols = file
            .symbols()
            .unwrap()
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), names.len());
        for (i, symbol) in symbols.iter().enumerate() {
            assert_eq!(symbol.name(), b"exported");
            let member = file.member(symbol.offset()).unwrap();
            assert_eq!(member.file_range(), members[i].file_range());
        }
    }
}

#[test]
fn indexes_include_weak_common_absolute_but_not_undefined_or_local() {
    let bytes = object(
        Target::X86_64LinuxGnu,
        ".text\n.globl strong\nstrong:\nret\n.weak weak\nweak:\nret\n.local local\nlocal:\nret\n.globl undefined\n.quad undefined\n.comm common,8,8\n.globl absolute\n.set absolute,7\n",
    );
    let result = archive::build(
        &[Input {
            name: "symbols.o",
            bytes: &bytes,
        }],
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let file = ArchiveFile::parse(result.as_slice()).unwrap();
    let names = file
        .symbols()
        .unwrap()
        .unwrap()
        .map(|s| s.unwrap().name().to_vec())
        .collect::<Vec<_>>();
    for name in [b"strong".as_slice(), b"weak", b"common", b"absolute"] {
        assert!(names.iter().any(|n| n == name), "{names:?}");
    }
    assert!(!names.iter().any(|n| n == b"undefined" || n == b"local"));
}

#[test]
fn extraction_is_transitive_and_unused_conflicts_stay_unused() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        let (entry, first, second) = if target == Target::X86_64LinuxGnu {
            (
                ".text\n.globl _start\n_start:\ncall first\nmov edi,eax\nmov eax,60\nsyscall\n",
                ".text\n.globl first\nfirst:\njmp second\n",
                ".text\n.globl second\nsecond:\nmov eax,42\nret\n",
            )
        } else {
            (
                ".text\n.globl _main\n_main:\nb first\n",
                ".text\n.globl first\nfirst:\nb second\n",
                ".text\n.globl second\nsecond:\nmov w0,#42\nret\n",
            )
        };
        let entry = object(target, entry);
        let first = object(target, first);
        let second = object(target, second);
        let archive = archive::build(
            &[
                Input {
                    name: "same.o",
                    bytes: &second,
                },
                Input {
                    name: "same.o",
                    bytes: &entry,
                },
                Input {
                    name: "same.o",
                    bytes: &first,
                },
            ],
            target,
        )
        .unwrap();
        let inputs = [
            Input {
                name: "entry.o",
                bytes: &entry,
            },
            Input {
                name: "libtest.a",
                bytes: &archive,
            },
        ];
        if target == Target::X86_64LinuxGnu {
            keel_link::elf::link(&inputs, Default::default()).unwrap();
        } else {
            keel_link::link(
                &inputs,
                keel_link::Options {
                    dylibs: vec![keel_link::Dylib {
                        install_name: "/usr/lib/libSystem.B.dylib".into(),
                        exports: Default::default(),
                        weak_exports: Default::default(),
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }
}

#[test]
fn malformed_wrong_target_and_atomic_failure_preserve_output() {
    let target = Target::X86_64LinuxGnu;
    let bytes = object(target, ".text\n.globl x\nx:\nret\n");
    for n in 0..bytes.len() {
        assert!(
            archive::build(
                &[Input {
                    name: "bad.o",
                    bytes: &bytes[..n]
                }],
                target
            )
            .is_err(),
            "truncation {n}"
        );
    }
    assert!(
        archive::build(
            &[Input {
                name: "wrong.o",
                bytes: &bytes
            }],
            Target::Aarch64AppleDarwin
        )
        .is_err()
    );
    let dir = std::env::temp_dir().join(format!("keel-archive-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("keep.a");
    std::fs::write(&output, b"preserve").unwrap();
    assert!(
        archive::write(
            &[Input {
                name: "bad.o",
                bytes: b"bad"
            }],
            target,
            &output
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"preserve");
    archive::write(
        &[Input {
            name: "x.o",
            bytes: &bytes,
        }],
        target,
        &output,
    )
    .unwrap();
    assert!(std::fs::read(&output).unwrap().starts_with(b"!<arch>\n"));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let directory_output = dir.join("directory.a");
    std::fs::create_dir(&directory_output).unwrap();
    assert!(
        archive::write(
            &[Input {
                name: "x.o",
                bytes: &bytes
            }],
            target,
            &directory_output
        )
        .is_err()
    );
    assert!(directory_output.is_dir());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn odd_payload_padding_empty_indexes_and_malformed_macho() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        let mut bytes = object(target, ".text\nret\n");
        if bytes.len().is_multiple_of(2) {
            bytes.push(0x7f);
        }
        let inputs = [
            Input {
                name: "odd.o",
                bytes: &bytes,
            },
            Input {
                name: "next.o",
                bytes: &bytes,
            },
        ];
        let result = archive::build(&inputs, target).unwrap();
        let file = ArchiveFile::parse(result.as_slice()).unwrap();
        assert_eq!(file.symbols().unwrap().unwrap().count(), 0);
        for member in file.members() {
            assert_eq!(member.unwrap().data(result.as_slice()).unwrap(), bytes);
        }
        let empty = archive::build(&[], target).unwrap();
        let file = ArchiveFile::parse(empty.as_slice()).unwrap();
        assert_eq!(file.members().count(), 0);
        assert_eq!(file.symbols().unwrap().unwrap().count(), 0);
    }
    let target = Target::Aarch64AppleDarwin;
    let bytes = object(target, ".text\n.globl _foo\n_foo:\nbl _undefined\nret\n");
    for n in 0..bytes.len() {
        assert!(
            archive::build(
                &[Input {
                    name: "bad.o",
                    bytes: &bytes[..n]
                }],
                target
            )
            .is_err(),
            "Mach-O truncation {n}"
        );
    }
    let mut bad = bytes.clone();
    bad[36..40].copy_from_slice(&0xfffffff8u32.to_le_bytes()); // first load command size
    assert!(
        archive::build(
            &[Input {
                name: "bad.o",
                bytes: &bad
            }],
            target
        )
        .is_err()
    );
    let mut bad = bytes;
    bad[160..164].copy_from_slice(&u32::MAX.to_le_bytes()); // first section relocation offset
    assert!(
        archive::build(
            &[Input {
                name: "bad.o",
                bytes: &bad
            }],
            target
        )
        .is_err()
    );
}

#[test]
fn darwin_common_and_weak_index_entries() {
    // Mach-O weak definition flag is part of n_desc. Keel's assembler emits the
    // remaining fixture; mutate only this flag to avoid requiring a host tool.
    let target = Target::Aarch64AppleDarwin;
    let mut bytes = object(
        target,
        ".text\n.globl _weak\n_weak:\nret\n.comm _common,8,3\n.globl _undefined\n.quad _undefined\n",
    );
    let mut at = 32;
    let mut symoff = None;
    while at + 24 <= bytes.len() {
        let command = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if command == 2 {
            symoff = Some(u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize);
            break;
        }
        at += u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let weak = file
        .symbols()
        .find(|s| s.name().unwrap() == "_weak")
        .unwrap()
        .index()
        .0;
    let desc = symoff.unwrap() + weak * 16 + 6;
    bytes[desc] |= 0x80;
    let result = archive::build(
        &[Input {
            name: "symbols.o",
            bytes: &bytes,
        }],
        target,
    )
    .unwrap();
    let file = ArchiveFile::parse(result.as_slice()).unwrap();
    let names = file
        .symbols()
        .unwrap()
        .unwrap()
        .map(|s| s.unwrap().name().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2);
    assert!(names.iter().any(|n| n == b"_weak"));
    assert!(names.iter().any(|n| n == b"_common"));
}

include!("fixtures/archive_validation.rs");

#[test]
fn raw_symbol_and_relocation_validation_preserves_output() {
    let elf = object(
        Target::X86_64LinuxGnu,
        ".text\n.globl exported\nexported:\ncall missing\nret\n",
    );
    let macho = object(
        Target::Aarch64AppleDarwin,
        ".text\n.globl exported\nexported:\nbl missing\nret\n",
    );
    let dir = std::env::temp_dir().join(format!("keel-archive-raw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("keep.a");
    for (name, darwin, bytes) in malformed_cases(&elf, &macho) {
        let target = if darwin {
            Target::Aarch64AppleDarwin
        } else {
            Target::X86_64LinuxGnu
        };
        let input = [Input {
            name,
            bytes: &bytes,
        }];
        assert!(archive::build(&input, target).is_err(), "{name}");
        std::fs::write(&output, b"preserve").unwrap();
        assert!(archive::write(&input, target, &output).is_err(), "{name}");
        assert_eq!(std::fs::read(&output).unwrap(), b"preserve", "{name}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn extended_indices_and_packaging_only_relocations_are_valid() {
    let elf = object(
        Target::X86_64LinuxGnu,
        ".text\n.globl exported\nexported:\ncall missing\nret\n",
    );
    let macho = object(
        Target::Aarch64AppleDarwin,
        ".text\n.globl exported\nexported:\nbl missing\nret\n",
    );
    let (extended, _, _) = extended_elf(&elf);
    let mut elf_tls = elf.clone();
    let (_, _, rel, _) = elf_tables(&elf);
    put32(&mut elf_tls, u64_at(&elf, rel + 24) + 8, 19); // R_X86_64_TLSGD
    let mut large_common = elf.clone();
    let (_, _, _, definition) = elf_tables(&elf);
    large_common[definition + 6..definition + 8].copy_from_slice(&0xff02u16.to_le_bytes());
    let mut addend = macho.clone();
    let (section, symbol, rel) = macho_tables(&macho);
    let new_rel = addend.len().next_multiple_of(4);
    addend.resize(new_rel + 16, 0);
    addend[new_rel..new_rel + 8].copy_from_slice(&macho[rel..rel + 8]);
    put32(&mut addend, new_rel + 4, 0xa4ffffff); // ADDEND -1, followed by BRANCH26
    addend[new_rel + 8..new_rel + 16].copy_from_slice(&macho[rel..rel + 8]);
    put32(&mut addend, section + 56, new_rel);
    put32(&mut addend, section + 60, 2);
    let mut absolute = macho.clone();
    absolute[symbol + 4] = 3; // N_ABS | N_EXT
    absolute[symbol + 5] = 0;
    put32(&mut absolute, rel + 4, 0x06000000); // UNSIGNED, local R_ABS, 8 bytes
    let mut indirect = macho.clone();
    indirect[symbol + 4] = 0x0b; // N_INDR | N_EXT; n_value is target string offset
    indirect[symbol + 5] = 0;
    put64(&mut indirect, symbol + 8, u32_at(&macho, symbol));
    let round2 = round2_valid_cases(&elf, &macho);
    for (bytes, target) in [
        (&extended, Target::X86_64LinuxGnu),
        (&elf_tls, Target::X86_64LinuxGnu),
        (&large_common, Target::X86_64LinuxGnu),
        (&addend, Target::Aarch64AppleDarwin),
        (&absolute, Target::Aarch64AppleDarwin),
        (&indirect, Target::Aarch64AppleDarwin),
    ]
    .into_iter()
    .chain(round2.iter().map(|(_, darwin, bytes)| {
        (
            bytes,
            if *darwin {
                Target::Aarch64AppleDarwin
            } else {
                Target::X86_64LinuxGnu
            },
        )
    })) {
        let result = archive::build(
            &[Input {
                name: "valid.o",
                bytes,
            }],
            target,
        )
        .unwrap();
        let file = ArchiveFile::parse(result.as_slice()).unwrap();
        let symbols = file
            .symbols()
            .unwrap()
            .unwrap()
            .map(|s| s.unwrap().name().to_vec())
            .collect::<Vec<_>>();
        assert_eq!(symbols, [b"exported"]);
        assert_eq!(
            file.members()
                .next()
                .unwrap()
                .unwrap()
                .data(result.as_slice())
                .unwrap(),
            bytes
        );
    }
}
