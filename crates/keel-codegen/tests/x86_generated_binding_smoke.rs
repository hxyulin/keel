//! Generated compile-only bindings; no emitted system instructions execute.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
#[path = "common/userspace.rs"]
mod userspace;
#[test]
fn generated_system_and_query_bindings_assemble_in_all_pipelines() {
    let source = include_str!("generated/x86-binding-smoke.ir");
    assert_eq!(source.matches("func @").count(), 87);
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    userspace::compile(source, level, selector, allocator, empty);
                }
            }
        }
    }
}
