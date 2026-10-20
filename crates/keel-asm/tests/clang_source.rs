#![allow(dead_code)]
mod common;
use common::{Temp, run};
use std::process::Command;

fn build_command(bytes: &[u8]) -> &[u8] {
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut at = 32;
    for _ in 0..word(16) {
        let size = word(at + 4) as usize;
        if word(at) == 0x32 {
            return &bytes[at..at + size];
        }
        at += size;
    }
    panic!("missing LC_BUILD_VERSION");
}

#[test]
fn deployment_metadata_matches_clang() {
    let temp = Temp::new();
    for directive in [
        ".build_version macos, 13, 2, 1 sdk_version 27, 0",
        ".build_version ios, 17, 0 sdk_version 18, 1, 2",
    ] {
        let source = format!("{directive}\n.text\nret\n");
        let input = temp.0.join("input.s");
        let output = temp.0.join("clang.o");
        std::fs::write(&input, &source).unwrap();
        run(Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-c"])
            .arg(input)
            .arg("-o")
            .arg(&output));
        let reference = std::fs::read(output).unwrap();
        let actual = keel_asm::assemble_object(&source).unwrap();
        assert_eq!(build_command(&actual), build_command(&reference));
    }
    for source in [
        ".build_version macos, 1",
        ".build_version macos, 1, 256",
        ".build_version macos, 65536, 0",
        ".build_version nonsense, 1, 0",
        ".build_version macos, 11, 0\n.build_version macos, 12, 0",
        ".build_version macos, 11, 0 sdk_version 27",
    ] {
        assert!(keel_asm::assemble(source).is_err(), "{source}");
    }
}

#[test]
fn unchanged_clang_simple_source() {
    let temp = Temp::new();
    let input = temp.0.join("input.c");
    let assembly = temp.0.join("input.s");
    std::fs::write(&input, "long f(long x) { return x * 17 + 3; }\n").unwrap();
    run(Command::new("clang")
        .args([
            "-S",
            "-O2",
            "-target",
            "arm64-apple-macosx11.0.0",
            "-mcpu=apple-m5",
        ])
        .arg(input)
        .arg("-o")
        .arg(&assembly));
    let source = std::fs::read_to_string(assembly).unwrap();
    common::compare_clang_with_features(
        &source,
        keel_asm::TargetFeatures::apple_m5(),
        &["-mcpu=apple-m5"],
    );
    keel_asm::assemble_object_with_features(&source, keel_asm::TargetFeatures::apple_m5()).unwrap();
}

