use keel_cranelift::{OptLevel, Options, TargetProfile};

#[test]
fn owned_flags_regions_explicitly_reject_both_output_routes() {
    let options = Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    };
    for source in [
        "func @owned(i64)->i64 {\nentry(v0:i64):\n v1,v2 = x86_rflags_region terminal \"save; swap 0 0; read 1; reapply; normal\"(v0)\n return v2\n}\n",
        "func @empty() {\nentry:\n x86_rflags_region terminal \"save; reapply; normal\"()\n return\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for error in [
            keel_cranelift::translate_to_clif(&module, &options).unwrap_err(),
            keel_cranelift::compile_object(&module, &options).unwrap_err(),
        ] {
            assert!(error.contains("x86 FLAGS owned region"), "{error}");
        }
    }
}
