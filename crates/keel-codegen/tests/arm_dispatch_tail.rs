//! Experimental allocated tails preserve SSA homes, edge copies and bounds.
use keel_codegen::{
    CodegenPipeline, ExperimentalDispatchTails, SelectionTier,
    regalloc::{
        Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator, ValuePart,
    },
};
use keel_ir::{EntityRef, InstData};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source(bits: u32) -> String {
    format!(
        "func @f{bits}(i{bits}, i32, i64, i64) -> i64 {{
block0(v0: i{bits}, v1: i32, v2: i64, v3: i64):
 brif v1, block1, block2
block1:
 jump block3(v0, v2, v3)
block2:
 jump block3(v0, v3, v2)
block3(v4: i{bits}, v5: i64, v6: i64):
 switch v4, block4, [10: block5(v5, v6), 11: block6, 12: block5(v6, v5), 14: block7]
block4:
 trap
block5(v7: i64, v8: i64):
 v9 = isub v7, v8
 v10 = iadd v9, v5
 return v10
block6:
 v11 = iadd v5, v6
 return v11
block7:
 v12 = isub v5, v6
 return v12
}}\n"
    )
}

fn mixed_source(bits: u32) -> String {
    source(bits)
        .replace(&format!("@f{bits}("), &format!("@mixed{bits}("))
        .replace(
            "jump block3(v0, v2, v3)",
            "brif v1, block3(v0, v2, v3), block3(v0, v3, v2)",
        )
}

fn compile(
    module: &keel_ir::Module,
    selector: &dyn keel_codegen::InstructionSelector,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    keel_ir::verify_module(module).unwrap();
    let pipeline = CodegenPipeline {
        selector,
        allocator,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
    if !module.functions.iter().any(|f| {
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }))
    }) {
        assert_eq!(program.stats().text_bytes, 0);
    }
    let object =
        keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap(), "{text}");
    let labels: Vec<_> = text.lines().filter(|l| l.ends_with(':')).collect();
    let unique: std::collections::HashSet<_> = labels.iter().collect();
    assert_eq!(labels.len(), unique.len(), "duplicate assembly labels");
    (text, object)
}

fn tails(text: &str) -> usize {
    text.lines()
        .filter(|l| l.ends_with("_table:") && l.contains("_tail"))
        .count()
}

#[test]
fn opt_in_replicates_two_tables_with_phi_edges_and_downstream_parameter_uses() {
    for bits in [32, 64] {
        let module = keel_ir::parse(&source(bits)).unwrap();
        for tier in [SelectionTier::Baseline, SelectionTier::Full] {
            for pools in [&SPILLED, &keel_codegen::aarch64::REGISTERS] {
                let (normal, _) = compile(&module, &tier, &AllocationTier::Full, pools);
                let (opt, _) = compile(
                    &module,
                    &ExperimentalDispatchTails(&tier),
                    &AllocationTier::Full,
                    pools,
                );
                assert_eq!(tails(&normal), 0);
                assert_eq!(tails(&opt), 2, "{opt}");
                assert_eq!(
                    opt.matches("_table:\n").count(),
                    3,
                    "original switch retained"
                );
                assert_eq!(opt.matches("\tb.hi\t").count(), 3, "bounds retained");
                assert!(opt.contains("brk\t#1"));
                let (fast, _) = compile(&module, &tier, &AllocationTier::Fast, pools);
                let (fast_opt, _) = compile(
                    &module,
                    &ExperimentalDispatchTails(&tier),
                    &AllocationTier::Fast,
                    pools,
                );
                assert_eq!(fast, fast_opt);
            }
        }
    }
}

