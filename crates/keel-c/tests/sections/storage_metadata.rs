//! Byte relocations, ELF notes, and minimum function entry alignment.
use keel_ir::{Module, Target};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, path::Path, process::Command};

fn compile(source: &str, target: Target, optimize: bool) -> Module {
    let mut m = keel_c::compile(
        source,
        "storage.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimize {
        keel_opt::optimize(&mut m).unwrap();
    }
    let text = m.to_string();
    let m = keel_ir::parse(&text).unwrap();
    let canonical = m.to_string();
    assert_eq!(keel_ir::parse(&canonical).unwrap().to_string(), canonical);
    keel_ir::verify_module(&m).unwrap();
    m
}
fn assemble(m: &Module, target: Target) -> Vec<u8> {
    let text = keel_codegen::compile(m, target).unwrap();
    let program = keel_codegen::compile_program(m, target).unwrap();
    let (text, typed) = match target {
        Target::X86_64LinuxGnu => (
            keel_asm::x86::assemble_object(&text).unwrap(),
            keel_asm::x86::assemble_program_object(&program).unwrap(),
        ),
        Target::Aarch64LinuxGnu => (
            keel_asm::assemble_elf_object(&text).unwrap(),
            keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap(),
        ),
        Target::Aarch64AppleDarwin => (
            keel_asm::assemble_object(&text).unwrap(),
            keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
        ),
        Target::Wasm32UnknownUnknown => unreachable!(),
    };
    assert_eq!(text, typed, "text/typed {target:?}");
    let selector = keel_codegen::OptionsSelector(Default::default());
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selector,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let chunked = match target {
        Target::X86_64LinuxGnu => {
            let mut asm = keel_asm::x86::ProgramAssembler::new(Default::default());
            keel_codegen::compile_program_chunks_with_pipeline(m, target, &pipeline, &mut |p| {
                asm.push(p)
            })
            .unwrap();
            asm.finish().unwrap()
        }
        Target::Aarch64LinuxGnu => {
            let mut asm = keel_asm::ElfProgramAssembler::new(Default::default());
            keel_codegen::compile_program_chunks_with_pipeline(m, target, &pipeline, &mut |p| {
                asm.push(p)
            })
            .unwrap();
            asm.finish().unwrap()
        }
        Target::Aarch64AppleDarwin => {
            let mut asm = keel_asm::ProgramAssembler::new(Default::default());
            keel_codegen::compile_program_chunks_with_pipeline(m, target, &pipeline, &mut |p| {
                asm.push(p)
            })
            .unwrap();
            asm.finish().unwrap()
        }
        Target::Wasm32UnknownUnknown => unreachable!(),
    };
    assert_eq!(typed, chunked, "typed/chunked {target:?}");
    typed
}
fn reference(source: &str, target: Target, case: &str) -> Vec<u8> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("storage-metadata")
        .join(format!("{target:?}-{case}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    let out = Command::new("clang")
        .arg(format!("--target={}", target.triple()))
        .args(["-fno-pic", "-fno-pie", "-c"])
        .arg(dir.join("source.c"))
        .arg("-o")
        .arg(dir.join("clang.o"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::read(dir.join("clang.o")).unwrap()
}
#[test]
fn packed_relocation_bytes_offsets_symbols_and_addends_match_clang() {
    for target in Target::NATIVE {
        let (section, lookup) = if target == Target::Aarch64AppleDarwin {
            ("__DATA,__packed", "__packed")
        } else {
            (".packed", ".packed")
        };
        let source = format!(
            r#"
int anchor[4] = {{ 11, 23, 37, 41 }};
int callback(void) {{ return 5; }}
struct __attribute__((packed)) P {{ char tag; int *p; char middle; int (*f)(void); unsigned long q; char tail; }};
struct P packed __attribute__((section("{section}"))) = {{ 7, anchor+2, 9, callback, (unsigned long)(anchor+1), 13 }};
int read_packed(void) {{ return packed.tag + *packed.p + packed.middle + packed.f() + *(int *)packed.q + packed.tail; }}
"#
        );
        let ref_bytes = reference(&source, target, "packed");
        let expected = object::File::parse(&*ref_bytes).unwrap();
        for optimize in [false, true] {
            let bytes = assemble(&compile(&source, target, optimize), target);
            let actual = object::File::parse(&*bytes).unwrap();
            let a = actual.section_by_name(lookup).unwrap();
            let b = expected.section_by_name(lookup).unwrap();
            assert_eq!(a.align(), 1);
            assert_eq!(a.align(), b.align());
            assert_eq!(a.data().unwrap(), b.data().unwrap());
            let relocs = |file: &object::File<'_>, section: object::Section<'_, '_>| {
                let mut entries = section
                    .relocations()
                    .map(|(offset, r)| {
                        let object::RelocationTarget::Symbol(id) = r.target() else {
                            panic!("symbol relocation expected")
                        };
                        (
                            offset,
                            r.kind(),
                            r.size(),
                            r.addend(),
                            file.symbol_by_index(id).unwrap().name().unwrap().to_owned(),
                        )
                    })
                    .collect::<Vec<_>>();
                entries.sort_by_key(|e| e.0);
                entries
            };
            assert_eq!(relocs(&actual, a), relocs(&expected, b), "{target:?}");
        }
    }
}
#[test]
fn elf_note_type_flags_alignment_and_records_match_clang() {
    let source = r#"
struct Note { unsigned namesz, descsz, type; char name[8]; unsigned desc; };
const struct Note note __attribute__((section(".note.Linux"), aligned(4), used)) = { 6, 4, 0x100, "Linux", 0x1234 };
"#;
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let expected_bytes = reference(source, target, "note");
        let expected = object::File::parse(&*expected_bytes).unwrap();
        for optimize in [false, true] {
            let bytes = assemble(&compile(source, target, optimize), target);
            let actual = object::File::parse(&*bytes).unwrap();
            let a = actual.section_by_name(".note.Linux").unwrap();
            let b = expected.section_by_name(".note.Linux").unwrap();
            assert_eq!(a.kind(), object::SectionKind::Note);
            assert_eq!(a.kind(), b.kind());
            assert_eq!(a.flags(), object::SectionFlags::Elf { sh_flags: 2 });
            assert_eq!(a.flags(), b.flags());
            assert_eq!(a.align(), b.align());
            assert_eq!(a.data().unwrap(), b.data().unwrap());
            assert_eq!(a.relocations().count(), 0);
        }
    }
}
#[test]
fn function_alignment_merges_and_is_observable_in_objects() {
    let source = "void before(void) {}\n__attribute__((weak,aligned(64))) void aligned(void);\nvoid aligned(void) {}\nvoid aligned(void) __attribute__((aligned(32)));\n";
    for target in Target::NATIVE {
        let reference_bytes = reference(source, target, "align");
        let reference = object::File::parse(&*reference_bytes).unwrap();
        for optimize in [false, true] {
            let m = compile(source, target, optimize);
            assert_eq!(
                m.functions
                    .iter()
                    .find(|f| f.name == "aligned")
                    .unwrap()
                    .attrs
                    .align,
                Some(64)
            );
            let bytes = assemble(&m, target);
            let file = object::File::parse(&*bytes).unwrap();
            let name = if target == Target::Aarch64AppleDarwin {
                "_aligned"
            } else {
                "aligned"
            };
            for f in [&file, &reference] {
                let symbol = f.symbols().find(|s| s.name().unwrap() == name).unwrap();
                let section = f.section_by_index(symbol.section_index().unwrap()).unwrap();
                assert!(symbol.is_weak());
                assert_eq!(symbol.address() % 64, 0);
                assert_eq!(section.align(), 64);
            }
        }
    }
}
#[test]
fn invalid_note_storage_and_non_elf_notes_are_rejected() {
    for source in [
        "int x __attribute__((section(\".note.Linux\")))=1;",
        "const int x __attribute__((section(\".note.GNU-stack\")))=1;",
        "const int x __attribute__((section(\".note.gnu.property\")))=1;",
        "int x; int *const p __attribute__((section(\".note.Linux\")))=&x;",
        "void f(void) __attribute__((section(\".note.Linux\"))); void f(void) {}",
        "const char note __attribute__((section(\".note.Linux\")))=1;",
        "_Thread_local int x __attribute__((section(\".note.Linux\")))=1;",
    ] {
        for target in Target::NATIVE {
            assert!(
                keel_c::compile(
                    source,
                    "bad.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "{target:?}: {source}"
            );
        }
    }
}
#[test]
fn packed_relocation_runtime_on_native_host() {
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Target::Aarch64AppleDarwin
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Target::X86_64LinuxGnu
    } else {
        return;
    };
    let source = "int anchor[2]={17,29}; struct __attribute__((packed)) P { char a; int *p; char b; } packed={3,anchor+1,7}; int read(void){return packed.a+*packed.p+packed.b;}";
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("storage-metadata-runtime");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("keel.o"),
        assemble(&compile(source, target, true), target),
    )
    .unwrap();
    fs::write(
        dir.join("main.c"),
        "extern int read(void); int main(void){return read()!=39;}",
    )
    .unwrap();
    let mut link = Command::new("clang");
    // Darwin chained fixups cannot encode byte-aligned locations. Classic
    // dyld fixups support them, including Clang's identical packed object.
    if target == Target::Aarch64AppleDarwin {
        link.arg("-Wl,-no_fixup_chains");
    }
    let result = link
        .arg(dir.join("keel.o"))
        .arg(dir.join("main.c"))
        .arg("-o")
        .arg(dir.join("run"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(Command::new(dir.join("run")).status().unwrap().success());
}

#[test]
fn address_bitfields_and_out_of_range_addends_remain_unsupported() {
    for (source, message) in [
        (
            "extern int anchor; struct __attribute__((packed)) S { unsigned long p:64; unsigned tail:3; } s = { (unsigned long)&anchor, 5 };",
            "address relocation in a bit-field initializer",
        ),
        (
            "extern char anchor[]; char *p=anchor-1;",
            "pointer relocation addend",
        ),
        (
            "extern char anchor[]; char *p=anchor+4294967296UL;",
            "pointer relocation addend",
        ),
    ] {
        let error = keel_c::compile(
            source,
            "bad.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn elf_linker_applies_byte_relocations_and_preserves_function_alignment() {
    let source = "char anchor[4] = {11,23,37,41}; struct __attribute__((packed)) P { char a; char *p; char b; } packed={3,anchor+2,7}; void _start(void) __attribute__((aligned(64))); void _start(void) {}";
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let bytes = assemble(&compile(source, target, true), target);
        let options = keel_link::elf::Options {
            machine: if target == Target::Aarch64LinuxGnu {
                keel_link::elf::Machine::Aarch64
            } else {
                keel_link::elf::Machine::X86_64
            },
            ..Default::default()
        };
        let linked = keel_link::elf::link(
            &[keel_link::Input {
                name: "storage.o",
                bytes: &bytes,
            }],
            options,
        )
        .unwrap();
        let file = object::File::parse(&*linked.bytes).unwrap();
        let symbol = |name| file.symbols().find(|s| s.name().unwrap() == name).unwrap();
        let packed = symbol("packed");
        let section = file
            .section_by_index(packed.section_index().unwrap())
            .unwrap();
        let offset = (packed.address() - section.address()) as usize;
        let data = &section.data().unwrap()[offset..offset + 10];
        assert_eq!(data[0], 3);
        assert_eq!(data[9], 7);
        assert_eq!(
            u64::from_le_bytes(data[1..9].try_into().unwrap()),
            symbol("anchor").address() + 2
        );
        assert_eq!(symbol("_start").address() % 64, 0);
    }
}
