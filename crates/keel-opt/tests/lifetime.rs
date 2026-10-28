//! Successful reads cannot supply facts across fixed-slot lifetime epochs.
use keel_ir::{InstData, Intrinsic};

fn source(markers: &str) -> String {
    format!(
        "func @f() -> i64 {{
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    v1 = iconst.i64 1
    store v1, v0
    v2 = load.i64 v0
    {markers}
    v3 = load.i64 v0
    v4 = iadd v2, v3
    return v4
}}"
    )
}

#[test]
fn load_facts_are_invalidated_by_start_end_and_restart() {
    // This checks conservative analysis invalidation only. Dead-epoch loads
    // are not executed or used as differential source-level equivalence cases.
    for pass in ["load-reuse", "memory-forward"] {
        for markers in [
            "",
            "intrinsic lifetime_start(v0)",
            "intrinsic lifetime_end(v0)",
            "intrinsic lifetime_end(v0)\n    intrinsic lifetime_start(v0)",
        ] {
            let mut module = keel_ir::parse(&source(markers)).unwrap();
            keel_ir::verify_module(&module).unwrap();
            keel_opt::run_passes(&mut module, &[pass]).unwrap();
            let function = &module.functions[0];
            let loads = function
                .layout_insts()
                .filter(|(_, i)| matches!(function.inst_data(*i), InstData::Load { .. }))
                .count();
            let before = usize::from(pass == "load-reuse");
            assert_eq!(
                loads,
                before + usize::from(!markers.is_empty()),
                "{pass}: {markers}"
            );
            keel_opt::run_passes(&mut module, &["dce", "gcm"]).unwrap();
            let function = &module.functions[0];
            let count = function
                .layout_insts()
                .filter(|(_, i)| {
                    matches!(
                        function.inst_data(*i),
                        InstData::Intrinsic {
                            id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(
                count,
                markers.lines().count(),
                "DCE/GCM must retain markers"
            );
        }
    }
}
