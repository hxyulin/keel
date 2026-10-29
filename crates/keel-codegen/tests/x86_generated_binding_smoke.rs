//! Generated compile-only bindings; no emitted system instructions execute.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
#[path = "common/userspace.rs"]
mod userspace;
#[test]
fn generated_system_and_query_bindings_assemble_in_all_pipelines() {
    let source = include_str!("generated/x86-binding-smoke.ir");
    assert_eq!(source.matches("func @").count(), 87);
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(keel_codegen::aarch64::compile(&module).is_err());
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    let (_, text) = userspace::compile(source, level, selector, allocator, empty);
                    // LAR and LSL name the selector register at destination width.
                    for opcode in ["lar", "lsl"] {
                        for (destination, segment) in [("ax", "cx"), ("eax", "ecx"), ("rax", "rcx")]
                        {
                            let form = format!("\t{opcode}\t{destination}, {segment}");
                            assert!(text.contains(&form), "missing {form}");
                        }
                    }
                }
            }
        }
    }
}
