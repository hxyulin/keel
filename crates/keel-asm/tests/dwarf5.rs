//! DWARF 5 line tables (`.file 0`, `md5`, `source`) against Clang's integrated
//! assembler, which upgrades to version 5 on `.file 0` exactly as Keel does.
//! Headers, files, checksums, embedded source, `__debug_line_str` bytes and
//! decoded rows are compared; `__debug_line` program bytes are not, because
//! Keel uses standard opcodes where LLVM uses special opcodes.
use keel_asm::{assemble, assemble_object};
use object::{Object, ObjectSection};
use std::path::Path;
use std::process::Command;
#[allow(dead_code)]
mod common;
use common::{Temp, run};

/// Decoded line-table header (version, directories, file entries) and rows.
#[derive(Debug, PartialEq)]
struct Table {
    header: Vec<String>,
    rows: Vec<String>,
}

fn table(path: &Path) -> Table {
    let output = common::dwarfdump()
        .arg("--debug-line")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let (mut header, mut rows, mut file_entry) = (Vec::new(), Vec::new(), false);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version:")
            || trimmed.starts_with("address_size:")
            || trimmed.starts_with("prologue_length:")
            || trimmed.starts_with("include_directories[")
        {
            header.push(trimmed.to_string());
        } else if trimmed.starts_with("file_names[") {
            file_entry = true;
            header.push(trimmed.to_string());
        } else if file_entry
            && ["name:", "dir_index:", "md5_checksum:", "source:"]
                .iter()
                .any(|key| trimmed.starts_with(key))
        {
            header.push(trimmed.to_string());
        } else if line.starts_with("0x") {
            rows.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    Table { header, rows }
}

fn section(path: &Path, name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    file.section_by_name(name)
        .map(|section| section.data().unwrap().to_vec())
}

fn verify(path: &Path) {
    let output = common::dwarfdump()
        .arg("--verify")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Assembles `source` with Keel and Clang, compares the decoded tables and
/// the generated `__debug_line_str`, and verifies Keel's object.
fn compare(source: &str) -> Table {
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let expected = table(&reference);
    let got = table(&ours);
    assert_eq!(got, expected);
    assert!(got.header.iter().any(|s| s == "version: 5"), "{got:?}");
    assert_eq!(
        section(&ours, "__debug_line_str"),
        section(&reference, "__debug_line_str")
    );
    verify(&ours);
    got
}

const MD5_A: &str = "0x00112233445566778899aabbccddeeff";
const MD5_B: &str = "0xffeeddccbbaa99887766554433221100";
const MD5_C: &str = "0x0123456789abcdef0123456789abcdef";

#[test]
fn file_zero_directories_checksums_and_rows_match_clang() {
    let source = format!(
        r#"
.file 0 "/work/project" "src/main.c" md5 {MD5_A}
.file 1 "/work/project" "include/util.h" md5 {MD5_B}
.file 2 "/usr/include" "stdio.h" md5 {MD5_C}
.file 3 "relative/dir" "local.h" md5 0x7
.file 4 "/usr/include/sys/types.h" md5 0xABCDEF
.file 5 "plain.h" md5 {MD5_A}
.file 6 "/usr/include" "other.h" md5 {MD5_B}
.text
.globl _f
_f:
.loc 0 2 0
nop
.loc 0 3 4 prologue_end
nop
.loc 1 7 1 is_stmt 0
nop
.loc 2 8 2 is_stmt 1 basic_block
nop
.loc 3 9 3 discriminator 5
nop
.loc 4 10 1 isa 1
nop
.loc 5 11
nop
.loc 6 12 epilogue_begin
.loc 0 13 6
ret
"#
    );
    let table = compare(&source);
    // Entry 0 is the compilation directory and the root keeps its relative
    // name. A directory equal to it is dropped and the name then split, as
    // are one-string paths; directories are shared by first use.
    for expected in [
        "include_directories[  0] = \"/work/project\"",
        "include_directories[  1] = \"include\"",
        "include_directories[  2] = \"/usr/include\"",
        "include_directories[  3] = \"relative/dir\"",
        "include_directories[  4] = \"/usr/include/sys\"",
        "name: \"src/main.c\"",
        "name: \"util.h\"",
        "md5_checksum: 00000000000000000000000000000007",
        "md5_checksum: 00000000000000000000000000abcdef",
    ] {
        assert!(table.header.iter().any(|s| s == expected), "{expected}");
    }
    assert_eq!(table.rows.len(), 10);
}

#[test]
fn tables_without_checksums_match_clang() {
    compare(
        r#"
.file 0 "/w" "a.c"
.file 1 "/w" "b.h"
.file 2 "/x/y" "c.h"
.text
_g:
.loc 0 1 1
nop
.loc 1 2 2
nop
.loc 2 3 3
ret
"#,
    );
}

/// LLVM accepts mixed checksum presence but warns and omits the checksum
/// column for all entries. File identity still uses the supplied checksum.
#[test]
fn mixed_checksums_match_clang_without_inventing_values() {
    for files in [
        format!(".file 0 \"/w\" \"a.c\"\n.file 1 \"/w\" \"b.h\" md5 {MD5_B}"),
        format!(".file 0 \"/w\" \"a.c\" md5 {MD5_A}\n.file 1 \"/w\" \"b.h\""),
        // Equal names with different checksum presence must remain distinct.
        format!(".file 0 \"/w\" \"a.c\"\n.file 1 \"/w\" \"a.c\" md5 {MD5_A}"),
        format!(".file 0 \"/w\" \"a.c\" md5 {MD5_A}\n.file 1 \"/w\" \"a.c\""),
    ] {
        let table = compare(&format!(
            "{files}\n.text\n_f:\n.loc 0 1 0\nnop\n.loc 1 2 1\nret\n"
        ));
        assert!(
            !table
                .header
                .iter()
                .any(|entry| entry.starts_with("md5_checksum:")),
            "{table:?}"
        );
        assert_eq!(table.rows.len(), 3);
    }
}

#[test]
fn embedded_source_matches_clang() {
    // An absent source becomes an empty string once any entry has one.
    let table = compare(&format!(
        r#"
.file 0 "/w" "a.c" md5 {MD5_A} source "int main(void) {{\n  return \"\\061\";\n}}\n"
.file 1 "/w" "b.h" md5 {MD5_B} source "enum {{ B = 1 }};\n"
.file 2 "/w" "c.h" md5 {MD5_C}
.file 3 "/w" "d.h" md5 {MD5_A} source "int main(void) {{\n  return \"\\061\";\n}}\n"
.text
_main:
.loc 0 1 0
nop
.loc 1 1 0
nop
.loc 2 1 0
nop
.loc 3 2 3
ret
"#
    ));
    assert!(
        table
            .header
            .iter()
            .any(|s| s.starts_with("source: \"int main(void)")),
        "{table:?}"
    );
}

/// An empty embedded source is "\n", so it differs from an absent one: upstream
/// LLVM and DWARF issue 180201.1. Apple clang-2100 still writes "" here.
#[test]
fn empty_embedded_source_is_a_newline() {
    let source = format!(
        ".file 0 \"/w\" \"a.c\" md5 {MD5_A} source \"x\"\n.file 1 \"/w\" \"b.h\" md5 {MD5_B} source \"\"\n.file 2 \"/w\" \"c.h\" md5 {MD5_C}\n.text\n.loc 1 1 0\nret\n"
    );
    let temp = Temp::new();
    let ours = temp.0.join("ours.o");
    std::fs::write(&ours, assemble_object(&source).unwrap()).unwrap();
    assert_eq!(
        section(&ours, "__debug_line_str").unwrap(),
        b"/w\0a.c\0x\0b.h\0\n\0c.h\0\0"
    );
    let table = table(&ours);
    assert!(
        table.header.contains(&"source: \"\\n\"".to_string()),
        "{table:?}"
    );
    verify(&ours);
}

#[test]
fn header_only_table_without_rows_matches_clang() {
    // Data-only translation units still reference a line table from
    // DW_AT_stmt_list; LLVM emits one once a file table exists.
    let table = compare(&format!(
        ".file 0 \"/w\" \"data.c\" md5 {MD5_A}\n.file 1 \"/usr/include\" \"x.h\" md5 {MD5_B}\n.data\n_value:\n.long 7\n"
    ));
    assert!(table.rows.is_empty());
}

#[test]
fn root_duplicate_is_folded_like_llvm() {
    // LLVM's tryGetFile returns 0 for a name and checksum equal to the root
    // file (the directory is not compared) without allocating the number,
    // so the number may be declared again.
    compare(&format!(
        r#"
.file 0 "/w" "a.c" md5 {MD5_A}
.file 1 "/elsewhere" "a.c" md5 {MD5_A}
.file 1 "/w" "b.c" md5 {MD5_B}
.text
_f:
.loc 1 4 0
nop
.loc 0 5 0
ret
"#
    ));
    let folded = format!(
        ".file 0 \"/w\" \"a.c\" md5 {MD5_A}\n.file 1 \"/w\" \"a.c\" md5 {MD5_A}\n.text\n.loc 1 1 0\nret\n"
    );
    let temp = Temp::new();
    let input = temp.0.join("folded.s");
    std::fs::write(&input, &folded).unwrap();
    let clang = Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c", "-o"])
        .arg(temp.0.join("clang.o"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        !clang.status.success(),
        "Clang accepted .loc to a folded file"
    );
    let error = assemble(&folded).unwrap_err().to_string();
    assert!(error.contains("matched the .file 0 root file"), "{error}");
}

/// Unchanged `clang -gdwarf-5 -S` output: every section other than the
/// generated line program is byte-identical, the line tables and
/// `__debug_line_str` match, and the linked program's dSYM verifies.
/// `-O0` avoids cross-function LEB128 range offsets (see docs/targets/aarch64/assembler.md).
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unchanged_clang_dwarf5_output_matches_and_links() {
    let temp = Temp::new();
    std::fs::create_dir(temp.0.join("inc")).unwrap();
    std::fs::write(
        temp.0.join("inc/scale.h"),
        "static inline int scale(int x) {\n  return x * 3 + 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        temp.0.join("prog.c"),
        r#"#include "inc/scale.h"
struct point { int x, y; };
static int sum(const struct point *p, int n) {
  int s = 0;
  for (int i = 0; i < n; i++)
    s += p[i].x * p[i].y + scale(i);
  return s;
}
int main(int argc, char **argv) {
  struct point pts[3] = {{1, 2}, {3, 4}, {argc, (int)(argv != 0)}};
  return sum(pts, 3) == 27 ? 0 : 1;
}
"#,
    )
    .unwrap();
    for (name, flags) in [
        ("plain", &["-gdwarf-5"][..]),
        ("embedded", &["-gdwarf-5", "-gembed-source"][..]),
    ] {
        let assembly = temp.0.join(format!("{name}.s"));
        let reference = temp.0.join(format!("{name}.clang.o"));
        let ours = temp.0.join(format!("{name}.keel.o"));
        let exe = temp.0.join(name);
        run(Command::new("clang")
            .current_dir(&temp.0)
            .args(["-target", "arm64-apple-macos11", "-O0", "-S"])
            .args(flags)
            .args(["prog.c", "-o"])
            .arg(&assembly));
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(&assembly)
            .arg("-o")
            .arg(&reference));
        let source = std::fs::read_to_string(&assembly).unwrap();
        assert!(source.contains(".file\t0 "), "{name}: no .file 0");
        std::fs::write(&ours, assemble_object(&source).unwrap()).unwrap();
        let expected = table(&reference);
        assert_eq!(table(&ours), expected, "{name}");
        assert!(
            expected
                .header
                .iter()
                .any(|s| s.starts_with("md5_checksum"))
        );
        if name == "embedded" {
            assert!(expected.header.iter().any(|s| s.starts_with("source:")));
        }
        let bytes = std::fs::read(&ours).unwrap();
        let keel = object::File::parse(bytes.as_slice()).unwrap();
        let bytes = std::fs::read(&reference).unwrap();
        let clang = object::File::parse(bytes.as_slice()).unwrap();
        for expected in clang.sections() {
            let name = expected.name().unwrap();
            if name == "__debug_line" || name == "__compact_unwind" {
                continue;
            }
            let got = keel.section_by_name(name).unwrap();
            assert_eq!(got.data().unwrap(), expected.data().unwrap(), "{name}");
        }
        for name in [
            "__debug_str_offs",
            "__debug_addr",
            "__debug_names",
            "__debug_line_str",
        ] {
            assert!(keel.section_by_name(name).is_some(), "{name}");
        }
        verify(&ours);
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11"])
            .arg(&ours)
            .arg("-o")
            .arg(&exe));
        assert!(common::execute(&exe).success());
        run(Command::new("dsymutil").arg(&exe));
        verify(&temp.0.join(format!("{name}.dSYM")));
    }
}

/// HEAD a24ae66's exact DWARF 4 `__debug_line` for this input: without
/// `.file 0` nothing about the version 4 table may change.
#[test]
fn dwarf4_tables_are_unchanged_without_file_zero() {
    let source = ".file 1 \"/work\" \"a.c\"\n.file 2 \"/usr/include\" \"b.h\"\n.file 3 \"c.c\"\n.text\n.globl _f\n_f:\n.loc 1 3 2\nnop\n.loc 2 7 1 prologue_end\nnop\n.loc 3 9 0 is_stmt 0 discriminator 3\nnop\n.loc 1 4 5 is_stmt 1 isa 2 basic_block epilogue_begin\nret\n";
    let expected = "7d00000004003c000000010101fb0e0d0001010101000000010000012f776f726b002f7573722f696e636c7564650000612e6300010000622e6800020000632e6300000000000009020000000000000000030205020102040402030405010a01020404030302050000020403060102040401037b05050c0206070b010204000101";
    let assembly = assemble(source).unwrap();
    let line = assembly
        .sections
        .iter()
        .find(|s| s.name == "__debug_line")
        .unwrap();
    let hex: String = line.data().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, expected);
    assert!(
        assembly
            .sections
            .iter()
            .all(|s| s.name != "__debug_line_str")
    );
}

