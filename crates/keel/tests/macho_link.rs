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
