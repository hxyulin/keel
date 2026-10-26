use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{AsmArch, AsmOperand, InstData, Target};
const SOURCE: &str = include_str!("fixtures/x86_inline_asm.c");
fn compile(source: &str, debug: bool) -> Result<keel_ir::Module, String> {
    let pp = keel_c::preprocess(
        source,
        "x86-inline.c",
        &keel_c::PreprocessOptions {
            target: Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    keel_c::compile(
        &pp,
        "x86-inline.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: debug,
        },
    )
    .map_err(|e| e.to_string())
}
#[test]
fn scalar_constraints_emit_identical_text_typed_and_chunked_objects() {
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let export = std::env::var_os("KEEL_X86_INLINE_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut m = compile(SOURCE, debug).unwrap();
            keel_ir::verify_module(&m).unwrap();
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
                let features = Features::for_level(level);
                for tier in [
                    SelectionTier::Baseline,
                    SelectionTier::Trees,
                    SelectionTier::Full,
                ] {
                    let selector = FeatureSelector {
                        inner: &tier,
                        features,
                    };
                    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                        let pipeline = CodegenPipeline {
                            selector: &selector,
                            allocator: &alloc,
                        };
                        for (pool_name, pools) in [
                            ("normal", &keel_codegen::x86_64::REGISTERS),
                            ("empty", &empty),
                        ] {
                            let p = keel_codegen::x86_64::compile_program_with_pipeline(
                                &m, pools, &pipeline,
                            )
                            .unwrap();
                            let obj =
                                keel_asm::x86::assemble_program_object_with_features(&p, features)
                                    .unwrap();
                            let text =
                                keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)
                                    .unwrap();
                            assert_eq!(
                                obj,
                                keel_asm::x86::assemble_object_with_features(&text, features)
                                    .unwrap()
                            );
                            let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                            keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                                &m,
                                pools,
                                &pipeline,
                                &mut |p| chunks.push(p),
                            )
                            .unwrap();
                            assert_eq!(obj, chunks.finish().unwrap());
                            if let Some(path) = &export {
                                std::fs::write(path.join(format!("{level:?}-{tier:?}-{alloc:?}-{pool_name}-opt{optimize}-debug{debug}.o")),&obj).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn contracts_and_effects_survive_optimization() {
    let mut m = compile(SOURCE, false).unwrap();
    let count = |m: &keel_ir::Module| {
        m.functions
            .iter()
            .flat_map(|f| f.layout_insts().map(move |(_, i)| (f, i)))
            .filter(|(f, i)| matches!(f.inst_data(*i), InstData::InlineAsm { .. }))
            .count()
    };
    let before = count(&m);
    assert!(before >= 20);
    let f = m.functions.iter().find(|f| f.name == "asm_mul").unwrap();
    let asm = f
        .layout_insts()
        .find_map(|(_, i)| {
            if let InstData::InlineAsm { asm, .. } = f.inst_data(i) {
                Some(asm)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(asm.early_clobbers, [0, 1]);
    assert!(matches!(asm.outputs[0].0,AsmOperand::Fixed(r) if r.arch==AsmArch::X86 && r.num==0));
    let text = m.to_string();
    let parsed = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&parsed).unwrap();
    assert_eq!(text, parsed.to_string());
    keel_opt::optimize(&mut m).unwrap();
    assert_eq!(count(&m), before);
}
#[test]
fn unsupported_and_impossible_contracts_are_diagnosed() {
    for (body, expected) in [
        ("asm goto(\"\"::::done); done:;", "asm goto"),
        ("asm(\"\" : \"=m\"(x));", "output constraint"),
        ("asm(\"\" : \"=rm\"(x));", "output constraint"),
        ("asm(\"\" : \"=r,r\"(x));", "output constraint"),
        ("asm(\"\" :: \"m\"(x));", "input constraint"),
        ("asm(\"\" :: \"w\"(x));", "input constraint"),
        ("asm(\"\" :: \"i\"(x));", "integer constant"),
        ("asm(\"\" ::: \"rsp\");", "stack or frame pointer"),
        ("asm(\"\" ::: \"ebp\");", "stack or frame pointer"),
        ("asm(\"\" ::: \"x0\");", "another architecture"),
        ("asm(\"\" ::: \"xmm0\");", "unknown register"),
        ("asm(\"%h0\" :: \"r\"(x));", "modifier"),
        ("asm(\"\" : \"+r\"(x) : \"0\"(x));", "more than one"),
        (
            "asm(\"\" : \"=r\"(x) : \"0\"(x),\"0\"(x));",
            "more than one",
        ),
    ] {
        let source = format!("void f(long x){{{body}}}");
        let err = compile(&source, false).unwrap_err();
        assert!(err.contains(expected), "{source}: {err}");
    }
    for (body, expected) in [
        ("asm(\"\" : \"=a\"(x) : \"a\"(x));", "overlaps"),
        ("asm(\"\" : \"=a\"(x) :: \"eax\");", "overlaps"),
        (
            "asm(\"\" : \"=r\"(x) :: \"rax\",\"rcx\",\"rdx\",\"rbx\",\"rsi\",\"rdi\",\"r8\",\"r9\",\"r10\",\"r11\",\"r12\",\"r13\",\"r14\",\"r15\");",
            "too many",
        ),
        ("asm(\"mov %c0,%%rax\" :: \"r\"(x));", "modifier"),
    ] {
        let source = format!("void f(long x){{{body}}}");
        let m = compile(&source, false).unwrap();
        let err = keel_codegen::compile(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{source}: {err}");
    }
}

struct ForceConstants;
impl keel_codegen::regalloc::RegisterAllocator for ForceConstants {
    fn name(&self) -> &'static str {
        "inline-asm-forced-constant-remat"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_codegen::regalloc::Loc;
        let mut alloc = AllocationTier::Full.allocate(input);
        for (_, inst) in input.func.layout_insts() {
            if matches!(input.func.inst_data(inst),InstData::Iconst {ty:keel_ir::Type::I64,imm} if *imm >= 0)
            {
                for &v in input.func.inst_results(inst) {
                    use keel_ir::EntityRef;
                    if alloc.locs[0][v.index()].is_some() {
                        alloc.locs[0][v.index()] = Some(Loc::Remat(v));
                    }
                }
            }
        }
        alloc
    }
}
#[test]
fn forced_rematerialization_preserves_fixed_operands() {
    let m = compile(SOURCE, false).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &ForceConstants,
    };
    let p = keel_codegen::x86_64::compile_program_with_pipeline(
        &m,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    let obj =
        keel_asm::x86::assemble_program_object_with_features(&p, Features::default()).unwrap();
    let text = keel_codegen::x86_64::compile_with_pipeline(
        &m,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    assert_eq!(obj, keel_asm::x86::assemble_object(&text).unwrap());
    if let Some(path) = std::env::var_os("KEEL_X86_INLINE_EXPORT") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(std::path::Path::new(&path).join("forced-remat.o"), obj).unwrap();
    }
}
