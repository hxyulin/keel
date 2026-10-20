use keel_asm::{assemble, assemble_object};
use object::{Object, ObjectSection};
use std::process::Command;
mod common;
use common::{Temp, run};

fn dump(path: &std::path::Path, flag: &str) -> String {
    let output = common::dwarfdump().arg(flag).arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn dwarf_data_directives_match_clang() {
    let source = r#"
.text
Lbegin: nop
Lend: ret
Lsize = Lend-Lbegin
.section __DWARF,__debug_info,regular,debug
.long Lsize
.asciz "equals= colon: semicolon; slash// quote\" backslash\\"
.ascii "\001\177\377\n\t"
.uleb128 0, 127, 128, 18446744073709551615
.sleb128 -9223372036854775808, -65, -64, 63, 64, 9223372036854775807
"#;
    let temp = Temp::new();
    let input = temp.0.join("data.s");
    let reference = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&reference));
    let bytes = std::fs::read(reference).unwrap();
    let reference = object::File::parse(bytes.as_slice()).unwrap();
    let bytes = assemble_object(source).unwrap();
    let ours = object::File::parse(bytes.as_slice()).unwrap();
    let got = ours.section_by_name("__debug_info").unwrap();
    let expected = reference.section_by_name("__debug_info").unwrap();
    assert_eq!(got.data().unwrap(), expected.data().unwrap());
    assert_eq!(got.flags(), expected.flags());
}
#[test]
fn line_rows_match_clang() {
    let source = ".file 1 \"example.c\"\n.file 2 \"include/other.h\"\n.text\n.globl _example\n_example:\n.loc 1 4 2\nnop\n.loc 1 9 7 prologue_end\nnop\n.loc 2 3 1\nret\n";
    common::compare_clang(source);
    let temp = Temp::new();
    let input = temp.0.join("lines.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let normalize = |dump: String| {
        dump.lines()
            .filter(|s| s.starts_with("0x"))
            .map(|s| s.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        normalize(dump(&ours, "--debug-line")),
        normalize(dump(&reference, "--debug-line"))
    );
    run(common::dwarfdump().arg("--verify").arg(&ours));
}
#[test]
fn malformed_metadata_is_diagnosed() {
    for source in [
        ".cfi_endproc",
        ".cfi_startproc",
        ".cfi_def_cfa sp,0",
        ".cfi_startproc\n.cfi_startproc",
        ".cfi_startproc\n.cfi_restore_state",
        ".cfi_startproc\n.cfi_offset x30,-7",
        ".file 1 \"foo\"\n.file 1 \"bar\"",
        ".loc 1 1 0",
        ".data\n.cfi_startproc",
        ".file 1 \"foo\"\n.loc 1 1 0 is_stmt 2",
        ".asciz \"bad\\q\"",
        ".uleb128 -1",
        ".sleb128 9223372036854775808",
        ".cfi_startproc\n.cfi_personality 155, _personality",
        ".cfi_startproc\n.cfi_lsda 16, Llsda",
        ".section __TEXT,__eh_frame,coalesced\n.byte 0\n.text\n.cfi_startproc\nret\n.cfi_endproc",
        ".file 1 \"foo\"\n.section __DWARF,__debug_line,regular,debug\n.byte 0\n.text\n.loc 1 1 0\nret",
    ] {
        assert!(assemble(source).is_err(), "accepted {source}");
    }
}
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn stack_unwinding_crosses_assembled_frame() {
    let temp = Temp::new();
    let obj = temp.0.join("frame.o");
    let c = temp.0.join("main.c");
    let exe = temp.0.join("run");
    let source = ".globl _frame\n_frame:\n.cfi_startproc\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa x29,16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\n.cfi_remember_state\n.cfi_restore_state\nbl _capture\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n";
    std::fs::write(
        &c,
        r#"
#include <unwind.h>
#include <stdint.h>
extern void frame(void);
static int found;
static _Unwind_Reason_Code cb(struct _Unwind_Context *ctx,void *arg) {
 (void)arg; uintptr_t ip=_Unwind_GetIP(ctx), base=(uintptr_t)frame;
 if (ip>base && ip<=base+24) found++;
 return _URC_NO_REASON;
}
void capture(void) { _Unwind_Backtrace(cb,0); }
int main(void) {frame();return found!=1;}
"#,
    )
    .unwrap();
    for section in [
        ".text",
        ".section __TEXT,__StaticInit,regular,pure_instructions",
    ] {
        let assembly = format!("{section}\n{source}");
        std::fs::write(&obj, assemble_object(&assembly).unwrap()).unwrap();
        run(Command::new("clang").arg(&c).arg(&obj).arg("-o").arg(&exe));
        assert!(common::execute(&exe).success(), "{section}");
    }
    let decoded = dump(&obj, "--eh-frame");
    assert!(decoded.contains("W29+16"));
    assert!(decoded.contains("W30=[CFA-8]"));
}
#[test]
fn keel_debug_info_verifies_and_builds_dsym() {
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let source = std::fs::read_to_string(repo.join("linkage.keel")).unwrap();
    let module = keel_ir::parse(&source).unwrap();
    let source = keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    let temp = Temp::new();
    let obj = temp.0.join("linkage.o");
    std::fs::write(&obj, assemble_object(&source).unwrap()).unwrap();
    let verified = common::dwarfdump()
        .arg("--verify")
        .arg(&obj)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stdout)
    );
    let info = dump(&obj, "--debug-info");
    assert!(info.contains("DW_TAG_subprogram"));

    // Object DWARF verification above is portable; linking and dSYM creation
    // need the native Mach-O linker and Apple SDK.
    #[cfg(target_os = "macos")]
    {
        let exe = temp.0.join("run");
        run(Command::new("clang")
            .arg(repo.join("linkage.c"))
            .arg(&obj)
            .arg("-o")
            .arg(&exe));
        run(Command::new("dsymutil").arg(&exe));
        run(common::dwarfdump()
            .arg("--verify")
            .arg(temp.0.join("run.dSYM")));
        let info = dump(&temp.0.join("run.dSYM"), "--debug-info");
        assert!(info.contains("DW_TAG_subprogram"));
    }
}

