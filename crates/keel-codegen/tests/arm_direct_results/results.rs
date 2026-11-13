//! Scalar multi-result ABI boundaries, including C struct padding and Fast registers.
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, UnwindTables,
    backend_profile, regalloc::AllocationTier,
};
use std::{fmt::Write, fs, process::Command};

fn request(target: Target, speed: bool) -> Request<'static> {
    let mut emission = EmissionOptions::default();
    if speed {
        emission.backend_mode = BackendMode::CompileSpeed;
    }
    emission.unwind_tables = UnwindTables::All;
    Request::new(
        target,
        CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .emission(emission)
}

fn module() -> keel_ir::Module {
    let mut source = include_str!("../programs/multi_results.keel").to_owned();
    source.push_str(include_str!("extra.keel"));
    for (name, large) in [("fast_named", false), ("fast_large", true)] {
        writeln!(
            source,
            "func @{name}(ptr,i64)->i64 {{\nblock0(v0:ptr,v1:i64):"
        )
        .unwrap();
        if large {
            // The result homes cross the largest scaled 64-bit SP offset (32760).
            for i in 0..4096 {
                writeln!(source, " padding{i}=iconst.i64 0").unwrap();
            }
        }
        source.push_str(" r0,r1,r2,r3,r4,r5,r6,r7=call @fast8(v0,v1)\n");
        for (i, weight) in [1, 3, 5, 7, 11, 13, 17, 19].into_iter().enumerate() {
            let operand = match i {
                3 | 5 => format!("r{i}"),
                4 => {
                    writeln!(source, " x{i}=load.i64 r{i}").unwrap();
                    format!("x{i}")
                }
                _ => {
                    writeln!(source, " x{i}=uext.i64 r{i}").unwrap();
                    format!("x{i}")
                }
            };
            writeln!(
                source,
                " w{i}=iconst.i64 {weight}\n p{i}=imul {operand},w{i}"
            )
            .unwrap();
            if i == 1 {
                source.push_str(" s1=iadd p0,p1\n");
            }
            if i > 1 {
                writeln!(source, " s{i}=iadd s{},p{i}", i - 1).unwrap();
            }
        }
        source.push_str(" return s7\n}\n");
    }
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn scalar_results_keep_shared_abi_and_object_parity() {
    let module = module();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let request = request(target, true);
        let (result, report) = backend_profile::capture(|| request.program(&module));
        let program = result.unwrap();
        // Floating-point, tail-call and rotate bodies deliberately exercise fallback.
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            module.functions.len() as u64 - 8
        );
        let object = keel_asm::assemble_program_for(target, &program).unwrap();
        assert_eq!(
            object,
            keel_asm::assemble_source_for(target, &request.text(&module).unwrap()).unwrap()
        );
    }
}

#[test]
fn native_scalar_results_match_c_in_all_registers_and_large_homes() {
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Target::Aarch64AppleDarwin
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Target::Aarch64LinuxGnu
    } else {
        keel_test_support::skip!("requires native AArch64 host");
    };
    let Some(clang) = keel_test_support::clang() else {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    };
    let program = request(target, true).program(&module()).unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct-results");
    fs::write(
        dir.join("results.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    let oracle = format!(
        "#define main original_main\n{}\n#undef main\n{}",
        include_str!("../programs/multi_results.c"),
        include_str!("oracle.c")
    );
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    let out = clang
        .command()
        .current_dir(dir.path())
        .args(["-O2", "oracle.c", "results.o", "-o", "oracle"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(dir.join("oracle")).output().unwrap();
    assert!(
        out.status.success(),
        "exit {:?}: {}{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn invalid_scalar_result_contracts_keep_ordinary_errors() {
    for source in [
        "func @f(i64)->(i32,i32) {\nblock0(v0:i64):\n a=trunc.i32 v0\n return a,a\n}\n",
        "func @f(i64)->(i64,i64,i64) {\nblock0(v0:i64):\n return v0,v0,v0\n}\n",
        "func @f(i64)->(i64,i64,i64,i64,i64,i64,i64,i64,i64) cc(fast) local {\nblock0(v0:i64):\n return v0,v0,v0,v0,v0,v0,v0,v0,v0\n}\n",
        "func @f(i64,ptr)->(i8 uext,ptr) {\nblock0(v0:i64,v1:ptr):\n a=trunc.i8 v0\n return a,v1\n}\n",
        "decl @bad(i64,ptr)->(i8 sext,ptr)\nfunc @f(i64,ptr)->(i8,ptr) {\nblock0(v0:i64,v1:ptr):\n a,b=call @bad(v0,v1)\n return a,b\n}\n",
        "func @f(ptr,i64,ptr)->(i8,ptr) {\nblock0(v0:ptr,v1:i64,v2:ptr):\n a,b=call_indirect v0(v1,v2):(i64,ptr)->(i8 uext,ptr)\n return a,b\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
            let (speed, report) =
                backend_profile::capture(|| request(target, true).program(&module));
            let ordinary = request(target, false).program(&module);
            assert_eq!(
                speed.unwrap_err(),
                ordinary.unwrap_err(),
                "{target:?}: {source}"
            );
            assert_eq!(
                report.counters[backend_profile::Counter::DirectFunctions as usize],
                0
            );
        }
    }
}
