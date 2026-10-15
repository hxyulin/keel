use keel_asm::{TargetFeatures, assemble_object, assemble_program_object, encode_instruction};
use keel_asm_ir::{FieldId::*, Instruction, Opcode, Program};

#[test]
fn typed_program_mixes_symbols_relocations_data_and_unwind_with_source() {
    let mut program = Program::new();
    program.text(".text\n.globl _answer\n");
    program.label("_answer");
    program.text(".cfi_startproc\n");
    program.instruction(Instruction::new(
        Opcode::MOVZ_32_movewide,
        [(Rd, 0), (imm16, 42), (hw, 0)],
    ));
    program.branch(false, "Lreturn");
    program.text("nop\n");
    program.label("Lreturn");
    program.instruction(Instruction::new(Opcode::RET_64R_branch_reg, [(Rn, 30)]));
    program.text(".cfi_endproc\n.data\n.quad _answer\n.subsections_via_symbols\n");
    let reference = ".text\n.globl _answer\n_answer:\n.cfi_startproc\nmovz w0,#42\nb Lreturn\nnop\nLreturn:\nret\n.cfi_endproc\n.data\n.quad _answer\n.subsections_via_symbols\n";
    assert_eq!(
        assemble_program_object(&program, TargetFeatures::default()).unwrap(),
        assemble_object(reference).unwrap()
    );
}

#[test]
fn direct_fields_still_validate_and_share_cpu_gates() {
    for instruction in [
        Instruction::new(Opcode::MOVZ_64_movewide, [(Rd, 32), (imm16, 0), (hw, 0)]),
        Instruction::new(Opcode::MOVZ_32_movewide, [(Rd, 0), (imm16, 0), (hw, 2)]),
        Instruction::new(Opcode::MOVZ_64_movewide, [(Rd, 0), (Rd, 0), (hw, 0)]),
    ] {
        assert!(encode_instruction(&instruction, TargetFeatures::default()).is_err());
    }
    let instruction = Instruction::new(Opcode::AESE_B_cryptoaes, [(Rd, 0), (Rn, 1)]);
    assert!(
        encode_instruction(&instruction, TargetFeatures::default())
            .unwrap_err()
            .contains("FEAT_AES")
    );
    assert!(encode_instruction(&instruction, TargetFeatures::apple_m5()).is_ok());
    for source in [".data\n", ".byte 0\n"] {
        let mut program = Program::new();
        program.text(source);
        program.instruction(Instruction::new(Opcode::NOP_HI_hints, []));
        assert!(
            assemble_program_object(&program, TargetFeatures::default())
                .unwrap_err()
                .message
                .contains("aligned")
        );
    }
}

#[test]
fn inline_macro_fallback_preserves_direct_words() {
    let mut program = Program::new();
    program.instruction(Instruction::new(Opcode::NOP_HI_hints, []));
    program.text(".macro extra\nret\n.endm\nextra\n");
    assert_eq!(
        assemble_program_object(&program, TargetFeatures::default()).unwrap(),
        assemble_object("nop\nret\n").unwrap()
    );
}

#[test]
fn oversized_functions_keep_existing_far_branch_relaxation() {
    let body = "nop\\n".repeat(265_000);
    let source = format!(
        "func @big(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 0\n v2 = icmp.eq v0, v1\n brif v2, block2, block1\nblock1:\n asm \"{body}\", \"\"()\n jump block2\nblock2:\n return v0\n}}\n"
    );
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = keel_codegen::compile(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    let program =
        keel_codegen::compile_program(&module, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    assert_eq!(
        program.stats().instructions,
        0,
        "oversized function must use existing relaxation"
    );
    assert_eq!(
        assemble_program_object(&program, TargetFeatures::default()).unwrap(),
        assemble_object(&text).unwrap()
    );
}

#[test]
fn macro_fallback_does_not_turn_invalid_structured_symbols_into_source() {
    for item in [
        keel_asm_ir::Item::Label("bad:\nnop\nLend".into()),
        keel_asm_ir::Item::Branch {
            link: false,
            target: "Lend\nnop\nLend:".into(),
        },
    ] {
        let mut program = Program::new();
        program.text(".rept 1\nnop\n.endr\n");
        program.items.push(item);
        assert!(
            assemble_program_object(&program, TargetFeatures::default())
                .unwrap_err()
                .message
                .contains("invalid symbol")
        );
    }
}
