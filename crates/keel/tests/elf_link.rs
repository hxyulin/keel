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

#[test]
fn weak_odr_ir_objects_link_across_translation_units_on_both_routes() {
    use object::{Object, ObjectSymbol};
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-comdat");
    std::fs::create_dir_all(&dir).unwrap();
    let start =
        keel_asm::x86::assemble_object(".text\n.globl _start\n_start: call answer\nret\n").unwrap();
    std::fs::write(dir.join("start.o"), start).unwrap();
    for unit in ["a", "b"] {
        let source = format!(
            "data @shared = weak_odr align 8 {{ i64 42 }}\ndata @pointer = weak_odr const align 8 {{ ptr @shared }}\nfunc @answer() -> i64 weak_odr {{\nblock0:\n v0 = iconst.i64 42\n return v0\n}}\nfunc @get_{unit}() -> ptr {{\nblock0:\n v0 = symbol_addr @shared\n return v0\n}}\n"
        );
        let input = format!("{unit}.keel");
        std::fs::write(dir.join(&input), source).unwrap();
        for route in ["typed", "text"] {
            let obj = format!("{unit}-{route}.o");
            let args = if route == "typed" {
                vec![
                    "compile",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--emit-object",
                    &input,
                    "-o",
                    &obj,
                ]
            } else {
                vec![
                    "compile",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    &input,
                    "-o",
                    "temp.s",
                ]
            };
            let output = invoke(&dir, &args);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if route == "text" {
                let output = invoke(
                    &dir,
                    &[
                        "asm",
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "temp.s",
                        "-o",
                        &obj,
                    ],
                );
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        assert_eq!(
            std::fs::read(dir.join(format!("{unit}-typed.o"))).unwrap(),
            std::fs::read(dir.join(format!("{unit}-text.o"))).unwrap()
        );
    }
    for route in ["typed", "text"] {
        let output = invoke(
            &dir,
            &[
                "link",
                "--target",
                "x86_64-unknown-linux-gnu",
                "start.o",
                &format!("a-{route}.o"),
                &format!("b-{route}.o"),
                "-o",
                "program",
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = std::fs::read(dir.join("program")).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        for name in ["answer", "shared", "pointer", "get_a", "get_b"] {
            assert_eq!(
                file.symbols().filter(|s| s.name().unwrap() == name).count(),
                1,
                "{name}"
            );
        }
    }
}

#[test]
fn elf_cli_gc_is_opt_in_and_failure_preserves_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-gc");
    std::fs::create_dir_all(&dir).unwrap();
    let obj = keel_asm::x86::assemble_object(
        ".text\n.globl _start\n_start: ret\n.section .text.dead,\"ax\"\ndead: call absent\nret\n",
    )
    .unwrap();
    std::fs::write(dir.join("input.o"), obj).unwrap();
    let base = [
        "link",
        "--target",
        "x86_64-unknown-linux-gnu",
        "input.o",
        "-o",
        "program",
    ];
    assert!(!invoke(&dir, &base).status.success());
    let mut gc = base.to_vec();
    gc.push("--gc-sections");
    let result = invoke(&dir, &gc);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let image = std::fs::read(dir.join("program")).unwrap();
    gc.push("--no-gc-sections");
    assert!(!invoke(&dir, &gc).status.success());
    assert_eq!(std::fs::read(dir.join("program")).unwrap(), image);
}

#[test]
fn sframe_failures_preserve_cli_destination_at_every_validation_stage() {
    use object::{Object, ObjectSection};
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("elf-cli-sframe-review");
    std::fs::create_dir_all(&dir).unwrap();
    let crt = include_bytes!("../../keel-link/tests/fixtures/sframe/crt1.o").to_vec();
    let stubs = keel_asm::x86::assemble_object(
        ".text\n.globl main\nmain: ret\n.globl __libc_start_main\n__libc_start_main: ret\n",
    )
    .unwrap();
    std::fs::write(dir.join("stubs.o"), stubs).unwrap();
    std::fs::write(dir.join("crt.o"), &crt).unwrap();
    let args = [
        "link",
        "--target",
        "x86_64-unknown-linux-gnu",
        "crt.o",
        "stubs.o",
        "-o",
        "program",
    ];
    let result = invoke(&dir, &args);
    assert!(result.status.success(), "{:?}", result.stderr);
    let destination = std::fs::read(dir.join("program")).unwrap();
    let input = object::File::parse(crt.as_slice()).unwrap();
    let metadata = input.section_by_name(".sframe").unwrap();
    let rela = input.section_by_name(".rela.sframe").unwrap();
    let d = metadata.file_range().unwrap().0 as usize;
    let r = rela.file_range().unwrap().0 as usize;
    let shoff = u64::from_le_bytes(crt[40..48].try_into().unwrap()) as usize;
    let mut cases = Vec::new();
    let mut bad = crt.clone();
    bad[d + 84] = 0; // second row must increase from PC0
    cases.push((bad, false, "row PCs"));
    let mut bad = crt.clone();
    bad[r + 24..r + 32].copy_from_slice(&28u64.to_le_bytes());
    cases.push((bad, false, "duplicate index"));
    let mut bad = crt.clone();
    bad[r + 64..r + 72].copy_from_slice(&i64::MAX.to_le_bytes());
    cases.push((bad, false, "outside its executable input section"));
    let mut bad = crt.clone();
    bad[r + 64..r + 72].copy_from_slice(&48i64.to_le_bytes());
    cases.push((bad, false, "unordered or overlapping"));
    let mut bad = crt.clone();
    bad[d + 84] = 0;
    cases.push((bad, true, "row PCs"));
    let mut bad = crt.clone();
    bad[r + 64..r + 72].copy_from_slice(&i64::MAX.to_le_bytes());
    cases.push((bad, true, "outside its executable input section"));
    let mut bad = crt.clone();
    bad[r + 64..r + 72].copy_from_slice(&48i64.to_le_bytes());
    cases.push((bad, true, "unordered or overlapping"));
    let mut empty = crt.clone();
    empty[d + 8..d + 28].fill(0);
    let header = shoff + 64 * metadata.index().0;
    empty[header + 32..header + 40].copy_from_slice(&28u64.to_le_bytes());
    let header = shoff + 64 * rela.index().0;
    empty[header + 32..header + 40].fill(0);

    for (bytes, gc, expected) in cases {
        std::fs::write(dir.join("crt.o"), bytes).unwrap();
        let mut args = args.to_vec();
        if gc {
            args.push("--gc-sections");
        }
        let result = invoke(&dir, &args);
        assert!(!result.status.success());
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains(expected), "{diagnostic}");
        assert_eq!(std::fs::read(dir.join("program")).unwrap(), destination);
    }
    for (bytes, count) in [(crt, 3), (empty, 0)] {
        std::fs::write(dir.join("crt.o"), bytes).unwrap();
        let mut args = args.to_vec();
        args.push("--gc-sections");
        let result = invoke(&dir, &args);
        assert!(result.status.success(), "{:?}", result.stderr);
        let bytes = std::fs::read(dir.join("program")).unwrap();
        let output = object::File::parse(bytes.as_slice()).unwrap();
        let data = output.section_by_name(".sframe").unwrap().data().unwrap();
        assert_eq!(u32::from_le_bytes(data[8..12].try_into().unwrap()), count);
        assert_eq!(data[3], 5);
    }
}
