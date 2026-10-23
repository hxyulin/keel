//! A live YMM0 witness independent of natural allocation heuristics. Every other
//! SSA value spills. The standard allocation validator checks the resulting plan.
use keel_codegen::regalloc::{
    Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator,
};
use keel_ir::EntityRef;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
pub struct LiveWide0(pub AllocationTier);
impl RegisterAllocator for LiveWide0 {
    fn name(&self) -> &'static str {
        "forced-live-ymm0"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let empty = AllocationInput {
            func: input.func,
            pools: &EMPTY,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        };
        let mut result = self.0.allocate(&empty);
        let mut count = 0;
        for (_, inst) in input.func.layout_insts() {
            for &value in input.func.inst_results(inst) {
                let ty = input.func.value_type(value);
                if input.func.resolve(value) == value && ty.is_vector() && ty.bytes() == Some(32) {
                    assert!(matches!(result.locs[0][value.index()], Some(Loc::Stack(_))));
                    result.locs[0][value.index()] = Some(Loc::FReg(0));
                    count += 1;
                }
            }
        }
        assert_eq!(
            count, 1,
            "fixture must have exactly one live wide SSA value"
        );
        result
    }
}
