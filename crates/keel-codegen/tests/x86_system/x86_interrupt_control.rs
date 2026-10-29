//! Compile/export only. CLI/STI/HLT never execute on the host.
use crate::userspace;
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
const SOURCE: &str =
    include_str!("../../../../suite/integration/x86-cpl0/cases/interrupt_control.keel");
#[test]
fn interrupt_control_registry_effects_and_foreign_rejection() {
    for (n, id, name) in [
        (814, Intrinsic::X86Cli, "x86_cli"),
        (815, Intrinsic::X86Sti, "x86_sti"),
        (816, Intrinsic::X86Hlt, "x86_hlt"),
        (817, Intrinsic::X86StiHlt, "x86_sti_hlt"),
    ] {
        assert_eq!(id as usize, n);
        assert_eq!(Intrinsic::ALL[n], id);
        assert_eq!(Intrinsic::from_name(name), Some(id));
        assert!(id.info().params.is_empty() && id.info().returns.is_empty());
        assert!(!id.info().pure && id.is_x86_target());
        assert_eq!(
            InstData::Intrinsic { id, args: vec![] }.memory_effects(),
            MemoryEffects::Barrier
        );
        let bad = format!("func @bad(i64) {{\nentry(v0:i64):\n intrinsic {name}(v0)\n return\n}}");
        if let Ok(m) = keel_ir::parse(&bad) {
            assert!(keel_ir::verify_module(&m).is_err());
        }
    }
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
}
#[test]
fn interrupt_control_tiers_adjacency_liveness_and_export() {
    let dir = std::env::var_os("KEEL_INTERRUPT_EXPORT_DIR").map(std::path::PathBuf::from);
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
                    for (op, n) in [("cli", 1), ("sti", 2), ("hlt", 2)] {
                        assert_eq!(
                            text.lines()
                                .filter(|l| l.split_whitespace().next() == Some(op))
                                .count(),
                            n,
                            "{text}"
                        );
                    }
                    let body = text
                        .split("\"interrupt_enable_halt\":")
                        .nth(1)
                        .unwrap()
                        .split(".Linterrupt_enable_halt_end:")
                        .next()
                        .unwrap();
                    let lines = body.lines().map(str::trim).collect::<Vec<_>>();
                    let sti = lines.iter().position(|l| *l == "sti").unwrap();
                    assert_eq!(lines[sti + 1], "hlt", "{body}");
                    if level == CpuLevel::V1
                        && let Some(dir) = &dir
                    {
                        let name = format!("interrupt-{s:?}-{a:?}-{spill}");
                        std::fs::write(dir.join(format!("{name}.o")), bytes).unwrap();
                        std::fs::write(dir.join(format!("{name}.s")), text).unwrap();
                    }
                }
            }
        }
    }
}
