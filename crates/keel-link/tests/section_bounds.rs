//! Raw assembler coverage deliberately independent of C section attributes.
use keel_link::{Dylib, Input, Options, elf};
use object::{Object, ObjectSection, ObjectSymbol};
use std::collections::BTreeSet;

fn elf_obj(s: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(s).unwrap()
}
fn macho_obj(s: &str) -> Vec<u8> {
    keel_asm::assemble(s).unwrap().to_macho().unwrap()
}
fn inputs(objects: &[Vec<u8>]) -> Vec<Input<'_>> {
    objects
        .iter()
        .map(|bytes| Input {
            name: "bounds.o",
            bytes,
        })
        .collect()
}
fn elf_link(objects: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    elf::link(&inputs(objects), Default::default()).map(|e| e.bytes)
}
fn macho_link(objects: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    keel_link::link(
        &inputs(objects),
        Options {
            dylibs: vec![Dylib {
                install_name: "/usr/lib/libSystem.B.dylib".into(),
                exports: BTreeSet::new(),
                weak_exports: BTreeSet::new(),
            }],
            ..Default::default()
        },
    )
    .map(|e| e.bytes)
}
fn value(f: &object::File<'_>, name: &str) -> u64 {
    f.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}
fn pointer(f: &object::File<'_>, name: &str) -> u64 {
    let addr = value(f, name);
    let s = f
        .sections()
        .find(|s| addr >= s.address() && addr + 8 <= s.address() + s.size())
        .unwrap();
    let at = (addr - s.address()) as usize;
    u64::from_le_bytes(s.data().unwrap()[at..at + 8].try_into().unwrap())
}
fn archive(member: Vec<u8>) -> Vec<u8> {
    let mut bytes = b"!<arch>\n".to_vec();
    bytes.extend_from_slice(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "unused.o/",
            0,
            0,
            0,
            "100644",
            member.len()
        )
        .as_bytes(),
    );
    bytes.extend(member);
    if !bytes.len().is_multiple_of(2) {
        bytes.push(b'\n');
    }
    bytes
}

#[test]
fn elf_interleaved_contributions_keep_identity_alignment_and_relocations() {
    let a = elf_obj(
        ".text\n.globl _start\n_start: lea rax,[rip+__start_registry]\nmov rax,qword ptr [rip+__stop_registry@GOTPCREL]\nret\n.section registry,\"aw\",@progbits\n.globl first\nfirst: .quad target\n.byte 7\n.data\n.globl begin,end,target\nbegin: .quad __start_registry\nend: .quad __stop_registry\ntarget: .quad 42\n",
    );
    let middle = elf_obj(".data\n.globl unrelated\nunrelated: .quad 99\n");
    let b = elf_obj(
        ".section registry,\"aw\",@progbits\n.p2align 5\n.globl last\nlast: .quad first\n.byte 8\n",
    );
    let image = elf_link(&[a, middle, b]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    let begin = pointer(&f, "begin");
    let end = pointer(&f, "end");
    assert_eq!(begin, value(&f, "first"));
    assert_eq!(value(&f, "last") % 32, 0);
    assert_eq!(end, value(&f, "last") + 9); // no final alignment rounding
    assert_eq!(pointer(&f, "first"), value(&f, "target"));
    assert_eq!(pointer(&f, "last"), begin);
    assert!(value(&f, "unrelated") >= end);
    assert_eq!(
        f.sections()
            .filter(|s| s.name().unwrap() == "registry")
            .count(),
        2
    );
    assert_eq!(
        u64::from_le_bytes(
            f.section_by_name(".got").unwrap().data().unwrap()[..8]
                .try_into()
                .unwrap()
        ),
        end
    );
    for name in ["__start_registry", "__stop_registry"] {
        assert!(matches!(
            f.symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap()
                .flags(),
            object::SymbolFlags::Elf { st_other: 2, .. }
        ));
    }
}

#[test]
fn elf_empty_missing_weak_overrides_and_archive_selection() {
    let main = ".text\n.globl _start\n_start: ret\n.data\n.globl begin,end\nbegin: .quad __start_registry\nend: .quad __stop_registry\n";
    let empty = elf_obj(&format!("{main}.section registry,\"aw\",@progbits\n"));
    let image = elf_link(&[empty]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "begin"), pointer(&f, "end"));
    assert!(
        elf_link(&[elf_obj(main)])
            .unwrap_err()
            .contains("undefined symbol")
    );
    let weak = elf_obj(&format!("{main}.weak __start_registry,__stop_registry\n"));
    let member = archive(elf_obj(
        ".section registry,\"aw\",@progbits\n.globl registration_only\nregistration_only: .quad 1\n",
    ));
    let image = elf_link(&[weak.clone(), member.clone()]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "begin"), 0);
    assert_eq!(pointer(&f, "end"), 0);
    assert!(
        !f.symbols()
            .any(|s| s.name().unwrap() == "registration_only")
    );
    assert!(
        elf_link(&[elf_obj(main), member])
            .unwrap_err()
            .contains("undefined symbol")
    );
    let image = elf_link(&[
        weak,
        elf_obj(".section registry,\"aw\",@progbits\n.quad 1\n"),
    ])
    .unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "end") - pointer(&f, "begin"), 8);
    for definition in [".globl", ".weak"] {
        let image = elf_link(&[elf_obj(&format!("{main}.section registry,\"aw\",@progbits\n.quad 1\n{definition} __start_registry\n__start_registry: .quad 2\n"))]).unwrap();
        let f = object::File::parse(image.as_slice()).unwrap();
        assert_eq!(pointer(&f, "begin"), value(&f, "__start_registry"));
        assert_eq!(pointer(&f, "end") - pointer(&f, "begin"), 8);
    }
}

