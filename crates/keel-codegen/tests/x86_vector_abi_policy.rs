#[test]
fn native_x86_accepts_policy_and_arm_keeps_explicit_rejection() {
    for policy in ["gnu", "clang"] {
        for src in [
            format!("decl @g(i64)->i64 x86_vector_abi({policy})\n"),
            format!(
                "func @f(i64)->i64 x86_vector_abi({policy}) {{\nblock0(v0:i64):\n return v0\n}}"
            ),
            format!(
                "func @f(ptr,i64)->i64 {{\nblock0(v0:ptr,v1:i64):\n v2 = call_indirect v0(v1) : (i64)->i64 x86_vector_abi({policy})\n return v2\n}}"
            ),
        ] {
            let m = keel_ir::parse(&src).unwrap();
            keel_ir::verify_module(&m).unwrap();
            keel_codegen::x86_64::compile(&m).unwrap();
            let err = keel_codegen::aarch64::compile(&m).unwrap_err();
            assert!(err.message.contains("explicit x86 vector ABI transport"));
        }
    }
}
#[test]
fn optimization_keeps_policy_and_untagged_output_is_unchanged() {
    let src = "func @plain(i64)->i64 {\nblock0(v0:i64):\n return v0\n}\n";
    let m = keel_ir::parse(src).unwrap();
    let mut explicit_none = m.clone();
    explicit_none.functions[0].sig.x86_vector_abi = None;
    assert_eq!(
        keel_codegen::x86_64::compile(&m).unwrap(),
        keel_codegen::x86_64::compile(&explicit_none).unwrap()
    );
    for policy in ["gnu", "clang"] {
        let src = format!(
            "func @f(i64)->i64 x86_vector_abi({policy}) local {{\nblock0(v0:i64):\n return v0\n}}"
        );
        let mut m = keel_ir::parse(&src).unwrap();
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(m.functions[0].sig.cc, keel_ir::CallConv::C);
        assert!(m.functions[0].sig.x86_vector_abi.is_some());
    }
}