#[test]
fn linker_hints_and_literal_metadata_match_clang() {
    use object::{Object, ObjectSection};
    let source = ".text\n.globl _f\n_f:\nL0: adrp x0, _data@PAGE\nL1: add x0, x0, _data@PAGEOFF\nL2: ldr x1, [x0]\nret\n.loh AdrpAdd L0, L1\n.loh AdrpAddLdr L0, L1, L2\n.section __TEXT,__literal4,4byte_literals\n.p2align 2, 0\n.long 123\n.section __TEXT,__literal8,8byte_literals\n.p2align 3, 0\n.quad 456\n.section __TEXT,__literal16,16byte_literals\n.p2align 4, 0\n.quad 7, 8\n.section __TEXT,__cstring,cstring_literals\n.asciz \"a\\b\\f\"\n.subsections_via_symbols\n";
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&output));
    let reference = std::fs::read(output).unwrap();
    let actual = keel_asm::assemble_object(source).unwrap();
    fn hints(bytes: &[u8]) -> &[u8] {
        let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let mut at = 32;
        for _ in 0..word(16) {
            if word(at) == 0x2e {
                let start = word(at + 8);
                return &bytes[start..start + word(at + 12)];
            }
            at += word(at + 4);
        }
        panic!("missing linker hints");
    }
    assert_eq!(hints(&actual), hints(&reference));
    let actual = object::File::parse(actual.as_slice()).unwrap();
    let reference = object::File::parse(reference.as_slice()).unwrap();
    for section in reference.sections() {
        let actual = actual.section_by_name(section.name().unwrap()).unwrap();
        let flags = |s: object::SectionFlags| match s {
            object::SectionFlags::MachO { flags } => flags & !0x300,
            _ => panic!("expected Mach-O"),
        };
        assert_eq!(flags(actual.flags()), flags(section.flags()));
        assert!(actual.align() >= section.align());
        assert_eq!(actual.data().unwrap(), section.data().unwrap());
        assert_eq!(actual.relocations().count(), section.relocations().count());
    }
    for bad in [
        ".loh Bad L0, L1",
        ".loh AdrpAdd L0",
        ".loh AdrpAdd L0, Lmissing",
        ".loh AdrpAdd L0, Lend",
    ] {
        assert!(
            keel_asm::assemble(&format!(".text\nL0: nop\nL1: ret\nLend:\n{bad}\n")).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn unchanged_clang_pac_call_frame() {
    let temp = Temp::new();
    let input = temp.0.join("input.c");
    let assembly = temp.0.join("input.s");
    std::fs::write(
        &input,
        "extern long g(long); long f(long x) { return g(x) + 1; }\n",
    )
    .unwrap();
    run(Command::new("clang")
        .args([
            "-S",
            "-O2",
            "-target",
            "arm64-apple-macosx11.0.0",
            "-mcpu=apple-m5",
            "-mbranch-protection=standard",
            "-fasynchronous-unwind-tables",
        ])
        .arg(input)
        .arg("-o")
        .arg(&assembly));
    let source = std::fs::read_to_string(assembly).unwrap();
    common::compare_clang_with_features(
        &source,
        keel_asm::TargetFeatures::apple_m5(),
        &["-mcpu=apple-m5"],
    );
    let result =
        keel_asm::assemble_with_features(&source, keel_asm::TargetFeatures::apple_m5()).unwrap();
    assert!(
        source.contains("paciasp") && source.contains("retaa"),
        "{source}"
    );
    // Upstream LLVM intentionally omits negate_ra_state for Mach-O; Apple
    // clang emits it. Compare the producer's actual unwind rules, including
    // signed-RA state when present, rather than assuming Apple's producer.
    compare_unwind(&temp, &source, &result);
}

fn compare_unwind(temp: &Temp, source: &str, result: &keel_asm::Assembly) {
    use object::{Object, ObjectSection};
    let input = temp.0.join("unwind.s");
    let reference = temp.0.join("clang.o");
    let ours = temp.0.join("ours.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-mcpu=apple-m5", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    std::fs::write(&ours, result.to_macho().unwrap()).unwrap();
    let compact = |path: &std::path::Path| {
        let bytes = std::fs::read(path).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        file.section_by_name("__compact_unwind")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(compact(&ours), compact(&reference));
    let rows = |path: &std::path::Path| {
        let output = common::dwarfdump()
            .arg("--eh-frame")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let decoded = String::from_utf8(output.stdout).unwrap();
        let mut first = None;
        decoded
            .lines()
            .filter_map(|line| {
                let (address, rule) = line.trim().strip_prefix("0x")?.split_once(": ")?;
                let address = u64::from_str_radix(address, 16).ok()?;
                let start = *first.get_or_insert(address);
                Some((address - start, rule.to_string()))
            })
            .collect::<Vec<_>>()
    };
    let expected = rows(&reference);
    assert!(!expected.is_empty());
    assert_eq!(rows(&ours), expected);
}

#[test]
fn explicit_pac_call_frame_retains_signed_ra_state() {
    // Keep signed-RA DWARF coverage unconditional even when the installed
    // Clang producer omits that directive for Mach-O.
    let source = ".text\n.globl _f\n_f:\n.cfi_startproc\npaciasp\n.cfi_negate_ra_state\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa w29,16\n.cfi_offset w30,-8\n.cfi_offset w29,-16\nbl _g\nldp x29,x30,[sp],#16\nretaa\n.cfi_endproc\n";
    let result =
        keel_asm::assemble_with_features(source, keel_asm::TargetFeatures::apple_m5()).unwrap();
    assert!(
        result
            .sections
            .iter()
            .find(|s| s.name == "__eh_frame")
            .unwrap()
            .data()
            .contains(&0x2d)
    );
    let temp = Temp::new();
    compare_unwind(&temp, source, &result);
    let output = common::dwarfdump()
        .arg("--eh-frame")
        .arg(temp.0.join("ours.o"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("DW_CFA_AARCH64_negate_ra_state")
    );
}

#[test]
fn common_symbols_and_shifted_jump_tables_match_clang() {
    use object::{Object, ObjectSymbol};
    let source = ".comm _common,24,4\n.text\n.globl _f\n_f:\nL0: nop\nL1: ret\n.section __TEXT,__const\n.byte (L1-L0)>>2\n.quad (L0-L1)>>2\n.subsections_via_symbols\n";
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .arg(input)
        .arg("-o")
        .arg(&output));
    let reference = std::fs::read(output).unwrap();
    let actual = keel_asm::assemble_object(source).unwrap();
    let reference = object::File::parse(reference.as_slice()).unwrap();
    let actual = object::File::parse(actual.as_slice()).unwrap();
    let a = actual.symbol_by_name("_common").unwrap();
    let b = reference.symbol_by_name("_common").unwrap();
    assert_eq!(
        (a.size(), a.address(), a.section(), a.flags()),
        (b.size(), b.address(), b.section(), b.flags())
    );
    use object::ObjectSection;
    assert_eq!(
        actual.section_by_name("__const").unwrap().data().unwrap(),
        reference
            .section_by_name("__const")
            .unwrap()
            .data()
            .unwrap()
    );
    for bad in [
        ".comm _x, -1, 0",
        ".comm _x, 4, 16",
        ".comm _x, 4, 0\n_x: ret",
        ".byte (A-B)>>2",
        ".byte (L1-L0)>>128",
    ] {
        assert!(
            keel_asm::assemble(&format!(".text\nL0: nop\nL1: ret\n{bad}\n")).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn apple_vector_suffixes_and_alignment_match_clang() {
    common::compare_clang_with_features(
        ".text\n.p2align 4, 0\nadd.2d v0, v1, v2\nushll.4s v0, v1, #0\nushll2.4s v0, v1, #1\nsdot.4s v0, v1, v2\naddv.4s s0, v1\nld1r.2d { v0 }, [x0]\nld1.b {v0}[3], [x0]\nmov.d v0[1], v1[0]\nmov.s v1[0], w0\nmov s0, v1[3]\n.p2align 4,,4\nret\n",
        keel_asm::TargetFeatures::apple_m5(),
        &["-mcpu=apple-m5"],
    );
    for bad in [
        "add.2d x0, x1, x2",
        "mov.s v0.d[0], w0",
        "ushll.8b v0, v1, #0",
        ".p2align 4, 256",
        ".p2align 4,,0",
        "sdot.2d v0, v1, v2",
    ] {
        assert!(
            keel_asm::assemble_with_features(bad, keel_asm::TargetFeatures::apple_m5()).is_err(),
            "{bad}"
        );
    }
}
