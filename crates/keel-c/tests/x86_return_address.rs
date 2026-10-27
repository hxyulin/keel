use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, Module, Target};

const SOURCE: &str = include_str!("fixtures/x86_return_address.c");

fn compile(source: &str, target: Target) -> Result<Module, String> {
    keel_c::compile(
        source,
        "return-address.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn return_addresses_keep_a_frame_and_survive_all_object_routes() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V1);
    let export = std::env::var_os("KEEL_X86_RETURN_ADDRESS_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(SOURCE, Target::X86_64LinuxGnu).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let reparsed = keel_ir::parse(&module.to_string()).unwrap();
        keel_ir::verify_module(&reparsed).unwrap();
        for name in [
            "read_return",
            "read_inlined_return",
            "read_return_after_call",
            "read_aligned_return",
            "read_dynamic_return",
        ] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                function.layout_insts().any(|(_, i)| matches!(
                    function.inst_data(i),
                    InstData::Intrinsic {
                        id: Intrinsic::FrameAddress,
                        ..
                    }
                )),
                "{name}: missing frame contract"
            );
            assert!(
                function.layout_insts().any(|(_, i)| matches!(
                    function.inst_data(i),
                    InstData::Load {
                        ty: keel_ir::Type::Ptr,
                        ..
                    }
                )),
                "{name}: missing saved return-address load"
            );
        }
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let selector = FeatureSelector {
                inner: &selection,
                features,
            };
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &empty),
                ] {
                    let program = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    let bytes =
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap();
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    assert_eq!(
                        bytes,
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                    );
                    let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |p| chunks.push(p),
                    )
                    .unwrap();
                    assert_eq!(bytes, chunks.finish().unwrap());
                    if let Some(path) = &export {
                        std::fs::write(
                            path.join(format!(
                                "{selection:?}-{allocator:?}-{pool}-opt{optimize}.o"
                            )),
                            bytes,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn return_address_queries_check_target_depth_and_pointer_arguments() {
    for argument in [
        "1",
        "-1",
        "n",
        "0.0",
        "(n++, 0)",
        "(unsigned __int128)1 << 64",
        "((1U << 32) - (1U << 32))",
    ] {
        let source = format!("void *f(int n) {{ return __builtin_return_address({argument}); }}");
        let error = compile(&source, Target::X86_64LinuxGnu).unwrap_err();
        assert!(
            error.contains("constant integer depth 0"),
            "{source}: {error}"
        );
    }
    for builtin in ["return_address", "extract_return_addr", "frob_return_addr"] {
        for target in [
            Target::Aarch64AppleDarwin,
            Target::Aarch64LinuxGnu,
            Target::Wasm32UnknownUnknown,
        ] {
            let error = compile(
                &format!("void *f(void) {{ return __builtin_{builtin}(0); }}"),
                target,
            )
            .unwrap_err();
            assert!(
                error.contains("requires x86-64 Linux"),
                "{builtin} {target:?}: {error}"
            );
        }
    }
    for builtin in ["extract_return_addr", "frob_return_addr"] {
        compile(
            &format!("void *f(void) {{ return __builtin_{builtin}(0); }}"),
            Target::X86_64LinuxGnu,
        )
        .unwrap();
        assert!(
            compile(
                &format!("void *f(void) {{ return __builtin_{builtin}(1); }}"),
                Target::X86_64LinuxGnu
            )
            .is_err()
        );
    }
    for target in Target::NATIVE {
        let pp = keel_c::preprocess("#if __has_builtin(__builtin_return_address)\nint supported;\n#else\nint unavailable;\n#endif\n", "queries.c", &keel_c::PreprocessOptions { target, ..Default::default() }).unwrap();
        assert_eq!(
            pp.contains("int supported"),
            target == Target::X86_64LinuxGnu
        );
    }
}
