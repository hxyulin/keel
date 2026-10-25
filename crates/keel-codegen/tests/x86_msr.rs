//! Privileged compile/export tests. Target code executes only in a CPL0 guest.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, RegisterAllocator},
};
use keel_ir::{EntityRef, InstData, Intrinsic, MemoryEffects, Type, Value};
use object::{Object, ObjectSection, ObjectSymbol};

#[path = "common/userspace.rs"]
mod userspace;

const SOURCE: &str = include_str!("../../../suite/x86-cpl0/cases/msr.keel");

#[test]
fn signatures_effects_and_foreign_target_rejection() {
    for (id, name, params, returns, number) in [
        (
            Intrinsic::X86Rdmsr,
            "x86_rdmsr",
            &[Type::I32][..],
            &[Type::I64][..],
            776,
        ),
        (
            Intrinsic::X86Wrmsr,
            "x86_wrmsr",
            &[Type::I32, Type::I64][..],
            &[][..],
            777,
        ),
    ] {
        let info = id.info();
        assert_eq!(info.name, name);
        assert_eq!(info.params, params);
        assert_eq!(info.returns, returns);
        assert!(!info.pure);
        assert!(id.is_x86_target());
        assert_eq!(id as usize, number);
        assert_eq!(Intrinsic::ALL[number], id);
        let inst = InstData::Intrinsic {
            id,
            args: (0..params.len()).map(Value::new).collect(),
        };
        assert!(inst.is_pinned());
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
    }
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(keel_codegen::aarch64::compile(&module).is_err());
    for source in [
        "func @f(i64)->i64 {\nentry(v0:i64):\n v1 = intrinsic x86_rdmsr(v0)\n return v1\n}",
        "func @f(i16)->i64 {\nentry(v0:i16):\n v1 = intrinsic x86_rdmsr(v0)\n return v1\n}",
        "func @f(i32,i32) {\nentry(v0:i32,v1:i32):\n intrinsic x86_wrmsr(v0,v1)\n return\n}",
        "func @f(i32) {\nentry(v0:i32):\n intrinsic x86_wrmsr(v0)\n return\n}",
    ] {
        if let Ok(module) = keel_ir::parse(source) {
            assert!(keel_ir::verify_module(&module).is_err(), "{source}");
        }
    }
}

#[test]
fn msr_payload_parity_liveness_and_pinned_unused_reads() {
    let export = std::env::var_os("KEEL_MSR_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for spilled in [false, true] {
                    let (bytes, text) =
                        userspace::compile(SOURCE, level, selection, allocation, spilled);
                    for (op, count) in [("rdmsr", 4), ("wrmsr", 2)] {
                        assert_eq!(
                            text.lines().filter(|line| line.trim() == op).count(),
                            count,
                            "{text}"
                        );
                    }
                    let file = object::File::parse(&*bytes).unwrap();
                    assert!(
                        file.symbols()
                            .any(|s| s.name() == Ok("msr_observe") && s.is_definition())
                    );
                    assert!(
                        file.symbols()
                            .all(|s| !s.is_undefined() || s.name().unwrap().is_empty())
                    );
                    if level == CpuLevel::V1 {
                        if let Some(dir) = &export {
                            let stem = format!("msr-{selection:?}-{allocation:?}-{spilled}");
                            std::fs::write(dir.join(format!("{stem}.o")), &bytes).unwrap();
                            std::fs::write(dir.join(format!("{stem}.s")), &text).unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn msr_instructions_use_the_existing_literal_encodings() {
    for (op, expected) in [("rdmsr", [0x0f, 0x32]), ("wrmsr", [0x0f, 0x30])] {
        let bytes = keel_asm::x86::assemble_object_with_features(op, Features::NONE).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        assert_eq!(
            file.section_by_name(".text").unwrap().data().unwrap(),
            expected
        );
    }
}

// Force valid immutable locations without depending on an experimental
// allocator profile. Rematerializing the value into RAX must preserve the
// index already captured in ECX.
struct ConstantRemat;
impl RegisterAllocator for ConstantRemat {
    fn name(&self) -> &'static str {
        "msr-constant-remat"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut allocation = AllocationTier::Fast.allocate(input);
        if input.func.name == "msr_constant" {
            for index in [0, 1] {
                let value = Value::new(index);
                allocation.locs[0][index] = Some(Loc::Remat(value));
            }
        }
        allocation
    }
}

#[test]
fn forced_rematerialized_msr_inputs_have_parity_and_export() {
    let (bytes, text) = userspace::compile_with_allocator(
        SOURCE,
        CpuLevel::V1,
        SelectionTier::Full,
        &ConstantRemat,
        true,
    );
    let body = text
        .split("\"msr_constant\":")
        .nth(1)
        .unwrap()
        .split(".Lmsr_constant_end:")
        .next()
        .unwrap();
    assert!(body.contains("mov\tecx, 3221225730"), "{body}");
    assert!(body.contains("movabs\trax, 95075992133360"), "{body}");
    assert!(!body.contains("mov\trcx, qword ptr"), "{body}");
    if let Some(dir) = std::env::var_os("KEEL_MSR_EXPORT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("msr-remat.o"), bytes).unwrap();
        std::fs::write(dir.join("msr-remat.s"), text).unwrap();
    }
}
