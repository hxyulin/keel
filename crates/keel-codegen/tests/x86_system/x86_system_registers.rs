//! Compile/export only. Control/debug register access executes only in guests.
use crate::userspace;
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{
    SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, RegisterAllocator},
};
use keel_ir::{EntityRef, InstData, Intrinsic, MemoryEffects, Type, Value};
const SOURCE: &str =
    include_str!("../../../../suite/integration/x86-cpl0/cases/system_registers.keel");
const REGISTERS: [&str; 13] = [
    "cr0", "cr2", "cr3", "cr4", "cr8", "dr0", "dr1", "dr2", "dr3", "dr4", "dr5", "dr6", "dr7",
];
#[test]
fn system_register_registry_effects_and_foreign_rejection() {
    assert!(Intrinsic::ALL.len() >= 814);
    for (index, reg) in REGISTERS.iter().enumerate() {
        for (write, action) in [(false, "read"), (true, "write")] {
            let name = format!("x86_{action}_{reg}");
            let id = Intrinsic::from_name(&name).unwrap();
            assert_eq!(id as usize, 788 + index * 2 + usize::from(write));
            assert_eq!(Intrinsic::ALL[id as usize], id);
            assert_eq!(id.x86_system_register(), Some((*reg, write)));
            assert_eq!(id.info().params, if write { &[Type::I64][..] } else { &[] });
            assert_eq!(
                id.info().returns,
                if write { &[][..] } else { &[Type::I64] }
            );
            assert!(!id.info().pure && id.is_x86_target());
            assert_eq!(
                InstData::Intrinsic { id, args: vec![] }.memory_effects(),
                MemoryEffects::Barrier
            );
            let bad = if write {
                format!("func @bad(i32) {{\nentry(v0:i32):\n intrinsic {name}(v0)\n return\n}}")
            } else {
                format!(
                    "func @bad(i64) {{\nentry(v0:i64):\n v1 = intrinsic {name}(v0)\n return\n}}"
                )
            };
            if let Ok(m) = keel_ir::parse(&bad) {
                assert!(keel_ir::verify_module(&m).is_err(), "{bad}");
            }
        }
    }
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
}
#[test]
fn system_register_tiers_parity_liveness_and_export() {
    let dir = std::env::var_os("KEEL_SYSTEM_REGISTER_EXPORT_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &dir {
        std::fs::create_dir_all(dir).unwrap();
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for spill in [false, true] {
                    let (bytes, text) = userspace::compile(SOURCE, level, s, a, spill);
                    for reg in REGISTERS {
                        let count = text
                            .lines()
                            .filter(|l| {
                                l.split_whitespace().next() == Some("mov")
                                    && l.split([' ', '\t', ',']).any(|s| s == reg)
                            })
                            .count();
                        // Every unused third read must remain after optimization.
                        assert_eq!(count, if reg == "dr0" { 6 } else { 4 }, "{reg}: {text}");
                    }
                    if level == CpuLevel::V1
                        && let Some(dir) = &dir
                    {
                        let stem = format!("system-register-{s:?}-{a:?}-{spill}");
                        std::fs::write(dir.join(format!("{stem}.o")), bytes).unwrap();
                        std::fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                    }
                }
            }
        }
    }
}
struct ConstantRemat;
impl RegisterAllocator for ConstantRemat {
    fn name(&self) -> &'static str {
        "system-register-constant-remat"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut allocation = AllocationTier::Fast.allocate(input);
        if input.func.name == "system_constant" {
            let value = Value::new(0);
            allocation.locs[0][value.index()] = Some(Loc::Remat(value));
        }
        allocation
    }
}
#[test]
fn rematerialized_system_write_input_exports() {
    let (bytes, text) = userspace::compile_with_allocator(
        SOURCE,
        CpuLevel::V1,
        SelectionTier::Full,
        &ConstantRemat,
        true,
    );
    let body = text
        .split("\"system_constant\":")
        .nth(1)
        .unwrap()
        .split(".Lsystem_constant_end:")
        .next()
        .unwrap();
    assert!(body.contains("movabs\trax, 95075992133360"), "{body}");
    assert!(body.contains("mov\tdr0, rax"), "{body}");
    if let Some(dir) = std::env::var_os("KEEL_SYSTEM_REGISTER_EXPORT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("system-register-remat.o"), bytes).unwrap();
        std::fs::write(dir.join("system-register-remat.s"), text).unwrap();
    }
}
