//! Compile/export only: privileged instructions must never execute on the host.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use keel_ir::EntityRef;
use object::{Object, ObjectSymbol};

use crate::userspace;

const SOURCE: &str = include_str!("../../../../suite/integration/x86-cpl0/cases/descriptor.keel");
const LOAD_SOURCE: &str =
    include_str!("../../../../suite/integration/x86-cpl0/cases/descriptor_load.keel");

#[test]
fn descriptor_observation_payload_export_and_parity() {
    export_payload(
        SOURCE,
        "KEEL_DESCRIPTOR_EXPORT_DIR",
        "descriptor",
        "descriptor_observe",
        &[("sgdt", 1), ("sidt", 1)],
    );
}

#[test]
fn descriptor_load_payload_export_and_parity() {
    export_payload(
        LOAD_SOURCE,
        "KEEL_DESCRIPTOR_LOAD_EXPORT_DIR",
        "descriptor-load",
        "descriptor_load",
        &[("lgdt", 1), ("lidt", 1)],
    );
    let m = keel_ir::parse(LOAD_SOURCE).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    for (id, name) in [
        (keel_ir::Intrinsic::X86Lgdt, "x86_lgdt"),
        (keel_ir::Intrinsic::X86Lidt, "x86_lidt"),
    ] {
        assert_eq!(id.info().name, name);
        assert_eq!(id.info().params, &[keel_ir::Type::Ptr]);
        assert!(id.info().returns.is_empty());
        assert!(!id.info().pure);
        assert!(id.is_x86_target());
        assert_eq!(
            keel_ir::InstData::Intrinsic {
                id,
                args: vec![keel_ir::Value::new(0)]
            }
            .memory_effects(),
            keel_ir::MemoryEffects::Barrier
        );
    }
    assert_eq!(keel_ir::Intrinsic::X86Lgdt as usize, 774);
    assert_eq!(keel_ir::Intrinsic::X86Lidt as usize, 775);
}

#[test]
fn descriptor_status_payload_export_and_parity() {
    let source =
        include_str!("../../../../suite/integration/x86-cpl0/cases/descriptor_status.keel");
    export_payload(
        source,
        "KEEL_DESCRIPTOR_STATUS_EXPORT_DIR",
        "descriptor-status",
        "descriptor_status",
        &[("sldt", 4), ("str", 4), ("smsw", 4)],
    );
    let m = keel_ir::parse(source).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
}

fn export_payload(source: &str, variable: &str, prefix: &str, symbol: &str, ops: &[(&str, usize)]) {
    let export = std::env::var_os(variable).map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        std::fs::create_dir_all(dir).unwrap();
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (object, text) = userspace::compile(source, CpuLevel::V1, s, a, spilled);
                for &(query, count) in ops {
                    assert_eq!(
                        text.lines()
                            .filter(|line| line.split_whitespace().next() == Some(query))
                            .count(),
                        count,
                        "{text}"
                    );
                }
                let file = object::File::parse(&*object).unwrap();
                assert_eq!(file.format(), object::BinaryFormat::Elf);
                assert_eq!(file.architecture(), object::Architecture::X86_64);
                assert!(
                    file.symbols()
                        .any(|sym| sym.name() == Ok(symbol) && sym.is_definition())
                );
                assert!(
                    file.symbols()
                        .all(|sym| !sym.is_undefined() || sym.name().unwrap().is_empty()),
                    "payload needs an external helper"
                );
                if let Some(dir) = &export {
                    let stem = format!("{prefix}-{s:?}-{a:?}-{spilled}");
                    std::fs::write(dir.join(format!("{stem}.o")), object).unwrap();
                    std::fs::write(dir.join(format!("{stem}.s")), text).unwrap();
                }
            }
        }
    }
}

#[test]
fn unused_descriptor_queries_survive_optimization_and_allocation() {
    let mut source = String::from("func @unused(ptr,i64) {\nentry(v0:ptr,v1:i64):\n");
    for (index, id) in keel_ir::Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_descriptor_query().is_some())
        .enumerate()
    {
        let (_, _, bytes) = id.x86_descriptor_query().unwrap();
        if bytes == 0 {
            source += &format!(" v{} = intrinsic {}(v1)\n", index + 2, id.info().name);
        } else {
            source += &format!(" intrinsic {}(v0)\n", id.info().name);
        }
    }
    source += " return\n}\n";
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (_, text) = userspace::compile(&source, CpuLevel::V1, s, a, spilled);
                for (query, count) in [
                    ("sgdt", 1),
                    ("sidt", 1),
                    ("sldt", 4),
                    ("str", 4),
                    ("smsw", 4),
                ] {
                    // Count instruction tokens, not symbols or substrings.
                    assert_eq!(
                        text.lines()
                            .filter(|line| line.split_whitespace().next() == Some(query))
                            .count(),
                        count,
                        "{text}"
                    );
                }
            }
        }
    }
}
