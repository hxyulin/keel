//! Exercise independent stage choices and custom implementations on both targets.
use keel_codegen::regalloc::{Allocation, AllocationInput, AllocationTier, RegisterAllocator};
use keel_codegen::select::{Selection, Target as SelectionTarget};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier, Target};
use keel_ir::{Function, InstData, Value};
use std::cell::Cell;

const SOURCE: &str = "func @f(i64, i64, i64) -> i64 {
block0(v0: i64, v1: i64, v2: i64):
    v3 = imul v0, v1
    v4 = iadd v3, v2
    return v4
}";

struct Selector(Cell<usize>);
impl InstructionSelector for Selector {
    fn name(&self) -> &'static str {
        "test-selector"
    }
    fn select(
        &self,
        func: &Function,
        target: SelectionTarget,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> Selection {
        self.0.set(self.0.get() + 1);
        SelectionTier::Full.select(func, target, folds)
    }
}
struct Allocator(Cell<usize>);
impl RegisterAllocator for Allocator {
    fn name(&self) -> &'static str {
        "test-allocator"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        self.0.set(self.0.get() + 1);
        AllocationTier::Full.allocate(input)
    }
}

#[test]
fn backends_use_supplied_stages() {
    let module = keel_ir::parse(SOURCE).unwrap();
    for target in Target::NATIVE {
        let selector = Selector(Cell::new(0));
        let allocator = Allocator(Cell::new(0));
        let output = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            },
        )
        .unwrap();
        assert_eq!(selector.0.get(), 1);
        assert_eq!(allocator.0.get(), 1);
        if target == Target::Aarch64AppleDarwin {
            assert!(output.contains("\tmadd\t"));
        }
    }
}

#[test]
fn independent_tiers_preserve_legacy_defaults_and_tree_selection() {
    let module = keel_ir::parse(SOURCE).unwrap();
    for target in Target::NATIVE {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for selector in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                let output = keel_codegen::compile_with_pipeline(
                    &module,
                    target,
                    &CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    },
                )
                .unwrap();
                if selector == SelectionTier::Baseline && allocator == AllocationTier::Full {
                    assert_eq!(output, keel_codegen::compile(&module, target).unwrap());
                }
                if target == Target::Aarch64AppleDarwin {
                    assert_eq!(
                        output.contains("\tmadd\t"),
                        selector != SelectionTier::Baseline
                    );
                }
            }
        }
    }
}
