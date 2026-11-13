//! Direct functions share object, ABI, unwind and fallback contracts with normal codegen.
use keel_asm_ir::{Directive, Item, Program};
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, UnwindTables,
    backend_profile, regalloc::AllocationTier,
};
use object::{Object, ObjectSection};

const LEAF: &str = "func @leaf(i64,i64)->i64 {\nblock0(v0:i64,v1:i64):\n v2=iconst.i64 -1\n v3=xor v0,v2\n v4=imul v3,v1\n return v4\n}\n";
fn request(target: Target, unwind: UnwindTables) -> Request<'static> {
    let mut emission = EmissionOptions::default();
    emission.backend_mode = BackendMode::CompileSpeed;
    emission.unwind_tables = unwind;
    Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .emission(emission)
}
fn direct_count(program: &Program) -> usize {
    program
        .items
        .iter()
        .filter(|i| matches!(i, Item::Directive(Directive::Bytes(_))))
        .count()
}
fn parse(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
#[test]
fn direct_objects_text_chunks_unwind_and_parallel_agree() {
    // Mix relocations with patched branches and trapping arithmetic. All output
    // routes must agree on the byte offsets around symbolic address sequences.
    let source = format!(
        "data @divisor = local align 8 {{ b\"\\x03\", zero 7 }}\n\
         {LEAF}\
         func @dispatch(i64)->i64 {{\n\
         block0(v0:i64):\n\
          v1=symbol_addr @divisor\n\
          v2=load.i64 v1\n\
          switch v0,block1(v0,v2),[0:block2(v2,v2),7:block2(v0,v2)]\n\
         block1(v3:i64,v4:i64):\n\
          v5=sdiv v3,v4\n\
          return v5\n\
         block2(v6:i64,v7:i64):\n\
          v8=urem v6,v7\n\
          return v8\n\
         }}\n\
         func @fallback(f64)->f64 {{\nblock0(v0:f64):\n return v0\n}}\n"
    );
    let module = parse(&source);
    for target in [
        Target::Aarch64AppleDarwin,
        Target::Aarch64LinuxGnu,
        Target::X86_64LinuxGnu,
    ] {
        let mut images = Vec::new();
        for unwind in [UnwindTables::All, UnwindTables::RequiredOnly] {
            let req = request(target, unwind);
            let (result, report) = backend_profile::capture(|| req.program(&module));
            let program = result.unwrap();
            assert!(direct_count(&program) > 0);
            assert_eq!(
                report.counters[backend_profile::Counter::DirectFunctions as usize],
                2
            );
            let bytes = keel_asm::assemble_program_for(target, &program).unwrap();
            assert_eq!(
                bytes,
                keel_asm::assemble_source_for(target, &req.text(&module).unwrap()).unwrap()
            );
            let mut chunks = Program::new();
            req.program_chunks(&module, &mut |p| chunks.append(p))
                .unwrap();
            assert_eq!(
                bytes,
                keel_asm::assemble_program_for(target, &chunks).unwrap()
            );
            if target == Target::X86_64LinuxGnu {
                let assembler = req
                    .assemble_x86(
                        &module,
                        keel_asm::x86::ProgramAssembler::new(Default::default()),
                        Box::new(|name| Err(format!("unexpected binary input {name}"))),
                    )
                    .unwrap();
                assert_eq!(bytes, assembler.finish().unwrap());
            }

            let mut parallel = Program::new();
            std::thread::scope(|scope| {
                keel_codegen::parallel::program_chunks(
                    &module,
                    keel_codegen::parallel::Options {
                        target,
                        pipeline: keel_codegen::parallel::Pipeline {
                            selector: &SelectionTier::Baseline,
                            allocator: &AllocationTier::Fast,
                        },
                        emission: req.emission,
                        pools: None,
                        workers: std::num::NonZeroUsize::new(2).unwrap(),
                        budget: std::num::NonZeroUsize::new(2).unwrap(),
                    },
                    &|job| {
                        scope.spawn(move || job.run());
                    },
                    &mut |p| parallel.append(p),
                )
                .unwrap();
            });
            assert_eq!(
                bytes,
                keel_asm::assemble_program_for(target, &parallel).unwrap()
            );
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let section = if target == Target::Aarch64AppleDarwin {
                "__compact_unwind"
            } else {
                ".eh_frame"
            };
            assert_eq!(
                file.section_by_name(section).is_some(),
                unwind == UnwindTables::All
            );
            images.push(bytes);
        }
        let a = object::File::parse(images[0].as_slice()).unwrap();
        let b = object::File::parse(images[1].as_slice()).unwrap();
        let section = if target == Target::Aarch64AppleDarwin {
            "__text"
        } else {
            ".text"
        };
        assert_eq!(
            a.section_by_name(section).unwrap().data().unwrap(),
            b.section_by_name(section).unwrap().data().unwrap()
        );
    }
}
#[test]
fn unsupported_functions_and_debug_use_ordinary_lowering() {
    let cases=[
        "func @f(f64)->f64 {\nblock0(v0:f64):\n return v0\n}\n".to_owned(),
        "decl @callee(i64,...)->i64\nfunc @f(i64)->i64 {\nblock0(v0:i64):\n v1=call @callee(v0)\n return v1\n}\n".to_owned(),
        "func @f(ptr byval {i64})->i64 {\nblock0(v0:ptr):\n v1=iconst.i64 1\n return v1\n}\n".to_owned(),
        "func @f()->i64 {\n ss0=stack_slot 4096, align 16\nblock0:\n v0=iconst.i64 1\n return v0\n}\n".to_owned(),
        format!("file f0 = \"debug.c\"\n{LEAF}"),
        "func @f()->i64 {\nblock0:\n".to_owned()+ &(0..520).map(|i|format!(" v{i}=iconst.i64 {i}\n")).collect::<String>()+" return v519\n}\n",
    ];
    for source in cases {
        let module = parse(&source);
        let req = request(Target::Aarch64AppleDarwin, UnwindTables::All);
        let (program, report) = backend_profile::capture(|| req.program(&module));
        let program = program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            0,
            "{source}"
        );
        let ordinary = Request::new(req.target, req.pipeline)
            .program(&module)
            .unwrap();
        assert_eq!(
            keel_asm::assemble_program_for(req.target, &program).unwrap(),
            keel_asm::assemble_program_for(req.target, &ordinary).unwrap()
        );
    }
}

