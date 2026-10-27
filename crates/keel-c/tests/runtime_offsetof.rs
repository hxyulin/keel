//! Offset designators compute integer byte offsets without accessing an object.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use keel_ir::{InstData, Module, Target, Type};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

const SOURCE: &str = include_str!("fixtures/runtime_offsetof.c");
const RUNNER: &str = include_str!("fixtures/runtime_offsetof_runner.c");

#[test]
fn constant_offsets_cannot_hide_evaluated_effects_inside_index_wrappers() {
    for index in [
        "__builtin_expect((++changes, 2), 1)",
        "&values[(++changes, 2)] - &values[0]",
        "(int)__real__ __builtin_complex((++changes, 2.0), 0.0)",
    ] {
        let source = format!(
            "int changes, values[4]; struct S {{ long a[4]; }}; \
             _Static_assert(__builtin_offsetof(struct S, a[{index}]) == 16, \"not constant\");"
        );
        let error = compile(&source, Target::X86_64LinuxGnu).unwrap_err();
        assert!(
            error.message.contains("constant expression"),
            "{index}: {error}"
        );
    }
}

fn compile(source: &str, target: Target) -> Result<Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "runtime-offsetof.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

fn object(module: &Module, target: Target) -> Vec<u8> {
    let text = keel_codegen::compile(module, target).unwrap();
    let program = keel_codegen::compile_program(module, target).unwrap();
    let (typed, textual) = match target {
        Target::Aarch64AppleDarwin => (
            keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
            keel_asm::assemble_object(&text).unwrap(),
        ),
        Target::Aarch64LinuxGnu => (
            keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap(),
            keel_asm::assemble_elf_object(&text).unwrap(),
        ),
        Target::X86_64LinuxGnu => (
            keel_asm::x86::assemble_program_object(&program).unwrap(),
            keel_asm::x86::assemble_object(&text).unwrap(),
        ),
        _ => unreachable!(),
    };
    assert_eq!(typed, textual, "{target:?}");
    typed
}

