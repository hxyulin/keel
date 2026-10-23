use keel_cranelift::{OptLevel, Options, TargetProfile};

#[test]
fn unsupported_odr_definitions_fail_all_output_routes() {
    let options = Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    };
    for src in [
        "decl @optional() weak\nfunc @f() {\nblock0:\n return\n}\n",
        "func @f() weak {\nblock0:\n return\n}\n",
        "data @d = weak align 8 { i64 1 }\n",
        "decl data weak @d\n",
        "func @f() weak_odr {\nblock0:\n return\n}\n",
        "data @d = weak_odr align 8 { i64 1 }\n",
        "data @d = weak_odr hidden align 8 { i64 1 }\n",
        "decl data @name\ndata @d = align 8 { rtti_name @name }\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            keel_cranelift::compile_object(&m, &options)
                .unwrap_err()
                .contains("weak_odr")
        );
        assert!(
            keel_cranelift::compile_verified(&m, &options)
                .unwrap_err()
                .contains("weak_odr")
        );
        assert!(
            keel_cranelift::translate_to_clif(&m, &options)
                .unwrap_err()
                .contains("weak_odr")
        );
        let support = keel_cranelift::function_support(&m);
        assert_eq!(support.len(), 1);
        assert!(support[0].1.as_ref().unwrap_err().contains("weak_odr"));
    }
}
