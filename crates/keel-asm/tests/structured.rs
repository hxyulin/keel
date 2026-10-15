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
    assert!(
        program.stats().instructions > 0,
        "oversized function must retain native instructions"
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

#[test]
fn fully_structured_metadata_and_symbolic_instructions_match_source() {
    use keel_asm_ir::{
        Cfi, Directive as D, Expression as E, PageKind, SectionKind, SymbolicInstruction as S,
    };
    let mut p = Program::new();
    p.directive(D::Section {
        segment: "__TEXT".into(),
        name: "__text".into(),
        kind: SectionKind::Data,
    });
    p.directive(D::File {
        index: 1,
        path: b"Lkeel_metadata_a \"quoted\" file.c".to_vec(),
    });
    p.directive(D::Global("_entry".into()));
    p.directive(D::Align(2));
    p.label("_entry");
    p.directive(D::Cfi(Cfi::Start));
    p.directive(D::Cfi(Cfi::DefCfa {
        register: 31,
        offset: 16,
    }));
    p.directive(D::Cfi(Cfi::Offset {
        register: 72,
        offset: -8,
    }));
    p.directive(D::Loc {
        file: 1,
        line: 7,
        column: 2,
        prologue_end: true,
    });
    p.symbolic(S::Conditional {
        condition: 0,
        target: "Lend".into(),
    });
    p.symbolic(S::Compare {
        register: 1,
        is64: false,
        nonzero: true,
        target: "Lend".into(),
    });
    p.symbolic(S::Address {
        register: 2,
        target: "Lend".into(),
    });
    for kind in [PageKind::Direct, PageKind::Got, PageKind::ThreadLocal] {
        p.symbolic(S::PageAddress {
            register: 3,
            kind,
            target: "_external".into(),
        });
        p.symbolic(S::PageOffset {
            register: 3,
            base: 3,
            load: kind != PageKind::Direct,
            kind,
            target: "_external".into(),
        });
    }
    p.label("Lend");
    p.instruction(Instruction::new(Opcode::RET_64R_branch_reg, [(Rn, 30)]));
    p.directive(D::Cfi(Cfi::End));
    p.directive(D::Section {
        segment: "__DATA".into(),
        name: "__data".into(),
        kind: SectionKind::Data,
    });
    p.label("Ldata");
    p.directive(D::Integer {
        width: 8,
        expression: E::symbol("_entry"),
        got: false,
    });
    p.directive(D::Leb {
        signed: false,
        expression: E::difference("Ldata_end", "Ldata"),
    });
    p.directive(D::Leb {
        signed: true,
        expression: E::constant(-129),
    });
    p.directive(D::CString(b"hi\n".to_vec()));
    p.directive(D::Bytes(vec![0, 255]));
    p.directive(D::Space(3));
    p.label("Ldata_end");
    p.directive(D::Set {
        name: "Lsize".into(),
        expression: E::difference("Ldata_end", "Ldata"),
    });
    p.directive(D::Integer {
        width: 4,
        expression: E::symbol("Lsize"),
        got: false,
    });
    p.directive(D::ZeroFill {
        symbol: "_zero".into(),
        size: 32,
        alignment: 4,
        thread_local: false,
    });
    p.directive(D::ZeroFill {
        symbol: "_tls".into(),
        size: 8,
        alignment: 3,
        thread_local: true,
    });
    p.directive(D::SubsectionsViaSymbols);
    let source = r#".text
.file 1 "Lkeel_metadata_a \"quoted\" file.c"
.globl _entry
.p2align 2
_entry:
.cfi_startproc
.cfi_def_cfa sp, 16
.cfi_offset d8, -8
.loc 1 7 2 prologue_end
b.eq Lend
cbnz w1, Lend
adr x2, Lend
adrp x3, _external@PAGE
add x3, x3, _external@PAGEOFF
adrp x3, _external@GOTPAGE
ldr x3, [x3, _external@GOTPAGEOFF]
adrp x3, _external@TLVPPAGE
ldr x3, [x3, _external@TLVPPAGEOFF]
Lend:
ret
.cfi_endproc
.data
Ldata:
.quad _entry
.uleb128 Ldata_end-Ldata
.sleb128 -129
.asciz "hi\n"
.byte 0,255
.space 3
Ldata_end:
Lsize = Ldata_end-Ldata
.long Lsize
.zerofill __DATA,__bss,_zero,32,4
.tbss _tls,8,3
.subsections_via_symbols
"#;
    assert_eq!(p.stats().text_bytes, 0);
    let expected = assemble_object(source).unwrap();
    assert_eq!(
        assemble_program_object(&p, TargetFeatures::default()).unwrap(),
        expected
    );
    // Force preprocessing to exercise validated serialization of every new kind.
    p.text(".rept 0\nnop\n.endr\n");
    assert_eq!(
        assemble_program_object(&p, TargetFeatures::default()).unwrap(),
        expected
    );
}

#[test]
fn invalid_typed_metadata_and_symbols_are_rejected_with_and_without_macro_fallback() {
    use keel_asm_ir::{
        Cfi, Directive as D, Expression as E, Item, PageKind, SymbolicInstruction as S,
    };
    let invalid = [
        Item::Directive(D::Global("bad\nnop".into())),
        Item::Directive(D::Integer {
            width: 3,
            expression: E::constant(0),
            got: false,
        }),
        Item::Directive(D::Integer {
            width: 8,
            expression: E::symbol("bad\nnop"),
            got: false,
        }),
        Item::Directive(D::Cfi(Cfi::DefCfa {
            register: 96,
            offset: 0,
        })),
        Item::Directive(D::Align(17)),
        Item::Directive(D::Loc {
            file: 1,
            line: 1,
            column: 0,
            prologue_end: false,
        }),
        Item::Symbolic(S::Conditional {
            condition: 16,
            target: "Lend".into(),
        }),
        Item::Symbolic(S::Compare {
            register: 32,
            is64: true,
            nonzero: false,
            target: "Lend".into(),
        }),
        Item::Symbolic(S::Address {
            register: 0,
            target: "bad\nnop".into(),
        }),
        Item::Symbolic(S::PageOffset {
            register: 0,
            base: 0,
            load: false,
            kind: PageKind::Got,
            target: "_x".into(),
        }),
    ];
    for fallback in [false, true] {
        for item in &invalid {
            let mut p = Program::new();
            if fallback {
                p.text(".rept 0\nnop\n.endr\n");
            }
            p.items.push(item.clone());
            p.label("Lend");
            assert!(
                assemble_program_object(&p, TargetFeatures::default()).is_err(),
                "{item:?}, fallback={fallback}"
            );
        }
    }
}

#[test]
fn partial_cfa_updates_are_typed_for_both_targets() {
    use keel_asm_ir::{Cfi, Directive};
    for x86 in [false, true] {
        let mut p = Program::new();
        p.text(".text\nf:\n");
        p.directive(Directive::Cfi(Cfi::Start));
        p.directive(Directive::Cfi(Cfi::DefCfaOffset(16)));
        p.directive(Directive::Cfi(Cfi::DefCfaRegister(if x86 {
            6
        } else {
            29
        })));
        p.text("ret\n");
        p.directive(Directive::Cfi(Cfi::End));
        let source = format!(
            ".text\nf:\n.cfi_startproc\n.cfi_def_cfa_offset 16\n.cfi_def_cfa_register {}\nret\n.cfi_endproc\n",
            if x86 { "rbp" } else { "x29" }
        );
        if x86 {
            assert_eq!(
                keel_asm::x86::assemble_program_object(&p).unwrap(),
                keel_asm::x86::assemble_object(&source).unwrap()
            );
        } else {
            assert_eq!(
                assemble_program_object(&p, TargetFeatures::default()).unwrap(),
                assemble_object(&source).unwrap()
            );
        }
    }
}