#[test]
fn dwarf5_sections_pass_through_with_debug_attributes() {
    let source = ".section __DWARF,__debug_str_offs,regular,debug\n.long 8\n.section __DWARF,__debug_addr,regular,debug\n.quad 1\n.section __DWARF,__debug_rnglists,regular,debug\n.byte 0\n.section __DWARF,__debug_loclists,regular,debug\n.byte 0\n.section __DWARF,__debug_names,regular,debug\n.long 0\n.section __DWARF,__debug_line_str,regular,debug\n.asciz \"x\"\n";
    let temp = Temp::new();
    let input = temp.0.join("sections.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let bytes = std::fs::read(&ours).unwrap();
    let keel = object::File::parse(bytes.as_slice()).unwrap();
    let bytes = std::fs::read(&reference).unwrap();
    let clang = object::File::parse(bytes.as_slice()).unwrap();
    for name in [
        "__debug_str_offs",
        "__debug_addr",
        "__debug_rnglists",
        "__debug_loclists",
        "__debug_names",
        "__debug_line_str",
    ] {
        let (got, expected) = (
            keel.section_by_name(name).unwrap(),
            clang.section_by_name(name).unwrap(),
        );
        assert_eq!(got.data().unwrap(), expected.data().unwrap(), "{name}");
        assert_eq!(got.flags(), expected.flags(), "{name}");
        assert_eq!(got.segment_name().unwrap(), Some("__DWARF"));
    }
}

#[test]
fn explicit_line_strings_precede_generated_ones() {
    // Generated line_strp offsets account for explicit contents; LLVM would
    // emit offsets from zero here. Offsets are checked through the decoder.
    let source = ".section __DWARF,__debug_line_str,regular,debug\n.asciz \"explicit\"\n.file 0 \"/w\" \"a.c\"\n.text\n.loc 0 1 0\nret\n";
    let temp = Temp::new();
    let ours = temp.0.join("ours.o");
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let table = table(&ours);
    assert!(
        table
            .header
            .contains(&"include_directories[  0] = \"/w\"".to_string())
    );
    assert!(table.header.contains(&"name: \"a.c\"".to_string()));
    assert_eq!(
        section(&ours, "__debug_line_str").unwrap(),
        b"explicit\0/w\0a.c\0"
    );
    verify(&ours);
}

#[test]
fn malformed_dwarf5_directives_are_diagnosed() {
    let root = format!(".file 0 \"/w\" \"a.c\" md5 {MD5_A}\n");
    let cases: Vec<(String, &str)> = vec![
        (".file 0 \"a.c\"".into(), "nonempty compilation directory"),
        (
            ".file 0 \"\" \"a.c\"".into(),
            "nonempty compilation directory",
        ),
        (".file 0 \"/w\" \"\"".into(), "nonempty file name"),
        (format!("{root}{root}"), "duplicate .file 0"),
        (
            format!(".file 1 \"/w\" \"b.c\"\n{root}"),
            "must precede numbered .file",
        ),
        (
            format!(".file 1 \"/w\" \"b.c\"\n.text\n.loc 1 1 0\n{root}"),
            "must precede numbered .file",
        ),
        (".text\n.loc 0 1 0".into(), "at least one without .file 0"),
        (
            ".file 1 \"/w\" \"b.c\"\n.text\n.loc 0 1 0".into(),
            "at least one without .file 0",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 0xg1"),
            "md5 checksum",
        ),
        (format!("{root}.file 1 \"/w\" \"b.c\" md5"), "integer md5"),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 0x1{}", "0".repeat(32)),
            "at most 128 bits",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 -1"),
            "integer md5",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 0x+1"),
            "integer md5",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A} md5 {MD5_A}"),
            "duplicate .file md5",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A} source \"x\" source \"y\""),
            "duplicate .file source",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A} source \"x"),
            "unterminated string",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A} source \"a\\000b\""),
            "source contains NUL",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A} junk"),
            "unexpected token in .file",
        ),
        (
            format!("{root}.file 1 \"dir/\" md5 {MD5_A}"),
            "must not end with `/`",
        ),
        (
            format!("{root}.file 1 \"//net/a.c\" md5 {MD5_A}"),
            "start with `//`",
        ),
        (
            format!("{root}.file 1 \"/w\" \"b.c\" md5 {MD5_A}\n.file 1 \"/w\" \"c.c\" md5 {MD5_B}"),
            "duplicate",
        ),
        (
            format!("{root}.file 2 \"/w\" \"b.c\" md5 {MD5_A}\n.text\n.loc 2 1 0\nret"),
            "contiguous starting at 1",
        ),
        (
            format!("{root}.file 2 \"/w\" \"b.c\" md5 {MD5_A}"),
            "contiguous starting at 1",
        ),
        (format!("{root}.text\n.loc 1 1 0"), "undeclared file"),
        (
            format!(
                "{root}.file 1 \"/x\" \"a.c\" md5 {MD5_A} source \"other\"\n",
                root = ".file 0 \"/w\" \"a.c\" md5 0x00112233445566778899aabbccddeeff source \"mine\"\n"
            ),
            "source differs",
        ),
        (
            ".file 1 \"/d\" \"a.c\" md5 0x1".into(),
            "require DWARF 5 line tables; declare the root file with .file 0",
        ),
        (
            ".file 1 \"/d\" \"a.c\" source \"x\"".into(),
            "require DWARF 5 line tables",
        ),
        (
            format!("{root}.section __DWARF,__debug_line,regular,debug\n.byte 0"),
            "explicit __debug_line",
        ),
    ];
    for (source, message) in cases {
        let error = assemble(&source)
            .err()
            .unwrap_or_else(|| panic!("accepted {source:?}"))
            .to_string();
        assert!(error.contains(message), "{source:?}: {error}");
    }
}