#[test]
fn elf_rejects_incompatible_groups_and_does_not_bound_dotted_names() {
    let main =
        elf_obj(".text\n.globl _start\n_start: ret\n.section registry,\"aw\",@progbits\n.quad 1\n");
    assert!(
        elf_link(&[
            main,
            elf_obj(".section registry,\"a\",@progbits\n.quad 1\n")
        ])
        .unwrap_err()
        .contains("incompatible registration")
    );
    assert!(elf_link(&[elf_obj(".text\n.globl _start\n_start: ret\n.data\n.quad __start_.registry\n.section .registry,\"aw\",@progbits\n.quad 1\n")]).unwrap_err().contains("undefined symbol"));
}

#[test]
fn macho_contributions_deferred_bounds_and_data_const_aliases() {
    let a = macho_obj(
        ".text\n.globl _main\n_main:\nadrp x0, section$start$__DATA_CONST$registry@PAGE\nadd x0,x0,section$start$__DATA_CONST$registry@PAGEOFF\nadrp x1, section$end$__DATA$registry@GOTPAGE\nldr x1,[x1,section$end$__DATA$registry@GOTPAGEOFF]\nmov w0,#0\nret\n.section __DATA_CONST,registry\n.globl _first\n_first: .quad _target\n.byte 7\n.data\n.p2align 3\n.globl _begin,_end,_target\n_begin: .quad section$start$__DATA_CONST$registry\n_end: .quad section$end$__DATA$registry\n_target: .quad 42\n",
    );
    let b = macho_obj(
        ".section __DATA,registry\n.p2align 5\n.globl _last\n_last: .quad _first\n.byte 8\n",
    );
    let image = macho_link(&[a, b]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    let section = f.section_by_name("registry").unwrap();
    assert_eq!(pointer(&f, "_begin"), section.address());
    assert_eq!(pointer(&f, "_end"), section.address() + section.size());
    assert_eq!(section.size(), 41);
    assert_eq!(value(&f, "_last") % 32, 0);
    assert_eq!(pointer(&f, "_first"), value(&f, "_target"));
    assert_eq!(pointer(&f, "_last"), value(&f, "_first"));
    for name in [
        "section$start$__DATA_CONST$registry",
        "section$end$__DATA$registry",
    ] {
        let symbol = f.symbols().find(|s| s.name().unwrap() == name).unwrap();
        assert_eq!(symbol.scope(), object::SymbolScope::Linkage);
    }
}

#[test]
fn macho_empty_missing_overrides_weak_and_archive_selection() {
    let main = ".text\n.globl _main\n_main: mov w0,#0\nret\n.data\n.globl _begin,_end\n_begin: .quad section$start$__DATA$registry\n_end: .quad section$end$__DATA$registry\n";
    let unused = archive(macho_obj(
        ".section __DATA,registry\n.globl _registration_only\n_registration_only: .quad 1\n",
    ));
    for extra in [
        "",
        ".section __DATA,registry\n",
        ".weak_reference section$start$__DATA$registry\n.weak_reference section$end$__DATA$registry\n",
    ] {
        let image = macho_link(&[macho_obj(&format!("{main}{extra}")), unused.clone()]).unwrap();
        let f = object::File::parse(image.as_slice()).unwrap();
        assert_eq!(pointer(&f, "_begin"), pointer(&f, "_end"));
        assert!(
            !f.symbols()
                .any(|s| s.name().unwrap() == "_registration_only")
        );
        assert_eq!(f.imports().unwrap().len(), 0);
    }
    for definition in [".globl", ".weak_definition"] {
        let image = macho_link(&[macho_obj(&format!("{main}.section __DATA,registry\n.quad 1\n.globl section$start$__DATA$registry\n{definition} section$start$__DATA$registry\nsection$start$__DATA$registry: .quad 2\n"))]).unwrap();
        let f = object::File::parse(image.as_slice()).unwrap();
        assert_eq!(
            pointer(&f, "_begin"),
            value(&f, "section$start$__DATA$registry")
        );
        assert_eq!(pointer(&f, "_end") - pointer(&f, "_begin"), 8);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_arm64_and_apple_execute_same_registration_objects() {
    let temp =
        std::env::temp_dir().join(format!("keel-registration-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let check = ".text\n.globl _main\n_main:\nadrp x8,section$start$__DATA$registry@PAGE\nadd x8,x8,section$start$__DATA$registry@PAGEOFF\nadrp x9,section$end$__DATA$registry@GOTPAGE\nldr x9,[x9,section$end$__DATA$registry@GOTPAGEOFF]\nsub x10,x9,x8\ncmp x10,#41\nb.ne Lfail\nldr x10,[x8]\nadrp x11,_target@PAGE\nadd x11,x11,_target@PAGEOFF\ncmp x10,x11\nb.ne Lfail\nldr x10,[x8,#32]\ncmp x10,x8\nb.ne Lfail\nadrp x11,_begin@PAGE\nldr x10,[x11,_begin@PAGEOFF]\ncmp x10,x8\nb.ne Lfail\nadrp x11,_end@PAGE\nldr x10,[x11,_end@PAGEOFF]\ncmp x10,x9\nb.ne Lfail\nmov w0,#42\nret\nLfail: mov w0,#1\nret\n.section __DATA,registry\n.globl _first\n_first: .quad _target\n.byte 7\n.data\n.p2align 3\n.globl _begin,_end,_target\n_begin: .quad section$start$__DATA$registry\n_end: .quad section$end$__DATA$registry\n_target: .quad 42\n";
    let missing = ".text\n.globl _main\n_main:\nadrp x8,section$start$__DATA$missing@GOTPAGE\nldr x8,[x8,section$start$__DATA$missing@GOTPAGEOFF]\nadrp x9,section$end$__DATA$missing@GOTPAGE\nldr x9,[x9,section$end$__DATA$missing@GOTPAGEOFF]\nsub x0,x9,x8\nret\n";
    let cases = [
        (
            vec![macho_obj(
                ".section __TEXT,custom,regular,pure_instructions\n.globl _main\n_main: mov w0,#42\nret\n",
            )],
            42,
        ),
        (
            vec![
                macho_obj(check),
                macho_obj(".data\n.quad 99\n"),
                macho_obj(".section __DATA,registry\n.p2align 5\n.quad _first\n.byte 8\n"),
            ],
            42,
        ),
        (
            vec![macho_obj(&format!(
                "{missing}.section __DATA,missing\n.quad 1\n.globl section$start$__DATA$missing\nsection$start$__DATA$missing: .quad 2\n"
            ))],
            8,
        ),
        (
            vec![macho_obj(&format!(
                "{missing}.section __DATA,missing\n.quad 1\n.globl section$start$__DATA$missing\n.weak_definition section$start$__DATA$missing\nsection$start$__DATA$missing: .quad 2\n"
            ))],
            8,
        ),
        (vec![macho_obj(missing)], 0),
        (
            vec![macho_obj(&format!("{missing}.section __DATA,missing\n"))],
            0,
        ),
        (
            vec![macho_obj(&format!(
                "{missing}.weak_reference section$start$__DATA$missing\n.weak_reference section$end$__DATA$missing\n"
            ))],
            0,
        ),
    ];
    for (i, (objects, expected)) in cases.into_iter().enumerate() {
        let native = temp.join(format!("{i}-native"));
        keel_link::link_signed(
            &inputs(&objects),
            Options {
                dylibs: vec![Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: BTreeSet::new(),
                    weak_exports: BTreeSet::new(),
                }],
                ..Default::default()
            },
            &native,
        )
        .unwrap();
        let system = temp.join(format!("{i}-apple"));
        let mut command = std::process::Command::new("clang");
        for (j, bytes) in objects.iter().enumerate() {
            let path = temp.join(format!("{i}-{j}.o"));
            std::fs::write(&path, bytes).unwrap();
            command.arg(path);
        }
        let result = command.arg("-o").arg(&system).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        for exe in [&native, &system] {
            assert_eq!(
                std::process::Command::new(exe).status().unwrap().code(),
                Some(expected),
                "{}",
                exe.display()
            );
            let exports = std::process::Command::new("dyld_info")
                .arg("-exports")
                .arg(exe)
                .output()
                .unwrap();
            assert!(exports.status.success());
            if expected != 8 {
                // User definitions retain their own export visibility.
                assert!(!String::from_utf8_lossy(&exports.stdout).contains("section$"));
            }
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn clang_elf_objects_match_system_linker_bounds_policy() {
    // ELF object/image comparison on Darwin too; execution is separately gated.
    let linker = std::env::var_os("KEEL_TEST_ELF_LD")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let out = std::process::Command::new("rustc")
                .args(["--print", "sysroot"])
                .output()
                .ok()?;
            let root = std::path::PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
            let host = std::process::Command::new("rustc")
                .arg("-vV")
                .output()
                .ok()?;
            let version = String::from_utf8(host.stdout).ok()?;
            let host = version.lines().find_map(|s| s.strip_prefix("host: "))?;
            let path = root.join("lib/rustlib").join(host).join("bin/rust-lld");
            path.is_file().then_some(path)
        });
    let Some(linker) = linker else {
        eprintln!("ELF system comparison skipped: set KEEL_TEST_ELF_LD to GNU ld or ld.lld");
        return;
    };
    let temp = std::env::temp_dir().join(format!(
        "keel-elf-registration-bounds-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let common = ".intel_syntax noprefix\n.text\n.globl _start\n_start: lea rax,[rip+__start_registry]\nret\n.data\n.globl begin,end,target\nbegin: .quad __start_registry\nend: .quad __stop_registry\ntarget: .quad 42\n";
    let cases = [
        (vec![format!("{common}.section registry,\"aw\",@progbits\n.globl first\nfirst: .quad target\n.byte 7\n"), ".data\n.quad 99\n".into(), ".section registry,\"aw\",@progbits\n.p2align 5\n.globl last\nlast: .quad first\n.byte 8\n".into()], Some(41)),
        (vec![format!("{common}.section registry,\"aw\",@progbits\n")], Some(0)),
        (vec![common.into()], None),
        (vec![format!("{common}.weak __start_registry,__stop_registry\n")], Some(0)),
        (vec![format!("{common}.section registry,\"aw\",@progbits\n.quad 1\n.globl __start_registry\n__start_registry: .quad 2\n")], Some(8)),
        (vec![format!("{common}.section registry,\"aw\",@progbits\n.quad 1\n.weak __start_registry\n__start_registry: .quad 2\n")], Some(8)),
    ];
    for (i, (sources, span)) in cases.into_iter().enumerate() {
        let mut objects = Vec::new();
        let mut command = std::process::Command::new(&linker);
        if linker.file_name().unwrap() == "rust-lld" {
            command.args(["-flavor", "gnu"]);
        }
        command.arg("-m").arg("elf_x86_64");
        for (j, source) in sources.into_iter().enumerate() {
            let path = temp.join(format!("{i}-{j}.s"));
            let object = path.with_extension("o");
            std::fs::write(&path, source).unwrap();
            let out = std::process::Command::new("clang")
                .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                .arg(&path)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            objects.push(std::fs::read(&object).unwrap());
            command.arg(object);
        }
        let system = temp.join(format!("{i}-system"));
        let out = command.arg("-o").arg(&system).output().unwrap();
        let native = elf_link(&objects);
        if let Some(span) = span {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            for image in [native.unwrap(), std::fs::read(system).unwrap()] {
                let f = object::File::parse(image.as_slice()).unwrap();
                assert_eq!(pointer(&f, "end") - pointer(&f, "begin"), span);
                if span == 41 {
                    assert_eq!(pointer(&f, "begin") % 32, 0);
                    assert_eq!(pointer(&f, "first"), value(&f, "target"));
                    assert_eq!(pointer(&f, "last"), value(&f, "first"));
                }
            }
        } else {
            assert!(!out.status.success());
            assert!(native.unwrap_err().contains("undefined symbol"));
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn elf_nobits_spans_and_trailing_empty_alignment_are_bounded() {
    let main = elf_obj(
        ".text\n.globl _start\n_start: ret\n.data\n.globl begin,end\nbegin: .quad __start_registry\nend: .quad __stop_registry\n.section registry,\"aw\",@nobits\n.globl first\nfirst: .zero 9\n",
    );
    let middle = elf_obj(".bss\n.globl unrelated\nunrelated: .zero 8\n");
    let tail = elf_obj(".section registry,\"aw\",@nobits\n.p2align 6\n");
    let image = elf_link(&[main, middle, tail]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "end") - pointer(&f, "begin"), 64);
    assert_eq!(pointer(&f, "begin") % 64, 0);
    assert!(value(&f, "unrelated") >= pointer(&f, "end"));
}

#[test]
fn macho_boundary_scope_and_custom_executable_entry() {
    let image = macho_link(&[macho_obj(
        ".section __TEXT,custom,regular,pure_instructions\n.globl _main\n_main: mov w0,#0\nret\n",
    )])
    .unwrap();
    assert!(
        object::File::parse(image.as_slice())
            .unwrap()
            .section_by_name("custom")
            .is_some()
    );
    assert!(
        macho_link(&[macho_obj(".data\n.globl _main\n_main: .quad 0\n")])
            .unwrap_err()
            .contains("text definition")
    );
    for name in [
        "section$start$__DATA",
        "section$start$__DATA$",
        "section$end$__OTHER$registry",
        "section$end$__DATA$__got",
    ] {
        let err = macho_link(&[macho_obj(&format!(
            ".text\n.globl _main\n_main: ret\n.data\n.quad {name}\n"
        ))])
        .unwrap_err();
        assert!(err.contains("boundary"), "{err}");
    }
    let err = macho_link(&[macho_obj(".text\n.globl _main\n_main: ret\n.section __DATA,__thread_data,thread_local_regular\n.quad 1\n.data\n.quad section$start$__DATA$__thread_data\n")]).unwrap_err();
    assert!(err.contains("special section"));
}

#[test]
fn macho_selected_dylib_definition_precedes_boundary_synthesis() {
    let object = macho_obj(
        ".text\n.globl _main\n_main: ret\n.data\n.quad section$start$__DATA$registry\n.section __DATA,registry\n.quad 1\n",
    );
    let exe = keel_link::link(
        &inputs(&[object]),
        Options {
            dylibs: vec![Dylib {
                install_name: "/usr/lib/libSystem.B.dylib".into(),
                exports: BTreeSet::from(["section$start$__DATA$registry".into()]),
                weak_exports: BTreeSet::new(),
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(file.imports().unwrap().len(), 1);
    assert_eq!(
        file.imports().unwrap()[0].name(),
        b"section$start$__DATA$registry"
    );
}

#[test]
fn macho_entry_requires_file_backed_instructions() {
    let original =
        macho_obj(".section __TEXT,custom,regular,pure_instructions\n.globl _main\n_main: ret\n");
    // Exercise each instruction attribute independently, including malformed
    // zerofill code that is admitted by the generic object reader.
    for (flags, valid) in [
        (0u32, false),
        (0x400, true),
        (0x80000000, true),
        (0x80000400, true),
        (0x80000401, false),
        (0x80000412, false),
    ] {
        let mut bytes = original.clone();
        let at = bytes
            .windows(16)
            .position(|b| b == b"custom\0\0\0\0\0\0\0\0\0\0")
            .unwrap()
            + 64;
        bytes[at..at + 4].copy_from_slice(&flags.to_le_bytes());
        let result = macho_link(&[bytes]);
        assert_eq!(
            result.is_ok(),
            valid,
            "flags {flags:#x}: {:?}",
            result.as_ref().err()
        );
        if !valid {
            assert!(result.unwrap_err().contains("text definition"));
        }
    }
}

#[test]
fn elf_local_and_common_boundary_collisions_keep_their_identity() {
    let references = elf_obj(
        ".text\n.globl _start\n_start: ret\n.data\n.globl begin,end\nbegin: .quad __start_registry\nend: .quad __stop_registry\n.section registry,\"aw\",@progbits\n.quad 1\n",
    );
    let local = elf_obj(
        ".data\n__start_registry: .quad 2\n.globl local_pointer\nlocal_pointer: .quad __start_registry\n",
    );
    let image = elf_link(&[references.clone(), local]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "end") - pointer(&f, "begin"), 8);
    assert_ne!(pointer(&f, "local_pointer"), pointer(&f, "begin"));
    assert_eq!(
        f.symbols()
            .filter(|s| s.name().unwrap() == "__start_registry")
            .count(),
        2
    );
    let common = elf_obj(".comm __start_registry,8,8\n");
    let image = elf_link(&[references, common]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "begin"), value(&f, "__start_registry"));
    assert_eq!(
        f.symbols()
            .filter(|s| s.name().unwrap() == "__start_registry")
            .count(),
        1
    );
    assert!(pointer(&f, "begin") >= pointer(&f, "end"));
}

#[test]
fn macho_local_and_common_boundary_collisions_keep_their_identity() {
    let references = macho_obj(
        ".text\n.globl _main\n_main: ret\n.data\n.globl _begin,_end\n_begin: .quad section$start$__DATA$registry\n_end: .quad section$end$__DATA$registry\n.section __DATA,registry\n.quad 1\n",
    );
    let local = macho_obj(
        ".data\nsection$start$__DATA$registry: .quad 2\n.globl _local_pointer\n_local_pointer: .quad section$start$__DATA$registry\n",
    );
    let image = macho_link(&[references.clone(), local]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(pointer(&f, "_end") - pointer(&f, "_begin"), 8);
    assert_ne!(pointer(&f, "_local_pointer"), pointer(&f, "_begin"));
    let common = macho_obj(".comm section$start$__DATA$registry,8,3\n");
    let image = macho_link(&[references, common]).unwrap();
    let f = object::File::parse(image.as_slice()).unwrap();
    assert_eq!(
        pointer(&f, "_begin"),
        value(&f, "section$start$__DATA$registry")
    );
    assert_eq!(
        f.symbols()
            .filter(|s| s.name().unwrap() == "section$start$__DATA$registry")
            .count(),
        1
    );
    assert!(pointer(&f, "_begin") >= pointer(&f, "_end"));
}

#[test]
fn elf_present_bounds_are_hidden_but_absent_names_can_use_a_dso_definition() {
    // Minimal ET_DYN interface: retain the real symbol records and string
    // table emitted by the native linker, then expose them as DYNSYM.
    let mut library = elf_link(&[elf_obj(
        ".text\n.globl _start\n_start: ret\n.data\n.globl __start_registry\n__start_registry: .quad 99\n.section .dynamic,\"aw\",@progbits\n.quad 0,0\n",
    )]).unwrap();
    let f = object::File::parse(library.as_slice()).unwrap();
    let dynamic = f.section_by_name(".dynamic").unwrap().index().0;
    let sym = f.section_by_name(".symtab").unwrap().index().0;
    let strings = f.section_by_name(".strtab").unwrap().index().0;
    let shoff = u64::from_le_bytes(library[40..48].try_into().unwrap()) as usize;
    library[16..18].copy_from_slice(&3u16.to_le_bytes());
    library[shoff + sym * 64 + 4..shoff + sym * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    library[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8]
        .copy_from_slice(&6u32.to_le_bytes());
    library[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    library[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    for present in [false, true] {
        let extra = if present {
            ".section registry,\"aw\",@progbits\n.quad 1\n"
        } else {
            ""
        };
        let main = elf_obj(&format!(
            ".text\n.globl _start\n_start: ret\n.data\n.globl begin\nbegin: .quad __start_registry\n{extra}"
        ));
        let image = elf_link(&[main, library.clone()]).unwrap();
        let f = object::File::parse(image.as_slice()).unwrap();
        let dynamic = f
            .dynamic_symbols()
            .find(|s| s.name().unwrap() == "__start_registry");
        if present {
            assert!(dynamic.is_none());
            assert_eq!(
                pointer(&f, "begin"),
                f.section_by_name("registry").unwrap().address()
            );
        } else {
            assert!(dynamic.unwrap().is_undefined());
        }
    }
}
