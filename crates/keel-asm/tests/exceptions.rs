use keel_asm::{RelocationKind, assemble, assemble_object};
use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags};
use std::process::Command;
mod common;
use common::{Temp, run};

fn generated() -> String {
    let source = include_str!("../../keel-codegen/tests/programs/exceptions.keel");
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap()
}
#[test]
fn generated_lsda_matches_clang() {
    let source = generated();
    common::compare_clang(&source);
    let temp = Temp::new();
    let input = temp.0.join("exceptions.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, &source).unwrap();
    run(Command::new("clang")
        .arg("-c")
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    let bytes = std::fs::read(reference).unwrap();
    let reference = object::File::parse(bytes.as_slice()).unwrap();
    let bytes = assemble_object(&source).unwrap();
    std::fs::write(&ours, &bytes).unwrap();
    let ours_object = object::File::parse(bytes.as_slice()).unwrap();
    let expected = reference.section_by_name("__gcc_except_tab").unwrap();
    let actual = ours_object.section_by_name("__gcc_except_tab").unwrap();
    // Apple clang leaves sentinel values in unresolved GOT words. Compare
    // table bytes outside those words, and compare the relocation records.
    let normalized = |section: &object::Section<'_, '_>| {
        let mut data = section.data().unwrap().to_vec();
        for (offset, r) in section.relocations() {
            assert!(matches!(
                r.flags(),
                RelocationFlags::MachO {
                    r_type: 7,
                    r_pcrel: true,
                    r_length: 2
                }
            ));
            data[offset as usize..offset as usize + 4].fill(0);
        }
        data
    };
    assert_eq!(normalized(&expected), normalized(&actual));
    let records = |section: object::Section<'_, '_>, file: &object::File<'_>| {
        section
            .relocations()
            .map(|(offset, r)| {
                let object::RelocationTarget::Symbol(index) = r.target() else {
                    panic!("expected symbol relocation")
                };
                (
                    offset,
                    file.symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_string(),
                    r.flags(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(records(expected, &reference), records(actual, &ours_object));
    let frame = ours_object.section_by_name("__eh_frame").unwrap();
    assert!(frame.relocations().any(|(_, r)| matches!(
        r.flags(),
        RelocationFlags::MachO {
            r_type: 7,
            r_pcrel: true,
            r_length: 2
        }
    )));
    let output = Command::new("dwarfdump")
        .arg("--eh-frame")
        .arg(&ours)
        .output()
        .unwrap();
    assert!(output.status.success());
    let decoded = String::from_utf8(output.stdout).unwrap();
    assert!(decoded.contains("zPLR"));
    assert!(decoded.contains("LSDA Address"), "{decoded}");
}
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn cpp_exceptions_catches_and_cleanup_execute() {
    let temp = Temp::new();
    let obj = temp.0.join("exceptions.o");
    let exe = temp.0.join("run");
    std::fs::write(&obj, assemble_object(&generated()).unwrap()).unwrap();
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../keel-codegen/tests/programs/exceptions.cpp");
    let order = temp.0.join("order");
    std::fs::write(&order, "_k_two\n_k_cleanup\n_k_catch\n").unwrap();
    for reorder in [false, true] {
        let mut command = Command::new("clang++");
        command.arg(&driver).arg(&obj);
        if reorder {
            command
                .arg("-Wl,-dead_strip")
                .arg("-Xlinker")
                .arg("-order_file")
                .arg("-Xlinker")
                .arg(&order);
        }
        run(command.arg("-o").arg(&exe));
        assert!(common::execute(&exe).success(), "reorder={reorder}");
    }
}
#[test]
fn symbolic_leb_layout_and_got_constraints() {
    let source = ".data\nLstart:\n.uleb128 Lend-Lstart\n.zero 127\nLend:\n.sleb128 Lstart-Lend\nLptr:\n.long _type@GOT-Lptr\n";
    let assembly = assemble(source).unwrap();
    let data = assembly.sections[0].data();
    assert_eq!(&data[..2], &[0x81, 1]);
    assert_eq!(&data[129..131], &[0xff, 0x7e]);
    let r = &assembly.relocations[0];
    assert_eq!(r.kind, RelocationKind::PointerToGot);
    assert_eq!(r.addend, 0);
    for source in [
        ".data\n.uleb128 Missing-Lmissing",
        ".data\nLbegin:\n.uleb128 Lend-Lbegin\n.text\nLend: ret",
        ".data\nLbegin:\n.uleb128 129-Lend+Lbegin\nLend:",
        ".cfi_startproc\n.cfi_personality 0, _personality\n.cfi_endproc",
        ".cfi_startproc\n.cfi_lsda 0, Llsda\n.cfi_endproc",
        ".cfi_startproc\n.cfi_personality 155, _p\n.cfi_personality 155, _p\n.cfi_endproc",
        ".data\nLhere: .quad _type@GOT-Lhere",
        ".data\nLhere: .long _type@GOT-Lhere+1",
        ".data\nLhere: .byte 0\n.long _type@GOT-Lhere",
        ".data\nLhere: .long _type@GOTPAGEOFF-Lhere",
    ] {
        assert!(assemble(source).is_err(), "accepted {source}");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn exception_crosses_plain_frame_with_referenced_interior_label() {
    let source = ".globl _wrapper\n_wrapper:\n.cfi_startproc\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa x29,16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\nLcall: bl _thrower\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n.data\n.quad Lcall\n";
    let temp = Temp::new();
    let obj = temp.0.join("plain.o");
    let driver = temp.0.join("main.cpp");
    let exe = temp.0.join("run");
    std::fs::write(&obj, assemble_object(source).unwrap()).unwrap();
    std::fs::write(&driver,"extern \"C\" void wrapper(); extern \"C\" void thrower(){throw 42;} int main(){try {wrapper();} catch(int n){return n!=42;} return 1;}").unwrap();
    run(Command::new("clang++")
        .arg(driver)
        .arg(obj)
        .arg("-o")
        .arg(&exe));
    assert!(common::execute(&exe).success());
}
