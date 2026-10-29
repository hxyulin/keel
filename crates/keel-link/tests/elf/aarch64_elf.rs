use keel_link::{Input, archive, elf};
use object::{Object, ObjectSection, ObjectSymbol};

const START: &str = include_str!("../fixtures/aarch64/start.s");
const HELPER: &str = include_str!("../fixtures/aarch64/helper.s");
fn options() -> elf::Options {
    elf::Options {
        machine: elf::Machine::Aarch64,
        ..Default::default()
    }
}
fn build() -> Vec<u8> {
    let start = keel_asm::assemble_elf_object(START).unwrap();
    let helper = keel_asm::assemble_elf_object(HELPER).unwrap();
    let library = archive::build(
        &[Input {
            name: "helper.o",
            bytes: &helper,
        }],
        archive::Target::Aarch64LinuxGnu,
    )
    .unwrap();
    elf::link(
        &[
            Input {
                name: "start.o",
                bytes: &start,
            },
            Input {
                name: "helper.a",
                bytes: &library,
            },
        ],
        options(),
    )
    .unwrap()
    .bytes
}
#[test]
fn links_aarch64_objects_and_archives_on_any_host() {
    let bytes = build();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.architecture(), object::Architecture::Aarch64);
    assert_eq!(file.kind(), object::ObjectKind::Executable);
    assert!(file.entry() != 0);
    assert!(file.section_by_name(".bss").unwrap().size() >= 32);
    assert_eq!(file.symbol_by_name("shared").unwrap().size(), 16);
}
#[test]
fn rejects_wrong_architecture_and_unsupported_directives() {
    let arm = keel_asm::assemble_elf_object(START).unwrap();
    assert!(
        elf::link(
            &[Input {
                name: "arm.o",
                bytes: &arm
            }],
            Default::default()
        )
        .unwrap_err()
        .contains("x86-64")
    );
    for source in [
        ".cfi_startproc",
        ".section .group,\"aG\",%progbits",
        ".arch armv9-a",
    ] {
        assert!(keel_asm::assemble_elf_object(source).is_err(), "{source}");
    }
    let bad = keel_asm::assemble_elf_object(".global _start\n_start: bl missing\n").unwrap();
    assert!(
        elf::link(
            &[Input {
                name: "bad.o",
                bytes: &bad
            }],
            options()
        )
        .unwrap_err()
        .contains("undefined symbol")
    );
}
#[test]
fn rejects_out_of_range_or_misaligned_relocations() {
    for instruction in [
        "b target+134217728",
        "bl target+2",
        "ldr x0, [x0, :lo12:target+1]",
    ] {
        let source = format!(".text\n.global _start, target\n_start: {instruction}\ntarget: ret\n");
        let bytes = keel_asm::assemble_elf_object(&source).unwrap();
        let error = elf::link(
            &[Input {
                name: "overflow.o",
                bytes: &bytes,
            }],
            options(),
        )
        .unwrap_err();
        assert!(
            error.contains("overflow") || error.contains("misalignment"),
            "{error}"
        );
    }
}
#[test]
fn executes_on_native_arm_linux() {
    if !cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("keel-aarch64-elf-{}", std::process::id()));
    std::fs::write(&path, build()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = std::process::Command::new(&path).status();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(result.unwrap().code(), Some(42));
}
