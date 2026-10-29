use crate::common;

use crate::common::Temp;
use keel_asm::{assemble, assemble_with_metrics};
use std::process::Command;

fn compare_clang(source: &str) {
    common::compare_clang_with_features(
        source,
        keel_asm::TargetFeatures::default(),
        &["-x", "assembler"],
    );
}

#[test]
fn apple_comments_and_line_endings_match_clang() {
    // On Darwin A64, semicolon starts a comment, rather than a statement.
    compare_clang("nop; ret\rret // ignored /*\r\nnop ; not_an_instruction\n");
    assert_eq!(assemble("nop; ret").unwrap().text.len(), 4);
    compare_clang(concat!(
        "/* header\nwith ; // and \"quotes\" */\n",
        "Lentry: MOV/* separator */ X0, /* operand */ X1\n",
        "add x0, /* across\r\nphysical\rlines */ x0, #1\r",
        "/* one *//* two */b Lend /* three */\n",
        "nop\nLend: ret /* trailing comment */",
    ));
}

#[test]
fn comment_markers_inside_strings_and_line_comments_are_literal() {
    compare_clang(concat!(
        ".ascii \"/* ; // */\"\n.ascii \"escaped \\\" /* */\"\n",
        ".ascii \"λ中\"\n",
        "// /* unterminated, ignored\n",
        "; /* ignored too\n",
    ));
    let text = assemble(".ascii \"/* ; // */\"").unwrap().text;
    assert_eq!(text, b"/* ; // */");
}

#[test]
fn binary_octal_and_hex_literals_match_clang() {
    compare_clang(concat!(
        "mov x0, #0b101010\nmov w1, #0B11110000\n",
        "mov x2, #010\nmov x3, #-010\nmov x4, #0Xff\n",
        "movz x5, #0177777, LSL #16\n",
        "add x6, x5, #0b11\nsub x6, x6, #010\n",
        "orr x7, x7, #0B11111111\n",
        "ldr x8, [sp, #010]\n",
        ".byte 0b101, 010, 0Xff, 00\n",
        ".short -010, 0b100000000\n",
        ".long 010+0b10\n.quad 01777777777777777777777\n",
    ));
}

#[test]
fn register_31_is_zero_and_case_insensitive_shifts_match_clang() {
    compare_clang(concat!(
        "MOV X0, X31\nMOV W31, W0\n",
        "ADD X31, X0, X1, LSL #2\n",
        "SUB W0, W1, W31, ASR #3\n",
        "ORR X0, X1, X31, ROR #7\n",
        "MOVZ X0, #1, LSL #16\nADD X0, X1, #1, LSL #12\n",
        "LDR X0, [SP, X31]\nLDR X0, [SP, W31, UXTW #3]\n",
        "STR X31, [SP]\nRET X31\n",
    ));
}

#[test]
fn new_syntax_preserves_physical_lines_and_instruction_metrics() {
    let source = "/* header\r\ncontinued */\rMOV X0, /* operand\ncontinued */ X31\r\nret\n";
    let (_, locations) = assemble_with_metrics(source).unwrap();
    assert_eq!(
        locations
            .iter()
            .map(|l| (l.line, l.offset))
            .collect::<Vec<_>>(),
        [(3, 0), (5, 4)]
    );
    for (source, expected) in [
        ("/* a\nb */ invalid", 2),
        ("nop\r\n/* a\rb */\rinvalid", 4),
        ("mov x0, /* a\nb */ sp\ninvalid", 3),
        ("nop\n/* a\nb */ /* unterminated", 3),
        ("nop\n/* unterminated", 2),
        ("/* a\nb */\n.unsupported", 3),
    ] {
        let error = assemble(source).unwrap_err();
        assert_eq!(error.line, expected, "{source:?}: {error}");
    }
    assert!(
        assemble("nop\n/* unterminated")
            .unwrap_err()
            .message
            .contains("unterminated block comment")
    );
}

#[test]
fn invalid_radices_and_zero_register_roles_are_rejected_by_both() {
    let temp = Temp::new();
    let input = temp.0.join("invalid.s");
    let clang = |source: &str| {
        std::fs::write(&input, source).unwrap();
        Command::new("clang")
            .args(["-target", "arm64-apple-macos11", "-x", "assembler", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(temp.0.join("invalid.o"))
            .output()
            .unwrap()
    };
    let lines = [
        "mov x0, #08",
        "mov x0, #0b102",
        "mov x0, #0b",
        "mov x0, #0b+1",
        "mov x0, #0X+1",
        "mov x0, #0o10",
        ".byte 09",
        ".long 0B2",
        "movz x0, #08",
        "add x0, x1, #09",
        "ldr x0, [x31]",
        "add x0, x31, #1",
        "mov sp, x31",
        "mov x31, sp",
        "add x31, x0, #1",
        "mov x0, x/* comment */1",
        ".unknown /* comment */ 1",
    ];
    // Clang only rejects `mov x0, x32` at fixup time, after its parse errors,
    // and an unterminated comment swallows the lines after it.
    let alone = ["mov x0, x32", "nop /* unterminated"];
    for source in lines.iter().chain(&alone) {
        let error = assemble(source).expect_err(source);
        assert_eq!(error.line, 1, "{source}: {error}");
    }
    // Clang keeps going after a parse error, so one run checks every line.
    let output = clang(&format!("{}\n", lines.join("\n")));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    for (n, source) in lines.iter().enumerate() {
        let location = format!("{}:{}:", input.display(), n + 1);
        assert!(
            stderr
                .lines()
                .any(|l| l.starts_with(&location) && l.contains("error:")),
            "clang accepted {source}\n{stderr}"
        );
    }
    for source in alone {
        let output = clang(&format!("{source}\n"));
        assert!(!output.status.success(), "clang accepted {source}");
    }
}
