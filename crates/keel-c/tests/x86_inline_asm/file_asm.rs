use keel_ir::Target;
use object::{Object, ObjectSection, ObjectSymbol};

fn compile(source: &str) -> keel_ir::Module {
    let module = keel_c::compile(
        source,
        "file-asm.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn module_assembly_preserves_order_macros_symbols_and_relocations() {
    let source = r#"
asm(".macro load_answer\nmovl $37,%eax\n.endm\n");
int target(void) { return 19; }
__asm__(".text\n.globl asm_answer\n.type asm_answer,@function\nasm_answer:\nload_answer\nret\n.size asm_answer,.-asm_answer\n"
        ".weak alias\n.set alias,target\n"
        ".pushsection .keel.exports,\"a\",@progbits\n.balign 8\n.quad target\n.popsection\n");
extern int asm_answer(void);
extern int alias(void);
int call_assembly(void) { return asm_answer() + alias(); }
"#;
    let mut module = compile(source);
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let module = keel_ir::parse(&module.to_string()).unwrap();
    let text = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
    let bytes = keel_asm::x86::assemble_object(&text).unwrap();
    let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_program_object(&program).unwrap()
    );
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(file.symbol_by_name("alias").unwrap().is_weak());
    let section = file.section_by_name(".keel.exports").unwrap();
    assert_eq!(section.size(), 8);
    assert_eq!(section.relocations().count(), 1);
    let answer = file.symbol_by_name("asm_answer").unwrap();
    let code = file
        .section_by_index(answer.section_index().unwrap())
        .unwrap()
        .data()
        .unwrap();
    let start = answer.address() as usize;
    assert_eq!(&code[start..start + 6], &[0xb8, 37, 0, 0, 0, 0xc3]);
}

