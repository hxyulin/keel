use super::*;
use crate::regalloc::{Pools, ValuePart, baseline_allocation, build_intervals, transfer_tests};

const LOOP: &str = r#"
func @loop_sum(ptr,i64,i64,i64) -> i64 {
entry(v0:ptr,v1:i64,v2:i64,v3:i64):
 v4=iconst.i64 0
 v14=icmp.ne v3,v4
 brif v14,exit(v4,v2),preheader
preheader:
 jump header(v1,v4,v2)
header(v5:i64,v6:i64,v18:i64):
 v7=icmp.eq v5,v4
 brif v7,exit(v6,v18),body
body:
 v8=load.i64 volatile v0
 v15=icmp.eq v8,v4
 brif v15,exit2(v6,v18),step
step:
 v9=iadd v6,v8
 v10=iconst.i64 1
 v11=isub v5,v10
 jump header(v11,v9,v18)
exit(v12:i64,v20:i64):
 v13=iadd v12,v2
 v21=iadd v13,v20
 return v21
exit2(v16:i64,v22:i64):
 v17=iadd v16,v2
 v23=iadd v17,v22
 return v23
}
"#;

fn forced(input: &AllocationInput<'_>) -> (Allocation, Vec<Interval>, ControlFlowGraph) {
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
    for iv in &intervals {
        locs[iv.half][iv.value.index()] = Some(Loc::Stack(slots));
        slots += iv.spill_units;
    }
    let owner = f.block_params(f.entry_block().unwrap())[2];
    locs[0][owner.index()] = Some(Loc::Reg(input.pools.callee_saved[0]));
    (
        baseline_allocation(order, locs, slots, input.pools),
        intervals,
        cfg,
    )
}

fn pools() -> Pools<'static> {
    Pools {
        caller_saved: &[],
        callee_saved: &[19],
        float_caller_saved: &[],
        float_callee_saved: &[],
    }
}

#[test]
fn shared_exit_phi_and_zero_trip_preserve_tokens() {
    let module = keel_ir::parse(LOOP).unwrap();
    let selection = crate::select::Selection::default();
    let pools = pools();
    let input = AllocationInput {
        func: &module.functions[0],
        selection: &selection,
        pools: &pools,
        is_call: &|_| false,
        returns_twice: &|_| false,
    };
    let (baseline, intervals, cfg) = forced(&input);
    let slots = baseline.spill_slots;
    let (actual, reserved) = repair(&input, baseline, &intervals, &cfg);
    assert!(!reserved.is_empty(), "one read per iteration must qualify");
    assert_eq!(actual.spill_slots, slots + 1);
    actual.validate(&input).unwrap();
    for seed in 0..32 {
        transfer_tests::check(&input, &actual, seed);
    }
    let entry = input.func.entry_block().unwrap();
    let outside = *input.func.block_insts(entry).last().unwrap();
    assert!(
        actual.edits.edge(outside, 0).is_empty(),
        "outside path must not restore an unsaved owner"
    );
}

#[test]
fn header_phi_owner_is_not_an_invariant_register() {
    let source = r#"
func @f(ptr,i64,i64,i64) -> i64 {
entry(v0:ptr,v1:i64,v2:i64,v3:i64):
 v4=iconst.i64 0
 jump header(v1,v4,v2)
header(v5:i64,v6:i64,v20:i64):
 v7=icmp.eq v5,v4
 brif v7,exit,body
body:
 v8=load.i64 volatile v0
 v9=iadd v6,v8
 v10=iconst.i64 1
 v11=isub v5,v10
 jump header(v11,v9,v3)
exit:
 v13=iadd v6,v20
 return v13
}
"#;
    let module = keel_ir::parse(source).unwrap();
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
    let (mut baseline, intervals, cfg) = forced(&input);
    let original = f.block_params(f.entry_block().unwrap())[2];
    let header = cfg.succs(f.entry_block().unwrap())[0];
    let phi = f.block_params(header)[2];
    let stack = baseline.loc(phi);
    baseline.locs[0][original.index()] = stack;
    baseline.locs[0][phi.index()] = Some(Loc::Reg(19));
    let (actual, reserved) = repair(&input, baseline, &intervals, &cfg);
    assert!(
        reserved.is_empty(),
        "a backedge redefines this apparent idle owner"
    );
    for seed in 0..16 {
        transfer_tests::check(&input, &actual, seed);
    }
}

