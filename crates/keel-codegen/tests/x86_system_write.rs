//! Compile/export only: CPL0 instructions never execute on the host.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use object::{Object, ObjectSymbol};
#[path = "common/userspace.rs"]
mod userspace;
const LOADS: &str = include_str!("../../../suite/integration/x86-cpl0/cases/system_load.keel");
const INVALIDATE: &str = include_str!("../../../suite/integration/x86-cpl0/cases/invlpg.keel");
#[test]
fn system_write_signatures_effects_and_foreign_rejection() {
    for (n, id, name, ty) in [
        (781, Intrinsic::X86Lldt, "x86_lldt", Type::I16),
        (782, Intrinsic::X86LldtMem, "x86_lldt_mem", Type::Ptr),
        (783, Intrinsic::X86Ltr, "x86_ltr", Type::I16),
        (784, Intrinsic::X86LtrMem, "x86_ltr_mem", Type::Ptr),
        (785, Intrinsic::X86Lmsw, "x86_lmsw", Type::I16),
        (786, Intrinsic::X86LmswMem, "x86_lmsw_mem", Type::Ptr),
        (787, Intrinsic::X86Invlpg, "x86_invlpg", Type::Ptr),
    ] {
        assert_eq!(id as usize, n);
        assert_eq!(Intrinsic::ALL[n], id);
        assert_eq!(id.info().name, name);
        assert_eq!(id.info().params, &[ty]);
        assert!(id.info().returns.is_empty() && !id.info().pure && id.is_x86_target());
        assert!(id.x86_system_write().is_some());
        assert_eq!(
            InstData::Intrinsic { id, args: vec![] }.memory_effects(),
            MemoryEffects::Barrier
        );
        let bad = format!("func @f(i32) {{\nentry(v0:i32):\n intrinsic {name}(v0)\n return\n}}");
        if let Ok(m) = keel_ir::parse(&bad) {
            assert!(keel_ir::verify_module(&m).is_err(), "{bad}");
        }
    }
    for source in [LOADS, INVALIDATE] {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(keel_codegen::aarch64::compile(&m).is_err());
    }
}
fn export(source: &str, prefix: &str, variable: &str, symbols: &[&str], ops: &[(&str, usize)]) {
    let dir = std::env::var_os(variable).map(std::path::PathBuf::from);
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
                    let (bytes, text) = userspace::compile(source, level, s, a, spill);
                    for &(op, count) in ops {
                        assert_eq!(
                            text.lines()
                                .filter(|l| l.split_whitespace().next() == Some(op))
                                .count(),
                            count,
                            "{text}"
                        );
                    }
                    let f = object::File::parse(&*bytes).unwrap();
                    for &name in symbols {
                        assert!(
                            f.symbols()
                                .any(|s| s.name() == Ok(name) && s.is_definition())
                        );
                    }
                    assert!(
                        f.symbols()
                            .all(|s| !s.is_undefined() || s.name().unwrap().is_empty())
                    );
                    if level == CpuLevel::V1
                        && let Some(dir) = &dir
                    {
                        let stem = format!("{prefix}-{s:?}-{a:?}-{spill}");
                        std::fs::write(dir.join(format!("{stem}.o")), bytes).unwrap();
                        std::fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                    }
                }
            }
        }
    }
}
#[test]
fn descriptor_status_mutations_export_and_parity() {
    export(
        LOADS,
        "system-load",
        "KEEL_SYSTEM_LOAD_EXPORT_DIR",
        &["system_reg", "system_mem"],
        &[("lldt", 2), ("ltr", 2), ("lmsw", 2)],
    );
}
#[test]
fn address_only_invalidation_export_and_parity() {
    export(
        INVALIDATE,
        "invlpg",
        "KEEL_INVLPG_EXPORT_DIR",
        &["invalidate"],
        &[("invlpg", 1)],
    );
}