/// Every value has a distinct spill home. Dispatch parameters arrive in
/// reversed homes; the Switch's before edit restores them with a real cycle.
/// Its key has an instruction-specific register location. The Switch edge
/// transfer and ordinary phi copy form one cycle on the 10 edge.
struct Edited;
impl RegisterAllocator for Edited {
    fn name(&self) -> &'static str {
        "dispatch-test-edits"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        for (i, loc) in alloc.locs[0].iter_mut().enumerate() {
            *loc = Some(Loc::Stack(i as u32));
        }
        alloc.spill_slots = input.func.num_values() as u32;
        let dispatch = input.func.blocks().find(|&b| matches!(input.func.block_insts(b), [i] if matches!(input.func.inst_data(*i), InstData::Switch { .. }))).unwrap();
        let params = input.func.block_params(dispatch);
        let (a, b) = (params[1], params[2]);
        let (la, lb) = (Loc::Stack(a.index() as u32), Loc::Stack(b.index() as u32));
        alloc.edits.set_param(dispatch, ValuePart::low(a), lb);
        alloc.edits.set_param(dispatch, ValuePart::low(b), la);
        let switch = input.func.block_insts(dispatch)[0];
        let edit = alloc.edits.instruction_mut(switch);
        edit.before = vec![
            (la, lb),
            (lb, la),
            (Loc::Reg(9), Loc::Stack(params[0].index() as u32)),
        ];
        edit.uses.push((ValuePart::low(params[0]), Loc::Reg(9)));
        // Set block5's phi destinations to the opposite dispatch homes.
        // On key 10, phi copies are lb <- la and la <- lb; an edge edit
        // simultaneously saves v5 for the direct use in the following add.
        let InstData::Switch { dests, .. } = input.func.inst_data(switch) else {
            unreachable!()
        };
        let body = dests[1].block;
        let body_params = input.func.block_params(body);
        let saved_a = Loc::Stack(body_params[0].index() as u32);
        alloc
            .edits
            .set_param(body, ValuePart::low(body_params[0]), lb);
        alloc
            .edits
            .set_param(body, ValuePart::low(body_params[1]), la);
        for edge in [1, 3] {
            alloc.edits.edge_mut(switch, edge).push((saved_a, la));
        }
        let body_insts = input.func.block_insts(body);
        alloc.edits.instruction_mut(body_insts[0]).uses = vec![
            (ValuePart::low(body_params[0]), lb),
            (ValuePart::low(body_params[1]), la),
        ];
        alloc.edits.instruction_mut(body_insts[1]).uses = vec![(ValuePart::low(a), saved_a)];
        alloc
    }
}

#[test]
fn before_operand_and_edge_allocation_edits_are_replayed() {
    for bits in [32, 64] {
        let module = keel_ir::parse(&source(bits)).unwrap();
        let (text, _) = compile(
            &module,
            &ExperimentalDispatchTails(&SelectionTier::Full),
            &Edited,
            &keel_codegen::aarch64::REGISTERS,
        );
        assert_eq!(tails(&text), 2);
    }
}

