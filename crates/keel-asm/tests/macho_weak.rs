//! Apple weak definitions retain their nlist attributes and linker coalescing.
#![allow(dead_code)]
mod common;
use common::{Temp, run};
use object::{Object, ObjectSymbol};
use std::{collections::BTreeMap, process::Command};

// Read nlist directly: object::Symbol::is_weak does not distinguish the
// N_WEAK_DEF (0x80) and N_WEAK_REF / N_WEAK_DEF_CAN_BE_HIDDEN (0x40) bits.
fn nlists(bytes: &[u8]) -> BTreeMap<String, (u8, u8, u16, u64)> {
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let mut command = 32;
    for _ in 0..word(16) {
        if word(command) == 2 {
            let symoff = word(command + 8) as usize;
            let count = word(command + 12) as usize;
            let stroff = word(command + 16) as usize;
            return (0..count)
                .filter_map(|i| {
                    let offset = symoff + 16 * i;
                    let name = &bytes[stroff + word(offset) as usize..];
                    let end = name.iter().position(|&b| b == 0).unwrap();
                    let name = std::str::from_utf8(&name[..end]).unwrap();
                    (!name.starts_with("ltmp")).then(|| {
                        (
                            name.to_owned(),
                            (
                                bytes[offset + 4],
                                bytes[offset + 5],
                                u16::from_le_bytes(
                                    bytes[offset + 6..offset + 8].try_into().unwrap(),
                                ),
                                u64::from_le_bytes(
                                    bytes[offset + 8..offset + 16].try_into().unwrap(),
                                ),
                            ),
                        )
                    })
                })
                .collect();
        }
        command += word(command + 4) as usize;
    }
    panic!("no LC_SYMTAB");
}

fn clang_object(temp: &Temp, name: &str, source: &str) -> Vec<u8> {
    let input = temp.0.join(format!("{name}.s"));
    let output = temp.0.join(format!("{name}.o"));
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&output));
    std::fs::read(output).unwrap()
}

