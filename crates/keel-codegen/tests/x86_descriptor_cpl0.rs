//! Compile/export only: privileged instructions must never execute on the host.
use keel_asm_ir::x86::CpuLevel;
use keel_codegen::{SelectionTier, regalloc::AllocationTier};
use object::{Object, ObjectSymbol};

#[path = "common/userspace.rs"]
mod userspace;

const SOURCE: &str = include_str!("../../../suite/x86-cpl0/cases/descriptor.keel");

#[test]
fn descriptor_observation_payload_export_and_parity() {
    let export = std::env::var_os("KEEL_DESCRIPTOR_EXPORT_DIR").map(std::path::PathBuf::from);
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
                let (object, text) = userspace::compile(SOURCE, CpuLevel::V1, s, a, spilled);
                for query in ["sgdt", "sidt"] {
                    assert_eq!(
                        text.lines()
                            .filter(|line| line.split_whitespace().next() == Some(query))
                            .count(),
                        1,
                        "{text}"
                    );
                }
                let file = object::File::parse(&*object).unwrap();
                assert_eq!(file.format(), object::BinaryFormat::Elf);
                assert_eq!(file.architecture(), object::Architecture::X86_64);
                assert!(
                    file.symbols()
                        .any(|sym| sym.name() == Ok("descriptor_observe") && sym.is_definition())
                );
                assert!(
                    file.symbols()
                        .all(|sym| !sym.is_undefined() || sym.name().unwrap().is_empty()),
                    "payload needs an external helper"
                );
                if let Some(dir) = &export {
                    let stem = format!("descriptor-{s:?}-{a:?}-{spilled}");
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
