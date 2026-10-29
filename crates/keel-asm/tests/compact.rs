use keel_asm::assemble;
use object::{Object, ObjectSection};
use std::{fmt::Write, process::Command};
mod common;
use common::{Temp, run};

fn frame(name: &str, rules: &str) -> String {
    format!(".text\n{name}:\n.cfi_startproc\n{rules}\nret\n.cfi_endproc\n")
}
fn encodings(a: &keel_asm::Assembly) -> Vec<u32> {
    a.sections
        .iter()
        .find(|s| s.name == "__compact_unwind")
        .unwrap()
        .data()
        .as_chunks::<32>()
        .0
        .iter()
        .map(|entry| u32::from_le_bytes(entry[12..16].try_into().unwrap()))
        .collect()
}
const FP: &str = ".cfi_def_cfa x29,16\n.cfi_offset x30,-8\n.cfi_offset x29,-16";
#[test]
fn every_canonical_register_pair_subset_matches_clang() {
    let mut source = String::new();
    for mask in 0..512 {
        let mut rules = FP.to_string();
        let mut offset = -24;
        for (i, r) in [19, 21, 23, 25, 27, 72, 74, 76, 78].into_iter().enumerate() {
            if mask & (1 << i) != 0 {
                writeln!(
                    rules,
                    "\n.cfi_offset {r},{offset}\n.cfi_offset {},{}",
                    r + 1,
                    offset - 8
                )
                .unwrap();
                offset -= 16;
            }
        }
        source += &frame(&format!("_f{mask}"), &rules);
    }
    for (i, size) in [0, 16, 32, 4096, 65520].into_iter().enumerate() {
        source += &frame(&format!("_leaf{i}"), &format!(".cfi_def_cfa_offset {size}"));
    }
    source += &frame("_empty", "");
    source += &frame(
        "_objc",
        &format!("{FP}\n.cfi_personality 155,___objc_personality_v0"),
    );
    source += &frame(
        "_gxx",
        &format!("{FP}\n.cfi_personality 155,___gxx_personality_v0"),
    );
    let temp = Temp::new();
    let input = temp.0.join("compact.s");
    let reference = temp.0.join("clang.o");
    std::fs::write(&input, &source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&reference));
    let bytes = std::fs::read(reference).unwrap();
    let reference = object::File::parse(bytes.as_slice()).unwrap();
    let a = assemble(&source).unwrap();
    // One Clang object checks the instructions and the unwind records.
    let text = reference.section_by_name("__text").unwrap();
    assert_eq!(
        a.text,
        text.data().unwrap(),
        "instruction bytes differ from clang"
    );
    assert!((1u64 << a.text_alignment) >= text.align());
    let bytes = a.to_macho().unwrap();
    let ours = object::File::parse(bytes.as_slice()).unwrap();
    let got = ours.section_by_name("__compact_unwind").unwrap();
    let expected = reference.section_by_name("__compact_unwind").unwrap();
    assert_eq!(got.size(), expected.size());
    for (i, (got, want)) in got
        .data()
        .unwrap()
        .as_chunks::<32>()
        .0
        .iter()
        .zip(expected.data().unwrap().as_chunks::<32>().0.iter())
        .enumerate()
    {
        assert_eq!(got, want, "compact record {i}");
    }
    assert!(!a.sections.iter().any(|s| s.name == "__eh_frame"));
}
#[test]
fn unrepresentable_rules_retain_dwarf() {
    for rules in [
        format!("{FP}\n.cfi_offset x19,-32\n.cfi_offset x20,-24"),
        format!("{FP}\n.cfi_offset x19,-24"),
        format!("{FP}\n.cfi_offset x19,-40\n.cfi_offset x20,-48"),
        format!(
            "{FP}\n.cfi_offset x21,-24\n.cfi_offset x22,-32\n.cfi_offset x19,-40\n.cfi_offset x20,-48"
        ),
        format!("{FP}\n.cfi_offset d8,-48\n.cfi_offset d9,-40"),
        format!("{FP}\n.cfi_remember_state\n.cfi_restore_state"),
        format!("{FP}\n.cfi_same_value x19"),
        format!("{FP}\n.cfi_offset x29,-16"),
        format!("{FP}\n.cfi_personality 155,_custom_personality"),
        ".cfi_personality 155,_custom_personality".into(),
        ".cfi_def_cfa x28,16".into(),
        ".cfi_def_cfa x29,32\n.cfi_offset x29,-16\n.cfi_offset x30,-8".into(),
        ".cfi_def_cfa_offset 24".into(),
        ".cfi_def_cfa_offset 65536".into(),
        ".cfi_def_cfa_offset 16\n.cfi_def_cfa_offset 32".into(),
    ] {
        let a = assemble(&frame("_fallback", &rules)).unwrap();
        assert_eq!(encodings(&a), [0x0300_0000], "{rules}");
        assert!(
            a.sections
                .iter()
                .any(|s| s.name == "__eh_frame" && s.size() > 20)
        );
    }
}
#[test]
fn directive_order_does_not_change_frame_semantics() {
    let a=assemble(&frame("_fp_first",".cfi_offset x29,-16\n.cfi_def_cfa x29,16\n.cfi_offset x30,-8\n.cfi_offset d9,-32\n.cfi_offset d8,-24")).unwrap();
    assert_eq!(encodings(&a), [0x0400_0100]);
    let a = assemble(
        &(frame("_compact", FP) + &frame("_dwarf", &format!("{FP}\n.cfi_offset x19,-32"))),
    )
    .unwrap();
    assert_eq!(encodings(&a), [0x0400_0000, 0x0300_0000]);
    let temp = Temp::new();
    let obj = temp.0.join("mixed.o");
    std::fs::write(&obj, a.to_macho().unwrap()).unwrap();
    let output = common::dwarfdump()
        .arg("--eh-frame")
        .arg(obj)
        .output()
        .unwrap();
    assert!(output.status.success());
    let dump = String::from_utf8(output.stdout).unwrap();
    assert_eq!(dump.matches(" FDE cie=").count(), 1);
}

