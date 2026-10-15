use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
mod vector_common;
use vector_common::{command, fixture, harness, temp};
fn abi_fixture() -> (String, String) {
    vector_common::abi_fixture(false)
}

const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3],
    float_caller_saved: &[0, 1],
    float_callee_saved: &[],
};
fn compile(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    tiny: bool,
    features: Features,
) -> (String, keel_asm_ir::Program) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if matches!(selection, SelectionTier::Full) {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &selection,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    let pools = if tiny {
        &TINY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let typed = keel_asm::x86::assemble_program_object_with_features(&program, features)
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    let parsed = keel_asm::x86::assemble_object_with_features(&text, features)
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(typed, parsed);
    (text, program)
}
#[test]
fn all_vector_operations_emit_zero_text_and_identical_typed_objects() {
    let source = fixture();
    for (s, a, t) in [
        (SelectionTier::Baseline, AllocationTier::Full, false),
        (SelectionTier::Full, AllocationTier::Full, true),
        (SelectionTier::Full, AllocationTier::Fast, true),
    ] {
        let (text, _) = compile(&source, s, a, t, Features::default());
        for opcode in [
            "paddb", "paddw", "paddd", "paddq", "psubb", "psubq", "pmullw", "addps", "addpd",
            "sqrtps", "sqrtpd",
        ] {
            assert!(text.contains(opcode), "missing {opcode}");
        }
    }
}

#[test]
fn vector_runtime_reference_harness_compiles() {
    let dir = temp("syntax");
    let c = dir.join("test.c");
    fs::write(&c, harness()).unwrap();
    command(
        Command::new("clang")
            .args(["-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(&c),
    );
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn vector_operations_execute_unaligned_edges_masks_and_spills() {
    let dir = temp("runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness()).unwrap();
    let source = fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for tiny in [false, true] {
                let features = if std::is_x86_feature_detected!("avx")
                    && std::is_x86_feature_detected!("bmi1")
                    && std::is_x86_feature_detected!("bmi2")
                {
                    vec![Features::default(), Features::all()]
                } else {
                    vec![Features::default()]
                };
                for features in features {
                    let (_, program) = compile(&source, selection, allocation, tiny, features);
                    let object = dir.join("test.o");
                    fs::write(
                        &object,
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap(),
                    )
                    .unwrap();
                    let exe = dir.join("test");
                    command(
                        Command::new("clang")
                            .args(["-O2", "-ffp-contract=off"])
                            .arg(&c)
                            .arg(&object)
                            .arg("-lm")
                            .arg("-o")
                            .arg(&exe),
                    );
                    command(&mut Command::new(exe));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn vector_abi_fixture_emits_and_reference_harness_cross_compiles() {
    let (source, harness) = abi_fixture();
    let (_, program) = compile(
        &(fixture() + &source),
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::default(),
    );
    assert_eq!(program.stats().text_bytes, 0);
    let dir = temp("abi-syntax");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(&c),
    );
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn vector_sysv_arguments_results_calls_tail_calls_varargs_cycles_and_pressure_execute() {
    let (extra, harness) = abi_fixture();
    let source = fixture() + &extra;
    let dir = temp("abi-runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let (_, program) = compile(
                &source,
                SelectionTier::Full,
                allocation,
                tiny,
                Features::default(),
            );
            let object = dir.join("test.o");
            fs::write(
                &object,
                keel_asm::x86::assemble_program_object(&program).unwrap(),
            )
            .unwrap();
            let exe = dir.join("test");
            command(
                Command::new("clang")
                    .args(["-O2", "-ffp-contract=off"])
                    .arg(&c)
                    .arg(&object)
                    .arg("-o")
                    .arg(&exe),
            );
            command(&mut Command::new(exe));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
