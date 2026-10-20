//! Syntax that Clang's own AArch64 output uses: Apple mnemonic-suffix SIMD,
//! the scalar forms that appeared alongside it in QuickJS and SQLite, and
//! DWARF 4 two-string `.file` with `.loc` row state.
use keel_asm::{TargetFeatures, assemble, assemble_object, assemble_with_features};
use object::{Object, ObjectSection, ObjectSymbol, SectionKind};
use std::process::Command;
#[allow(dead_code)]
mod common;
use common::{Temp, compare_clang_with_features, run};

fn compare(source: &str) {
    compare_clang_with_features(source, TargetFeatures::apple_m5(), &["-mcpu=apple-m5"]);
}

#[test]
fn apple_suffix_roles_match_clang() {
    compare(
        "
        dup.2d v0, x8
        dup.4s v1, w8
        dup.4h v2, w12
        dup.2s v3, w10
        dup.16b v4, w1
        dup.2d v2, v2[1]
        dup.4s v5, v6[3]
        dup.8h v7, v8[7]
        dup.16b v9, v10[15]
        mov.h v0[0], w11
        mov.b v1[15], w2
        umov.h w0, v2[3]
        umov.b w1, v3[0]
        smov.h x2, v4[1]
        ins.s v5[1], w6
        tbl.16b v30, { v28 }, v27
        tbl.8b v0, { v1, v2 }, v3
        tbx.16b v4, { v5, v6, v7 }, v8
        ld2.4s { v1, v2 }, [x13]
        st2.2d { v0, v1 }, [x12]
        ld2.16b { v20, v21 }, [x15], #32
        ld4.4s { v3, v4, v5, v6 }, [x15], #64
        st1.8b { v0 }, [x1]
        ext.16b v1, v0, v0, #8
        cmeq.16b v18, v18, #0
        cmlt.4h v0, v0, #0
        cmhi.4s v3, v1, v0
        cmhs.2d v3, v1, v0
        xtn.8b v0, v1
        xtn.4h v2, v2
        xtn2.16b v0, v1
        shrn.2s v0, v0, #32
        shrn2.8h v0, v1, #3
        umull.2d v0, v0, v1
        umull2.2d v2, v0, v1
        smull.8h v2, v0, v1
        saddw.4s v1, v1, v17
        saddw2.4s v0, v0, v17
        ssubw.2d v2, v2, v24
        usubw2.8h v5, v5, v24
        uzp1.16b v0, v1, v0
        uzp2.4s v1, v1, v2
        trn1.8h v1, v2, v3
        rev64.2s v0, v0
        rev16.16b v1, v1
        rev32.8b v0, v0
        sli.16b v2, v18, #4
        ushr.8h v4, v4, #5
        sshr.4s v4, v4, #31
        shl.2s v16, v16, #8
        ushl.2d v1, v0, v1
        mls.4s v0, v1, v2
        mla.8h v0, v1, v2
        mvni.2s v1, #9
        umaxv.4s s0, v0
        sminv.8h h0, v1
        smax.4s v1, v1, v17
        umin.16b v1, v1, v17
        neg.2d v6, v3
        fneg.4s v1, v1
        bit.16b v0, v1, v2
        bif.8b v0, v1, v2
        orn.8b v0, v0, v3
        udot.4s v0, v5, v1
        udot.2s v0, v2, v1
        ",
    );
}

#[test]
fn scalar_forms_from_clang_sources_match_clang() {
    compare(
        "
        frinta d4, d1
        frinta s4, s1
        frintx d2, d0
        frintx s2, s0
        fmaxnm d8, d8, d0
        fmaxnm s8, s8, s0
        fminnm d8, d8, d0
        fminnm s8, s8, s0
        smsubl x11, w23, w11, x10
        umsubl x11, w13, w11, x12
        smaddl x1, w2, w3, x4
        ",
    );
}

