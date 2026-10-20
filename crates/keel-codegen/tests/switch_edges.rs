//! Sparse-switch default control flow with out-of-line case argument copies.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};

#[test]
fn sparse_default_jumps_over_case_pads_on_both_targets() {
    let module = keel_ir::parse(
        "func @f(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 1
    switch v0, block1, [259: block2(v1), 288: block2(v0)]
block1:
    v2 = iconst.i32 0
    jump block2(v2)
block2(v3: i32):
    return v3
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let arm = keel_codegen::aarch64::compile_with_pipeline(
                &module,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::assemble_object(&arm).unwrap();
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::x86::assemble_object(&x86).unwrap();
            for (text, jump) in [(&arm, "b"), (&x86, "jmp")] {
                // An unmatched value traverses every comparison. The last
                // instruction before the first case pad must reach default,
                // rather than falling into the case's argument-copy path.
                let first_pad = text
                    .lines()
                    .position(|line| line.ends_with(':') && line.contains("_edge"))
                    .expect("case argument landing pad");
                let previous = text.lines().take(first_pad).last().unwrap().trim();
                let mut words = previous.split_whitespace();
                assert_eq!(words.next(), Some(jump), "{text}");
                assert!(
                    words
                        .next()
                        .unwrap()
                        .trim_matches('"')
                        .ends_with("f_block1"),
                    "{text}"
                );
            }
        }
    }
}
