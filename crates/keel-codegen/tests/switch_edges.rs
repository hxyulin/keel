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

#[test]
fn x86_edges_whose_arguments_are_in_place_need_no_landing_pad() {
    // Both back edges pass block1's own parameter, so their copies are
    // no-ops and each branch can target block1 directly.
    let module = keel_ir::parse(
        "func @f(i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v5: i32):
    jump block1(v1)
block1(v2: i32):
    v3 = iadd v2, v5
    brif v3, block2, block1(v2)
block2:
    switch v0, block3, [1: block1(v2), 2: block3, 3: block1(v2), 4: block3]
block3:
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
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::x86::assemble_object(&x86).unwrap();
            assert!(
                !x86.lines().any(|line| line.ends_with(':')
                    && (line.contains("_else") || line.contains("_edge"))),
                "{x86}"
            );
        }
    }
}
