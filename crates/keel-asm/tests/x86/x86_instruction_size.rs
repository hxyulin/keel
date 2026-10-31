use keel_asm::x86::{
    Features, assemble_object, assemble_object_with_features,
    assemble_program_object_with_features, instruction_size, instruction_size_with_features,
};
use keel_asm_ir::{
    Item, Program,
    x86::{Instruction, Opcode},
};
use object::{Object, ObjectSection};

fn text(bytes: &[u8]) -> Vec<u8> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn exact_sizes_include_register_immediate_and_address_encoding_choices() {
    for (source, expected) in [
        ("ret", 1),
        ("cmp al,3", 2),
        ("cmp dil,3", 4),
        ("cmp ax,3", 4),
        ("cmp eax,3", 3),
        ("cmp r8d,3", 4),
        ("cmp eax,128", 5),
        ("cmp dil,128", 4),
        ("cmp ax,-1", 4),
        ("movabs r11,0x123456789abcdef0", 10),
        ("mov rax,[rbp]", 4),
        ("mov rax,[r12+8*r13+127]", 5),
        ("mov rax,[r12+8*r13+128]", 8),
        ("mov eax,[eax+4]", 4),
        ("mov rax,[rip+8]", 7),
        ("lock add qword ptr [rax],1", 5),
        ("jmp r11", 3),
    ] {
        let instruction = Instruction::parse_intel(source).unwrap();
        assert_eq!(
            instruction_size(&instruction).unwrap(),
            Some(expected),
            "{source}"
        );
        let program = Program {
            items: vec![Item::X86Instruction(instruction)],
        };
        let typed = assemble_program_object_with_features(&program, Features::default()).unwrap();
        let source_bytes = text(&assemble_object(source).unwrap());
        assert_eq!(text(&typed), source_bytes, "{source}");
        assert_eq!(source_bytes.len(), expected, "{source}");
    }
}

#[test]
fn optional_encodings_use_the_same_features_promotion_and_compressed_displacements() {
    let features = Features::parse("avx2,avx512f,avx512bw,avx512vl,apx_f").unwrap();
    for source in [
        "vaddps ymm0,ymm1,ymm2",
        "vaddps ymm0,ymm1,ymm9",
        "mov r20,r21",
        "vaddps zmm20,zmm21,zmm22",
        "vmovdqu8 zmm1,[rax+64]",
        "vmovdqu8 zmm1,[rax+65]",
        "vmovdqu8 xmm20,[r12+16]",
    ] {
        let instruction = Instruction::parse_intel(source).unwrap();
        assert!(instruction_size(&instruction).is_err(), "{source}");
        let size = instruction_size_with_features(&instruction, features).unwrap();
        let typed = assemble_program_object_with_features(
            &Program {
                items: vec![Item::X86Instruction(instruction)],
            },
            features,
        )
        .unwrap();
        let source_bytes = text(&assemble_object_with_features(source, features).unwrap());
        assert_eq!(size, Some(source_bytes.len()), "{source}");
        assert_eq!(text(&typed), source_bytes, "{source}");
    }
    let compressed = Instruction::parse_intel("vmovdqu8 zmm1,[rax+64]").unwrap();
    let uncompressed = Instruction::parse_intel("vmovdqu8 zmm1,[rax+65]").unwrap();
    assert_eq!(
        instruction_size_with_features(&compressed, features).unwrap(),
        Some(7)
    );
    assert_eq!(
        instruction_size_with_features(&uncompressed, features).unwrap(),
        Some(10)
    );

    // Typed producers can still carry the pre-EVEX opcode. The public query
    // must promote it before checking permissions, just like object assembly.
    let mut instruction = Instruction::parse_intel("vaddps zmm0,zmm1,zmm2").unwrap();
    instruction.opcode = Opcode::Vaddps;
    let original = instruction.clone();
    assert!(instruction_size_with_features(&instruction, Features::parse("avx").unwrap()).is_err());
    assert_eq!(
        instruction_size_with_features(&instruction, features).unwrap(),
        Some(6)
    );
    let bytes = assemble_program_object_with_features(
        &Program {
            items: vec![Item::X86Instruction(instruction.clone())],
        },
        features,
    )
    .unwrap();
    assert_eq!(text(&bytes).len(), 6);
    assert_eq!(
        instruction, original,
        "query must not mutate the caller's instruction"
    );
}

#[test]
fn symbolic_operands_and_relative_branches_require_layout() {
    for source in [
        "jmp target",
        "je target",
        "call external@PLT",
        "jmp 0",
        "mov rax,[rip+external@GOTPCREL]",
        "mov eax,[rbx+end-start]",
        "add eax,end-start",
        "mov rax,external",
    ] {
        let instruction = Instruction::parse_intel(source).unwrap();
        assert_eq!(instruction_size(&instruction).unwrap(), None, "{source}");
    }
    for (padding, expected) in [(0, 2), (128, 5)] {
        let source = format!("jmp target\n.zero {padding}\ntarget:");
        assert_eq!(
            text(&assemble_object(&source).unwrap()).len(),
            padding + expected
        );
    }
    // This symbolic displacement can become either disp8 or disp32 after layout.
    for (padding, expected) in [(127, 3), (128, 6)] {
        let source = format!("start:\n.zero {padding}\nend:\nmov eax,[rbx+end-start]");
        assert_eq!(
            text(&assemble_object(&source).unwrap()).len(),
            padding + expected
        );
    }
    let symbolic_avx = Instruction::parse_intel("vaddps ymm0,ymm1,[rip+external]").unwrap();
    assert!(instruction_size(&symbolic_avx).is_err());
    assert_eq!(
        instruction_size_with_features(&symbolic_avx, Features::parse("avx").unwrap()).unwrap(),
        None
    );
}

#[test]
fn invalid_concrete_encodings_and_feature_sets_are_errors() {
    for source in ["mov ah,r8b", "cmp eax,rcx", "cmp al,256", "lock cmp eax,1"] {
        let instruction = Instruction::parse_intel(source).unwrap();
        let error = instruction_size(&instruction).unwrap_err();
        assert_eq!(error.line, 0, "{source}");
        assert!(assemble_object(source).is_err(), "{source}");
    }
    let features = Features {
        avx: true,
        ..Features::default()
    };
    let instruction = Instruction::parse_intel("ret").unwrap();
    assert!(instruction_size_with_features(&instruction, features).is_err());
}
