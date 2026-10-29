#[allow(dead_code)]
mod common;
#[path = "preprocessing/syntax.rs"]
mod syntax;
use keel_asm::{assemble, assemble_with_metrics};

#[test]
fn named_macros_nested_calls_and_repetition_match_clang() {
    common::compare_clang(
        r#"
.text
.macro add_to reg amount
  add \reg, \reg, #\amount
.endmacro
.macro twice reg
  .rept 2
    add_to \reg, 3
  .endr
.endm
.globl _entry
_entry:
  twice x0
  .rept 3
    nop
    .rept 2
      add_to x1, 1
    .endr
  .endr
  ret
"#,
    );
}

#[test]
fn literal_conditionals_match_clang() {
    common::compare_clang(
        r#"
.macro choose value
 .if \value
  mov x0, #1
 .elseif 1
  mov x0, #2
 .else
  bogus instruction
 .endif
.endm
choose 0
choose 1
.if 0
 .if undefined_symbol
  bogus
 .endif
 .macro ignored
  bogus
 .endm
 .rept undefined_symbol
  bogus
 .endr
.else
 nop
.endif
.ifeq 0
 nop
.endif
.ifne -1
 nop
.endif
.ifgt 1
 nop
.endif
.ifge 0
 nop
.endif
.iflt -1
 nop
.endif
.ifle 0
 nop
.endif
"#,
    );
}

#[test]
fn arguments_strings_comments_and_concatenation_match_clang() {
    common::compare_clang(
        r#"
// .macro fake
.macro load dst, address
 ldr \dst, [\address] // a comment with \unknown and .endm
.endm
.macro data suffix, value
Litem\suffix\():
 .ascii "\value"
.endm
load x0, x1
data 1, "a,b // ; quote: \" and slash: \\"
// A zero repeat must not evaluate its body.
.rept 0
 unknown_instruction
.endr
.p2align 2
ret
"#,
    );
}

#[test]
fn diagnostics_and_metrics_retain_source_lines() {
    let source = ".macro broken reg\n  add \\reg, x0, #99999\n.endm\nbroken x1\n";
    let error = assemble(source).unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("expanded at line 4"), "{error}");
    let (_, metrics) = assemble_with_metrics(".rept 2\nnop\n.endr\nret").unwrap();
    assert_eq!(
        metrics.iter().map(|m| m.line).collect::<Vec<_>>(),
        [2, 2, 4]
    );
    assert_eq!(
        metrics.iter().map(|m| m.offset).collect::<Vec<_>>(),
        [0, 4, 8]
    );
    assert_eq!(assemble(".if 1\n.endif\ninvalid").unwrap_err().line, 3);
}

#[test]
fn unsupported_constructs_and_malformed_blocks_are_errors() {
    for (source, expected) in [
        (".macro x p=1\n.endm", "plain names"),
        (".macro x p:vararg\n.endm", "plain names"),
        (".macro x p,p\n.endm", "plain names"),
        (".macro x p\n.endm\nx", "expects 1 arguments"),
        (".macro x\n.endm\nx 1", "expects 0 arguments"),
        (".macro x\n.endm\n.macro x\n.endm", "already defined"),
        (
            ".macro x\n.byte \\unknown\n.endm\nx",
            "unknown macro parameter",
        ),
        (".macro x a,b\n.endm\nx 1 2", "must be quoted"),
        (".macro x a\n.endm\nx a=1", "keyword"),
        (".macro x a\n.endm\nx \"a\" \"b\"", "mixed quoted"),
        (".macro x\n.ascii \"$0\"\n.endm\nx", "positional"),
        (".rept #1\n.endr", "without #"),
        (".rept -1\n.endr", "count must be"),
        (".rept 100001\n.endr", "count must be"),
        (".rept 1+1\n.endr", "integer literal"),
        (".if symbol\n.endif", "integer literal"),
        (".if 1\n.else\n.else\n.endif", "after .else"),
        (".if 1\n.else 1\n.endif", "unexpected .else"),
        (".if 1\n.endif 1", "unexpected .endif"),
        (".endif", "unmatched"),
        (".else", "unmatched"),
        (".elseif 1", "unmatched"),
        (".if 1", "unterminated conditional"),
        (".macro x", "unterminated"),
        (".rept 1\n.endm", "mismatched"),
        (".rept 1\n.endr 1", "unexpected"),
        (".endm", "unmatched"),
        (".endr", "unmatched"),
        (".include \"missing.s\"", "unsupported directive"),
        (".ifdef foo\n.endif", "unsupported directive"),
    ] {
        let error = assemble(source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{source:?}: {error}; expected {expected}"
        );
    }
}

#[test]
fn recursion_and_expansion_are_bounded() {
    for source in [
        ".macro recursive\nrecursive\n.endm\nrecursive",
        ".macro a\nb\n.endm\n.macro b\na\n.endm\na",
        ".rept 100000\n.rept 100000\n.endr\n.endr",
    ] {
        let error = assemble(source).unwrap_err();
        assert!(error.message.contains("exceeds"), "{error}");
    }
    let source = format!(".rept 100000\n.ascii \"{}\"\n.endr", "x".repeat(200));
    assert!(assemble(&source).unwrap_err().message.contains("8 MiB"));
}

#[test]
fn ordinary_source_and_quoted_directive_names_are_unchanged() {
    common::compare_clang(".ascii \".macro .rept .if // ;\"\n.p2align 2\nnop\n");
    assert_eq!(assemble("nop\nret").unwrap().text.len(), 8);
}

#[test]
fn expansion_combines_with_block_comments_radices_and_physical_lines() {
    let source = "/* heading\n comment */\n.macro bump reg\nadd \\reg, /* operand\n comment */ \\reg, #0b1\n.endm\n.rept 02\nbump x0\n.endr\nret\n";
    for source in [source.to_owned(), source.replace('\n', "\r")] {
        common::compare_clang(&source);
        let (_, metrics) = assemble_with_metrics(&source).unwrap();
        assert_eq!(
            metrics.iter().map(|m| m.line).collect::<Vec<_>>(),
            [4, 4, 10]
        );
        let error = assemble(&source.replace("#0b1", "#99999")).unwrap_err();
        assert_eq!(error.line, 4);
        assert!(error.message.contains("expanded at line 8"), "{error}");
    }
    assert_eq!(assemble(".rept 1\n/* unclosed").unwrap_err().line, 2);
}

#[test]
fn standalone_example_matches_clang() {
    common::compare_clang(include_str!("../examples/standalone.s"));
}

#[test]
fn required_parameters_and_unique_invocations_match_clang() {
    common::compare_clang(".macro f reg:req\nLtmp\\@: add \\reg, \\reg, #1\n.endm\nf x0\nf x1\n");
    for source in [".macro f a:req\n.endm\nf", ".macro f a:req,b\n.endm\nf ,1"] {
        assert!(
            assemble(source)
                .unwrap_err()
                .message
                .contains("required macro argument")
        );
    }
}
