//! Explicit producer metadata must not disappear when unused symbols are filtered.
#[test]
fn rejects_unused_producer_abi_metadata() {
    for producer in ["gnu", "clang"] {
        for body in [
            "",
            "func @plain(i64)->i64 {\nblock0(v0:i64):\n return v0\n}\n",
        ] {
            let text = format!("decl @tagged(i64)->i64 x86_vector_abi({producer})\n{body}");
            let module = keel_ir::parse(&text).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let error =
                keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap_err();
            assert!(error.contains("explicit x86 vector ABI"), "{error}");
        }
        // An unused indirect signature is also part of the module's policy.
        let mut module =
            keel_ir::parse("func @plain(i64)->i64 {\nblock0(v0:i64):\n return v0\n}\n").unwrap();
        let mut signature = keel_ir::Signature::new(&[], &[]);
        signature.x86_vector_abi = Some(if producer == "gnu" {
            keel_ir::X86VectorAbi::Gnu
        } else {
            keel_ir::X86VectorAbi::Clang
        });
        module.functions[0].import_signature(signature);
        keel_ir::verify_module(&module).unwrap();
        let error = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap_err();
        assert!(error.contains("explicit x86 vector ABI"), "{error}");
    }
}
