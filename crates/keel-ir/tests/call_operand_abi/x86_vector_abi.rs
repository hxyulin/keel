use keel_ir::{CallConv, Signature, Type, X86VectorAbi};
fn valid(src: &str) -> keel_ir::Module {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
#[test]
fn signatures_roundtrip_and_keep_indirect_policies_distinct() {
    let src = "decl @g(f64x1)->f64x1 x86_vector_abi(gnu)\ndecl @c(f64x1)->f64x1 x86_vector_abi(clang)\nfunc @adapter(ptr,ptr,f64x1)->f64x1 {\nblock0(v0:ptr,v1:ptr,v2:f64x1):\n v3 = call_indirect v0(v2) : (f64x1)->f64x1 x86_vector_abi(gnu)\n v4 = call_indirect v1(v3) : (f64x1)->f64x1 x86_vector_abi(clang)\n return v4\n}";
    let m = valid(src);
    assert_eq!(m.functions[0].signatures().count(), 2);
    let text = m.to_string();
    let n = valid(&text);
    assert_eq!(text, n.to_string());
    assert_eq!(n.decls[0].sig.x86_vector_abi, Some(X86VectorAbi::Gnu));
    assert_eq!(n.decls[1].sig.x86_vector_abi, Some(X86VectorAbi::Clang));
    let mut sig = Signature::new(&[Type::I64], &[Type::I64]);
    assert_eq!(sig.x86_vector_abi, None);
    assert!(!sig.to_string().contains("x86_vector_abi"));
    let plain = sig.clone();
    sig.x86_vector_abi = Some(X86VectorAbi::Gnu);
    assert_ne!(sig, plain);
    sig.x86_vector_abi = Some(X86VectorAbi::Clang);
    assert_ne!(sig, plain);
}
#[test]
fn invalid_policy_and_fast_contracts_fail() {
    for attr in [
        "x86_vector_abi(ms)",
        "x86_vector_abi gnu",
        "x86_vector_abi(gnu) x86_vector_abi(clang)",
    ] {
        assert!(keel_ir::parse(&format!("decl @f(i64)->i64 {attr}\n")).is_err());
    }
    let m = keel_ir::parse(
        "func @f(i64)->i64 cc(fast) x86_vector_abi(gnu) local {\nblock0(v0:i64):\n return v0\n}",
    )
    .unwrap();
    let err = keel_ir::verify_module(&m).unwrap_err();
    assert!(
        err.iter()
            .any(|e| e.message.contains("x86_vector_abi requires cc(c)"))
    );
    let mut sig = Signature::new(&[], &[]);
    sig.cc = CallConv::C;
    sig.x86_vector_abi = Some(X86VectorAbi::Gnu);
    assert!(sig.to_string().contains("cc(c) x86_vector_abi(gnu)"));
}
#[test]
fn explicit_tail_policies_defer_physical_compatibility_and_clones_keep_the_policy() {
    for (caller, callee, ok) in [
        ("gnu", "gnu", true),
        ("clang", "clang", true),
        ("gnu", "clang", true),
        ("clang", "gnu", true),
    ] {
        let src = format!(
            "decl @g(f64x1)->f64x1 x86_vector_abi({callee})\nfunc @f(f64x1)->f64x1 x86_vector_abi({caller}) {{\nblock0(v0:f64x1):\n return_call @g(v0)\n}}"
        );
        let m = keel_ir::parse(&src).unwrap();
        assert_eq!(keel_ir::verify_module(&m).is_ok(), ok);
        assert_eq!(
            m.functions[0].sig.clone().x86_vector_abi,
            m.functions[0].sig.x86_vector_abi
        );
    }
}
