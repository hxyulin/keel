//! Compiler-emitted Mach-O sections preserve section attributes and runtime roles.
#![allow(dead_code)]
mod common;
use common::{Temp, run};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::{collections::BTreeMap, process::Command};

fn clang_object(temp: &Temp, source: &str) -> Vec<u8> {
    let input = temp.0.join("input.s");
    let output = temp.0.join("reference.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&output));
    std::fs::read(output).unwrap()
}

fn sections(bytes: &[u8]) -> BTreeMap<(String, String), (u64, object::SectionFlags, Vec<u8>)> {
    let file = object::File::parse(bytes).unwrap();
    file.sections()
        // The default empty __text section predates these changes and Keel
        // gives it a minimum instruction alignment even if it remains empty.
        .filter(|section| section.name().unwrap() != "__text")
        .map(|section| {
            (
                (
                    section.segment_name().unwrap().unwrap().to_owned(),
                    section.name().unwrap().to_owned(),
                ),
                (
                    section.size(),
                    match section.flags() {
                        // Existing writer behavior sets S_ATTR_EXT_RELOC;
                        // Clang omits it. Preserve exact type and every other
                        // attribute, especially PURE/SOME_INSTRUCTIONS.
                        object::SectionFlags::MachO { flags } => object::SectionFlags::MachO {
                            flags: flags & !0x200,
                        },
                        flags => flags,
                    },
                    section.data().unwrap().to_vec(),
                ),
            )
        })
        .collect()
}

fn relocations(bytes: &[u8]) -> Vec<(String, u64, String, u8, object::RelocationFlags)> {
    let file = object::File::parse(bytes).unwrap();
    let mut result = Vec::new();
    for section in file.sections() {
        for (offset, relocation) in section.relocations() {
            let name = match relocation.target() {
                RelocationTarget::Symbol(index) => file
                    .symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned(),
                RelocationTarget::Section(index) => file
                    .section_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned(),
                target => panic!("unexpected relocation target {target:?}"),
            };
            result.push((
                section.name().unwrap().to_owned(),
                offset,
                name,
                relocation.size(),
                relocation.flags(),
            ));
        }
    }
    result.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    result
}

fn compare(source: &str) {
    let temp = Temp::new();
    let expected = clang_object(&temp, source);
    let actual = keel_asm::assemble_object(source).unwrap();
    assert_eq!(sections(&actual), sections(&expected), "{source}");
    assert_eq!(relocations(&actual), relocations(&expected), "{source}");
    let actual = object::File::parse(actual.as_slice()).unwrap();
    let expected = object::File::parse(expected.as_slice()).unwrap();
    for section in expected.sections() {
        let ours = actual
            .sections()
            .find(|candidate| {
                candidate.name() == section.name()
                    && candidate.segment_name() == section.segment_name()
            })
            .unwrap();
        assert!(ours.align() >= section.align(), "{source}");
    }
}

const CONSTRUCTOR: &str = r#"
.subsections_via_symbols
.text
.p2align 2
.globl _get_state, _initialize_helper
_get_state:
adrp x0, _state@PAGE
ldr w0, [x0, _state@PAGEOFF]
ret
_initialize_helper:
mov w0, #37
ret
.section __TEXT,__StaticInit,regular,pure_instructions
.p2align 2
.private_extern _initialize
_initialize:
stp x29, x30, [sp, #-16]!
mov x29, sp
bl _initialize_helper
Lpage: adrp x8, _state@PAGE
Ladd: add x8, x8, _state@PAGEOFF
str w0, [x8]
ldp x29, x30, [sp], #16
ret
.loh AdrpAdd Lpage,Ladd
.data
.p2align 2
.globl _state
_state: .long 0
.section __DATA,__mod_init_func,mod_init_funcs
.p2align 3
.quad _initialize
"#;

#[test]
fn compiler_section_contents_flags_and_relocations_match_clang() {
    // The sources in one group declare distinct sections, so each section
    // means the same together as alone. A redeclared section with another
    // type or content goes in a later group.
    for group in [
        &[
            ".section __TEXT,__custom\n.byte 7\n.section __DATA,__custom\n.byte 8\n",
            ".section __TEXT,__custom_code,regular,pure_instructions\n.p2align 2\nret\n",
            ".section __TEXT,__StaticInit,regular,pure_instructions\n.p2align 2\nret\n",
            ".section __DWARF,__debug_aranges,regular,debug\n.long 12\n.short 2\n.long 0\n.byte 8,0\n",
            ".section __DATA,__mod_init_func,mod_init_funcs\n.p2align 3\n.quad _initialize\n",
            ".section __DWARF,__custom,regular\n.byte 1,2\n",
            ".tbss _zero,4\n.tbss _other,8,3\n",
        ][..],
        &[
            ".section __TEXT,__StaticInit,regular,pure_instructions\n.long 0xd65f03c0\n",
            ".section __DWARF,__debug_aranges\n.byte 1,2\n",
            ".section __TEXT,__custom\nret\n.byte 0\n.p2align 4\n",
            ".section __DATA,__mod_init_func\n.quad _initialize\n",
        ],
        &[
            ".section __TEXT,__StaticInit,regular,pure_instructions\n",
            ".section __DWARF,__debug_aranges,regular\n.byte 1,2\n",
        ],
        &[".section __TEXT,__StaticInit\n.long 0\n"],
        &[".section __TEXT,__StaticInit\nret\n"],
        &[CONSTRUCTOR],
    ] {
        compare(&group.concat());
    }
}

#[test]
fn reserved_section_defaults_match_native_types_and_storage() {
    let temp = Temp::new();
    let reserved = [
        ("__TEXT", "__cstring", 2),
        ("__TEXT", "__literal4", 3),
        ("__TEXT", "__literal8", 4),
        ("__TEXT", "__literal16", 14),
        ("__DATA", "__bss", 1),
        ("__DATA", "__thread_data", 17),
        ("__DATA", "__thread_bss", 18),
        ("__DATA", "__thread_vars", 19),
        ("__TEXT", "__eh_frame", 0x6800_000b),
        ("__TEXT", "__StaticInit", 0),
        ("__DATA", "__mod_init_func", 0),
    ];
    for suffix in ["", ",regular"] {
        // One source per spelling declares every reserved section.
        let source = reserved
            .iter()
            .map(|(segment, name, _)| format!(".section {segment},{name}{suffix}\n.zero 16\n"))
            .collect::<String>();
        let expected = clang_object(&temp, &source);
        let actual = keel_asm::assemble_object(&source).unwrap();
        assert_eq!(sections(&actual), sections(&expected), "{source}");
        for (segment, name, flags) in reserved {
            for bytes in [&actual, &expected] {
                let file = object::File::parse(bytes.as_slice()).unwrap();
                let section = file.section_by_name(name).unwrap();
                assert_eq!(
                    section.flags(),
                    object::SectionFlags::MachO { flags },
                    "{segment},{name}{suffix}"
                );
                assert_eq!(section.size(), 16);
                assert_eq!(section.file_range().is_none(), matches!(flags, 1 | 18));
            }
            // Keel's writer derives its TLV header flag from the effective
            // section type, independently of declaration spelling. Clang omits
            // this flag in objects; native TLS behavior is checked below.
            let alone = format!(".section {segment},{name}{suffix}\n.zero 16\n");
            let actual = keel_asm::assemble_object(&alone).unwrap();
            let header_flags = u32::from_le_bytes(actual[24..28].try_into().unwrap());
            assert_eq!(header_flags & 0x800000 != 0, flags == 19, "{alone}");
        }
    }
}

const RESELECT_CODE: &str = r#"
.subsections_via_symbols
.section __TEXT,__custom,regular,pure_instructions
.p2align 2
.globl _first
_first: mov w0,#7
ret
.text
.globl _entry
_entry: b _second
.section __TEXT,__custom
.globl _second
_second: mov w0,#41
ret
"#;

#[test]
fn bare_section_reselection_preserves_declared_kind_and_contents() {
    for source in [
        RESELECT_CODE,
        ".section __TEXT,__custom,regular,pure_instructions\nret\n.text\nret\n.section __TEXT,__custom\nret\n",
        ".section __TEXT,__strings,cstring_literals\n.asciz \"one\"\n.data\n.byte 7\n.section __TEXT,__strings\n.asciz \"two\"\n",
        ".section __DATA,__zeros,zerofill\n.zero 4\n.text\nret\n.section __DATA,__zeros\n.zero 8\n",
        ".section __DWARF,__custom,regular,debug\n.byte 1\n.text\nret\n.section __DWARF,__custom\n.byte 2\n",
    ] {
        compare(source);
    }
    for source in [
        ".section __TEXT,__custom,regular,pure_instructions\n.section __TEXT,__custom,regular\n",
        ".section __DATA,__custom,cstring_literals\n.section __DATA,__custom,zerofill\n",
    ] {
        let error = keel_asm::assemble(source).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("different type"), "{error}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn reselected_custom_code_links_and_executes_both_fragments() {
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        "extern int first(void), second(void), entry(void);\nint main(void) { return first() != 7 || second() != 41 || entry() != 41; }\n",
    )
    .unwrap();
    // bare_section_reselection_preserves_declared_kind_and_contents checks
    // these bytes and relocations against Clang's, so only Keel's object runs.
    let object = temp.0.join("reselect.o");
    let executable = temp.0.join("run");
    std::fs::write(&object, keel_asm::assemble_object(RESELECT_CODE).unwrap()).unwrap();
    run(Command::new("clang")
        .arg(&driver)
        .arg(object)
        .arg("-Wl,-dead_strip")
        .arg("-o")
        .arg(&executable));
    assert!(common::execute(&executable).success());
}

#[test]
fn typed_code_and_module_init_sections_preserve_program_api() {
    use keel_asm_ir::{Directive, Item, Program, SectionKind};
    let mut program = Program::default();
    program.items.push(Item::Directive(Directive::Section {
        segment: "__TEXT".into(),
        name: "__StaticInit".into(),
        kind: SectionKind::Code,
    }));
    program.items.push(Item::Text("_initialize: ret".into()));
    program.items.push(Item::Directive(Directive::Section {
        segment: "__DATA".into(),
        name: "__mod_init_func".into(),
        kind: SectionKind::ModuleInit,
    }));
    program.items.push(Item::Text(".quad _initialize".into()));
    let source = ".section __TEXT,__StaticInit,regular,pure_instructions\n_initialize: ret\n.section __DATA,__mod_init_func,mod_init_funcs\n.quad _initialize\n";
    assert_eq!(
        keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap(),
        keel_asm::assemble_object(source).unwrap()
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn module_init_function_runs_before_main_with_cross_section_call_and_loh() {
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        "extern int state, get_state(void);\nint main(void) { return state != 37 || get_state() != 37; }\n",
    )
    .unwrap();
    // compiler_section_contents_flags_and_relocations_match_clang checks
    // these bytes and relocations against Clang's, so only Keel's object runs.
    let object = temp.0.join("constructor.o");
    let executable = temp.0.join("run");
    std::fs::write(&object, keel_asm::assemble_object(CONSTRUCTOR).unwrap()).unwrap();
    run(Command::new("clang")
        .arg(&driver)
        .arg(object)
        .arg("-Wl,-dead_strip")
        .arg("-o")
        .arg(&executable));
    assert!(common::execute(&executable).success());
}

const TLS: &str = r#"
.text
.p2align 2
.globl _get_tls
_get_tls:
stp x29, x30, [sp, #-16]!
adrp x0, _value@TLVPPAGE
ldr x0, [x0, _value@TLVPPAGEOFF]
ldr x16, [x0]
blr x16
ldp x29, x30, [sp], #16
ret
.tbss _storage,4
.section __DATA,__thread_vars,thread_local_variables
.p2align 3
.globl _value
_value: .quad __tlv_bootstrap,0,_storage
.globl _value_alias, _get_tls_alias
.set _value_alias,_value
.set _get_tls_alias,_get_tls
"#;

#[test]
fn omitted_tbss_alignment_and_tlv_descriptor_match_clang() {
    compare(TLS);
    let temp = Temp::new();
    for bytes in [
        keel_asm::assemble_object(TLS).unwrap(),
        clang_object(&temp, TLS),
    ] {
        let file = object::File::parse(bytes.as_slice()).unwrap();
        for (alias, original) in [("_value_alias", "_value"), ("_get_tls_alias", "_get_tls")] {
            let alias = file.symbols().find(|s| s.name().unwrap() == alias).unwrap();
            let original = file
                .symbols()
                .find(|s| s.name().unwrap() == original)
                .unwrap();
            assert_eq!(alias.address(), original.address());
            assert_eq!(alias.section_index(), original.section_index());
            assert!(alias.is_global());
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn inferred_thread_sections_use_real_thread_local_storage() {
    // `.tbss` and both thread-section spellings give one Keel object, and
    // compare() checks each against Clang, so one Keel image runs.
    let object = keel_asm::assemble_object(TLS).unwrap();
    for suffix in ["", ",regular"] {
        let source = TLS
            .replace(
                ".tbss _storage,4",
                &format!(".section __DATA,__thread_bss{suffix}\n_storage: .zero 4"),
            )
            .replace(
                ".section __DATA,__thread_vars,thread_local_variables",
                &format!(".section __DATA,__thread_vars{suffix}"),
            );
        compare(&source);
        assert_eq!(
            keel_asm::assemble_object(&source).unwrap(),
            object,
            "{source}"
        );
    }
    check_tls_runtime(&object);
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn check_tls_runtime(object_bytes: &[u8]) {
    let temp = Temp::new();
    let driver = temp.0.join("tls.c");
    std::fs::write(
        &driver,
        r#"
#include <pthread.h>
#include <stdint.h>
extern _Thread_local int value, value_alias;
extern int *get_tls(void), *get_tls_alias(void);
static int *main_pointer;
static void *thread(void *unused) {
    (void)unused;
    int *p = get_tls();
    if (p == main_pointer || p != &value || p != &value_alias || p != get_tls_alias() || *p != 0) return (void *)(uintptr_t)1;
    *p = 73;
    return (void *)(uintptr_t)(value != 73);
}
int main(void) {
    pthread_t worker;
    void *result;
    main_pointer = get_tls();
    if (main_pointer != &value || main_pointer != &value_alias || main_pointer != get_tls_alias() || *main_pointer != 0) return 1;
    *main_pointer = 42;
    if (pthread_create(&worker, 0, thread, 0)) return 2;
    if (pthread_join(worker, &result)) return 3;
    return result != 0 || value != 42 || *get_tls() != 42;
}
"#,
    )
    .unwrap();
    let object = temp.0.join("tls.o");
    let executable = temp.0.join("run");
    std::fs::write(&object, object_bytes).unwrap();
    run(Command::new("clang")
        .arg("-std=c11")
        .arg(&driver)
        .arg(object)
        .arg("-pthread")
        .arg("-o")
        .arg(&executable));
    assert!(common::execute(&executable).success());
}
