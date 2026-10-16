use keel_llvm::{TargetProfile, export};
use std::process::Command;
fn translate(src: &str) -> Result<String, String> {
    export(&keel_ir::parse(src).unwrap(), TargetProfile::AppleArm64)
}
#[test]
fn definition_restrict_facts_are_conservatively_omitted() {
    let ll = translate("func @read(ptr) -> i32 restrict_params(0) {\nblock0(v0: ptr):\n v1 = load.i32 v0\n return v1\n}\n").unwrap();
    assert!(ll.contains("define i32"));
    assert!(!ll.contains("noalias"));
    assert!(!ll.contains("restrict_params"));
}
#[test]
fn cfg_and_semantic_shapes() {
    let ll = translate(include_str!("fixtures/core.keel")).unwrap();
    assert!(ll.contains("phi i64 [ %v1, %edge3_0 ]"));
    assert!(ll.contains("and i64 %v1, 63"));
    assert!(ll.contains("and i8 %t2.amount, 7"));
    assert!(ll.contains("fcmp une double"));
    assert!(ll.contains("zext i1"));
    assert!(!ll.contains("load volatile")); // fixture has volatile store
    assert!(ll.contains("store volatile i64"));
    assert!(ll.contains("llvm.fptosi.sat.i32.f64"));
    assert!(ll.contains("llvm.fptoui.sat.i64.f32"));
    assert!(!ll.contains("inbounds"));
    assert!(!ll.contains("nsw"));
    assert!(!ll.contains("fast "));
}
#[test]
fn reject_division_with_context() {
    for op in ["sdiv", "udiv", "srem", "urem"] {
        let src = format!(
            "func @bad(i64, i64) -> i64 {{\nblock0(v0: i64, v1: i64):\n v2 = {op} v0, v1\n return v2\n}}\n"
        );
        let error = translate(&src).unwrap_err();
        assert!(error.contains("function @bad: block0, inst0"), "{error}");
        assert!(error.contains("unsupported binary operation"));
    }
}
#[test]
fn reject_aggregate_signature() {
    let m = keel_ir::Module {
        decls: vec![keel_ir::FuncDecl {
            name: "aggregate".into(),
            attrs: Default::default(),
            sig: keel_ir::Signature {
                params: vec![keel_ir::AbiParam {
                    ty: keel_ir::Type::Ptr,
                    ext: keel_ir::ArgExt::None,
                    purpose: keel_ir::Purpose::ByVal(keel_ir::Layout::Scalar(keel_ir::Type::I64)),
                }],
                ..keel_ir::Signature::new(&[], &[])
            },
        }],
        ..Default::default()
    };
    assert!(
        export(&m, TargetProfile::AppleArm64)
            .unwrap_err()
            .contains("aggregate byval/sret")
    );
}
#[test]
fn reject_debug_atomic_and_indirect() {
    let debug = "file f0 = \"x.c\"\n";
    assert!(translate(debug).unwrap_err().contains("debug information"));
    let atomic = "func @bad(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = atomic_load.i64 seq_cst v0\n return v1\n}\n";
    assert!(
        translate(atomic)
            .unwrap_err()
            .contains("unsupported instruction")
    );
    let indirect = "func @bad(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = call_indirect v0() : () -> i64\n return v1\n}\n";
    assert!(
        translate(indirect)
            .unwrap_err()
            .contains("unsupported instruction")
    );
}
#[test]
fn failed_export_does_not_publish_output() {
    let dir = std::env::temp_dir().join(format!("keel-llvm-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("bad.keel");
    let output = dir.join("out.ll");
    std::fs::write(&input,"func @bad(i64, i64) -> i64 {\nblock0(v0: i64, v1: i64):\n v2 = sdiv v0, v1\n return v2\n}\n").unwrap();
    std::fs::write(&output, "sentinel").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_keel-llvm"))
        .args(["--target", "arm64-apple-macosx11.0.0"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "sentinel");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordinary_functions_and_calls_are_not_builtins() {
    let ll = translate(include_str!("fixtures/libcall.keel")).unwrap();
    for line in ll.lines().filter(|line| {
        line.starts_with("declare ") || line.starts_with("define ") || line.contains(" = call ")
    }) {
        assert!(line.contains("nobuiltin"), "{line}");
    }
    let core = translate(include_str!("fixtures/core.keel")).unwrap();
    for line in core.lines().filter(|line| line.contains("@llvm.")) {
        assert!(
            !line.contains("nobuiltin"),
            "explicit intrinsic disabled: {line}"
        );
    }
}

#[test]
fn extended_fixtures_export_completely() {
    for source in [
        include_str!("fixtures/extended.keel"),
        include_str!("fixtures/wide.keel"),
    ] {
        let ll = translate(source).unwrap();
        assert!(ll.contains("define "));
        assert!(!ll.contains(" fast "));
    }
}

#[test]
fn rejected_feature_matrix() {
    for (source, expected) in [
        ("decl @bad(i128)", "i128 ABI"),
        ("decl @bad() -> (i64, i64)", "multiple-result"),
        ("decl @bad(i32, ...)", "variadic"),
        ("decl @bad(ptr) -> i32 cc(c) returns_twice", "returns_twice"),
        ("data @tls = thread_local align 8 { i64 0 }", "thread-local"),
        ("decl @bad(ptr sret {i64})", "aggregate byval/sret"),
        (
            "func @bad(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n return v0\n}",
            "vector types",
        ),
        (
            "func @bad() {\nblock0:\n asm \"nop\", \"\"()\n return\n}",
            "unsupported instruction",
        ),
        (
            "decl @personality()\nfunc @bad() cc(c) personality @personality {\nblock0:\n return\n}",
            "exception handling/debug",
        ),
        ("decl @llvm.reserved()", "intrinsic namespace"),
        ("func @bad() {\nblock0:\n jump block0\n}", "entry"),
    ] {
        let error = translate(source).unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}

#[test]
fn symbols_are_quoted_by_bytes_and_duplicates_rejected() {
    let mut m = keel_ir::parse("func @placeholder() {\nblock0:\n return\n}\n").unwrap();
    m.functions[0].name = "odd\"\\é".into();
    let ll = export(&m, TargetProfile::AppleArm64).unwrap();
    assert!(ll.contains("@\"\\6F\\64\\64\\22\\5C\\C3\\A9\""), "{ll}");
    m.functions.push(m.functions[0].clone());
    assert!(
        export(&m, TargetProfile::AppleArm64)
            .unwrap_err()
            .contains("more than once")
    );
}

#[test]
fn cli_whole_module_buffering_and_errors() {
    let dir = std::env::temp_dir().join(format!("keel-llvm-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let valid = dir.join("valid.keel");
    let late = dir.join("late.keel");
    let malformed = dir.join("parse.keel");
    std::fs::write(&valid, "func @ok() {\nblock0:\n return\n}\n").unwrap();
    let unsupported = "func @late(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = atomic_load.i64 seq_cst v0\n return v1\n}\n";
    std::fs::write(
        &late,
        format!(
            "{}\n{unsupported}",
            std::fs::read_to_string(&valid).unwrap()
        ),
    )
    .unwrap();
    std::fs::write(&malformed, "invalid syntax").unwrap();
    let exe = env!("CARGO_BIN_EXE_keel-llvm");
    for args in [
        vec![valid.to_str().unwrap()],
        vec!["--target", "wrong", valid.to_str().unwrap()],
        vec!["--target"],
        vec![
            "--target",
            "arm64-apple-macosx11.0.0",
            late.to_str().unwrap(),
        ],
        vec![
            "--target",
            "arm64-apple-macosx11.0.0",
            malformed.to_str().unwrap(),
        ],
        vec![
            "--target",
            "arm64-apple-macosx11.0.0",
            "/nonexistent/keel-llvm-input",
        ],
    ] {
        let result = Command::new(exe).args(args).output().unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
    let success = Command::new(exe)
        .args(["--target", "arm64-apple-macosx11.0.0"])
        .arg(&valid)
        .output()
        .unwrap();
    assert!(success.status.success());
    assert!(
        String::from_utf8(success.stdout)
            .unwrap()
            .contains("define ")
    );
    let sentinel = dir.join("sentinel.ll");
    std::fs::write(&sentinel, "sentinel").unwrap();
    let result = Command::new(exe)
        .args(["--target", "arm64-apple-macosx11.0.0"])
        .arg(&late)
        .arg("-o")
        .arg(&sentinel)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel");
    // A nonempty directory is an invalid output file. The temporary file must
    // be removed, while its existing contents survive the failed rename.
    let child = Command::new(exe)
        .args(["--target", "arm64-apple-macosx11.0.0"])
        .arg(&valid)
        .arg("-o")
        .arg(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let child_pid = child.id();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "sentinel");
    assert!(
        !dir.parent()
            .unwrap()
            .join(format!(".keel-llvm-{child_pid}.tmp"))
            .exists()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
