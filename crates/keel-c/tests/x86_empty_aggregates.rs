//! Empty GNU C values have local identity and effects, but no SysV payload.
use keel_ir::Target;
use std::fs;
const SOURCE: &str = include_str!("fixtures/x86_empty_aggregates.c");
const RUNNER: &str = include_str!("fixtures/x86_empty_aggregates_runner.c");
fn compile(source: &str, target: Target, debug: bool) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "empty-aggregates.c",
        keel_c::Options {
            target,
            debug_info: debug,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
#[test]
fn signatures_and_metadata_use_abi_indices() {
    let module = compile(
        r#"
        struct E {}; struct B { long x[5]; };
        struct E empty(struct E e) { return e; }
        long restricted(struct E e, long n, long *restrict p) { return *p+n; }
        struct B hidden(struct E e, long *restrict p) { struct B b={{*p}}; return b; }
    "#,
        Target::X86_64LinuxGnu,
        true,
    );
    let empty = module.functions.iter().find(|f| f.name == "empty").unwrap();
    assert!(empty.sig.params.is_empty());
    assert!(empty.sig.returns.is_empty());
    assert!(empty.debug_vars.iter().any(|v| v.name == "e" && v.param));
    let restricted = module
        .functions
        .iter()
        .find(|f| f.name == "restricted")
        .unwrap();
    assert_eq!(restricted.sig.params.len(), 2);
    assert_eq!(restricted.restrict_params, [1]);
    let hidden = module
        .functions
        .iter()
        .find(|f| f.name == "hidden")
        .unwrap();
    assert_eq!(hidden.sig.params.len(), 2);
    assert_eq!(hidden.restrict_params, [1]);
}
#[test]
fn unsupported_targets_and_shapes_still_reject() {
    for target in [Target::Aarch64LinuxGnu, Target::Aarch64AppleDarwin] {
        let error = keel_c::compile(
            "struct E {};struct E f(struct E e){return e;}",
            "empty.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("zero-size aggregate"),
            "{target:?}: {error}"
        );
    }
    for source in [
        "struct E {}; void f(int,...);void g(void){f(1,(struct E){});}",
        "struct E {}; void f();void g(void){f((struct E){});}",
        "struct E {}; struct E f(__builtin_va_list p){return __builtin_va_arg(p,struct E);}",
        "struct E {int a[0];};void f(struct E e){}",
        "struct E {} __attribute__((aligned(8)));void f(struct E e){}",
        "struct E;void f(struct E e){}",
    ] {
        assert!(
            keel_c::compile(
                source,
                "empty.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "accepted {source}"
        );
    }
}
#[test]
fn empty_types_keep_c_compatibility_and_old_style_boundary() {
    let source = "struct E {};long f(e,n) struct E e; long n; {return sizeof(e)+n;}";
    let error = keel_c::compile(
        source,
        "empty.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported old-style parameter type"),
        "{error}"
    );
    let source = "struct E {};struct F {};void f(struct E);void f(struct F);";
    assert!(
        keel_c::compile(
            source,
            "empty.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false
            }
        )
        .is_err()
    );
}
#[test]
fn empty_aggregate_calls_preserve_object_parity() {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::{AllocationTier, Pools},
        x86_features::{CpuLevel, FeatureSelector, Features},
    };
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let export = std::env::var_os("KEEL_X86_EMPTY_AGGREGATE_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), SOURCE).unwrap();
        fs::write(dir.join("main.c"), RUNNER).unwrap();
    }
    for (optimize, debug) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut module = compile(SOURCE, Target::X86_64LinuxGnu, debug);
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let module = keel_ir::parse(&module.to_string()).unwrap();
        for tier in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let selector = FeatureSelector {
                inner: &tier,
                features: Features::for_level(CpuLevel::V1),
            };
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                for (name, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &empty),
                ] {
                    let text =
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .unwrap();
                    let object = keel_asm::x86::assemble_object(&text).unwrap();
                    let program = keel_codegen::x86_64::compile_program_with_pipeline(
                        &module, pools, &pipeline,
                    )
                    .unwrap();
                    assert_eq!(
                        object,
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    );
                    let mut chunks =
                        keel_asm::x86::ProgramAssembler::new(Features::for_level(CpuLevel::V1));
                    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                        &module,
                        pools,
                        &pipeline,
                        &mut |program| chunks.push(program),
                    )
                    .unwrap();
                    assert_eq!(object, chunks.finish().unwrap());
                    if let Some(dir) = &export {
                        fs::write(
                            dir.join(format!(
                                "empty-{optimize}-{debug}-{tier:?}-{allocator:?}-{name}.o"
                            )),
                            object,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
