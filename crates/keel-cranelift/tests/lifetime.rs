//! CLIF optimizers must see fixed-stack lifetime boundaries.
use keel_cranelift::{OptLevel, Options, TargetProfile};

#[test]
fn fixed_stack_lifetime_epochs_translate_to_sequence_points() {
    let source = "func @f() -> i64 {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    intrinsic lifetime_start(v0)
    v1 = iconst.i64 1
    store v1, v0
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v0)
    v2 = iconst.i64 2
    store v2, v0
    v3 = load.i64 v0
    intrinsic lifetime_end(v0)
    return v3
}";
    let module = keel_ir::parse(source).unwrap();
    for opt in [OptLevel::None, OptLevel::Speed] {
        let options = Options {
            target: TargetProfile::AppleArm64,
            opt,
            verify_clif: true,
        };
        let clif = keel_cranelift::translate_to_clif(&module, &options).unwrap();
        assert_eq!(clif.matches("sequence_point").count(), 4, "{clif}");
        assert!(
            !keel_cranelift::compile_object(&module, &options)
                .unwrap()
                .is_empty()
        );
    }
}
