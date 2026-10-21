//! Canonical masks are a lane-value proof, independent of register allocation.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const ARM_SPILLED: Pools = Pools {
    float_caller_saved: &[],
    float_callee_saved: &[],
    ..keel_codegen::aarch64::REGISTERS
};
const X86_SPILLED: Pools = Pools {
    float_caller_saved: &[],
    float_callee_saved: &[],
    ..keel_codegen::x86_64::REGISTERS
};

#[test]
fn direct_masks_and_conservative_boundaries() {
    for shape in ["i8x16", "i16x8", "i32x4", "i64x2"] {
        for mode in ["direct", "loaded", "phi", "bitcast"] {
            let mask = match mode {
                "direct" => " v5 = vicmp.sgt v3,v4\n".into(),
                "loaded" => format!(" v5 = load.{shape} v0\n"),
                "phi" => format!(" v6 = vicmp.sgt v3,v4\n jump block1(v6)\nblock1(v5: {shape}):\n"),
                "bitcast" => format!(
                    " v6 = bitcast.i8x16 v3\n v7 = bitcast.i8x16 v4\n v8 = vicmp.sgt v6,v7\n v5 = bitcast.{shape} v8\n"
                ),
                _ => unreachable!(),
            };
            let source = format!(
                "func @proof(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} v0\n v4 = load.{shape} v1\n{mask} v9 = vselect v5,v3,v4\n store v9,v2\n return\n}}"
            );
            let module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocation,
                    };
                    for spilled in [false, true] {
                        let pools = if spilled {
                            &ARM_SPILLED
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let text =
                            keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::aarch64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            keel_asm::assemble_object(&text).unwrap(),
                            keel_asm::assemble_program_object(&program, Default::default())
                                .unwrap()
                        );
                        let select = text.split(" = vselect ").nth(1).unwrap();
                        assert_eq!(
                            select.contains("cmeq\t"),
                            mode != "direct",
                            "{mode}/{shape}: {text}"
                        );
                        let pools = if spilled {
                            &X86_SPILLED
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            keel_asm::x86::assemble_object(&text).unwrap(),
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        );
                        let select = text.split(" = vselect ").nth(1).unwrap();
                        assert_eq!(
                            select.contains("andnps"),
                            mode != "direct",
                            "{mode}/{shape}: {text}"
                        );
                    }
                }
            }
        }
    }
}
