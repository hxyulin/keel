//! Object serialization for the opt-in allocator on both code generators.
#![cfg(feature = "experimental-regalloc")]
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

const JOIN: &str = "func @f(ptr, i64, i64, i8) -> i64 {
block0(v0: ptr, v1: i64, v2: i64, v3: i8):
    brif v3, block2(v2, v1), block1
block1:
    store v1, v0
    jump block2(v2, v1)
block2(v4: i64, v5: i64):
    v6 = isub v4, v5
    return v6
}";

#[test]
fn affinity_objects_match_text_on_both_targets() {
    for src in [
        JOIN,
        include_str!("programs/regalloc_calls.keel"),
        include_str!("programs/regalloc_loops.keel"),
        include_str!("programs/regalloc_pressure.keel"),
    ] {
        let module = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in Target::NATIVE {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::FullAffinity,
                };
                let text = keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
                let program =
                    keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                        .unwrap();
                let (typed, parsed) = match target {
                    keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                    Target::Aarch64AppleDarwin => (
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                        keel_asm::assemble_object(&text).unwrap(),
                    ),
                    Target::Aarch64LinuxGnu => (
                        keel_asm::assemble_elf_program_object(&program, Default::default())
                            .unwrap(),
                        keel_asm::assemble_elf_object(&text).unwrap(),
                    ),
                    Target::X86_64LinuxGnu => (
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                        keel_asm::x86::assemble_object(&text).unwrap(),
                    ),
                };
                assert!(!typed.is_empty());
                assert_eq!(typed, parsed, "{target:?} {selector:?}");
            }
        }
    }
}
