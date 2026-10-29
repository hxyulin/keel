use std::{path::Path, process::Command};
fn invoke(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(args)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn standalone_arm_linux_assembly_archive_and_link() {
    let dir = std::env::temp_dir().join(format!("keel-arm-linux-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |s: &str| dir.join(s).to_str().unwrap().to_owned();
    let target = "aarch64-unknown-linux-gnu";
    std::fs::write(
        path("start.s"),
        include_str!("../../../keel-link/tests/fixtures/aarch64/start.s"),
    )
    .unwrap();
    std::fs::write(
        path("helper.s"),
        include_str!("../../../keel-link/tests/fixtures/aarch64/helper.s"),
    )
    .unwrap();
    for name in ["start", "helper"] {
        invoke(&[
            "asm",
            "--target",
            target,
            &path(&format!("{name}.s")),
            "-o",
            &path(&format!("{name}.o")),
        ]);
    }
    invoke(&[
        "archive",
        "--target",
        target,
        &path("helper.o"),
        "-o",
        &path("libhelper.a"),
    ]);
    invoke(&[
        "link",
        "--target",
        target,
        &path("start.o"),
        "-L",
        dir.to_str().unwrap(),
        "-lhelper",
        "-o",
        &path("program"),
    ]);
    assert_eq!(
        &std::fs::read(path("program")).unwrap()[18..20],
        &183u16.to_le_bytes()
    );
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        assert_eq!(
            Command::new(path("program")).status().unwrap().code(),
            Some(42)
        );
        // Both directions of interoperability with an independent assembler/linker.
        let reference = Command::new("clang")
            .args(["-c", &path("helper.s"), "-o", &path("reference.o")])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        invoke(&[
            "link",
            "--target",
            target,
            &path("start.o"),
            &path("reference.o"),
            "-o",
            &path("reference-program"),
        ]);
        assert_eq!(
            Command::new(path("reference-program"))
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
        let system = Command::new("clang")
            .args([
                "-nostdlib",
                "-static",
                &path("start.o"),
                &path("helper.o"),
                "-o",
                &path("system-program"),
            ])
            .output()
            .unwrap();
        assert!(
            system.status.success(),
            "{}",
            String::from_utf8_lossy(&system.stderr)
        );
        assert_eq!(
            Command::new(path("system-program"))
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
        // The default assembler and linker also select ARM ELF on this host.
        invoke(&["asm", &path("start.s"), "-o", &path("default.o")]);
        invoke(&[
            "link",
            &path("default.o"),
            &path("helper.o"),
            "-o",
            &path("default-program"),
        ]);
        assert_eq!(
            Command::new(path("default-program"))
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
    }
    // A failed link must preserve its existing destination.
    std::fs::write(path("protected"), b"keep").unwrap();
    let error = Command::new(env!("CARGO_BIN_EXE_keel"))
        .args([
            "link",
            "--target",
            target,
            &path("start.o"),
            "-o",
            &path("protected"),
        ])
        .output()
        .unwrap();
    assert!(!error.status.success());
    assert_eq!(std::fs::read(path("protected")).unwrap(), b"keep");
    std::fs::remove_dir_all(Path::new(&dir)).unwrap();
}

/// Every production step runs with no external programs discoverable on PATH.
#[test]
fn standalone_c_ir_assembly_archive_and_link() {
    let dir = std::env::temp_dir().join(format!("keel-arm-full-stack-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |name: &str| dir.join(name).to_str().unwrap().to_owned();
    let target = "aarch64-unknown-linux-gnu";
    std::fs::write(
        path("answer.c"),
        "#define COUNT 4\nextern int increment(int);\nstatic int values[COUNT] = { 4, 8, 12, 16 };\nint answer(void) { int sum = 0; for (int i = 0; i < COUNT; ++i) sum += values[i]; return increment(sum); }\n",
    ).unwrap();
    std::fs::write(
        path("increment.keel"),
        "func @increment(i32) -> i32 {\nentry(v0: i32):\n v1 = iconst.i32 2\n v2 = iadd v0, v1\n return v2\n}\n",
    ).unwrap();
    std::fs::write(
        path("start.s"),
        include_str!("../../../keel-link/tests/fixtures/aarch64/start.s"),
    )
    .unwrap();
    invoke(&[
        "asm",
        "--target",
        target,
        &path("start.s"),
        "-o",
        &path("start.o"),
    ]);
    for direct in [false, true] {
        for (command, source, name) in [
            ("cc", "answer.c", "answer"),
            ("compile", "increment.keel", "increment"),
        ] {
            let output = path(&format!("{name}.{}", if direct { "o" } else { "s" }));
            let input = path(source);
            let mut args = vec![command, "--target", target, "-O", &input, "-o", &output];
            if direct {
                args.push("--emit-object");
            }
            invoke(&args);
            if !direct {
                invoke(&[
                    "asm",
                    "--target",
                    target,
                    &output,
                    "-o",
                    &path(&format!("{name}.o")),
                ]);
            }
        }
        invoke(&[
            "archive",
            "--target",
            target,
            &path("answer.o"),
            &path("increment.o"),
            "-o",
            &path("libanswer.a"),
        ]);
        invoke(&[
            "link",
            "--target",
            target,
            &path("start.o"),
            "-L",
            dir.to_str().unwrap(),
            "-lanswer",
            "-o",
            &path("program"),
        ]);
        let bytes = std::fs::read(path("program")).unwrap();
        assert_eq!(&bytes[18..20], &183u16.to_le_bytes());
        let mut execution = if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            Some(Command::new(path("program")))
        } else {
            std::env::var_os("KEEL_AARCH64_RUNNER").map(|runner| {
                let mut command = Command::new(runner);
                command.arg(path("program"));
                command
            })
        };
        if let Some(command) = &mut execution {
            assert_eq!(command.env("PATH", "").status().unwrap().code(), Some(42));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