#[test]
fn calls_and_existing_loop_cache_edits_keep_the_baseline() {
    for source in [
        format!(
            "decl @touch()\n{}",
            LOOP.replace("body:\n", "body:\n call @touch()\n")
        ),
        LOOP.to_owned(),
    ] {
        let module = keel_ir::parse(&source).unwrap();
        let f = &module.functions[0];
        let selection = crate::select::Selection::default();
        let pools = pools();
        let input = AllocationInput {
            func: f,
            selection: &selection,
            pools: &pools,
            is_call: &|d| matches!(d, InstData::Call { .. }),
            returns_twice: &|_| false,
        };
        let (mut baseline, intervals, cfg) = forced(&input);
        if module.decls.is_empty() {
            let pointer = f.block_params(f.entry_block().unwrap())[0];
            let home = baseline.loc(pointer).unwrap();
            let load = f
                .layout_insts()
                .find(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
                .unwrap()
                .1;
            baseline
                .edits
                .instruction_mut(load)
                .uses
                .push((ValuePart::low(pointer), home));
        }
        let slots = baseline.spill_slots;
        let (actual, reserved) = repair(&input, baseline, &intervals, &cfg);
        assert!(reserved.is_empty());
        assert_eq!(actual.spill_slots, slots);
        for seed in 0..16 {
            transfer_tests::check(&input, &actual, seed);
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_shared_exit_and_unsigned_loop_oracle() {
    let module = keel_ir::parse(LOOP).unwrap();
    let pipeline = crate::CodegenPipeline {
        selector: &crate::SelectionTier::Full,
        allocator: &Forced,
    };
    let text = crate::compile_with_pipeline(&module, crate::Target::Aarch64AppleDarwin, &pipeline)
        .unwrap();
    let dir = keel_test_support::TempDir::new("loop-region");
    std::fs::write(dir.join("loop.s"), text).unwrap();
    std::fs::write(
        dir.join("driver.c"),
        r#"
typedef unsigned long long U;
U loop_sum(U*,U,U,U);
int main(void) {
    U ns[]={0,1,31,1000}, ps[]={0,1,~0ULL};
    for(unsigned n=0;n<4;n++)for(unsigned p=0;p<3;p++)for(U bypass=0;bypass<2;bypass++) {
        U point=ps[p],owner=~0ULL;
        U want=2*owner+(bypass?0:ns[n]*point);
        if(loop_sum(&point,ns[n],owner,bypass)!=want)return 1;
    }
    return 0;
}
"#,
    )
    .unwrap();
    let linked = std::process::Command::new("clang")
        .current_dir(dir.path())
        .args(["driver.c", "loop.s", "-o", "oracle"])
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

use crate::regalloc::RegisterAllocator;
struct Forced;
impl RegisterAllocator for Forced {
    fn name(&self) -> &'static str {
        "forced-loop-region"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let (baseline, intervals, cfg) = forced(input);
        let (actual, reserved) = repair(input, baseline, &intervals, &cfg);
        assert!(
            !reserved.is_empty(),
            "native test must execute loop displacement"
        );
        actual
    }
}

#[test]
fn loop_edge_copies_match_text_and_structured_objects_on_both_targets() {
    let module = keel_ir::parse(LOOP).unwrap();
    let pipeline = crate::CodegenPipeline {
        selector: &crate::SelectionTier::Full,
        allocator: &Forced,
    };
    for target in [
        crate::Target::Aarch64AppleDarwin,
        crate::Target::X86_64LinuxGnu,
    ] {
        let text = crate::compile_with_pipeline(&module, target, &pipeline).unwrap();
        let program = crate::Request::new(target, pipeline)
            .program(&module)
            .unwrap();
        assert_eq!(
            keel_asm::assemble_source_for(target, &text).unwrap(),
            keel_asm::assemble_program_for(target, &program).unwrap(),
            "{target:?}"
        );
    }
}
