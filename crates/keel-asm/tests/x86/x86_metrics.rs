use keel_asm::x86::{self, Features, RegionKind, RelocKind};
use object::{Object, ObjectSection, ObjectSymbol};

fn check(source: &str, features: Features) -> x86::EncodingReport {
    let (bytes, report) = x86::assemble_object_with_metrics(source, features).unwrap();
    assert_eq!(
        bytes,
        x86::assemble_object_with_features(source, features).unwrap()
    );
    let file = object::File::parse(bytes.as_slice()).unwrap();
    for (index, section) in report.sections.iter().enumerate() {
        let mut end = 0;
        let object_section = file.section_by_name(&section.name).unwrap();
        assert_eq!(object_section.size(), section.size, "{}", section.name);
        for region in report.regions.iter().filter(|r| r.section == index) {
            assert_eq!(region.offset, end, "overlap or gap in {}", section.name);
            end += region.size;
            if region.kind == RegionKind::Instruction {
                assert_eq!(
                    region.bytes,
                    object_section.data().unwrap()[region.offset as usize..end as usize]
                );
                assert_eq!(region.bytes.len() as u64, region.size);
            } else {
                assert!(region.bytes.is_empty());
            }
        }
        assert_eq!(end, section.size);
    }
    report
}

#[test]
fn final_metrics_preserve_objects_and_separate_jump_tables_padding_and_relocations() {
    let source = ".text\n.globl entry\nentry:\n cmp dil,3\n je target\n .byte 0xeb,0xfe,0x90\n .p2align 4\n jmp far\n .zero 140\ntarget:\n call external@PLT\nfar:\n ret\n.data\n.quad entry\n.bss\n.zero 23\n.p2align 4\n";
    let report = check(source, Features::default());
    let text = report
        .sections
        .iter()
        .position(|s| s.name == ".text")
        .unwrap();
    let code: Vec<_> = report
        .regions
        .iter()
        .filter(|r| r.kind == RegionKind::Instruction)
        .collect();
    assert_eq!(code.len(), 5);
    assert_eq!(code[0].prefixes, [0x40]);
    assert_eq!(code[0].line, 4);
    let branch = code[1].branch.as_ref().unwrap();
    assert_eq!(branch.displacement_bytes, 4);
    assert!(branch.displacement.unwrap() > 127);
    let bytes = x86::assemble_object(source).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let target = file
        .symbols()
        .find(|s| s.name().ok() == Some("target"))
        .unwrap();
    assert_eq!(branch.target_offset, Some(target.address()));
    assert!(code[3].branch.as_ref().unwrap().displacement.is_none());
    assert_eq!(code[3].relocations[0].kind, RelocKind::Plt32);
    assert_eq!(code[3].relocations[0].kind.name(), "R_X86_64_PLT32");
    assert!(
        report
            .regions
            .iter()
            .any(|r| r.section == text && r.kind == RegionKind::Alignment)
    );
    assert!(
        report
            .regions
            .iter()
            .any(|r| r.section == text && r.kind == RegionKind::Data && r.size == 3)
    );
    assert!(
        report
            .sections
            .iter()
            .any(|s| s.name == ".bss" && s.nobits && s.size == 32)
    );
    let program = keel_asm_ir::x86::parse_program(source).unwrap();
    let (typed, typed_metrics) =
        x86::assemble_program_object_with_metrics(&program, Features::default()).unwrap();
    assert_eq!(typed, bytes);
    assert_eq!(
        typed_metrics
            .regions
            .iter()
            .filter(|r| r.kind == RegionKind::Instruction)
            .map(|r| &r.bytes)
            .collect::<Vec<_>>(),
        code.iter().map(|r| &r.bytes).collect::<Vec<_>>()
    );
}

