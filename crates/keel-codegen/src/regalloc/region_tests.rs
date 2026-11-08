use super::*;
use crate::regalloc::{
    ControlFlowGraph, Pools, baseline_allocation, build_intervals, transfer_tests,
};

const ACROSS_CALL: &str = r#"
decl @touch()
func @f(ptr,i64) -> i64 {
block0(v0:ptr,v1:i64):
 v2=load.i64 v0
 v3=load.i64 v0
 v4=load.i64 v0
 v5=load.i64 v0
 call @touch()
 v6=iadd v2,v1
 return v6
}
"#;

fn pools() -> Pools<'static> {
    Pools {
        caller_saved: &[],
        callee_saved: &[19],
        float_caller_saved: &[],
        float_callee_saved: &[],
    }
}

// Keep the second argument in a register and force everything else to stack.
// This makes owner displacement necessary rather than relying on scan choices.
fn forced_homes(input: &AllocationInput<'_>) -> (Allocation, Vec<Interval>) {
    let f = input.func;
    let cfg = ControlFlowGraph::compute(f);
    let order = cfg.reverse_postorder(f);
    let intervals = build_intervals(
        f,
        &cfg,
        &order,
        input.is_call,
        input.returns_twice,
        input.selection,
    );
    let mut locs = [vec![None; f.num_values()], vec![None; f.num_values()]];
    let mut slots = 0;
    for interval in &intervals {
        locs[interval.half][interval.value.index()] = Some(Loc::Stack(slots));
        slots += interval.spill_units;
    }
    let owner = f.block_params(f.entry_block().unwrap())[1];
    locs[0][owner.index()] = Some(Loc::Reg(19));
    (
        baseline_allocation(order, locs, slots, input.pools),
        intervals,
    )
}

#[test]
fn restores_owner_before_call_and_keeps_parallel_sources() {
    let module = keel_ir::parse(ACROSS_CALL).unwrap();
    let selection = crate::select::Selection::default();
    let pools = pools();
    let input = AllocationInput {
        func: &module.functions[0],
        selection: &selection,
        pools: &pools,
        is_call: &|d| matches!(d, InstData::Call { .. }),
        returns_twice: &|_| false,
    };
    let (baseline, intervals) = forced_homes(&input);
    let actual = repair(&input, baseline, &intervals, &[]);
    assert!(
        !actual.edits.is_empty(),
        "forced homes must require displacement"
    );
    actual.validate(&input).unwrap();
    for seed in 0..16 {
        transfer_tests::check(&input, &actual, seed);
    }
}

#[test]
fn preserves_existing_edits_and_limits_displacement_to_candidate_uses() {
    let module = keel_ir::parse(
        r#"
func @f(ptr,i64) -> i64 {
block0(v0:ptr,v1:i64):
 v2=load.i64 v0
 v3=load.i64 v0
 v4=load.i64 v0
 v5=load.i64 v0
 jump block1
block1:
 v11=iadd v1,v1
 v6=load.i64 v0
 v7=load.i64 v0
 v8=load.i64 v0
 v9=load.i64 v0
 v10=iadd v9,v11
 v12=iadd v10,v1
 return v12
}
"#,
    )
    .unwrap();
    let f = &module.functions[0];
    let selection = crate::select::Selection::default();
    let pools = pools();
    let input = AllocationInput {
        func: f,
        selection: &selection,
        pools: &pools,
        is_call: &|_| false,
        returns_twice: &|_| false,
    };
    let (mut baseline, intervals) = forced_homes(&input);
    let slots = baseline.spill_slots;
    let pointer = f.block_params(f.entry_block().unwrap())[0];
    let first = f.block_insts(baseline.order[0])[0];
    let prefix = f.block_insts(baseline.order[1])[0];
    let second = f.block_insts(baseline.order[1])[1];
    let home = baseline.loc(pointer).unwrap();
    baseline
        .edits
        .instruction_mut(first)
        .uses
        .push((ValuePart::low(pointer), home));
    let actual = repair(&input, baseline, &intervals, &[]);
    assert_eq!(
        actual.spill_slots,
        slots + 1,
        "second block must displace an owner"
    );
    assert!(
        actual.edits.instruction_index(prefix).is_none(),
        "owner use before candidate window must remain unchanged"
    );
    let existing = actual
        .edits
        .instruction(actual.edits.instruction_index(first).unwrap());
    assert!(existing.before.is_empty());
    assert_eq!(existing.uses, [(ValuePart::low(pointer), home)]);
    assert_eq!(
        actual
            .edits
            .instruction(actual.edits.instruction_index(second).unwrap())
            .before
            .len(),
        2
    );
    actual.validate(&input).unwrap();
    for seed in 0..16 {
        transfer_tests::check(&input, &actual, seed);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn emitted_owner_save_restore_matches_native_oracle() {
    use crate::regalloc::RegisterAllocator;
    struct Forced;
    impl RegisterAllocator for Forced {
        fn name(&self) -> &'static str {
            "forced-region"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let (baseline, intervals) = forced_homes(input);
            let actual = repair(input, baseline, &intervals, &[]);
            assert!(
                !actual.edits.is_empty(),
                "native oracle must execute displacement"
            );
            actual
        }
    }
    let module = keel_ir::parse(ACROSS_CALL).unwrap();
    let pipeline = crate::CodegenPipeline {
        selector: &crate::SelectionTier::Full,
        allocator: &Forced,
    };
    let text = crate::compile_with_pipeline(&module, crate::Target::Aarch64AppleDarwin, &pipeline)
        .unwrap();
    let dir = keel_test_support::TempDir::new("region-native");
    std::fs::write(dir.join("f.s"), text).unwrap();
    std::fs::write(
        dir.join("driver.c"),
        r#"
typedef unsigned long long U;
U f(U *, U);
void touch(void) {
    __asm__ volatile("mov x9,#0\nmov x10,#0" ::: "x9", "x10");
}
int main(void) {
    for (U i = 0; i < 1000; i++) {
        U p = i * 17;
        if (f(&p, i) != i * 18) return 1;
    }
    return 0;
}
"#,
    )
    .unwrap();
    let linked = std::process::Command::new("clang")
        .current_dir(dir.path())
        .args(["driver.c", "f.s", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    assert!(
        std::process::Command::new(dir.join("oracle"))
            .status()
            .unwrap()
            .success()
    );
}
