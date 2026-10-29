//! Accepted hand-written input must mean what Clang's integrated assembler
//! makes of it: same bytes, section attributes and symbol bindings.
use crate::common::Temp;
use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection};
use std::process::Command;

const X86: &str = "x86_64-unknown-linux-gnu";

/// Clang's object for `source`, or its diagnostics when it rejects it.
fn clang(target: &str, source: &str) -> Result<Vec<u8>, String> {
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    let result = Command::new("clang")
        .args(["-target", target, "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("clang is required for assembler differential tests");
    if result.status.success() {
        Ok(std::fs::read(output).unwrap())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}

fn section_data(bytes: &[u8], name: &str) -> Vec<u8> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(name)
        .map_or_else(Vec::new, |s| s.data().unwrap().to_vec())
}

/// Accept/reject parity, and identical bytes for accepted sources.
fn compare_x86(source: &str, sections: &[&str]) {
    let ours = assemble_object(source);
    match (ours, clang(X86, source)) {
        (Ok(ours), Ok(reference)) => {
            for name in sections {
                assert_eq!(
                    section_data(&ours, name),
                    section_data(&reference, name),
                    "{name} differs from clang for:\n{source}"
                );
            }
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => panic!("clang rejects but keel accepts:\n{source}\n{e}"),
        (Err(e), Ok(_)) => panic!("keel rejects but clang accepts:\n{source}\n{e}"),
    }
}

/// `case` with its own `.text` and `.data` sections, ending in AT&T syntax.
fn isolate(n: usize, case: &str) -> String {
    let mut out = format!(".section .text.case{n},\"ax\",@progbits\n");
    for line in case.lines() {
        match line.trim() {
            ".data" => out += &format!(".section .data.case{n},\"aw\",@progbits\n"),
            ".text" => out += &format!(".section .text.case{n},\"ax\",@progbits\n"),
            _ => {
                out += line;
                out.push('\n');
            }
        }
    }
    out + ".att_syntax prefix\n"
}

/// `compare_x86` for many cases with two Clang runs. Clang reports every
/// rejected line, so one run over all cases gives each verdict, and one run
/// over the accepted cases gives their bytes. Symbols must be unique.
fn compare_x86_each(cases: &[String], sections: &[&str]) {
    let temp = Temp::new();
    let input = temp.0.join("cases.s");
    let mut source = String::new();
    let mut first_lines = Vec::new();
    for (n, case) in cases.iter().enumerate() {
        first_lines.push(source.lines().count() + 1);
        source += &isolate(n, case);
    }
    std::fs::write(&input, &source).unwrap();
    let result = Command::new("clang")
        .args(["-target", X86, "-c"])
        .arg(&input)
        .arg("-o")
        .arg(temp.0.join("cases.o"))
        .output()
        .expect("clang is required for assembler differential tests");
    let stderr = String::from_utf8_lossy(&result.stderr);
    let prefix = format!("{}:", input.display());
    let mut rejected = vec![false; cases.len()];
    for line in stderr.lines().filter(|l| l.contains("error:")) {
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let number: usize = rest.split(':').next().unwrap().parse().unwrap();
        rejected[first_lines.partition_point(|&first| first <= number) - 1] = true;
    }
    let mut accepted = Vec::new();
    for (n, case) in cases.iter().enumerate() {
        match (assemble_object(case), rejected[n]) {
            (Ok(ours), false) => accepted.push((n, case, ours)),
            (Err(_), true) => {}
            (Ok(_), true) => panic!("clang rejects but keel accepts:\n{case}\n{stderr}"),
            (Err(e), false) => panic!("keel rejects but clang accepts:\n{case}\n{e}"),
        }
    }
    let source = accepted
        .iter()
        .map(|(n, case, _)| isolate(*n, case))
        .collect::<String>();
    let reference = clang(X86, &source).unwrap();
    for (n, case, ours) in accepted {
        for name in sections {
            assert_eq!(
                section_data(&ours, name),
                section_data(&reference, &format!("{name}.case{n}")),
                "{name} differs from clang for:\n{case}"
            );
        }
    }
}

#[test]
fn x86_data_literal_ranges_match_clang() {
    let mut cases = Vec::new();
    for (directive, bits) in [(".byte", 8), (".short", 16), (".long", 32), (".quad", 64)] {
        let edges: [i128; 6] = [
            -(1 << (bits - 1)),
            -(1 << (bits - 1)) - 1,
            -1,
            (1 << bits) - 1,
            1 << bits,
            1 << (bits - 1),
        ];
        for value in edges {
            // Clang rejects literals above u64::MAX outright, and wraps
            // values below -2^63 (see the test below).
            if value > i128::from(u64::MAX) || value < i128::from(i64::MIN) {
                continue;
            }
            cases.push(format!(".data\n{directive} {value}"));
        }
    }
    for source in [
        ".data\n.byte 300",
        ".data\n.byte 0x100",
        ".data\n.byte 255+1",
        ".data\n.byte 1, 256",
        ".data\n.byte 0x8000000000000000-0x7fffffffffffff01",
        ".data\n.word 65535",
        ".data\n.int -1",
        ".data\n.quad 0xffffffffffffffff",
        ".data\n.quad 18446744073709551616",
        ".data\n.quad 0x10000000000000000",
        ".data\n.quad 0x10000000000000000-1",
        // A resolved absolute symbol shares the same field range check.
        ".set big, 300\n.data\n.byte big",
        ".set ok, 255\n.data\n.byte ok",
    ] {
        cases.push(source.to_string());
    }
    compare_x86_each(&cases, &[".data"]);
}

/// LLVM evaluates in wrapping 64-bit arithmetic before its range check, so it
/// accepts these as a different number than written. Keel rejects them instead.
#[test]
fn x86_data_literals_that_llvm_wraps_are_rejected() {
    let sources = [
        ".data\n.byte 0xffffffffffffffff",
        ".data\n.byte -0xffffffffffffffff",
        ".data\n.quad -0x8000000000000001",
        ".data\n.quad 0x7fffffffffffffff+0x8000000000000001",
    ];
    let all = sources
        .iter()
        .enumerate()
        .map(|(n, source)| isolate(n, source))
        .collect::<String>();
    if let Err(e) = clang(X86, &all) {
        panic!("clang now rejects:\n{all}\n{e}");
    }
    for source in sources {
        let error = assemble_object(source).unwrap_err().to_string();
        assert!(
            error.contains("out of range literal") || error.contains("expression exceeds 64 bits"),
            "{source}: {error}"
        );
    }
}

#[test]
fn x86_leading_zero_literals_are_octal_like_clang() {
    compare_x86(
        r#".intel_syntax noprefix
.text
mov eax, 010
mov eax, [rax + 010]
add eax, 0x10
mov eax, 0
mov eax, 00
mov rax, -017
.byte 010, 0, 07
.long 010-1
.zero 010
.uleb128 010
.sleb128 -010
.set x, 010
.long x
.data
.byte 1
.p2align 03
.byte 2
.balign 010
.quad 0777777777777777777777, 01777777777777777777777, -010
"#,
        &[".text", ".data"],
    );
    compare_x86_each(
        &[
            ".data\n.byte 08",
            ".data\n.byte 09",
            ".data\n.quad 02000000000000000000000",
            ".intel_syntax noprefix\nmov eax, 08",
            ".data\n.byte 0x+5",
        ]
        .map(str::to_string),
        &[".data", ".text"],
    );
}
