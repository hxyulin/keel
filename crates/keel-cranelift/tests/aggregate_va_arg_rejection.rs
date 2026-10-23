use keel_cranelift::{OptLevel, Options, TargetProfile};
#[test]
fn explicit_operand_layout_and_aggregate_homes_reject_before_translation() {
    let options = Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    };
    for source in [
        "decl @sink(i64,...)\nfunc @f(ptr){\nentry(v0:ptr):\n v1=iconst.i64 7\n call @sink(v1,v0 : ptr byval {i64,f64})\n return\n}\n",
        "func @f(ptr)->ptr{\n ss0=stack_slot 16, align 8\nentry(v0:ptr):\n v1=va_arg_aggregate {i64,f64},v0,ss0\n return v1\n}\n",
    ] {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let error = keel_cranelift::translate_to_clif(&m, &options).unwrap_err();
        assert!(error.contains("aggregate va_arg homes are unsupported"));
    }
}
