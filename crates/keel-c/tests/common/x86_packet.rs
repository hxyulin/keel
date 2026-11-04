//! Reused SSE–AVX2 fixture emission and native execution boundary.
use keel_test_support::native_program;

use keel_codegen::x86_features::{CpuLevel, FeatureSelector, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
use keel_ir::Target;

pub fn frontend(source: &str) -> keel_ir::Module {
    let target = Target::X86_64LinuxGnu;
    let preprocessed = keel_c::preprocess(
        source,
        "construct.c",
        &keel_c::PreprocessOptions {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    let module = keel_c::compile(
        &preprocessed,
        "construct.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

pub fn check(source: &str, runner: &str, name: &str, reference_flags: Option<&[&str]>) {
    let module = frontend(source);
    let features = Features::for_level(CpuLevel::V3);
    let mut optimized = module.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    for (config, module, selection, spilled) in [
        ("baseline", &module, SelectionTier::Baseline, false),
        ("trees", &module, SelectionTier::Trees, false),
        ("optimized-spill", &optimized, SelectionTier::Full, true),
    ] {
        let selector = FeatureSelector {
            inner: &selection,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let empty = Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let pools = if spilled {
            &empty
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
        let bytes =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            bytes,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
            "{config}"
        );
        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
            module,
            pools,
            &pipeline,
            &mut |chunk| chunks.push(chunk),
        )
        .unwrap();
        assert_eq!(bytes, chunks.finish().unwrap(), "{config}");
        let object = format!("{config}.o");
        let mut native = native_program!(format!("{name}-{config}"))
            .requires(&["avx2"])
            .file("kernels.c", source)
            .file("runner.c", runner)
            .file(&*object, bytes)
            .compile(["-O2", "-c", "runner.c", "-o", "runner.o"]);
        let mut inputs = vec!["runner.o", &object];
        if let Some(flags) = reference_flags {
            native = native.compile(["-O2"].iter().chain(flags).chain(&[
                "-c",
                "kernels.c",
                "-o",
                "reference.o",
            ]));
            inputs.push("reference.o");
        }
        native.link(inputs).check();
    }
}
