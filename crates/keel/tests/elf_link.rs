use std::path::Path;
use std::process::{Command, Output};
fn invoke(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .env("PATH", "")
        .output()
        .unwrap()
}
#[test]
fn native_elf_cli_is_host_independent_and_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli");
    std::fs::create_dir_all(&dir).unwrap();
    let object = keel_asm::x86::assemble_object(
        ".text\n.globl begin\nbegin:\nmov eax,60\nmov edi,42\nsyscall\n",
    )
    .unwrap();
    // A filename that resembles an option also exercises end-of-options handling.
    std::fs::write(dir.join("--target"), object).unwrap();
    let args = [
        "link",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--static",
        "-e",
        "begin",
        "-o",
        "program",
        "--",
        "--target",
    ];
    let output = invoke(&dir, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(dir.join("program")).unwrap();
    assert_eq!(&bytes[..4], b"\x7fELF");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    assert_eq!(
        Command::new(dir.join("program")).status().unwrap().code(),
        Some(42)
    );
    let output = invoke(
        &dir,
        &[
            "link",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-o",
            "program",
            "--",
            "--target",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing entry"));
    assert_eq!(std::fs::read(dir.join("program")).unwrap(), bytes);
    assert!(
        !invoke(
            &dir,
            &["link", "--target", "x86_64-unknown-linux-gnu", "--shared"]
        )
        .status
        .success()
    );
}

#[test]
fn archive_search_with_sysroot_and_exact_names_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-library-search");
    std::fs::create_dir_all(dir.join("sysroot/usr/lib")).unwrap();
    let object = keel_asm::x86::assemble_object(
        ".text\n.globl _start\n_start:\nmov eax,60\nmov edi,42\nsyscall\n",
    )
    .unwrap();
    let mut archive = b"!<arch>\n".to_vec();
    archive.extend_from_slice(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "start.o/",
            0,
            0,
            0,
            "100644",
            object.len()
        )
        .as_bytes(),
    );
    archive.extend_from_slice(&object);
    if !archive.len().is_multiple_of(2) {
        archive.push(b'\n');
    }
    std::fs::write(dir.join("sysroot/usr/lib/libentry.a"), archive).unwrap();
    let output = invoke(
        &dir,
        &[
            "link",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--static",
            "--sysroot",
            "sysroot",
            "-lentry",
            "-o",
            "program",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = std::fs::read(dir.join("program")).unwrap();
    let output = invoke(
        &dir,
        &[
            "link",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-Lsysroot/usr/lib",
            "-l:libentry.a",
            "-o",
            "exact",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(dir.join("exact")).unwrap(), expected);
}

#[test]
fn gnu_note_failures_preserve_existing_cli_destination() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-gnu-notes");
    std::fs::create_dir_all(&dir).unwrap();
    let assemble = |source: &str| keel_asm::x86::assemble_object(source).unwrap();
    let start = ".text\n.globl _start\n_start: ret\n";
    let property = ".section .note.gnu.property,\"a\",@note\n.p2align 3\n.globl prop\nprop: .long 4,16,5,0x00554e47,0xc0008002,4,8,0\n";
    let valid = assemble(&format!("{start}.data\n.quad _start\n{property}"));
    std::fs::write(dir.join("main.o"), &valid).unwrap();
    let args = [
        "link",
        "--target",
        "x86_64-unknown-linux-gnu",
        "main.o",
        "-o",
        "program",
    ];
    let output = invoke(&dir, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let destination = std::fs::read(dir.join("program")).unwrap();
    let read64 =
        |bytes: &[u8], at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize;
    let read32 = |bytes: &[u8], at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let shoff = read64(&valid, 40);
    let count = u16::from_le_bytes(valid[60..62].try_into().unwrap()) as usize;
    let rela = (1..count)
        .map(|i| shoff + 64 * i)
        .find(|&h| read32(&valid, h + 4) == 4)
        .unwrap();
    let wrong_table = (1..count)
        .find(|&i| read32(&valid, shoff + 64 * i + 4) == 3)
        .unwrap() as u32;
    for kind in [4u32, 9] {
        // RELA and unsupported REL: neither can hide its association.
        for bad_link in [0, u32::MAX, wrong_table] {
            let mut bad = valid.clone();
            bad[rela + 4..rela + 8].copy_from_slice(&kind.to_le_bytes());
            bad[rela + 40..rela + 44].copy_from_slice(&bad_link.to_le_bytes());
            std::fs::write(dir.join("main.o"), bad).unwrap();
            let output = invoke(&dir, &args);
            assert!(!output.status.success());
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                diagnostic.contains("relocation table") && diagnostic.contains("sh_link"),
                "{diagnostic}"
            );
            assert_eq!(std::fs::read(dir.join("program")).unwrap(), destination);
        }
    }
    // Use the independent review's complete DSO, including its real relocation.
    std::fs::write(
        dir.join("libreview.so"),
        include_bytes!("../../keel-link/tests/fixtures/gnu-notes/libreview.so"),
    )
    .unwrap();
    let args = [
        "link",
        "--target",
        "x86_64-unknown-linux-gnu",
        "main.o",
        "libreview.so",
        "-o",
        "program",
    ];
    for binding in [".globl prop", ".weak prop"] {
        let main = assemble(&format!(
            "{start}{}",
            property.replace(".globl prop", binding)
        ));
        std::fs::write(dir.join("main.o"), main).unwrap();
        let output = invoke(&dir, &args);
        assert!(!output.status.success());
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostic.contains("libreview.so") && diagnostic.contains("reference into consumed"),
            "{diagnostic}"
        );
        assert_eq!(std::fs::read(dir.join("program")).unwrap(), destination);
    }
    // The same complete DSO and an ordinary data provider remain accepted.
    std::fs::write(
        dir.join("main.o"),
        assemble(&format!("{start}.data\n.globl prop\nprop: .quad 0\n")),
    )
    .unwrap();
    let output = invoke(&dir, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(std::fs::read(dir.join("program")).unwrap(), destination);
}

#[test]
fn compiler_defined_dso_note_rejections_preserve_cli_destination() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-defined-gnu-notes");
    std::fs::create_dir_all(&dir).unwrap();
    let assemble = |source: &str| keel_asm::x86::assemble_object(source).unwrap();
    let start = ".text\n.globl _start\n_start: ret\n";
    let ordinary = assemble(&format!(
        "{start}.data\n.globl prop\n.type prop,@object\nprop: .quad 7\n"
    ));
    let property = ".section .note.gnu.property,\"a\",@note\n.p2align 3\n.globl prop\n.type prop,@object\nprop: .long 4,16,5,0x00554e47,0xc0008002,4,8,0\n";
    for (name, shared) in [
        (
            "defined",
            include_bytes!("../../keel-link/tests/fixtures/gnu-notes/libdefined.so").as_slice(),
        ),
        (
            "weak-defined",
            include_bytes!("../../keel-link/tests/fixtures/gnu-notes/libweak-defined.so")
                .as_slice(),
        ),
        (
            "undefined",
            include_bytes!("../../keel-link/tests/fixtures/gnu-notes/libundefined.so").as_slice(),
        ),
    ] {
        std::fs::write(dir.join("liblookup.so"), shared).unwrap();
        std::fs::write(dir.join("main.o"), &ordinary).unwrap();
        let args = [
            "link",
            "--target",
            "x86_64-unknown-linux-gnu",
            "main.o",
            "liblookup.so",
            "-o",
            "program",
        ];
        let output = invoke(&dir, &args);
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let destination = std::fs::read(dir.join("program")).unwrap();
        for binding in [".globl prop", ".weak prop"] {
            std::fs::write(
                dir.join("main.o"),
                assemble(&format!(
                    "{start}{}",
                    property.replace(".globl prop", binding)
                )),
            )
            .unwrap();
            let output = invoke(&dir, &args);
            assert!(!output.status.success());
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                diagnostic.contains("reference into consumed") && diagnostic.contains("prop"),
                "{name}: {diagnostic}"
            );
            assert_eq!(std::fs::read(dir.join("program")).unwrap(), destination);
        }
    }
    std::fs::write(
        dir.join("liblookup.so"),
        include_bytes!("../../keel-link/tests/fixtures/gnu-notes/libprotected-defined.so"),
    )
    .unwrap();
    let output = invoke(
        &dir,
        &[
            "link",
            "--target",
            "x86_64-unknown-linux-gnu",
            "main.o",
            "liblookup.so",
            "-o",
            "program",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
