//! Scalar call ABI, LR preservation, recursion, indirect calls and native unwinding.
use keel_codegen::{
    BackendMode, CodegenPipeline, EmissionOptions, Request, SelectionTier, Target, UnwindTables,
    backend_profile, regalloc::AllocationTier,
};
use std::{fs, process::Command};
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
fn module() -> keel_ir::Module {
    let m = keel_ir::parse(include_str!("arm_direct_calls/calls.keel")).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
#[test]
fn calls_keep_symbolic_relocations_and_text_object_unwind_parity() {
    let module = module();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        for unwind in [UnwindTables::All, UnwindTables::RequiredOnly] {
            let request = request(target, unwind);
            let (result, report) = backend_profile::capture(|| request.program(&module));
            let program = result.unwrap();
            assert_eq!(
                report.counters[backend_profile::Counter::DirectFunctions as usize],
                module.functions.len() as u64 - 2
            );
            assert!(
                program
                    .items
                    .iter()
                    .any(|item| matches!(item, keel_asm_ir::Item::Branch { link: true, .. }))
            );
            let object = keel_asm::assemble_program_for(target, &program).unwrap();
            assert_eq!(
                object,
                keel_asm::assemble_source_for(target, &request.text(&module).unwrap()).unwrap()
            );
            let mut chunks = keel_asm_ir::Program::new();
            request
                .program_chunks(&module, &mut |part| chunks.append(part))
                .unwrap();
            assert_eq!(
                object,
                keel_asm::assemble_program_for(target, &chunks).unwrap()
            );
        }
    }
}
#[test]
fn native_direct_calls_and_unwind_through_fallback_match_c() {
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
    let program = request(target, UnwindTables::All)
        .program(&module())
        .unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct-calls");
    fs::write(
        dir.join("calls.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("arm_direct_calls/oracle.c"),
    )
    .unwrap();
    let mut command = clang.command();
    command.current_dir(dir.path()).args([
        "-O2",
        "-fno-optimize-sibling-calls",
        "oracle.c",
        "calls.o",
        "-o",
        "oracle",
    ]);
    if target == Target::Aarch64LinuxGnu {
        command.args(["-rdynamic", "-ldl"]);
    }
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(dir.join("oracle")).output().unwrap();
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn unsupported_call_shapes_remain_transactional_fallback() {
    for source in [
        "decl @nine(i64,i64,i64,i64,i64,i64,i64,i64,i64)->i64\nfunc @f(i64)->i64 {\nblock0(v0:i64):\n v1=call @nine(v0,v0,v0,v0,v0,v0,v0,v0,v0)\n return v1\n}\n",
        "decl @floating(f64)->f64\nfunc @f(f64)->f64 {\nblock0(v0:f64):\n v1=call @floating(v0)\n return v1\n}\n",
        "decl @varargs(ptr,...)->i64\nfunc @f(ptr,i64)->i64 {\nblock0(v0:ptr,v1:i64):\n v2=call @varargs(v0,v1)\n return v2\n}\n",
        "decl @twice(ptr)->i32 returns_twice\nfunc @f(ptr)->i32 {\nblock0(v0:ptr):\n v1=call @twice(v0)\n return v1\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let request = request(Target::Aarch64AppleDarwin, UnwindTables::All);
        let (result, report) = backend_profile::capture(|| request.program(&module));
        let program = result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            0
        );
        let ordinary = Request::new(request.target, request.pipeline)
            .program(&module)
            .unwrap();
        assert_eq!(
            keel_asm::assemble_program_for(request.target, &program).unwrap(),
            keel_asm::assemble_program_for(request.target, &ordinary).unwrap()
        );
    }
}

#[test]
fn leaf_encoding_stays_unchanged_and_lr_slot_respects_frame_limit() {
    let source = "func @leaf(i64,i64)->i64 {\nblock0(v0:i64,v1:i64):\n v2=iconst.i64 -1\n v3=xor v0,v2\n v4=imul v3,v1\n return v4\n}\n";
    let module = keel_ir::parse(source).unwrap();
    let program = request(Target::Aarch64AppleDarwin, UnwindTables::All)
        .program(&module)
        .unwrap();
    let bytes: Vec<u8> = program
        .items
        .iter()
        .filter_map(|item| {
            if let keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Bytes(bytes)) = item {
                Some(bytes.as_slice())
            } else {
                None
            }
        })
        .flatten()
        .copied()
        .collect();
    let expected=keel_asm::assemble("sub sp,sp,#64\nstr x0,[sp]\nstr x1,[sp,#8]\nmovz x9,#65535\nmovk x9,#65535,lsl #16\nmovk x9,#65535,lsl #32\nmovk x9,#65535,lsl #48\nstr x9,[sp,#16]\nldr x9,[sp]\nldr x10,[sp,#16]\neor x9,x9,x10\nstr x9,[sp,#24]\nldr x9,[sp,#24]\nldr x10,[sp,#8]\nmul x9,x9,x10\nstr x9,[sp,#32]\nldr x0,[sp,#32]\nadd sp,sp,#64\nret\n").unwrap().text;
    assert_eq!(bytes, expected);
    let constants = (0..510)
        .map(|i| format!(" v{i}=iconst.i64 {i}\n"))
        .collect::<String>();
    for (call, expected_count) in [("", 1), (" call @external()\n", 0)] {
        let source = format!(
            "decl @external()\nfunc @f()->i64 {{\nblock0:\n{constants}{call} return v509\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let (result, report) = backend_profile::capture(|| {
            request(Target::Aarch64AppleDarwin, UnwindTables::All).program(&module)
        });
        result.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            expected_count
        );
    }
}

#[test]
fn external_darwin_narrow_calls_keep_ordinary_abi_rejection() {
    for source in [
        "decl @external8(i8)->i64\nfunc @caller(i8 uext)->i64 {\nblock0(v0:i8):\n v1=call @external8(v0)\n return v1\n}\n",
        "func @caller(ptr,i8 uext)->i64 {\nblock0(v0:ptr,v1:i8):\n v2=call_indirect v0(v1):(i8)->i64\n return v2\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let fast = request(Target::Aarch64AppleDarwin, UnwindTables::All);
        let ordinary = Request::new(fast.target, fast.pipeline);
        let expected = ordinary.program(&module).unwrap_err();
        let actual = fast.program(&module).unwrap_err();
        assert_eq!(actual, expected);
        assert!(actual.message.contains("arguments to external code need"));
        // Linux does not impose Darwin's caller-extension contract.
        let (program, report) = backend_profile::capture(|| {
            request(Target::Aarch64LinuxGnu, UnwindTables::All).program(&module)
        });
        program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            1
        );
    }
}
