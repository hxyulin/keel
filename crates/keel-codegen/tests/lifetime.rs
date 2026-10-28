//! Lifetime events constrain optimization and add no native instruction sequence.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn fixed_slot_lifetime_events_emit_no_native_instructions() {
    let source = "func @f() -> i64 {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    intrinsic lifetime_start(v0)
    v1 = iconst.i64 37
    store v1, v0
    v2 = load.i64 v0
    intrinsic lifetime_end(v0)
    return v2
}";
    let marked = keel_ir::parse(source).unwrap();
    let plain = keel_ir::parse(
        &source
            .replace("    intrinsic lifetime_start(v0)\n", "")
            .replace("    intrinsic lifetime_end(v0)\n", ""),
    )
    .unwrap();
    keel_ir::verify_module(&marked).unwrap();
    for target in Target::NATIVE {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let compile = |m| {
                keel_codegen::compile_with_pipeline(m, target, &pipeline)
                    .unwrap()
                    .lines()
                    .filter(|line| {
                        !line.trim_start().starts_with(';')
                            && !line.trim_start().starts_with('#')
                            && !line.trim_start().starts_with("//")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(compile(&marked), compile(&plain), "{target:?} {selector:?}");
        }
    }
}