fn execute(dir: &Path, compiler: &str, input: &Path, reference: bool) {
    let executable = dir.join(format!("run-{compiler}-{reference}"));
    let mut command = Command::new(compiler);
    command.arg("-O2");
    if reference {
        command.arg("-DOFFSETOF_REFERENCE");
    }
    let run = command
        .arg(input)
        .arg(dir.join("main.c"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{compiler}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    assert!(
        run.status.success(),
        "{compiler}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, b"runtime-offsetof PASS\n");
}

#[test]
fn offsets_preserve_native_layout_and_runtime_index_effects() {
    for target in Target::NATIVE {
        for optimize in [false, true] {
            let mut module = compile(SOURCE, target).unwrap();
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            for name in ["runtime_offset", "anonymous_offset", "flexible_offset"] {
                let f = module.functions.iter().find(|f| f.name == name).unwrap();
                assert!(
                    f.layout_insts()
                        .all(|(_, i)| !matches!(f.inst_data(i), InstData::Load { .. })),
                    "offset calculation dereferenced memory: {name}"
                );
            }
            let module = keel_ir::parse(&module.to_string()).unwrap();
            let bytes = object(&module, target);
            if Target::host() == Some(target) {
                let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("runtime-offsetof-{optimize}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("keel.o"), bytes).unwrap();
                fs::write(dir.join("source.c"), SOURCE).unwrap();
                fs::write(dir.join("main.c"), RUNNER).unwrap();
                let compilers: &[&str] = if target == Target::X86_64LinuxGnu {
                    &["gcc", "clang"]
                } else {
                    &["clang"]
                };
                for compiler in compilers {
                    execute(&dir, compiler, &dir.join("keel.o"), false);
                    execute(&dir, compiler, &dir.join("source.c"), true);
                }
            }
        }
    }
}

#[test]
fn x86_offset_objects_have_text_typed_and_chunked_parity() {
    let features = Features::for_level(CpuLevel::V1);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let export = std::env::var_os("KEEL_RUNTIME_OFFSETOF_EXPORT").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("source.c"), SOURCE).unwrap();
        fs::write(dir.join("main.c"), RUNNER).unwrap();
    }
    for optimize in [false, true] {
        let mut module = compile(SOURCE, Target::X86_64LinuxGnu).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
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
                    if let Some(dir) = &export {
                        fs::write(
                            dir.join(format!(
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
fn invalid_designators_and_nonconstant_uses_are_diagnosed() {
    for (source, expected) in [
        (
            "struct S { int *p; }; unsigned long f(int i){return __builtin_offsetof(struct S,p[i]);}",
            "non-array",
        ),
        (
            "struct S { int a[3]; }; unsigned long f(void){return __builtin_offsetof(struct S,a[1.5]);}",
            "must be an integer",
        ),
        (
            "struct S { int b:2; }; unsigned long f(void){return __builtin_offsetof(struct S,b);}",
            "bit-field",
        ),
        (
            "struct S { int a; }; unsigned long f(void){return __builtin_offsetof(struct S,missing);}",
            "no member",
        ),
        (
            "struct S { int a; }; unsigned long f(void){return __builtin_offsetof(struct S,);}",
            "member designator",
        ),
        (
            "struct S { int a; }; unsigned long f(void){return __builtin_offsetof(struct S,a[1]);}",
            "non-array",
        ),
        (
            "struct S { int a[4]; }; int i; _Static_assert(__builtin_offsetof(struct S,a[i])==0,\"runtime offset\");",
            "constant expression",
        ),
        (
            "struct S { int a[4]; }; int i; _Static_assert(__builtin_offsetof(struct S,a[(i++,2)])==8,\"side effect\");",
            "constant expression",
        ),
        (
            "struct S { int a[4]; }; int i; unsigned long bad=__builtin_offsetof(struct S,a[i]);",
            "initializer is not a constant",
        ),
        (
            "struct S { int a[4]; }; int i; unsigned long bad=__builtin_offsetof(struct S,a[(i++,2)]);",
            "initializer is not a constant",
        ),
    ] {
        let error = compile(source, Target::X86_64LinuxGnu).unwrap_err();
        assert!(error.message.contains(expected), "{source}: {error}");
    }
    let source = "int side_effect; int f(void){return __builtin_constant_p((++side_effect,2));}";
    let module = compile(source, Target::X86_64LinuxGnu).unwrap();
    let f = module.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(f.layout_insts().all(|(_, i)| !matches!(
        f.inst_data(i),
        InstData::Store { .. } | InstData::Load { .. }
    )));
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Iconst { imm: 0, .. }))
    );
}

#[test]
fn wasm_offsets_use_the_private_unsigned_size_domain() {
    let source = r#"
struct Leaf { short x; unsigned long y; };
struct Record { char c; struct Leaf rows[4][3]; };
_Static_assert(sizeof(__builtin_offsetof(struct Record,rows[1][2].y))==4,"size_t width");
_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_offsetof(struct Record,rows[1][2].y)),unsigned long),"size_t type");
_Static_assert(__builtin_offsetof(struct Record,rows[1][2].y)==48,"private layout");
unsigned long offset(int i,long long j){return __builtin_offsetof(struct Record,rows[i][j].y);}
unsigned long negative(void){return __builtin_offsetof(struct Record,rows[-1][-1].y);}
"#;
    let module = compile(source, Target::Wasm32UnknownUnknown).unwrap();
    keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).unwrap();
    assert!(
        module
            .functions
            .iter()
            .all(|f| f.sig.returns[0].ty == Type::I32)
    );
    let bytes = keel_wasm::emit_wasm32_runtime_module(
        &module,
        &keel_wasm::Wasm32RuntimeOptions {
            memory: keel_wasm::Wasm32MemoryOptions {
                scalar: keel_wasm::Options {
                    exports: ["offset", "negative"]
                        .into_iter()
                        .map(|name| keel_wasm::Export {
                            name: name.into(),
                            function: name.into(),
                        })
                        .collect(),
                },
                pages: 1,
                export_name: "memory".into(),
            },
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        },
    )
    .unwrap();
    let script = r#"const assert=require('assert/strict');const b=require('fs').readFileSync(0);const e=new WebAssembly.Instance(new WebAssembly.Module(b)).exports;
for(let i=-20;i<20;i++)for(const j of [-17n,-1n,0n,7n,4294967299n,9223372036854775807n,-9223372036854775808n])assert.equal(e.offset(i,j)>>>0,Number(BigInt.asUintN(32,8n+24n*BigInt(i)+8n*j)));
assert.equal(e.negative()>>>0,Number(BigInt.asUintN(32,8n-24n-8n)));"#;
    let mut node = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    node.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = node.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
