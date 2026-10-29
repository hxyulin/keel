//! Target covers must preserve custom allocation and explicit source boundaries.
use keel_codegen::regalloc::{AllocationTier, Constraint, OperandKind, ValuePart};
use keel_codegen::select::{Selection, Target};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier};
use keel_ir::{EntityRef, Function, InstData, Value};

const REPLICATE: &str = "func @rep(ptr,ptr) {
block0(v0: ptr,v1: ptr):
 v2 = load.i32 v0
 v3 = vsplat.i32x4 v2
 store v3,v1
 return
}";
const ADD: &str = "func @add(ptr,ptr,ptr) {
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i32x4 v0
 v4 = load.i64x2 v1
 v5 = swiden_low.i64x2 v3
 v6 = viadd v5,v4
 store v6,v2
 return
}";
fn pair(f: &Function) -> (keel_ir::Inst, keel_ir::Inst) {
    let consumer = f
        .layout_insts()
        .find(|(_, i)| {
            matches!(
                f.inst_data(*i),
                InstData::VectorSplat { .. } | InstData::VectorBinary { .. }
            )
        })
        .unwrap()
        .1;
    let insts = f.block_insts(f.inst_block(consumer));
    let at = insts.iter().position(|&i| i == consumer).unwrap();
    (insts[at - 1], consumer)
}
struct Constrained {
    producer: bool,
}
impl InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "fusion-boundary"
    }
    fn select(
        &self,
        f: &Function,
        target: Target,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> Selection {
        let mut plan = SelectionTier::Baseline.select(f, target, folds);
        let (producer, consumer) = pair(f);
        let inst = if self.producer { producer } else { consumer };
        let value = match *f.inst_data(inst) {
            InstData::Load { addr, .. } => addr,
            InstData::VectorSplat { arg, .. } | InstData::Convert { arg, .. } => arg,
            InstData::VectorBinary { args, .. } => args[0],
            _ => unreachable!(),
        };
        plan.set_constraint(
            inst,
            ValuePart::low(f.resolve(value)),
            OperandKind::Use,
            Constraint::Register,
        );
        plan
    }
}
#[test]
fn either_instruction_constraint_prevents_vector_cover() {
    for (src, fused) in [(REPLICATE, "ld1r\t"), (ADD, "saddw\t")] {
        let m = keel_ir::parse(src).unwrap();
        for producer in [false, true] {
            let selector = Constrained { producer };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            let text = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert!(!text.contains(fused), "{text}");
        }
    }
}
#[test]
fn distinct_source_lines_prevent_covers_but_same_lines_allow_them() {
    for (src, fused) in [(REPLICATE, "ld1r\t"), (ADD, "saddw\t")] {
        for same in [false, true] {
            let mut m = keel_ir::parse(src).unwrap();
            m.files.push("fusion.c".into());
            let f = &mut m.functions[0];
            let (producer, consumer) = pair(f);
            for (inst, line) in [(producer, 10), (consumer, if same { 10 } else { 11 })] {
                f.set_inst_loc(
                    inst,
                    Some(keel_ir::SourceLoc {
                        file: keel_ir::SourceFile::new(0),
                        line,
                        col: 1,
                        scope: None,
                    }),
                );
            }
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Baseline,
                allocator: &AllocationTier::Full,
            };
            let text = keel_codegen::aarch64::compile_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert_eq!(text.contains(fused), same, "{text}");
        }
    }
}