#[test]
fn keel_types_scopes_and_variables_verify() {
    let source = include_str!("../../keel-ir/tests/golden/debug_info.keel");
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let source = keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    let temp = Temp::new();
    let obj = temp.0.join("types.o");
    std::fs::write(&obj, assemble_object(&source).unwrap()).unwrap();
    let output = common::dwarfdump()
        .arg("--verify")
        .arg(&obj)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let info = dump(&obj, "--debug-info");
    for tag in [
        "DW_TAG_variable",
        "DW_TAG_formal_parameter",
        "DW_TAG_structure_type",
        "DW_TAG_enumeration_type",
        "DW_TAG_lexical_block",
    ] {
        assert!(info.contains(tag), "missing {tag}");
    }
}

#[test]
fn compact_cfi_forms_match_clang_size_and_decoded_rules() {
    let source = ".text\n.globl _compact_cfi\n_compact_cfi:\n.cfi_startproc\n.cfi_def_cfa sp,16\n.cfi_offset 63,-8\n.cfi_offset d0,-16\n.cfi_offset d31,-24\n.cfi_restore 63\n.cfi_restore d0\n.zero 60\n.cfi_def_cfa_offset 24\n.zero 64\n.cfi_def_cfa_offset 32\n.zero 256\n.cfi_def_cfa_offset 40\n.zero 65536\n.cfi_def_cfa_offset 48\nret\n.cfi_endproc\n";
    let temp = Temp::new();
    let input = temp.0.join("compact.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let frame_size = |path: &std::path::Path| {
        let bytes = std::fs::read(path).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
        object.section_by_name("__eh_frame").unwrap().size()
    };
    assert_eq!(frame_size(&ours), frame_size(&reference));
    let decoded = dump(&ours, "--eh-frame");
    let expected = dump(&reference, "--eh-frame");
    let rows = |text: &str| {
        let mut first = None;
        text.lines()
            .filter_map(|line| {
                let (address, rule) = line.trim().strip_prefix("0x")?.split_once(": ")?;
                let address = u64::from_str_radix(address, 16).ok()?;
                let start = *first.get_or_insert(address);
                Some((address - start, rule.to_string()))
            })
            .collect::<Vec<_>>()
    };
    assert!(!rows(&decoded).is_empty());
    assert_eq!(rows(&decoded), rows(&expected));
    for opcode in [
        "advance_loc:",
        "advance_loc1:",
        "advance_loc2:",
        "advance_loc4:",
        "offset:",
        "offset_extended:",
        "restore:",
        "restore_extended:",
    ] {
        assert!(
            decoded.contains(&format!("DW_CFA_{opcode}")),
            "{opcode}: {decoded}"
        );
    }
}

