use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, RegisterAllocator, ValuePart},
};
use keel_ir::{EntityRef, Value};

struct BinarySecond;
impl RegisterAllocator for BinarySecond {
    fn name(&self) -> &'static str {
        "f80-binary-second-home"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        a.spill_slots = 4;
        for (i, slot) in [0, 2, 2].into_iter().enumerate() {
            a.locs[0][i] = Some(Loc::Stack(slot));
        }
        a.edits.set_param(
            input.func.entry_block().unwrap(),
            ValuePart::low(Value::new(0)),
            Loc::Stack(0),
        );
        a
    }
}
struct ScalarReuse;
impl RegisterAllocator for ScalarReuse {
    fn name(&self) -> &'static str {
        "f80-i64-temporal-home"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        a.spill_slots = 2;
        a.locs[0][0] = Some(Loc::Stack(0));
        a.locs[0][1] = Some(Loc::Stack(0));
        a.edits.set_param(
            input.func.entry_block().unwrap(),
            ValuePart::low(Value::new(0)),
            Loc::Stack(0),
        );
        a
    }
}
#[test]
fn second_sign_is_read_before_coalesced_binary_result_is_written() {
    let mut failures = Vec::new();
    for op in ["fcopysign", "fmin", "fmax"] {
        let module=keel_ir::parse(&format!("func @witness(f80,f80)->f80 {{\nentry(v0:f80,v1:f80):\n v2 = {op} v0,v1\n return v2\n}}\n")).unwrap();
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &BinarySecond,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &pipeline,
        )
        .unwrap();
        let lines: Vec<_> = text.lines().collect();
        let fld = lines
            .iter()
            .rev()
            .find(|l| l.contains("fld\ttbyte ptr [rbp + "))
            .unwrap();
        let offset: i64 = fld
            .split("[rbp + ")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let at = if op == "fcopysign" {
            lines
                .iter()
                .position(|l| l.contains("# v2 = fcopysign"))
                .unwrap()
        } else {
            lines
                .iter()
                .position(|l| l.contains("_f80_minmax_equal") && l.ends_with(':'))
                .unwrap()
        };
        let body = &lines[at..];
        let write = body
            .iter()
            .position(|l| l.contains(&format!("mov\tqword ptr [rbp + {offset}],")))
            .unwrap();
        let read = body
            .iter()
            .position(|l| {
                l.contains("movzx") && l.contains(&format!("word ptr [rbp + {}]", offset + 8))
            })
            .unwrap();
        if read >= write {
            failures.push(format!(
                "{op}: sign read {read} follows result write {write}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
#[test]
fn scalar_result_reusing_expired_f80_home_assembles_both_transports() {
    let module = keel_ir::parse(
        "func @witness(f80)->i64 {\nentry(v0:f80):\n v1 = fptosi.i64 v0\n return v1\n}\n",
    )
    .unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Baseline,
        allocator: &ScalarReuse,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let a = keel_asm::x86::assemble_object(&text);
    let b = keel_asm::x86::assemble_program_object(&program);
    assert!(a.is_ok() && b.is_ok(), "text={a:?}, typed={b:?}\n{text}");
}

struct Coalesced {
    second: bool,
}
impl RegisterAllocator for Coalesced {
    fn name(&self) -> &'static str {
        "f80-coalesced-operands-and-types"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        for loc in &mut a.locs[0] {
            *loc = Some(Loc::Stack(0));
        }
        a.spill_slots = 8;
        let f = input.func;
        let v0 = ValuePart::low(Value::new(0));
        a.edits
            .set_param(f.entry_block().unwrap(), v0, Loc::Stack(0));
        if f.sig.params.len() == 2 {
            a.locs[0][1] = Some(Loc::Stack(2));
            if f.sig.returns[0].ty == keel_ir::Type::F80 {
                a.locs[0][2] = Some(Loc::Stack(if self.second { 2 } else { 0 }));
            }
        }
        if f.name.ends_with("_component") {
            // F80 expires into a scalar result, which is then copied through
            // a component conservatively marked for 128-bit F80 transfers.
            a.locs[0][2] = Some(Loc::Stack(4));
            let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
            let v1 = ValuePart::low(Value::new(1));
            let first = a.edits.instruction_mut(insts[0]);
            first.defs.push((v1, Loc::Stack(2)));
            first.after.push((Loc::Stack(0), Loc::Stack(2)));
            let use_inst = a.edits.instruction_mut(insts[2]);
            use_inst.uses.push((v1, Loc::Stack(6)));
            use_inst.before.push((Loc::Stack(6), Loc::Stack(0)));
        }
        a
    }
}
#[test]
fn coalesced_binary_and_temporal_homes_export_both_transports() {
    use keel_asm_ir::x86::{CpuLevel, Features};
    use keel_codegen::x86_features::FeatureSelector;
    let export = std::env::var_os("KEEL_F80_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    let module = keel_ir::parse(include_str!("f80/coalescing.keel")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for (cpu, level) in [("v1", CpuLevel::V1), ("v3", CpuLevel::V3)] {
        let features = Features::for_level(level);
        for (selection, tier) in [
            ("baseline", SelectionTier::Baseline),
            ("full", SelectionTier::Full),
        ] {
            for (operand, second) in [("first", false), ("second", true)] {
                let name = format!("coalesced-{cpu}-{selection}-{operand}");
                let selector = FeatureSelector {
                    inner: &tier,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &Coalesced { second },
                };
                let text = keel_codegen::x86_64::compile_with_pipeline(
                    &module,
                    &keel_codegen::x86_64::REGISTERS,
                    &pipeline,
                )
                .unwrap_or_else(|e| panic!("{name}: {e}"));
                let program = keel_codegen::x86_64::compile_program_with_pipeline(
                    &module,
                    &keel_codegen::x86_64::REGISTERS,
                    &pipeline,
                )
                .unwrap();
                let typed =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap_or_else(|e| panic!("{name}: {e}"));
                let parsed = keel_asm::x86::assemble_object_with_features(&text, features)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(typed, parsed, "{name}");
                if let Some(dir) = &export {
                    std::fs::write(dir.join(format!("{name}.typed.o")), typed).unwrap();
                    std::fs::write(dir.join(format!("{name}.text.o")), parsed).unwrap();
                    std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