const EXCEPTIONS: &str = r#"
.subsections_via_symbols
.text
.globl _catch_compact
_catch_compact:
Lbegin:
.cfi_startproc
.cfi_personality 155,___gxx_personality_v0
.cfi_lsda 16,Llsda
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x30,-8
.cfi_offset x29,-16
stp x20,x19,[sp,#-16]!
.cfi_offset x19,-24
.cfi_offset x20,-32
stp d9,d8,[sp,#-16]!
.cfi_offset d8,-40
.cfi_offset d9,-48
mov w19,#11
mov w20,#31
fmov d8,#2.0
fmov d9,#3.0
Lfrom: bl _compact_throw
Lto: brk #1
Lpad:
bl ___cxa_begin_catch
bl ___cxa_end_catch
add w0,w19,w20
fcvtzs w1,d8
add w0,w0,w1
fcvtzs w1,d9
add w0,w0,w1
ldp d9,d8,[sp],#16
ldp x20,x19,[sp],#16
ldp x29,x30,[sp],#16
ret
.cfi_endproc
_compact_throw:
.cfi_startproc
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x30,-8
.cfi_offset x29,-16
stp x20,x19,[sp,#-16]!
.cfi_offset x19,-24
.cfi_offset x20,-32
stp d9,d8,[sp,#-16]!
.cfi_offset d8,-40
.cfi_offset d9,-48
mov w19,#0
mov w20,#0
fmov d8,#9.0
fmov d9,#10.0
bl _thrower
brk #2
.cfi_endproc
.section __TEXT,__gcc_except_tab
.p2align 2
GCC_except_table_compact:
Llsda:
.byte 255,155
.uleb128 Lttbase-Lttref
Lttref:
.byte 1
.uleb128 Lcsend-Lcs
Lcs:
.uleb128 Lfrom-Lbegin
.uleb128 Lto-Lfrom
.uleb128 Lpad-Lbegin
.byte 1
Lcsend:
.byte 1,0
.p2align 2
Ltype:
.long __ZTIi@GOT-Ltype
Lttbase:
"#;
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn compact_exception_handlers_restore_integer_and_fp_registers() {
    let a = assemble(EXCEPTIONS).unwrap();
    assert_eq!(encodings(&a), [0x4400_0101, 0x0400_0101]);
    assert!(!a.sections.iter().any(|s| s.name == "__eh_frame"));
    let temp = Temp::new();
    let obj = temp.0.join("compact.o");
    let cpp = temp.0.join("main.cpp");
    let order = temp.0.join("order");
    let exe = temp.0.join("run");
    std::fs::write(&obj, a.to_macho().unwrap()).unwrap();
    std::fs::write(&cpp,r#"extern "C" int catch_compact(); extern "C" void thrower(){throw 42;} int main(){return catch_compact()!=47;}"#).unwrap();
    std::fs::write(&order, "_compact_throw\n_catch_compact\n").unwrap();
    for reorder in [false, true] {
        let mut command = Command::new("clang++");
        command.arg(&cpp).arg(&obj);
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
