//! Target-specific locked RMW semantics must not silently become generic IR.
#[test]
fn cx16_is_rejected_by_foreign_exporter() {
    let module = keel_ir::parse("func @cas(ptr,ptr,ptr,ptr) {\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.i128 v1\n v5 = load.i128 v2\n v6 = intrinsic x86_cx16_cmpxchg16b(v0,v4,v5)\n store v6,v3\n return\n}").unwrap();
    keel_ir::verify_module(&module).unwrap();
    let error = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap_err();
    assert!(
        error.contains("unsupported intrinsic X86Cx16Cmpxchg16b"),
        "{error}"
    );
}
