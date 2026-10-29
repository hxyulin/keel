#[path = "preprocess/incbin.rs"]
mod incbin;

use std::path::Path;
use std::process::{Command, Output};

fn invoke(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn success(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn raw_c_preprocess_compile_and_preprocessed_input() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess");
    std::fs::create_dir_all(dir.join("headers")).unwrap();
    std::fs::write(dir.join("headers/value.h"), "#define VALUE (BASE + 2)\n").unwrap();
    std::fs::write(dir.join("main.c"), "#include <value.h>\n#ifdef OMIT\n#error should be undefined\n#endif\nint main(void) { return VALUE; }\n").unwrap();
    let preprocessed = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-Iheaders",
            "-DBASE=40",
            "-DOMIT",
            "-UOMIT",
            "main.c",
        ],
    ));
    assert!(preprocessed.contains("40 + 2"), "{preprocessed}");
    let ir = success(invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "-I",
            "headers",
            "-D",
            "BASE=40",
            "-U",
            "OMIT",
            "main.c",
        ],
    ));
    assert!(ir.contains("@main"));
    std::fs::write(dir.join("main.i"), preprocessed).unwrap();
    let from_i = success(invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "--sysroot",
            "does-not-exist-sdk",
            "main.i",
        ],
    ));
    assert_eq!(from_i, ir);
    let conflict = invoke(&dir, &["cc", "-E", "--emit-ir", "main.c"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used together"));
    let conflict = invoke(&dir, &["cc", "-E", "--emit-object", "main.c"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used together"));
}

#[test]
fn default_environment_uses_sysroot_and_embedded_headers_without_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-sysroot");
    std::fs::create_dir_all(dir.join("sdk/usr/include")).unwrap();
    std::fs::create_dir_all(dir.join("system")).unwrap();
    std::fs::write(dir.join("sdk/usr/include/sdk.h"), "#define SDK_VALUE 40\n").unwrap();
    std::fs::write(dir.join("system/extra.h"), "#define EXTRA_VALUE 2\n").unwrap();
    std::fs::write(dir.join("main.c"), "#include <sdk.h>\n#include <extra.h>\n#include <stddef.h>\n#include <stdint.h>\n#ifndef __KEEL__\n#error expected native environment\n#endif\nint main(void) { return SDK_VALUE + EXTRA_VALUE + sizeof(uint64_t) - sizeof(size_t); }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .env("PATH", "")
        .args([
            "cc",
            "--sysroot=sdk",
            "-isystem",
            "system",
            "--emit-ir",
            "main.c",
        ])
        .output()
        .unwrap();
    let ir = success(out);
    assert!(ir.contains("@main"));
    success(invoke(
        &dir,
        &[
            "cc",
            "-isysroot",
            "sdk",
            "-isystem",
            "system",
            "--emit-ir",
            "main.c",
        ],
    ));
    for args in [
        vec!["cc", "--sysroot=", "main.c"],
        vec!["cc", "--sysroot", "missing-directory", "main.c"],
    ] {
        assert!(!invoke(&dir, &args).status.success());
    }
    for flag in ["--clang", "--no-clang"] {
        let out = invoke(&dir, &["cc", flag, "main.c"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unknown flag"));
    }
}

#[test]
fn macro_dump_reports_target_abi_and_compatibility_identity() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-macros");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("empty.c"), "#define USER 42\n").unwrap();
    let out = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-dM",
            "--target",
            "x86_64-unknown-linux-gnu",
            "empty.c",
        ],
    ));
    assert!(out.contains("#define __KEEL__ 1"));
    assert!(out.contains("#define __SIZEOF_LONG_DOUBLE__ 16"));
    assert!(out.contains("#define __INT64_TYPE__ long int"));
    assert!(out.contains("#define USER 42"));
    let out = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-dM",
            "--target",
            "aarch64-apple-darwin",
            "-mmacosx-version-min=14.2",
            "empty.c",
        ],
    ));
    assert!(out.contains("#define __ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__ 140200"));
    assert!(out.contains("#define __INT64_TYPE__ long long int"));
}

#[test]
fn preprocessing_releases_mapped_input_before_in_place_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess-in-place");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#define ANSWER 42\nint main(void) { return ANSWER; }\n",
    )
    .unwrap();
    success(invoke(&dir, &["cc", "-E", "main.c", "-o", "main.c"]));
    assert!(
        std::fs::read_to_string(dir.join("main.c"))
            .unwrap()
            .contains("return 42")
    );
    std::fs::rename(dir.join("main.c"), dir.join("main.i")).unwrap();
    assert!(success(invoke(&dir, &["cc", "--emit-ir", "main.i"])).contains("@main"));
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn raw_c_preprocessing_composes_with_native_object_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess-object");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#define ANSWER 42\nint main(void) { return ANSWER; }\n",
    )
    .unwrap();
    success(invoke(
        &dir,
        &["cc", "--emit-object", "main.c", "-o", "main.o"],
    ));
    assert_eq!(
        &std::fs::read(dir.join("main.o")).unwrap()[..4],
        &[0xcf, 0xfa, 0xed, 0xfe]
    );
}

#[test]
fn linux_object_from_preprocessed_input_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-linux-native-no-tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.i"), "int main(void) { return 42; }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .env("PATH", &dir)
        .args([
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-object",
            "main.i",
            "-o",
            "main.o",
        ])
        .output()
        .unwrap();
    success(output);
    assert_eq!(&std::fs::read(dir.join("main.o")).unwrap()[..4], b"\x7fELF");
}

#[test]
fn standalone_linux_assembler_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-linux-asm-no-tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("answer.s"),
        ".intel_syntax noprefix\n.globl answer\nanswer: mov eax,42; ret\n",
    )
    .unwrap();
    success(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .env("PATH", &dir)
            .args([
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "answer.s",
                "--out-dir",
                "objects",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(
        &std::fs::read(dir.join("objects/answer.o")).unwrap()[..4],
        b"\x7fELF"
    );
}
