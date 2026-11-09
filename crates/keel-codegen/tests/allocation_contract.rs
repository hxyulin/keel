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
    for target in Target::NATIVE {
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

#[test]
fn spilled_edge_values_reuse_free_homes_but_keep_live_inputs_distinct() {
    use keel_codegen::regalloc::Pools;
    use object::{Object, ObjectSection};
    use std::{cell::RefCell, collections::BTreeMap};

    struct Observed {
        tier: AllocationTier,
        homes: RefCell<BTreeMap<String, Vec<Option<Loc>>>>,
    }
    impl RegisterAllocator for Observed {
        fn name(&self) -> &'static str {
            "spill-affinity-oracle"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let allocation = self.tier.allocate(input);
            self.homes
                .borrow_mut()
                .insert(input.func.name.clone(), allocation.locs[0].clone());
            allocation
        }
    }
    let module = keel_ir::parse(
        "func @choose(i32, i64, i64) -> i64 {
block0(v0:i32, v1:i64, v2:i64):
 brif v0, block1(v1,v2), block2(v1,v2)
block1(v3:i64,v4:i64):
 v5 = iadd v3,v4
 return v5
block2(v6:i64,v7:i64):
 v8 = isub v6,v7
 return v8
}
func @overlap(i32, i64, i64) -> i64 {
block0(v0:i32,v1:i64,v2:i64):
 brif v0, block1(v1), block1(v2)
block1(v3:i64):
 v4 = xor v3,v1
 return v4
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let dir = keel_test_support::TempDir::new("spill-affinity-oracle");
    std::fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
uint64_t choose(int, uint64_t, uint64_t), overlap(int, uint64_t, uint64_t);
int main(void) {
    const uint64_t values[] = {0,1,17,UINT64_MAX,UINT64_C(1)<<63};
    for (unsigned i=0;i<5;++i) for (unsigned j=0;j<5;++j) for (int route=0;route<2;++route) {
        uint64_t a=values[i], b=values[j];
        if (choose(route,a,b)!=(route?a+b:a-b)) return 1;
        if (overlap(route,a,b)!=(route?0:b^a)) return 2;
    }
    return 0;
}
"#,
    )
    .unwrap();
    for target in Target::NATIVE {
        let mut sizes = Vec::new();
        for tier in [
            AllocationTier::Full,
            AllocationTier::Compact,
            AllocationTier::Bounded,
        ] {
            let observed = Observed {
                tier,
                homes: RefCell::new(BTreeMap::new()),
            };
            let request = keel_codegen::Request::new(
                target,
                CodegenPipeline {
                    selector: &SelectionTier::Full,
                    allocator: &observed,
                },
            )
            .pools(&empty);
            let text = request.text(&module).unwrap();
            let bytes = keel_asm::assemble_source_for(target, &text).unwrap();
            assert_eq!(
                bytes,
                keel_asm::assemble_program_for(target, &request.program(&module).unwrap()).unwrap()
            );
            if tier == AllocationTier::Compact {
                let homes = observed.homes.borrow();
                let choose = &homes["choose"];
                for (param, arg) in [(3, 1), (4, 2), (6, 1), (7, 2)] {
                    assert_eq!(
                        choose[param], choose[arg],
                        "{target:?}/{tier:?}: {choose:?}"
                    );
                }
                let overlap = &homes["overlap"];
                assert_ne!(overlap[3], overlap[1], "live input lost: {overlap:?}");
            }
            let object = object::File::parse(bytes.as_slice()).unwrap();
            sizes.push(
                object
                    .section_by_name(if target == Target::Aarch64AppleDarwin {
                        "__text"
                    } else {
                        ".text"
                    })
                    .unwrap()
                    .size(),
            );
            if Some(target) == Target::host() {
                std::fs::write(dir.join("generated.o"), bytes).unwrap();
                let link = std::process::Command::new("clang")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("generated.o"))
                    .arg("-o")
                    .arg(dir.join("oracle"))
                    .output()
                    .unwrap();
                assert!(
                    link.status.success(),
                    "{}",
                    String::from_utf8_lossy(&link.stderr)
                );
                let run = std::process::Command::new(dir.join("oracle"))
                    .output()
                    .unwrap();
                assert!(run.status.success(), "{target:?}/{tier:?}: {run:?}");
            }
        }
        assert!(sizes[1] < sizes[0], "{target:?}: {sizes:?}");
        assert_eq!(sizes[0], sizes[2], "{target:?}: {sizes:?}");
    }
}
