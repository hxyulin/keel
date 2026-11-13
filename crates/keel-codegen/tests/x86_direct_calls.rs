//! Direct call sites keep fixed-size branch relocations and stack-home values.
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, UnwindTables,
    backend_profile, regalloc::AllocationTier,
};
use object::{Object, ObjectSection, ObjectSymbol};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::fmt::Write;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::{collections::BTreeMap, fs, process::Command};
const SOURCE: &str = include_str!("x86_direct_calls/calls.keel");
const ORACLE: &str = include_str!("x86_direct_calls/oracle.c");
fn request() -> Request<'static> {
    let mut emission = EmissionOptions::default();
    emission.backend_mode = BackendMode::CompileSpeed;
    Request::new(
        Target::X86_64LinuxGnu,
        CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .emission(emission)
}
fn parse(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
#[test]
fn direct_calls_preserve_stream_and_parallel_relocations_and_unwind() {
    let module = parse(SOURCE);
    for unwind in [UnwindTables::All, UnwindTables::RequiredOnly] {
        let mut req = request();
        req.emission.unwind_tables = unwind;
        let (result, report) = backend_profile::capture(|| req.program(&module));
        let program = result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            6
        );
        let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
        assert_eq!(
            bytes,
            keel_asm::assemble_source_for(req.target, &req.text(&module).unwrap()).unwrap()
        );
        let stream = req
            .assemble_x86(
                &module,
                keel_asm::x86::ProgramAssembler::new(Default::default()),
                Box::new(|_| Err("no binary inputs".into())),
            )
            .unwrap();
        assert_eq!(bytes, stream.finish().unwrap());
        let mut parallel = keel_asm_ir::Program::new();
        std::thread::scope(|scope| {
            keel_codegen::parallel::program_chunks(
                &module,
                keel_codegen::parallel::Options {
                    target: req.target,
                    pipeline: keel_codegen::parallel::Pipeline {
                        selector: &SelectionTier::Baseline,
                        allocator: &AllocationTier::Fast,
                    },
                    emission: req.emission,
                    pools: None,
                    workers: std::num::NonZeroUsize::new(3).unwrap(),
                    budget: std::num::NonZeroUsize::new(3).unwrap(),
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
            keel_asm::assemble_program_for(req.target, &parallel).unwrap()
        );
        let file = object::File::parse(bytes.as_slice()).unwrap();
        assert_eq!(
            file.section_by_name(".eh_frame").is_some(),
            unwind == UnwindTables::All
        );
        let text = file.section_by_name(".text").unwrap();
        let mut names = Vec::new();
        for (offset, relocation) in text.relocations() {
            let object::RelocationTarget::Symbol(index) = relocation.target() else {
                panic!("non-symbol call relocation");
            };
            let name = file.symbol_by_index(index).unwrap().name().unwrap();
            assert_eq!(relocation.size(), 32);
            assert_eq!(relocation.addend(), -4);
            assert_eq!(text.data().unwrap()[offset as usize - 1], 0xe8);
            names.push(name.to_owned());
        }
        assert!(names.contains(&"foreign6".to_owned()));
        assert!(names.contains(&"narrow_observer".to_owned()));
    }
}
#[test]
fn call_bindings_follow_symbols_for_every_code_model() {
    let source = "decl @external()->i64\nfunc @strong()->i64 {\nblock0:\n v0=iconst.i64 1\n return v0\n}\nfunc @weak()->i64 weak {\nblock0:\n v0=iconst.i64 2\n return v0\n}\nfunc @caller()->i64 {\nblock0:\n v0=call @strong()\n v1=call @weak()\n v2=call @external()\n v3=iadd v0,v1\n v4=iadd v3,v2\n return v4\n}\n";
    let module = parse(source);
    for model in [
        keel_codegen::x86_64::CodeModel::PositionIndependent,
        keel_codegen::x86_64::CodeModel::Small,
        keel_codegen::x86_64::CodeModel::Kernel,
    ] {
        let mut req = request();
        req.emission.x86_code_model = Some(model);
        let text = req.text(&module).unwrap();
        assert!(text.contains("call\t\"strong\"\n"), "{text}");
        assert!(text.contains("call\t\"weak\"@PLT"));
        assert!(text.contains("call\t\"external\"@PLT"));
    }
}
#[test]
fn unsupported_call_shapes_fall_back() {
    let cases = [
        "decl @f(i64,...)->i64\nfunc @caller()->i64 {\nblock0:\n v0=iconst.i64 1\n v1=call @f(v0)\n return v1\n}\n",
        "decl @f()->i64 returns_twice\nfunc @caller()->i64 {\nblock0:\n v0=call @f()\n return v0\n}\n",
        "decl @f(ptr byval {i64})->i64\nfunc @caller()->i64 {\n ss0=stack_slot 8,align 8\nblock0:\n v0=stack_addr ss0\n v1=call @f(v0)\n return v1\n}\n",
    ];
    for source in cases {
        let module = parse(source);
        let req = request();
        let (result, report) = backend_profile::capture(|| req.program(&module));
        result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            0
        );
    }
    let mut module = parse(
        "decl @f(i64)->i64\nfunc @caller()->i64 {\nblock0:\n v0=iconst.i64 1\n v1=call @f(v0)\n return v1\n}\n",
    );
    let function = &mut module.functions[0];
    let call = function
        .layout_insts()
        .find_map(|(_, inst)| {
            matches!(function.inst_data(inst), keel_ir::InstData::Call { .. }).then_some(inst)
        })
        .unwrap();
    if let keel_ir::InstData::Call { args, .. } = function.inst_data_mut(call) {
        *args = keel_ir::CallArgs::with_abi(
            args.iter()
                .copied()
                .map(|value| (value, keel_ir::AbiParam::new(keel_ir::Type::I64))),
        );
    }
    keel_ir::verify_module(&module).unwrap();
    let (result, report) = backend_profile::capture(|| request().program(&module));
    result.unwrap();
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
}
#[test]
fn call_oracle_executes_actual_bytes_under_rosetta_and_exports_linux() {
    let module = parse(SOURCE);
    let req = request();
    let program = req.program(&module).unwrap();
    let bytes = keel_asm::assemble_program_for(req.target, &program).unwrap();
    keel_test_support::native_program!("direct-calls")
        .file("calls.keel", SOURCE)
        .file("calls.o", &bytes)
        .file("oracle.c", ORACLE)
        .link(["-O2", "oracle.c", "calls.o"])
        .check();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if !Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .unwrap()
            .success()
        {
            keel_test_support::skip!("Rosetta unavailable for x86 call byte oracle");
        }
        // Preserve the entire ELF text layout. Replace only five-byte symbolic
        // CALL relocations with Mach-O calls and prefix external C symbol names.
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let section = file.section_by_name(".text").unwrap();
        let data = section.data().unwrap();
        let mut labels = BTreeMap::<usize, Vec<String>>::new();
        for symbol in file
            .symbols()
            .filter(|s| s.kind() == object::SymbolKind::Text && s.size() > 0)
        {
            labels
                .entry(symbol.address() as usize)
                .or_default()
                .push(symbol.name().unwrap().to_owned());
        }
        let mut calls = BTreeMap::new();
        for (offset, relocation) in section.relocations() {
            let object::RelocationTarget::Symbol(index) = relocation.target() else {
                panic!("non-symbol relocation");
            };
            assert_eq!(relocation.size(), 32);
            assert_eq!(relocation.addend(), -4);
            assert_eq!(data[offset as usize - 1], 0xe8);
            calls.insert(
                offset as usize - 1,
                file.symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned(),
            );
        }
        let mut assembly = String::from(".text\n.p2align 4\n");
        let mut position = 0;
        while position < data.len() {
            if let Some(names) = labels.get(&position) {
                for name in names {
                    writeln!(assembly, ".globl _{name}\n_{name}:").unwrap();
                }
            }
            if let Some(name) = calls.get(&position) {
                writeln!(assembly, "call _{name}").unwrap();
                position += 5;
                continue;
            }
            writeln!(assembly, ".byte {}", data[position]).unwrap();
            position += 1;
        }
        let dir = keel_test_support::TempDir::new("x86-direct-calls-rosetta");
        fs::write(dir.join("calls.s"), assembly).unwrap();
        fs::write(dir.join("oracle.c"), ORACLE).unwrap();
        let Some(clang) = keel_test_support::clang() else {
            keel_test_support::skip!(keel_test_support::NO_CLANG);
        };
        let result = clang
            .command()
            .current_dir(dir.path())
            .args([
                "-arch", "x86_64", "-O2", "oracle.c", "calls.s", "-o", "oracle",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            Command::new("arch")
                .arg("-x86_64")
                .arg(dir.join("oracle"))
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn direct_call_cfi_reaches_both_callers() {
    let source = "decl @trace()->i64\nfunc @entry()->i64 {\nblock0:\n v0=call @middle()\n return v0\n}\nfunc @middle()->i64 {\nblock0:\n v0=call @trace()\n return v0\n}\n";
    let oracle = r#"#include <stdint.h>
#include <unwind.h>
extern uint64_t entry(void),middle(void);
static unsigned seen;
static _Unwind_Reason_Code visit(struct _Unwind_Context *context,void *argument) {
 (void)argument;uintptr_t start=(uintptr_t)_Unwind_GetRegionStart(context);
 if(start==(uintptr_t)entry)seen|=1;
 if(start==(uintptr_t)middle)seen|=2;
 return _URC_NO_REASON;
}
uint64_t trace(void){seen=0;_Unwind_Backtrace(visit,0);return seen;}
int main(void){return entry()!=3;}
"#;
    let module = parse(source);
    let req = request();
    let bytes = keel_asm::assemble_program_for(req.target, &req.program(&module).unwrap()).unwrap();
    keel_test_support::native_program!("direct-call-unwind")
        .file("calls.keel", source)
        .file("calls.o", bytes)
        .file("oracle.c", oracle)
        .link(["-O2", "oracle.c", "calls.o"])
        .check();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if !Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .unwrap()
            .success()
        {
            keel_test_support::skip!("Rosetta unavailable for x86 direct unwind oracle");
        }
        // Keep emitted bytes and CFI together. Only ELF section/type/size wrappers
        // and symbol spelling change for Mach-O; direct CALLs remain five bytes.
        let text = req.text(&module).unwrap();
        let mut assembly = String::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(".type") || trimmed.starts_with(".size") {
                continue;
            }
            let mut line = if trimmed.starts_with(".section") {
                ".text".to_owned()
            } else {
                line.replace("@PLT", "")
            };
            for name in ["entry", "middle", "trace"] {
                line = line.replace(&format!("\"{name}\""), &format!("\"_{name}\""));
                line = line.replace(&format!(".globl {name}"), &format!(".globl _{name}"));
                if line.trim() == format!("{name}:") {
                    line = format!("_{name}:");
                }
            }
            writeln!(assembly, "{line}").unwrap();
        }
        let dir = keel_test_support::TempDir::new("x86-direct-unwind-rosetta");
        fs::write(dir.join("calls.s"), assembly).unwrap();
        fs::write(dir.join("oracle.c"), oracle).unwrap();
        let Some(clang) = keel_test_support::clang() else {
            keel_test_support::skip!(keel_test_support::NO_CLANG);
        };
        let result = clang
            .command()
            .current_dir(dir.path())
            .args([
                "-arch", "x86_64", "-O2", "oracle.c", "calls.s", "-o", "oracle",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            Command::new("arch")
                .arg("-x86_64")
                .arg(dir.join("oracle"))
                .status()
                .unwrap()
                .success()
        );
    }
}
