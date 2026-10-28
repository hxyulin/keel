//! Characterization tests for the Python LLVM importer contract.
use std::{path::PathBuf, process::Command};
const HEADER: &str = "target triple = \"arm64-apple-macosx11.0.0\"\ntarget datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128-Fn32\"\n";
fn importer() -> Option<keel_llvm::import::Importer> {
    let path = std::env::var_os("KEEL_TEST_LIBLLVM")?;
    // SAFETY: the test environment supplies the trusted LLVM 23 runtime.
    Some(
        unsafe { keel_llvm::import::Importer::load(&PathBuf::from(path)) }
            .expect("load trusted test LLVM"),
    )
}
#[test]
fn scalar_flags_and_boolean_refinement() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let src =
        format!("{HEADER}define i64 @f(i64 %a, i64 %b) {{ %r = add nsw i64 %a, %b\nret i64 %r }}");
    let text = i.translate(src.as_bytes(), Default::default()).unwrap();
    assert!(text.contains("iadd"));
    assert!(!text.contains("nsw"));
    for body in [
        "define i64 @f() { ret i64 poison }",
        "define i64 @f() { %r = freeze i64 poison\nret i64 %r }",
        "define void @f(i64 %n) { %p = alloca i64, i64 %n\nret void }",
    ] {
        assert!(
            i.translate(format!("{HEADER}{body}").as_bytes(), Default::default())
                .is_err(),
            "{body}"
        );
    }
}
#[test]
fn fixture_output_matches_python_reference() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = [
        "crates/keel-cranelift/tools/ir-fixtures/scalar.ll",
        "suite/llvm_import_linkage_fixtures/available.ll",
    ];
    for path in paths {
        let source = std::fs::read(root.join(path)).unwrap();
        for localize in [false, true] {
            let script = "import sys;sys.path.insert(0,sys.argv[1]);from llvm_import import LLVM,import_module;print(import_module(LLVM(sys.argv[2]),sys.stdin.buffer.read(),localize_odr=sys.argv[3]=='true'),end='')";
            let mut child = Command::new("python3")
                .args(["-c", script])
                .arg(root.join("suite"))
                .arg(std::env::var_os("KEEL_TEST_LIBLLVM").unwrap())
                .arg(localize.to_string())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(&source).unwrap();
            }
            let python = child.wait_with_output().unwrap();
            let rust = i.translate(
                &source,
                keel_llvm::import::Options {
                    localize_odr_functions: localize,
                },
            );
            assert_eq!(
                rust.is_ok(),
                python.status.success(),
                "{path} localize={localize}: Rust {rust:?}; Python {}",
                String::from_utf8_lossy(&python.stderr)
            );
            if let Ok(text) = rust {
                let a = keel_ir::parse(&text).unwrap().to_string();
                let b = keel_ir::parse(&String::from_utf8(python.stdout).unwrap())
                    .unwrap()
                    .to_string();
                assert_eq!(a, b, "{path}");
            }
        }
    }
}
