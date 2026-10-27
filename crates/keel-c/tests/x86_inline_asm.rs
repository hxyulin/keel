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
fn basic_asm_preserves_literal_text_and_global_memory_effects() {
    let source = r#"
int basic_global;
int basic_store(int x) {
    basic_global = x;
    asm("movl $37, basic_global(%rip)\n\t# literal %0 and %% and %=");
    return basic_global;
}

void tile_bytes(void) { asm volatile(".byte 0xc4, 0xe2, 0x78, 0x49, 0xc0"); }
static inline void unused_basic(void) { asm("unsupported_unused_instruction"); }
"#;
    for optimize in [false, true] {
        let mut module = compile(source, false).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let function = module
            .functions
            .iter()
            .find(|f| f.name == "basic_store")
            .unwrap();
        let asm = function
            .layout_insts()
            .find_map(|(_, i)| match function.inst_data(i) {
                InstData::InlineAsm { asm, .. } => Some(asm),
                _ => None,
            })
            .unwrap();
        assert!(asm.clobbers.contains(&keel_ir::AsmClobber::Memory));
        assert!(asm.clobbers.contains(&keel_ir::AsmClobber::Flags));
        let text = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
        assert!(text.contains("movl $37, basic_global(%rip)"), "{text}");
        assert!(text.contains("# literal %0 and %% and %="), "{text}");
        let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        let object = keel_asm::x86::assemble_program_object(&program).unwrap();
        assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
        if let Some(path) = std::env::var_os("KEEL_X86_BASIC_ASM_EXPORT") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                std::path::Path::new(&path).join(format!("basic-opt{optimize}.o")),
                object,
            )
            .unwrap();
        }
    }
}
#[test]
fn kernel_sized_alignment_uses_existing_storage_paths() {
    let source = r#"
struct irq_stack { char bytes[16384]; } __attribute__((aligned(16384)));
struct irq_stack irq_global __attribute__((section(".init.data")));
unsigned long kernel_alignment(void) {
    struct irq_stack local;
    asm("" :: "r"(&local) : "memory");
    return ((unsigned long)&local | (unsigned long)&irq_global) & 16383;
}
"#;
    for optimize in [false, true] {
        let mut module = compile(source, false).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        let object = keel_asm::x86::assemble_program_object(&program).unwrap();
        if let Some(path) = std::env::var_os("KEEL_X86_BASIC_ASM_EXPORT") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                std::path::Path::new(&path).join(format!("alignment-opt{optimize}.o")),
                object,
            )
            .unwrap();
        }
    }
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
    // Parsing renumbers SSA values after required inlining removed calls.
    let contracts = |m: &keel_ir::Module| {
        m.functions
            .iter()
            .flat_map(|f| {
                f.layout_insts().filter_map(move |(_, i)| {
                    if let InstData::InlineAsm { asm, .. } = f.inst_data(i) {
                        Some(asm.clone())
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(contracts(&m), contracts(&parsed));
    let normalized = parsed.to_string();
    assert_eq!(normalized, keel_ir::parse(&normalized).unwrap().to_string());
    keel_opt::optimize(&mut m).unwrap();
    assert_eq!(count(&m), before);
}
#[test]
fn unsupported_and_impossible_contracts_are_diagnosed() {
    for (body, expected) in [
        ("asm goto(\"\"::::done); done:;", "asm goto"),
        ("asm(\"\" : \"=ri\"(x));", "output constraint"),
        ("asm(\"\" : \"=&m\"(x));", "early-clobber"),
        ("asm(\"\" : \"=m\"(x) : \"0\"(x));", "register output"),
        ("asm(\"\" :: \"m\"(x+1));", "assignable"),
        ("asm(\"\" : \"=r,r\"(x));", "output constraint"),
        ("asm(\"\" :: \"w\"(x));", "input constraint"),
        ("asm(\"\" :: \"i\"(x));", "constant immediate"),
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

#[test]
fn required_inline_finalizes_numeric_and_symbolic_immediates_without_optimization() {
    let source = r#"
extern long global;
extern long other;
static __attribute__((always_inline)) inline void *rip(void *p) {
    asm("leaq %c1(%%rip), %0" : "=r"(p) : "i"(p));
    return p;
}
static __attribute__((always_inline)) inline long number(long n) {
    long out;
    asm("movq %1,%0" : "=r"(out) : "n"(n));
    return out;
}
static __attribute__((always_inline)) inline void *outer(void *p) { return rip(p); }
void *address(void) { return outer(&global); }
void *offset(void) { return rip((char *)&other + 7); }
long constant(void) { return number(19 + 23); }
static __attribute__((always_inline)) inline void unused(long x) { asm("" :: "i"(x)); }
"#;
    for debug in [false, true] {
        let m = compile(source, debug).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            !m.functions
                .iter()
                .any(|f| matches!(f.name.as_str(), "rip" | "outer" | "number" | "unused"))
        );
        let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
        assert!(text.contains("global(%rip)"), "{text}");
        assert!(text.contains("other+7(%rip)"), "{text}");
        assert!(text.contains("$42"), "{text}");
        let p = keel_codegen::x86_64::compile_program_with_pipeline(
            &m,
            &keel_codegen::x86_64::REGISTERS,
            &CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &AllocationTier::Full,
            },
        )
        .unwrap();
        let a = keel_asm::x86::assemble_object(&text).unwrap();
        let b =
            keel_asm::x86::assemble_program_object_with_features(&p, Features::default()).unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn deferred_immediates_reject_runtime_symbols_for_n_and_failed_required_calls() {
    for (source, expected) in [
        (
            r#"void f(long x) { asm("" :: "i"(x)); }"#,
            "constant immediate",
        ),
        (
            r#"void f(void) { long x=7; asm("" :: "r"(&x) : "memory"); asm("" :: "i"(x)); }"#,
            "constant immediate",
        ),
        (
            r#"void f(volatile long *p) { asm("" :: "i"(*p)); }"#,
            "constant immediate",
        ),
        (
            r#"extern int g; void f(void) { asm("" :: "n"(&g)); }"#,
            "constant immediate",
        ),
        (
            r#"static __attribute__((always_inline)) inline void f(long x) { asm("" :: "i"(x)); } void g(long x) { f(x); }"#,
            "constant immediate",
        ),
        (
            r#"static __attribute__((always_inline)) inline void f(long x) { f(x); asm("" :: "i"(x)); } void g(void) { f(1); }"#,
            "recursive",
        ),
        (
            r#"__attribute__((always_inline)) void f(long); void g(void) { f(1); asm(""); }"#,
            "no definition",
        ),
        (
            r#"static __attribute__((always_inline,noinline)) void f(void) { asm(""); } void g(void) { f(); }"#,
            "noinline",
        ),
        (
            r#"static __attribute__((always_inline)) inline void f(long x) { asm("" :: "i"(x)); } void (*p)(long)=f;"#,
            "constant immediate",
        ),
    ] {
        let err = compile(source, false).unwrap_err();
        assert!(err.contains(expected), "{source}: {err}");
    }
}

#[test]
fn finalized_symbol_choices_and_flags_match_clang_and_gnu() {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
    use std::{fs, process::Command};
    let source = r#"
extern unsigned char anchor[64];
static __attribute__((always_inline)) inline void *get(void *p) {
    asm("leaq %a1,%0" : "=r"(p) : "i"(p));
    return p;
}
unsigned long f(void) {
    void *p = get(anchor + 7);
    unsigned long q;
    asm("movabsq %1,%0" : "=r"(q) : "i"(anchor + 19));
    unsigned long zero, carry;
    asm("cmpq %3,%2" : "=@ccz"(zero), "=@ccc"(carry) : "r"(p), "g"(7) : "cc");
    return (unsigned long)p ^ q ^ (unsigned long)get(anchor - 7) ^ zero ^ carry;
}
"#;
    let m = compile(source, false).unwrap();
    let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
    let own = keel_asm::x86::assemble_object(&text).unwrap();
    let inspect = |bytes: &[u8]| {
        let file = object::File::parse(bytes).unwrap();
        let section = file.section_by_name(".text").unwrap();
        let relocs = section
            .relocations()
            .map(|(offset, r)| {
                let RelocationTarget::Symbol(index) = r.target() else {
                    panic!("expected symbol relocation");
                };
                (
                    offset,
                    file.symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_owned(),
                    r.addend(),
                    r.flags(),
                )
            })
            .collect::<Vec<_>>();
        (section.data().unwrap().to_vec(), relocs)
    };
    let expected = inspect(&own);
    assert!(expected.1.iter().any(|r| r.1 == "anchor" && r.2 == 3)); // +7 minus RIP's four-byte field.
    assert!(expected.1.iter().any(|r| r.1 == "anchor" && r.2 == 19));
    assert!(expected.1.iter().any(|r| r.1 == "anchor" && r.2 == -11));
    let dir = std::env::temp_dir().join(format!("keel-deferred-oracles-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    fs::write(&input, &text).unwrap();
    for gnu in [false, true] {
        let mut command = if gnu {
            let mut command =
                Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            command.args(["--64", "-mrelax-relocations=no"]);
            command
        } else {
            let mut command = Command::new("clang");
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
        assert_eq!(expected, inspect(&fs::read(&output).unwrap()), "GNU={gnu}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_outputs_use_real_addresses_and_keep_ordinary_loads_after_the_barrier() {
    let direct = compile(
        r#"void f(volatile long *p) { asm("incq %0" : "+m"(*p) :: "cc"); }"#,
        false,
    )
    .unwrap();
    let f = &direct.functions[0];
    assert!(!f.layout_insts().any(|(_, i)| matches!(
        f.inst_data(i),
        InstData::Load { .. } | InstData::Store { .. }
    )));
    let inst = f
        .layout_insts()
        .find_map(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }).then_some(i))
        .unwrap();
    assert!(f.inst_results(inst).is_empty());
    let source = r#"
unsigned long f(unsigned long *p) {
    unsigned long before = *p, reg = 7, out;
    asm("addq %4,%1; addq %1,%0; movq %0,%3; movq %3,%2"
        : "+m"(p[0]), "+&r"(reg), "=m"(p[1]), "=r"(out) : "r"(before) : "cc");
    return before + *p + reg + out;
}
"#;
    for debug in [false, true] {
        let original = compile(source, debug).unwrap();
        for optimize in [false, true] {
            let mut m = original.clone();
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            let f = &m.functions[0];
            let asm_inst = f
                .layout_insts()
                .find_map(|(_, i)| {
                    matches!(f.inst_data(i), InstData::InlineAsm { .. }).then_some(i)
                })
                .unwrap();
            let InstData::InlineAsm { asm, args } = f.inst_data(asm_inst) else {
                unreachable!()
            };
            assert_eq!(asm.outputs.len(), 2);
            assert_eq!(asm.early_clobbers, [0]);
            assert_eq!(
                &asm.inputs[..2],
                &[AsmOperand::MemReadWrite, AsmOperand::MemWrite]
            );
            assert_eq!(asm.inputs[3], AsmOperand::Tied(0));
            for arg in &args[..2] {
                assert_eq!(f.value_type(f.resolve(*arg)), keel_ir::Type::Ptr);
            }
            let mut after = false;
            assert!(f.layout_insts().any(|(_, i)| {
                if i == asm_inst {
                    after = true;
                }
                after && matches!(f.inst_data(i), InstData::Load { .. })
            }));
            let text = m.to_string();
            keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
        }
    }
}

#[test]
fn invalid_memory_outputs_are_diagnosed() {
    for (source, expected) in [
        (
            r#"void f(void) { const long x=1; asm("" : "+m"(x)); }"#,
            "modifiable",
        ),
        (
            r#"void f(void) { const long x[2]={1,2}; asm("" : "+m"(x)); }"#,
            "modifiable",
        ),
        (
            r#"void f(void) { _Atomic(long) x=1; asm("" : "+m"(x)); }"#,
            "non-atomic",
        ),
        (
            r#"struct S { int b:3; }; void f(struct S *s) { asm("" : "+m"(s->b)); }"#,
            "bit-field",
        ),
    ] {
        let err = compile(source, false).unwrap_err();
        assert!(err.contains(expected), "{source}: {err}");
    }
    let m = compile(r#"void f(long *p) { asm("incq %q0" : "+m"(*p)); }"#, false).unwrap();
    assert!(
        keel_codegen::compile(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string()
            .contains("memory modifier")
    );
}

#[test]
fn q_register_contracts_normalize_to_the_long_mode_gpr_class() {
    let m = compile(SOURCE, false).unwrap();
    let f = m.functions.iter().find(|f| f.name == "asm_q_rex").unwrap();
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
    assert_eq!(asm.outputs, [(AsmOperand::Reg, keel_ir::Type::I8)]);
    assert_eq!(asm.early_clobbers, [0]);
    assert_eq!(asm.inputs, [AsmOperand::Reg, AsmOperand::Reg]);
    let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
    // All six non-stack legacy GPRs are clobbered. The byte operands must use
    // the extended register views admitted by q in 64-bit mode.
    assert!(text.contains("movb %r9b,%r8b; addb %r10b,%r8b"), "{text}");
    for source in [
        r#"void f(double x) { asm("" : "=q"(x)); }"#,
        r#"void f(__int128 x) { asm("" :: "q"(x)); }"#,
        r#"void f(long x) { asm("" : "=Q"(x)); }"#,
    ] {
        assert!(compile(source, false).is_err(), "{source}");
    }
    let source = r#"void f(long x) { asm("" : "+q"(x)); }"#;
    let err = keel_c::compile(
        source,
        "q-aarch64.c",
        keel_c::Options {
            target: Target::Aarch64AppleDarwin,
            debug_info: false,
        },
    )
    .unwrap_err();
    assert!(err.message.contains("output constraint"), "{err}");
}

#[test]
fn address_and_negation_modifiers_check_operand_kinds() {
    for (source, expected) in [
        (
            r##"void f(char x) { asm("# %a0" :: "r"(x)); }"##,
            "32- or 64-bit register",
        ),
        (
            r##"void f(long x) { asm("# %n0" :: "r"(x)); }"##,
            "register modifier",
        ),
        (
            r##"void f(long x) { asm("# %a0" :: "m"(x)); }"##,
            "memory modifier",
        ),
        (
            r##"extern char anchor; void f(void) { asm("# %n0" :: "i"(&anchor)); }"##,
            "numeric constant",
        ),
        (
            r##"extern _Thread_local long x; void f(void) { asm("# %a0" :: "i"(&x)); }"##,
            "TLS symbols",
        ),
    ] {
        let m = compile(source, false).unwrap();
        let err = keel_codegen::compile(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{source}: {err}");
    }
    let source = r#"
unsigned long f(unsigned long x) {
    unsigned long out;
    asm("leaq %a1,%0" : "=a"(out) : "c"((unsigned int)x));
    asm("leaq %n[delta]0(%0),%0" : "+a"(out) : [delta] "n"(3));
    return out;
}
"#;
    let m = compile(source, false).unwrap();
    let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
    assert!(text.contains("leaq (%ecx),%rax"), "{text}");
    assert!(text.contains("leaq -30(%rax),%rax"), "{text}");
    keel_asm::x86::assemble_object(&text).unwrap();
}

#[test]
fn choices_keep_optional_immediates_until_backend_selection() {
    let source = r#"
static __attribute__((always_inline)) inline long add(long x, long n) {
    asm("addq %1,%0" : "+rm"(x) : "iq"(n) : "cc"); return x;
}
long f(long x) { return add(add(x, 17+6), x); }
"#;
    for optimize in [false, true] {
        let mut m = compile(source, false).unwrap();
        if optimize {
            keel_opt::optimize(&mut m).unwrap();
        }
        keel_ir::verify_module(&m).unwrap();
        let f = m.functions.iter().find(|f| f.name == "f").unwrap();
        let contracts: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                InstData::InlineAsm { asm, .. } => Some(asm),
                _ => None,
            })
            .collect();
        assert_eq!(contracts.len(), 2);
        assert!(
            contracts
                .iter()
                .all(|asm| asm.inputs[0] == AsmOperand::RegImm)
        );
        let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
        assert!(text.contains("addq $23,"), "{text}");
        assert!(text.contains("addq %"), "{text}");
        keel_asm::x86::assemble_object(&text).unwrap();
        let roundtrip = keel_ir::parse(&m.to_string()).unwrap();
        keel_ir::verify_module(&roundtrip).unwrap();
    }
    for choice in [
        "iq", "qi", "ri", "ir", "rm", "mr", "qm", "mq", "irm", "mri", "g",
    ] {
        let source = format!(
            "long f(long x) {{ long out; asm(\"movq %1,%0\" : \"=r\"(out) : \"{choice}\"(x)); return out; }}"
        );
        let m = compile(&source, false).unwrap();
        let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_object(&text).unwrap();
    }
    let source = r##"void f(void) { asm("# %c0" :: "iq"(42) : "rax","rcx","rdx","rbx","rsi","rdi","r8","r9","r10","r11","r12","r13","r14","r15"); }"##;
    let m = compile(source, false).unwrap();
    assert!(
        keel_codegen::compile(&m, Target::X86_64LinuxGnu)
            .unwrap()
            .contains("# 42")
    );
}

#[test]
fn flags_are_typed_boolean_results_with_generated_conditions() {
    let mut m = compile(SOURCE, false).unwrap();
    let f = m
        .functions
        .iter()
        .find(|f| f.name == "asm_flag_conditions")
        .unwrap();
    let asm = f
        .layout_insts()
        .find_map(|(_, i)| match f.inst_data(i) {
            InstData::InlineAsm { asm, .. } => Some(asm),
            _ => None,
        })
        .unwrap();
    assert_eq!(asm.outputs.len(), 16);
    for (cc, &(kind, ty)) in asm.outputs.iter().enumerate() {
        assert_eq!(kind, AsmOperand::Flag(cc as u8));
        assert_eq!(ty, keel_ir::Type::I64);
    }
    assert!(asm.early_clobbers.is_empty());
    let flag_only = compile(
        r#"long f(long x) { long z; asm("testq %1,%1" : "=@ccz"(z) : "r"(x)); return z; }"#,
        false,
    )
    .unwrap();
    assert!(
        keel_codegen::compile(&flag_only, Target::Aarch64AppleDarwin)
            .unwrap_err()
            .to_string()
            .contains("x86 constraint")
    );
    let f = m
        .functions
        .iter_mut()
        .find(|f| f.name == "asm_flag_conditions")
        .unwrap();
    let i = f
        .layout_insts()
        .find_map(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }).then_some(i))
        .unwrap();
    let InstData::InlineAsm { asm, .. } = f.inst_data_mut(i) else {
        unreachable!()
    };
    asm.outputs[0].0 = AsmOperand::Flag(16);
    assert!(keel_ir::verify_module(&m).is_err());
}

#[test]
fn malformed_flag_and_choice_contracts_are_diagnosed() {
    for (body, expected) in [
        (r#"asm("" : "+@ccz"(x));"#, "require ="),
        (r#"asm("" : "=&@ccz"(x));"#, "require ="),
        (r#"asm("" : "=@ccwat"(x));"#, "unknown x86 flag condition"),
        (r#"asm("" : "=@cc"(x));"#, "unknown x86 flag condition"),
        (r#"asm("" : "=@ccz,@cce"(x));"#, "alternatives"),
        (r#"asm("%0" : "=@ccz"(x));"#, "cannot be referenced"),
        (
            r#"asm("%c[flag]" : [flag] "=@ccz"(x));"#,
            "cannot be referenced",
        ),
        (r#"asm("" : "=@ccz"(x) : "0"(x));"#, "register output"),
        (r#"asm("" :: "iq,m"(x));"#, "input constraint"),
        (r#"asm("" :: "rr"(x));"#, "input constraint"),
    ] {
        let source = format!("void f(long x){{{body}}}");
        let err = compile(&source, false).unwrap_err();
        assert!(err.contains(expected), "{source}: {err}");
    }
    for source in [
        r#"void f(double x) { asm("" : "=@ccz"(x)); }"#,
        r#"void f(void *x) { asm("" : "=@ccz"(x)); }"#,
        r#"void f(__int128 x) { asm("" : "=@ccz"(x)); }"#,
        r#"void f(void) { const long x=1; asm("" : "=@ccz"(x)); }"#,
        r#"void f(void) { _Atomic(long) x=1; asm("" : "=@ccz"(x)); }"#,
        r#"struct S { int x:3; }; void f(struct S *p) { asm("" : "=@ccz"(p->x)); }"#,
        r#"struct S { long x[3]; }; void f(struct S *p) { asm("" : "+rm"(*p)); }"#,
        r#"void f(double x) { asm("" :: "iq"(x)); }"#,
    ] {
        assert!(compile(source, false).is_err(), "{source}");
    }
    let m = compile(r##"void f(long x) { asm("# %c0" :: "iq"(x)); }"##, false).unwrap();
    assert!(
        keel_codegen::compile(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string()
            .contains("register modifier")
    );
}

#[test]
fn flag_output_capability_is_advertised_only_for_x86() {
    for target in [
        Target::X86_64LinuxGnu,
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
    ] {
        let macros = keel_c::predefined_macros(target, None).unwrap();
        assert_eq!(
            macros.contains("#define __GCC_ASM_FLAG_OUTPUTS__ 1"),
            target == Target::X86_64LinuxGnu
        );
    }
    compile("#ifndef __GCC_ASM_FLAG_OUTPUTS__\n#error missing flag output capability\n#endif\nint f(long x) { int z; asm(\"testq %1,%1\" : \"=@ccz\"(z) : \"r\"(x)); return z; }", false).unwrap();
}

#[test]
fn machine_immediate_choices_select_by_range_after_required_inlining() {
    for optimize in [false, true] {
        for (constraint, kind, cases) in [
            (
                "er",
                AsmOperand::RegSignedImm32,
                vec![
                    (-2147483649i64, false),
                    (-2147483648, true),
                    (-1, true),
                    (0, true),
                    (2147483647, true),
                    (2147483648, false),
                    (4294967295, false),
                    (i64::MIN, false),
                    (i64::MAX, false),
                ],
            ),
            (
                "Ir",
                AsmOperand::RegShiftImm5,
                vec![
                    (-1, false),
                    (0, true),
                    (31, true),
                    (32, false),
                    (63, false),
                    (i64::MAX, false),
                ],
            ),
        ] {
            for (value, immediate) in cases {
                let source = format!(
                    r#"
static __attribute__((always_inline)) inline long pick(long n) {{
    long out; asm("movq %1,%0" : "=r"(out) : "{constraint}"(n)); return out;
}}
long f(void) {{ return pick({value}L); }}
"#
                );
                let mut m = compile(&source, false).unwrap();
                if optimize {
                    keel_opt::optimize(&mut m).unwrap();
                }
                keel_ir::verify_module(&m).unwrap();
                let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                let asm = f
                    .layout_insts()
                    .find_map(|(_, i)| match f.inst_data(i) {
                        InstData::InlineAsm { asm, .. } => Some(asm),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(asm.inputs, [kind]);
                let roundtrip = keel_ir::parse(&m.to_string()).unwrap();
                keel_ir::verify_module(&roundtrip).unwrap();
                let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
                assert_eq!(
                    text.contains(&format!("movq ${value},")),
                    immediate,
                    "{constraint} {value}: {text}"
                );
                keel_asm::x86::assemble_object(&text).unwrap();
            }
        }
    }
    for constraint in ["er", "re", "Ir", "rI"] {
        let source = format!(
            r#"long f(long x) {{ long out; asm("movq %1,%0" : "=r"(out) : "{constraint}"(x)); return out; }}"#
        );
        let m = compile(&source, false).unwrap();
        let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
        assert!(!text.contains("movq $"), "{text}");
        keel_asm::x86::assemble_object(&text).unwrap();
        assert!(
            compile(
                &format!("void f(double x) {{ asm(\"\" :: \"{constraint}\"(x)); }}"),
                false
            )
            .is_err()
        );
    }
    // A symbol's signed-32-bit range is not established by the default code model.
    let m = compile(r#"extern char anchor; void *f(void) { void *out; asm("movq %1,%0" : "=r"(out) : "er"(&anchor)); return out; }"#, false).unwrap();
    let text = keel_codegen::compile(&m, Target::X86_64LinuxGnu).unwrap();
    assert!(!text.contains("movq $anchor"), "{text}");
    keel_asm::x86::assemble_object(&text).unwrap();
    for constraint in ["e", "I", "eer", "IIr", "eIr", "er,m"] {
        assert!(
            compile(
                &format!("void f(long x) {{ asm(\"\" :: \"{constraint}\"(x)); }}"),
                false
            )
            .is_err()
        );
    }
}
