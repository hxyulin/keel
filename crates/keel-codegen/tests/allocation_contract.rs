//! Allocation boundary constraints must be enforced before either emitter runs.
use keel_codegen::regalloc::{
    Allocation, AllocationInput, AllocationTier, Constraint, Loc, OperandKind, RegisterAllocator,
    ValuePart,
};
use keel_codegen::select::{Selection, Target as SelectionTarget};
use keel_codegen::{CodegenPipeline, InstructionSelector, SelectionTier, Target};
use keel_ir::{EntityRef, Function, InstData, Value};

const SOURCE: &str = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iadd v0, v1
 return v2
}";

struct Constrained;
impl InstructionSelector for Constrained {
    fn name(&self) -> &'static str {
        "test-constraints"
    }
    fn select(
        &self,
        func: &Function,
        target: SelectionTarget,
        folds: &dyn Fn(&InstData) -> Vec<Value>,
    ) -> Selection {
        let mut selection = SelectionTier::Baseline.select(func, target, folds);
        let regs = match target {
            SelectionTarget::Aarch64 => keel_codegen::aarch64::REGISTERS,
            SelectionTarget::X86 => keel_codegen::x86_64::REGISTERS,
        };
        let inst = func.block_insts(func.entry_block().unwrap())[0];
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(0)),
            OperandKind::Use,
            Constraint::Fixed(Loc::Reg(regs.caller_saved[0])),
        );
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(1)),
            OperandKind::Use,
            Constraint::Register,
        );
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(2)),
            OperandKind::Def,
            Constraint::Reuse(ValuePart::low(Value::new(0))),
        );
        selection
    }
}
struct Allocator {
    invalid: bool,
}
impl RegisterAllocator for Allocator {
    fn name(&self) -> &'static str {
        "test-constraints"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        for (i, loc) in alloc.locs[0].iter_mut().enumerate() {
            *loc = Some(Loc::Stack(i as u32));
        }
        alloc.spill_slots = input.func.num_values() as u32;
        let inst = input.func.block_insts(input.func.entry_block().unwrap())[0];
        let a = Loc::Reg(input.pools.caller_saved[0]);
        let b = Loc::Reg(input.pools.caller_saved[1]);
        let edit = alloc.edits.instruction_mut(inst);
        edit.uses = vec![
            (ValuePart::low(Value::new(0)), a),
            (ValuePart::low(Value::new(1)), b),
        ];
        edit.defs = vec![(
            ValuePart::low(Value::new(2)),
            if self.invalid { b } else { a },
        )];
        edit.before = vec![(a, Loc::Stack(0)), (b, Loc::Stack(1))];
        edit.after = vec![(Loc::Stack(2), a)];
        alloc
    }
}
#[test]
fn fixed_and_reused_operand_locations_reach_both_emitters() {
    let module = keel_ir::parse(SOURCE).unwrap();
    for target in Target::ALL {
        let assembly = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &Constrained,
                allocator: &Allocator { invalid: false },
            },
        )
        .unwrap();
        assert!(!assembly.is_empty());
        let error = keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &Constrained,
                allocator: &Allocator { invalid: true },
            },
        )
        .unwrap_err();
        assert!(
            error.message.contains("unsatisfied allocation constraint"),
            "{error}"
        );
        if Some(target) == Target::host() {
            let dir =
                std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("allocation-constraints");
            std::fs::create_dir_all(&dir).unwrap();
            let asm = dir.join("f.s");
            let driver = dir.join("driver.c");
            let exe = dir.join("run");
            std::fs::write(&asm, assembly).unwrap();
            std::fs::write(
                &driver,
                "extern long f(long,long); int main(void) { return f(17,25) != 42; }",
            )
            .unwrap();
            let output = std::process::Command::new("clang")
                .args([&asm, &driver])
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(std::process::Command::new(&exe).status().unwrap().success());
        }
    }
}
