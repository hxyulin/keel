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
