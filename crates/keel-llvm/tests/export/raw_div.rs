#[test]
fn architectural_division_is_explicitly_unsupported() {
    for sign in ["u", "s"] {
        for w in [8, 16, 32, 64] {
            let src = format!(
                "func @raw(i{w},i{w},i{w})->i{w} {{\nentry(v0:i{w},v1:i{w},v2:i{w}):\n v3,v4 = intrinsic x86_div_{sign}{w}(v0,v1,v2)\n return v3\n}}\n"
            );
            let m = keel_ir::parse(&src).unwrap();
            keel_ir::verify_module(&m).unwrap();
            let error = keel_llvm::export(&m, keel_llvm::TargetProfile::AppleArm64).unwrap_err();
            assert!(error.contains("unsupported intrinsic"), "{error}");
        }
    }
}