#[test]
fn native_arithmetic_memory_parallel_copies_and_mixed_calls_match_c() {
    use std::{fmt::Write, fs, process::Command};
    // Every condition crosses signed/unsigned boundaries and includes equality.
    let mut source = include_str!("compile_speed/leaves.keel").to_owned();
    let mut oracle = String::from("#include <stdint.h>\n");
    let mut checks = String::from(
        "int check_comparisons(void) {\nuint64_t values[]={0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff)};\n",
    );
    for (condition, operator, signed) in [
        ("eq", "==", false),
        ("ne", "!=", false),
        ("ult", "<", false),
        ("ule", "<=", false),
        ("ugt", ">", false),
        ("uge", ">=", false),
        ("slt", "<", true),
        ("sle", "<=", true),
        ("sgt", ">", true),
        ("sge", ">=", true),
    ] {
        writeln!(source,"func @cmp_{condition}(i64,i64)->i64 {{\nblock0(v0:i64,v1:i64):\n v2=icmp.{condition} v0,v1\n brif v2,block1,block2\nblock1:\n v3=iconst.i64 1\n return v3\nblock2:\n v4=iconst.i64 0\n return v4\n}}\n").unwrap();
        writeln!(
            oracle,
            "extern uint64_t cmp_{condition}(uint64_t,uint64_t);"
        )
        .unwrap();
        let cast = if signed { "(int64_t)" } else { "" };
        writeln!(checks,"for(unsigned i=0;i<5;i++)for(unsigned j=0;j<5;j++) if(cmp_{condition}(values[i],values[j])!=({cast}values[i]{operator}{cast}values[j]))return 1;").unwrap();
    }
    checks.push_str("return 0;}\n");
    oracle.push_str(&checks);
    oracle.push_str(include_str!("compile_speed/oracle.c"));
    let module = parse(&source);
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Target::Aarch64AppleDarwin
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Target::Aarch64LinuxGnu
    } else {
        keel_test_support::skip!("requires native AArch64 host");
    };
    let (result, report) =
        backend_profile::capture(|| request(target, UnwindTables::All).program(&module));
    let program = result.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        14
    );
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    };
    let dir = keel_test_support::TempDir::new("compile-speed-native");
    fs::write(
        dir.join("leaf.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    let output = clang
        .command()
        .current_dir(dir.path())
        .args(["-O2", "oracle.c", "leaf.o", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(dir.join("oracle")).status().unwrap().success());
}