#[test]
fn ineligible_sparse_small_and_debug_tails_keep_original_emission() {
    let selector = ExperimentalDispatchTails(&SelectionTier::Full);
    for src in [
        source(32).replace("14: block7", "1000: block7"),
        source(32).replace(", 14: block7", ""),
        source(32).replace(" switch v4", " v13 = iconst.i32 1\n switch v4"),
        source(32).replace(
            " brif v1",
            " v13 = asm \"mov %w0, #1\", \"=r\"() -> i32\n brif v1",
        ),
    ] {
        let module = keel_ir::parse(&src).unwrap();
        let (normal, _) = compile(
            &module,
            &SelectionTier::Full,
            &AllocationTier::Full,
            &SPILLED,
        );
        let (opt, _) = compile(&module, &selector, &AllocationTier::Full, &SPILLED);
        assert_eq!(normal, opt);
    }
    let mut module = keel_ir::parse(&source(32)).unwrap();
    module.files.push("dispatch.c".into());
    let (normal, _) = compile(
        &module,
        &SelectionTier::Full,
        &AllocationTier::Full,
        &SPILLED,
    );
    let (opt, _) = compile(&module, &selector, &AllocationTier::Full, &SPILLED);
    assert_eq!(normal, opt);
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_all_cases_and_default_traps_match_independent_oracle() {
    use std::{fs, process::Command};
    let module =
        keel_ir::parse(&(source(32) + &source(64) + &mixed_source(32) + &mixed_source(64)))
            .unwrap();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("dispatch-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("fixtures/arm_dispatch_tail.c"),
    )
    .unwrap();
    for (allocator, pools) in [
        (&AllocationTier::Full as &dyn RegisterAllocator, &SPILLED),
        (&AllocationTier::Full, &keel_codegen::aarch64::REGISTERS),
        (&Edited, &keel_codegen::aarch64::REGISTERS),
    ] {
        for selector in [
            &SelectionTier::Full as &dyn keel_codegen::InstructionSelector,
            &ExperimentalDispatchTails(&SelectionTier::Full),
        ] {
            let (_, object) = compile(&module, selector, allocator, pools);
            fs::write(dir.join("test.o"), object).unwrap();
            let output = Command::new("clang")
                .arg("-O2")
                .arg(dir.join("oracle.c"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(dir.join("test")).status().unwrap().success());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

fn fanout(sites: usize, cases: usize, stride: usize) -> String {
    let dispatch = sites + 1;
    let mut src = format!("func @fanout(i32) -> i32 {{\nblock0(v0: i32):\n switch v0, block1, [");
    for i in 0..sites {
        if i != 0 {
            src += ", ";
        }
        src += &format!("{i}: block{}", i + 1);
    }
    src += "]\n";
    for i in 1..=sites {
        src += &format!("block{i}:\n jump block{dispatch}(v0)\n");
    }
    src += &format!(
        "block{dispatch}(v1: i32):\n switch v1, block{}, [",
        dispatch + 1
    );
    for i in 0..cases {
        if i != 0 {
            src += ", ";
        }
        src += &format!("{}: block{}", i * stride, dispatch + 2);
    }
    src += &format!(
        "]\nblock{}:\n trap\nblock{}:\n return v1\n}}",
        dispatch + 1,
        dispatch + 2
    );
    src
}

#[test]
fn clone_and_table_entry_budgets_and_case_count_limits_are_fixed() {
    for (sites, cases, stride, expected) in [
        (10, 4, 1, 8),
        (10, 128, 3, 2),
        (2, 128, 1, 2),
        (2, 129, 1, 0),
    ] {
        let module = keel_ir::parse(&fanout(sites, cases, stride)).unwrap();
        let (text, _) = compile(
            &module,
            &ExperimentalDispatchTails(&SelectionTier::Full),
            &AllocationTier::Full,
            &SPILLED,
        );
        assert_eq!(
            tails(&text),
            expected,
            "sites={sites}, cases={cases}, stride={stride}"
        );
    }
}

struct Constrained;
impl keel_codegen::InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "dispatch-test-constraint"
    }
    fn select(
        &self,
        func: &keel_ir::Function,
        target: keel_codegen::select::Target,
        folds: &dyn Fn(&InstData) -> Vec<keel_ir::Value>,
    ) -> keel_codegen::select::Selection {
        let mut selection =
            keel_codegen::InstructionSelector::select(&SelectionTier::Full, func, target, folds);
        let (_, switch) = func
            .layout_insts()
            .find(|(_, i)| matches!(func.inst_data(*i), InstData::Switch { .. }))
            .unwrap();
        let InstData::Switch { arg, .. } = func.inst_data(switch) else {
            unreachable!()
        };
        selection.set_constraint(
            switch,
            ValuePart::low(*arg),
            keel_codegen::regalloc::OperandKind::Use,
            keel_codegen::regalloc::Constraint::Any,
        );
        selection
    }
}

#[test]
fn custom_constraints_disable_the_experiment() {
    let module = keel_ir::parse(&source(64)).unwrap();
    let (normal, _) = compile(&module, &Constrained, &AllocationTier::Full, &SPILLED);
    let (opt, _) = compile(
        &module,
        &ExperimentalDispatchTails(&Constrained),
        &AllocationTier::Full,
        &SPILLED,
    );
    assert_eq!(normal, opt);
}

#[test]
fn nonjump_edges_still_enter_the_original_switch() {
    for bits in [32, 64] {
        let module = keel_ir::parse(&mixed_source(bits)).unwrap();
        let (text, _) = compile(
            &module,
            &ExperimentalDispatchTails(&SelectionTier::Full),
            &AllocationTier::Full,
            &SPILLED,
        );
        assert_eq!(tails(&text), 1);
        assert_eq!(text.matches("_table:\n").count(), 2);
    }
}

struct UnsupportedAfter;
impl RegisterAllocator for UnsupportedAfter {
    fn name(&self) -> &'static str {
        "dispatch-test-unsupported"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        let (_, switch) = input
            .func
            .layout_insts()
            .find(|(_, i)| matches!(input.func.inst_data(*i), InstData::Switch { .. }))
            .unwrap();
        alloc
            .edits
            .instruction_mut(switch)
            .after
            .push((Loc::Reg(9), Loc::Reg(9)));
        alloc
    }
}

#[test]
fn unsupported_terminator_after_event_is_rejected_before_emission() {
    let module = keel_ir::parse(&source(64)).unwrap();
    for selector in [
        &SelectionTier::Full as &dyn keel_codegen::InstructionSelector,
        &ExperimentalDispatchTails(&SelectionTier::Full),
    ] {
        let error = keel_codegen::aarch64::compile_with_pipeline(
            &module,
            &keel_codegen::aarch64::REGISTERS,
            &CodegenPipeline {
                selector,
                allocator: &UnsupportedAfter,
            },
        )
        .unwrap_err();
        assert!(
            error
                .message
                .contains("terminator transfers must be placed on successor edges"),
            "{error}"
        );
    }
}
