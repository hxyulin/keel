use std::process::Command;

fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(args)
        .env("PATH", "")
        .output()
        .unwrap()
}

#[test]
fn disassembles_aarch64_macho_and_elf_without_external_tools() {
    let dir = std::env::temp_dir().join(format!("keel-disasm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("input.s");
    std::fs::write(&source, ".text\n.globl entry\nentry:\nstp x29, x30, [sp, #-16]!\nmov x29, sp\nadd x0, x0, #1\nldp x29, x30, [sp], #16\nret\n.byte 0x12, 0x34\n").unwrap();
    for (format, target) in [
        ("macho", "aarch64-apple-darwin"),
        ("elf", "aarch64-unknown-linux-gnu"),
    ] {
        let object = dir.join(format!("{format}.o"));
        let result = invoke(&[
            "asm",
            "--target",
            target,
            source.to_str().unwrap(),
            "-o",
            object.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = invoke(&["disasm", object.to_str().unwrap()]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains("0000000000000000 <entry>:"), "{text}");
        assert!(
            text.contains("fd 7b bf a9  stp x29, x30, [sp, #-16]!"),
            "{text}"
        );
        assert!(text.contains("add x0, x0, #1"), "{text}");
        assert!(
            text.contains("0000000000000014: 12 34        .byte 0x12, 0x34"),
            "{text}"
        );
        let output = dir.join(format!("{format}.txt"));
        let result = invoke(&[
            "disasm",
            object.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ]);
        assert!(result.status.success());
        assert_eq!(std::fs::read_to_string(output).unwrap(), text);
    }
    let bad = dir.join("bad.o");
    std::fs::write(&bad, b"not an object").unwrap();
    let result = invoke(&["disasm", bad.to_str().unwrap()]);
    assert!(!result.status.success());
    let x86 = dir.join("x86.o");
    std::fs::write(&source, ".text\nret\n").unwrap();
    let result = invoke(&[
        "asm",
        "--target",
        "x86_64-unknown-linux-gnu",
        source.to_str().unwrap(),
        "-o",
        x86.to_str().unwrap(),
    ]);
    assert!(result.status.success());
    let result = invoke(&["disasm", x86.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("little-endian AArch64"));
    std::fs::remove_dir_all(dir).unwrap();
}
