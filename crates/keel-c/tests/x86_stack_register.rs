//! Read-only RSP bindings sample hardware state and never allocate global data.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, Target};
use object::{Object, ObjectSymbol};

const SOURCE: &str = include_str!("fixtures/x86_stack_register.c");

fn compile(source: &str, target: Target) -> Result<keel_ir::Module, String> {
    keel_c::compile(
        source,
        "stack-register.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn rsp_reads_survive_stack_changes_without_emitting_a_data_symbol() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let features = Features::for_level(CpuLevel::V1);
    let export = std::env::var_os("KEEL_X86_STACK_REGISTER_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(SOURCE, Target::X86_64LinuxGnu).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let snapshot = module
            .functions
            .iter()
            .find(|f| f.name == "stack_register_snapshot")
            .unwrap();
        let saves = snapshot
            .layout_insts()
            .filter(|(_, inst)| {
                matches!(
                    snapshot.inst_data(*inst),
                    InstData::Intrinsic {
                        id: Intrinsic::StackSave,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(saves, 2, "both reads must remain around dynamic alloca");
        assert!(
            module
                .data
                .iter()
                .all(|d| d.name != "current_stack_pointer" && d.name != "rsp")
        );
        keel_ir::verify_module(&keel_ir::parse(&module.to_string()).unwrap()).unwrap();
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
                    let object = object::File::parse(bytes.as_slice()).unwrap();
                    assert!(
                        object
                            .symbols()
                            .all(|s| !matches!(s.name().unwrap(), "rsp" | "current_stack_pointer"))
                    );
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
fn unsupported_register_declarations_and_writes_fail_closed() {
    for (source, expected) in [
        ("register unsigned long sp;", "read-only global rsp"),
        (
            "register unsigned long sp asm(\"r12\");",
            "read-only global rsp",
        ),
        (
            "register unsigned long sp asm(\"RSP\");",
            "read-only global rsp",
        ),
        (
            "register long sp asm(\"rsp\");",
            "unqualified unsigned long",
        ),
        (
            "register void *sp asm(\"rsp\");",
            "unqualified unsigned long",
        ),
        (
            "register const unsigned long sp asm(\"rsp\");",
            "unqualified unsigned long",
        ),
        (
            "register volatile unsigned long sp asm(\"rsp\");",
            "unqualified unsigned long",
        ),
        (
            "typedef const unsigned long U; register U sp asm(\"rsp\");",
            "unqualified unsigned long",
        ),
        ("register unsigned long sp asm(\"rsp\")=0;", "initializers"),
        (
            "register unsigned long sp asm(\"rsp\") __attribute__((used));",
            "declaration attributes",
        ),
        (
            "static register unsigned long sp asm(\"rsp\");",
            "another storage class",
        ),
        (
            "register static unsigned long sp asm(\"rsp\");",
            "another storage class",
        ),
        (
            "unsigned long sp; register unsigned long sp asm(\"rsp\");",
            "existing declaration",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); extern unsigned long sp;",
            "ordinary symbol",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); void f(void){int sp; {extern unsigned long sp;}}",
            "ordinary symbol",
        ),
        (
            "void f(void){register unsigned long sp asm(\"rsp\");}",
            "automatic locals/register",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); void f(void){sp=0;}",
            "assignable",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); void f(void){++sp;}",
            "assignable",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); void *f(void){return &sp;}",
            "assignable",
        ),
        (
            "register unsigned long sp asm(\"rsp\"); void f(void){asm(\"\":\"=r\"(sp));}",
            "stack-call contract",
        ),
    ] {
        let error = compile(source, Target::X86_64LinuxGnu).unwrap_err();
        assert!(
            error.contains(expected),
            "{source}: expected {expected}: {error}"
        );
    }
    for target in [
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
        Target::Wasm32UnknownUnknown,
    ] {
        let error = compile("register unsigned long sp asm(\"rsp\");", target).unwrap_err();
        assert!(
            error.contains("global register variables require x86-64 Linux"),
            "{target:?}: {error}"
        );
    }
    compile(
        "int f(register int x){register int y=x+1; return y;}",
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    compile(
        "register unsigned long sp asm(\"rsp\"); int f(void){int sp=3;return sp;}",
        Target::X86_64LinuxGnu,
    )
    .unwrap();
}