#[test]
fn malformed_apple_suffix_operands_are_diagnosed() {
    let m5 = TargetFeatures::apple_m5();
    for (source, message) in [
        ("dup.2d v0, w8", "dup requires W for B/H/S or X for D"),
        ("dup.4s v0, x8", "dup requires W for B/H/S or X for D"),
        (
            "dup.2d v0, v1.d[0]",
            "Apple suffix requires unqualified vector registers",
        ),
        ("dup.4s v32, w0", "invalid SIMD register `v32`"),
        ("dup.2d v0, v1[2]", ""),
        (
            "tbl.16b v0, { v1.16b }, v2",
            "Apple suffix requires unqualified vector registers",
        ),
        (
            "tbl.16b v0, v1, { v2 }",
            "unexpected Apple SIMD register list",
        ),
        (
            "add.4s v0, { v1 }, v2",
            "unexpected Apple SIMD register list",
        ),
        ("ld2.4s { v1, x2 }, [x3]", "expected vector register list"),
        ("xtn.16b v0, v1", "invalid narrowing suffix"),
        ("xtn2.8b v0, v1", "invalid narrowing suffix"),
        ("saddw.8b v0, v1, v2", "invalid widening suffix"),
        ("umull2.8b v0, v1, v2", "invalid widening suffix"),
        ("udot.8h v0, v1, v2", "invalid dot-product suffix"),
        (
            "sqadd.4s v0, v1, v2",
            "unsupported Apple SIMD mnemonic suffix",
        ),
        ("add.4s v0, v1[0], v2", ""),
        ("mov.h v0[8], w1", ""),
        ("umov.h x0, v2[0]", ""),
        (
            "smsubl x0, x1, w2, x3",
            "invalid widening/high multiply register widths",
        ),
        ("frinta d0, s1", ""),
    ] {
        let error = assemble_with_features(source, m5).expect_err(source);
        assert!(
            error.message.contains(message),
            "{source}: {}",
            error.message
        );
    }
    // Feature checks still apply after rewriting.
    let error = assemble("udot.4s v0, v1, v2").unwrap_err();
    assert!(
        error.message.contains("requires FEAT_DotProd"),
        "{}",
        error.message
    );
}

