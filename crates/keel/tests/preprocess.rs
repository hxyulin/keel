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
            "--no-clang",
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
            "--no-clang",
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
            "--clang",
            "does-not-exist-clang",
            "main.i",
        ],
    ));
    assert_eq!(from_i, ir);
    let conflict = invoke(&dir, &["cc", "-E", "--emit-ir", "main.c"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used together"));
}