#[test]
fn x86_elf_accepts_directory_and_dwarf5_root_but_requires_root_for_extensions() {
    for source in [".file 0 \"/w\" \"a.c\"", ".file 1 \"/w\" \"a.c\""] {
        keel_asm::x86::assemble_object(source).unwrap();
    }
    for source in [
        ".file 1 \"a\\\".c\" source \"x\"",
        ".file 1 \"a.c\" md5 0x00112233445566778899aabbccddeeff",
        ".file 1 \"a.c\" source \"x\"",
    ] {
        assert!(
            keel_asm::x86::assemble_object(source).is_err(),
            "x86 accepted {source}"
        );
    }
    // The single-path form, including escaped quotes, still works.
    keel_asm::x86::assemble_object(".file 1 \"a\\\"b.c\"\n.text\n.loc 1 2 3\nret\n").unwrap();
}

#[test]
fn line_sequences_across_text_sections_match_clang() {
    let source = r#"
.file 0 "/w" "a.c"
.section __TEXT,__StaticInit,regular,pure_instructions
_init:
.loc 0 10 0
nop
.text
_f:
.loc 0 20 0
nop
.section __TEXT,__StaticInit,regular,pure_instructions
.loc 0 11 0
ret
.text
.loc 0 21 0
ret
"#;
    let table = compare(source);
    assert_eq!(
        table
            .rows
            .iter()
            .filter(|row| row.contains("end_sequence"))
            .count(),
        2,
        "{table:?}"
    );
}