fn dump(path: &std::path::Path, flag: &str) -> String {
    let output = common::dwarfdump()
        .arg(flag)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Header file/directory tables and every decoded row field, including
/// is_stmt, prologue_end, epilogue_begin, basic_block, ISA and discriminator.
fn line_table(path: &std::path::Path) -> Vec<String> {
    dump(path, "--debug-line")
        .lines()
        .filter(|s| {
            s.starts_with("0x")
                || s.starts_with("include_directories")
                || s.starts_with("file_names")
                || s.trim_start().starts_with("name:")
                || s.trim_start().starts_with("dir_index:")
        })
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn two_string_files_and_loc_state_match_clang() {
    let source = r#"
.file 1 "/work/project" "src/main.c"
.file 2 "/work/project" "include/util.h"
.file 3 "/usr/include" "stdio.h"
.file 4 "" "plain.c"
.text
.globl _f
_f:
.loc 1 2 0
nop
.loc 1 3 4 prologue_end
nop
.loc 1 3 9 is_stmt 0
nop
.loc 2 7 1
nop
.loc 3 8 2 is_stmt 1 basic_block
nop
.loc 1 9 3 discriminator 5
nop
.loc 1 9 3 isa 1
nop
.loc 4 10
nop
.loc 1 11 epilogue_begin
.loc 1 12 6
ret
"#;
    let temp = Temp::new();
    let input = temp.0.join("lines.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, assemble_object(source).unwrap()).unwrap();
    let expected = line_table(&reference);
    assert!(
        expected.iter().any(|s| s.contains("is_stmt")),
        "{expected:?}"
    );
    assert_eq!(line_table(&ours), expected);
    run(common::dwarfdump().arg("--verify").arg(&ours));
}

#[test]
fn malformed_file_and_loc_operands_are_diagnosed() {
    for (source, message) in [
        (
            ".file 0 \"a.c\"",
            ".file 0 requires a nonempty compilation directory",
        ),
        (
            ".file 1 \"/d\" \"a.c\" md5 0x00112233445566778899aabbccddeeff",
            "require DWARF 5",
        ),
        (".file 1 \"/d\" \"a.c\" source \"x\"", "require DWARF 5"),
        (".file 1 \"/d\" \"a.c\" junk", "unexpected token in .file"),
        (".file 1 \"/d\" a.c", "unexpected token in .file"),
        (".file 1 \"/d", "unterminated string"),
        (".file \"a.c\"", "unnumbered .file is unsupported"),
        (".file x \"a.c\"", "invalid file number"),
        (".file 1 \"a\" \"b\"\n.file 1 \"c\" \"d\"", "duplicate"),
        (".file 1 \"a\\0\" \"b\"", "NUL"),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 1 0 is_stmt 2",
            "is_stmt value must be 0 or 1",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 1 0 is_stmt",
            "is_stmt value must be 0 or 1",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 1 0 view 0",
            "unknown .loc sub-directive `view`",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 1 0 isa",
            "invalid .loc isa value",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 1 0 discriminator -1",
            "invalid .loc discriminator",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 1 -1 0",
            "invalid .loc line or column",
        ),
        (
            ".file 1 \"a\" \"b\"\n.loc 0 1 0",
            "file number must be at least one",
        ),
        (".file 1 \"a\" \"b\"\n.loc 2 1 0", "undeclared file"),
        (".file 1 \"a\" \"b\"\n.loc", "invalid .loc file number"),
        (
            ".file 1 \"a\" \"b\"\n.data\n.loc 1 1 0",
            ".loc requires __text",
        ),
        (
            ".file 1 \"a\" \"b\"\n.file 3 \"a\" \"c\"\n.loc 1 1 0\nret",
            "contiguous",
        ),
    ] {
        let error = assemble(source).expect_err(source);
        assert!(
            error.message.contains(message),
            "{source}: {}",
            error.message
        );
    }
}

#[test]
fn common_zerofill_matches_clang() {
    let source = "
.globl _a
.zerofill __DATA,__common,_a,8,3
.globl _b
.zerofill __DATA,__common,_b,4,2
.zerofill __DATA,__bss,_c,16,4
";
    let temp = Temp::new();
    let input = temp.0.join("common.s");
    let reference = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    let theirs = std::fs::read(reference).unwrap();
    let theirs = object::File::parse(theirs.as_slice()).unwrap();
    let ours = assemble_object(source).unwrap();
    let ours = object::File::parse(ours.as_slice()).unwrap();
    let describe = |file: &object::File| {
        let mut sections: Vec<_> = file
            .sections()
            .filter(|s| s.kind() == SectionKind::UninitializedData)
            .map(|s| (s.name().unwrap().to_owned(), s.size(), s.align(), s.flags()))
            .collect();
        sections.sort_by(|a, b| a.0.cmp(&b.0));
        let mut symbols: Vec<_> = file
            .symbols()
            .filter(|s| s.name().is_ok_and(|n| n.starts_with('_')))
            .map(|s| {
                let section = s
                    .section_index()
                    .map(|i| file.section_by_index(i).unwrap().name().unwrap().to_owned());
                let base = s
                    .section_index()
                    .map_or(0, |i| file.section_by_index(i).unwrap().address());
                (
                    s.name().unwrap().to_owned(),
                    s.is_global(),
                    section,
                    s.address() - base,
                )
            })
            .collect();
        symbols.sort();
        (sections, symbols)
    };
    assert_eq!(describe(&ours), describe(&theirs));
    for source in [
        ".zerofill __DATA,__data,_x,8,3",
        ".zerofill __TEXT,__common,_x,8,3",
    ] {
        let error = assemble(source).unwrap_err();
        assert!(
            error
                .message
                .contains("zerofill requires __DATA,__bss or __DATA,__common"),
            "{}",
            error.message
        );
    }
}
