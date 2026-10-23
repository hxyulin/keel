use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn run(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {:?}\n{}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn keel() -> Command {
    Command::new(env!("CARGO_BIN_EXE_keel"))
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/arm-linux")
}
#[test]
fn cross_target_c_and_ir_emit_elf_with_debug() {
    let dir = std::env::temp_dir().join(format!("keel-arm-compiler-cross-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("test.c");
    std::fs::write(
        &source,
        "char c; int answer(int x) { return x + (unsigned char)c; }\n",
    )
    .unwrap();
    for object in [false, true] {
        let output = dir.join(if object { "test.o" } else { "test.s" });
        let mut cmd = keel();
        cmd.args(["cc", "--target", "aarch64-unknown-linux-gnu", "-g"]);
        if object {
            cmd.arg("--emit-object");
        }
        run(cmd.arg(&source).arg("-o").arg(&output));
        if object {
            let bytes = std::fs::read(output).unwrap();
            assert_eq!(&bytes[..4], b"\x7fELF");
            assert_eq!(&bytes[18..20], &183u16.to_le_bytes());
        } else {
            let text = std::fs::read_to_string(output).unwrap();
            assert!(text.contains(".cfi_startproc"));
            assert!(!text.contains("@PAGE"));
            assert!(text.contains(".debug_info"));
        }
    }
    let ir = dir.join("answer.keel");
    std::fs::write(
        &ir,
        "func @answer() -> i32 {\nentry:\n  v0 = iconst.i32 42\n  return v0\n}\n",
    )
    .unwrap();
    run(keel()
        .args([
            "compile",
            "--target",
            "aarch64-unknown-linux-gnu",
            "--emit-object",
        ])
        .arg(&ir)
        .arg("-o")
        .arg(dir.join("answer.o")));
    let wide = dir.join("wide.keel");
    std::fs::write(&wide, "decl @sink(i32, ...) -> i32\nfunc @caller() -> i32 {\nblock0:\n v0 = iconst.i32 0\n v1 = iconst.i128 1\n v2 = call @sink(v0, v1)\n return v2\n}\n").unwrap();
    run(keel()
        .args([
            "compile",
            "--target",
            "aarch64-unknown-linux-gnu",
            "--emit-object",
        ])
        .arg(&wide)
        .arg("-o")
        .arg(dir.join("wide.o")));
    let unsupported = keel()
        .args(["compile", "--target", "aarch64-apple-darwin"])
        .arg(&wide)
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("variadic i128 arguments"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn native_linux_abi_interoperates_with_clang_in_both_directions() {
    if !cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        return;
    }
    let dir = std::env::temp_dir().join(format!("keel-arm-compiler-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fixtures = fixture();
    run(Command::new("clang")
        .args(["-O2", "-c"])
        .arg(fixtures.join("threads.c"))
        .arg("-o")
        .arg(dir.join("threads.o")));
    for optimize in [false, true] {
        for allocator in ["fast", "full"] {
            for route in ["object", "clang-asm", "keel-asm"] {
                for name in ["callee", "driver"] {
                    let mut cmd = keel();
                    cmd.args(["cc", "--regalloc", allocator, "-g"]);
                    if optimize {
                        cmd.arg("-O");
                    }
                    if route == "object" {
                        cmd.arg("--emit-object");
                    }
                    let path = dir.join(format!(
                        "{name}.{}",
                        if route == "object" { "o" } else { "s" }
                    ));
                    run(cmd
                        .arg(fixtures.join(format!("{name}.c")))
                        .arg("-o")
                        .arg(&path));
                    if route == "clang-asm" {
                        run(Command::new("clang")
                            .arg("-c")
                            .arg(&path)
                            .arg("-o")
                            .arg(dir.join(format!("{name}.o"))));
                    }
                    if route == "keel-asm" {
                        run(keel()
                            .arg("asm")
                            .arg(&path)
                            .arg("-o")
                            .arg(dir.join(format!("{name}.o"))));
                    }
                }
                for name in ["callee", "driver"] {
                    run(Command::new("clang")
                        .args(["-O2", "-g", "-c"])
                        .arg(fixtures.join(format!("{name}.c")))
                        .arg("-o")
                        .arg(dir.join(format!("{name}-clang.o"))));
                }
                for (callee, driver) in [
                    ("callee.o", "driver-clang.o"),
                    ("callee-clang.o", "driver.o"),
                    ("callee.o", "driver.o"),
                ] {
                    let exe = dir.join("program");
                    run(Command::new("clang")
                        .arg(dir.join(callee))
                        .arg(dir.join(driver))
                        .arg(dir.join("threads.o"))
                        .arg("-pthread")
                        .arg("-o")
                        .arg(&exe));
                    let result = run(&mut Command::new(exe));
                    assert_eq!(
                        String::from_utf8_lossy(&result.stdout),
                        "ARM Linux ABI ok\n"
                    );
                }
                run(Command::new("llvm-dwarfdump")
                    .arg("--verify")
                    .arg(dir.join("callee.o")));
            }
        }
    }
    // Hosted linking through the explicit system driver supplies CRT/libc.
    run(keel()
        .args(["link", "--system"])
        .arg(dir.join("callee.o"))
        .arg(dir.join("driver.o"))
        .arg(dir.join("threads.o"))
        .arg("-pthread")
        .arg("-o")
        .arg(dir.join("hosted")));
    run(&mut Command::new(dir.join("hosted")));
    // A C/IR-produced frame can also pass through the native ELF linker.
    std::fs::write(dir.join("answer.c"), "int answer(void) { return 42; }\n").unwrap();
    run(keel()
        .args(["cc", "--emit-object"])
        .arg(dir.join("answer.c"))
        .arg("-o")
        .arg(dir.join("answer.o")));
    std::fs::write(
        dir.join("start.s"),
        include_str!("../../keel-link/tests/fixtures/aarch64/start.s"),
    )
    .unwrap();
    run(keel()
        .arg("asm")
        .arg(dir.join("start.s"))
        .arg("-o")
        .arg(dir.join("start.o")));
    run(keel()
        .arg("link")
        .arg(dir.join("start.o"))
        .arg(dir.join("answer.o"))
        .arg("-o")
        .arg(dir.join("answer")));
    assert_eq!(
        Command::new(dir.join("answer")).status().unwrap().code(),
        Some(42)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn tls_addresses_survive_forced_spills_in_both_object_routes() {
    let module = keel_ir::parse("data @value = thread_local align 8 { i64 17 }\nfunc @main() -> i32 {\nblock0:\n v0 = symbol_addr @value\n v1 = load.i64 v0\n v2 = iconst.i64 17\n v3 = icmp.eq v1, v2\n v4 = iconst.i32 0\n v5 = iconst.i32 1\n v6 = select v3, v4, v5\n return v6\n}\n").unwrap();
    let pools = keel_codegen::Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let dir = std::env::temp_dir().join(format!("keel-arm-spilled-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for allocator in [
        keel_codegen::regalloc::AllocationTier::Fast,
        keel_codegen::regalloc::AllocationTier::Full,
    ] {
        let pipeline = keel_codegen::CodegenPipeline {
            selector: &keel_codegen::SelectionTier::Baseline,
            allocator: &allocator,
        };
        let source =
            keel_codegen::aarch64::compile_linux_with_pipeline(&module, &pools, &pipeline).unwrap();
        let program =
            keel_codegen::aarch64::compile_linux_program_with_pipeline(&module, &pools, &pipeline)
                .unwrap();
        let direct = keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap();
        let parsed = keel_asm::assemble_elf_object(&source).unwrap();
        assert_eq!(direct, parsed);
        if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
            std::fs::write(dir.join("tls.o"), direct).unwrap();
            run(Command::new("clang")
                .arg(dir.join("tls.o"))
                .arg("-o")
                .arg(dir.join("tls")));
            run(&mut Command::new(dir.join("tls")));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
