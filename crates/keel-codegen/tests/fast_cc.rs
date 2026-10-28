use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::{CodegenPipeline, SelectionTier};

const SOURCE: &str = include_str!("../../../suite/ir/llvm_import_fast_fixtures/fast.keel");
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

#[test]
fn private_fast_calls_cover_stack_args_recursion_and_spills() {
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for arm in [true, false] {
                let (text, program) = if arm {
                    (
                        keel_codegen::aarch64::compile_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                        keel_codegen::aarch64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                } else {
                    (
                        keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap(),
                        keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
                            .unwrap(),
                    )
                };
                let (parsed, typed) = if arm {
                    (
                        keel_asm::assemble_object(&text).unwrap(),
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                    )
                } else {
                    (
                        keel_asm::x86::assemble_object(&text).unwrap(),
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                    )
                };
                assert_eq!(parsed, typed);
            }
        }
    }
}

const LEAF: &str = "func @leaf(i64) -> i64 cc(fast) local {
block0(v0: i64):
 v1 = iconst.i64 42
 v2 = iadd v0, v1
 return v2
}";

#[test]
fn fast_leaf_omits_frame_and_retains_unwind_metadata() {
    use object::{Object, ObjectSection};
    let m = keel_ir::parse(LEAF).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let arm = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!arm.contains("stp\tx29, x30"), "{arm}");
            if selector == SelectionTier::Full {
                assert!(arm.contains("add\tx0, x0, #42"), "{arm}");
                assert!(!arm.contains("mov\tx9, x0"), "{arm}");
            }
            let program = keel_codegen::aarch64::compile_program_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let bytes = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(bytes, keel_asm::assemble_object(&arm).unwrap());
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let unwind = file
                .section_by_name("__compact_unwind")
                .unwrap()
                .data()
                .unwrap();
            assert_eq!(
                u32::from_le_bytes(unwind[12..16].try_into().unwrap()),
                0x0200_0000
            );
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!x86.contains("push\trbp"), "{x86}");
            let program = keel_codegen::x86_64::compile_program_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert_eq!(bytes, keel_asm::x86::assemble_object(&x86).unwrap());
            let file = object::File::parse(bytes.as_slice()).unwrap();
            assert!(file.section_by_name(".eh_frame").unwrap().size() > 0);
        }
    }
}

#[test]
fn frame_observers_calls_debug_spills_and_stack_arguments_retain_frames() {
    let mut modules = vec![
        keel_ir::parse(&LEAF.replace("cc(fast) local", "cc(c) local")).unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " v8 = intrinsic frame_address()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " v8 = intrinsic stack_save()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
        keel_ir::parse(&format!(
            "decl @outside()\n{}",
            LEAF.replace(
                " v1 = iconst.i64 42",
                " call @outside()\n v1 = iconst.i64 42"
            )
        ))
        .unwrap(),
        keel_ir::parse(&LEAF.replace(
            " v1 = iconst.i64 42",
            " asm \"nop\", \"\"()\n v1 = iconst.i64 42",
        ))
        .unwrap(),
    ];
    let mut debug = keel_ir::parse(LEAF).unwrap();
    debug.files.push("private-leaf.c".into());
    modules.push(debug);
    let params = (0..10)
        .map(|i| format!("v{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    modules.push(
        keel_ir::parse(&format!(
            "func @stack({}) -> i64 cc(fast) local {{\nblock0({params}):\n return v9\n}}",
            vec!["i64"; 10].join(", ")
        ))
        .unwrap(),
    );
    for m in modules {
        keel_ir::verify_module(&m).unwrap();
        let arm = keel_codegen::compile(&m, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
        assert!(arm.contains("stp\tx29, x30"), "{arm}");
        // The x86 backend rejects stack intrinsics and inline asm independently.
        if let Ok(x86) = keel_codegen::compile(&m, keel_codegen::Target::X86_64LinuxGnu) {
            assert!(x86.contains("push\trbp"), "{x86}");
        }
    }
    let m = keel_ir::parse(LEAF).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let arm = keel_codegen::aarch64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    assert!(arm.contains("stp\tx29, x30"));
    let x86 = keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    assert!(x86.contains("push\trbp"));
}

#[test]
fn tls_getters_keep_lr_frame_and_argument_bank_reserved() {
    let m = keel_ir::parse("data @tls = thread_local align 8 { i64 5 }\nfunc @leaf(i64) -> i64 cc(fast) local {\nblock0(v0: i64):\n v1 = symbol_addr @tls\n v2 = load.i64 v1\n v3 = iadd v0, v2\n return v3\n}").unwrap();
    keel_ir::verify_module(&m).unwrap();
    let arm = keel_codegen::compile(&m, keel_codegen::Target::Aarch64AppleDarwin).unwrap();
    assert!(arm.contains("stp\tx29, x30"), "{arm}");
    assert!(arm.contains("mov\tx9, x0"), "{arm}");
    let x86 = keel_codegen::compile(&m, keel_codegen::Target::X86_64LinuxGnu).unwrap();
    assert!(x86.contains("push\trbp"), "{x86}");
}
