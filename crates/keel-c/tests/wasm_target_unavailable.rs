use keel_c::{Options, PreprocessOptions, compile, predefined_macros, preprocess};
use keel_ir::Target;

fn unavailable(error: impl std::fmt::Display) {
    let error = error.to_string();
    assert!(
        error.contains("unsupported target wasm32-unknown-unknown"),
        "{error}"
    );
    assert!(error.contains("unavailable"), "{error}");
}

#[test]
fn public_wasm_c_entry_points_reject_before_source_processing() {
    for source in [
        "",
        "int answer(void) { return 42; }",
        "this is invalid C",
        "\"unterminated",
    ] {
        for debug_info in [false, true] {
            let error = compile(
                source,
                "input.i",
                Options {
                    target: Target::Wasm32UnknownUnknown,
                    debug_info,
                },
            )
            .unwrap_err();
            assert_eq!((&*error.file, error.line, error.col), ("input.i", 1, 1));
            unavailable(error);
        }
    }
    for dump_macros in [false, true] {
        let options = PreprocessOptions {
            target: Target::Wasm32UnknownUnknown,
            dump_macros,
            sysroot: Some("/nonexistent-wasm-sysroot".into()),
            include_dirs: vec!["/nonexistent-wasm-include".into()],
            ..Default::default()
        };
        unavailable(
            preprocess(
                "#include <immintrin.h>\n#error should not be read",
                "input.c",
                &options,
            )
            .unwrap_err(),
        );
        #[cfg(feature = "profile")]
        unavailable(
            keel_c::preprocess_with_stats("#error should not be read", "input.c", &options)
                .unwrap_err(),
        );
    }
    for deployment in [None, Some("13.0")] {
        unavailable(predefined_macros(Target::Wasm32UnknownUnknown, deployment).unwrap_err());
    }
    #[cfg(feature = "profile")]
    unavailable(
        keel_c::profile::compile(
            "\"unterminated",
            "input.c",
            Options {
                target: Target::Wasm32UnknownUnknown,
                ..Default::default()
            },
        )
        .unwrap_err(),
    );
}

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