#[test]
fn weak_definition_attributes_match_clang_without_changing_visibility() {
    let temp = Temp::new();
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for body in [
            "DIRECTIVE _foo\n_foo: ret\n",
            ".globl _foo\nDIRECTIVE _foo\n_foo: ret\n",
            "_foo: ret\nDIRECTIVE _foo\n.globl _foo\n",
            ".private_extern _foo\nDIRECTIVE _foo\n_foo: ret\n",
            "DIRECTIVE _foo\n_foo: ret\n.private_extern _foo\n",
            "DIRECTIVE _foo\n",
            "DIRECTIVE _foo\nbl _foo\n",
            "DIRECTIVE _foo\nDIRECTIVE _foo\n.globl _foo\n_foo: ret\n",
            "DIRECTIVE _foo\n.comm _foo,8,3\n",
            ".comm _foo,8,3\nDIRECTIVE _foo\n",
            "DIRECTIVE _foo,_bar\n.globl _foo,_bar\n_foo: ret\n_bar: ret\n",
            "DIRECTIVE _foo\n.weak_definition _foo\n.globl _foo\n_foo: ret\n",
            ".weak_definition _foo\nDIRECTIVE _foo\n.globl _foo\n_foo: ret\n",
        ] {
            let source = body.replace("DIRECTIVE", directive);
            let reference = clang_object(&temp, "reference", &source);
            let actual = keel_asm::assemble_object(&source).unwrap();
            assert_eq!(nlists(&actual), nlists(&reference), "{source}");
            let parsed = keel_asm::assemble(&source).unwrap();
            for symbol in parsed.symbols {
                assert!(symbol.weak_definition, "{source}");
                assert_eq!(
                    symbol.weak_can_be_hidden,
                    directive.ends_with("hidden"),
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn weak_definition_rejects_local_temporary_and_malformed_names() {
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for operand in ["Ltemporary", "123", "_foo,,_bar", "_foo,"] {
            let source = format!("{directive} {operand}\n");
            let error = keel_asm::assemble(&source).unwrap_err();
            assert_eq!(error.line, 1, "{source}");
            assert!(!error.message.is_empty(), "{source}");
        }
    }
}

#[test]
fn exact_aliases_take_target_weak_attributes_with_independent_visibility() {
    let temp = Temp::new();
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for body in [
            ".globl _target,_alias\nDIRECTIVE _target\n_target: ret\n.set _alias,_target\n",
            // Forward, reverse-sorted chains require multiple resolution passes.
            ".globl _target,_alias,_chain\n.set _alias,_chain\n.set _chain,_target\n_target: ret\nDIRECTIVE _target\n",
            ".globl _target,_alias,_chain\n.set _chain,_alias\n.set _alias,_target\n_target: ret\nDIRECTIVE _target\n",
            // An alias annotation cannot turn a strong target into a weak one.
            ".globl _target,_alias\nDIRECTIVE _alias\n.set _alias,_target\n_target: ret\n",
            ".globl _target,_alias,_chain\nDIRECTIVE _alias,_chain\n.set _alias,_chain\n.set _chain,_target\n_target: ret\n",
            // Replace, rather than union, the alias's can-hide annotation.
            ".globl _target,_alias\n.weak_definition _target\nDIRECTIVE _alias\n.set _alias,_target\n_target: ret\n",
            ".globl _target,_alias\nDIRECTIVE _target\n.weak_definition _alias\n.set _alias,_target\n_target: ret\n",
            ".private_extern _target\n.globl _alias\nDIRECTIVE _target\n.set _alias,_target\n_target: ret\n",
            ".globl _target\n.private_extern _alias\nDIRECTIVE _target\n.set _alias,_target\n_target: ret\n",
            ".globl _target\nDIRECTIVE _target\n.set _alias,_target\n_target: ret\n",
            ".globl _alias\nDIRECTIVE _target\n.set _alias,_target\n_target: ret\n",
            // Coincident labels are separate definitions, not assignment aliases.
            ".globl _target,_alias\nDIRECTIVE _target\n_target:\n_alias: ret\n",
        ] {
            for subsections in ["", ".subsections_via_symbols\n"] {
                let source = format!("{subsections}{}", body.replace("DIRECTIVE", directive));
                let expected = clang_object(&temp, "reference", &source);
                let actual = keel_asm::assemble_object(&source).unwrap();
                assert_eq!(nlists(&actual), nlists(&expected), "{source}");
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn weak_aliases_and_forward_chains_coalesce_across_objects_at_runtime() {
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        r#"
extern int target(void), alias(void), chain(void);
int main(void) {
    int value = target();
    int (*volatile pointers[])(void) = {target, alias, chain};
    return (value != 7 && value != 11) || alias() != value || chain() != value
        || pointers[0] != pointers[1] || pointers[0] != pointers[2];
}
"#,
    )
    .unwrap();
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for reference in [true, false] {
            let mut objects = Vec::new();
            for (index, value) in [7, 11].into_iter().enumerate() {
                let source = format!(
                    ".subsections_via_symbols\n.globl _target,_alias,_chain\n.set _alias,_chain\n.set _chain,_target\n{directive} _target\n_target: mov w0,#{value}\nret\n"
                );
                let bytes = if reference {
                    clang_object(&temp, &format!("reference_{index}"), &source)
                } else {
                    keel_asm::assemble_object(&source).unwrap()
                };
                let object = temp.0.join(format!("weak_{index}.o"));
                std::fs::write(&object, bytes).unwrap();
                objects.push(object);
            }
            for reverse in [false, true] {
                if reverse {
                    objects.reverse();
                }
                let executable = temp.0.join("run");
                run(Command::new("clang")
                    .arg(&driver)
                    .args(&objects)
                    .arg("-Wl,-dead_strip")
                    .arg("-o")
                    .arg(&executable));
                let status = common::execute(&executable);
                assert!(
                    status.success(),
                    "directive={directive}, clang={reference}, reverse={reverse}, status={status}"
                );
            }
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn weak_annotation_on_alias_of_strong_target_still_overrides_weak_object() {
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        "extern int target(void), alias(void);\nint main(void) { int (*volatile pointers[])(void) = {target, alias}; return target() != 42 || alias() != 42 || pointers[0] != pointers[1]; }\n",
    )
    .unwrap();
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for reference in [true, false] {
            let mut objects = Vec::new();
            for (name, source) in [
                (
                    "weak",
                    ".subsections_via_symbols\n.globl _alias\n.weak_definition _alias\n_alias: mov w0,#7\nret\n".to_owned(),
                ),
                (
                    "strong",
                    format!(
                        ".subsections_via_symbols\n.globl _target,_alias\n{directive} _alias\n.set _alias,_target\n_target: mov w0,#42\nret\n"
                    ),
                ),
            ] {
                let bytes = if reference {
                    clang_object(&temp, name, &source)
                } else {
                    keel_asm::assemble_object(&source).unwrap()
                };
                let object = temp.0.join(format!("{name}.o"));
                std::fs::write(&object, bytes).unwrap();
                objects.push(object);
            }
            for reverse in [false, true] {
                if reverse {
                    objects.reverse();
                }
                let executable = temp.0.join("run");
                run(Command::new("clang")
                    .arg(&driver)
                    .args(&objects)
                    .arg("-Wl,-dead_strip")
                    .arg("-o")
                    .arg(&executable));
                let status = common::execute(&executable);
                assert!(
                    status.success(),
                    "directive={directive}, clang={reference}, reverse={reverse}, status={status}"
                );
            }
        }
    }
}

#[test]
fn weak_calls_and_pointers_keep_symbol_relocations() {
    let source = ".subsections_via_symbols\n.globl _value,_caller\n.weak_definition _value\n_value: ret\n_caller: b _value\n.data\n.quad _value\n";
    let temp = Temp::new();
    let reference = clang_object(&temp, "reference", source);
    let actual = keel_asm::assemble_object(source).unwrap();
    let relocations = |bytes: &[u8]| {
        use object::{ObjectSection, RelocationTarget};
        let file = object::File::parse(bytes).unwrap();
        let mut result = Vec::new();
        for section in file.sections() {
            for (offset, relocation) in section.relocations() {
                let RelocationTarget::Symbol(index) = relocation.target() else {
                    panic!("weak definition reference must remain a symbol relocation");
                };
                result.push((
                    section.name().unwrap().to_owned(),
                    offset,
                    file.symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_owned(),
                    relocation.size(),
                    relocation.flags(),
                ));
            }
        }
        result
    };
    assert_eq!(relocations(&actual), relocations(&reference));
    assert_eq!(relocations(&actual).len(), 2);
}

#[test]
fn weak_same_atom_branches_match_clang_resolution() {
    use object::ObjectSection;
    let temp = Temp::new();
    for source in [
        ".globl _value,_caller\n.weak_definition _value\n_value: ret\n_caller: b _value\n",
        ".subsections_via_symbols\n.globl _value\n.weak_definition _value\n_value: b _value\n",
        ".subsections_via_symbols\n.globl _value\n.weak_def_can_be_hidden _value\n_value: b _value\n",
    ] {
        let expected = clang_object(&temp, "reference", source);
        let actual = keel_asm::assemble_object(source).unwrap();
        let expected = object::File::parse(expected.as_slice()).unwrap();
        let actual = object::File::parse(actual.as_slice()).unwrap();
        let expected = expected.section_by_name("__text").unwrap();
        let actual = actual.section_by_name("__text").unwrap();
        assert_eq!(expected.relocations().count(), 0, "{source}");
        assert_eq!(actual.relocations().count(), 0, "{source}");
        assert_eq!(actual.data().unwrap(), expected.data().unwrap(), "{source}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn weak_definitions_coalesce_and_yield_to_strong_definitions_at_runtime() {
    let temp = Temp::new();
    let driver = temp.0.join("main.c");
    std::fs::write(
        &driver,
        r#"
extern int value(void), read_one(void), read_two(void);
extern int (*pointer_one)(void), (*pointer_two)(void);
int main(void) {
    int result = value();
    return (EXPECTED ? result != EXPECTED : result != 7 && result != 11)
        || read_one() != result || read_two() != result
        || pointer_one != value || pointer_two != value;
}
"#,
    )
    .unwrap();
    let strong = temp.0.join("strong.o");
    std::fs::write(
        &strong,
        clang_object(
            &temp,
            "strong_reference",
            ".globl _value\n_value: mov w0,#42\nret\n",
        ),
    )
    .unwrap();
    for directive in [".weak_definition", ".weak_def_can_be_hidden"] {
        for reference in [false, true] {
            let mut objects = Vec::new();
            for (name, value) in [("one", 7), ("two", 11)] {
                let source = format!(
                    ".subsections_via_symbols\n.globl _value,_read_{name},_pointer_{name}\n{directive} _value\n_value: mov w0,#{value}\nret\n_read_{name}: b _value\n.data\n.p2align 3\n_pointer_{name}: .quad _value\n"
                );
                let bytes = if reference {
                    clang_object(&temp, name, &source)
                } else {
                    keel_asm::assemble_object(&source).unwrap()
                };
                let object = temp.0.join(format!("weak_{name}.o"));
                std::fs::write(&object, bytes).unwrap();
                objects.push(object);
            }
            // Reverse the order as well: a strong definition always prevails,
            // while either weak body may be selected when both are weak.
            for reverse in [false, true] {
                if reverse {
                    objects.reverse();
                }
                for with_strong in [false, true] {
                    let executable = temp.0.join("run");
                    let mut command = Command::new("clang");
                    command.arg(&driver).args(&objects);
                    if with_strong {
                        command.arg(&strong);
                    }
                    command
                        .arg(if with_strong {
                            "-DEXPECTED=42"
                        } else {
                            "-DEXPECTED=0"
                        })
                        .arg("-Wl,-dead_strip")
                        .arg("-o")
                        .arg(&executable);
                    run(&mut command);
                    assert!(
                        common::execute(&executable).success(),
                        "directive={directive}, clang={reference}, reverse={reverse}, strong={with_strong}"
                    );
                }
            }
        }
    }
}
