//! Symbol-free instructions are encoded once into merged byte runs, while
//! symbolic ones are relaid on every relaxation pass. These cases pin the
//! observable behavior that storage must not change: bytes across text and
//! typed programs, widening cascades, and diagnostic precedence and lines.
use keel_asm::x86::{
    Features, assemble_object, assemble_object_with_features, assemble_program_object,
    assemble_program_object_with_features, profile_object,
};
use keel_asm_ir::{
    Item, Program,
    x86::{self, Instruction},
};
use object::{Object, ObjectSection, ObjectSymbol};

fn section(bytes: &[u8], name: &str) -> Vec<u8> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(name).unwrap().data().unwrap().to_vec()
}

#[test]
fn literal_data_preserves_signed_unsigned_radix_and_alias_bytes() {
    let source = ".data\n.byte -128\n.byte +255\n.short -32768\n.word 0Xffff\n\
                  .long -2147483648\n.int 0xffffffff\n.quad -9223372036854775808\n\
                  .quad 18446744073709551615\n.byte 010\n.byte 0b101\nret";
    let object = assert_paths_agree(source);
    let mut expected = vec![0x80, 0xff, 0, 0x80, 0xff, 0xff];
    expected.extend_from_slice(&i32::MIN.to_le_bytes());
    expected.extend_from_slice(&u32::MAX.to_le_bytes());
    expected.extend_from_slice(&i64::MIN.to_le_bytes());
    expected.extend_from_slice(&u64::MAX.to_le_bytes());
    expected.extend_from_slice(&[8, 5, 0xc3]);
    assert_eq!(section(&object, ".data"), expected);
}

#[test]
fn literal_data_keeps_locations_sections_and_relaxation_order() {
    let source = format!(
        ".file 1 \"literal.c\"\n.text\n.loc 1 3 0\n.byte 0x90\n\
         start: jmp far\n{}\n.loc 1 4 0\n.data\n.byte 7\n.text\n\
         .byte 0x90\nfar: ret\n.long far-start\n.long .-start\n\
         .uleb128 far-start\n.bss\n.quad 0\n.text\n.byte 0\n",
        ".byte 0\n".repeat(130)
    );
    let object = assert_paths_agree(&source);
    // Adding an expression term forces the original per-pass text path.
    // Whole-object equality covers pending .loc, branch widening, dot capture,
    // symbols, LEB stabilization, NOBITS size, and section selection.
    let fallback = source
        .replace(".byte 0x90", ".byte 0x90+0")
        .replace(".byte 0\n", ".byte 0+0\n")
        .replace(".byte 7", ".byte 7+0")
        .replace(".quad 0", ".quad 0+0");
    assert_eq!(object, assemble_object(&fallback).unwrap());
    assert_eq!(section(&object, ".text")[1], 0xe9);
}

#[test]
fn literal_data_fallbacks_keep_complete_diagnostics_and_precedence() {
    for (source, line, message) in [
        (".bss\n.byte 1", 2, "nonzero initializer in NOBITS section"),
        (".byte 256", 1, "out of range literal value for `.byte`"),
        (
            ".short -32769",
            1,
            "out of range literal value for `.short`",
        ),
        (".byte 08", 1, "invalid symbol `08`"),
        (
            ".quad 18446744073709551616",
            1,
            "invalid symbol `18446744073709551616`",
        ),
        (".byte 1,256", 1, "out of range literal value for `.byte`"),
        (
            ".bss\n.byte 0\n.short 0\n.word 0\n.long 0\n.int 0\n.quad 0\n.byte 1,256",
            8,
            "nonzero initializer in NOBITS section",
        ),
        (
            ".bss\n.byte 256\n.byte 1",
            2,
            "out of range literal value for `.byte`",
        ),
    ] {
        assert_eq!(
            error(assemble_object(source)),
            (line, message.into()),
            "{source}"
        );
    }
    let (line, message) = error(assemble_object(".byte 256\nunknown_instruction"));
    assert_eq!(line, 2);
    assert!(message.contains("unknown_instruction"), "{message}");
    let (line, message) = error(assemble_object(".byte 256\nandn eax, ebx, ecx"));
    assert_eq!(line, 2);
    assert!(
        message.contains("requires x86 target features"),
        "{message}"
    );
}
fn error(result: Result<Vec<u8>, keel_asm::Error>) -> (usize, String) {
    let e = result.unwrap_err();
    (e.line, e.message)
}
/// The same source as explicit text, a typed program (borrowed instructions),
/// and one opaque text item must produce identical objects.
fn assert_paths_agree(source: &str) -> Vec<u8> {
    let object = assemble_object(source).unwrap();
    let typed = x86::parse_program(source).unwrap();
    assert!(
        typed
            .items
            .iter()
            .any(|item| matches!(item, Item::X86Instruction(_)))
    );
    assert_eq!(assemble_program_object(&typed).unwrap(), object, "typed");
    let mut opaque = Program::new();
    opaque.text(source);
    assert_eq!(assemble_program_object(&opaque).unwrap(), object, "text");
    object
}

