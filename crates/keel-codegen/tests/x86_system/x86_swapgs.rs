//! Compile/export only. SWAPGS executes only in a disposable CPL0 guest.
use crate::userspace;
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use object::{Object, ObjectSymbol};
const SOURCE: &str = include_str!("../../../../suite/integration/x86-cpl0/cases/swapgs.keel");
#[test]
fn swapgs_registry_effects_and_foreign_rejection() {
    let id = Intrinsic::X86Swapgs;
    assert_eq!(id as usize, 818);
    assert_eq!(Intrinsic::ALL[818], id);
    assert_eq!(Intrinsic::from_name("x86_swapgs"), Some(id));
    assert!(id.info().params.is_empty() && id.info().returns.is_empty());
    assert!(!id.info().pure && id.is_x86_target());
    assert_eq!(
        InstData::Intrinsic { id, args: vec![] }.memory_effects(),
        MemoryEffects::Barrier
    );
    let bad = "func @bad(i64) {\nentry(v0:i64):\n intrinsic x86_swapgs(v0)\n return\n}";
    if let Ok(m) = keel_ir::parse(bad) {
        assert!(keel_ir::verify_module(&m).is_err());
    }
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
}
#[test]
fn swapgs_tiers_roundtrip_retention_and_export() {
    let dir = std::env::var_os("KEEL_SWAPGS_EXPORT_DIR").map(std::path::PathBuf::from);
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
                    assert_eq!(
                        text.lines()
                            .filter(|l| l.split_whitespace().next() == Some("swapgs"))
                            .count(),
                        4,
                        "{text}"
                    );
                    let object = object::File::parse(&*bytes).unwrap();
                    let undefined = object
                        .symbols()
                        .filter(|s| s.is_undefined())
                        .filter_map(|s| s.name().ok())
                        .filter(|n| !n.is_empty())
                        .collect::<Vec<_>>();
                    assert_eq!(undefined, ["gs_observe"]);
                    if level == CpuLevel::V1
                        && let Some(dir) = &dir
                    {
                        let name = format!("swapgs-{s:?}-{a:?}-{spill}");
                        std::fs::write(dir.join(format!("{name}.o")), bytes).unwrap();
                        std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
                    }
                }
            }
        }
    }
}
