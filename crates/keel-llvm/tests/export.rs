use keel_llvm::{TargetProfile, export};
use std::process::Command;
fn translate(src: &str) -> Result<String, String> {
    export(&keel_ir::parse(src).unwrap(), TargetProfile::AppleArm64)
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