#[test]
fn prefixes_are_taken_from_final_vex_evex_rex2_and_legacy_encodings() {
    let features = Features::parse("avx512f,apx_f").unwrap();
    let source = "cmp ax,3\nvaddps ymm0,ymm1,ymm2\nvaddps ymm0,ymm1,ymm9\nvaddps zmm20,zmm21,zmm22\nmov r20,r21\n";
    let report = check(source, features);
    let code: Vec<_> = report
        .regions
        .iter()
        .filter(|r| r.kind == RegionKind::Instruction)
        .collect();
    assert_eq!(code[0].prefixes, [0x66]);
    for (r, first, length) in [
        (code[1], 0xc5, 2),
        (code[2], 0xc4, 3),
        (code[3], 0x62, 4),
        (code[4], 0xd5, 2),
    ] {
        assert_eq!(r.prefixes[0], first);
        assert_eq!(r.prefixes.len(), length);
    }
    assert!(x86::assemble_object_with_metrics(source, Features::default()).is_err());
}

#[test]
fn folded_aliases_macro_lines_short_backward_branches_and_generated_sections_remain_exact() {
    let source = ".file 1 \"metrics.c\"\n.text\n.cfi_startproc\nstart:\n.loc 1 2 0\n.macro step\n add eax,1\n.endm\n.rept 2\n step\n.endr\nend:\nmov eax,[rbx+end-start]\njne start\n.p2align 6,,1\nret\n.cfi_endproc\n";
    let report = check(source, Features::default());
    let code: Vec<_> = report
        .regions
        .iter()
        .filter(|r| r.kind == RegionKind::Instruction)
        .collect();
    assert_eq!(code.len(), 5);
    assert_eq!(code[0].line, code[1].line);
    assert_eq!(code[2].size, 3);
    let branch = code[3].branch.as_ref().unwrap();
    assert_eq!(branch.displacement_bytes, 1);
    assert!(branch.displacement.unwrap() < 0);
    assert_eq!(branch.target_offset, Some(0));
    assert!(
        !report
            .regions
            .iter()
            .any(|r| r.kind == RegionKind::Alignment && r.section == 0)
    );
    assert!(report.sections.iter().any(|s| s.name == ".eh_frame"));
}

#[test]
fn indirect_targets_and_interposable_definitions_do_not_invent_branch_distances() {
    let source = "mov rax,[rip+external]\ncall qword ptr [rip+external]\njmp rax\n.globl exported\nexported:\ncall exported\nret\n";
    let report = check(source, Features::default());
    let code: Vec<_> = report
        .regions
        .iter()
        .filter(|r| r.kind == RegionKind::Instruction)
        .collect();
    assert_eq!(code[0].relocations.len(), 1);
    assert_eq!(code[1].relocations.len(), 1);
    assert!(code[1].branch.is_none());
    assert!(code[2].branch.is_none());
    assert_eq!(code[3].relocations.len(), 1);
    assert!(code[3].branch.as_ref().unwrap().displacement.is_none());
}

#[test]
fn explicit_dialect_and_binary_resolver_preserve_bytes_and_resolve_once() {
    let source = "addl $1,%eax\n.incbin \"blob\",1,2\n.incbin \"blob\",0,1\n";
    let payload = [0x90, 0xeb, 0xfe];
    let mut reads = 0;
    let (bytes, report) = x86::assemble_object_with_metrics_and_binary_inputs(
        source,
        x86::Syntax::Att,
        Features::default(),
        &mut |name| {
            assert_eq!(name, "blob");
            reads += 1;
            Ok(payload.to_vec())
        },
    )
    .unwrap();
    assert_eq!(reads, 1);
    let ordinary = x86::assemble_object_with_binary_inputs(
        source,
        x86::Syntax::Att,
        Features::default(),
        &mut |_| Ok(payload.to_vec()),
    )
    .unwrap();
    assert_eq!(bytes, ordinary);
    assert_eq!(report.regions[0].kind, RegionKind::Instruction);
    assert_eq!(report.regions[0].bytes, [0x83, 0xc0, 1]);
    assert_eq!(report.regions[1].kind, RegionKind::Data);
    assert_eq!(report.regions[1].size, 3);
}
