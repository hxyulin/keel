//! A successful load is reused on a conditional shift edge on both backends.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};
use keel_ir::InstData;

const SOURCE: &str = "func @shift(ptr, ptr, i32) cc(c) {
block0(v0: ptr, v1: ptr, v2: i32):
 v3 = load.i32 v0
 v4 = icmp.sgt v3, v2
 brif v4, block1, block2
block1:
 v5 = load.i32 v0
 store v5, v1
 return
block2:
 return
}";

#[test]
fn conditional_reload_removed_for_both_targets_and_tiers() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        for target in Target::ALL {
            let mut m = keel_ir::parse(SOURCE).unwrap();
            keel_ir::verify_module(&m).unwrap();
            keel_opt::run_passes(&mut m, tier.pipeline()).unwrap();
            keel_opt::run_passes(&mut m, &["load-reuse"]).unwrap();
            let f = &m.functions[0];
            assert_eq!(
                f.layout_insts()
                    .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
                    .count(),
                1
            );
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
                    assert_eq!(text.matches(" = load.i32 ").count(), 1, "{text}");
                    // Exercise structured encoding too, rather than trusting IR comments.
                    let program =
                        keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
                    let object = match target {
                        Target::Aarch64AppleDarwin => {
                            keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                        }
                        Target::X86_64LinuxGnu => {
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        }
                    };
                    assert!(!object.is_empty());
                }
            }
        }
    }
}