#[test]
fn basic_asm_is_literal_and_invalid_source_is_diagnosed() {
    for source in [
        r#"asm(L"nop");"#,
        r#"asm("nop\0");"#,
        r#"asm("nop" : :);"#,
        r#"asm volatile("nop");"#,
    ] {
        assert!(
            keel_c::compile(
                source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "{source}"
        );
    }
    let module = compile("asm(\"this_is_not_an_instruction %eax\");");
    assert_eq!(module.assembly, ["this_is_not_an_instruction %eax"]);
    // Stateful source validation now diagnoses malformed literal instructions
    // before either text or structured codegen publishes the module.
    for error in [
        keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap_err(),
        keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap_err(),
    ] {
        assert_eq!(error.func, "<module asm>");
        assert!(
            error.message.contains("this_is_not_an_instruction"),
            "{error}"
        );
    }
    for target in [
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
        Target::Wasm32UnknownUnknown,
    ] {
        assert!(
            keel_c::compile(
                "asm(\"nop\");",
                "bad.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .is_err()
        );
        if target != Target::Wasm32UnknownUnknown {
            assert!(
                keel_codegen::compile(&module, target)
                    .unwrap_err()
                    .to_string()
                    .contains("module assembly")
            );
        }
    }
    assert!(
        keel_wasm::emit_module(&module, &Default::default())
            .unwrap_err()
            .to_string()
            .contains("module assembly")
    );
}

#[test]
fn module_asm_references_keep_explicitly_used_static_definitions() {
    let mut module = compile(
        r#"
static __attribute__((used)) long kept = 41;
static __attribute__((used)) long kept_fn(void) { return kept; }
asm(".pushsection .references,\"a\",@progbits\n.quad kept,kept_fn\n.popsection\n");
"#,
    );
    keel_opt::optimize(&mut module).unwrap();
    assert!(module.functions.iter().any(|f| f.name == "kept_fn"));
    assert!(module.data.iter().any(|d| d.name == "kept"));
    let bytes = keel_asm::x86::assemble_program_object(
        &keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap(),
    )
    .unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(
        file.section_by_name(".references")
            .unwrap()
            .relocations()
            .count(),
        2
    );
}

#[test]
fn kernel_lifetime_sections_have_correct_types_flags_and_zero_bytes() {
    let source = r#"
int first __attribute__((section(".init.data"))) = 19;
int zeros[4] __attribute__((section(".init.data"), aligned(32)));
const int table[2] __attribute__((section(".init.rodata"))) = {23, 29};
int init(void) __attribute__((section(".init.text")));
int init(void) { return first + table[0] + zeros[0]; }
"#;
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let m = keel_c::compile(
            source,
            "init.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        let text = keel_codegen::compile(&m, target).unwrap();
        let bytes = if target == Target::X86_64LinuxGnu {
            keel_asm::x86::assemble_object(&text).unwrap()
        } else {
            keel_asm::assemble_elf_object(&text).unwrap()
        };
        let file = object::File::parse(bytes.as_slice()).unwrap();
        for (name, flags) in [(".init.data", 3), (".init.text", 6), (".init.rodata", 2)] {
            let s = file.section_by_name(name).unwrap();
            assert_eq!(s.flags(), object::SectionFlags::Elf { sh_flags: flags });
            assert_ne!(s.kind(), object::SectionKind::UninitializedData);
        }
        let symbol = file.symbol_by_name("zeros").unwrap();
        let data = file
            .section_by_index(symbol.section_index().unwrap())
            .unwrap()
            .data()
            .unwrap();
        assert_eq!(&data[symbol.address() as usize..][..16], &[0; 16]);
    }
    for source in [
        r#"int x __attribute__((section(".init.text")));"#,
        r#"int x __attribute__((section(".init_array")));"#,
        r#"__thread int x __attribute__((section(".init.data")));"#,
        r#"int x __attribute__((section(".init.rodata")));"#,
    ] {
        assert!(
            keel_c::compile(
                source,
                "bad.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: false
                }
            )
            .is_err(),
            "{source}"
        );
    }
}

const FIXTURE: &str = include_str!("../fixtures/x86_file_asm.c");

#[test]
fn module_asm_has_text_typed_and_chunked_parity_across_codegen_choices() {
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
    let export = std::env::var_os("KEEL_X86_FILE_ASM_EXPORT").map(std::path::PathBuf::from);
    if let Some(path) = &export {
        std::fs::create_dir_all(path).unwrap();
    }
    for debug in [false, true] {
        for optimize in [false, true] {
            let mut module = keel_c::compile(
                FIXTURE,
                "file-asm.c",
                keel_c::Options {
                    target: Target::X86_64LinuxGnu,
                    debug_info: debug,
                },
            )
            .unwrap();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            let module = keel_ir::parse(&module.to_string()).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for tier in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                let features = Features::for_level(CpuLevel::V1);
                let selector = FeatureSelector {
                    inner: &tier,
                    features,
                };
                for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    for (pool_name, pools) in [
                        ("normal", &keel_codegen::x86_64::REGISTERS),
                        ("empty", &empty),
                    ] {
                        let text =
                            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                                .unwrap();
                        let bytes = keel_asm::x86::assemble_object(&text).unwrap();
                        let program = keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline,
                        )
                        .unwrap();
                        assert_eq!(
                            bytes,
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        );
                        let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
                        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                            &module,
                            pools,
                            &pipeline,
                            &mut |program| chunks.push(program),
                        )
                        .unwrap();
                        assert_eq!(bytes, chunks.finish().unwrap());
                        if let Some(path) = &export {
                            std::fs::write(path.join(format!("{tier:?}-{allocator:?}-{pool_name}-opt{optimize}-debug{debug}.o")), bytes).unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn emitted_module_asm_matches_clang_and_gnu_sections_symbols_and_relocations() {
    use object::{RelocationTarget, SymbolSection};
    let module = compile(FIXTURE);
    let text = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
    let snapshot = |bytes: &[u8]| {
        let file = object::File::parse(bytes).unwrap();
        let mut sections = Vec::new();
        for section in file.sections() {
            let name = section.name().unwrap();
            if ![
                ".text",
                ".data",
                ".init.text",
                ".init.data",
                ".init.rodata",
                ".keel.exports",
                ".keel.relative",
            ]
            .contains(&name)
            {
                continue;
            }
            let relocations = section
                .relocations()
                .map(|(offset, r)| {
                    let RelocationTarget::Symbol(symbol) = r.target() else {
                        panic!("expected symbolic relocation");
                    };
                    let symbol = file.symbol_by_index(symbol).unwrap();
                    let name = if symbol.kind() == object::SymbolKind::Section {
                        format!(
                            "section:{}",
                            file.section_by_index(symbol.section_index().unwrap())
                                .unwrap()
                                .name()
                                .unwrap()
                        )
                    } else {
                        symbol.name().unwrap().to_owned()
                    };
                    (offset, name, r.addend(), format!("{:?}", r.flags()))
                })
                .collect::<Vec<_>>();
            sections.push((
                name.to_owned(),
                section.data().unwrap().to_vec(),
                section.align(),
                format!("{:?}", section.flags()),
                relocations,
            ));
        }
        sections.sort();
        let mut symbols = file
            .symbols()
            .filter(|s| {
                [
                    "asm_leaf",
                    "alias",
                    "target",
                    "datum",
                    "zeros",
                    "records",
                    "asm_bytes",
                    "kept",
                    "kept_fn",
                    "table",
                ]
                .contains(&s.name().unwrap())
            })
            .map(|s| {
                let section = match s.section() {
                    SymbolSection::Section(index) => file
                        .section_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_owned(),
                    other => format!("{other:?}"),
                };
                (
                    s.name().unwrap().to_owned(),
                    section,
                    s.address(),
                    s.size(),
                    s.is_weak(),
                    s.is_global(),
                    format!("{:?}", s.kind()),
                )
            })
            .collect::<Vec<_>>();
        symbols.sort();
        (sections, symbols)
    };
    let own = keel_asm::x86::assemble_object(&text).unwrap();
    let expected = snapshot(&own);
    let dir = std::env::temp_dir().join(format!("keel-file-asm-oracles-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    std::fs::write(&input, text).unwrap();
    for gnu in [false, true] {
        let mut command = if gnu {
            let mut command = std::process::Command::new(
                std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()),
            );
            command.args(["--64", "-mrelax-relocations=no"]);
            command
        } else {
            let mut command = std::process::Command::new("clang");
            command.args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-c",
                "-Wa,-mrelax-relocations=no",
            ]);
            command
        };
        let output = dir.join("test.o");
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut expected = expected.clone();
        if gnu {
            // GAS rewrites this PLT reference through the .set target. Clang
            // and Keel retain the weak alias identity; all other fields agree.
            for section in &mut expected.0 {
                for relocation in &mut section.4 {
                    if relocation.1 == "alias" {
                        relocation.1 = "target".into();
                    }
                }
            }
        }
        assert_eq!(
            expected,
            snapshot(&std::fs::read(output).unwrap()),
            "GNU={gnu}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn external_section_annotations_do_not_allocate_storage() {
    let module = compile(
        r#"
extern int imported __attribute__((section(".init.data")));
extern const void * const pointer __attribute__((section(".data.rel.ro")));
extern _Thread_local int tls __attribute__((section(".tbss")));
extern int external(void) __attribute__((section(".init.text")));
int read_external(void) { return imported + external(); }
extern int owned __attribute__((section(".init.data")));
int owned = 7;
int defined(void) __attribute__((section(".init.text")));
int defined(void) { return owned; }
"#,
    );
    let decl = module.decls.iter().find(|d| d.name == "external").unwrap();
    assert!(decl.attrs.section.is_none());
    assert!(
        module
            .data
            .iter()
            .find(|d| d.name == "imported")
            .unwrap()
            .section
            .is_none()
    );
    assert_eq!(
        module
            .functions
            .iter()
            .find(|f| f.name == "defined")
            .unwrap()
            .attrs
            .section
            .unwrap()
            .as_str(),
        ".init.text"
    );
    assert_eq!(
        module
            .data
            .iter()
            .find(|d| d.name == "owned")
            .unwrap()
            .section
            .unwrap()
            .as_str(),
        ".init.data"
    );
    let roundtrip = keel_ir::parse(&module.to_string()).unwrap();
    let bytes = keel_asm::x86::assemble_program_object(
        &keel_codegen::compile_program(&roundtrip, Target::X86_64LinuxGnu).unwrap(),
    )
    .unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(file.symbol_by_name("external").unwrap().is_undefined());
    assert!(file.symbol_by_name("imported").unwrap().is_undefined());
    assert!(file.section_by_name(".tbss").is_none());
    assert!(file.section_by_name(".data.rel.ro").is_none());
    assert_eq!(file.section_by_name(".init.data").unwrap().size(), 4);
}
