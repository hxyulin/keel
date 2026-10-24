use keel_c::{Options, PreprocessOptions, compile, predefined_macros, preprocess};
use keel_ir::Target;

#[test]
fn native_c_entry_points_keep_their_profiles() {
    for target in Target::NATIVE {
        let macros = predefined_macros(target, None).unwrap();
        assert!(macros.contains("#define __LP64__ 1"));
        let text = preprocess(
            "int answer(void) { return 42; }",
            "control.c",
            &PreprocessOptions {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        let module = compile(
            &text,
            "control.c",
            Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}
