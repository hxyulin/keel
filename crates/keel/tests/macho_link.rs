use std::{path::Path, process::Command};

#[test]
fn native_macho_cli_signs_with_explicit_sdk_and_empty_path() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("macho-cli-sign");
    std::fs::create_dir_all(dir.join("sdk/usr/lib")).unwrap();
    std::fs::write(dir.join("sdk/usr/lib/libSystem.tbd"), "--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: '/usr/lib/libSystem.B.dylib'\n").unwrap();
    let object =
        keel_asm::assemble_object(".text\n.globl _main\n_main:\nmov w0,#42\nret\n").unwrap();
    std::fs::write(dir.join("main.o"), object).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .env("PATH", "")
            .env_remove("SDKROOT")
            .args([
                "link",
                "--target",
                "aarch64-apple-darwin",
                "--sysroot",
                "sdk",
                "main.o",
                "-o",
                "program",
            ])
            .output()
            .unwrap()
    };
    let output = invoke();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let signed = std::fs::read(dir.join("program")).unwrap();
    let le32 = |at| u32::from_le_bytes(signed[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 32;
    let mut found = false;
    for _ in 0..le32(16) {
        if le32(at) == 0x1d {
            let offset = le32(at + 8);
            assert_eq!(offset + le32(at + 12), signed.len());
            assert_eq!(&signed[offset..offset + 4], &0xfade0cc0u32.to_be_bytes());
            found = true;
        }
        at += le32(at + 4);
    }
    assert!(found);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        assert!(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict"])
                .arg(dir.join("program"))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            Command::new(dir.join("program")).status().unwrap().code(),
            Some(42)
        );
    }
    // A mapped input may also be the destination. Atomic installation must not
    // truncate it while the linker still owns its input map.
    std::fs::copy(dir.join("main.o"), dir.join("program")).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .env("PATH", "")
        .args([
            "link",
            "--target",
            "aarch64-apple-darwin",
            "--sysroot",
            "sdk",
            "program",
            "-o",
            "program",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(dir.join("program")).unwrap(), signed);
    // Signing failure or bad input cannot replace a previously installed image.
    std::fs::write(dir.join("main.o"), b"invalid").unwrap();
    assert!(!invoke().status.success());
    assert_eq!(std::fs::read(dir.join("program")).unwrap(), signed);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn macho_dead_strip_cli_aliases_drop_dead_undefined_and_require_exports() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("macho-cli-gc");
    std::fs::create_dir_all(dir.join("sdk/usr/lib")).unwrap();
    std::fs::write(dir.join("sdk/usr/lib/libSystem.tbd"), "--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: '/usr/lib/libSystem.B.dylib'\n").unwrap();
    for (name, source) in [
        ("main", ".globl _main\n_main:\nmov w0,#0\nret\n"),
        ("dead", ".globl _dead\n_dead:\nb _missing\n"),
    ] {
        std::fs::write(
            dir.join(format!("{name}.o")),
            keel_asm::assemble_object(source).unwrap(),
        )
        .unwrap();
    }
    let invoke = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .env("PATH", "")
            .args([
                "link",
                "--target",
                "aarch64-apple-darwin",
                "--sysroot",
                "sdk",
                "main.o",
                "dead.o",
                "-o",
                "program",
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    assert!(!invoke(&[]).status.success());
    for flag in ["--dead-strip", "-dead_strip"] {
        let result = invoke(&[flag]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = invoke(&[flag, "--require-export", "_dead"]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("_missing"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn exported_weak_odr_addresses_agree_across_objects() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("macho-cli-weak-odr");
    std::fs::create_dir_all(dir.join("sdk/usr/lib")).unwrap();
    std::fs::write(dir.join("sdk/usr/lib/libSystem.tbd"), "--- !tapi-tbd\ntbd-version: 4\ntargets: [arm64-macos]\ninstall-name: '/usr/lib/libSystem.B.dylib'\n").unwrap();
    let keel = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    // Each unit defines the same exported and hidden weak_odr symbols and
    // returns their addresses.
    for unit in ["a", "b"] {
        let source = format!(
            "data @shared = weak_odr align 8 {{ i64 42 }}\n\
             data @hidden_shared = weak_odr hidden align 8 {{ i64 43 }}\n\
             func @answer() -> i64 weak_odr {{\nblock0:\n v0 = iconst.i64 42\n return v0\n}}\n\
             func @shared_{unit}() -> ptr {{\nblock0:\n v0 = symbol_addr @shared\n return v0\n}}\n\
             func @hidden_{unit}() -> ptr {{\nblock0:\n v0 = symbol_addr @hidden_shared\n return v0\n}}\n\
             func @answer_{unit}() -> ptr {{\nblock0:\n v0 = symbol_addr @answer\n return v0\n}}\n"
        );
        std::fs::write(dir.join(format!("{unit}.keel")), source).unwrap();
    }
    // The exit status has one bit per failed check.
    let mut main = String::new();
    for unit in ["a", "b"] {
        main += &format!(
            "decl @shared_{unit}() -> ptr\ndecl @hidden_{unit}() -> ptr\ndecl @answer_{unit}() -> ptr\n"
        );
    }
    main += "func @main() -> i32 {\nblock0:\n\
             v0 = call @shared_a()\n v1 = call @shared_b()\n\
             v2 = call @answer_a()\n v3 = call @answer_b()\n\
             v4 = call @hidden_a()\n v5 = call @hidden_b()\n\
             v6 = load.i64 v0\n v7 = call_indirect v2() : () -> i64\n\
             v8 = iconst.i64 42\n v9 = iconst.i32 0\n\
             v10 = icmp.ne v0, v1\n v11 = iconst.i32 1\n v12 = select v10, v11, v9\n\
             v13 = icmp.ne v2, v3\n v14 = iconst.i32 2\n v15 = select v13, v14, v9\n\
             v16 = icmp.ne v4, v5\n v17 = iconst.i32 4\n v18 = select v16, v17, v9\n\
             v19 = icmp.ne v6, v8\n v20 = iconst.i32 8\n v21 = select v19, v20, v9\n\
             v22 = icmp.ne v7, v8\n v23 = iconst.i32 16\n v24 = select v22, v23, v9\n\
             v25 = or v12, v15\n v26 = or v25, v18\n v27 = or v26, v21\n v28 = or v27, v24\n\
             return v28\n}\n";
    std::fs::write(dir.join("main.keel"), main).unwrap();
    for unit in ["a", "b", "main"] {
        keel(&[
            "compile",
            "--target",
            "aarch64-apple-darwin",
            "--emit-object",
            &format!("{unit}.keel"),
            "-o",
            &format!("{unit}.o"),
        ]);
    }
    keel(&[
        "link",
        "--target",
        "aarch64-apple-darwin",
        "--sysroot",
        "sdk",
        "main.o",
        "a.o",
        "b.o",
        "-o",
        "program",
    ]);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(
        Command::new(dir.join("program")).status().unwrap().code(),
        Some(0)
    );
}
