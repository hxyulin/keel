//! Privileged compile/export checks; never execute the payload on the host.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use object::{Object, ObjectSection, ObjectSymbol};

#[path = "common/userspace.rs"]
mod userspace;

const SOURCE: &str = include_str!("../../../suite/integration/x86-cpl0/cases/cpu_control.keel");
const OPS: [(Intrinsic, &str, &str, usize, u8); 3] = [
    (Intrinsic::X86Clts, "x86_clts", "clts", 778, 0x06),
    (Intrinsic::X86Invd, "x86_invd", "invd", 779, 0x08),
    (Intrinsic::X86Wbinvd, "x86_wbinvd", "wbinvd", 780, 0x09),
];

#[test]
fn control_signatures_effects_and_literal_encodings() {
    for (id, name, op, number, last_byte) in OPS {
        let info = id.info();
        assert_eq!(info.name, name);
        assert!(info.params.is_empty() && info.returns.is_empty());
        assert!(!info.pure);
        assert!(id.is_x86_target());
        assert_eq!(id as usize, number);
        assert_eq!(Intrinsic::ALL[number], id);
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(inst.is_pinned());
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
        let bytes = keel_asm::x86::assemble_object_with_features(op, Features::NONE).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        assert_eq!(
            file.section_by_name(".text").unwrap().data().unwrap(),
            [0x0f, last_byte]
        );
        let bad = format!("func @f(i64) {{\nentry(v0:i64):\n intrinsic {name}(v0)\n return\n}}");
        if let Ok(module) = keel_ir::parse(&bad) {
            assert!(keel_ir::verify_module(&module).is_err(), "{bad}");
        }
    }
    let module = keel_ir::parse(SOURCE).unwrap();
    assert!(keel_codegen::aarch64::compile(&module).is_err());
}

#[test]
fn privileged_controls_survive_optimization_with_parity_and_live_inputs() {
    let export = std::env::var_os("KEEL_CPU_CONTROL_EXPORT_DIR").map(std::path::PathBuf::from);
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
                    let ops: Vec<_> = text
                        .lines()
                        .map(str::trim)
                        .filter(|line| {
                            !line.is_empty()
                                && !line.starts_with('.')
                                && !line.starts_with('#')
                                && !line.ends_with(':')
                        })
                        .filter_map(|line| line.split_whitespace().next())
                        .collect();
                    for (_, _, op, _, _) in OPS {
                        assert_eq!(ops.iter().filter(|s| **s == op).count(), 1, "{text}");
                    }
                    // This finite fixture writes back immediately before INVD;
                    // no compiler spill store may dirty its stack between them.
                    let at = ops.iter().position(|s| *s == "wbinvd").unwrap();
                    assert_eq!(ops[at + 1], "invd", "{text}");
                    assert!(!text.contains("xmm") && !text.contains("ymm"), "{text}");
                    let file = object::File::parse(&*bytes).unwrap();
                    assert!(
                        file.symbols()
                            .any(|s| s.name() == Ok("cpu_control") && s.is_definition())
                    );
                    assert!(
                        file.symbols()
                            .all(|s| !s.is_undefined() || s.name().unwrap().is_empty())
                    );
                    if level == CpuLevel::V1 {
                        if let Some(dir) = &export {
                            let stem =
                                format!("cpu-control-{selection:?}-{allocation:?}-{spilled}");
                            std::fs::write(dir.join(format!("{stem}.o")), bytes).unwrap();
                            std::fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                        }
                    }
                }
            }
        }
    }
}
