//! Explicit static ELF readonly sections admit pointer relocations and scalar data.
use keel_ir::Target;
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, path::Path, process::Command};

const SOURCE: &str = r#"
int anchor=3;
const long plain __attribute__((section(".init.rodata"),aligned(8)))=5;
int *const pointer __attribute__((section(".init.rodata"),aligned(8)))=&anchor;
const long normal_plain __attribute__((section(".rodata"),aligned(8)))=7;
int *const normal_pointer __attribute__((section(".rodata"),aligned(8)))=&anchor;
long mutable __attribute__((section(".init.data"),aligned(8)))=11;
long ref_zero __attribute__((section(".ref.data"),aligned(8)));
long ref_value __attribute__((section(".ref.data"),aligned(8)))=13;
long read_sections(void) {return *pointer+*normal_pointer+plain+normal_plain+mutable+ref_zero+ref_value;}
"#;
const MAIN: &str = "extern long read_sections(void),ref_zero,ref_value;extern int anchor;int main(void){if(read_sections()!=42)return 1;anchor=23;ref_zero=17;ref_value=19;return read_sections()!=105;}";

fn module(source: &str, target: Target) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "elf-sections.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_ir::parse(&module.to_string()).unwrap()
}
fn object(module: &keel_ir::Module, target: Target) -> Vec<u8> {
    let text = keel_codegen::compile(module, target).unwrap();
    let program = keel_codegen::compile_program(module, target).unwrap();
    let (a, b) = if target == Target::X86_64LinuxGnu {
        (
            keel_asm::x86::assemble_object(&text).unwrap(),
            keel_asm::x86::assemble_program_object(&program).unwrap(),
        )
    } else {
        (
            keel_asm::assemble_elf_object(&text).unwrap(),
            keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap(),
        )
    };
    assert_eq!(a, b, "text/typed parity {target:?}");
    a
}
#[test]
fn section_flags_bytes_symbols_and_relocations_match_static_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-readonly-sections");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.c");
    fs::write(&source, SOURCE).unwrap();
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let bytes = object(&module(SOURCE, target), target);
        let generated = object::File::parse(&*bytes).unwrap();
        let reference = dir.join(format!("{target:?}.o"));
        let result = Command::new("clang")
            .arg(format!("--target={}", target.triple()))
            .args(["-fno-pic", "-fno-pie", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let expected_bytes = fs::read(reference).unwrap();
        let expected = object::File::parse(&*expected_bytes).unwrap();
        for name in [".init.rodata", ".rodata", ".init.data", ".ref.data"] {
            let a = generated.section_by_name(name).unwrap();
            let b = expected.section_by_name(name).unwrap();
            assert_eq!(a.flags(), b.flags(), "{target:?}: {name}");
            assert_eq!(a.kind(), b.kind(), "{target:?}: {name}");
            assert_eq!(a.align(), b.align(), "{target:?}: {name}");
            assert_eq!(a.size(), b.size(), "{target:?}: {name}");
            // Clang places zero contributions last; compare their symbol bytes below.
            if name != ".ref.data" {
                assert_eq!(a.data().unwrap(), b.data().unwrap(), "{target:?}: {name}");
            }
            let relocs = |file: &object::File<'_>, section: object::Section<'_, '_>| {
                section
                    .relocations()
                    .map(|(offset, r)| {
                        let object::RelocationTarget::Symbol(symbol) = r.target() else {
                            panic!("unexpected relocation")
                        };
                        (
                            offset,
                            r.kind(),
                            r.size(),
                            r.addend(),
                            file.symbol_by_index(symbol)
                                .unwrap()
                                .name()
                                .unwrap()
                                .to_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                relocs(&generated, a),
                relocs(&expected, b),
                "{target:?}: {name}"
            );
        }
        for name in [
            "plain",
            "pointer",
            "normal_plain",
            "normal_pointer",
            "mutable",
            "ref_zero",
            "ref_value",
        ] {
            let a = generated
                .symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap();
            let b = expected
                .symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap();
            assert_eq!(
                (a.size(), a.kind()),
                (b.size(), b.kind()),
                "{target:?}: {name}"
            );
            let sa = generated
                .section_by_index(a.section_index().unwrap())
                .unwrap();
            let sb = expected
                .section_by_index(b.section_index().unwrap())
                .unwrap();
            assert_eq!(sa.name().unwrap(), sb.name().unwrap(), "{target:?}: {name}");
            let aa = (a.address() - sa.address()) as usize;
            let bb = (b.address() - sb.address()) as usize;
            assert_eq!(
                &sa.data().unwrap()[aa..aa + a.size() as usize],
                &sb.data().unwrap()[bb..bb + b.size() as usize],
                "{target:?}: {name}"
            );
        }
        if target == Target::X86_64LinuxGnu
            && let Some(export) = std::env::var_os("KEEL_ELF_READONLY_EXPORT")
        {
            let export = std::path::PathBuf::from(export);
            fs::create_dir_all(&export).unwrap();
            fs::write(export.join("source.c"), SOURCE).unwrap();
            fs::write(export.join("main.c"), MAIN).unwrap();
            fs::write(export.join("generated.o"), bytes).unwrap();
        }
    }
}
#[test]
fn readonly_lifetime_families_share_static_storage_rules() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        for section in [
            ".rodata1",
            ".init.rodata.extra",
            ".exit.rodata",
            ".meminit.rodata",
            ".ref.rodata",
        ] {
            object(
                &module(&SOURCE.replace(".init.rodata", section), target),
                target,
            );
        }
    }
}
#[test]
fn incompatible_sections_retention_and_default_relro_still_reject_or_remain_distinct() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let module = module("int anchor;int *const pointer=&anchor;", target);
        let bytes = object(&module, target);
        let file = object::File::parse(&*bytes).unwrap();
        assert!(file.section_by_name(".data.rel.ro").is_some());
        for source in [
            "int x __attribute__((section(\".init.rodata\")))=1;",
            "int x __attribute__((section(\".ref.rodata\")))=1;",
            "void f(void) __attribute__((section(\".ref.data\")));void f(void){}",
            "int x __attribute__((section(\".ref.data\")))=1;int y __attribute__((section(\".ref.data\"),retain))=2;",
            "const int x __attribute__((section(\"custom\")))=1;int y __attribute__((section(\"custom\")))=2;",
        ] {
            assert!(
                keel_c::compile(
                    source,
                    "bad-sections.c",
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
