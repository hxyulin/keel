#[path = "common/x86_packet.rs"]
mod packet;

#[test]
fn state_and_streaming_operations_preserve_explicit_effects() {
    packet::check(
        include_str!("fixtures/x86_state_intrinsics.c"),
        include_str!("fixtures/x86_state_intrinsics_runner.c"),
        "KEEL_C_STATE_EXPORT_DIR",
        "state",
        None,
    );
}

#[test]
fn monitor_wait_wrappers_encode_without_executing_privileged_instructions() {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::AllocationTier,
        x86_features::{CpuLevel, FeatureSelector, Features},
    };
    use keel_ir::Target;
    let module = packet::frontend(
        "#include <pmmintrin.h>\nvoid f(const void*p,unsigned a,unsigned b){_mm_monitor(p,a,b);_mm_mwait(a,b);}",
    );
    let features = Features::for_level(CpuLevel::V3)
        .with_features("monitor")
        .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let program = keel_codegen::compile_program_with_pipeline(
        &module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap();
    assert!(
        keel_asm::x86::assemble_program_object_with_features(
            &program,
            Features::for_level(CpuLevel::V3)
        )
        .is_err()
    );
    let object = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert!(object.windows(3).any(|x| x == [0x0f, 0x01, 0xc8]));
    assert!(object.windows(3).any(|x| x == [0x0f, 0x01, 0xc9]));
}
