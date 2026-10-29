#[path = "f80_allocation/f80_coalescing.rs"]
mod f80_coalescing;

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, RegisterAllocator, ValuePart},
};
use keel_ir::{EntityRef, Value};

#[derive(Clone, Copy, Debug)]
enum Plan {
    Plain,
    Param,
    Instruction,
    Before,
    After,
    Edge,
    Transit,
    Mixed,
    RegisterParam,
    RegisterInstruction,
    RegisterTransfer,
    RegisterFallback,
    InvalidCapacity,
}
impl Plan {
    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Param => "param",
            Self::Instruction => "instruction",
            Self::Before => "before",
            Self::After => "after",
            Self::Edge => "edge",
            Self::Transit => "transit",
            Self::Mixed => "mixed",
            Self::RegisterParam => "register-param",
            Self::RegisterInstruction => "register-instruction",
            Self::RegisterTransfer => "register-transfer",
            Self::RegisterFallback => "register-fallback",
            Self::InvalidCapacity => "invalid-capacity",
        }
    }
    fn source(self) -> &'static str {
        match self {
            Self::After | Self::Transit => {
                "func @identity(f80)->f80 {\nentry(v0:f80):\n v1 = fneg v0\n v2 = fneg v1\n return v2\n}\n"
            }
            Self::Mixed => {
                "func @identity(f80)->f80 {\n ss0 = stack_slot 8, align 8\nentry(v0:f80):\n v1 = iconst.i64 7\n v2 = stack_addr ss0\n store v1,v2\n v3 = load.i64 v2\n v4 = iconst.i64 7\n v5 = icmp.eq v3,v4\n v6 = fneg v0\n v7 = select v5,v0,v6\n return v7\n}\n"
            }
            Self::Edge => {
                "func @identity(f80)->f80 {\nentry(v0:f80):\n jump next\nnext:\n return v0\n}\n"
            }
            _ => "func @identity(f80)->f80 {\nentry(v0:f80):\n return v0\n}\n",
        }
    }
}
struct Custom(Plan);
impl RegisterAllocator for Custom {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        for i in 0..input.func.num_values() {
            a.locs[0][i] = Some(Loc::Stack(i as u32 * 2));
        }
        a.spill_slots = match self.0 {
            Plan::Plain
            | Plan::Param
            | Plan::Instruction
            | Plan::RegisterParam
            | Plan::InvalidCapacity => 2,
            Plan::Before => 4,
            Plan::After => 8,
            Plan::Mixed => 22,
            _ => 12,
        };
        let entry = input.func.entry_block().unwrap();
        let insts: Vec<_> = input.func.layout_insts().map(|(_, i)| i).collect();
        let last = *insts.last().unwrap();
        let v0 = ValuePart::low(Value::new(0));
        match self.0 {
            Plan::Plain => {}
            Plan::Param => a.edits.set_param(entry, v0, Loc::Stack(0)),
            Plan::Instruction => a.edits.instruction_mut(last).uses.push((v0, Loc::Stack(0))),
            Plan::Before => {
                a.edits.set_param(entry, v0, Loc::Stack(0));
                let edit = a.edits.instruction_mut(last);
                edit.uses.push((v0, Loc::Stack(2)));
                edit.before.push((Loc::Stack(2), Loc::Stack(0)));
            }
            Plan::After => {
                a.edits.set_param(entry, v0, Loc::Stack(0));
                let edit = a.edits.instruction_mut(insts[0]);
                edit.defs
                    .push((ValuePart::low(Value::new(1)), Loc::Stack(6)));
                edit.after.push((Loc::Stack(2), Loc::Stack(6)));
            }
            Plan::Edge => {
                a.edits.set_param(entry, v0, Loc::Stack(0));
                a.edits
                    .edge_mut(insts[0], 0)
                    .push((Loc::Stack(4), Loc::Stack(0)));
                a.edits.instruction_mut(last).uses.push((v0, Loc::Stack(4)));
            }
            Plan::Transit => {
                a.edits.set_param(entry, v0, Loc::Stack(0));
                a.edits
                    .instruction_mut(insts[0])
                    .after
                    .push((Loc::Stack(8), Loc::Stack(2)));
                let edit = a.edits.instruction_mut(insts[1]);
                edit.uses
                    .push((ValuePart::low(Value::new(1)), Loc::Stack(10)));
                edit.before.push((Loc::Stack(10), Loc::Stack(8)));
            }
            Plan::Mixed => {
                a.edits.set_param(entry, v0, Loc::Stack(0));
                // Full selection can cover these producers and fold the store's
                // address. A custom plan must only bind selected operands.
                let (mut def, mut value_use, mut ptr_use) = (false, false, false);
                input.for_each_operand(insts[0], |o| def |= o.part.value == Value::new(1));
                input.for_each_operand(insts[2], |o| {
                    value_use |= o.part.value == Value::new(1);
                    ptr_use |= o.part.value == Value::new(2);
                });
                if def && value_use && ptr_use {
                    let edit = a.edits.instruction_mut(insts[0]);
                    edit.defs
                        .push((ValuePart::low(Value::new(1)), Loc::Stack(16)));
                    edit.after.push((Loc::Stack(2), Loc::Stack(16)));
                    let edit = a.edits.instruction_mut(insts[2]);
                    edit.uses.extend([
                        (ValuePart::low(Value::new(1)), Loc::Stack(18)),
                        (ValuePart::low(Value::new(2)), Loc::Stack(20)),
                    ]);
                    edit.before.extend([
                        (Loc::Stack(18), Loc::Stack(2)),
                        (Loc::Stack(20), Loc::Stack(4)),
                    ]);
                }
            }
            Plan::RegisterParam => a.edits.set_param(entry, v0, Loc::FReg(0)),
            Plan::RegisterInstruction => {
                a.edits.instruction_mut(last).uses.push((v0, Loc::FReg(0)))
            }
            Plan::RegisterTransfer => {
                let edit = a.edits.instruction_mut(last);
                edit.uses.push((v0, Loc::Stack(4)));
                edit.before
                    .extend([(Loc::FReg(0), Loc::Stack(0)), (Loc::Stack(4), Loc::FReg(0))]);
            }
            Plan::RegisterFallback => {
                a.locs[0][0] = Some(Loc::FReg(0));
                let edit = a.edits.instruction_mut(last);
                edit.before.push((Loc::Stack(4), Loc::FReg(0)));
            }
            Plan::InvalidCapacity => a.edits.set_param(entry, v0, Loc::Stack(12)),
        }
        a
    }
}
fn compile(
    plan: Plan,
    selector: SelectionTier,
) -> Result<(String, Vec<u8>), keel_codegen::CodegenError> {
    let module = keel_ir::parse(plan.source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &Custom(plan),
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )?;
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
    Ok((text, object))
}
#[test]
fn redundant_sparse_param_preserves_the_initialized_home() {
    let (text, _) = compile(Plan::Param, SelectionTier::Baseline).unwrap();
    let store = text
        .lines()
        .find(|l| l.contains("mov\tqword ptr [rbp"))
        .unwrap();
    let load = text
        .lines()
        .find(|l| l.contains("fld\ttbyte ptr [rbp"))
        .unwrap();
    let memory = |line: &str| line[line.find('[').unwrap()..=line.find(']').unwrap()].to_owned();
    assert_eq!(memory(store), memory(load), "{text}");
}
#[test]
fn stack_only_sparse_bindings_and_transfers_preserve_full_payloads() {
    let export = std::env::var_os("KEEL_F80_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    for (selection, tier) in [
        ("baseline", SelectionTier::Baseline),
        ("full", SelectionTier::Full),
    ] {
        for plan in [
            Plan::Plain,
            Plan::Param,
            Plan::Instruction,
            Plan::Before,
            Plan::After,
            Plan::Edge,
            Plan::Transit,
            Plan::Mixed,
        ] {
            let name = format!("custom-{selection}-{}", plan.name());
            let (text, object) = compile(plan, tier).unwrap_or_else(|e| panic!("{name}: {e}"));
            if matches!(
                plan,
                Plan::Before | Plan::After | Plan::Edge | Plan::Transit
            ) {
                assert!(
                    text.contains("movups"),
                    "full-payload transfer missing: {name}\n{text}"
                );
                assert!(
                    text.contains("xmmword ptr"),
                    "128-bit home metadata missing: {name}\n{text}"
                );
            }
            if let Some(dir) = &export {
                std::fs::write(dir.join(format!("{name}.typed.o")), &object).unwrap();
                std::fs::write(
                    dir.join(format!("{name}.text.o")),
                    keel_asm::x86::assemble_object(&text).unwrap(),
                )
                .unwrap();
                std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
            }
        }
    }
}
#[test]
fn unsupported_register_transitions_fail_closed() {
    for plan in [
        Plan::RegisterParam,
        Plan::RegisterInstruction,
        Plan::RegisterTransfer,
        Plan::RegisterFallback,
    ] {
        let error = compile(plan, SelectionTier::Baseline).unwrap_err();
        assert!(
            error.message.contains("f80 sparse")
                || error.message.contains("f80 register fallbacks"),
            "{plan:?}: {error}"
        );
    }
}
#[test]
fn invalid_sparse_capacity_is_rejected_before_normalization() {
    let error = compile(Plan::InvalidCapacity, SelectionTier::Baseline).unwrap_err();
    assert!(
        error.message.contains("invalid block parameter location"),
        "{error}"
    );
}