#[test]
fn wsp_cfi_alias_matches_sp_and_clang() {
    let source = ".text\n.globl _wsp_frame\n_wsp_frame:\n.cfi_startproc\n.cfi_def_cfa wsp,16\nnop\n.cfi_def_cfa_register wsp\nret\n.cfi_endproc\n";
    let bytes = assemble_object(source).unwrap();
    assert_eq!(
        bytes,
        assemble_object(
            &source
                .replace("wsp,", "sp,")
                .replace("register wsp", "register sp")
        )
        .unwrap()
    );
    let temp = Temp::new();
    let input = temp.0.join("wsp.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&ours, bytes).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    let rules = |path: &std::path::Path| {
        dump(path, "--eh-frame")
            .lines()
            .filter(|line| {
                line.contains("DW_CFA_def_cfa:") || line.contains("DW_CFA_def_cfa_register:")
            })
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>()
    };
    let got = rules(&ours);
    assert!(!got.is_empty());
    assert_eq!(got, rules(&reference));
    run(common::dwarfdump().arg("--verify").arg(&ours));
}

#[test]
fn unwind_addresses_target_each_executable_section() {
    let source = ".text\n.globl _text_frame\n_text_frame:\n.cfi_startproc\n.cfi_def_cfa wsp,16\nret\n.cfi_endproc\n.section __TEXT,__StaticInit,regular,pure_instructions\n.globl _init_frame\n_init_frame:\n.cfi_startproc\n.cfi_def_cfa wsp,16\nret\n.cfi_endproc\n";
    let temp = Temp::new();
    let input = temp.0.join("sections.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    // Check the serialized object references on every host, before the native
    // linker resolves them. Each compact record must point at its own code
    // section, with the exact start offset and four-byte function extent.
    for path in [&ours, &reference] {
        use object::{ObjectSymbol, RelocationTarget};
        let bytes = std::fs::read(path).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let compact = file.section_by_name("__compact_unwind").unwrap();
        let data = compact.data().unwrap();
        assert_eq!(data.len(), 64);
        let mut targets = Vec::new();
        for (offset, relocation) in compact.relocations() {
            assert_eq!(offset % 32, 0);
            assert_eq!(relocation.kind(), object::RelocationKind::Absolute);
            assert_eq!(relocation.size(), 64);
            let at = offset as usize;
            let value = u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
            let (section, address) = match relocation.target() {
                RelocationTarget::Symbol(index) => {
                    let symbol = file.symbol_by_index(index).unwrap();
                    (symbol.section_index().unwrap(), symbol.address() + value)
                }
                RelocationTarget::Section(index) => (index, value),
                target => panic!("unexpected unwind relocation: {target:?}"),
            };
            let section = file.section_by_index(section).unwrap();
            targets.push((
                section.name().unwrap().to_string(),
                address - section.address(),
                u32::from_le_bytes(data[at + 8..at + 12].try_into().unwrap()),
            ));
        }
        targets.sort();
        assert_eq!(
            targets,
            [("__StaticInit".into(), 0, 4), ("__text".into(), 0, 4)]
        );
    }
    // Object-file FDE addresses still contain linker relocations. Retain the
    // final linked-address check wherever the native Mach-O linker is available.
    #[cfg(target_os = "macos")]
    for object in [&ours, &reference] {
        let linked = object.with_extension("dylib");
        run(Command::new("clang")
            .arg("-dynamiclib")
            .arg(object)
            .arg("-o")
            .arg(&linked));
        let decoded = dump(&linked, "--eh-frame");
        let ranges = decoded
            .lines()
            .filter_map(|line| {
                let (_, range) = line.split_once("pc=")?;
                let (start, end) = range.split_once("...")?;
                Some((
                    u64::from_str_radix(start, 16).unwrap(),
                    u64::from_str_radix(end, 16).unwrap(),
                ))
            })
            .collect::<Vec<_>>();
        assert_eq!(ranges.len(), 2, "{decoded}");
        let bytes = std::fs::read(&linked).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        for name in ["_text_frame", "_init_frame"] {
            use object::ObjectSymbol;
            let address = file
                .symbols()
                .find(|symbol| symbol.name().unwrap() == name)
                .unwrap()
                .address();
            assert!(
                ranges.contains(&(address, address + 4)),
                "{name}: {address:x}: {decoded}"
            );
        }
    }
    run(common::dwarfdump().arg("--verify").arg(&ours));
    for directive in [".cfi_def_cfa wsp,16", ".cfi_restore_state", ".cfi_endproc"] {
        let invalid = format!(
            ".text\n.cfi_startproc\nnop\n.section __TEXT,__StaticInit,regular,pure_instructions\n{directive}\n"
        );
        assert!(
            assemble(&invalid)
                .unwrap_err()
                .to_string()
                .contains("cannot span sections")
        );
    }
}
