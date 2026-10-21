//! Bounded memcpy uses the established vector path on both targets, including
//! custom selector constraints and empty register pools. No copy cover may
//! hide an operand from allocation.
use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::regalloc::{AllocationTier, Constraint, OperandKind, Pools, ValuePart};
use keel_codegen::select::{Selection, Target};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier};
use keel_ir::{Function, InstData, Value};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Constrained;
impl InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "copy-operands"
    }
    fn select(
        &self,
        f: &Function,
        t: Target,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> Selection {
        let mut plan = SelectionTier::Baseline.select(f, t, folds);
        for (_, i) in f.layout_insts() {
            match *f.inst_data(i) {
                InstData::Load { addr, .. } => {
                    plan.set_constraint(
                        i,
                        ValuePart::low(addr),
                        OperandKind::Use,
                        Constraint::Register,
                    );
                    plan.set_constraint(
                        i,
                        ValuePart::low(f.first_result(i)),
                        OperandKind::Def,
                        Constraint::Register,
                    );
                }
                InstData::Store { value, addr, .. } => {
                    for v in [value, addr] {
                        plan.set_constraint(
                            i,
                            ValuePart::low(v),
                            OperandKind::Use,
                            Constraint::Register,
                        );
                    }
                }
                _ => {}
            }
        }
        plan
    }
}

#[test]
fn exact_copy_both_targets_selectors_allocators_spills_and_objects() {
    for size in [16, 17, 24, 31, 32] {
        let mut m = keel_ir::parse(&format!("func @copy(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\nv2 = iconst.i64 {size}\nintrinsic memcpy(v0,v1,v2)\nreturn\n}}" )).unwrap();
        keel_opt::run_passes(&mut m, &["small-memory"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for selector in [
            &SelectionTier::Baseline as &dyn InstructionSelector,
            &SelectionTier::Full,
            &Constrained,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector,
                    allocator: &allocator,
                };
                for spilled in [false, true] {
                    // Explicit Register constraints require an allocatable pool.
                    // Empty-pool scratch/spill handling is checked with both builtin selectors.
                    if spilled && selector.name() == "copy-operands" {
                        continue;
                    }
                    for arm in [true, false] {
                        let pools = if spilled {
                            &EMPTY
                        } else if arm {
                            &keel_codegen::aarch64::REGISTERS
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let (text, program) = if arm {
                            (
                                keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline)
                                    .unwrap(),
                                keel_codegen::aarch64::compile_program_with_pipeline(
                                    &m, pools, &pipeline,
                                )
                                .unwrap(),
                            )
                        } else {
                            (
                                keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)
                                    .unwrap(),
                                keel_codegen::x86_64::compile_program_with_pipeline(
                                    &m, pools, &pipeline,
                                )
                                .unwrap(),
                            )
                        };
                        let (parsed, typed) = if arm {
                            (
                                assemble_object(&text).unwrap(),
                                assemble_program_object(&program, TargetFeatures::default())
                                    .unwrap(),
                            )
                        } else {
                            (
                                keel_asm::x86::assemble_object(&text).unwrap(),
                                keel_asm::x86::assemble_program_object(&program).unwrap(),
                            )
                        };
                        assert_eq!(parsed, typed, "{text}");
                        if arm {
                            assert!(text.contains("ldr\tq"), "{text}");
                            assert!(text.contains("str\tq"), "{text}");
                        } else {
                            assert!(text.contains("movups"), "{text}");
                        }
                    }
                }
            }
        }
    }
}