#[test]
fn text_and_typed_programs_agree_across_runs_branches_and_metadata() {
    let mut source = String::from(
        r#".intel_syntax noprefix
.file 1 "t.c"
.text
.globl f
.p2align 4
.type f,@function
f:
.cfi_startproc
.loc 1 1 0
push rbp
.cfi_def_cfa_offset 16
.cfi_offset rbp, -16
mov rbp, rsp
.cfi_def_cfa_register rbp
start: jmp far
"#,
    );
    // 26 * 5 bytes forces the forward branch to widen across one merged run.
    source.push_str(&"mov eax, 1\n".repeat(26));
    source.push_str(
        r#"je start
lea rax, [rip + data]
call external@PLT
.p2align 5,0x90,3
near: nop; nop
jne near
mov ecx, [rax + 8*rbx - 16]
far: pop rbp
.cfi_def_cfa rsp, 8
ret
.cfi_endproc
.size f, .-f
.data
data: .quad f + 4
.long far - start
.set alias, near
.section .bss
.zero 16
"#,
    );
    let object = assert_paths_agree(&source);
    let code = section(&object, ".text");
    let file = object::File::parse(&*object).unwrap();
    let start = file
        .symbols()
        .find(|s| s.name() == Ok("start"))
        .unwrap()
        .address() as usize;
    assert_eq!(code[start], 0xe9, "forward branch widened");
    assert!(file.section_by_name(".eh_frame").is_some());
    assert!(file.section_by_name(".debug_line").is_some());
}

#[test]
fn typed_runs_join_parsed_text_and_borrowed_instructions() {
    let mut program = Program::new();
    program.text("nop\nmov eax, 1\n");
    program.x86_instruction(Instruction::parse_intel("nop").unwrap());
    program.label("top");
    program.x86_instruction(Instruction::parse_intel("add rax, 1").unwrap());
    program.x86_instruction(Instruction::parse_intel("jmp top").unwrap());
    let typed = assemble_program_object(&program).unwrap();
    let text = assemble_object("nop\nmov eax, 1\nnop\ntop: add rax, 1\njmp top").unwrap();
    assert_eq!(typed, text);
    assert_eq!(
        section(&text, ".text"),
        [
            0x90, 0xb8, 1, 0, 0, 0, 0x90, 0x48, 0x83, 0xc0, 1, 0xeb, 0xfa
        ]
    );
}

#[test]
fn widening_cascades_through_merged_runs_and_max_skip_alignment() {
    // Widening the first branch disables a max-skip alignment, moving the
    // initially short second branch out of range on the next pass.
    let mut source = String::from("f: jmp target\njmp far\n");
    source.push_str(&"nop\n".repeat(123));
    source.push_str(".p2align 7,0xcc,4\ntarget: .long target-f\n");
    source.push_str(&"mov eax, 1\n".repeat(26));
    source.push_str("far: ret\n");
    let object = assert_paths_agree(&source);
    let code = section(&object, ".text");
    assert_eq!(code[0], 0xe9);
    assert_eq!(code[5], 0xe9);
    // Backward conditional pushed beyond -128 by a widened forward branch.
    let mut source = String::from("start: jmp far\n");
    source.push_str(&"nop\n".repeat(124));
    source.push_str("je start\n");
    source.push_str(&"nop\n".repeat(128));
    source.push_str("far: ret\n");
    let code = section(&assert_paths_agree(&source), ".text");
    assert_eq!(code[0], 0xe9);
    assert_eq!(&code[129..131], &[0x0f, 0x84]);
}

#[test]
fn layout_dependent_operands_stay_symbolic() {
    // `distance` changes from 127 to 130 once the branch widens; the push must
    // be reencoded, even though the surrounding runs are encoded once.
    let mut source = String::from("start: jmp far\n");
    source.push_str(&"mov eax, 1\n".repeat(25));
    source.push_str("end: .set distance,end-start\npush distance\nnop\n");
    source.push_str(&"nop\n".repeat(127));
    source.push_str("far: ret\n");
    let code = section(&assert_paths_agree(&source), ".text");
    assert_eq!(code[0], 0xe9);
    assert_eq!(&code[130..135], &[0x68, 130, 0, 0, 0]);
}

