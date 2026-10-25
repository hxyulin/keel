//! Shared explicit-level text/typed/chunked parity and operand feature gate.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
    x86_features::FeatureSelector,
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

pub fn compile(
    source: &str,
    level: CpuLevel,
    s: SelectionTier,
    a: AllocationTier,
    spilled: bool,
) -> (Vec<u8>, String) {
    compile_with_allocator(source, level, s, &a, spilled)
}

pub fn compile_with_allocator(
    source: &str,
    level: CpuLevel,
    s: SelectionTier,
    allocator: &dyn RegisterAllocator,
    spilled: bool,
) -> (Vec<u8>, String) {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut module).unwrap();
    }
    let features = Features::for_level(level);
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator,
    };
    let pools = if spilled {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        &module,
        pools,
        &pipeline,
        &mut |p| chunks.items.extend(p.items),
    )
    .unwrap();
    for p in [&program, &chunks] {
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let object = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (object, text)
}
