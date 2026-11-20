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
fn leaf_encoding_stays_unchanged() {
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

fn stack_module() -> keel_ir::Module {
    use std::fmt::Write;
    let types = [
        "i64", "i64", "i64", "i64", "i64", "i64", "i64", "i64", "i8 sext", "i16 uext", "i32",
        "ptr", "i64", "i8 uext",
    ];
    let bare = [
        "i64", "i64", "i64", "i64", "i64", "i64", "i64", "i64", "i8", "i16", "i32", "ptr", "i64",
        "i8",
    ];
    let signature = types.join(",");
    let params = bare
        .iter()
        .enumerate()
        .map(|(i, t)| format!("v{i}:{t}"))
        .collect::<Vec<_>>()
        .join(",");
    let args = (0..14)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut source = format!(
        "decl @stack_probe({signature})->i64\ndecl @capture_large(i64)->i64\nfunc @incoming({signature})->i64 {{\nblock0({params}):\n"
    );
    let weights = [1, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43];
    let mut next = 14;
    let mut sum = None;
    for (i, weight) in weights.iter().enumerate() {
        let value = match i {
            8 | 10 => {
                writeln!(source, " v{next}=sext.i64 v{i}").unwrap();
                next += 1;
                next - 1
            }
            9 | 13 => {
                writeln!(source, " v{next}=uext.i64 v{i}").unwrap();
                next += 1;
                next - 1
            }
            11 => {
                writeln!(source, " v{next}=load.i64 v{i}").unwrap();
                next += 1;
                next - 1
            }
            _ => i,
        };
        writeln!(
            source,
            " v{next}=iconst.i64 {weight}\n v{}=imul v{value},v{next}",
            next + 1
        )
        .unwrap();
        let term = next + 1;
        next += 2;
        if let Some(previous) = sum {
            writeln!(source, " v{next}=iadd v{previous},v{term}").unwrap();
            sum = Some(next);
            next += 1;
        } else {
            sum = Some(term);
        }
    }
    writeln!(source, " return v{}\n}}", sum.unwrap()).unwrap();
    let reversed = [7, 6, 5, 4, 3, 2, 1, 0, 8, 9, 10, 11, 12, 13]
        .iter()
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(source,"func @outgoing({signature})->i64 {{\nblock0({params}):\n v14=call @stack_probe({reversed})\n v15=iadd v14,v0\n return v15\n}}\nfunc @fallback_stack({signature})->i64 {{\nblock0({params}):\n v14=call @stack_probe({args})\n v15=iconst.i64 0\n v16=rotr v14,v15\n return v16\n}}\nfunc @through_fallback({signature})->i64 {{\nblock0({params}):\n v14=call @fallback_stack({args})\n v15=iadd v14,v0\n return v15\n}}\nfunc @through_direct({signature})->i64 {{\nblock0({params}):\n v14=call @incoming({args})\n v15=iadd v14,v1\n return v15\n}}").unwrap();
    let indirect_params = bare
        .iter()
        .enumerate()
        .map(|(i, t)| format!("v{}:{t}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let indirect_args = (1..15)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(source,"func @indirect_stack(ptr,{signature})->i64 {{\nblock0(v0:ptr,{indirect_params}):\n v15=call_indirect v0({indirect_args}):({signature})->i64\n v16=iadd v15,v1\n return v16\n}}").unwrap();
    source.push_str("func @indirect_large_homes(ptr)->i64 {\nblock0(v0:ptr):\n");
    for i in 1..=4200 {
        writeln!(source, " v{i}=iconst.i64 {i}").unwrap();
    }
    let args = (4187..=4200)
        .map(|i| format!("v{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let signature = std::iter::repeat_n("i64", 14).collect::<Vec<_>>().join(",");
    writeln!(
        source,
        " v4201=call_indirect v0({args}):({signature})->i64\n return v4201\n}}"
    )
    .unwrap();
    source.push_str("func @large_frame(i64)->i64 {\n ss0=stack_slot 65520, align 16\n ss1=stack_slot 8, align 8\nblock0(v0:i64):\n v1=stack_addr ss1\n store v0,v1\n v2=call @capture_large(v0)\n v3=load.i64 v1\n v4=iadd v2,v3\n return v4\n}\n");
    source.push_str(
        r#"
decl @pair_probe(ptr byval [2 x i64],i64,i64,i64,i64,i64,ptr byval [2 x i64],i64)->i64 cc(c)
func @outgoing_pairs(i64,i64)->i64 cc(c) {
 ss0=stack_slot 16, align 8
 ss1=stack_slot 16, align 8
block0(v0:i64,v1:i64):
 v2=stack_addr ss0
 v3=stack_addr ss1
 v4=iconst.i64 8
 v5=ptradd v2,v4
 v6=ptradd v3,v4
 v7=iadd v0,v1
 v8=xor v0,v1
 store v0,v2
 store v1,v5
 store v7,v3
 store v8,v6
 v9=iconst.i64 3
 v10=iconst.i64 5
 v11=call @pair_probe(v2,v0,v1,v7,v8,v9,v3,v10)
 return v11
}
"#,
    );
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
#[test]
fn scalar_stack_abi_and_large_offsets_keep_both_object_routes() {
    let module = stack_module();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let request = request(target, UnwindTables::All);
        let (program, report) = backend_profile::capture(|| request.program(&module));
        let program = program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            module.functions.len() as u64 - 2
        );
        let text = request.text(&module).unwrap();
        let pairs = text.split_once("outgoing_pairs:\n").unwrap().1;
        assert!(!pairs.contains("stack_addr ss0"), "{pairs}");
        assert!(pairs.contains("stack_addr ss1"), "{pairs}");
        assert_eq!(
            keel_asm::assemble_program_for(target, &program).unwrap(),
            keel_asm::assemble_source_for(target, &text).unwrap()
        );
    }
}
#[test]
fn native_scalar_stack_abi_large_homes_and_large_frame_unwind_match_c() {
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
    let module = stack_module();
    let program = request(target, UnwindTables::All).program(&module).unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct-stack");
    fs::write(
        dir.join("stack.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("arm_direct_calls/stack.c"),
    )
    .unwrap();
    let mut command = clang.command();
    command.current_dir(dir.path()).args([
        "-O2",
        "-fno-optimize-sibling-calls",
        "oracle.c",
        "stack.o",
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
fn multi_instruction_sp_adjustments_publish_exact_cfa_rows() {
    let module=keel_ir::parse("func @f()->i64 {\n ss0=stack_slot 4096, align 16\nblock0:\n v0=iconst.i64 1\n return v0\n}\n").unwrap();
    let program = request(Target::Aarch64AppleDarwin, UnwindTables::All)
        .program(&module)
        .unwrap();
    let mut bytes = Vec::new();
    let mut rows = Vec::new();
    for item in &program.items {
        match item {
            keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Bytes(part)) => {
                bytes.extend_from_slice(part)
            }
            keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Cfi(
                keel_asm_ir::Cfi::DefCfaOffset(offset),
            )) => rows.push((bytes.len(), *offset)),
            _ => {}
        }
    }
    assert_eq!(rows, [(4, 4096), (8, 4112), (36, 16), (40, 0), (44, 4112)]);
    let expected=keel_asm::assemble("sub sp,sp,#1,lsl #12\nsub sp,sp,#16\nmovz x9,#1\nmovk x9,#0,lsl #16\nmovk x9,#0,lsl #32\nmovk x9,#0,lsl #48\nstr x9,[sp]\nldr x0,[sp]\nadd sp,sp,#1,lsl #12\nadd sp,sp,#16\nret\n").unwrap().text;
    assert_eq!(bytes, expected);
}

#[test]
fn incoming_offsets_above_frame_limit_encode_without_truncation() {
    let signature = std::iter::repeat_n("i64", 12).collect::<Vec<_>>().join(",");
    let params = (0..12)
        .map(|i| format!("v{i}:i64"))
        .collect::<Vec<_>>()
        .join(",");
    // 12 value homes + 12 edge temporaries leave a frame at the normal limit.
    // The final incoming argument is above that limit relative to the new SP.
    let limit = (1u32 << 24) - 16;
    let padding = limit - 24 * 8;
    let source = format!(
        "func @maximum({signature})->i64 {{\n ss0=stack_slot {padding}, align 16\nblock0({params}):\n return v11\n}}\n"
    );
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let req = request(target, UnwindTables::All);
        let (program, report) = backend_profile::capture(|| req.program(&module));
        let program = program.unwrap();
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            1
        );
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
        let expected = keel_asm::assemble(
            "add x12,sp,#4095,lsl #12\nadd x12,x12,#1,lsl #12\nadd x12,x12,#8\nldr x9,[x12]\n",
        )
        .unwrap()
        .text;
        assert!(
            bytes
                .windows(expected.len())
                .any(|window| window == expected),
            "incoming frame+24 truncated"
        );
        assert_eq!(
            keel_asm::assemble_program_for(target, &program).unwrap(),
            keel_asm::assemble_source_for(target, &req.text(&module).unwrap()).unwrap()
        );
    }
    let module=keel_ir::parse("func @too_large()->i64 {\n ss0=stack_slot 16777216, align 16\nblock0:\n v0=stack_addr ss0\n v1=ptrtoint.i64 v0\n return v1\n}\n").unwrap();
    let (result, report) = backend_profile::capture(|| {
        request(Target::Aarch64AppleDarwin, UnwindTables::All).program(&module)
    });
    assert!(result.is_err());
    assert_eq!(
        report.counters[backend_profile::Counter::DirectFunctions as usize],
        0
    );
}

#[test]
fn memory_intrinsics_use_ordinary_helper_calls_and_match_native_bytes() {
    let module = keel_ir::parse(include_str!("arm_direct_calls/memory.keel")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        for unwind in [UnwindTables::All, UnwindTables::RequiredOnly] {
            let req = request(target, unwind);
            let (program, report) = backend_profile::capture(|| req.program(&module));
            let program = program.unwrap();
            assert_eq!(
                report.counters[backend_profile::Counter::DirectFunctions as usize],
                4
            );
            let calls = program
                .items
                .iter()
                .filter_map(|item| {
                    if let keel_asm_ir::Item::Branch { link: true, target } = item {
                        Some(target.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let prefix = if target == Target::Aarch64AppleDarwin {
                "_"
            } else {
                ""
            };
            let expected = ["memcpy", "memmove", "memset", "memset", "memcpy", "memmove"]
                .map(|name| format!("{prefix}{name}"));
            assert_eq!(calls, expected);
            assert_eq!(
                program
                    .items
                    .iter()
                    .filter(|item| matches!(
                        item,
                        keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Cfi(
                            keel_asm_ir::Cfi::Offset { register: 30, .. }
                        ))
                    ))
                    .count(),
                if unwind == UnwindTables::All { 8 } else { 0 }
            );
            assert_eq!(
                keel_asm::assemble_program_for(target, &program).unwrap(),
                keel_asm::assemble_source_for(target, &req.text(&module).unwrap()).unwrap()
            );
        }
    }
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
    let program = request(target, UnwindTables::All).program(&module).unwrap();
    let dir = keel_test_support::TempDir::new("arm-direct-memory");
    fs::write(
        dir.join("memory.o"),
        keel_asm::assemble_program_for(target, &program).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("arm_direct_calls/memory.c"),
    )
    .unwrap();
    let out = clang
        .command()
        .current_dir(dir.path())
        .args(["-O2", "oracle.c", "memory.o", "-o", "oracle"])
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
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[path = "arm_direct_results/results.rs"]
mod results;