#[test]
fn run_diagnostics_use_the_first_failing_instruction_line() {
    assert_eq!(
        error(assemble_object(".bss\nnop\nnop")),
        (2, "instruction in NOBITS section".into())
    );
    assert_eq!(
        error(assemble_object(".text\nnop\nnop\n.bss\nmov eax, 1\nnop")),
        (5, "instruction in NOBITS section".into())
    );
    // A symbol-free encoding failure is reported in layout order.
    let (line, message) = error(assemble_object("nop\nnop\nmov al, 300\n.bss\nnop"));
    assert_eq!(line, 3);
    assert!(message.contains("300"), "{message}");
    let (line, _) = error(assemble_object(".bss\nnop\n.text\nmov al, 300"));
    assert_eq!(line, 2);
}

#[test]
fn parse_errors_precede_feature_errors_which_precede_layout_errors() {
    let bmi = "andn eax, ebx, ecx";
    // Layout error first in source order, feature error later: features win.
    let (line, message) = error(assemble_object(&format!("mov al, 300\n{bmi}")));
    assert_eq!(line, 2);
    assert!(
        message.contains("requires x86 target features"),
        "{message}"
    );
    // A later parse error wins over an earlier feature error.
    let (line, message) = error(assemble_object(&format!("{bmi}\nnop\nmov eax, [")));
    assert_eq!(line, 3);
    assert!(
        !message.contains("requires x86 target features"),
        "{message}"
    );
    // The first feature error is reported.
    let (line, _) = error(assemble_object(&format!("nop\n{bmi}\n{bmi}")));
    assert_eq!(line, 2);
    // With the feature enabled the same instruction joins an encoded run.
    let features = Features::parse("bmi1").unwrap();
    let source = format!("nop\n{bmi}\nnop");
    let object = assemble_object_with_features(&source, features).unwrap();
    assert_eq!(
        section(&object, ".text"),
        [0x90, 0xc4, 0xe2, 0x60, 0xf2, 0xc1, 0x90]
    );
    let typed = x86::parse_program(&source).unwrap();
    assert_eq!(
        assemble_program_object_with_features(&typed, features).unwrap(),
        object
    );
}

#[test]
fn typed_program_diagnostics_use_item_and_text_lines() {
    let nop = || Instruction::parse_intel("nop").unwrap();
    let mut program = Program::new();
    program.text(".bss\n");
    program.x86_instruction(nop());
    program.x86_instruction(nop());
    assert_eq!(
        error(assemble_program_object(&program)),
        (2, "instruction in NOBITS section".into())
    );
    // Lines inside explicit text count from that item's position.
    let mut program = Program::new();
    program.x86_instruction(nop());
    program.text("nop\n.bss\nnop\n");
    assert_eq!(
        error(assemble_program_object(&program)),
        (4, "instruction in NOBITS section".into())
    );
    let mut program = Program::new();
    program.x86_instruction(nop());
    program.x86_instruction(Instruction::parse_intel("andn eax, ebx, ecx").unwrap());
    let (line, message) = error(assemble_program_object(&program));
    assert_eq!(line, 2);
    assert!(
        message.contains("requires x86 target features"),
        "{message}"
    );
}

#[test]
fn profile_counts_parsed_items_not_runs() {
    let (object, profile) = profile_object("nop\nnop\nfoo: nop\n.byte 1\njmp foo").unwrap();
    assert_eq!(
        object,
        assemble_object("nop\nnop\nfoo: nop\n.byte 1\njmp foo").unwrap()
    );
    // nop, nop, foo, nop, ".byte 1\n", jmp.
    assert_eq!(profile.items, 6);
    assert_eq!(profile.widened, [0]);

    for source in [".byte 1; .byte 2; ret", ".byte 1\n.byte 2\nret"] {
        let (object, profile) = profile_object(source).unwrap();
        assert_eq!(object, assemble_object(source).unwrap());
        assert_eq!(profile.items, 3);
    }
}

#[test]
fn streamed_statements_keep_inline_syntax_quotes_and_directive_order() {
    let source = r#".data; .byte 1; .byte -1; .ascii "a;#//b"
"label:;#": .quad 3; .text; .att_syntax prefix; movl $7, %eax
.intel_syntax noprefix; add eax, 2; .byte 0x90; ret
"#;
    let bytes = assert_paths_agree(source);
    assert_eq!(
        section(&bytes, ".text"),
        [0xb8, 7, 0, 0, 0, 0x83, 0xc0, 2, 0x90, 0xc3]
    );
    assert_eq!(&section(&bytes, ".data")[..8], b"\x01\xffa;#//b");
    let (line, message) = error(assemble_object(".byte 256; .att_syntax prefix\nunknown"));
    assert_eq!(line, 2);
    assert!(message.contains("unknown"));
    let (line, message) = error(assemble_object("andn eax, ebx, ecx; .byte 256; unknown"));
    assert_eq!(line, 1);
    assert!(message.contains("unknown"));
}
